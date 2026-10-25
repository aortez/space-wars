//! Independent driven-platen fixture and downstream public compression contract.
use engine_core::Vec2;
use engine_material::{Region, Section, WOOD};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld, PhysicsWorldConfig, QuarterTurn,
    loading::{CaptureConfig, ContactSource, RegionId, RegionToken},
};

fn body(role: u64) -> BodyId {
    BodyId::new(PhysicsId::new(role + 1), BodyRole::PRIMARY)
}

fn scene(
    parts: u16,
    copies: u16,
    reverse: bool,
    turn: QuarterTurn,
    offset: Vec2,
) -> (PhysicsWorld, RegionToken) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_material_response().unwrap();
    let mut target = None;
    for role in if reverse { [1, 0] } else { [0, 1] } {
        let mut shapes = Vec::new();
        for part in 0..parts * copies {
            let mut shape = ColliderSpec::cuboid(
                ColliderId::new(body(role).entity, ColliderRole::PRIMARY, part),
                0.5,
                1.0 / f32::from(parts),
            );
            shape.local_position.y = -1.0 + f32::from(2 * (part / copies) + 1) / f32::from(parts);
            shape.friction = 0.0;
            shapes.push(shape);
        }
        if reverse {
            shapes.reverse();
        }
        let position = offset + turn.rotate(Vec2::X * role as f32);
        assert!(world.insert_body(
            body(role),
            BodySpec {
                kind: if role == 0 {
                    BodyKind::KinematicVelocity
                } else {
                    BodyKind::Fixed
                },
                position,
                lock_rotation: true,
                can_sleep: false,
                ..Default::default()
            },
            &shapes
        ));
        assert!(world.set_quarter_turn_pose(body(role), position, turn, true));
        let token = world
            .create_loading_region(
                RegionId {
                    entity: body(role).entity,
                    part: 0,
                },
                body(role),
            )
            .unwrap();
        if role == 1 {
            world
                .register_loading_material(
                    token,
                    Region::new(WOOD, 20.0).unwrap(),
                    Section::new(1.0, 1.0),
                )
                .unwrap();
            target = Some(token);
        }
        for shape in shapes {
            world
                .bind_loading_collider(
                    shape.id,
                    ContactSource {
                        region: token,
                        patch: 0,
                    },
                )
                .unwrap();
        }
    }
    (world, target.unwrap())
}

/// Numerical force-displacement integration, independent of the native segment
/// energy calculation. WOOD/full-face constants are written out deliberately.
#[derive(Default)]
struct Reference {
    x: f64,
    permanent: f64,
    work: f64,
    viscous: f64,
}
impl Reference {
    fn advance(&mut self, endpoint: f64, dt: f64) {
        const N: usize = 16384;
        let dx = (endpoint - self.x) / N as f64;
        let speed = (endpoint - self.x) / dt;
        for i in 0..N {
            let middle = self.x + (i as f64 + 0.5) * dx;
            let strain = middle - self.permanent;
            let force = if strain > 0.015 && speed > 0.0 {
                6000.0 + 1500.0 * speed
            } else {
                (400_000.0 * strain).clamp(0.0, 6000.0)
            };
            self.work += force * dx;
            let end = self.x + (i + 1) as f64 * dx;
            if end - self.permanent > 0.015 {
                let crush = end - self.permanent - 0.015;
                self.viscous += 1500.0 * speed * crush;
                self.permanent += crush;
            }
        }
        self.x = endpoint;
    }
    fn elastic(&self) -> f64 {
        200_000.0 * (self.x - self.permanent).clamp(0.0, 0.015).powi(2)
    }
}

fn near(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b} (tolerance {tolerance})"
    );
}

