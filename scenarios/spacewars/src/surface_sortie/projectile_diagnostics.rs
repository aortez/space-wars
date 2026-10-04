//! Opt-in physical projectile samples for offline diagnosis, never policy input.
use super::*;
use engine_rapier::world::BodyId as PhysicsBodyId;
use pilot::PilotMotion;

pub const PROJECTILE_RANGE: f32 = SHIP_SENSOR_HAZARD_RANGE;
pub const MAX_PROJECTILES: usize = SHIP_SENSOR_MAX_HAZARDS;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectileDiagnostics {
    pub version: u8,
    pub tick: u64,
    pub actor: PlayerId,
    pub vehicle: VehicleId,
    pub ship_form: ShipForm,
    pub observer: PilotMotion,
    /// Current solid-collider AABB enclosed about the physical body origin.
    /// Conservative instantaneous geometry, not a swept collision certificate.
    pub observer_radius: f32,
    pub range: f32,
    pub capacity: usize,
    pub debris_scanned: usize,
    pub unavailable_shells: usize,
    pub shells_in_range: usize,
    pub projectiles: Vec<ProjectileSample>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ProjectileSample {
    pub id: DebrisId,
    pub owner: Option<PlayerId>,
    pub spawn_tick: u64,
    pub radius: f32,
    pub collision_radius: f32,
    pub motion: PilotMotion,
    /// Projectile minus observer, in world axes, at physical body origins.
    pub relative_position: Vec2,
    /// Difference of velocity-at-origin, including each body's angular motion.
    pub relative_velocity: Vec2,
}

#[derive(Debug, Clone, Copy)]
struct RankedProjectile {
    distance_squared: f32,
    sample: ProjectileSample,
}

impl PartialEq for RankedProjectile {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for RankedProjectile {}
impl PartialOrd for RankedProjectile {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for RankedProjectile {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.distance_squared
            .total_cmp(&other.distance_squared)
            .then_with(|| self.sample.id.cmp(&other.sample.id))
    }
}

fn physical_motion(state: &SurfaceSortieState, body: PhysicsBodyId) -> Option<PilotMotion> {
    let world = &state.world.physics.world;
    let m = world.motion(body)?;
    Some(PilotMotion {
        position: m.position,
        velocity: world.velocity_at_point(body, m.position)?,
        angle: m.angle,
        spin: m.angular_velocity,
    })
}

fn enclosing_radius(state: &SurfaceSortieState, body: PhysicsBodyId, origin: Vec2) -> Option<f32> {
    let (min, max) = state.world.physics.world.body_solid_bounds(body)?;
    Some(
        [min, max, Vec2::new(min.x, max.y), Vec2::new(max.x, min.y)]
            .into_iter()
            .map(|corner| corner.distance_to(origin))
            .fold(0.0, f32::max),
    )
}

impl SurfaceSortieState {
    /// Read current physical bodies without ray casts, planner work, sensor
    /// profiling or mutation. Scans debris once, retains at most 64 shells
    /// within 600 units, ordered by distance then stable physics identity.
    /// Includes own shells; visibility and immunity are not inferred here.
    /// None means no aboard, living vehicle with a physical solid body.
    pub fn projectile_diagnostics(&self, player: usize) -> Option<ProjectileDiagnostics> {
        let pilot = self.pilots.get(player)?;
        let ship = self.world.ships.get(pilot.vehicle.0)?;
        if pilot.body.is_some() || ship.dead {
            return None;
        }
        let body = self.world.physics.ship_body(pilot.vehicle.0);
        let observer = physical_motion(self, body)?;
        let observer_radius = enclosing_radius(self, body, observer.position)?;
        let mut nearest = BinaryHeap::with_capacity(MAX_PROJECTILES + 1);
        let mut shells_in_range = 0;
        let mut unavailable_shells = 0;
        for debris in &self.world.debris {
            if debris.dead || debris.kind != DebrisKind::Shell {
                continue;
            }
            let sample = (|| {
                let id = DebrisId::from_value(debris.physics_id)?;
                let body = physics::primary_body(PhysicsId::new(debris.physics_id));
                let motion = physical_motion(self, body)?;
                Some(ProjectileSample {
                    id,
                    owner: debris.owner_id.and_then(PlayerId::from_index),
                    spawn_tick: debris.spawn_tick,
                    radius: debris.radius,
                    collision_radius: enclosing_radius(self, body, motion.position)?,
                    motion,
                    relative_position: motion.position - observer.position,
                    relative_velocity: motion.velocity - observer.velocity,
                })
            })();
            let Some(sample) = sample else {
                unavailable_shells += 1;
                continue;
            };
            let distance_squared = sample.relative_position.length_squared();
            if distance_squared > PROJECTILE_RANGE * PROJECTILE_RANGE {
                continue;
            }
            shells_in_range += 1;
            nearest.push(RankedProjectile {
                distance_squared,
                sample,
            });
            if nearest.len() > MAX_PROJECTILES {
                nearest.pop();
            }
        }
        Some(ProjectileDiagnostics {
            version: 1,
            tick: self.world.tick,
            actor: pilot.owner,
            vehicle: pilot.vehicle,
            ship_form: ship.form,
            observer,
            observer_radius,
            range: PROJECTILE_RANGE,
            capacity: MAX_PROJECTILES,
            debris_scanned: self.world.debris.len(),
            unavailable_shells,
            shells_in_range,
            projectiles: nearest
                .into_sorted_vec()
                .into_iter()
                .map(|v| v.sample)
                .collect(),
        })
    }
}

#[cfg(test)]
#[path = "projectile_diagnostics_tests.rs"]
mod tests;
