use super::*;

fn candidate(task: FlightTask, family: FlightFamily) -> FlightCandidate {
    FlightCandidate {
        generator_version: GENERATOR_VERSION,
        seed: 0,
        index: 0,
        family,
        recipe: "test".into(),
        task,
    }
}

#[test]
fn seeded_candidates_and_admission_are_reproducible_without_retries() {
    for index in 0..12 {
        let c = FlightCandidate::generate(20261022, index).unwrap();
        assert_eq!(c, FlightCandidate::generate(20261022, index).unwrap());
        let decoded: FlightCandidate =
            serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(c.inspect()).unwrap(),
            serde_json::to_value(decoded.inspect()).unwrap()
        );
        assert_ne!(
            c.task,
            FlightCandidate::generate(20261023, index).unwrap().task
        );
    }
}

#[test]
fn open_u_requires_retreat_but_sealed_u_has_no_conservative_witness() {
    let mut task = FlightTask::default();
    task.goal_offset = Vec2::new(75.0, 0.0);
    for sign in [-1.0, 1.0] {
        task.obstacles.push(FlightObstacle {
            offset: Vec2::new(0.0, sign * 22.0),
            half_extents: Vec2::new(40.0, 2.0),
            angle: 0.0,
        });
    }
    task.obstacles.push(FlightObstacle {
        offset: Vec2::new(40.0, 0.0),
        half_extents: Vec2::new(2.0, 24.0),
        angle: 0.0,
    });
    let open = candidate(task.clone(), FlightFamily::Obstructed).inspect();
    assert!(open.admitted, "{:?}", open.rejection);
    let e = open.evidence.unwrap();
    assert!(!e.direct_clear && e.first_leg_moves_away);
    task.obstacles.push(FlightObstacle {
        offset: Vec2::new(-40.0, 0.0),
        half_extents: Vec2::new(2.0, 24.0),
        angle: 0.0,
    });
    let sealed = candidate(task, FlightFamily::Obstructed).inspect();
    assert_eq!(
        sealed.rejection.as_deref(),
        Some("no_conservative_route_witness")
    );
}

#[test]
fn moving_goal_checks_whole_segment_not_only_endpoints() {
    let mut task = FlightTask {
        goal_offset: Vec2::new(40.0, 0.0),
        goal_velocity: Vec2::new(10.0, 0.0),
        ..Default::default()
    };
    assert!(
        candidate(task.clone(), FlightFamily::Moving)
            .inspect()
            .admitted
    );
    task.obstacles.push(FlightObstacle {
        offset: Vec2::new(180.0, 0.0),
        half_extents: Vec2::new(0.2, 10.0),
        angle: 0.0,
    });
    let env = FlightEnvironment::new(task.clone()).unwrap();
    let center = crate::universe_center(500.0);
    let own = env.state.world.physics.ship_body(0).entity;
    let clear = |a, b| {
        admission::segment_clear(
            &env.state.world.physics.world,
            own,
            center,
            500.0,
            &[],
            a,
            b,
            12.0,
        )
    };
    let a = env.frame.target_position;
    let b = a + task.goal_velocity * 30.0;
    assert!(clear(a, a) && clear(b, b));
    assert!(!clear(a, b));
    assert_eq!(
        candidate(task, FlightFamily::Moving)
            .inspect()
            .rejection
            .as_deref(),
        Some("unsafe_goal_trajectory")
    );
    let edge = FlightTask {
        goal_offset: Vec2::new(40.0, 0.0),
        goal_velocity: Vec2::new(20.0, 0.0),
        ..Default::default()
    };
    assert_eq!(
        candidate(edge, FlightFamily::Moving)
            .inspect()
            .rejection
            .as_deref(),
        Some("unsafe_goal_trajectory")
    );
}

#[test]
fn edits_open_a_real_passage_and_new_structures_block_the_prior_direct_path() {
    // The first four admitted altered candidates; a spinning planet carries
    // some goals through the new structures, and those are rejected.
    let admitted: Vec<_> = (3..64)
        .step_by(4)
        .map(|index| FlightCandidate::generate(20261022, index).unwrap())
        .filter(|c| c.inspect().admitted)
        .take(4)
        .collect();
    assert_eq!(admitted.len(), 4);
    for c in admitted {
        let altered = c.inspect();
        let mut before = c.clone();
        before.task.terrain_edits.clear();
        before.task.obstacles.clear();
        let old = before.inspect();
        let direct = altered.evidence.as_ref().unwrap().direct_clear;
        let old_direct = old.evidence.as_ref().unwrap().direct_clear;
        if c.recipe == "carved_passage" {
            assert!(direct && !old_direct);
        } else {
            assert!(!direct && old_direct);
        }
    }
}

