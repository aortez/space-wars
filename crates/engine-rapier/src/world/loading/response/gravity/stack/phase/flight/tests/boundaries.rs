use super::*;

fn refused(
    world: &PhysicsWorld,
    run: impl FnOnce(Staged) -> Result<Staged, Rejection>,
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
    let staged = Staged::prepare(world, GravitySettling::BoundedNumerical, 2).unwrap();
    assert_eq!(run(staged).err(), Some(expected));
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
fn gravity_stack_flight_limits_refuse_after_provisional_drift_and_departure() {
    let world = stack_world([11, 2, 7], true, [4, 2, 4], 2);
    for (dt, intervals, projections, expected) in [
        (0.001, 0, 256, Rejection::AdvanceLimit),
        (0.001, 8, 0, Rejection::SolveLimit),
        (0.001, 8, 1, Rejection::SolveLimit),
        (0.1, 8, 256, Rejection::Numerical),
        (0.0, 8, 256, Rejection::InvalidTimestep),
        (-1.0, 8, 256, Rejection::InvalidTimestep),
        (f32::NAN, 8, 256, Rejection::InvalidTimestep),
    ] {
        refused(
            &world,
            |s| s.fly(dt, intervals, projections).map(|(s, _)| s),
            expected,
        );
    }
    refused(
        &world,
        |mut s| {
            for _ in 0..8 {
                (s, _) = s.fly(0.0001, usize::MAX, usize::MAX)?;
            }
            assert_eq!(s.intervals, 8);
            assert_eq!(s.metrics.len(), 25);
            s.fly(0.0001, usize::MAX, usize::MAX).map(|(s, _)| s)
        },
        Rejection::AdvanceLimit,
    );
    refused(
        &world,
        |mut s| {
            s.projections = MAX_PROJECTIONS - 1;
            s.fly(0.001, 8, usize::MAX).map(|(s, _)| s)
        },
        Rejection::SolveLimit,
    );

    // Configure the record cap before enrollment. Bootstrap and a damaging
    // impact fit; a later End record after all three flight solves does not.
    let mut world = world;
    world.set_pose(body(11), Vec2::ZERO, 0.0, true);
    let staged = Staged::prepare(&world, GravitySettling::BoundedNumerical, 2).unwrap();
    let (staged, hit) = staged.impact(8).unwrap();
    assert!(
        hit.impact
            .response
            .impacts
            .iter()
            .any(|hit| hit.outcome.consumed != [0.0; 2])
    );
    let before_records = staged.world.loading.as_ref().unwrap().records;
    let (accepted, flight) = staged.fly(0.001, 8, 256).unwrap();
    assert!(flight.departure.is_some());
    assert_eq!(
        accepted.world.loading.as_ref().unwrap().records,
        before_records + 2
    );
    world.loading.as_mut().unwrap().config.max_records_per_step = before_records + 1;
    refused(
        &world,
        |s| {
            let (s, _) = s.impact(8)?;
            s.fly(0.001, 8, 256).map(|(s, _)| s)
        },
        Rejection::RecordLimit,
    );
    // Previous committed impact reports survive a later rejected flight too.
    instant(&mut world, GravitySettling::BoundedNumerical).unwrap();
    refused(
        &world,
        |s| s.fly(0.001, 8, 1).map(|(s, _)| s),
        Rejection::SolveLimit,
    );
}

#[test]
fn gravity_stack_flight_fixed_numerical_allowances_refuse_exhaustion() {
    let world = stack_world([11, 2, 7], true, [4, 2, 4], 2);
    let base = Staged::prepare(&world, GravitySettling::BoundedNumerical, 2)
        .unwrap()
        .world;
    let (_, sample) = Staged::prepare(&base, GravitySettling::BoundedNumerical, 0)
        .unwrap()
        .fly(0.001, 8, 256)
        .unwrap();
    for (i, axis) in [
        GravityErrorAxis::Work,
        GravityErrorAxis::Impulse,
        GravityErrorAxis::Position,
    ]
    .into_iter()
    .enumerate()
    {
        assert_ne!(sample.flight.errors[i], 0.0);
        let mut world = base.clone();
        let h = &mut world
            .loading
            .as_mut()
            .unwrap()
            .gravity
            .as_mut()
            .unwrap()
            .history;
        let bound = 1.0e-5 * h.scales[i];
        h.errors[i] = bound.copysign(sample.flight.errors[i]);
        let attempted = h.errors[i] + sample.flight.errors[i];
        let world = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
        refused(
            &world,
            |s| s.fly(0.001, 8, 256).map(|(s, _)| s),
            Rejection::NumericalAllowance {
                axis,
                attempted,
                bound,
            },
        );
    }
    let mut world = base;
    let h = &mut world
        .loading
        .as_mut()
        .unwrap()
        .gravity
        .as_mut()
        .unwrap()
        .history;
    h.reference[1] = f64::from(1.0e20_f32);
    h.scales[0] = 10.0 * h.reference[1] * h.reference[1] - 200.0 * h.reference[0];
    h.scales[1] = libm::sqrt(40.0 * h.scales[0]);
    h.errors[0] = 1.0e20;
    let world = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    refused(
        &world,
        |s| s.fly(0.001, 8, 256).map(|(s, _)| s),
        Rejection::Numerical,
    );
}

#[test]
fn gravity_stack_flight_requires_free_motion_and_current_owners() {
    for speed in [0.0, 4.0] {
        let mut world = stack_world([11, 2, 7], true, [4, 2, 4], 2);
        world.set_pose(body(11), Vec2::ZERO, 0.0, true);
        world.set_velocity(body(11), Vec2::X * speed, 0.0, true);
        refused(
            &world,
            |s| s.fly(0.001, 8, 256).map(|(s, _)| s),
            if speed == 0.0 {
                Rejection::UnsafeSupport
            } else {
                Rejection::UnaccountedApproach
            },
        );
    }
    let world = stack_world([11, 2, 7], true, [4, 2, 4], 2);
    refused(
        &world,
        |mut s| {
            let sources = s.active()?.support.sources;
            s.world
                .loading
                .as_mut()
                .unwrap()
                .episodes
                .get_mut(&sources)
                .unwrap()
                .id = EpisodeId(999);
            s.fly(0.001, 8, 256).map(|(s, _)| s)
        },
        Rejection::StaleContinuation,
    );
}
