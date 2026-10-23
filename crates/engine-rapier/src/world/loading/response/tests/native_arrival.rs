use super::super::arrival::ArrivalRejection;
use super::*;

fn locked_scene(parts: u16, reverse: bool, shift: f32) -> (PhysicsWorld, Vec<RegionToken>) {
    let (mut world, tokens) = scene(parts, reverse, shift, false);
    for n in [1, 2] {
        let h = world.body_handle(body(n)).unwrap();
        world.raw.bodies[h].lock_rotations(true, true);
    }
    (world, tokens)
}

#[test]
fn touching_subthreshold_approaches_require_accounting() {
    for speed in [0.00005, 0.0001] {
        let (mut world, tokens) = locked_scene(1, false, 0.0);
        let h = world.collider_handle(collider(1, 0)).unwrap();
        world.raw.colliders[h].set_mass(1.0e12);
        world.refresh_mass_properties(body(1));
        world.set_velocity(body(1), Vec2::X * speed, 0.0, true);
        let before = world.snapshot_bytes().unwrap();
        let capacities = balances(&world, &tokens);
        let initial = energy(&world, 0.0);
        assert!(initial > 1200.0);
        assert_eq!(
            world.step_material_ball_arrivals(DT).unwrap_err(),
            ArrivalRejection::NumericalArrival
        );
        assert_eq!(world.snapshot_bytes().unwrap(), before);
        assert_eq!(balances(&world, &tokens), capacities);
        assert!(world.material_response_batches().is_empty());
        assert!(world.loading_batches().is_empty());

        // The same physical motion is representable when capture admits it.
        world.loading.as_mut().unwrap().config.min_approach_speed = 0.0;
        world.step_material_ball_arrivals(DT).unwrap();
        assert_ne!(balances(&world, &tokens), capacities);
        assert!((initial - energy(&world, 0.0) - loss(applied(&world))).abs() < 1.0e-5 * initial);
    }
    // Rest and separation need no new impact; a later subthreshold kick into
    // the retained support episode must refuse rather than spend solver work.
    for speed in [0.0, -0.00005] {
        let (mut world, _) = locked_scene(1, false, 0.0);
        world.set_velocity(body(1), Vec2::X * speed, 0.0, true);
        world.step_material_ball_arrivals(DT).unwrap();
        assert_eq!(world.material_response_diagnostics().applied_batches, 0);
        world.set_pose(body(1), Vec2::ZERO, 0.0, true);
        world.set_velocity(body(1), Vec2::X * 0.00005, 0.0, true);
        let before = world.snapshot_bytes().unwrap();
        let outputs = world.loading_batches().to_vec();
        assert_eq!(
            world.step_material_ball_arrivals(DT).unwrap_err(),
            ArrivalRejection::ExistingApproach
        );
        assert_eq!(world.snapshot_bytes().unwrap(), before);
        assert_eq!(world.loading_batches(), outputs);
    }
}

#[test]
fn unrepresentable_rebound_refuses_before_material_commit() {
    for shift in [-1_000_000.0, 1_000_000.0] {
        for parts in [1, 2, 4] {
            let (mut world, tokens) = locked_scene(parts, false, shift);
            world.set_velocity(body(1), Vec2::X * (shift + 0.25), 0.0, true);
            let before = world.snapshot_bytes().unwrap();
            let capacities = balances(&world, &tokens);
            assert_eq!(
                world.step_material_ball_arrivals(DT).unwrap_err(),
                ArrivalRejection::Response(ResponseRejection::Backend)
            );
            assert_eq!(world.snapshot_bytes().unwrap(), before);
            assert_eq!(balances(&world, &tokens), capacities);
            assert!(world.material_response_batches().is_empty());
            assert!(world.loading_batches().is_empty());

            // Ordinary stepping also refuses material accounting, while its
            // documented physics-only fallback remains the caller's choice.
            world.step(DT);
            assert_eq!(
                world.material_response_diagnostics().last_rejection,
                Some(ResponseRejection::Backend)
            );
            assert_eq!(world.material_response_diagnostics().applied_batches, 0);
            assert_eq!(balances(&world, &tokens), capacities);
        }
    }
}