#[test]
fn independent_fixture_measures_driven_motion_and_returned_work() {
    let (mut world, token) = scene(1, 1, false, QuarterTurn::Deg0, Vec2::ZERO);
    let mut reference = Reference::default();
    for (speed, ticks) in [(0.05, 120), (0.0, 30), (-0.05, 120), (0.05, 120)] {
        world.set_velocity(body(0), Vec2::X * speed, 0.0, true);
        for _ in 0..ticks {
            world.step(0.01);
            let motion = world.motion(body(0)).unwrap();
            reference.advance(f64::from(motion.position.x), f64::from(0.01_f32));
        }
    }
    near(reference.x, 0.06, 1e-6);
    near(reference.permanent, 0.045, 1e-6);
    near(reference.elastic(), 45.0, 0.01);
    near(reference.viscous, 3.375, 0.002);
    near(reference.work, 318.375, 0.01);
    // Ordinary rigid stepping does not implement this material coupling.
    assert_eq!(world.loading_material_remaining(token), Some(1200.0));
}

use engine_rapier::world::loading::{
    EpisodeTransition,
    response::compression::{CompressionRejection as Rejection, CompressionStep},
};

fn replay(world: &mut PhysicsWorld, dt: f32, speed: f32) -> CompressionStep {
    let bytes = world.snapshot_bytes().unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let step = world.step_material_compression(dt, speed).unwrap();
    assert_eq!(restored.step_material_compression(dt, speed).unwrap(), step);
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        restored.snapshot_bytes().unwrap()
    );
    step
}

#[test]
fn native_load_hold_unload_reload_matches_independent_work_integral() {
    let (mut world, token) = scene(1, 1, false, QuarterTurn::Deg0, Vec2::ZERO);
    let mut reference = Reference::default();
    let mut owner = None;
    let mut end_count = 0;
    for (speed, ticks) in [(0.05, 120), (0.0, 30), (-0.05, 120), (0.05, 120)] {
        for _ in 0..ticks {
            let step = replay(&mut world, 0.01, speed);
            for event in step.events {
                match event.event.transition {
                    EpisodeTransition::Support => {
                        if let Some(old) = owner {
                            assert_ne!(event.event.id, old);
                        }
                        owner = Some(event.event.id);
                    }
                    EpisodeTransition::End => {
                        assert_eq!(Some(event.event.id), owner);
                        end_count += 1;
                    }
                    EpisodeTransition::Impact => {
                        panic!("quasi-static compression is not an impact")
                    }
                }
            }
            let load = step.load.unwrap();
            let summary = step.summary.unwrap();
            reference.advance(
                f64::from(world.motion(body(0)).unwrap().position.x),
                f64::from(0.01_f32),
            );
            near(summary.driver_work, reference.work, 0.005);
            near(summary.viscous_loss, reference.viscous, 1e-7);
            near(summary.elastic_energy, reference.elastic(), 1e-7);
            near(
                summary.damage_work + summary.charge_error,
                6000.0 * reference.permanent,
                1e-7,
            );
            near(
                summary.driver_work,
                summary.elastic_energy
                    + summary.damage_work
                    + summary.charge_error
                    + summary.viscous_loss,
                1e-8,
            );
            assert_eq!(
                1200.0 - world.loading_material_remaining(token).unwrap(),
                summary.damage_work
            );
            near(
                load.driver_work,
                load.elastic_after - load.elastic_before
                    + load.damage_work
                    + load.charge_error
                    + load.viscous_loss,
                1e-10,
            );
            if speed == 0.0 {
                assert_eq!(load.damage_work, 0.0);
                assert_eq!(load.driver_work, 0.0);
                near(load.force, 6000.0, 1e-8);
                near(load.impulse, load.force * f64::from(0.01_f32), 1e-8);
            }
        }
    }
    assert_eq!(end_count, 1);
    near(
        world.material_compression_summary().unwrap().damage_work,
        270.0,
        0.01,
    );
}

