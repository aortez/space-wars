use super::*;

fn refused(
    w: &PhysicsWorld,
    policy: GravitySettling,
    run: impl FnOnce(Staged) -> Result<Staged, Rejection>,
    expected: Rejection,
) {
    let bytes = w.snapshot_bytes().unwrap();
    let batches = w.loading_batches().to_vec();
    let responses = w.material_response_batches().to_vec();
    let events = w.contact_events().to_vec();
    let diagnostics = (w.loading_diagnostics(), w.material_response_diagnostics());
    let result = Staged::prepare(w, policy, 2).and_then(run);
    assert_eq!(result.err(), Some(expected));
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading_batches(), batches);
    assert_eq!(w.material_response_batches(), responses);
    assert_eq!(w.contact_events(), events);
    assert_eq!(
        (w.loading_diagnostics(), w.material_response_diagnostics()),
        diagnostics
    );
}

#[test]
fn gravity_stack_support_combined_load_failed_regions_and_owned_motion_refuse() {
    let mut overloaded = world(0.0, 1, 1.0);
    overloaded.set_gravity(Vec2::X * 200.0);
    Staged::prepare(&overloaded, GravitySettling::BoundedNumerical, 2).unwrap();
    refused(
        &overloaded,
        GravitySettling::BoundedNumerical,
        |s| s.hold(0.01, 8, 256, 2).map(|(s, _, _)| s),
        Rejection::UnsafeSupport,
    );
    let landing = returned(GravitySettling::BoundedNumerical, 200.0);
    refused(
        &landing,
        GravitySettling::BoundedNumerical,
        |s| s.settle(256).map(|(s, _)| s),
        Rejection::UnsafeSupport,
    );
    for side in 0..3 {
        let mut w = world(0.0, 4, 1.0);
        let token = *w
            .loading
            .as_ref()
            .unwrap()
            .response
            .as_ref()
            .unwrap()
            .materials
            .keys()
            .nth(side)
            .unwrap();
        w.loading
            .as_mut()
            .unwrap()
            .response
            .as_mut()
            .unwrap()
            .materials
            .get_mut(&token)
            .unwrap()
            .remaining = 0.0;
        refused(
            &w,
            GravitySettling::Refuse,
            |s| s.hold(0.01, 8, 256, 2).map(|(s, _, _)| s),
            Rejection::FailedRegion,
        );
    }
    let moving = world(0.01, 1, 1.0);
    refused(
        &moving,
        GravitySettling::BoundedNumerical,
        |s| s.hold(0.01, 8, 256, 2).map(|(s, _, _)| s),
        Rejection::UnsafeSupport,
    );
    let mut owner = returned(GravitySettling::BoundedNumerical, 10.0);
    let handle = owner.body_handle(body(11)).unwrap();
    owner.raw.bodies[handle].set_linvel(Vector::ZERO, true);
    active_mut(&mut owner).plates[0].endpoint.velocity[0] = 0.0;
    refused(
        &owner,
        GravitySettling::BoundedNumerical,
        |s| s.hold(0.01, 8, 256, 2).map(|(s, _, _)| s),
        Rejection::UnsafeSupport,
    );
}

#[test]
fn gravity_stack_support_limits_discard_bootstrap_capture_and_late_projection() {
    let w = world(0.0, 1, 1.0);
    let (accepted, support, _) = Staged::prepare(&w, GravitySettling::BoundedNumerical, 2)
        .unwrap()
        .hold(0.01, 8, 256, 2)
        .unwrap();
    assert!(support.solves > 2);
    for (intervals, projections, captures, expected) in [
        (0, 256, 2, Rejection::AdvanceLimit),
        (8, 256, 1, Rejection::SolveLimit),
        (8, 0, 2, Rejection::SolveLimit),
        (8, 1, 2, Rejection::SolveLimit),
        (8, support.solves - 1, 2, Rejection::SolveLimit),
    ] {
        refused(
            &w,
            GravitySettling::BoundedNumerical,
            |s| {
                s.hold(0.01, intervals, projections, captures)
                    .map(|(s, _, _)| s)
            },
            expected,
        );
    }
    for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        refused(
            &w,
            GravitySettling::BoundedNumerical,
            |s| s.hold(dt, 8, 256, 2).map(|(s, _, _)| s),
            Rejection::InvalidTimestep,
        );
    }
    let mut limited = w.clone();
    limited
        .loading
        .as_mut()
        .unwrap()
        .config
        .max_records_per_step = accepted.world.loading.as_ref().unwrap().records - 1;
    refused(
        &limited,
        GravitySettling::BoundedNumerical,
        |s| s.hold(0.01, 8, 256, 2).map(|(s, _, _)| s),
        Rejection::RecordLimit,
    );
    refused(
        &w,
        GravitySettling::BoundedNumerical,
        |mut s| {
            for _ in 0..8 {
                (s, _, _) = s.hold(0.001, usize::MAX, usize::MAX, usize::MAX)?;
            }
            s.hold(0.001, usize::MAX, usize::MAX, usize::MAX)
                .map(|(s, _, _)| s)
        },
        Rejection::AdvanceLimit,
    );
}

