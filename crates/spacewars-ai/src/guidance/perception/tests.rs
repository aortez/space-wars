use super::*;
use crate::model::{DenseModel, MODEL_VERSION, ModelMemory, shared::SharedModel};
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{SurfaceSortieScenario, pilot::MaterialFlightStart};
use std::time::Duration;

const DT: Duration = Duration::from_nanos(16_666_667);

fn practice() -> scenario_spacewars::surface_sortie::SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material_flight(
        42,
        1,
        &[(
            PlayerId::PLAYER_1,
            MaterialFlightStart {
                bearing: 0.4,
                altitude: 90.0,
                radial_speed: -3.0,
                lateral_speed: 5.0,
                heading_offset: 0.3,
            },
        )],
    );
    SurfaceSortieScenario::step(
        &mut state,
        &Movement::default().encode(PlayerId::PLAYER_1),
        DT,
    );
    state
}

/// A real observation with the ship moved to `position`.
fn observation(position: Vec2, velocity: Vec2, angle: f32) -> FreeFlightObservation {
    moved(
        &practice().free_flight_observation(0).unwrap(),
        position,
        velocity,
        angle,
    )
}

fn moved(
    base: &FreeFlightObservation,
    position: Vec2,
    velocity: Vec2,
    angle: f32,
) -> FreeFlightObservation {
    let mut o = base.clone();
    o.ship.position = position;
    o.ship.velocity = velocity;
    o.ship.angle = angle;
    o
}

fn body(kind: CelestialKind, x: f32, y: f32, radius: f32) -> CelestialBody {
    CelestialBody {
        kind,
        position: Vec2::new(x, y),
        velocity: Vec2::ZERO,
        radius,
    }
}

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() <= 1e-4 * (1.0 + expected.abs())
}

#[test]
fn records_measure_nearest_surface_motion_and_size_in_the_ship_frame() {
    let mut planet = body(CelestialKind::Planet(2), 600.0, 500.0, 50.0);
    planet.velocity = Vec2::new(1.0, 0.0);
    let bodies = [
        body(CelestialKind::Sun, 500.0, 800.0, 100.0),
        body(CelestialKind::Edge, 300.0, 500.0, 1000.0),
        planet,
    ];
    let o = observation(Vec2::new(500.0, 500.0), Vec2::new(3.0, 4.0), 0.0);
    let p = Perception::read(&o, &bodies, Some(2));
    assert_eq!(p.omitted, 0);
    // The destination, then the sun (threat 200 - 2 × 4) before the edge
    // (800 - 2 × 3).
    let [destination, sun, edge] = p.bodies.as_slice() else {
        panic!("{:?}", p.bodies);
    };
    let expect = |r: &BodyRecord, kind, direction: Vec2, values: [f32; 4]| {
        assert_eq!(r.kind, kind);
        assert!(r.direction.distance_to(direction) < 1e-6, "{r:?}");
        for (actual, expected) in [r.clearance, r.closing_speed, r.sideways_speed, r.size]
            .into_iter()
            .zip(values)
        {
            assert!(close(actual, expected), "{r:?} {values:?}");
        }
    };
    assert!(destination.destination && !sun.destination && !edge.destination);
    expect(
        destination,
        Kind::Planet,
        Vec2::X,
        [50.0, 2.0, 4.0, 1.0 / 3.0],
    );
    let sun_size = (1.0_f32 / 3.0).asin() / FRAC_PI_2;
    expect(sun, Kind::Sun, Vec2::Y, [200.0, 4.0, -3.0, sun_size]);
    expect(edge, Kind::Edge, Vec2::X, [800.0, 3.0, 4.0, 1.0]);

    // Turning the ship a quarter counterclockwise turns every local reading
    // a quarter clockwise; speeds along and across are unchanged.
    let turned = Perception::read(
        &observation(Vec2::new(500.0, 500.0), Vec2::new(3.0, 4.0), FRAC_PI_2),
        &bodies,
        Some(2),
    );
    expect(
        &turned.bodies[0],
        Kind::Planet,
        -Vec2::Y,
        [50.0, 2.0, 4.0, 1.0 / 3.0],
    );
    expect(
        &turned.bodies[1],
        Kind::Sun,
        Vec2::X,
        [200.0, 4.0, -3.0, sun_size],
    );
    // The planet's nearest point leaves arc 0 for arc 12, below the ship.
    let nearest = |p: &Perception, arc: usize| p.arcs[arc].nearest.unwrap();
    assert!(close(nearest(&p, 0).distance, 50.0));
    assert!(close(nearest(&turned, 12).distance, 50.0));
    assert_eq!(nearest(&turned, 12).kind, Kind::Planet);
    assert!(close(nearest(&turned, 12).closing_speed, 2.0));
}