#[test]
fn native_arrivals_preserve_work_capture_time_and_replay() {
    for hz in [30.0, 60.0, 120.0, 240.0] {
        for (speed, gap) in [(0.25, 0.001), (40.0, 0.01), (400.0, 2.0), (120.0, 1.0)] {
            for parts in [1, 2, 4] {
                for shift in [0.0, 17.0] {
                    let (mut world, tokens) = locked_scene(parts, true, shift);
                    world.set_pose(body(1), -Vec2::X * gap, 0.0, true);
                    world.set_velocity(body(1), Vec2::X * (speed + shift), 0.0, true);
                    let mut reference = world.clone();
                    reference.set_pose(body(1), Vec2::ZERO, 0.0, true);
                    reference.step(1.0 / hz);
                    let expected = applied(&reference).clone();
                    let initial = energy(&world, shift);
                    let world_initial = energy(&world, 0.0);
                    let mut records = Vec::new();
                    for _ in 0..8 {
                        let mut replay = world.clone();
                        let step = world.step_material_ball_arrivals(1.0 / hz).unwrap();
                        let other = replay.step_material_ball_arrivals(1.0 / hz).unwrap();
                        assert_eq!(step.flight_seconds, other.flight_seconds);
                        assert!((1..=2).contains(&step.advances.len()));
                        assert_eq!(
                            world.material_response_batches(),
                            replay.material_response_batches()
                        );
                        assert_eq!(world.loading_batches(), replay.loading_batches());
                        assert_eq!(
                            world.snapshot_bytes().unwrap(),
                            replay.snapshot_bytes().unwrap()
                        );
                        assert!(
                            (world
                                .loading_batches()
                                .iter()
                                .map(|b| f64::from(b.duration))
                                .sum::<f64>()
                                - f64::from(1.0 / hz))
                            .abs()
                                < 1.0e-8
                        );
                        for (i, b) in world.loading_batches().iter().enumerate() {
                            assert_eq!(b.substep, i);
                        }
                        records.extend(
                            world
                                .material_response_batches()
                                .iter()
                                .map(|b| b.result.as_ref().unwrap().clone()),
                        );
                    }
                    assert_eq!(records.len(), 1);
                    assert_eq!(records[0].impacts, expected.impacts);
                    assert_eq!(balances(&world, &tokens), balances(&reference, &tokens));
                    let lost = loss(&records[0]);
                    assert!(
                        (initial - energy(&world, shift) - lost).abs() < 1.0e-5 * initial,
                        "hz={hz} speed={speed} gap={gap} parts={parts} shift={shift} initial={initial} final={} lost={lost}",
                        energy(&world, shift)
                    );
                    let tolerance = 1.0e-5 * initial
                        + 8.0 * f64::from(f32::EPSILON) * (world_initial + energy(&world, 0.0));
                    assert!(
                        (world_initial - energy(&world, 0.0) + records[0].prescribed_work - lost)
                            .abs()
                            < tolerance
                    );
                }
            }
        }
    }
}

