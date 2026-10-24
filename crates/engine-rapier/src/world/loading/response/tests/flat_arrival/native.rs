//! Qualify the native operation against the independent fixture and its refusals.
use super::*;
use crate::world::loading::response::flat_arrival::{
    FlatArrivalRejection as R, FlatArrivalStep, FlatEventPhase,
};

fn check_trace(result: &FlatArrivalStep, world: &PhysicsWorld, dt: f32) {
    assert_eq!(result.advances.len(), result.previews.len());
    assert_eq!(result.advances.len(), result.durations.len());
    assert_eq!(result.advances.len(), world.loading_batches().len());
    assert!(result.advances.len() <= MAX_ADVANCES);
    assert!(
        (result.durations.iter().map(|d| f64::from(*d)).sum::<f64>() - f64::from(dt)).abs()
            < 2.0 * f64::from(f32::EPSILON * dt)
    );
    let mut events = result
        .events
        .iter()
        .filter(|e| e.phase == FlatEventPhase::BeforeSolve);
    let mut elapsed = 0.0;
    for (i, batch) in world.loading_batches().iter().enumerate() {
        assert_eq!(batch.substep, i);
        assert_eq!(batch.duration, result.durations[i]);
        for expected in &batch.events {
            let actual = events.next().unwrap();
            assert_eq!(actual.substep, i);
            assert_eq!(actual.time_seconds, elapsed);
            assert_eq!(actual.event, *expected);
        }
        assert!(
            batch
                .events
                .windows(2)
                .all(|w| w[0].sources <= w[1].sources)
        );
        elapsed += f64::from(result.durations[i]);
    }
    assert!(events.next().is_none());
    for event in &result.events {
        let end = event.substep + usize::from(event.phase == FlatEventPhase::AfterDrift);
        assert_eq!(
            event.time_seconds,
            result.durations[..end]
                .iter()
                .map(|d| f64::from(*d))
                .sum::<f64>()
        );
        if event.phase == FlatEventPhase::AfterDrift {
            assert_eq!(event.event.transition, EpisodeTransition::End);
        }
    }
    for pair in result.events.windows(2) {
        assert!(pair[0].time_seconds <= pair[1].time_seconds);
        if pair[0].time_seconds == pair[1].time_seconds {
            assert!(pair[0].substep <= pair[1].substep);
            if pair[0].substep == pair[1].substep && pair[0].phase == pair[1].phase {
                assert!(pair[0].event.sources <= pair[1].event.sources);
            }
        }
    }
    let backend: Duration = result
        .advances
        .iter()
        .chain(&result.previews)
        .map(|m| m.wall_time)
        .sum();
    assert!(result.wall_time >= backend);
    assert!(
        world
            .raw
            .colliders
            .iter()
            .all(|(_, c)| c.active_hooks().is_empty())
    );
}

