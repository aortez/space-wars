//! Check backend reports against momentum, independently of the material ledger.
use super::*;
use rapier2d::prelude::{FixedJointBuilder, PhysicsHooks, SolverContactBatchContext};

struct AppliedReports;

impl PhysicsHooks for AppliedReports {
    fn report_applied_contact_impulses(&self) -> bool {
        true
    }
}

fn fixture(multibody: bool, speed: f32, reverse: bool) -> (RapierWorld, Vec<RigidBodyHandle>) {
    let mut world = RapierWorld {
        gravity: Vector::new(0.0, -10.0),
        ..Default::default()
    };
    let ground = RigidBodyBuilder::fixed().translation(Vector::new(0.0, -0.5));
    let moving = RigidBodyBuilder::dynamic()
        .translation(Vector::new(0.0, 0.5))
        .linvel(Vector::new(speed, 0.0))
        .can_sleep(false)
        .locked_axes(if multibody {
            LockedAxes::empty()
        } else {
            LockedAxes::ROTATION_LOCKED
        });
    let (ball, ground) = if reverse {
        let ball = world.bodies.insert(moving);
        (ball, world.bodies.insert(ground))
    } else {
        let ground = world.bodies.insert(ground);
        (world.bodies.insert(moving), ground)
    };
    let floor = ColliderBuilder::cuboid(20.0, 0.5).friction(0.5);
    let shape = ColliderBuilder::ball(0.5)
        .mass(if multibody { 10.0 } else { 20.0 })
        .friction(0.5)
        .active_events(ActiveEvents::CONTACT_FORCE_EVENTS);
    if reverse {
        world
            .colliders
            .insert_with_parent(shape, ball, &mut world.bodies);
        world
            .colliders
            .insert_with_parent(floor, ground, &mut world.bodies);
    } else {
        world
            .colliders
            .insert_with_parent(floor, ground, &mut world.bodies);
        world
            .colliders
            .insert_with_parent(shape, ball, &mut world.bodies);
    }
    let mut moving = vec![ball];
    if multibody {
        // A coincident, colliderless mass exercises generic contact constraints.
        // Its joint force is internal; sum both bodies' momentum below.
        let child = world.bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(0.0, 0.5))
                .linvel(Vector::new(speed, 0.0))
                .additional_mass_properties(MassProperties::new(Vector::ZERO, 10.0, 1.25))
                .can_sleep(false),
        );
        let joint = world
            .multibody_joints
            .insert(ball, child, FixedJointBuilder::new(), true)
            .unwrap();
        // Multibodies own generalized velocities independently of body builders.
        world
            .multibody_joints
            .get_mut(joint)
            .unwrap()
            .0
            .generalized_velocity_mut()[0] = speed;
        assert!(world.multibody_joints.rigid_body_link(ball).is_some());
        moving.push(child);
    }
    (world, moving)
}

fn momentum(world: &RapierWorld, moving: &[RigidBodyHandle]) -> [f64; 2] {
    if let Some(link) = world.multibody_joints.rigid_body_link(moving[0]) {
        // The solver writes the final multibody velocity to generalized state;
        // body velocities are refreshed later. Both masses share this free root
        // and center of mass, so its translation gives their total momentum.
        let velocity = world
            .multibody_joints
            .get_multibody(link.multibody)
            .unwrap()
            .generalized_velocity();
        return [20.0 * f64::from(velocity[0]), 20.0 * f64::from(velocity[1])];
    }
    let mass = 20.0 / moving.len() as f64;
    std::array::from_fn(|axis| {
        moving
            .iter()
            .map(|h| mass * f64::from(world.bodies[*h].linvel()[axis]))
            .sum()
    })
}

fn reported(world: &RapierWorld) -> [f64; 2] {
    let mut total = [0.0; 2];
    for pair in world.narrow_phase.contact_pairs() {
        let first = world.colliders[pair.collider1].parent().unwrap();
        let sign = if world.bodies[first].is_dynamic() {
            1.0
        } else {
            -1.0
        };
        for manifold in &pair.manifolds {
            let normal = -manifold.data.normal;
            let tangent = Vector::new(-normal.y, normal.x);
            for point in &manifold.points {
                for axis in 0..2 {
                    total[axis] += sign
                        * (f64::from(normal[axis]) * f64::from(point.data.impulse)
                            + f64::from(tangent[axis]) * f64::from(point.data.tangent_impulse[0]));
                }
            }
        }
    }
    total
}

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 2.0e-4,
        "actual={actual} expected={expected}"
    );
}

#[derive(Default)]
struct Forces(Mutex<Vec<f32>>);

impl EventHandler for Forces {
    fn handle_collision_event(
        &self,
        _: &RigidBodySet,
        _: &ColliderSet,
        _: CollisionEvent,
        _: Option<&ContactPair>,
    ) {
    }

    fn handle_contact_force_event(
        &self,
        _: f32,
        _: &RigidBodySet,
        _: &ColliderSet,
        _: &ContactPair,
        force: f32,
    ) {
        self.0.lock().unwrap().push(force);
    }
}

