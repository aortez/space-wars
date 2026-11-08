use super::*;

fn refused(
    world: &mut PhysicsWorld,
    policy: GravitySettling,
    captures: usize,
    impacts: usize,
    expected: Rejection,
) {
    let bytes = world.snapshot_bytes().unwrap();
    let batches = world.loading_batches().to_vec();
    let responses = world.material_response_batches().to_vec();
    let events = world.contact_events().to_vec();
    let diagnostics = (
        world.loading_diagnostics(),
        world.material_response_diagnostics(),
    );
    let result = Staged::prepare(world, policy, captures).and_then(|s| s.impact(impacts));
    assert!(
        matches!(&result, Err(e) if *e == expected),
        "expected {expected:?}, got {:?}",
        result.err()
    );
    assert_eq!(world.snapshot_bytes().unwrap(), bytes);
    assert_eq!(world.loading_batches(), batches);
    assert_eq!(world.material_response_batches(), responses);
    assert_eq!(world.contact_events(), events);
    assert_eq!(
        (
            world.loading_diagnostics(),
            world.material_response_diagnostics()
        ),
        diagnostics
    );
}

#[test]
fn gravity_stack_phase_limits_and_missing_accounting_roll_back_both_captures() {
    let mut prior = touching(4.0);
    instant(&mut prior, GravitySettling::Refuse).unwrap();
    for case in 0..7 {
        let mut world = touching(4.0);
        let expected = match case {
            0 | 1 => Rejection::SolveLimit,
            2 => {
                world.loading.as_mut().unwrap().config.max_batches_per_step = 1;
                Rejection::Response(ResponseRejection::Capture(CaptureRejection::StepLimit))
            }
            3 => {
                world.loading.as_mut().unwrap().config.max_records_per_step = 2;
                Rejection::Response(ResponseRejection::Capture(CaptureRejection::StepLimit))
            }
            4 => {
                world.set_velocity(body(11), Vec2::X * 0.00005, 0.0, true);
                Rejection::UnaccountedApproach
            }
            5 => {
                world.set_velocity(body(11), Vec2::X * 100.0, 0.0, true);
                Rejection::FailedRegion
            }
            _ => {
                world.set_gravity(Vec2::X * 400.0);
                Rejection::UnsafeSupport
            }
        };
        // Preserve real previous reports through even a late provisional refusal.
        let old = prior.loading.as_ref().unwrap();
        let l = world.loading.as_mut().unwrap();
        l.batches = old.batches.clone();
        l.diagnostics = old.diagnostics;
        l.response.as_mut().unwrap().batches = old.response.as_ref().unwrap().batches.clone();
        l.response.as_mut().unwrap().diagnostics = old.response.as_ref().unwrap().diagnostics;
        world.contact_events = prior.contact_events.clone();
        refused(
            &mut world,
            GravitySettling::Refuse,
            if case == 0 { 0 } else { 2 },
            if case == 1 { 0 } else { 8 },
            expected,
        );
    }
    let mut slow = touching(0.00005);
    slow.loading.as_mut().unwrap().config.min_approach_speed = 0.0;
    let response = instant(&mut slow, GravitySettling::Refuse).unwrap();
    assert!(response.impact.outgoing <= 0.0);
}

#[test]
fn gravity_stack_phase_continuation_preserves_history_and_rejects_stale_owners() {
    let mut staged = Staged::prepare(&touching(4.0), GravitySettling::BoundedNumerical, 2).unwrap();
    staged
        .world
        .loading
        .as_mut()
        .unwrap()
        .gravity
        .as_mut()
        .unwrap()
        .history
        .errors = [1.0e-6, -2.0e-6, 3.0e-7];
    let before = staged
        .world
        .loading
        .as_ref()
        .unwrap()
        .gravity
        .as_ref()
        .unwrap()
        .history
        .clone();
    let bytes = staged.world.snapshot_bytes().unwrap();
    let (staged, _) = staged.impact(8).unwrap();
    assert_eq!(
        staged
            .world
            .loading
            .as_ref()
            .unwrap()
            .gravity
            .as_ref()
            .unwrap()
            .history,
        before
    );
    let mut world = staged.world;
    refused(
        &mut world,
        GravitySettling::BoundedNumerical,
        2,
        8,
        Rejection::ExistingEpisode,
    );
    refused(
        &mut world,
        GravitySettling::Refuse,
        2,
        8,
        Rejection::PolicyChanged,
    );
    for edit in 0..4 {
        let w = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        let mut staged = Staged::prepare(&w, GravitySettling::BoundedNumerical, 0).unwrap();
        match edit {
            0 => {
                let h = staged.scene.plates.bodies[0];
                staged.world.raw.bodies[h].set_linvel(Vector::X * 3.0, true);
            }
            1 => staged.world.raw.gravity.x = 11.0,
            2 => {
                staged
                    .world
                    .raw
                    .integration_parameters
                    .num_solver_iterations += 1
            }
            _ => {
                let sources = staged.active().unwrap().support.sources;
                staged
                    .world
                    .loading
                    .as_mut()
                    .unwrap()
                    .episodes
                    .get_mut(&sources)
                    .unwrap()
                    .id = EpisodeId(999);
            }
        }
        assert!(matches!(
            staged.impact(8),
            Err(Rejection::StaleContinuation)
        ));
        assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    }
    // A geometrically valid old ordinary support is not native enrollment.
    let mut old = touching(0.0);
    old.set_pose(body(11), Vec2::X * -0.02, 0.0, true);
    old.set_gravity(Vec2::ZERO);
    old.step(0.00001);
    old.set_gravity(Vec2::X * 10.0);
    assert!(matches!(
        Staged::prepare(&old, GravitySettling::Refuse, 2),
        Err(Rejection::StaleContinuation)
    ));
}

#[test]
fn gravity_stack_phase_exact_stop_checks_combined_weight_before_commit() {
    for gravity in [10.0, 200.0] {
        let mut world = touching(0.05);
        world.set_gravity(Vec2::X * gravity);
        for material in world
            .loading
            .as_mut()
            .unwrap()
            .response
            .as_mut()
            .unwrap()
            .materials
            .values_mut()
        {
            material.parameters[4] = 1.0;
        }
        if gravity == 200.0 {
            refused(
                &mut world,
                GravitySettling::Refuse,
                2,
                8,
                Rejection::UnsafeSupport,
            );
        } else {
            let result = instant(&mut world, GravitySettling::Refuse).unwrap();
            assert_eq!(result.impact.outgoing, 0.0);
            let resumed = Staged::prepare(&world, GravitySettling::Refuse, 0).unwrap();
            assert!(resumed.active().unwrap().incoming.as_ref().unwrap().held);
            assert!(resumed.bootstrap.is_none());
        }
    }
}