#[test]
fn rotation_envelope_rejects_clear_now_but_unsafe_later_geometry() {
    let env = FlightEnvironment::new(FlightTask::default()).unwrap();
    let w = &env.state.world.physics.world;
    let own = env.state.world.physics.ship_body(0).entity;
    let center = env.state.world.planets[0].position;
    let p = center + Vec2::new(0.0, 100.0);
    assert!(admission::segment_clear(
        w,
        own,
        center,
        500.0,
        &[],
        p,
        p,
        12.0
    ));
    // A rotating protrusion can sweep this point despite being elsewhere now.
    assert!(!admission::segment_clear(
        w,
        own,
        center,
        500.0,
        &[(center, 110.0)],
        p,
        p,
        12.0
    ));
}

#[test]
fn deadline_rejection_preserves_witness_and_gravity_evidence() {
    let mut task = FlightTask::default();
    task.start.gravity = true;
    task.deadline_ticks = 60;
    let report = candidate(task.clone(), FlightFamily::Clear).inspect();
    assert_eq!(
        report.rejection.as_deref(),
        Some("reference_time_exceeds_deadline")
    );
    let e = report.evidence.unwrap();
    assert!(e.reference_time_seconds.unwrap() > e.deadline_seconds);
    assert!(!e.route.is_empty());
    assert!(
        e.gravity_samples
            .iter()
            .any(|s| s.acceleration.length() > 1.0)
    );
    let env = FlightEnvironment::new(task).unwrap();
    // Production samples gravity before the first motion update. At this quiet
    // fixture that differs negligibly from the completed-step legacy anchor.
    assert!(
        (e.gravity_samples[0].acceleration - env.frame.gravity).length() < 0.01,
        "probe {:?}, actual {:?}",
        e.gravity_samples[0].acceleration,
        env.frame.gravity
    );
    assert!(e.goal_margin_after_hull_and_tolerance > 1.0);
}

#[test]
fn inspection_is_read_only_and_rejects_unmodelled_dynamic_bodies() {
    let c = candidate(FlightTask::default(), FlightFamily::Clear);
    let mut env = FlightEnvironment::new(c.task.clone()).unwrap();
    let before = env.state.world.physics.world.snapshot_bytes().unwrap();
    let inputs = env.policy_observation().unwrap();
    let mut report = c.inspect();
    admission::inspect(&env, &c, &mut report, 30.0, 200.0).unwrap();
    assert_eq!(
        before,
        env.state.world.physics.world.snapshot_bytes().unwrap()
    );
    assert_eq!(inputs, env.policy_observation().unwrap());
    let id = PhysicsId::new(9_200_000);
    env.state.world.physics.world.insert_body(
        PhysicsBodyId::new(id, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Dynamic,
            position: env.frame.origin + Vec2::new(100.0, 0.0),
            ..Default::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(id, ColliderRole::PRIMARY, 0),
            1.0,
        )],
    );
    assert_eq!(
        admission::inspect(&env, &c, &mut report, 30.0, 200.0),
        Err("unsupported_independent_body_motion")
    );
}

#[test]
fn balanced_batch_accounts_for_rejections_and_reports_budget_exhaustion() {
    let batch = FlightTaskBatch::generate(20261022, 3, 24).unwrap();
    assert!(batch.complete);
    assert_eq!(batch.admitted_by_family, [3; 4]);
    assert!(batch.attempts.len() > 12);
    assert_eq!(batch.attempts.iter().filter(|r| r.admitted).count(), 12);
    assert!(
        batch
            .attempts
            .iter()
            .filter(|r| !r.admitted)
            .all(|r| r.rejection.is_some())
    );
    let short = FlightTaskBatch::generate(20261022, 3, 1).unwrap();
    assert!(!short.complete);
    assert_eq!(short.attempts.len(), 4);
    assert!(short.admitted_by_family.iter().all(|&n| n < 3));
}