pub(super) fn compare(
    native: &mut PhysicsWorld,
    fixture: &PhysicsWorld,
    tracker: &departure::Tracker,
    trace: &Trace,
    dt: f32,
) {
    let mut replay = native.clone();
    let actual = native.step_material_flat_arrivals(dt).unwrap();
    let restored = replay.step_material_flat_arrivals(dt).unwrap();
    check_trace(&actual, native, dt);
    assert_eq!(actual.durations, trace.durations);
    assert_eq!(actual.durations, restored.durations);
    assert_eq!(actual.events, restored.events);
    assert_eq!(
        actual
            .events
            .iter()
            .filter(|e| e.phase == FlatEventPhase::AfterDrift)
            .map(|e| (e.time_seconds, e.event))
            .collect::<Vec<_>>(),
        trace
            .departures
            .iter()
            .map(|d| (d.at, d.event))
            .collect::<Vec<_>>()
    );
    for other in [fixture, &replay] {
        assert_eq!(native.loading_batches(), other.loading_batches());
        assert_eq!(
            native.material_response_batches(),
            other.material_response_batches()
        );
    }
    assert_eq!(
        native.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    // Compare the independent witness wire data separately from the complete
    // physical/capture snapshot; the fixture keeps its tracker outside the world.
    assert_eq!(
        bincode::serialize(
            native
                .loading
                .as_ref()
                .unwrap()
                .flat_departures
                .as_ref()
                .unwrap()
        )
        .unwrap(),
        bincode::serialize(tracker).unwrap()
    );
    let mut physical = native.clone();
    physical.loading.as_mut().unwrap().flat_departures = None;
    assert!(
        physical.snapshot_bytes().unwrap() == fixture.snapshot_bytes().unwrap(),
        "snapshot mismatch"
    );
}

fn unchanged(
    world: &PhysicsWorld,
    snapshot: &[u8],
    loading: &[ContactBatch],
    response: &[ResponseBatch],
) {
    assert!(
        world.snapshot_bytes().unwrap() == snapshot,
        "snapshot changed on refusal"
    );
    assert_eq!(world.loading_batches(), loading);
    assert_eq!(world.material_response_batches(), response);
}

#[test]
fn native_flat_admission_and_failure_preserve_previous_outputs() {
    for case in 0..33 {
        let (mut world, _) = fixture(&[4, 2, 4], 1, false, 0.0, Vec2::ZERO, 0.0, true);
        world.step_material_flat_arrivals(0.001).unwrap();
        assert!(!world.material_response_batches().is_empty());
        let b = world.body_handle(body(1)).unwrap();
        let c = world.collider_handle(collider(1, 0)).unwrap();
        let mut expected = R::UnsupportedScene;
        match case {
            0 => world.raw.bodies[b].lock_rotations(false, true),
            1 => world.raw.colliders[c].set_friction(0.1),
            2 => {
                world.set_gravity(Vec2::Y);
            }
            3 => world.raw.bodies[b].enable_ccd(true),
            4 => world.raw.bodies[b].set_soft_ccd_prediction(0.01),
            5 => world.raw.bodies[b].add_force(Vector::Y, true),
            6 => world.raw.bodies[b].set_linear_damping(0.1),
            7 => world.raw.colliders[c].set_sensor(true),
            8 => world.raw.colliders[c].set_contact_skin(0.01),
            9 => world.raw.colliders[c].set_active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS),
            10 => world.raw.colliders[c].set_solver_groups(CollisionGroups::NONE.to_rapier()),
            11 => world.raw.colliders[c].set_collision_groups(CollisionGroups::NONE.to_rapier()),
            12 => world.raw.bodies[b].set_body_type(RigidBodyType::KinematicPositionBased, true),
            13 => {
                world.raw.bodies[b]
                    .activation_mut()
                    .normalized_linear_threshold = 0.4
            }
            14 => world.loading.as_mut().unwrap().unbind(collider(1, 0)),
            15 => {
                for h in world.raw.bodies[b].colliders().to_vec() {
                    world.raw.colliders[h].set_mass(0.0);
                }
            }
            16 => world.raw.colliders[c].set_translation_wrt_parent(Vector::new(0.01, -0.75)),
            17 => {
                // Outer bounds still match; only an interior gap is introduced.
                let middle = world.collider_handle(collider(1, 1)).unwrap();
                world.raw.colliders[middle].set_translation_wrt_parent(Vector::new(0.0, -0.125));
            }
            18 => world.raw.colliders[c].set_rotation_wrt_parent(0.01),
            19 => world.raw.colliders[c].set_shape(SharedShape::ball(0.5)),
            20 => {
                world.set_pose(body(1), Vec2::ZERO, 0.01, true);
            }
            21 => {
                let v = world.raw.bodies[b].linvel() + Vector::Y;
                world.raw.bodies[b].set_linvel(v, true);
                expected = R::Numerical;
            }
            22 => {
                world.set_pose(body(2), Vec2::new(1.0, 2.0), 0.0, true);
                expected = R::Numerical;
            }
            23 => {
                world.set_pose(body(1), Vec2::X * 0.5, 0.0, true);
                expected = R::Numerical;
            }
            24 => {
                world.insert_body(
                    body(4),
                    BodySpec::default(),
                    &[ColliderSpec::cuboid(collider(4, 0), 0.5, 1.0)],
                );
            }
            25 => {
                world.loading.as_mut().unwrap().response = None;
                expected = R::ResponseDisabled;
            }
            26 => world.loading.as_mut().unwrap().config.touch_tolerance = 0.001,
            27 => world.raw.colliders[c].set_enabled(false),
            28 => world.raw.bodies[b].set_dominance_group(1),
            29 => world.raw.bodies[b].set_enabled(false),
            30 => {
                // Still a filled rectangle with the same outer bounds, but
                // unequal copy counts lie outside the admitted representation.
                for part in 0..4 {
                    let h = world.collider_handle(collider(1, part)).unwrap();
                    world.raw.colliders[h].set_shape(SharedShape::cuboid(0.5, 0.5));
                    world.raw.colliders[h].set_translation_wrt_parent(Vector::new(
                        0.0,
                        if part == 3 { 0.5 } else { -0.5 },
                    ));
                }
            }
            31 => {
                world
                    .loading
                    .as_mut()
                    .unwrap()
                    .bindings
                    .get_mut(&collider(1, 0))
                    .unwrap()
                    .patch = 1
            }
            _ => {
                let source = world.loading.as_ref().unwrap().bindings[&collider(1, 0)];
                world
                    .loading
                    .as_mut()
                    .unwrap()
                    .response
                    .as_mut()
                    .unwrap()
                    .materials
                    .remove(&source.region);
            }
        }
        let before = world.snapshot_bytes().unwrap();
        let loading = world.loading_batches().to_vec();
        let response = world.material_response_batches().to_vec();
        assert_eq!(
            world.step_material_flat_arrivals(DT).unwrap_err(),
            expected,
            "case {case}"
        );
        unchanged(&world, &before, &loading, &response);
    }
}