/// Nearest hit of one circle along a unit direction from the origin.
fn hit(circle: &CelestialBody, origin: Vec2, direction: Vec2) -> Option<f32> {
    let to = circle.position - origin;
    let along = to.dot(direction);
    let discriminant = along * along - to.length_squared() + circle.radius * circle.radius;
    if discriminant < 0.0 {
        return None;
    }
    let distance = if circle.kind == CelestialKind::Edge {
        along + discriminant.sqrt()
    } else {
        along - discriminant.sqrt()
    };
    (distance >= 0.0).then_some(distance)
}

fn kind(body: &CelestialBody) -> Kind {
    match body.kind {
        CelestialKind::Sun => Kind::Sun,
        CelestialKind::Planet(_) => Kind::Planet,
        CelestialKind::Edge => Kind::Edge,
    }
}

/// Worlds like the training matches: a sun, planets of varied sizes and the
/// edge, with the ship near a planet, near the sun or in open space.
fn worlds() -> Vec<(FreeFlightObservation, Vec<CelestialBody>)> {
    let mut seed = 0x9e37_79b9_u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    let base = practice().free_flight_observation(0).unwrap();
    let mut worlds = Vec::new();
    for case in 0..48 {
        let universe = 700.0 + 600.0 * next();
        let center = Vec2::splat(universe);
        let mut bodies = vec![body(CelestialKind::Sun, universe, universe, 200.0)];
        for planet in 0..3 {
            let angle = next() * TAU;
            let orbit = 260.0 + (universe - 420.0) * next();
            let mut b = body(
                CelestialKind::Planet(planet),
                0.0,
                0.0,
                15.0 + 135.0 * next(),
            );
            b.position = center + unit(angle) * orbit;
            b.velocity = unit(angle + FRAC_PI_2) * 15.0;
            bodies.push(b);
        }
        bodies.push(body(CelestialKind::Edge, universe, universe, universe));
        let anchor = &bodies[case % 4];
        let mut position = if case % 4 == 3 {
            center + unit(next() * TAU) * (universe * 0.9 * next())
        } else {
            anchor.position + unit(next() * TAU) * (anchor.radius + 10.0 + 200.0 * next())
        };
        // Stay inside the edge, so every arc sees some surface.
        let out = position - center;
        if out.length() > universe * 0.95 {
            position = center + out.normalized() * universe * 0.95;
        }
        let velocity = unit(next() * TAU) * 60.0 * next();
        bodies.retain(|b| {
            let d = b.position.distance_to(position);
            if b.kind == CelestialKind::Edge {
                d < b.radius - 2.0
            } else {
                d > b.radius + 2.0
            }
        });
        worlds.push((moved(&base, position, velocity, next() * TAU - PI), bodies));
    }
    worlds
}