#[test]
fn clear_control_admission_is_separate_and_preserves_physical_criteria() {
    let mut task = FlightTask::default();
    task.goal_offset = Vec2::new(8.0, 0.0);
    let mut c = candidate(task, FlightFamily::Clear);
    c.recipe = "clear-control-v1/stratum-0".into();
    assert_eq!(
        c.inspect().rejection.as_deref(),
        Some("goal_distance_outside_recipe")
    );
    let admitted = c.inspect_clear_control();
    assert!(admitted.admitted, "{:?}", admitted.rejection);
    assert!(admitted.evidence.unwrap().direct_clear);
    c.task.arrival.position = 4.0;
    assert_eq!(
        c.inspect_clear_control().rejection.as_deref(),
        Some("outside_clear_control_recipe")
    );
    c.task.arrival.position = 3.0;
    c.task.start.gravity = true;
    assert!(!c.inspect_clear_control().admitted);
    c.task.start.gravity = false;
    c.task.start.relative_velocity = Vec2::new(1.1, 0.0);
    assert!(!c.inspect_clear_control().admitted);
}

#[test]
fn aligned_approach_admission_requires_stationary_forward_goal() {
    let mut task = FlightTask::default();
    task.goal_offset = Vec2::new(0.0, 5.0);
    let mut c = candidate(task, FlightFamily::Clear);
    c.recipe = "aligned-approach-v1/stratum-0".into();
    let admitted = c.inspect_aligned_approach();
    assert!(admitted.admitted, "{:?}", admitted.rejection);
    assert!(admitted.evidence.unwrap().direct_clear);
    assert!(!c.inspect_clear_control().admitted);
    c.task.start.heading_offset = 0.1;
    assert!(!c.inspect_aligned_approach().admitted);
    c.task.start.heading_offset = 0.0;
    c.task.start.relative_velocity = Vec2::new(0.01, 0.0);
    assert!(!c.inspect_aligned_approach().admitted);
    c.task.start.relative_velocity = Vec2::ZERO;
    c.task.start.spin = 0.01;
    assert!(!c.inspect_aligned_approach().admitted);
    c.task.start.spin = 0.0;
    c.task.arrival.dwell_ticks = 29;
    assert!(!c.inspect_aligned_approach().admitted);
    c.task.arrival.dwell_ticks = 30;
    c.recipe = "aligned-approach-v1/stratum-1".into();
    assert!(!c.inspect_aligned_approach().admitted);
    c.task.goal_offset = Vec2::new(0.0, 7.0);
    assert!(c.inspect_aligned_approach().admitted);
}

#[test]
fn hold_position_admission_requires_zero_error_and_preserves_criteria() {
    let mut task = FlightTask::default();
    task.goal_offset = Vec2::ZERO;
    let mut c = candidate(task, FlightFamily::Clear);
    c.recipe = "hold-position-v1/stratum-2".into();
    let admitted = c.inspect_hold_position();
    assert!(admitted.admitted, "{:?}", admitted.rejection);
    let evidence = admitted.evidence.unwrap();
    assert!(evidence.direct_clear && evidence.goal_trajectory_clear);
    assert_eq!(evidence.route_length, 0.0);
    assert!(!c.inspect_clear_control().admitted);
    assert!(!c.inspect_aligned_approach().admitted);
    let baseline = c.clone();
    c.task.goal_offset = Vec2::new(0.01, 0.0);
    assert!(!c.inspect_hold_position().admitted);
    c = baseline.clone();
    c.task.start.relative_velocity = Vec2::new(0.01, 0.0);
    assert!(!c.inspect_hold_position().admitted);
    c = baseline.clone();
    c.task.start.spin = 0.01;
    assert!(!c.inspect_hold_position().admitted);
    c = baseline.clone();
    c.task.arrival.dwell_ticks = 29;
    assert!(!c.inspect_hold_position().admitted);
    c = baseline.clone();
    c.task.start.gravity = true;
    assert!(!c.inspect_hold_position().admitted);
    c = baseline.clone();
    c.recipe = "hold-position-v1/stratum-0".into();
    assert!(!c.inspect_hold_position().admitted);
    c.task.start.heading_offset = -2.0;
    assert!(c.inspect_hold_position().admitted);
}

