//! Cost and accuracy of sensing for the guidance perception design, measured
//! in generated match worlds. Physics queries are compared with circle
//! geometry: nominal circles for the sun and planets, and the world edge.
//!
//! ```sh
//! SPACEWARS_SENSING_COST_SEEDS=42,73 SPACEWARS_SENSING_COST_OUT=/tmp/sensing.json \
//!   cargo +1.89.0 test --locked --offline -p scenario-spacewars --profile release --lib \
//!   guidance_sensing_cost -- --ignored --nocapture
//! ```
use super::*;
use crate::physics::{MechanicalEntity, classify_entity, planet_entity};
use engine_common::{DirtMode, MaterialAsteroidSettings};
use engine_rapier::world::{
    BallCastOptions, CollisionGroups, PhysicsId, PhysicsWorld, PointProjectionOptions,
    RayCastOptions,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    f32::consts::{PI, TAU},
    hint::black_box,
    time::Instant,
};

/// Terrain, the sun and the world edge, but not ships or debris. Mirrors
/// GROUP_BODY and GROUP_WORLD in physics.rs; every hit is classified below.
const CELESTIAL: CollisionGroups = CollisionGroups::new(u32::MAX, (1 << 5) | (1 << 6));
const ARCS: usize = 16;
const ARC: f32 = TAU / ARCS as f32;
const SAMPLES: [usize; 3] = [1, 4, 8];
const RANGES: [f32; 2] = [250.0, 1000.0];
const BALL_RADIUS: f32 = 4.0;
const PLANET_ALTITUDES: [f32; 5] = [10.0, 35.0, 85.0, 120.0, 200.0];
const SUN_ALTITUDES: [f32; 3] = [20.0, 65.0, 150.0];
const BEARINGS: usize = 32;
const OPEN_ORIGINS: usize = 64;
const MIN_CLEARANCE: f32 = 2.0;
const DT: Duration = Duration::from_nanos(16_666_667);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Sun,
    Planet,
    Edge,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Sun => "sun",
            Self::Planet => "planet",
            Self::Edge => "edge",
        }
    }

    fn of(entity: PhysicsId) -> Self {
        match classify_entity(entity) {
            Some(MechanicalEntity::Body(BodyId::Sun)) => Self::Sun,
            Some(MechanicalEntity::Body(BodyId::Planet(_))) => Self::Planet,
            Some(MechanicalEntity::World) => Self::Edge,
            other => panic!("celestial query struck {other:?}"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Body {
    kind: Kind,
    entity: PhysicsId,
    center: Vec2,
    radius: f32,
}

impl Body {
    /// Distance to the surface; the world edge is measured from inside.
    fn clearance(&self, point: Vec2) -> f32 {
        let distance = point.distance_to(self.center);
        if self.kind == Kind::Edge {
            self.radius - distance
        } else {
            distance - self.radius
        }
    }

    /// Distance along a unit direction to the surface, if it is struck.
    fn ray(&self, origin: Vec2, direction: Vec2) -> Option<f32> {
        let to = self.center - origin;
        let along = to.dot(direction);
        let discriminant = along * along - (to.length_squared() - self.radius * self.radius);
        if discriminant < 0.0 {
            return None;
        }
        let root = discriminant.sqrt();
        let distance = if self.kind == Kind::Edge {
            along + root
        } else {
            along - root
        };
        (distance >= 0.0).then_some(distance)
    }

    /// Bearing of the nearest surface point, the angular half-width covered
    /// within `range`, and the full half-width the body subtends.
    fn cover(&self, origin: Vec2, range: f32) -> (f32, f32, f32) {
        if self.kind == Kind::Edge {
            let outward = origin - self.center;
            let m = outward.length();
            let reach = (self.radius * self.radius - m * m - range * range) / (2.0 * m * range);
            (bearing(outward), reach.clamp(-1.0, 1.0).acos(), PI)
        } else {
            let to = self.center - origin;
            let d = to.length();
            let full = (self.radius / d).min(1.0).asin();
            let within = if d - self.radius > range {
                0.0
            } else if d * d - self.radius * self.radius <= range * range {
                full
            } else {
                let reach = (d * d + range * range - self.radius * self.radius) / (2.0 * d * range);
                reach.clamp(-1.0, 1.0).acos().min(full)
            };
            (bearing(to), within, full)
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Arc {
    nearest: Option<(f32, Kind)>,
    fill: f32,
}

fn wrap(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

/// Counterclockwise bearing, matching `unit`. `Vec2::angle_radians` keeps an
/// older clockwise convention.
fn bearing(vector: Vec2) -> f32 {
    vector.y.atan2(vector.x)
}

fn unit(angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(cos, sin)
}

fn closer(nearest: &mut Option<(f32, Kind)>, distance: f32, kind: Kind) {
    if nearest.is_none_or(|(best, _)| distance < best) {
        *nearest = Some((distance, kind));
    }
}

/// Exact nearest surface and covered fraction in each arc, from circles.
fn circle_ring(bodies: &[Body], origin: Vec2, range: f32) -> [Arc; ARCS] {
    let mut covers = [(0.0, 0.0, 0.0); 16];
    for (cover, body) in covers.iter_mut().zip(bodies) {
        *cover = body.cover(origin, range);
    }
    std::array::from_fn(|arc| {
        let center = (arc as f32 + 0.5) * ARC;
        let mut nearest = None;
        let mut spans = [(0.0_f32, 0.0_f32); 48];
        let mut count = 0;
        for (body, &(bearing, within, full)) in bodies.iter().zip(&covers) {
            let offset = wrap(bearing - center);
            // Nearest is the body's own nearest point when it lies in the arc,
            // otherwise the arc boundary closest to it.
            let distance = if offset.abs() <= ARC / 2.0 {
                Some(body.clearance(origin))
            } else if offset.abs() - ARC / 2.0 <= full {
                let edge = center + (ARC / 2.0).copysign(offset);
                body.ray(origin, unit(edge))
            } else {
                None
            };
            if let Some(distance) = distance.filter(|d| *d <= range) {
                closer(&mut nearest, distance, body.kind);
            }
            for shift in [-TAU, 0.0, TAU] {
                let low = (offset + shift - within).max(-ARC / 2.0);
                let high = (offset + shift + within).min(ARC / 2.0);
                if high > low {
                    spans[count] = (low, high);
                    count += 1;
                }
            }
        }
        let spans = &mut spans[..count];
        spans.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let (mut covered, mut end) = (0.0, -ARC);
        for &(low, high) in spans.iter() {
            covered += (high - low.max(end)).max(0.0);
            end = end.max(high);
        }
        Arc {
            nearest,
            fill: covered / ARC,
        }
    })
}

/// Ray directions for `samples` rays per arc, spread evenly inside each arc.
fn directions(samples: usize) -> Vec<Vec2> {
    (0..ARCS * samples)
        .map(|ray| unit((ray as f32 + 0.5) * ARC / samples as f32))
        .collect()
}

fn sampled(hits: impl Iterator<Item = Option<(f32, Kind)>>, samples: usize) -> [Arc; ARCS] {
    let mut ring = [Arc::default(); ARCS];
    for (ray, hit) in hits.enumerate() {
        if let Some((distance, kind)) = hit {
            let arc = &mut ring[ray / samples];
            closer(&mut arc.nearest, distance, kind);
            arc.fill += 1.0 / samples as f32;
        }
    }
    ring
}

fn circle_hit(bodies: &[Body], origin: Vec2, direction: Vec2, range: f32) -> Option<(f32, Kind)> {
    let mut nearest = None;
    for body in bodies {
        if let Some(distance) = body.ray(origin, direction).filter(|d| *d <= range) {
            closer(&mut nearest, distance, body.kind);
        }
    }
    nearest
}

fn query_hit(
    world: &PhysicsWorld,
    origin: Vec2,
    direction: Vec2,
    range: f32,
) -> Option<(f32, Kind)> {
    world
        .cast_ray(
            origin,
            direction,
            RayCastOptions {
                max_distance: range,
                collision_groups: CELESTIAL,
                ..Default::default()
            },
        )
        .map(|hit| (hit.distance, Kind::of(hit.collider.entity)))
}

type Hits = [Option<(f32, Kind)>; ARCS * 8];

fn query_hits(world: &PhysicsWorld, origin: Vec2, rays: &[Vec2], range: f32) -> Hits {
    let mut hits = [None; ARCS * 8];
    for (hit, &direction) in hits.iter_mut().zip(rays) {
        *hit = query_hit(world, origin, direction, range);
    }
    hits
}

fn circle_hits(bodies: &[Body], origin: Vec2, rays: &[Vec2], range: f32) -> Hits {
    let mut hits = [None; ARCS * 8];
    for (hit, &direction) in hits.iter_mut().zip(rays) {
        *hit = circle_hit(bodies, origin, direction, range);
    }
    hits
}

fn ball_ring(world: &PhysicsWorld, origin: Vec2, centers: &[Vec2], range: f32) -> [Arc; ARCS] {
    sampled(
        centers.iter().map(|&direction| {
            world
                .cast_ball(
                    origin,
                    BALL_RADIUS,
                    direction,
                    BallCastOptions {
                        max_distance: range,
                        collision_groups: CELESTIAL,
                        ..Default::default()
                    },
                )
                .map(|hit| (hit.distance, Kind::of(hit.collider.entity)))
        }),
        1,
    )
}

/// Nearest point on each body from point projection, as body records need.
fn projected(world: &PhysicsWorld, bodies: &[Body], origin: Vec2) -> [Option<(Vec2, f32)>; 16] {
    let mut records = [None; 16];
    for (record, body) in records.iter_mut().zip(bodies) {
        *record = world
            .project_point(
                origin,
                PointProjectionOptions {
                    collision_groups: CELESTIAL,
                    entity: Some(body.entity),
                    ..Default::default()
                },
            )
            .map(|hit| (hit.point, hit.distance));
    }
    records
}

/// The same records from circles: nearest point and clearance.
fn circled(bodies: &[Body], origin: Vec2) -> [Option<(Vec2, f32)>; 16] {
    let mut records = [None; 16];
    for (record, body) in records.iter_mut().zip(bodies) {
        let clearance = body.clearance(origin);
        let outward = (origin - body.center).normalized();
        *record = Some((body.center + outward * body.radius, clearance));
    }
    records
}

/// One guidance training match world: a 600-second round, loose dirt in the
/// given shape and no asteroid stream, as in the engine client's harness.
fn match_world(seed: u64, shape: DirtMode) -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material_match(seed);
    state.set_match_time_limit(Some(Duration::from_secs(600)));
    state.set_asteroid_pressure(MaterialAsteroidSettings::default());
    state.enable_dirt_mode(shape);
    // New terrain enters the query index on the next step.
    for _ in 0..2 {
        SurfaceSortieScenario::step(&mut state, &[], DT);
    }
    state
}

fn bodies(state: &SurfaceSortieState) -> Vec<Body> {
    let world = &state.world.physics.world;
    let entity = |wanted| {
        world
            .collider_ids()
            .map(|id| id.entity)
            .find(|entity| classify_entity(*entity) == Some(wanted))
            .expect("celestial entity")
    };
    let center = |entity| {
        world
            .motion(PhysicsBodyId::new(entity, BodyRole::PRIMARY))
            .expect("celestial body")
            .position
    };
    let mut bodies = Vec::new();
    if let Some(sun) = state.world.sun {
        let entity = entity(MechanicalEntity::Body(BodyId::Sun));
        bodies.push(Body {
            kind: Kind::Sun,
            entity,
            center: center(entity),
            radius: sun.radius,
        });
    }
    for (index, planet) in state.world.planets.iter().enumerate() {
        assert!(state.planet_terrain(index).is_some(), "material planet");
        let entity = planet_entity(index);
        bodies.push(Body {
            kind: Kind::Planet,
            entity,
            center: center(entity),
            // Generated terrain fills the cells whose centers lie inside this.
            radius: planet.radius * BODY_BOUNDS_RADIUS_SCALE,
        });
    }
    let entity = entity(MechanicalEntity::World);
    let radius = state.world.config.universe_radius as f32;
    assert_eq!(center(entity), Vec2::splat(radius));
    bodies.push(Body {
        kind: Kind::Edge,
        entity,
        center: center(entity),
        radius,
    });
    assert!(bodies.len() <= 16);
    bodies
}

/// Query points around each planet and the sun at fixed altitudes, plus
/// points spread over open space, all clear of every surface.
fn origins(bodies: &[Body], seed: u64) -> Vec<(String, Vec2)> {
    let mut points = Vec::new();
    for (index, body) in bodies.iter().enumerate() {
        let altitudes: &[f32] = match body.kind {
            Kind::Planet => &PLANET_ALTITUDES,
            Kind::Sun => &SUN_ALTITUDES,
            Kind::Edge => &[],
        };
        for &altitude in altitudes {
            for bearing in 0..BEARINGS {
                let angle = (bearing as f32 + 0.37 * index as f32) * TAU / BEARINGS as f32;
                let point = body.center + unit(angle) * (body.radius + altitude);
                points.push((format!("{}-{altitude}", body.kind.name()), point));
            }
        }
    }
    let edge = bodies.last().unwrap();
    let mut state = seed | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 40) as f32 / (1 << 24) as f32
    };
    for _ in 0..OPEN_ORIGINS {
        let point = edge.center + unit(next() * TAU) * (edge.radius * next().sqrt());
        points.push(("open".to_owned(), point));
    }
    points.retain(|(_, point)| bodies.iter().all(|b| b.clearance(*point) >= MIN_CLEARANCE));
    points
}

#[derive(Default)]
struct Timing {
    microseconds: Vec<f64>,
    queries: u64,
}

fn summary(mut values: Vec<f64>) -> Value {
    if values.is_empty() {
        return json!({"count": 0});
    }
    values.sort_by(f64::total_cmp);
    let at = |q: f64| values[((values.len() - 1) as f64 * q).round() as usize];
    json!({"count": values.len(), "mean": values.iter().sum::<f64>() / values.len() as f64,
        "p50": at(0.5), "p95": at(0.95), "p99": at(0.99), "max": at(1.0)})
}

#[derive(Default)]
struct Report {
    timings: BTreeMap<String, Timing>,
    errors: BTreeMap<String, Vec<f64>>,
    counts: BTreeMap<String, u64>,
}

impl Report {
    /// Time one decision's work, also under `<name>/approach` for the
    /// approach altitudes.
    fn timed<T>(
        &mut self,
        name: String,
        approach: bool,
        queries: usize,
        work: impl FnOnce() -> T,
    ) -> T {
        let start = Instant::now();
        let output = black_box(work());
        let microseconds = start.elapsed().as_secs_f64() * 1e6;
        let names = [
            Some(name.clone()),
            approach.then(|| format!("{name}/approach")),
        ];
        for name in names.into_iter().flatten() {
            let timing = self.timings.entry(name).or_default();
            timing.microseconds.push(microseconds);
            timing.queries += queries as u64;
        }
        output
    }

    fn error(&mut self, name: String, value: f32) {
        self.errors.entry(name).or_default().push(f64::from(value));
    }

    fn count(&mut self, name: String) {
        *self.counts.entry(name).or_default() += 1;
    }

    /// Nearest-distance and fill differences between two rings.
    fn compare(&mut self, name: &str, ring: &[Arc; ARCS], exact: &[Arc; ARCS]) {
        for (arc, exact) in ring.iter().zip(exact) {
            match (arc.nearest, exact.nearest) {
                (Some((distance, _)), Some((reference, kind))) => {
                    self.error(format!("{name}/{}", kind.name()), distance - reference)
                }
                (None, Some((_, kind))) => self.count(format!("{name}/missed/{}", kind.name())),
                (Some((_, kind)), None) => self.count(format!("{name}/extra/{}", kind.name())),
                (None, None) => {}
            }
            self.error(format!("{name}/fill"), (arc.fill - exact.fill).abs());
            self.count(format!("{name}/arcs"));
        }
    }

    fn measure(
        &mut self,
        world: &PhysicsWorld,
        bodies: &[Body],
        rays: &[Vec<Vec2>],
        band: &str,
        origin: Vec2,
    ) {
        let approach = band == "planet-85" || band == "planet-120";
        for range in RANGES {
            let exact = self.timed(format!("circle_ring/r{range}"), approach, 0, || {
                circle_ring(bodies, origin, range)
            });
            for (samples, rays) in SAMPLES.into_iter().zip(rays) {
                let name = format!("ray_ring/k{samples}/r{range}");
                let (hits, ring) = self.timed(name, approach, rays.len(), || {
                    let hits = query_hits(world, origin, rays, range);
                    (hits, sampled(hits.into_iter().take(rays.len()), samples))
                });
                self.compare(
                    &format!("ray_ring_vs_circles/k{samples}/r{range}"),
                    &ring,
                    &exact,
                );
                let circles = circle_hits(bodies, origin, rays, range);
                let circle_ring = sampled(circles.into_iter().take(rays.len()), samples);
                self.compare(
                    &format!("sampling/k{samples}/r{range}"),
                    &circle_ring,
                    &exact,
                );
                if samples != 8 {
                    continue;
                }
                // Same directions, so these differences come from geometry alone.
                for (query, circle) in hits.into_iter().zip(circles).take(rays.len()) {
                    let name = format!("geometry/r{range}");
                    match (query, circle) {
                        (Some((distance, kind)), Some((reference, expected))) => {
                            if kind != expected {
                                self.count(format!("{name}/kind_differs"));
                            }
                            self.error(format!("{name}/{}", kind.name()), distance - reference);
                        }
                        (Some((_, kind)), None) => {
                            self.count(format!("{name}/query_only/{}", kind.name()))
                        }
                        (None, Some((_, kind))) => {
                            self.count(format!("{name}/circle_only/{}", kind.name()))
                        }
                        (None, None) => {}
                    }
                    self.count(format!("{name}/rays"));
                }
            }
            let centers = &rays[0];
            self.timed(
                format!("ball_ring/r{range}"),
                approach,
                centers.len(),
                || ball_ring(world, origin, centers, range),
            );
        }
        let records = self.timed(
            "records/projection".to_owned(),
            approach,
            bodies.len(),
            || projected(world, bodies, origin),
        );
        let circles = self.timed("records/circles".to_owned(), approach, 0, || {
            circled(bodies, origin)
        });
        self.timed("nearest/projection".to_owned(), approach, 1, || {
            world.project_point(
                origin,
                PointProjectionOptions {
                    collision_groups: CELESTIAL,
                    ..Default::default()
                },
            )
        });
        for (body, (record, circle)) in bodies.iter().zip(records.iter().zip(&circles)) {
            let (Some((point, distance)), Some((_, clearance))) = (record, circle) else {
                self.count(format!("records/missing/{}", body.kind.name()));
                continue;
            };
            let kind = body.kind.name();
            self.error(format!("records/clearance/{kind}"), distance - clearance);
            let expected = if body.kind == Kind::Edge {
                bearing(origin - body.center)
            } else {
                bearing(body.center - origin)
            };
            self.error(
                format!("records/bearing_degrees/{kind}"),
                wrap(bearing(*point - origin) - expected).abs().to_degrees(),
            );
        }
    }

    fn json(self) -> Value {
        let timings: BTreeMap<_, _> = self
            .timings
            .into_iter()
            .map(|(name, timing)| {
                let total: f64 = timing.microseconds.iter().sum();
                let per_query = (timing.queries > 0).then(|| total / timing.queries as f64);
                (
                    name,
                    json!({"per_decision_us": summary(timing.microseconds),
                    "per_query_us": per_query}),
                )
            })
            .collect();
        let errors: BTreeMap<_, _> = self
            .errors
            .into_iter()
            .map(|(name, values)| {
                let absolute = values.iter().map(|v| v.abs()).collect();
                (
                    name,
                    json!({"signed": summary(values), "absolute": summary(absolute)}),
                )
            })
            .collect();
        json!({"timings": timings, "errors": errors, "counts": self.counts})
    }
}

#[test]
fn circle_ring_matches_dense_circle_rays() {
    let bodies = [
        Body {
            kind: Kind::Planet,
            entity: PhysicsId::new(1),
            center: Vec2::new(300.0, 0.0),
            radius: 40.0,
        },
        Body {
            kind: Kind::Sun,
            entity: PhysicsId::new(2),
            center: Vec2::new(-200.0, 150.0),
            radius: 120.0,
        },
        Body {
            kind: Kind::Edge,
            entity: PhysicsId::new(3),
            center: Vec2::new(100.0, -50.0),
            radius: 900.0,
        },
    ];
    let origin = Vec2::new(20.0, 30.0);
    for range in [250.0, 1000.0] {
        let exact = circle_ring(&bodies, origin, range);
        let dense = sampled(
            directions(4096)
                .into_iter()
                .map(|d| circle_hit(&bodies, origin, d, range)),
            4096,
        );
        for (arc, (exact, dense)) in exact.iter().zip(&dense).enumerate() {
            assert!((exact.fill - dense.fill).abs() < 0.002, "arc {arc} {range}");
            match (exact.nearest, dense.nearest) {
                (Some((a, ak)), Some((b, bk))) => {
                    assert!(b >= a - 0.01 && b - a < 0.5, "arc {arc} {range} {a} {b}");
                    assert_eq!(ak, bk);
                }
                (None, None) => {}
                other => panic!("arc {arc} {range} {other:?}"),
            }
        }
    }
}

#[test]
#[ignore = "release-mode benchmark; see the module comment"]
fn guidance_sensing_cost() {
    let seeds: Vec<u64> = std::env::var("SPACEWARS_SENSING_COST_SEEDS")
        .unwrap_or_else(|_| "42".to_owned())
        .split(',')
        .map(|seed| seed.trim().parse().unwrap())
        .collect();
    let shapes = [DirtMode::WeakAngular, DirtMode::WeakRound];
    let rays = SAMPLES.map(directions);
    let mut report = Report::default();
    let mut worlds = Vec::new();
    let mut origin_counts = BTreeMap::<String, u64>::new();
    let started = Instant::now();
    for &seed in &seeds {
        for shape in shapes {
            let state = match_world(seed, shape);
            let world = &state.world.physics.world;
            let bodies = bodies(&state);
            let points = origins(&bodies, seed);
            let planets: Vec<_> = (0..state.world.planets.len())
                .map(|index| {
                    let entity = planet_entity(index);
                    let chunks = world
                        .collider_ids()
                        .filter(|id| id.entity == entity)
                        .count();
                    json!({"radius": state.world.planets[index].radius, "colliders": chunks})
                })
                .collect();
            worlds.push(
                json!({"seed": seed, "shape": shape, "colliders": world.collider_count(),
                "sun": state.world.sun.map(|sun| sun.radius),
                "universe_radius": state.world.config.universe_radius,
                "planets": planets, "origins": points.len()}),
            );
            for (band, origin) in points {
                report.measure(world, &bodies, &rays, &band, origin);
                *origin_counts.entry(band).or_default() += 1;
            }
        }
    }
    let output = json!({"seeds": seeds, "shapes": shapes, "worlds": worlds,
        "origins": origin_counts, "arcs": ARCS, "samples": SAMPLES, "ranges": RANGES,
        "ball_radius": BALL_RADIUS, "seconds": started.elapsed().as_secs_f64(),
        "results": report.json()});
    let text = serde_json::to_string_pretty(&output).unwrap();
    match std::env::var("SPACEWARS_SENSING_COST_OUT") {
        Ok(path) => std::fs::write(path, text + "\n").unwrap(),
        Err(_) => println!("{text}"),
    }
}
