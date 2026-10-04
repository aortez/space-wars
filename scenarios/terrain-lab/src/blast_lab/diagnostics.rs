use std::collections::{BTreeMap, BTreeSet};

use engine_rapier::world::ColliderId;

use super::*;

/// Observation only: no sleeping, freezing, deposition, or solver changes.
#[derive(Debug, Clone, Copy, Default)]
pub struct MotionStats {
    /// Loose cell centers more than one cell above the original surface.
    pub above_surface_cells: u64,
    /// Slow relative to the ground, with an upward contact path to that ground.
    /// An instantaneous diagnostic, not a claim of permanent settling.
    pub supported_slow_cells: u64,
    /// Largest positive center clearance from the original surface, in units.
    pub max_clearance: f32,
}

impl BlastLab {
    pub fn motion_stats(&self) -> MotionStats {
        let ground = self.bodies[0].assembly.body();
        let ground_motion = self.physics.motion(ground).unwrap();
        let support_ids = |collider: ColliderId, up: Vec2| {
            self.physics
                .surface_contacts(collider)
                .filter(move |contact| contact.separation <= 0.02 && contact.normal.dot(up) > 0.2)
                .map(|contact| contact.collider.entity)
        };
        let up = |position: Vec2| match self.config.fixture {
            Fixture::Flat | Fixture::Slope => Vec2::new(0.0, 1.0),
            Fixture::MovingPlanet => {
                let offset = position - ground_motion.position;
                if offset.length_squared() > 0.0001 {
                    offset / offset.length()
                } else {
                    Vec2::new(0.0, 1.0)
                }
            }
        };
        let mut supports = BTreeMap::<PhysicsId, BTreeSet<PhysicsId>>::new();
        for (body, _) in self.bodies().skip(1) {
            let direction = up(self.physics.center_of_mass(body.assembly.body()).unwrap());
            supports.insert(
                body.id,
                body.geometry
                    .chunks()
                    .iter()
                    .flat_map(|chunk| body.assembly.chunk_collider_ids(chunk))
                    .flat_map(|collider| support_ids(collider, direction))
                    .collect(),
            );
        }
        for (grain, motion) in self.grains() {
            supports.insert(
                grain.id(),
                support_ids(grain.collider(), up(motion.position)).collect(),
            );
        }
        // Grains may also be supported through the ordinary test object.
        if let Some(probe) = self.probe_snapshot() {
            supports.insert(
                probe::body().entity,
                support_ids(probe::collider(), up(probe.motion.position)).collect(),
            );
        }
        // Contact with another airborne piece does not establish ground support.
        let mut supported = BTreeSet::from([GROUND]);
        loop {
            let before = supported.len();
            for (id, below) in &supports {
                if below.iter().any(|id| supported.contains(id)) {
                    supported.insert(*id);
                }
            }
            if supported.len() == before {
                break;
            }
        }
        let mut stats = MotionStats::default();
        let mut observe = |id, point: Vec2, velocity: Vec2, angular_velocity: f32| {
            let local = (point - ground_motion.position).rotate_radians(-ground_motion.angle);
            let clearance = match self.config.fixture {
                Fixture::Flat => local.y,
                Fixture::Slope => (local.y - SLOPE * local.x) / (1.0 + SLOPE * SLOPE).sqrt(),
                Fixture::MovingPlanet => local.length() - PLANET_RADIUS,
            };
            stats.max_clearance = stats.max_clearance.max(clearance);
            stats.above_surface_cells += u64::from(clearance > self.config.cell_size);
            let ground_velocity = self.physics.velocity_at_point(ground, point).unwrap();
            if supported.contains(&id)
                && velocity.distance_to(ground_velocity) < 0.2
                && (angular_velocity - ground_motion.angular_velocity).abs()
                    * self.config.cell_size
                    * 0.5
                    < 0.2
            {
                stats.supported_slow_cells += 1;
            }
        };
        for (body, motion) in self.bodies().skip(1) {
            for (index, cell) in body.terrain.cells().iter().enumerate() {
                if cell.material == MaterialId::VOID {
                    continue;
                }
                let coordinate = CellCoord::new(
                    (index as u32 % body.terrain.width()) as i32,
                    (index as u32 / body.terrain.width()) as i32,
                );
                let point = motion.position
                    + body
                        .terrain
                        .cell_center(coordinate)
                        .rotate_radians(motion.angle);
                observe(
                    body.id,
                    point,
                    self.physics
                        .velocity_at_point(body.assembly.body(), point)
                        .unwrap(),
                    motion.angular_velocity,
                );
            }
        }
        for (grain, motion) in self.grains() {
            observe(
                grain.id(),
                motion.position,
                motion.linear_velocity,
                motion.angular_velocity,
            );
        }
        stats
    }
}
