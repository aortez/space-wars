use super::*;
use engine_material::{SustainedLoad, sustained};

#[test]
fn gravity_loading_free_flight_exposes_callback_order_and_discrete_work() {
    for n in [1, 2, 4, 8] {
        for hz in [30.0, 60.0, 120.0, 240.0] {
            for parts in [&[1, 1][..], &[4, 2]] {
                for copies in [1, 2] {
                    for reverse in [false, true] {
                        let (mut world, tokens) = fixture(parts, copies, reverse, 2.0, 0.0, 10.0);
                        world.raw.integration_parameters.num_solver_iterations = n;
                        let capacity = balances(&world, &tokens);
                        let mut position = -2.0;
                        let mut speed = 0.0;
                        let mut elapsed = 0.0;
                        let mut defect = 0.0;
                        loop {
                            let remaining = (0.25 - elapsed) as f32;
                            let dt = (1.0_f32 / hz).min(remaining);
                            let start = world.motion(body(1)).unwrap();
                            let sample = replayed(&mut world, dt, true);
                            // The material hook observes the incoming velocity,
                            // before gravity and before ordinary contact solving.
                            assert_eq!(sample.before_material, start.linear_velocity.x);
                            assert_eq!(sample.after_material, sample.before_material);
                            assert_eq!(sample.points, 0);
                            assert!(sample.material.is_empty());
                            near(sample.impulse, 0.0, 0.0);
                            (position, speed) = predicted(position, speed, 10.0, f64::from(dt), n);
                            defect +=
                                0.5 * 20.0 * 100.0 * (f64::from(dt) * f64::from(dt)) / n as f64;
                            elapsed += f64::from(dt);
                            if dt == remaining {
                                break;
                            }
                        }
                        let motion = world.motion(body(1)).unwrap();
                        near(f64::from(motion.position.x), position, 8.0e-6);
                        near(f64::from(motion.linear_velocity.x), speed, 8.0e-6);
                        let gravitational_work = 200.0 * (f64::from(motion.position.x) + 2.0);
                        let kinetic = 10.0
                            * (f64::from(motion.linear_velocity.x)
                                * f64::from(motion.linear_velocity.x));
                        // A real discrepancy, not material damping or f32 noise.
                        near(gravitational_work - kinetic, defect, 0.002);
                        assert!(defect > 1.0e-5 * gravitational_work);
                        assert_eq!(balances(&world, &tokens), capacity);
                        assert!(world.loading.as_ref().unwrap().episodes.is_empty());
                    }
                }
            }
        }
    }
}

#[test]
fn gravity_loading_arrival_time_and_incoming_energy_depend_on_subdivision() {
    for n in [1, 2, 4, 8] {
        for parts in [&[1, 1][..], &[4, 2]] {
            let gap = 0.02_f32;
            let gravity = 10.0_f32;
            let continuous = libm::sqrt(2.0 * f64::from(gap) / f64::from(gravity)) as f32;
            let discrete =
                libm::sqrt(2.0 * f64::from(gap) / (f64::from(gravity) * (1.0 + 1.0 / n as f64)))
                    as f32;
            let (mut world, tokens) = fixture(parts, 2, true, gap, 0.0, gravity);
            world.raw.integration_parameters.num_solver_iterations = n;
            let mut overshot = world.clone();
            let wrong = replayed(&mut overshot, continuous, true);
            near(f64::from(wrong.position), f64::from(gap) / n as f64, 1.0e-7);
            assert!(wrong.position > 1.0e-5);
            let capacities = balances(&world, &tokens);
            let arrived = replayed(&mut world, discrete, true);
            near(f64::from(arrived.position), 0.0, 1.0e-7);
            assert_eq!(arrived.points, 0);
            assert!(arrived.material.is_empty());
            let work = 200.0 * f64::from(gap);
            let kinetic = 10.0 * (f64::from(arrived.velocity) * f64::from(arrived.velocity));
            near(kinetic / work, n as f64 / (n + 1) as f64, 1.0e-6);
            // Removing speculative points preserves the approach, but does not
            // make it an accounted material landing. Force admission still fails.
            let response = replayed(&mut world, 1.0e-4, true);
            assert!(response.points > 0);
            assert_eq!(response.material.len(), 1);
            assert_eq!(
                response.material[0].result,
                Err(ResponseRejection::ExternalWork)
            );
            assert_eq!(balances(&world, &tokens), capacities);
        }
    }
}