#[test]
fn elastic_support_returns_work_without_damage_and_pause_is_neutral() {
    let (mut world, token) = scene(1, 1, false, QuarterTurn::Deg0, Vec2::ZERO);
    let initial = world.snapshot_bytes().unwrap();
    assert!(
        world
            .step_material_compression(0.0, 0.25)
            .unwrap()
            .summary
            .is_none()
    );
    assert_eq!(world.snapshot_bytes().unwrap(), initial);
    let load = replay(&mut world, 0.5, 0.02).load.unwrap();
    near(load.force, 4000.0, 0.001);
    near(load.driver_work, 20.0, 0.001);
    let before = world.material_compression_summary().unwrap();
    for _ in 0..60 {
        let step = replay(&mut world, 1.0, 0.0);
        assert_eq!(step.load.as_ref().unwrap().driver_work, 0.0);
        assert_eq!(step.load.as_ref().unwrap().damage_work, 0.0);
        assert!(step.events.is_empty());
    }
    let returned = replay(&mut world, 0.5, -0.02).load.unwrap();
    near(returned.driver_work, -before.elastic_energy, 1e-8);
    near(
        world.material_compression_summary().unwrap().elastic_energy,
        0.0,
        1e-8,
    );
    assert_eq!(world.loading_material_remaining(token), Some(1200.0));
    assert_eq!(
        u32::from_le_bytes(world.snapshot_bytes().unwrap()[..4].try_into().unwrap()),
        14
    );
}

#[test]
fn subdivision_geometry_and_frame_changes_preserve_physical_work() {
    let mut baseline = None;
    for (hz, parts, copies, reverse, turn, offset) in [
        (30, 1, 1, false, QuarterTurn::Deg0, Vec2::ZERO),
        (60, 2, 2, true, QuarterTurn::Deg90, Vec2::ZERO),
        (120, 4, 1, false, QuarterTurn::Deg180, Vec2::new(0.25, -0.5)),
        (240, 4, 2, true, QuarterTurn::Deg270, Vec2::new(0.5, 0.25)),
    ] {
        let (mut world, _) = scene(parts, copies, reverse, turn, offset);
        for _ in 0..hz {
            replay(&mut world, 1.0 / hz as f32, 0.0625);
        }
        let summary = world.material_compression_summary().unwrap();
        near(summary.damage_work, 285.0, 0.08);
        near(summary.elastic_energy, 45.0, 1e-7);
        near(summary.viscous_loss, 4.453125, 0.002);
        if let Some(work) = baseline {
            near(summary.driver_work, work, 0.08);
        } else {
            baseline = Some(summary.driver_work);
        }
    }
}

fn refused(world: &mut PhysicsWorld, dt: f32, speed: f32, expected: Rejection) {
    let bytes = world.snapshot_bytes().unwrap();
    let summary = world.material_compression_summary();
    assert_eq!(world.step_material_compression(dt, speed), Err(expected));
    assert_eq!(world.snapshot_bytes().unwrap(), bytes);
    assert_eq!(world.material_compression_summary(), summary);
}

#[test]
fn failures_are_atomic_and_invalidation_survives_restore_and_removal() {
    let (mut world, _) = scene(1, 1, false, QuarterTurn::Deg0, Vec2::ZERO);
    refused(&mut world, f32::NAN, 0.0, Rejection::InvalidCommand);
    refused(&mut world, 1.0, 0.5, Rejection::InvalidCommand);
    replay(&mut world, 1.0, 0.125);
    refused(&mut world, 1.0, 0.125, Rejection::FailedRegion);
    refused(&mut world, 1.0, 0.25, Rejection::Limit);
    let retained = world.material_compression_summary().unwrap();
    let position = world.motion(body(0)).unwrap().position;
    world.set_pose(body(0), position + Vec2::X * 0.01, 0.0, true);
    world.set_pose(body(0), position, 0.0, true);
    refused(&mut world, 0.01, 0.0, Rejection::StaleContinuation);
    assert_eq!(replay(&mut world, 0.0, 0.2).summary, Some(retained.clone()));
    world.remove_entity(body(0).entity);
    world.remove_entity(body(1).entity);
    let mut restored = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    assert_eq!(restored.material_compression_summary(), Some(retained));
    refused(&mut restored, 0.01, 0.0, Rejection::StaleContinuation);
}

#[test]
fn rounded_motion_and_other_step_cannot_silently_reset_history() {
    let (mut world, _) = scene(1, 1, false, QuarterTurn::Deg0, Vec2::new(16.0, 0.0));
    refused(&mut world, 0.001, 1e-5, Rejection::Numerical);
    replay(&mut world, 0.5, 0.125);
    world.step(0.01);
    refused(&mut world, 0.01, 0.0, Rejection::StaleContinuation);
}
