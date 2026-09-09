//! Measured local flights across the assigned parked ship. No policy or world writes in sensors.
use super::*;
use engine_rapier::spaceling::jetpack as motor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CrossingDirection {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrossingPlan {
    pub planet: usize,
    pub revision: u64,
    pub direction: CrossingDirection,
    /// All geometry is in the planet's local frame; endpoints are real retained footing.
    pub start: Vec2,
    pub destination: Vec2,
    pub cruise_radius: f32,
    pub ship_position: Vec2,
    pub ship_angle: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JetpackCrossingObservation {
    pub version: u32,
    pub pilot: pilot::PilotObservationV1,
    pub charge: Option<f32>,
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
            burning: pack.is_some_and(|p| p.active),
            burn_seconds: pack.map_or(0.0, |p| p.burn_seconds),
            gravity: self.pilots[player].gravity,
            thrust: motor::THRUST,
            air_speed: motor::AIR_SPEED,
            surveyed,
            plan,
        }
    }

    fn crossing_plan(&self, player: usize, direction: CrossingDirection) -> Option<CrossingPlan> {
        let pilot = &self.pilots[player];
        if pilot.jetpack_charge.is_none()
            || self.world.physics.material_queries_dirty
            || !self.vehicle_settled(player)
        {
            return None;
        }
        let ship = &self.world.ships[pilot.vehicle.0];
        if ship.dead || ship.form != ShipForm::Ship {
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
        let endpoint = |side: f32| {
            [8.0, 9.0, 10.0, 11.0, 12.0].into_iter().find_map(|offset| {
                let ray_up = (center + right * side * offset).normalized();
                let hit = self.world.physics.material_ground_ray(
                    planet,
                    world(ray_up * (radius + 20.0)),
                    -ray_up.rotate_radians(frame.angle),
                    28.0,
                )?;
                let point = local(hit.point);
                (hit.normal.dot(ray_up.rotate_radians(frame.angle)) >= spec.min_support_alignment
                    && point_clear(point + ray_up * 1.25))
                .then_some(point)
            })
        };
        let left = endpoint(-1.0)?;
        let right = endpoint(1.0)?;
        let (start, destination) = match direction {
            CrossingDirection::Left => (right, left),
            CrossingDirection::Right => (left, right),
        };
        let top_start = start.normalized() * cruise_radius;
        let top_end = destination.normalized() * cruise_radius;
        let segment_clear = |a: Vec2, b: Vec2| {
            let count = (a.distance_to(b) / 0.20).ceil() as usize;
            count <= 128
                && (0..=count).all(|i| point_clear(a + (b - a) * (i as f32 / count.max(1) as f32)))
        };
        // Inflated capsule samples overlap along each short segment. The arc is
        // split into bounded chords, with the same real ship collider present.
        if !segment_clear(start + start.normalized() * 1.25, top_start)
            || !segment_clear(top_end, destination + destination.normalized() * 1.25)
        {
            return None;
        }
        let angle =
            (top_start.x * top_end.y - top_start.y * top_end.x).atan2(top_start.dot(top_end));
        if angle.abs() > 0.55 {
            return None;
        }
        for i in 0..16 {
            if !segment_clear(
                top_start.rotate_radians(angle * i as f32 / 16.0),
                top_start.rotate_radians(angle * (i + 1) as f32 / 16.0),
            ) {
                return None;
            }
        }
        Some(CrossingPlan {
            planet,
            revision: self.world.terrain.planets.get(&planet)?.field.revision(),
            direction,
            start,
            destination,
            cruise_radius,
            ship_position: center,
            ship_angle: body.angle - frame.angle,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: Duration = Duration::from_nanos(16_666_667);

    #[test]
    fn crossing_survey_is_read_only_and_requires_a_clear_real_corridor() {
        let mut state = SurfaceSortieScenario::init_material_jetpack(42, 1);
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
        let before = state.world.physics.world.snapshot_bytes().unwrap();
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
    }
}