#[test]
fn ring_matches_dense_rays_from_every_body() {
    const RAYS: usize = 2048;
    let mut checked_arcs = 0;
    for (o, bodies) in worlds() {
        let p = Perception::read(&o, &bodies, None);
        for (index, arc) in p.arcs.iter().enumerate() {
            // Every ray's nearest hit, with the arc's boundaries included.
            let hits: Vec<(f32, Kind, f32)> = (0..=RAYS)
                .filter_map(|ray| {
                    let local = (index as f32 + ray as f32 / RAYS as f32) * ARC;
                    let direction = unit(local + o.ship.angle);
                    bodies
                        .iter()
                        .filter_map(|b| {
                            let relative = o.ship.velocity - b.velocity;
                            hit(b, o.ship.position, direction)
                                .map(|d| (d, kind(b), relative.dot(direction)))
                        })
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                })
                .collect();
            assert_eq!(hits.len(), RAYS + 1, "the edge surrounds the ship");
            let covered = hits[..RAYS].iter().filter(|h| h.0 <= FILL_RANGE).count();
            let (distance, nearest_kind, closing) =
                *hits.iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
            let runner_up = hits
                .iter()
                .filter(|h| h.1 != nearest_kind)
                .map(|h| h.0)
                .fold(f32::INFINITY, f32::min);
            let exact = arc.nearest.unwrap();
            // Rays can only find a point at or beyond the exact minimum.
            assert!(
                exact.distance <= distance + 1e-5 * (1.0 + distance)
                    && distance - exact.distance < 0.05,
                "arc {index}: {exact:?} versus {distance}"
            );
            if runner_up - distance > 0.1 {
                assert_eq!(exact.kind, nearest_kind, "arc {index}");
                assert!(
                    (exact.closing_speed - closing).abs() < 0.05,
                    "arc {index}: {exact:?} versus {closing}"
                );
            }
            // Each covered span's two ends can each cost one sample.
            let sampled = covered as f32 / RAYS as f32;
            assert!(
                (arc.fill - sampled).abs() <= (2 * bodies.len()) as f32 / RAYS as f32,
                "arc {index}: fill {} versus {sampled}",
                arc.fill
            );
            checked_arcs += 1;
        }
        // The smallest arc distance is the exact clearance.
        let clearance = p
            .bodies
            .iter()
            .map(|b| b.clearance)
            .fold(f32::INFINITY, f32::min);
        let smallest = p
            .arcs
            .iter()
            .map(|a| a.nearest.unwrap().distance)
            .fold(f32::INFINITY, f32::min);
        assert!(close(smallest, clearance));
    }
    assert_eq!(checked_arcs, 48 * ARCS);
}

#[test]
fn slots_keep_the_destination_then_the_most_threatening_bodies() {
    let o = observation(Vec2::new(1000.0, 1000.0), Vec2::new(0.0, 10.0), 0.0);
    let mut bodies = vec![body(CelestialKind::Edge, 1000.0, 1000.0, 1000.0)];
    for planet in 0..9 {
        let x = 1100.0 + 100.0 * planet as f32;
        bodies.push(body(CelestialKind::Planet(planet), x, 1000.0, 20.0));
    }
    // Farther than planet 0, but closing at 70 units/s: its threat is
    // 200 - 2 × 70 against planet 0's 80.
    let mut fast = body(CelestialKind::Sun, 1000.0, 1300.0, 100.0);
    fast.velocity = Vec2::new(0.0, -60.0);
    bodies.push(fast);
    let p = Perception::read(&o, &bodies, Some(7));
    assert_eq!(p.bodies.len(), BODY_SLOTS);
    assert_eq!(p.omitted, 3);
    assert!(p.bodies[0].destination && close(p.bodies[0].clearance, 780.0));
    assert_eq!(p.bodies[1].kind, Kind::Sun);
    assert!(close(p.bodies[2].clearance, 80.0));
    assert!(p.bodies[1..].iter().all(|b| !b.destination));
    // The edge and the two farthest other planets are left out.
    assert!(
        p.bodies
            .iter()
            .all(|b| (b.kind != Kind::Edge && b.clearance < 700.0) || b.destination)
    );
    assert!(
        Perception::read(&o, &bodies, None)
            .bodies
            .iter()
            .all(|b| !b.destination)
    );
}

#[test]
fn a_ship_inside_a_surface_reads_contact_in_every_direction() {
    let bodies = [
        body(CelestialKind::Planet(0), 500.0, 500.0, 60.0),
        body(CelestialKind::Edge, 500.0, 500.0, 500.0),
    ];
    let o = observation(Vec2::new(500.0, 559.0), Vec2::new(1.0, -2.0), 0.3);
    let p = Perception::read(&o, &bodies, Some(0));
    assert_eq!(p.bodies[0].clearance, 0.0);
    assert_eq!(p.bodies[0].size, 1.0);
    for arc in p.arcs {
        let nearest = arc.nearest.unwrap();
        assert_eq!((nearest.distance, nearest.kind), (0.0, Kind::Planet));
        assert_eq!(arc.fill, 1.0);
    }
    let encoded = p.encode().unwrap();
    assert!(encoded.inputs.iter().all(|v| v.is_finite()));
}