#[test]
fn gravity_loading_rest_force_matches_reported_impulse() {
    for n in [1, 4, 8] {
        for hz in [30.0, 60.0, 120.0, 240.0] {
            for parts in [&[1, 1][..], &[4, 2]] {
                for copies in [1, 2] {
                    for warm in [0.0, 1.0] {
                        let (mut world, tokens) = fixture(parts, copies, true, 0.0, 0.0, 10.0);
                        world.raw.integration_parameters.num_solver_iterations = n;
                        world.raw.integration_parameters.warmstart_coefficient = warm;
                        let capacities = balances(&world, &tokens);
                        let dt = 1.0_f32 / hz;
                        let mut episode = None;
                        for tick in 0..3 {
                            let sample = replayed(&mut world, dt, false);
                            assert!(sample.points > 0);
                            assert_eq!(sample.before_material, sample.after_material);
                            assert!(sample.material.is_empty());
                            let reaction = 20.0
                                * (f64::from(sample.before_material) + 10.0 * f64::from(dt)
                                    - f64::from(sample.velocity));
                            near(reaction / f64::from(dt), 200.0, 0.001);
                            // The previous frame's warm-start cache must not be
                            // counted again as work applied during this frame.
                            near(sample.impulse, reaction, 1.0e-5);
                            if tick == 0 {
                                near(sample.incoming_warm, 0.0, 0.0);
                            } else {
                                assert!(sample.incoming_warm > 0.0);
                                near(sample.impulse / reaction, 1.0, 2.0e-6);
                            }
                            let current = world
                                .loading
                                .as_ref()
                                .unwrap()
                                .episodes
                                .values()
                                .next()
                                .unwrap()
                                .id;
                            assert_eq!(*episode.get_or_insert(current), current);
                            assert_eq!(world.loading.as_ref().unwrap().episodes.len(), 1);
                            for token in &tokens {
                                let stored = &world
                                    .loading
                                    .as_ref()
                                    .unwrap()
                                    .response
                                    .as_ref()
                                    .unwrap()
                                    .materials[token];
                                let outcome = sustained(
                                    &stored.region().unwrap(),
                                    SustainedLoad {
                                        force: (reaction / f64::from(dt)) as f32,
                                        width: 2.0,
                                        section: stored.section(),
                                        duration: dt,
                                        measured_plastic_displacement: Some(0.0),
                                    },
                                )
                                .unwrap();
                                assert_eq!(outcome.consumed, [0.0; 2]);
                            }
                            assert_eq!(balances(&world, &tokens), capacities);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn gravity_loading_solver_position_error_is_not_plastic_deformation() {
    let mut displacements = Vec::new();
    for parts in [&[1, 1][..], &[4, 2]] {
        let (mut world, tokens) = fixture(parts, 2, true, 0.0, 0.0, 10.0);
        let capacities = balances(&world, &tokens);
        for _ in 0..4 {
            let sample = replayed(&mut world, 1.0 / 60.0, false);
            assert!(sample.velocity.abs() < 1.0e-8);
            assert!(sample.material.is_empty());
        }
        displacements.push(world.motion(body(1)).unwrap().position.x);
        assert_eq!(balances(&world, &tokens), capacities);
    }
    // Same physical support, different redundant constraints: biased solver
    // positions move even though unbiased final velocities nearly vanish.
    assert!(displacements[0] > 1.0e-5);
    assert!(displacements[1].abs() < 1.0e-10);
}

#[test]
fn gravity_loading_static_overload_needs_a_deformation_work_source() {
    let (mut world, tokens) = fixture(&[4, 2], 2, true, 0.0, 0.0, 1000.0);
    let capacities = balances(&world, &tokens);
    let sample = replayed(&mut world, 1.0 / 60.0, false);
    let reaction = 20.0
        * (f64::from(sample.before_material) + 1000.0 * f64::from(sample.duration)
            - f64::from(sample.velocity));
    near(reaction / f64::from(sample.duration), 20000.0, 0.01);
    let stored = &world
        .loading
        .as_ref()
        .unwrap()
        .response
        .as_ref()
        .unwrap()
        .materials[&tokens[1]];
    let load = SustainedLoad {
        force: (reaction / f64::from(sample.duration)) as f32,
        width: 2.0,
        section: stored.section(),
        duration: sample.duration,
        measured_plastic_displacement: None,
    };
    let fallback = sustained(&stored.region().unwrap(), load).unwrap();
    let measured = sustained(
        &stored.region().unwrap(),
        SustainedLoad {
            measured_plastic_displacement: Some(0.0),
            ..load
        },
    )
    .unwrap();
    let available_motion_work = 20000.0 * f64::from(sample.position)
        - 10.0 * (f64::from(sample.velocity) * f64::from(sample.velocity));
    assert!(fallback.yielded[0] && measured.yielded[0]);
    assert!(fallback.consumed[0] > 900.0);
    assert!(available_motion_work.abs() < 1.0e-12);
    assert_eq!(measured.consumed, [0.0; 2]);
    assert_eq!(balances(&world, &tokens), capacities);
    assert!(sample.material.is_empty());
}

#[test]
fn gravity_loading_native_admission_remains_atomic() {
    let (mut world, tokens) = super::super::fixture(&[1, 1], 1, false, 0.0, Vec2::ZERO, 0.0, false);
    world.step_material_flat_arrivals(0.001).unwrap();
    assert!(!world.material_response_batches().is_empty());
    world.set_gravity(Vec2::X * 10.0);
    let before = world.snapshot_bytes().unwrap();
    let loading = world.loading_batches().to_vec();
    let response = world.material_response_batches().to_vec();
    let capacities = balances(&world, &tokens);
    assert_eq!(
        world.step_material_flat_arrivals(1.0 / 60.0).unwrap_err(),
        crate::world::loading::response::flat_arrival::FlatArrivalRejection::UnsupportedScene
    );
    assert!(world.snapshot_bytes().unwrap() == before);
    assert_eq!(world.loading_batches(), loading);
    assert_eq!(world.material_response_batches(), response);
    assert_eq!(balances(&world, &tokens), capacities);
    world.step_material_flat_arrivals(0.0).unwrap();
    assert!(world.snapshot_bytes().unwrap() == before);
    assert!(world.loading_batches().is_empty() && world.material_response_batches().is_empty());
}