#[test]
fn native_arrival_refusals_are_atomic_including_staged_failures() {
    for case in 0..20 {
        let (mut world, tokens) = locked_scene(1, false, 0.0);
        world.set_pose(body(1), -Vec2::X * 0.001, 0.0, true);
        let expected = match case {
            0 => {
                world.set_gravity(Vec2::Y);
                ArrivalRejection::UnsupportedScene
            }
            1 => {
                let h = world.body_handle(body(1)).unwrap();
                world.raw.bodies[h].enable_ccd(true);
                ArrivalRejection::UnsupportedScene
            }
            2 => {
                let h = world.body_handle(body(1)).unwrap();
                world.raw.bodies[h].lock_rotations(false, true);
                ArrivalRejection::UnsupportedScene
            }
            3 => {
                let h = world.collider_handle(collider(1, 0)).unwrap();
                world.raw.colliders[h].set_friction(0.5);
                ArrivalRejection::UnsupportedScene
            }
            4 => {
                world.loading.as_mut().unwrap().unbind(collider(2, 0));
                ArrivalRejection::UnsupportedScene
            }
            5 => {
                world.insert_body(
                    body(3),
                    BodySpec::default(),
                    &[ColliderSpec::ball(collider(3, 0), 0.5)],
                );
                ArrivalRejection::UnsupportedScene
            }
            6 => {
                world.loading.as_mut().unwrap().config.max_batches_per_step = 1;
                ArrivalRejection::Response(ResponseRejection::Capture(CaptureRejection::StepLimit))
            }
            7 => {
                world.loading.as_mut().unwrap().config.max_records_per_step = 1;
                ArrivalRejection::Response(ResponseRejection::Capture(CaptureRejection::StepLimit))
            }
            8 => {
                world.set_pose(body(1), Vec2::X * 0.1, 0.0, true);
                ArrivalRejection::NumericalArrival
            }
            9 => {
                world.loading.as_mut().unwrap().response = None;
                ArrivalRejection::ResponseDisabled
            }
            10 => {
                let h = world.collider_handle(collider(1, 0)).unwrap();
                world.raw.colliders[h].set_collision_groups(CollisionGroups::NONE.to_rapier());
                ArrivalRejection::UnsupportedScene
            }
            11 => {
                let h = world.collider_handle(collider(1, 0)).unwrap();
                world.raw.colliders[h].set_solver_groups(CollisionGroups::NONE.to_rapier());
                ArrivalRejection::UnsupportedScene
            }
            12 => {
                let h = world.collider_handle(collider(1, 0)).unwrap();
                world.raw.colliders[h].set_sensor(true);
                ArrivalRejection::UnsupportedScene
            }
            13 => {
                let h = world.collider_handle(collider(1, 0)).unwrap();
                world.raw.colliders[h].set_enabled(false);
                ArrivalRejection::UnsupportedScene
            }
            14 => {
                let h = world.collider_handle(collider(1, 0)).unwrap();
                world.raw.colliders[h].set_translation_wrt_parent(Vector::Y * 0.1);
                ArrivalRejection::UnsupportedScene
            }
            15 => {
                let h = world.body_handle(body(1)).unwrap();
                world.raw.bodies[h].add_force(Vector::X, true);
                ArrivalRejection::UnsupportedScene
            }
            16 => {
                let h = world.body_handle(body(1)).unwrap();
                world.raw.bodies[h].set_locked_axes(
                    LockedAxes::ROTATION_LOCKED | LockedAxes::TRANSLATION_LOCKED_X,
                    true,
                );
                ArrivalRejection::UnsupportedScene
            }
            17 => {
                let h = world.body_handle(body(1)).unwrap();
                world.raw.bodies[h]
                    .activation_mut()
                    .normalized_linear_threshold = 0.4;
                ArrivalRejection::UnsupportedScene
            }
            18 => {
                let h = world.collider_handle(collider(1, 0)).unwrap();
                world.raw.colliders[h].set_shape(SharedShape::cuboid(0.5, 0.5));
                ArrivalRejection::UnsupportedScene
            }
            _ => {
                world.set_pose(body(2), Vec2::new(1.5, 1.0), 0.0, true);
                ArrivalRejection::NumericalArrival
            }
        };
        let snapshot = world.snapshot_bytes().unwrap();
        let before = balances_optional(&world, &tokens);
        let outputs = world.loading_batches().to_vec();
        let result = world.step_material_ball_arrivals(DT);
        assert_eq!(result.unwrap_err(), expected, "case {case}");
        assert_eq!(snapshot, world.snapshot_bytes().unwrap(), "case {case}");
        assert_eq!(before, balances_optional(&world, &tokens));
        assert_eq!(outputs, world.loading_batches());
    }
}

fn balances_optional(world: &PhysicsWorld, tokens: &[RegionToken]) -> Vec<Option<f64>> {
    tokens
        .iter()
        .map(|t| world.loading_material_remaining(*t))
        .collect()
}