#[test]
fn early_hold_admission_requires_matching_recipe_and_terminal_contract() {
    let mut task = FlightTask::default();
    task.goal_offset = Vec2::ZERO;
    let mut c = candidate(task, FlightFamily::Clear);
    c.recipe = "hold-first-exit-v1/stratum-2".into();
    assert!(!c.inspect_hold_first_exit().admitted);
    c.task.hold_failure = Some(HoldFailure::FirstToleranceExitV1);
    assert!(c.inspect_hold_first_exit().admitted);
    assert!(!c.inspect_hold_position().admitted);
    c.recipe = "hold-position-v1/stratum-2".into();
    assert!(!c.inspect_hold_position().admitted);
    c.task.hold_failure = None;
    assert!(c.inspect_hold_position().admitted);
}

#[test]
fn robustness_admission_is_geometric_and_preserves_other_recipe_time_gates() {
    let mut task = FlightTask::default();
    task.goal_offset = Vec2::ZERO;
    task.deadline_ticks = 30;
    let mut c = candidate(task, FlightFamily::Clear);
    c.recipe = "hold-robustness-v1/nominal-30/stratum-2".into();
    let report = c.inspect_hold_robustness();
    assert!(report.admitted, "{:?}", report.rejection);
    let evidence = report.evidence.unwrap();
    assert!(
        evidence.full_hull_start_clear && evidence.goal_trajectory_clear && evidence.direct_clear
    );
    assert!(evidence.reference_time_seconds.unwrap() > evidence.deadline_seconds);
    assert!(!c.inspect_hold_position().admitted);
    c.task.goal_offset = Vec2::new(4.0, 0.0);
    c.task.deadline_ticks = 300;
    assert!(c.inspect_hold_robustness().admitted);
    c.task.goal_offset.x = 4.01;
    assert!(!c.inspect_hold_robustness().admitted);
}

#[test]
fn robustness_admission_rejects_changed_limits_early_failure_and_geometry() {
    let mut task = FlightTask::default();
    task.goal_offset = Vec2::ZERO;
    task.deadline_ticks = 300;
    task.arrival.dwell_ticks = 300;
    let mut c = candidate(task, FlightFamily::Clear);
    c.recipe = "hold-robustness-v1/nominal-300/stratum-2".into();
    let base = c.clone();
    assert!(c.inspect_hold_robustness().admitted);
    c.task.arrival.spin = 0.3;
    assert!(!c.inspect_hold_robustness().admitted);
    c = base.clone();
    c.task.hold_failure = Some(HoldFailure::FirstToleranceExitV1);
    assert!(!c.inspect_hold_robustness().admitted);
    c = base.clone();
    c.task.start.spin = 0.31;
    assert!(!c.inspect_hold_robustness().admitted);
    c = base.clone();
    c.task.start.relative_velocity = Vec2::new(1.6, 0.0);
    assert!(!c.inspect_hold_robustness().admitted);
    c = base;
    c.task.obstacles.push(FlightObstacle {
        offset: Vec2::ZERO,
        half_extents: Vec2::new(10.0, 10.0),
        angle: 0.0,
    });
    assert!(!c.inspect_hold_robustness().admitted);
}

#[test]
fn settle_recover_admits_only_declared_families_and_keeps_physical_limits() {
    let mut task = FlightTask::default();
    task.goal_offset = Vec2::ZERO;
    task.deadline_ticks = 900;
    task.arrival.dwell_ticks = 300;
    let mut c = candidate(task, FlightFamily::Clear);
    for slot in 0..4 {
        c.recipe = format!("settle-recover-v1/stratum-{slot}");
        c.task.goal_offset = if slot == 1 {
            Vec2::new(-6.0, 0.0)
        } else {
            Vec2::ZERO
        };
        c.task.start.relative_velocity = if slot == 2 {
            Vec2::new(0.0, -1.5)
        } else {
            Vec2::ZERO
        };
        c.task.start.spin = if slot == 3 { -0.4 } else { 0.0 };
        let r = c.inspect_settle_recover();
        assert!(r.admitted, "{:?}", r.rejection);
        assert!(r.evidence.unwrap().reference_time_seconds.unwrap() < 15.0);
        c.task.arrival.dwell_ticks = 30;
        assert!(c.inspect_settle_recover().admitted);
        c.task.arrival.dwell_ticks = 300;
    }
    c.task.arrival.velocity = 2.0;
    assert!(!c.inspect_settle_recover().admitted);
    c.task.arrival.velocity = 1.0;
    c.task.goal_offset = Vec2::new(1.0, 0.0);
    assert!(!c.inspect_settle_recover().admitted);
}