#[test]
fn contact_impulses_match_momentum_with_friction_and_multibodies() {
    for multibody in [false, true] {
        for n in [1, 4, 8] {
            for warm in [0.0, 0.5, 1.0] {
                for hz in [30.0, 120.0] {
                    for reverse in [false, true] {
                        let (mut world, moving) =
                            fixture(multibody, if reverse { -2.0 } else { 2.0 }, reverse);
                        world.integration_parameters.num_solver_iterations = n;
                        world.integration_parameters.warmstart_coefficient = warm;
                        for (tick, (scale, gravity)) in
                            [(1.0, -10.0), (0.5, -7.0), (2.0, -13.0), (1.0, -10.0)]
                                .into_iter()
                                .enumerate()
                        {
                            let dt = scale / hz;
                            world.integration_parameters.dt = dt;
                            world.gravity.y = gravity;
                            let before = momentum(&world, &moving);
                            let events = Forces::default();
                            world.step_with_events(&AppliedReports, &events);
                            let after = momentum(&world, &moving);
                            let impulse = reported(&world);
                            near(impulse[0], after[0] - before[0]);
                            near(
                                impulse[1],
                                after[1] - before[1] - 20.0 * f64::from(gravity) * f64::from(dt),
                            );
                            let forces = events.0.into_inner().unwrap();
                            assert!(forces.len() <= 1);
                            near(
                                forces.iter().map(|f| f64::from(*f)).sum::<f64>() * f64::from(dt),
                                impulse[1],
                            );
                            if tick == 0 {
                                assert!(impulse[0].abs() > 0.01);
                                let caches: Vec<_> = world
                                    .narrow_phase
                                    .contact_pairs()
                                    .flat_map(|p| &p.manifolds)
                                    .flat_map(|m| &m.points)
                                    .map(|p| p.data)
                                    .collect();
                                assert!(caches.iter().any(|c| c.warmstart_impulse > 0.0));
                                assert!(
                                    caches
                                        .iter()
                                        .any(|c| c.warmstart_tangent_impulse[0].abs() > 0.0)
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn contact_impulses_do_not_trigger_force_events_from_old_cache() {
    for multibody in [false, true] {
        let (mut world, _) = fixture(multibody, 0.0, false);
        for (_, collider) in world.colliders.iter_mut() {
            collider.set_contact_force_event_threshold(225.0);
        }
        for _ in 0..4 {
            let events = Forces::default();
            world.step_with_events(&AppliedReports, &events);
            // Weight is 200. The old four-substep report incorrectly said 250.
            assert!(events.0.into_inner().unwrap().is_empty());
        }
    }
}

#[test]
fn contact_impulses_release_does_not_report_past_support() {
    for multibody in [false, true] {
        for n in [1, 4, 8] {
            for warm in [0.0, 0.5, 1.0] {
                let (mut world, moving) = fixture(multibody, 0.0, false);
                world.integration_parameters.num_solver_iterations = n;
                world.integration_parameters.warmstart_coefficient = warm;
                world.step_with_events(&AppliedReports, &());
                assert!(reported(&world)[1] > 0.0);
                let before = momentum(&world, &moving);
                world.gravity.y = 10.0;
                let events = Forces::default();
                world.step_with_events(&AppliedReports, &events);
                let after = momentum(&world, &moving);
                near(reported(&world)[1], 0.0);
                near(
                    after[1] - before[1],
                    200.0 * f64::from(world.integration_parameters.dt),
                );
                assert!(events.0.into_inner().unwrap().is_empty());
            }
        }
    }
}

#[test]
fn applied_contact_reporting_is_opt_in_and_resets_between_steps() {
    struct ReportMode {
        applied: bool,
        incoming_cache: Mutex<f64>,
    }

    impl PhysicsHooks for ReportMode {
        fn report_applied_contact_impulses(&self) -> bool {
            self.applied
        }

        fn modify_solver_contacts_batch(&self, context: &mut SolverContactBatchContext) {
            // A retained manifold may have lost its solver contacts this step.
            // Only caches admitted to the current solve can enter its report.
            *self.incoming_cache.lock().unwrap() = context
                .contact_pairs()
                .flat_map(|p| &p.manifolds)
                .flat_map(|m| &m.data.solver_contacts)
                .map(|c| f64::from(c.warmstart_impulse))
                .sum();
        }
    }

    assert!(!().report_applied_contact_impulses());
    for multibody in [false, true] {
        for n in [1, 4, 8] {
            for warm in [0.0, 0.5, 1.0] {
                let (mut world, moving) = fixture(multibody, 0.0, false);
                world.integration_parameters.num_solver_iterations = n;
                world.integration_parameters.warmstart_coefficient = warm;
                world.step();
                let mut cached_steps = [0; 2];
                for applied in [
                    false, false, true, true, false, false, true, true, false, false,
                ] {
                    let before = momentum(&world, &moving);
                    let events = Forces::default();
                    let hooks = ReportMode {
                        applied,
                        incoming_cache: Mutex::new(0.0),
                    };
                    world.step_with_events(&hooks, &events);
                    let cached = hooks.incoming_cache.into_inner().unwrap();
                    cached_steps[usize::from(applied)] += usize::from(cached > 0.0);
                    let after = momentum(&world, &moving);
                    let reaction =
                        after[1] - before[1] + 200.0 * f64::from(world.integration_parameters.dt);
                    let expected = reaction + if applied { 0.0 } else { cached };
                    assert!(
                        (reported(&world)[1] - expected).abs() < 2.0e-4,
                        "multibody={multibody} n={n} warm={warm} applied={applied} cached={cached} reaction={reaction} reported={:?}",
                        reported(&world),
                    );
                    near(
                        events
                            .0
                            .into_inner()
                            .unwrap()
                            .into_iter()
                            .map(f64::from)
                            .sum::<f64>()
                            * f64::from(world.integration_parameters.dt),
                        expected,
                    );
                }
                // The rigid fixture guarantees a reused cache in both modes.
                // The free multibody can lose the contact feature between steps;
                // its expected report above uses the actual admitted cache.
                if !multibody {
                    assert!(cached_steps.into_iter().all(|n| n > 0));
                }
            }
        }
    }
}