#[test]
fn duplicate_impulses_round_velocity_once_on_a_moving_support() {
    let mut expected = None;
    for parts in [1, 2, 4] {
        let (mut world, _) = locked_scene(parts, false, 17.0);
        world.set_pose(body(1), -Vec2::X * 0.001, 0.0, true);
        world.set_velocity(body(1), Vec2::X * 17.25, 0.0, true);
        world.step_material_ball_arrivals(DT).unwrap();
        let velocity = world.motion(body(1)).unwrap().linear_velocity;
        if let Some(expected) = expected {
            assert_eq!(velocity, expected);
        } else {
            expected = Some(velocity);
        }
    }
}

#[test]
fn native_oblique_dynamic_arrivals_and_misses() {
    for dynamic in [false, true] {
        for height in [0.6, 1.001, 2.0] {
            let (mut world, tokens) = locked_scene(1, false, 17.0);
            world.set_pose(body(2), Vec2::new(1.5, height), 0.0, true);
            world.set_velocity(body(1), Vec2::X * 417.0, 0.0, true);
            if dynamic {
                let h = world.body_handle(body(2)).unwrap();
                world.raw.bodies[h].set_body_type(RigidBodyType::Dynamic, true);
                world.refresh_mass_properties(body(2));
            }
            let before = balances(&world, &tokens);
            let initial = energy(&world, 17.0);
            let mut lost = 0.0;
            let mut applied_count = 0;
            for _ in 0..8 {
                world.step_material_ball_arrivals(DT).unwrap();
                for batch in world.material_response_batches() {
                    lost += loss(batch.result.as_ref().unwrap());
                    applied_count += 1;
                }
            }
            assert_eq!(applied_count, if height < 1.0 { 1 } else { 0 });
            assert!((initial - energy(&world, 17.0) - lost).abs() < 1.0e-5 * initial);
            if height > 1.0 {
                assert_eq!(before, balances(&world, &tokens));
            }
        }
    }
}

#[test]
fn native_arrival_episode_lifetime_pause_and_reimpact() {
    let (mut world, tokens) = locked_scene(1, false, 0.0);
    world.set_pose(body(1), -Vec2::X * 0.001, 0.0, true);
    world.step_material_ball_arrivals(DT).unwrap();
    let first = applied(&world).roots[0];
    let old_outputs = world.material_response_batches().to_vec();
    let before = world.snapshot_bytes().unwrap();
    assert_eq!(
        world.step_material_ball_arrivals(f32::NAN).unwrap_err(),
        ArrivalRejection::InvalidTimestep
    );
    assert_eq!(before, world.snapshot_bytes().unwrap());
    assert_eq!(old_outputs, world.material_response_batches());
    // A new kick without sufficient separation cannot reuse the spent episode.
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 40.0, 0.0, true);
    let before = world.snapshot_bytes().unwrap();
    assert_eq!(
        world.step_material_ball_arrivals(DT).unwrap_err(),
        ArrivalRejection::ExistingApproach
    );
    assert_eq!(before, world.snapshot_bytes().unwrap());
    assert_eq!(old_outputs, world.material_response_batches());
    world.set_pose(body(1), -Vec2::X * 2.0, 0.0, true);
    world.set_velocity(body(1), Vec2::ZERO, 0.0, true);
    let mut ends = 0;
    for _ in 0..4 {
        world.step_material_ball_arrivals(DT).unwrap();
        ends += world
            .loading_batches()
            .iter()
            .flat_map(|b| &b.events)
            .filter(|e| e.transition == EpisodeTransition::End)
            .count();
    }
    assert_eq!(ends, 1);
    world.set_pose(body(1), -Vec2::X * 0.001, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 40.0, 0.0, true);
    world.step_material_ball_arrivals(DT).unwrap();
    assert_ne!(applied(&world).roots[0], first);
    let balances_before = balances(&world, &tokens);
    world.step_material_ball_arrivals(0.0).unwrap();
    assert!(world.loading_batches().is_empty());
    assert!(world.material_response_batches().is_empty());
    assert_eq!(balances_before, balances(&world, &tokens));
}
