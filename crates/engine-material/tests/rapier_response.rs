//! Isolated-contact experiment, not the #327 adapter. No gravity, friction,
//! initial penetration, or simultaneous independent contacts: measure one
//! incoming normal mode, set its restitution, and check actual motion/energy.

use engine_core::Vec2;
use engine_material::{
    ContactBody, ContactGeometry, HULL, Impact, Material, Outcome, ROCK, Region, Section,
    effective_mass, impact, lever,
};
use rapier2d::prelude::*;
use std::sync::Mutex;

struct ContactLaw {
    material: Material,
    volume: f32,
    outcome: Mutex<Option<Outcome>>,
}

impl PhysicsHooks for ContactLaw {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        if context.solver_contacts.is_empty() {
            return;
        }
        let a = &context.bodies[context.rigid_body1.unwrap()];
        let b = &context.bodies[context.rigid_body2.unwrap()];
        let normal = *context.normal;
        let point = context.solver_contacts[0].point;
        let mut recorded = self.outcome.lock().unwrap();
        if recorded.is_none() {
            let speed = (a.velocity_at_point(point) - b.velocity_at_point(point)).dot(normal);
            if speed <= 0.0 {
                return;
            }
            let inertia = |body: &RigidBody| {
                if body.is_dynamic() {
                    let offset = point - body.center_of_mass();
                    ContactBody::rotating(
                        body.mass(),
                        body.mass_properties().local_mprops.principal_inertia(),
                        lever(Vec2::new(offset.x, offset.y), Vec2::new(normal.x, normal.y)),
                    )
                } else {
                    ContactBody::IMMOVABLE
                }
            };
            let regions = [a, b].map(|body| {
                if body.user_data == 1 {
                    Region::new(self.material, self.volume).unwrap()
                } else {
                    let volume = if body.is_dynamic() {
                        body.mass() / ROCK.density
                    } else {
                        1000.0
                    };
                    Region::new(ROCK, volume).unwrap()
                }
            });
            let mut outcome = impact(
                &regions[0],
                &regions[1],
                ContactGeometry::new(0.5, Section::new(1.0, 0.1), Section::new(1.0, 0.1)),
                Impact {
                    closing_speed: speed,
                    effective_mass: effective_mass(inertia(a), inertia(b)).unwrap(),
                },
            )
            .unwrap();
            // Explicit fixture policy: block the impact and dissipate energy
            // beyond capacity. No unimplemented pass-through/fragments claim.
            outcome.ledger.resolve_excess(0.0).unwrap();
            *recorded = Some(outcome);
        }
        let coefficient = recorded.as_ref().unwrap().ledger.restitution().unwrap();
        for contact in context.solver_contacts.iter_mut() {
            contact.restitution = coefficient;
        }
    }
}

fn energy(body: &RigidBody) -> f32 {
    if body.is_dynamic() {
        0.5 * (body.mass() * body.linvel().length_squared()
            + body.mass_properties().local_mprops.principal_inertia() * body.angvel().powi(2))
    } else {
        0.0
    }
}

fn angular_momentum(body: &RigidBody) -> f32 {
    let p = body.center_of_mass();
    let momentum = body.linvel() * body.mass();
    p.x * momentum.y - p.y * momentum.x
        + body.mass_properties().local_mprops.principal_inertia() * body.angvel()
}