#[test]
fn encoding_is_bounded_and_lays_out_slots_then_arcs() {
    for (o, bodies) in worlds() {
        let p = Perception::read(&o, &bodies, Some(1));
        assert!(p.bodies.len() <= BODY_SLOTS && p.omitted == 0);
        let encoded = p.encode().unwrap();
        assert_eq!(encoded.inputs.len(), INPUTS);
        assert!(
            encoded
                .inputs
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1.0)
        );
        let (slots, arcs) = encoded.inputs.split_at(BODY_SLOTS * BODY_VALUES);
        for (slot, values) in slots.chunks_exact(BODY_VALUES).enumerate() {
            let Some(record) = p.bodies.get(slot) else {
                assert!(values.iter().all(|v| *v == 0.0));
                continue;
            };
            assert_eq!(values[0], 1.0);
            assert_eq!(&values[1..4], &record.kind.one_hot());
            assert_eq!(values[4], f32::from(u8::from(record.destination)));
            // Speeds and distances stay strictly inside their bounds.
            assert!(values[7] > 0.0 && values[7] < 1.0);
            assert!(values[8].abs() < 1.0 && values[9].abs() < 1.0);
        }
        for (index, values) in arcs.chunks_exact(ARC_VALUES).enumerate() {
            let center = unit((index as f32 + 0.5) * ARC);
            assert_eq!(&values[6..], &[center.x, center.y]);
            assert_eq!(values[1], p.arcs[index].fill);
            assert_eq!(values[2..5].iter().sum::<f32>(), 1.0);
        }
    }
    let (o, bodies) = worlds().swap_remove(0);
    let mut p = Perception::read(&o, &bodies, None);
    p.bodies[0].closing_speed = f32::NAN;
    assert!(p.encode().is_err());
}

fn dense_parent() -> DenseModel {
    let mut state = 0x2545_f491_u32;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as f32 / u32::MAX as f32 * 2.0 - 1.0
    };
    DenseModel::new(crate::model::DenseGenome {
        version: MODEL_VERSION,
        contract: super::super::contract(),
        memory: ModelMemory::None,
        layers: vec![
            DenseLayer {
                outputs: 8,
                activation: Activation::Tanh,
                weights: (0..72).map(|_| next()).collect(),
                biases: (0..8).map(|_| next()).collect(),
            },
            DenseLayer {
                outputs: 3,
                activation: Activation::Linear,
                weights: (0..24).map(|_| next()).collect(),
                biases: (0..3).map(|_| next()).collect(),
            },
        ],
    })
    .unwrap()
}