#[test]
fn native_flat_endpoint_pause_and_late_limits_are_atomic() {
    let (mut world, tokens) = fixture(&[2, 2], 1, false, 0.0, Vec2::ZERO, 0.5, true);
    world.set_pose(body(1), Vec2::X * -0.03125, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 32.0, 0.0, true);
    let capacity = balances(&world, &tokens);
    let result = world.step_material_flat_arrivals(1.0 / 1024.0).unwrap();
    assert!(result.events.is_empty());
    assert_eq!(capacity, balances(&world, &tokens));
    let result = world.step_material_flat_arrivals(1.0e-6).unwrap();
    assert_eq!(result.events[0].time_seconds, 0.0);
    assert_eq!(result.events[0].event.transition, EpisodeTransition::Impact);
    let before = world.snapshot_bytes().unwrap();
    let loading = world.loading_batches().to_vec();
    let response = world.material_response_batches().to_vec();
    assert_eq!(before[0], 5);
    for dt in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            world.step_material_flat_arrivals(dt).unwrap_err(),
            R::InvalidTimestep
        );
        unchanged(&world, &before, &loading, &response);
    }
    let result = world.step_material_flat_arrivals(0.0).unwrap();
    assert!(
        result.events.is_empty()
            && result.durations.is_empty()
            && result.previews.is_empty()
            && result.advances.is_empty()
    );
    unchanged(&world, &before, &[], &[]);

    for capture_limit in [false, true] {
        let (mut world, _) = fixture(&[1, 1, 1], 1, false, 0.0, Vec2::ZERO, 0.0, true);
        if capture_limit {
            world.loading.as_mut().unwrap().config.max_batches_per_step = 1;
        } else {
            world.set_pose(body(1), Vec2::ZERO, 0.0, true);
            world.set_pose(body(3), Vec2::X * 2.02, 0.0, true);
        }
        let before = world.snapshot_bytes().unwrap();
        let loading = world.loading_batches().to_vec();
        let response = world.material_response_batches().to_vec();
        let expected = if capture_limit {
            R::Response(ResponseRejection::Capture(CaptureRejection::StepLimit))
        } else {
            R::AdvanceLimit
        };
        assert_eq!(
            world
                .step_flats_with_limit(DT, if capture_limit { MAX_ADVANCES } else { 1 })
                .unwrap_err(),
            expected
        );
        unchanged(&world, &before, &loading, &response);
    }
}

