//! Seat-independent tanks in the canonical Spacewars physics world.
//!
//! The host owns stepping and gravity. Material admission uses completed-world
//! queries; raw assembly admission validates geometry only. No tank stores a
//! remembered planet or performs frame transport.

use std::collections::BTreeMap;

use engine_core::Vec2;
use engine_rapier::{
    rover::{RoverAssembly, RoverControl, RoverSnapshot, RoverSpawnPose, RoverSpec},
    world::{BodyId, PhysicsId, PhysicsWorld, RayCastOptions, SurfaceContact},
};

pub const MAX_TANKS: usize = 16;
const CONTACT_BUDGET: usize = 128;
const SPATIAL_BUDGET: usize = 8192;
const ADMISSION_BUDGET: usize = 8192;
pub(super) const ENTITY_BASE: u64 = 1 << 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TankId(u64);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TankCommand {
    pub drive: f32,
    pub brake: bool,
    /// Aim relative to the chassis, in radians.
    pub turret_angle: f32,
}

#[derive(Debug, Clone)]
struct Tank {
    assembly: RoverAssembly,
    command: TankCommand,
    blocked: bool,
    gravity: [Vec2; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct TankSnapshot {
    pub id: TankId,
    pub command: TankCommand,
    pub assembly: RoverSnapshot,
    pub mount_position: Vec2,
    pub mount_velocity: Vec2,
    pub muzzle_position: Vec2,
    pub muzzle_direction: Vec2,
    pub muzzle_velocity: Vec2,
    pub mount_angular_velocity: f32,
    pub drive_blocked: bool,
    pub local_gravity: Vec2,
    pub body_gravity: [Vec2; 3],
    pub support: Option<SurfaceContact>,
}

/// Counts of explicit tank control work in the most recent host physics step.
/// Snapshot/presentation queries are outside this control budget.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TankWork {
    pub queries: usize,
    pub contacts: usize,
    pub spatial_traversal: usize,
    pub spatial_exhaustions: usize,
    /// Underlying pairs, manifolds and contacts visited before filtering.
    pub contact_traversal: usize,
}

/// Most recent placement attempt, including rejected admission.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TankAdmissionWork {
    pub traversal: usize,
    pub exhausted: bool,
    pub capacity_rejected: bool,
}

/// Measured host integration counts, separate from support-query work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TankStepAudit {
    pub physics_steps: u64,
    pub gravity_applications: usize,
    pub gravity_bodies: usize,
    pub duplicate_gravity_applications: usize,
}

/// One canonical assembly per actor. Keep this collection paired with its host world.
/// Removal and clearing never rewind identity allocation, so stale commands cannot
/// address a replacement actor. Clone only together with the host physics world.
#[derive(Debug, Clone, Default)]
pub struct Tanks {
    actors: BTreeMap<TankId, Tank>,
    next_id: u64,
    work: TankWork,
    admission: TankAdmissionWork,
    audit: TankStepAudit,
    gravity_counts: BTreeMap<BodyId, usize>,
}

impl Tanks {
    pub(crate) fn begin_gravity_step(&mut self) {
        self.gravity_counts.clear();
        self.audit.gravity_applications = 0;
        self.audit.gravity_bodies = 0;
        self.audit.duplicate_gravity_applications = 0;
    }

    pub(crate) fn record_physics_step(&mut self) {
        self.audit.physics_steps += 1;
    }

    pub(crate) fn record_gravity(&mut self, body: BodyId, acceleration: Vec2) {
        let count = self.gravity_counts.entry(body).or_default();
        *count += 1;
        self.audit.gravity_applications += 1;
        self.audit.duplicate_gravity_applications += usize::from(*count > 1);
        self.audit.gravity_bodies = self.gravity_counts.len();
        for tank in self.actors.values_mut() {
            if let Some(index) = tank
                .assembly
                .bodies()
                .iter()
                .position(|candidate| *candidate == body)
            {
                tank.gravity[index] = acceleration;
                break;
            }
        }
    }