#[test]
fn warm_start_decides_exactly_as_its_parent_in_a_world() {
    let parent = dense_parent();
    let genome = warm_start(parent.genome(), 4, 0.5, 11).unwrap();
    assert_eq!(genome.parameter_count(), 255);
    assert!(
        genome
            .encoders
            .iter()
            .all(|e| e.layer.weights.iter().all(|w| *w != 0.0))
    );
    let child = SharedModel::new(genome.clone()).unwrap();
    let other = SharedModel::new(warm_start(parent.genome(), 4, 0.5, 12).unwrap()).unwrap();
    assert_ne!(child.id(), other.id());

    let mut state = practice();
    let o = state.free_flight_observation(0).unwrap();
    let binding = Binding {
        owner: o.owner,
        spaceling: o.spaceling,
        vehicle: o.vehicle,
        planet: o.planet,
        epoch: 1,
    };
    let target = Target {
        id: 1,
        position: o.ship.position + Vec2::new(30.0, 20.0),
        velocity: Vec2::ZERO,
    };
    let mut dense = Controller::new(Policy::Model(parent.clone()), binding).unwrap();
    let mut shared = Controller::new(Policy::Perception(child.clone()), binding).unwrap();
    let mut alternate = Controller::new(Policy::Perception(other), binding).unwrap();
    assert!(Controller::new(Policy::Perception(child.clone()), binding).is_ok());
    let mut summaries_moved = false;
    for _ in 0..60 {
        let o = state.free_flight_observation(0).unwrap();
        let request = Request {
            binding,
            tick: o.tick,
            target_id: target.id,
            features: Features::read(&o, target),
        };
        let perception = Perception::read(&o, &state.celestial_bodies(), Some(o.planet));
        let expected = dense.decide(request).unwrap();
        let actual = shared.decide_perceiving(request, &perception).unwrap();
        let bits = |d: &Decision| {
            d.raw_outputs
                .as_ref()
                .unwrap()
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        };
        assert_eq!(bits(&expected), bits(&actual));
        assert_eq!(expected.movement, actual.movement);
        assert_eq!(
            bits(&actual),
            bits(&alternate.decide_perceiving(request, &perception).unwrap())
        );
        assert_eq!(expected.perception, None);
        let sensed = actual.perception.as_ref().unwrap();
        // A trunk weight on a pooled summary would change the decision.
        let mut probe = genome.clone();
        probe.trunk[0].weights[9] = 0.5;
        let inputs = [actual.encoded.inputs.as_slice(), &sensed.inputs].concat();
        let changed = SharedModel::new(probe).unwrap().infer(&inputs).unwrap();
        summaries_moved |= changed != *actual.raw_outputs.as_ref().unwrap();
        SurfaceSortieScenario::step(&mut state, &expected.movement.encode(binding.owner), DT);
    }
    assert!(summaries_moved);
}

#[test]
fn controllers_require_perception_only_for_perception_policies() {
    let parent = dense_parent();
    let child = SharedModel::new(warm_start(parent.genome(), 4, 0.5, 1).unwrap()).unwrap();
    let state = practice();
    let o = state.free_flight_observation(0).unwrap();
    let binding = Binding {
        owner: o.owner,
        spaceling: o.spaceling,
        vehicle: o.vehicle,
        planet: o.planet,
        epoch: 1,
    };
    let target = Target {
        id: 1,
        position: o.ship.position,
        velocity: Vec2::ZERO,
    };
    let request = Request {
        binding,
        tick: o.tick,
        target_id: 1,
        features: Features::read(&o, target),
    };
    let perception = Perception::read(&o, &state.celestial_bodies(), Some(o.planet));
    let mut shared = Controller::new(Policy::Perception(child.clone()), binding).unwrap();
    assert!(shared.clone().decide(request).is_err());
    let first = shared.decide_perceiving(request, &perception).unwrap();
    assert_eq!(
        shared.decide_perceiving(request, &perception).unwrap(),
        first
    );
    let mut moved = perception.clone();
    moved.bodies[0].clearance += 1.0;
    assert!(shared.decide_perceiving(request, &moved).is_err());

    let mut dense = Controller::new(Policy::Model(parent.clone()), binding).unwrap();
    let mut plain = dense.clone();
    assert_eq!(
        dense.decide_perceiving(request, &perception).unwrap(),
        plain.decide(request).unwrap()
    );
    assert!(Controller::new(Policy::Model(parent), binding).is_ok());
    // Each model must carry its own observation contract.
    let mut wrong = child.genome().clone();
    wrong.contract = super::super::contract();
    assert!(SharedModel::new(wrong).is_err());
    let mut renamed = child.genome().clone();
    renamed.contract.observation = "arrival_local_9_v1".into();
    let renamed = SharedModel::new(renamed).unwrap();
    assert!(Controller::new(Policy::Perception(renamed), binding).is_err());
    assert!(warm_start(child_contract_genome().genome(), 4, 0.5, 1).is_err());
    assert!(warm_start(dense_parent().genome(), 4, f32::NAN, 1).is_err());
}

/// A dense genome with a contract other than the nine-feature one.
fn child_contract_genome() -> DenseModel {
    let mut genome = dense_parent().genome().clone();
    genome.contract.role = "other_role_v1".into();
    DenseModel::new(genome).unwrap()
}