#[test]
fn native_flat_retains_slow_touching_and_rounded_energy_refusals() {
    let (mut world, _) = fixture(&[1, 1], 1, false, 0.0, Vec2::ZERO, 0.0, false);
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    let c = world.collider_handle(collider(1, 0)).unwrap();
    world.raw.colliders[c].set_mass(1.0e12);
    world.refresh_mass_properties(body(1));
    world.set_velocity(body(1), Vec2::X * 0.00005, 0.0, true);
    let before = world.snapshot_bytes().unwrap();
    assert_eq!(
        world.step_material_flat_arrivals(DT).unwrap_err(),
        R::Unaccounted
    );
    unchanged(&world, &before, &[], &[]);
    let initial = tests::kinetic(&world, Vec2::ZERO);
    world.loading.as_mut().unwrap().config.min_approach_speed = 0.0;
    world.step_material_flat_arrivals(DT).unwrap();
    assert!(
        (initial - tests::kinetic(&world, Vec2::ZERO) - loss(applied(&world))).abs()
            < 1.0e-5 * initial
    );

    for shift in [-1_000_000.0, 1_000_000.0] {
        let (mut world, _) = fixture(&[4, 2], 2, true, 0.0, Vec2::X * shift, 0.5, false);
        world.set_pose(body(1), Vec2::ZERO, 0.0, true);
        world.set_velocity(body(1), Vec2::X * (shift + 0.25), 0.0, true);
        let before = world.snapshot_bytes().unwrap();
        assert_eq!(
            world.step_material_flat_arrivals(DT).unwrap_err(),
            R::Response(ResponseRejection::Backend)
        );
        unchanged(&world, &before, &[], &[]);
    }

    let (mut world, tokens) = fixture(&[2, 4], 1, true, 0.0, Vec2::ZERO, 0.5, false);
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.set_velocity(body(1), Vec2::ZERO, 0.0, true);
    world.step_material_flat_arrivals(DT).unwrap();
    let capacity = balances(&world, &tokens);
    for speed in [0.00005, 1.0] {
        world.set_velocity(body(1), Vec2::X * speed, 0.0, true);
        let before = world.snapshot_bytes().unwrap();
        let loading = world.loading_batches().to_vec();
        assert_eq!(
            world.step_material_flat_arrivals(DT).unwrap_err(),
            R::Unaccounted
        );
        unchanged(&world, &before, &loading, &[]);
        assert_eq!(balances(&world, &tokens), capacity);
    }
}

