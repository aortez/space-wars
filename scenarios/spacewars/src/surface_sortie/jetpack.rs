//! Bounded flights across vehicles and breaks in retained ground. Read-only sensors.
use super::*;
use engine_rapier::spaceling::jetpack as motor;
use ground_navigation::GroundMap;

pub mod flight;
pub mod forecast;

pub const MAX_TERRAIN_CROSSINGS: usize = 8;
/// The executor's final horizontal landing window, in world units.
pub const CROSSING_ARRIVAL_RANGE: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum CrossingAnchor {
    Vehicle {
        index: usize,
        form: ShipForm,
        position: Vec2,
        angle: f32,
    },
    GroundGap {
        from: u16,
        to: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CrossingDirection {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CrossingPlan {
    pub planet: usize,
    pub revision: u64,
    pub direction: CrossingDirection,
    /// All geometry is in the planet's local frame; endpoints are real retained footing.
    pub start: Vec2,
    pub destination: Vec2,
    pub cruise_radius: f32,
    pub anchor: CrossingAnchor,
}

impl CrossingPlan {
    pub fn same_corridor(&self, other: &Self) -> bool {
        let mut endpoint_tolerance = 0.5;
        let anchor_matches = match (&self.anchor, &other.anchor) {
            (
                CrossingAnchor::Vehicle {
                    index: a,
                    form: af,
                    position: ap,
                    angle: aa,
                },
                CrossingAnchor::Vehicle {
                    index: b,
                    form: bf,
                    position: bp,
                    angle: ba,
                },
            ) => {
                a == b
                    && af == bf
                    && ap.distance_to(*bp) <= 0.5
                    && ((aa - ba + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI)
                        .abs()
                        <= 0.1
            }
            (
                CrossingAnchor::GroundGap { from: a, to: b },
                CrossingAnchor::GroundGap { from: c, to: d },
            ) => {
                // A small capsule can alternately clear adjacent samples at a
                // rocking pod's edge. These IDs mark a walk-graph gap, not a
                // persistent obstacle. A freshly measured neighboring corridor
                // may continue the flight within its existing landing window.
                let neighboring = |a: u16, b: u16| {
                    let span = ground_navigation::GROUND_SAMPLES as u16;
                    a < span && b < span && (a.abs_diff(b) <= 1 || a.abs_diff(b) == span - 1)
                };
                endpoint_tolerance = CROSSING_ARRIVAL_RANGE;
                neighboring(*a, *c) && neighboring(*b, *d)
            }
            _ => false,
        };
        anchor_matches
            && self.planet == other.planet
            && self.direction == other.direction
            && self.start.distance_to(other.start) <= endpoint_tolerance
            && self.destination.distance_to(other.destination) <= endpoint_tolerance
            && (self.cruise_radius - other.cruise_radius).abs() <= 0.25
    }

    pub fn reversed(&self) -> Self {
        Self {
            direction: match self.direction {
                CrossingDirection::Left => CrossingDirection::Right,
                CrossingDirection::Right => CrossingDirection::Left,
            },
            start: self.destination,
            destination: self.start,
            ..*self
        }
    }
}

/// Equipment is sampled every tick; bounded bidirectional corridors use the
/// same completed, staggered survey as the ground map.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JetpackNavigationObservation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vehicle_forecast: Option<forecast::VehicleCrossingForecast>,

    pub charge: f32,
    pub reference_velocity: Vec2,
    pub burning: bool,
    pub burn_seconds: f32,
    pub gravity: Vec2,
    pub surveyed: bool,
    pub crossing: Option<CrossingPlan>,
    pub terrain_crossings: Vec<CrossingPlan>,
}

impl JetpackNavigationObservation {
    pub fn for_crossing(
        &self,
        pilot: &pilot::PilotObservationV1,
        selected: &CrossingPlan,
    ) -> JetpackCrossingObservation {
        JetpackCrossingObservation {
            version: 1,
            pilot: pilot.clone(),
            charge: Some(self.charge),
            reference_velocity: self.reference_velocity,
            burning: self.burning,
            burn_seconds: self.burn_seconds,
            gravity: self.gravity,
            thrust: motor::THRUST,
            air_speed: motor::AIR_SPEED,
            surveyed: self.surveyed,
            plan: self
                .crossing
                .iter()
                .chain(&self.terrain_crossings)
                .flat_map(|plan| [*plan, plan.reversed()])
                .find(|plan| selected.same_corridor(plan)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JetpackCrossingObservation {
    pub version: u32,
    pub pilot: pilot::PilotObservationV1,
    pub charge: Option<f32>,
    pub reference_velocity: Vec2,
    pub burning: bool,
    pub burn_seconds: f32,
    pub gravity: Vec2,
    pub thrust: f32,
    pub air_speed: f32,
    pub surveyed: bool,
    pub plan: Option<CrossingPlan>,
}

impl SurfaceSortieScenario {
    /// Opt-in equipment for the first bot crossing demonstration.
    pub fn init_material_jetpack(seed: u64, players: usize) -> SurfaceSortieState {
        let mut state = Self::init_material(seed, players);
        state.enable_jetpacks();
        state
    }
}

impl SurfaceSortieState {
    pub fn jetpack_navigation_observation(
        &self,
        player: usize,
    ) -> Option<JetpackNavigationObservation> {
        self.jetpack_navigation_with_ground(player, self.ground_navigation_map(player).as_ref())
    }

    pub(super) fn jetpack_navigation_with_ground(
        &self,
        player: usize,
        ground: Option<&GroundMap>,
    ) -> Option<JetpackNavigationObservation> {
        #[cfg(feature = "sensor-profile")]
        let _profile = super::sensor_profile::Scope::new("jetpack_navigation_with_ground");
        let pilot = &self.pilots[player];
        let pack = pilot.body.as_ref().and_then(|body| body.jetpack());
        let charge = pack.map(|p| p.charge).or(pilot.jetpack_charge)?;
        let surveyed = self.location(player) == PilotLocation::OnFoot
            && !self.world.physics.material_queries_dirty
            && (self.world.tick + player as u64 * 15).is_multiple_of(30);
        Some(JetpackNavigationObservation {
            vehicle_forecast: None,
            charge,
            reference_velocity: pack.map_or(Vec2::ZERO, |p| p.reference_velocity),
            burning: pack.is_some_and(|p| p.active),
            burn_seconds: pack.map_or(0.0, |p| p.burn_seconds),
            gravity: pilot.gravity,
            surveyed,
            crossing: surveyed
                .then(|| self.crossing_plan(player, CrossingDirection::Left))
                .flatten(),
            terrain_crossings: ground
                .filter(|_| surveyed)
                .map_or_else(Vec::new, |map| self.terrain_crossings(player, map)),
        })
    }

    pub fn enable_jetpacks(&mut self) {
        for pilot in &mut self.pilots {
            if pilot.jetpack_charge.is_none() {
                pilot.jetpack_charge = Some(1.0);
                if let Some(body) = &mut pilot.body {
                    assert!(body.equip_jetpack(1.0));
                }
            }
        }
    }

    pub fn jetpack_crossing_observation(
        &self,
        player: usize,
        direction: CrossingDirection,
    ) -> JetpackCrossingObservation {
        let pilot = self.pilot_observation(
            player,
            Some(pilot::LandingSiteId {
                planet: self.motion_planet_index(player),
                bearing: 0,
            }),
        );
        let pack = self.pilots[player]
            .body
            .as_ref()
            .and_then(|body| body.jetpack());
        let surveyed = pilot.queries_ready && (pilot.tick + player as u64 * 15).is_multiple_of(30);
        let plan = if surveyed {
            self.crossing_plan(player, direction)
        } else {
            None
        };
        JetpackCrossingObservation {
            version: 1,
            pilot,
            charge: pack
                .map(|p| p.charge)
                .or(self.pilots[player].jetpack_charge),
            reference_velocity: pack.map_or(Vec2::ZERO, |p| p.reference_velocity),
            burning: pack.is_some_and(|p| p.active),
            burn_seconds: pack.map_or(0.0, |p| p.burn_seconds),
            gravity: self.pilots[player].gravity,
            thrust: motor::THRUST,
            air_speed: motor::AIR_SPEED,
            surveyed,
            plan,
        }
    }

    pub(super) fn crossing_plan(
        &self,
        player: usize,
        direction: CrossingDirection,
    ) -> Option<CrossingPlan> {
        #[cfg(feature = "sensor-profile")]
        let _profile = super::sensor_profile::Scope::new("crossing_plan");
        let pilot = &self.pilots[player];
        if pilot.jetpack_charge.is_none()
            || self.world.physics.material_queries_dirty
            || !self.vehicle_settled(player)
        {
            return None;
        }
        let ship = &self.world.ships[pilot.vehicle.0];
        if ship.dead {
            return None;
        }
        let planet = pilot.planet;
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let local = |point: Vec2| (point - frame.position).rotate_radians(-frame.angle);
        let world = |point: Vec2| frame.position + point.rotate_radians(frame.angle);
        let body = self
            .world
            .physics
            .world
            .motion(self.world.physics.ship_body(pilot.vehicle.0))?;
        let center = local(body.position);
        let up = center.normalized();
        let right = Vec2::new(up.y, -up.x);
        let outline = self
            .world
            .physics
            .surface_vehicle_outline(pilot.vehicle.0, ship)?;
        let radius = self.world.planets[planet].radius;
        let cruise_radius = outline
            .iter()
            .map(|&point| local(point).length())
            .fold(radius, f32::max)
            + 2.5;
        if cruise_radius > radius + 18.0 {
            return None;
        }
        let spec = Self::spec();
        let clear = self.world.physics.world.capsule_clearance_test_excluding(
            spec.half_segment,
            spec.radius + 0.20,
            spec.collision_groups,
            vec![pilot_physics_id(pilot.owner)],
        );
        let point_clear = |point: Vec2| {
            clear(
                world(point),
                rotation_for_direction(point.normalized().rotate_radians(frame.angle)),
            )
        };
        let half_width = outline
            .iter()
            .map(|&point| (local(point) - center).dot(right).abs())
            .fold(0.0, f32::max);
        let endpoint = |side: f32| {
            (0..5).find_map(|i| {
                let offset = (half_width + 2.0).ceil() + i as f32;
                let ray_up = (center + right * side * offset).normalized();
                let hit = self.world.physics.material_ground_ray(
                    planet,
                    world(ray_up * (radius + 20.0)),
                    -ray_up.rotate_radians(frame.angle),
                    28.0,
                )?;
                let point = local(hit.point);
                (hit.normal.dot(ray_up.rotate_radians(frame.angle)) >= spec.min_support_alignment
                    && point_clear(point + ray_up * corridor_endpoint_height()))
                .then_some(point)
            })
        };
        let left = endpoint(-1.0)?;
        let right = endpoint(1.0)?;
        let (start, destination) = match direction {
            CrossingDirection::Left => (right, left),
            CrossingDirection::Right => (left, right),
        };
        if !corridor_clear(start, destination, cruise_radius, &point_clear) {
            return None;
        }
        Some(CrossingPlan {
            planet,
            revision: self.world.terrain.planets.get(&planet)?.field.revision(),
            direction,
            start,
            destination,
            cruise_radius,
            anchor: CrossingAnchor::Vehicle {
                index: pilot.vehicle.0,
                form: ship.form,
                position: center,
                angle: body.angle - frame.angle,
            },
        })
    }

    pub(super) fn terrain_crossings(&self, player: usize, map: &GroundMap) -> Vec<CrossingPlan> {
        #[cfg(feature = "sensor-profile")]
        let _profile = super::sensor_profile::Scope::new("terrain_crossings");
        let Some(actor) = self.spaceling_snapshot(player) else {
            return Vec::new();
        };
        let frame = motion::SurfaceFrame::read(&self.world.physics, map.planet);
        let local = (actor.motion.position - frame.position).rotate_radians(-frame.angle);
        let n = map.nodes.len();
        if n < 2 {
            return Vec::new();
        }
        let mut indices = [usize::MAX; ground_navigation::GROUND_SAMPLES];
        for (i, node) in map.nodes.iter().enumerate() {
            indices[usize::from(node.id)] = i;
        }
        let mut adjacent = vec![0_u8; n];
        for edge in &map.edges {
            let a = indices[usize::from(edge.from)];
            let b = indices[usize::from(edge.to)];
            if (a + 1) % n == b {
                adjacent[a] |= 1;
            }
            if (b + 1) % n == a {
                adjacent[b] |= 2;
            }
        }
        let mut gaps = (0..n)
            .filter(|&i| {
                adjacent[i] != 3
                    && map.nodes[i]
                        .position
                        .distance_to(map.nodes[(i + 1) % n].position)
                        <= 12.0
            })
            .collect::<Vec<_>>();
        gaps.sort_by(|&a, &b| {
            map.nodes[a]
                .position
                .distance_to(local)
                .total_cmp(&map.nodes[b].position.distance_to(local))
                .then(a.cmp(&b))
        });
        let spec = Self::spec();
        let capsule = self.world.physics.world.capsule_clearance_test_excluding(
            spec.half_segment,
            spec.radius + 0.20,
            spec.collision_groups,
            vec![pilot_physics_id(self.pilots[player].owner)],
        );
        let clear = |point: Vec2| {
            capsule(
                frame.position + point.rotate_radians(frame.angle),
                rotation_for_direction(point.normalized().rotate_radians(frame.angle)),
            )
        };
        let mut plans = Vec::new();
        // Reuse accepted retained footing. A few wider endpoints allow a climb
        // beside an overhanging pod or a short step without standing on debris.
        for i in gaps.into_iter().take(MAX_TERRAIN_CROSSINGS) {
            let anchor = CrossingAnchor::GroundGap {
                from: map.nodes[i].id,
                to: map.nodes[(i + 1) % n].id,
            };
            'endpoints: for margin in 0..8.min(n / 2) {
                let a = map.nodes[(i + n - margin) % n].position;
                let b = map.nodes[(i + 1 + margin) % n].position;
                if a.distance_to(b) > 24.0 || a.distance_to(b) < 1.0 {
                    continue;
                }
                for height in [3.0, 5.0, 7.0] {
                    let cruise = a.length().max(b.length()) + height;
                    if cruise > a.length().min(b.length()) + 10.0 {
                        continue;
                    }
                    if corridor_clear(a, b, cruise, &clear) {
                        plans.push(CrossingPlan {
                            planet: map.planet,
                            revision: map.revision,
                            direction: CrossingDirection::Left,
                            start: a,
                            destination: b,
                            cruise_radius: cruise,
                            anchor,
                        });
                        break 'endpoints;
                    }
                }
            }
        }
        plans
    }
}

fn corridor_endpoint_height() -> f32 {
    let spec = SurfaceSortieState::spec();
    // Include the flight capsule's extra clearance on supported slopes.
    spec.half_segment + (spec.radius + 0.20) / spec.min_support_alignment + 0.08
}

fn corridor_clear(
    start: Vec2,
    destination: Vec2,
    radius: f32,
    clear: &impl Fn(Vec2) -> bool,
) -> bool {
    let top_start = start.normalized() * radius;
    let top_end = destination.normalized() * radius;
    let segment = |a: Vec2, b: Vec2| {
        let count = (a.distance_to(b) / 0.20).ceil() as usize;
        count <= 128 && (0..=count).all(|i| clear(a + (b - a) * (i as f32 / count.max(1) as f32)))
    };
    let height = corridor_endpoint_height();
    if !segment(start + start.normalized() * height, top_start)
        || !segment(top_end, destination + destination.normalized() * height)
    {
        return false;
    }
    let angle = (top_start.x * top_end.y - top_start.y * top_end.x).atan2(top_start.dot(top_end));
    angle.abs() <= 0.55
        && (0..16).all(|i| {
            segment(
                top_start.rotate_radians(angle * i as f32 / 16.0),
                top_start.rotate_radians(angle * (i + 1) as f32 / 16.0),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: Duration = Duration::from_nanos(16_666_667);

    #[test]
    fn fresh_adjacent_gap_measurement_can_continue_a_flight_within_its_landing_window() {
        let original = CrossingPlan {
            planet: 0,
            revision: 0,
            direction: CrossingDirection::Left,
            start: Vec2::new(-1.46, -59.5),
            destination: Vec2::new(2.19, -59.5),
            cruise_radius: 64.54,
            anchor: CrossingAnchor::GroundGap { from: 254, to: 259 },
        };
        let mut fresh = original;
        fresh.anchor = CrossingAnchor::GroundGap { from: 255, to: 259 };
        fresh.destination.x = 2.92;
        assert!(original.same_corridor(&fresh));
        assert!(original.reversed().same_corridor(&fresh.reversed()));
        let accepted = fresh;
        fresh.destination.x += 0.4;
        assert!(
            !original.same_corridor(&fresh),
            "endpoint moved outside the landing window"
        );
        fresh = accepted;
        fresh.anchor = CrossingAnchor::GroundGap { from: 256, to: 259 };
        assert!(
            !original.same_corridor(&fresh),
            "a different gap needs a new route"
        );
        fresh = accepted;
        fresh.cruise_radius += 0.3;
        assert!(
            !original.same_corridor(&fresh),
            "a higher flight needs a new route"
        );
        fresh = accepted;
        fresh.revision += 1;
        assert!(
            original.same_corridor(&fresh),
            "a fresh physical survey can revalidate the same corridor after an edit"
        );
        fresh.planet += 1;
        assert!(
            !original.same_corridor(&fresh),
            "another planet cannot revalidate the flight"
        );
    }

    #[test]
    fn crossing_survey_is_read_only_and_requires_a_clear_real_corridor() {
        let mut state = SurfaceSortieScenario::init_material_jetpack(42, 1);
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
        assert!(!state.jetpack_navigation_observation(0).unwrap().surveyed);
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1)],
            DT,
        );
        for _ in 0..29 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let navigation = state.jetpack_navigation_observation(0).unwrap();
        assert_eq!(
            Some(navigation.clone()),
            state.jetpack_navigation_observation(0)
        );
        let both = navigation.crossing.as_ref().unwrap();
        assert_eq!(both.reversed().reversed(), *both);
        let o = state.jetpack_crossing_observation(0, CrossingDirection::Left);
        assert_eq!(
            Some(o.clone()),
            Some(state.jetpack_crossing_observation(0, CrossingDirection::Left))
        );
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
        let plan = o.plan.expect("parked ship has a measured corridor");
        assert!(plan.start.distance_to(plan.destination) > 12.0);
        let frame = state.planet_motion(0);
        let position = frame.position
            + plan.start.normalized().rotate_radians(frame.angle) * plan.cruise_radius;
        use engine_rapier::world::{
            BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
        };
        let id = PhysicsId::new(9_000_000);
        assert!(state.world.physics.world.insert_body(
            BodyId::new(id, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position,
                ..Default::default()
            },
            &[ColliderSpec::cuboid(
                ColliderId::new(id, ColliderRole::PRIMARY, 0),
                3.0,
                3.0
            )]
        ));
        for _ in 0..30 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
        assert!(
            state
                .jetpack_crossing_observation(0, CrossingDirection::Left)
                .plan
                .is_none(),
            "a solid roof blocks the flight even with charge available"
        );
        state.world.physics.material_queries_dirty = true;
        let dirty = state.jetpack_crossing_observation(0, CrossingDirection::Left);
        assert!(!dirty.surveyed && dirty.plan.is_none());
        let navigation = state.jetpack_navigation_observation(0).unwrap();
        assert!(
            !navigation.surveyed
                && navigation.crossing.is_none()
                && navigation.terrain_crossings.is_empty()
        );
    }
}