    pub(crate) fn bodies(&self) -> Vec<BodyId> {
        self.actors
            .values()
            .flat_map(|tank| tank.assembly.bodies())
            .collect()
    }

    pub fn ids(&self) -> impl Iterator<Item = TankId> + '_ {
        self.actors.keys().copied()
    }

    pub fn len(&self) -> usize {
        self.actors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actors.is_empty()
    }

    pub fn insert(
        &mut self,
        physics: &mut PhysicsWorld,
        pose: RoverSpawnPose,
        spec: RoverSpec,
    ) -> Option<TankId> {
        if self.len() >= MAX_TANKS {
            return None;
        }
        let next = self.next_id.checked_add(1)?;
        let entity = PhysicsId::new(ENTITY_BASE.checked_add(next)?);
        let assembly = RoverAssembly::insert_at(physics, entity, pose, spec)?;
        let id = TankId(next);
        self.next_id = next;
        self.actors.insert(
            id,
            Tank {
                assembly,
                command: TankCommand::default(),
                blocked: false,
                gravity: [Vec2::ZERO; 3],
            },
        );
        Some(id)
    }

    /// Apply source-independent motor intent; no simulation step or gravity is applied.
    pub fn command(
        &mut self,
        physics: &mut PhysicsWorld,
        id: TankId,
        command: TankCommand,
    ) -> bool {
        if !command.drive.is_finite() || !command.turret_angle.is_finite() {
            return false;
        }
        let Some(tank) = self.actors.get_mut(&id) else {
            return false;
        };
        let command = TankCommand {
            drive: command.drive.clamp(-1.0, 1.0),
            ..command
        };
        if !tank.assembly.set_control(
            physics,
            RoverControl {
                throttle: command.drive,
                brake: command.brake,
            },
        ) {
            return false;
        }
        tank.command = command;
        true
    }

    pub fn snapshot(&self, physics: &PhysicsWorld, id: TankId) -> Option<TankSnapshot> {
        let tank = self.actors.get(&id)?;
        let assembly = tank.assembly.snapshot_bounded(physics, CONTACT_BUDGET)?;
        let mount_position =
            assembly.chassis.position + Vec2::new(0.0, 0.8).rotate_radians(assembly.chassis.angle);
        let muzzle_direction =
            Vec2::from_radians(assembly.chassis.angle + tank.command.turret_angle);
        let remaining = std::cell::Cell::new(CONTACT_BUDGET);
        Some(TankSnapshot {
            id,
            command: tank.command,
            drive_blocked: tank.blocked,
            local_gravity: tank.gravity[0],
            body_gravity: tank.gravity,
            support: tank
                .assembly
                .wheel_colliders()
                .into_iter()
                .flat_map(|c| physics.surface_contacts_bounded(c, &remaining))
                .find(|c| {
                    c.normal.dot(Vec2::Y.rotate_radians(assembly.chassis.angle)) > 0.7
                        && c.separation <= 0.05
                }),
            mount_position,
            mount_velocity: physics.velocity_at_point(tank.assembly.bodies()[0], mount_position)?,
            muzzle_position: mount_position + muzzle_direction * 2.5,
            muzzle_velocity: physics.velocity_at_point(
                tank.assembly.bodies()[0],
                mount_position + muzzle_direction * 2.5,
            )?,
            mount_angular_velocity: assembly.chassis.angular_velocity,
            muzzle_direction,
            assembly,
        })
    }

    /// Bounded lookahead in the current contact frame. Refusal only changes motor
    /// intent: no pose correction, teleport, or artificial ground is introduced.
    pub(crate) fn prepare_controls(&mut self, physics: &mut PhysicsWorld, dirty: bool) {
        self.work = TankWork::default();
        for tank in self.actors.values_mut() {
            let Some(chassis) = physics.motion(tank.assembly.bodies()[0]) else {
                continue;
            };
            let up = Vec2::Y.rotate_radians(chassis.angle);
            let remaining = std::cell::Cell::new(CONTACT_BUDGET);
            let supports: Vec<_> = tank
                .assembly
                .wheel_colliders()
                .into_iter()
                .flat_map(|c| physics.surface_contacts_bounded(c, &remaining))
                .inspect(|_| self.work.contacts += 1)
                .filter(|c| c.normal.dot(up) > 0.7 && c.separation <= 0.05)
                .collect();
            // A wheel on loose grains has several different point normals.
            // Their actual contact frame avoids extrapolating one grain's flank
            // across the whole route. Apply the gravity slope limit to this frame.
            let support = supports.first().copied().map(|mut support| {
                let normal = supports.iter().fold(Vec2::ZERO, |sum, c| sum + c.normal);
                support.normal = normal.normalized();
                let right = Vec2::new(up.y, -up.x);
                let left = supports
                    .iter()
                    .min_by(|a, b| a.position.dot(right).total_cmp(&b.position.dot(right)))
                    .unwrap();
                let rightmost = supports
                    .iter()
                    .max_by(|a, b| a.position.dot(right).total_cmp(&b.position.dot(right)))
                    .unwrap();
                let span = rightmost.position - left.position;
                if span.dot(right) > 2.0 {
                    let frame_normal = Vec2::new(-span.y, span.x).normalized();
                    if frame_normal.dot(support.normal) > 0.85 {
                        support.normal = frame_normal;
                    }
                }
                support.position = supports.iter().fold(Vec2::ZERO, |sum, c| sum + c.position)
                    / supports.len() as f32;
                support
            });
            self.work.contact_traversal += CONTACT_BUDGET - remaining.get();
            let safe = !dirty
                && remaining.get() > 0
                && support.is_some_and(|support| {
                    if support.normal.dot(-tank.gravity[0].normalized()) < 0.7 {
                        return false;
                    }
                    let tangent = Vec2::new(support.normal.y, -support.normal.x);
                    let relative = chassis.linear_velocity - support.velocity;
                    let ahead = tangent
                        * tank.command.drive.signum()
                        * (3.5 + relative.dot(tangent).abs() * 0.5);
                    let options = RayCastOptions {
                        max_distance: 3.0,
                        collision_groups: super::physics::tank_material_groups(),
                        exclude_entity: Some(tank.assembly.entity()),
                        ..Default::default()
                    };
                    let origin = support.position + ahead + support.normal;

                    // Include the undercarriage above the admitted step height.
                    // A chassis-only envelope misses low solid obstacles that the
                    // wheels can otherwise climb despite the route refusal limit.
                    let spec = super::physics::tank_spec();
                    let chassis_height = (chassis.position - support.position).dot(support.normal);
                    let top = chassis_height + spec.chassis_half_height;
                    let radius = ((top - 0.6) * 0.5).max(0.01);
                    let center = chassis.position
                        + ahead
                        + support.normal * ((top + 0.6) * 0.5 - chassis_height);
                    let mut budget = SPATIAL_BUDGET;
                    let mut normal_sum = Vec2::ZERO;
                    let mut route_safe = true;
                    // Sample a narrow footprint rather than treating one grain
                    // flank as the slope of the entire route. Every sample must
                    // retain material within the step limit and solid clearance.
                    for offset in [-0.125, 0.0, 0.125] {
                        self.work.queries += 2;
                        let result = physics.tank_route_query(
                            origin + tangent * offset,
                            -support.normal,
                            options,
                            center,
                            chassis.angle - std::f32::consts::FRAC_PI_2,
                            (spec.chassis_half_width - radius).max(0.0),
                            radius,
                            spec.collision_groups,
                            &mut budget,
                        );
                        match result {
                            Ok((Some(hit), true))
                                if (hit.point - support.position).dot(support.normal).abs()
                                    <= 0.6 =>
                            {
                                normal_sum += hit.normal;
                            }
                            Err(()) => {
                                self.work.spatial_exhaustions += 1;
                                route_safe = false;
                                break;
                            }
                            _ => {
                                route_safe = false;
                                break;
                            }
                        }
                    }
                    self.work.spatial_traversal += SPATIAL_BUDGET - budget;
                    route_safe && normal_sum.normalized().dot(support.normal) > 0.85
                });
            tank.blocked = tank.command.drive != 0.0 && !safe;
            tank.assembly.set_control(
                physics,
                RoverControl {
                    throttle: if tank.blocked {
                        0.0
                    } else {
                        tank.command.drive
                    },
                    brake: tank.command.brake || tank.blocked,
                },
            );
        }
    }

    pub fn remove(&mut self, physics: &mut PhysicsWorld, id: TankId) -> bool {
        let Some(tank) = self.actors.remove(&id) else {
            return false;
        };
        tank.assembly.remove(physics)
    }

    pub fn clear(&mut self, physics: &mut PhysicsWorld) {
        for (_, tank) in std::mem::take(&mut self.actors) {
            tank.assembly.remove(physics);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::Vec2;
    use engine_rapier::world::PhysicsWorldConfig;

    fn pose(x: f32) -> RoverSpawnPose {
        RoverSpawnPose {
            wheel_center: Vec2::new(x, 0.0),
            up_angle: std::f32::consts::FRAC_PI_2,
        }
    }

    fn host_step(state: &mut super::super::SpacewarsState) {
        use engine_common::Scenario;
        super::super::SpacewarsScenario::step(
            state,
            &[],
            std::time::Duration::from_secs_f32(1.0 / 60.0),
        );
    }

    #[test]
    fn moving_material_drive_and_brake_trace() {
        for mode in [
            None,
            Some(engine_common::DirtMode::Round),
            Some(engine_common::DirtMode::Angular),
        ] {
            let mut state = super::super::SpacewarsScenario::init_terrain_fixture(42);
            state.ships[0].position = Vec2::new(700.0, 700.0);
            state.ships[1].position = Vec2::new(800.0, 500.0);
            if let Some(mode) = mode {
                state
                    .enable_loose_terrain(super::super::surface_sortie::dirt_config(mode))
                    .unwrap();
            }
            host_step(&mut state);
            let a = state.place_tank(Vec2::new(496.0, 565.0), -Vec2::Y).unwrap();
            host_step(&mut state);
            let b = state.place_tank(Vec2::new(435.0, 500.0), Vec2::X).unwrap();
            state.command_tank(
                a,
                TankCommand {
                    brake: true,
                    ..Default::default()
                },
            );
            state.command_tank(
                b,
                TankCommand {
                    brake: true,
                    ..Default::default()
                },
            );
            for _ in 0..90 {
                host_step(&mut state);
            }
            for (phase, command) in [
                (
                    "drive",
                    TankCommand {
                        drive: 1.0,
                        ..Default::default()
                    },
                ),
                (
                    "brake",
                    TankCommand {
                        brake: true,
                        ..Default::default()
                    },
                ),
            ] {
                state.command_tank(a, command);
                let mut speed_sum = [0.0; 2];
                let mut samples = [0; 2];
                for tick in 0..120 {
                    state.planets[0].position += Vec2::X * (0.3 / 60.0);
                    host_step(&mut state);
                    for (i, id) in [a, b].into_iter().enumerate() {
                        let s = state.tank_snapshot(id).unwrap();
                        let speed = s.support.map(|c| {
                            (s.assembly.chassis.linear_velocity - c.velocity)
                                .dot(Vec2::new(c.normal.y, -c.normal.x))
                        });
                        if tick >= 60 {
                            if let Some(speed) = speed {
                                speed_sum[i] += speed;
                                samples[i] += 1;
                            }
                        }
                        if tick % 30 == 0 || tick == 119 {
                            eprintln!(
                                "{mode:?} {phase} {tick} tank={i}: {:?} speed={speed:?} blocked={}",
                                s.assembly.chassis.position, s.drive_blocked
                            );
                        }
                    }
                }
                assert!(
                    samples.into_iter().all(|n| n >= 50),
                    "sustained real material contacts required: {samples:?}"
                );
                let mean = [
                    speed_sum[0] / samples[0] as f32,
                    speed_sum[1] / samples[1] as f32,
                ];
                if phase == "drive" {
                    assert!(mean[0] > 1.0, "successful independent drive: {mean:?}");
                } else {
                    assert!(mean[0].abs() < 0.8, "support-relative brake: {mean:?}");
                }
                assert!(
                    mean[1].abs() < 0.8,
                    "other tank must remain braked: {mean:?}"
                );
            }
        }
    }

    #[test]
    fn material_gap_refusal_keeps_real_support_and_inertia() {
        use engine_terrain::{Brush, EditMode, TerrainEdit};
        let mut state = super::super::SpacewarsScenario::init_terrain_fixture(42);
        state.ships[0].position = Vec2::new(700.0, 700.0);
        host_step(&mut state);
        let id = state.place_tank(Vec2::new(500.0, 565.0), -Vec2::Y).unwrap();
        state.command_tank(
            id,
            TankCommand {
                brake: true,
                ..Default::default()
            },
        );
        for _ in 0..90 {
            host_step(&mut state);
        }
        let before = state.tank_snapshot(id).unwrap();
        let planet = state.planets[0];
        let contact = before.support.expect("supported before planning gap");
        let tangent = Vec2::new(contact.normal.y, -contact.normal.x);
        let local = (contact.position + tangent * 7.0 - planet.position)
            .rotate_radians(-planet.wrapper_angle);
        let center = state
            .planet_terrain(0)
            .unwrap()
            .local_to_cell(local)
            .unwrap();
        state
            .queue_planet_edit(
                0,
                TerrainEdit {
                    brush: Brush::Circle { center, radius: 3 },
                    mode: EditMode::Remove,
                },
            )
            .unwrap();
        state.command_tank(
            id,
            TankCommand {
                drive: 1.0,
                ..Default::default()
            },
        );
        let mut blocked = 0;
        let mut supported = 0;
        for tick in 0..180 {
            host_step(&mut state);
            let s = state.tank_snapshot(id).unwrap();
            blocked += usize::from(s.drive_blocked);
            supported += usize::from(s.support.is_some());
            if tick % 30 == 0 {
                eprintln!(
                    "gap {tick}: {:?} blocked={} support={}",
                    s.assembly.chassis.position,
                    s.drive_blocked,
                    s.support.is_some()
                );
            }
        }
        assert!(blocked > 30, "must refuse unsafe route");
        assert!(supported > 150, "must retain actual ground: {supported}");
        let after = state.tank_snapshot(id).unwrap();
        assert!((after.assembly.chassis.position - state.planets[0].position).length() > 58.0);
    }

    #[test]
    fn host_material_placement_two_bearings_pause_clone_replay_and_cleanup() {
        use engine_common::Scenario;
        let mut state = super::super::SpacewarsScenario::init_terrain_fixture(42);
        // No stale spatial index may authorize placement.
        assert!(
            state
                .place_tank(Vec2::new(500.0, 565.0), -Vec2::Y)
                .is_none()
        );
        host_step(&mut state);
        let a = state
            .place_tank(Vec2::new(500.0, 565.0), -Vec2::Y)
            .expect("top material placement");
        host_step(&mut state);
        let b = state
            .place_tank(Vec2::new(435.0, 500.0), Vec2::X)
            .expect("side material placement");
        host_step(&mut state);
        assert_ne!(a, b);
        assert!(matches!(
            super::super::physics::classify_entity(state.physics.tanks.bodies()[0].entity),
            Some(super::super::physics::MechanicalEntity::Tank(_))
        ));
        let before = state.tank_snapshot(a).unwrap();
        assert!(before.local_gravity.y < 0.0);
        assert!(state.tank_snapshot(b).unwrap().local_gravity.x > 0.0);
        assert!(
            state
                .place_tank(before.assembly.chassis.position + Vec2::Y * 5.0, -Vec2::Y)
                .is_none()
        );
        super::super::SpacewarsScenario::step(&mut state, &[], std::time::Duration::ZERO);
        assert_eq!(state.tank_snapshot(a).unwrap(), before);
        state.command_tank(
            a,
            TankCommand {
                drive: 1.0,
                turret_angle: 0.3,
                ..Default::default()
            },
        );
        state.command_tank(
            b,
            TankCommand {
                brake: true,
                ..Default::default()
            },
        );
        let mut restored = state.clone();
        let bytes = state.physics.world.snapshot_bytes().unwrap();
        restored.physics.world = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(state.tank_snapshot(a), restored.tank_snapshot(a));
        assert_eq!(state.tank_snapshot(b), restored.tank_snapshot(b));
        for _ in 0..90 {
            let steps = state.tank_step_audit().physics_steps;
            host_step(&mut state);
            let audit = state.tank_step_audit();
            assert_eq!(audit.physics_steps, steps + 1);
            assert_eq!(audit.gravity_applications, 6);
            assert_eq!(audit.gravity_bodies, 6);
            assert_eq!(audit.duplicate_gravity_applications, 0);
            assert!(state.tank_work().queries <= 12);
            assert!(state.tank_work().contacts <= 256);
            assert!(state.tank_work().contact_traversal <= 256);
            host_step(&mut restored);
        }
        assert_eq!(state.tank_snapshot(a), restored.tank_snapshot(a));
        assert_eq!(state.tank_snapshot(b), restored.tank_snapshot(b));
        let bodies = state.physics.world.body_count();
        let joints = state.physics.world.joint_count();
        let colliders = state.physics.world.collider_count();
        let entities = state.physics.world.entity_count();
        state.clear_tanks();
        assert_eq!(state.physics.world.joint_count(), joints - 4);
        assert_eq!(state.physics.world.collider_count(), colliders - 6);
        assert_eq!(state.physics.world.entity_count(), entities - 2);
        assert_eq!(state.physics.world.body_count(), bodies - 6);
        assert_eq!(state.tank_ids().count(), 0);
        assert!(state.tank_snapshot(a).is_none());
    }

    #[test]
    fn queued_material_cut_removes_support_and_tank_falls_without_frame_glue() {
        use engine_terrain::{Brush, EditMode, TerrainEdit};
        let mut state = super::super::SpacewarsScenario::init_terrain_fixture(42);
        host_step(&mut state);
        let id = state.place_tank(Vec2::new(500.0, 565.0), -Vec2::Y).unwrap();
        for _ in 0..60 {
            host_step(&mut state);
        }
        let before = state.tank_snapshot(id).unwrap();
        assert!(
            before.support.is_some(),
            "tank must contact material before cut"
        );
        let planet = state.planets[0];
        let local = (before.assembly.chassis.position - planet.position)
            .rotate_radians(-planet.wrapper_angle);
        let center = state
            .planet_terrain(0)
            .unwrap()
            .local_to_cell(local)
            .unwrap();
        state
            .queue_planet_edit(
                0,
                TerrainEdit {
                    brush: Brush::Circle { center, radius: 10 },
                    mode: EditMode::Remove,
                },
            )
            .unwrap();
        host_step(&mut state);
        assert!(state.terrain_removed_cells() > 0);
        let mut lost_support = false;
        for _ in 0..8 {
            host_step(&mut state);
            lost_support |= state.tank_snapshot(id).unwrap().support.is_none();
        }
        let after = state.tank_snapshot(id).unwrap();
        assert!(lost_support);
        let radial = (before.assembly.chassis.position - planet.position).normalized();
        assert!(
            (after.assembly.chassis.position - before.assembly.chassis.position).dot(radial) < -1.0
        );
    }

    #[test]
    fn unsupported_host_tank_retains_inertia_and_receives_gravity_once() {
        let mut state = super::super::SpacewarsScenario::init_terrain_fixture(42);
        host_step(&mut state);
        let id = state.place_tank(Vec2::new(500.0, 565.0), -Vec2::Y).unwrap();
        // Remove the actual supporting material body, retaining the gravity source.
        state
            .physics
            .world
            .remove_entity(super::super::physics::planet_entity(0));
        let before = state.tank_snapshot(id).unwrap().assembly.chassis;
        host_step(&mut state);
        let after = state.tank_snapshot(id).unwrap();
        let dt = 1.0 / 60.0;
        assert!(!after.drive_blocked);
        assert!(after.support.is_none());
        assert!(after.assembly.chassis.position.y < before.position.y);
        // Suspension transfers momentum between bodies. Compare total assembly
        // momentum, allowing only the assembly's small ordinary damping loss.
        let bodies = state.physics.tanks.bodies();
        let mut expected = Vec2::ZERO;
        let mut measured = Vec2::ZERO;
        let mut mass = 0.0;
        for (index, body) in bodies.iter().enumerate() {
            let m = state.physics.world.body_mass(*body).unwrap();
            expected += after.body_gravity[index] * (dt * m);
            measured += state.physics.world.motion(*body).unwrap().linear_velocity * m;
            mass += m;
        }
        assert!(
            ((measured - expected) / mass).length() < 0.02,
            "{measured:?} {expected:?}"
        );
        assert!(after.assembly.chassis.angle.abs() < 0.01);
    }

    #[test]
    fn bounded_admission_and_repeated_cleanup_preserve_stale_identity_safety() {
        let mut physics = PhysicsWorld::new(PhysicsWorldConfig::default());
        let mut tanks = Tanks::default();
        let mut stale = None;
        for _ in 0..3 {
            let ids: Vec<_> = (0..MAX_TANKS)
                .map(|i| {
                    tanks
                        .insert(&mut physics, pose(i as f32 * 10.0), RoverSpec::default())
                        .unwrap()
                })
                .collect();
            assert_eq!(physics.body_count(), MAX_TANKS * 3);
            assert!(
                tanks
                    .insert(&mut physics, pose(1000.0), RoverSpec::default())
                    .is_none()
            );
            if let Some(old) = stale {
                assert!(!tanks.command(&mut physics, old, TankCommand::default()));
                assert!(tanks.snapshot(&physics, old).is_none());
            }
            stale = Some(ids[0]);
            assert!(tanks.remove(&mut physics, ids[0]));
            assert!(!tanks.remove(&mut physics, ids[0]));
            tanks.clear(&mut physics);
            assert!(tanks.is_empty());
            assert_eq!(physics.body_count(), 0);
        }
    }

    #[test]
    fn commands_are_independent_and_invalid_admission_does_not_allocate_bodies() {
        let mut physics = PhysicsWorld::new(PhysicsWorldConfig::default());
        let mut tanks = Tanks::default();
        assert!(
            tanks
                .insert(&mut physics, pose(f32::NAN), RoverSpec::default())
                .is_none()
        );
        assert_eq!(physics.body_count(), 0);
        let a = tanks
            .insert(&mut physics, pose(0.0), RoverSpec::default())
            .unwrap();
        let b = tanks
            .insert(&mut physics, pose(10.0), RoverSpec::default())
            .unwrap();
        let before = tanks.snapshot(&physics, b).unwrap();
        assert!(tanks.command(
            &mut physics,
            a,
            TankCommand {
                drive: 2.0,
                brake: true,
                turret_angle: 0.7
            }
        ));
        assert_eq!(tanks.snapshot(&physics, b).unwrap(), before);
        assert_eq!(tanks.snapshot(&physics, a).unwrap().command.drive, 1.0);
        assert!(!tanks.command(
            &mut physics,
            a,
            TankCommand {
                drive: f32::NAN,
                ..TankCommand::default()
            }
        ));
        assert_eq!(
            tanks.snapshot(&physics, a).unwrap().command.turret_angle,
            0.7
        );
        tanks.clear(&mut physics);
    }
}

impl super::SpacewarsState {
    /// Cast at most 20 world units onto actual material, then validate a conservative
    /// assembly envelope against every reciprocal solid collision group. Queries
    /// share an 8192-unit traversal cap; capacity rejection does no query work.
    /// Admission requires a completed host step and invalidates queries afterward.
    pub fn place_tank(&mut self, origin: Vec2, down: Vec2) -> Option<TankId> {
        self.physics.tanks.admission = TankAdmissionWork::default();
        if self.physics.tanks.len() >= MAX_TANKS {
            self.physics.tanks.admission.capacity_rejected = true;
            return None;
        }
        let mut remaining = ADMISSION_BUDGET;
        let result = self.place_tank_bounded(origin, down, &mut remaining);
        self.physics.tanks.admission.traversal = ADMISSION_BUDGET - remaining;
        self.physics.tanks.admission.exhausted = remaining == 0 && result.is_none();
        result
    }

    pub fn tank_admission_work(&self) -> TankAdmissionWork {
        self.physics.tanks.admission
    }

    fn place_tank_bounded(
        &mut self,
        origin: Vec2,
        down: Vec2,
        remaining: &mut usize,
    ) -> Option<TankId> {
        if self.physics.material_queries_dirty {
            return None;
        }
        let options = RayCastOptions {
            max_distance: 20.0,
            collision_groups: super::physics::tank_material_groups(),
            ..Default::default()
        };
        let (hit, _) = self
            .physics
            .world
            .tank_route_query(
                origin,
                down,
                options,
                origin,
                0.0,
                0.0,
                0.01,
                engine_rapier::world::CollisionGroups::new(0, 0),
                remaining,
            )
            .ok()?;
        let hit = hit?;
        let spec = super::physics::tank_spec();
        let up = hit.normal;
        if up.dot(-down.normalized()) < 0.85 {
            return None;
        }
        let tangent = Vec2::new(up.y, -up.x);
        let wheel_center = hit.point + up * (spec.wheel_radius + 0.08);
        let angle = up.y.atan2(up.x);
        let chassis =
            wheel_center + up * (spec.suspension_rest_length - spec.suspension_anchor_height);
        let world = &self.physics.world;
        let clear_options = RayCastOptions {
            max_distance: 0.0,
            collision_groups: engine_rapier::world::CollisionGroups::new(0, 0),
            ..Default::default()
        };
        let mut clear = |position, rotation, segment, radius| {
            world
                .tank_route_query(
                    origin,
                    down,
                    clear_options,
                    position,
                    rotation,
                    segment,
                    radius,
                    spec.collision_groups,
                    remaining,
                )
                .is_ok_and(|(_, clear)| clear)
        };
        if !clear(
            chassis,
            angle - std::f32::consts::PI,
            spec.chassis_half_width,
            spec.chassis_half_height,
        ) {
            return None;
        }
        for sign in [-1.0, 1.0] {
            let wheel = wheel_center + tangent * (sign * spec.wheel_offset);
            if !clear(wheel, 0.0, 0.0, spec.wheel_radius) {
                return None;
            }
        }
        let id = self.physics.tanks.insert(
            &mut self.physics.world,
            RoverSpawnPose {
                wheel_center,
                up_angle: angle,
            },
            spec,
        )?;
        self.physics.material_queries_dirty = true;
        Some(id)
    }

    pub fn tank_step_audit(&self) -> TankStepAudit {
        self.physics.tanks.audit
    }

    pub fn tank_work(&self) -> TankWork {
        self.physics.tanks.work
    }

    pub fn tank_ids(&self) -> impl Iterator<Item = TankId> + '_ {
        self.physics.tanks.ids()
    }

    pub fn command_tank(&mut self, id: TankId, command: TankCommand) -> bool {
        self.physics
            .tanks
            .command(&mut self.physics.world, id, command)
    }

    pub fn tank_snapshot(&self, id: TankId) -> Option<TankSnapshot> {
        self.physics.tanks.snapshot(&self.physics.world, id)
    }

    pub fn remove_tank(&mut self, id: TankId) -> bool {
        self.physics.tanks.remove(&mut self.physics.world, id)
    }

    pub fn clear_tanks(&mut self) {
        self.physics.tanks.clear(&mut self.physics.world);
    }
}

#[cfg(test)]
#[path = "tanks_physical_tests.rs"]
mod physical_tests;