#[test]
fn native_flat_center_offsets_and_spatial_source_order_are_explicit() {
    for offset_center in [false, true] {
        let (mut world, _) = fixture(&[4, 2, 4], 2, true, 0.37, Vec2::new(17.0, -11.0), 0.5, true);
        if offset_center {
            for (_, c) in world.raw.colliders.iter_mut() {
                c.set_mass_properties(MassProperties::new(
                    Vector::new(0.125, -0.25),
                    c.mass(),
                    1.0,
                ));
            }
            for n in 1..=3 {
                world.refresh_mass_properties(body(n));
            }
        }
        let mut reference = world.clone();
        let mut tracker = departure::Tracker::default();
        let trace =
            step_tracked(&mut reference, DT, MAX_ADVANCES, true, Some(&mut tracker)).unwrap();
        compare(&mut world, &reference, &tracker, &trace, DT);
    }

    let (mut world, _) = fixture(&[4, 2, 4], 2, true, 0.0, Vec2::ZERO, 0.5, true);
    for n in 1..=3 {
        let motion = world.motion(body(n)).unwrap();
        world.set_pose(body(n), -motion.position, 0.0, true);
        world.set_velocity(body(n), -motion.linear_velocity, 0.0, true);
    }
    // Teleporting a warm B-C manifold across the whole layout leaves invalid
    // backend contact depths. Refuse those instead of accepting correction work.
    let before = world.snapshot_bytes().unwrap();
    let loading = world.loading_batches().to_vec();
    assert_eq!(
        world.step_material_flat_arrivals(DT).unwrap_err(),
        R::Numerical
    );
    unchanged(&world, &before, &loading, &[]);

    // A fresh pair has no old support manifold. Source order is now opposite
    // spatial order, and the arrival must still complete with the same ledger.
    let (mut world, tokens) = fixture(&[4, 2], 2, true, 0.0, Vec2::ZERO, 0.5, true);
    for n in 1..=2 {
        let motion = world.motion(body(n)).unwrap();
        world.set_pose(body(n), -motion.position, 0.0, true);
        world.set_velocity(body(n), -motion.linear_velocity, 0.0, true);
    }
    world.collect_events = true;
    let initial = tests::kinetic(&world, Vec2::ZERO);
    let mut replay = world.clone();
    let actual = world.step_material_flat_arrivals(DT).unwrap();
    let restored = replay.step_material_flat_arrivals(DT).unwrap();
    check_trace(&actual, &world, DT);
    assert_eq!(actual.events, restored.events);
    assert_eq!(world.contact_events(), replay.contact_events());
    assert_eq!(world.sensor_intersections(), replay.sensor_intersections());
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    let response = applied(&world);
    assert_eq!(
        response.impacts[0].sources.map(|s| s.region),
        [tokens[0], tokens[1]]
    );
    assert_eq!(response.impacts.len(), 1);
    assert!(
        (initial - tests::kinetic(&world, Vec2::ZERO) - loss(response)).abs() < 1.0e-5 * initial
    );
}

#[test]
fn native_flat_discards_ball_departure_evidence_only_on_commit() {
    type Proofs = BTreeMap<[ContactSource; 2], (EpisodeId, [Vec2; 2])>;
    fn proofs(world: &PhysicsWorld) -> Proofs {
        let tracker = world
            .loading
            .as_ref()
            .unwrap()
            .arrival_departures
            .as_ref()
            .unwrap();
        bincode::deserialize(&bincode::serialize(tracker).unwrap()).unwrap()
    }
    let (mut world, _) = super::super::scene(1, false, 17.0, true);
    for (n, x) in [(1, -0.01), (2, 1.0), (3, 2.02)] {
        let h = world.body_handle(body(n)).unwrap();
        world.raw.bodies[h].lock_rotations(true, true);
        world.set_pose(body(n), Vec2::X * x, 0.0, true);
    }
    world.step_material_line_arrivals(0.00025).unwrap();
    world
        .step_material_line_arrivals(5229.0_f32 * 1.0e-9)
        .unwrap();
    assert_eq!(proofs(&world).len(), 1);
    for (_, c) in world.raw.colliders.iter_mut() {
        c.set_shape(SharedShape::cuboid(0.5, 1.0));
    }
    let before = world.snapshot_bytes().unwrap();
    let loading = world.loading_batches().to_vec();
    let response = world.material_response_batches().to_vec();
    assert_eq!(
        world.step_flats_with_limit(1.0e-9, 0).unwrap_err(),
        R::AdvanceLimit
    );
    unchanged(&world, &before, &loading, &response);
    assert_eq!(proofs(&world).len(), 1);
    world.step_material_flat_arrivals(0.0).unwrap();
    unchanged(&world, &before, &[], &[]);
    let mut replay = world.clone();
    let a = world.step_material_flat_arrivals(1.0e-9).unwrap();
    let b = replay.step_material_flat_arrivals(1.0e-9).unwrap();
    assert_eq!(a.events, b.events);
    assert!(proofs(&world).is_empty());
    assert_eq!(world.snapshot_bytes().unwrap()[0], 5);
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
}