fn run(
    speed: f32,
    volume: f32,
    dynamic_support: bool,
    shift: f32,
    offset: f32,
    substeps: usize,
) -> Outcome {
    let mut world = PhysicsWorld {
        gravity: Vector::ZERO,
        integration_parameters: IntegrationParameters {
            dt: 1.0 / 240.0,
            max_ccd_substeps: substeps,
            ..Default::default()
        },
        ..Default::default()
    };
    let support = if dynamic_support {
        RigidBodyBuilder::dynamic().linvel(Vector::new(shift, 0.0))
    } else {
        RigidBodyBuilder::fixed()
    };
    let (wall, _) = world.insert(
        support.translation(Vector::new(1.5, 0.0)).can_sleep(false),
        ColliderBuilder::cuboid(0.5, 5.0)
            .mass(200.0)
            .friction(0.0)
            .restitution(1.0),
    );
    let (striker, _) = world.insert(
        RigidBodyBuilder::dynamic()
            .translation(Vector::new(0.0, offset))
            .linvel(Vector::new(speed + shift, 0.0))
            .ccd_enabled(true)
            .can_sleep(false)
            .user_data(1),
        ColliderBuilder::ball(0.5)
            .mass(20.0)
            .friction(0.0)
            .restitution(1.0)
            .active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS),
    );
    // Commit mass properties without spending any motion before measurement.
    for handle in [striker, wall] {
        world.bodies[handle].recompute_mass_properties_from_colliders(&world.colliders);
    }
    let before = energy(&world.bodies[striker]) + energy(&world.bodies[wall]);
    let momentum_before = world.bodies[striker].linvel() * world.bodies[striker].mass()
        + world.bodies[wall].linvel() * world.bodies[wall].mass();
    let angular_before =
        angular_momentum(&world.bodies[striker]) + angular_momentum(&world.bodies[wall]);
    let law = ContactLaw {
        material: HULL,
        volume,
        outcome: Mutex::new(None),
    };
    let mut steps_after_contact = 0;
    for _ in 0..10000 {
        world.step_with_events(&law, &());
        if law.outcome.lock().unwrap().is_some() {
            steps_after_contact += 1;
            if steps_after_contact == 5 {
                break;
            }
        }
    }
    let outcome = law
        .outcome
        .into_inner()
        .unwrap()
        .expect("an actual solver contact");
    let after = energy(&world.bodies[striker]) + energy(&world.bodies[wall]);
    let ledger = outcome.ledger;
    let lost = ledger
        .dissipated
        .iter()
        .chain(&ledger.irreversible)
        .sum::<f32>()
        + ledger.failure_dissipated;
    let tolerance = 2.0e-3 * before.max(1.0e-4);
    assert!(
        (before - after - lost).abs() <= tolerance,
        "speed={speed} dynamic={dynamic_support} offset={offset} ccd={substeps}: before={before}, after={after}, lost={lost}, {ledger:?}"
    );
    if dynamic_support {
        let momentum_after = world.bodies[striker].linvel() * world.bodies[striker].mass()
            + world.bodies[wall].linvel() * world.bodies[wall].mass();
        assert!(
            (momentum_after - momentum_before).length()
                < 1.0e-4 * momentum_before.length().max(1.0)
        );
        let angular_after =
            angular_momentum(&world.bodies[striker]) + angular_momentum(&world.bodies[wall]);
        assert!((angular_after - angular_before).abs() < 2.0e-3 * angular_before.abs().max(1.0));
        if offset != 0.0 {
            assert!(world.bodies[wall].angvel().abs() > 0.01);
        }
    }
    outcome
}

#[test]
fn actual_solver_closes_safe_damaging_and_exhausting_contacts() {
    for dynamic in [false, true] {
        for substeps in [1, 4] {
            let safe = run(0.1, 6.0, dynamic, 0.0, 0.0, substeps);
            assert!(safe.safe());
            assert!(safe.ledger.restitution().unwrap() < 0.2);
            let dent = run(20.0, 6.0, dynamic, 0.0, 0.0, substeps);
            assert!(dent.consumed.iter().sum::<f32>() > 0.0 && dent.failed == [false; 2]);
            let failed = run(200.0, 0.01, dynamic, 0.0, 0.0, substeps);
            assert!(failed.failed.iter().any(|failed| *failed));
            assert!(failed.ledger.failure_dissipated > 0.0);
        }
    }
}

#[test]
fn dynamic_support_recoil_and_angular_energy_survive_a_common_velocity_shift() {
    let baseline = run(20.0, 6.0, true, 0.0, 2.0, 4);
    let shifted = run(20.0, 6.0, true, 7.0, 2.0, 4);
    for side in 0..2 {
        assert!((baseline.consumed[side] - shifted.consumed[side]).abs() < 1.0e-3);
    }
}