#[test]
fn gravity_stack_support_projection_limits_and_errors_restore_solver_settings() {
    for warm in [0.0, 0.5, 1.0] {
        for case in 0..5 {
            let mut s = Staged::prepare(&world(0.0, 1, warm), GravitySettling::BoundedNumerical, 2)
                .unwrap();
            s.capture_stack(2).unwrap();
            for i in 0..2 {
                s.world.raw.bodies[s.scene.plates.bodies[i]].set_linvel(Vector::X * 0.05, true);
            }
            let config = s.world.raw.integration_parameters;
            let gravity = s.world.raw.gravity;
            let flags: Vec<_> = s
                .world
                .raw
                .colliders
                .iter()
                .map(|(h, c)| (h, c.active_hooks()))
                .collect();
            if case == 2 {
                s.projections = MAX_PROJECTIONS - 1;
            }
            if case == 3 {
                s.world.raw.bodies[s.scene.plates.bodies[0]].set_linvel(Vector::X * -0.05, true);
            }
            if case == 4 {
                s.world.raw.bodies[s.scene.plates.bodies[0]]
                    .set_linvel(Vector::new(f32::INFINITY, 0.0), true);
            }
            let result = s.project_stack(
                if case == 0 { 0 } else { usize::MAX },
                if case == 1 { 1 } else { usize::MAX },
            );
            assert_eq!(
                result.err(),
                Some(if case >= 3 {
                    Rejection::Numerical
                } else {
                    Rejection::SolveLimit
                })
            );
            assert_eq!(s.world.raw.integration_parameters, config);
            assert_eq!(s.world.raw.gravity, gravity);
            assert!(
                flags
                    .iter()
                    .all(|(h, f)| s.world.raw.colliders[*h].active_hooks() == *f)
            );
            if case == 1 {
                assert_eq!(s.projections, 1);
            }
            if case == 2 {
                assert_eq!(s.projections, 256);
            }
        }
    }
}

#[test]
fn gravity_stack_support_settlement_refuses_unproved_excess_or_spent_energy() {
    let strict = returned(GravitySettling::Refuse, 10.0);
    refused(
        &strict,
        GravitySettling::Refuse,
        |s| s.settle(256).map(|(s, _)| s),
        Rejection::SettlingDisabled,
    );
    let w = returned(GravitySettling::BoundedNumerical, 10.0);
    for case in 0..4 {
        let mut changed = w.clone();
        let e = active_mut(&mut changed).incoming.as_mut().unwrap();
        match case {
            0 => e.turned = false,
            1 => {
                e.gap_upper = f64::from_bits(
                    (2.0 * f64::from(CaptureConfig::default().touch_tolerance)).to_bits() + 1,
                )
            }
            2 => e.energy = 0.0,
            _ => e.energy *= 0.5,
        }
        // The stored state is structurally valid; eligibility is a phase check.
        let changed =
            PhysicsWorld::from_snapshot_bytes(&changed.snapshot_bytes().unwrap()).unwrap();
        refused(
            &changed,
            GravitySettling::BoundedNumerical,
            |s| s.settle(256).map(|(s, _)| s),
            if case < 2 {
                Rejection::AmbiguousDeparture
            } else {
                Rejection::Numerical
            },
        );
    }
    let (accepted, result) = Staged::prepare(&w, GravitySettling::BoundedNumerical, 0)
        .unwrap()
        .settle(256)
        .unwrap();
    assert!(result.solves > 1);
    refused(
        &w,
        GravitySettling::BoundedNumerical,
        |s| s.settle(result.solves - 1).map(|(s, _)| s),
        Rejection::SolveLimit,
    );
    let (held, support, _) = accepted.hold(0.01, 8, 256, 0).unwrap();
    assert!(support.solves > 0);
    refused(
        &w,
        GravitySettling::BoundedNumerical,
        |s| {
            let (s, _) = s.settle(256)?;
            s.hold(0.01, 8, result.solves + support.solves - 1, 0)
                .map(|(s, _, _)| s)
        },
        Rejection::SolveLimit,
    );
    assert!(held.world.contact_events().is_empty());
    let mut record_limited = w;
    record_limited
        .loading
        .as_mut()
        .unwrap()
        .config
        .max_records_per_step = 1;
    active_mut(&mut record_limited).capture.max_records_per_step = 1;
    refused(
        &record_limited,
        GravitySettling::BoundedNumerical,
        |s| {
            let (s, _) = s.settle(256)?;
            s.hold(0.01, 8, 256, 0).map(|(s, _, _)| s)
        },
        Rejection::RecordLimit,
    );
}

#[test]
fn gravity_stack_support_nonconverging_projection_cannot_raise_the_hard_cap() {
    let mut w = world(0.0, 1, 1.0);
    // This admitted solver configuration performs no velocity iterations.
    // A caller-provided large limit must still terminate at 32 raw solves.
    w.raw.integration_parameters.num_internal_pgs_iterations = 0;
    w.raw
        .integration_parameters
        .num_internal_stabilization_iterations = 0;
    let mut s = Staged::prepare(&w, GravitySettling::BoundedNumerical, 2).unwrap();
    s.capture_stack(2).unwrap();
    for i in 0..2 {
        s.world.raw.bodies[s.scene.plates.bodies[i]].set_linvel(Vector::X * 0.05, true);
    }
    let config = s.world.raw.integration_parameters;
    assert_eq!(
        s.project_stack(usize::MAX, usize::MAX).err(),
        Some(Rejection::SolveLimit)
    );
    assert_eq!(s.projections, 32);
    assert_eq!(s.metrics.len(), 34);
    assert_eq!(s.world.raw.integration_parameters, config);
    refused(
        &w,
        GravitySettling::BoundedNumerical,
        |s| {
            s.hold(0.01, usize::MAX, usize::MAX, usize::MAX)
                .map(|(s, _, _)| s)
        },
        Rejection::SolveLimit,
    );
}
