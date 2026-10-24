use super::*;

fn replayed_policy(w: &mut PhysicsWorld, dt: f32, policy: GravitySettling) -> Trace {
    let mut replay = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
    let trace = step(w, dt, policy).unwrap();
    let other = step(&mut replay, dt, policy).unwrap();
    assert_eq!(trace.records, other.records);
    assert_eq!(trace.durations, other.durations);
    assert_eq!(trace.numerical, other.numerical);
    assert_eq!(
        w.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    assert_eq!(w.loading_batches(), replay.loading_batches());
    assert_eq!(
        w.material_response_batches(),
        replay.material_response_batches()
    );
    assert_eq!(
        trace.durations.iter().map(|d| f64::from(*d)).sum::<f64>(),
        f64::from(dt)
    );
    assert!(
        trace
            .records
            .windows(2)
            .all(|r| r[0].at <= r[1].at && r[0].phase <= r[1].phase)
    );
    trace
}

fn refused_policy(
    w: &mut PhysicsWorld,
    dt: f32,
    policy: GravitySettling,
    limits: Limits,
    reason: Rejection,
) {
    let bytes = w.snapshot_bytes().unwrap();
    let loading = w.loading_batches().to_vec();
    let responses = w.material_response_batches().to_vec();
    let diagnostics = (w.loading_diagnostics(), w.material_response_diagnostics());
    let events = w.contact_events().to_vec();
    assert_eq!(with_work_limits(w, dt, policy, limits).unwrap_err(), reason);
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading_batches(), loading);
    assert_eq!(w.material_response_batches(), responses);
    assert_eq!(
        (w.loading_diagnostics(), w.material_response_diagnostics()),
        diagnostics
    );
    assert_eq!(w.contact_events(), events);
}

fn full_damping(w: &mut PhysicsWorld) {
    for r in w
        .loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials
        .values_mut()
    {
        r.parameters[4] = 1.0;
    }
}

#[test]
fn gravity_support_zero_rebound_rest_and_frame_end_preserve_policy_owner_and_history() {
    for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
        for speed in [0.0, 0.05] {
            let mut w = moving(0.0, speed, 10.0);
            full_damping(&mut w);
            let trace = replayed_policy(&mut w, 0.01, policy);
            let owner = w.loading_batches()[0].events[0].id;
            assert_eq!(
                w.material_response_batches().len(),
                usize::from(speed > 0.0)
            );
            assert!(
                matches!(&trace.records.last().unwrap().record, Record::Support(s) if s.episode==owner)
            );
            assert!(
                !trace
                    .records
                    .iter()
                    .any(|r| matches!(r.record, Record::Settlement(_)))
            );
            assert_eq!(w.motion(body(11)).unwrap().linear_velocity, Vec2::ZERO);
            assert!(w.contact_events().is_empty());
            assert!(w.clone().contact_events().is_empty());
            let history = w
                .loading
                .as_ref()
                .unwrap()
                .gravity
                .as_ref()
                .unwrap()
                .history
                .clone();
            let before = w.snapshot_bytes().unwrap();
            let other = if policy == GravitySettling::Refuse {
                GravitySettling::BoundedNumerical
            } else {
                GravitySettling::Refuse
            };
            step(&mut w, 0.0, other).unwrap();
            assert_eq!(w.snapshot_bytes().unwrap(), before);
            refused_policy(
                &mut w,
                0.01,
                other,
                Limits::default(),
                Rejection::PolicyChanged,
            );
            let held = replayed_policy(&mut w, 0.1, policy);
            assert_eq!(held.records.len(), 1);
            assert!(matches!(held.records[0].record, Record::Support(ref s) if s.episode==owner));
            assert_eq!(
                w.loading
                    .as_ref()
                    .unwrap()
                    .gravity
                    .as_ref()
                    .unwrap()
                    .history,
                history
            );
            assert!(w.loading_batches().is_empty() && w.material_response_batches().is_empty());
            // A real held endpoint does not exempt edit-and-restore invalidation.
            w.set_velocity(body(11), Vec2::X, 0.0, true);
            w.set_velocity(body(11), Vec2::ZERO, 0.0, true);
            assert!(w.clone().contact_events().is_empty());
            refused_policy(
                &mut w,
                0.01,
                policy,
                Limits::default(),
                Rejection::StaleContinuation,
            );
        }
        // Exact arrival speed equals the stored inverse mass: impulse 1 stops it.
        let mut endpoint = moving(0.0015625, 0.0, 0.8);
        full_damping(&mut endpoint);
        let trace = replayed_policy(&mut endpoint, 0.0625, policy);
        assert!(
            matches!(trace.records.last().unwrap().record, Record::Impact(ref i) if i.outgoing==0.0)
        );
        let owner = endpoint.loading_batches()[0].events[0].id;
        let next = replayed_policy(&mut endpoint, 0.01, policy);
        assert_eq!(next.records.len(), 1);
        assert!(matches!(&next.records[0].record, Record::Support(s) if s.episode==owner));
    }
}

#[test]
fn gravity_support_projection_capture_and_late_record_limits_roll_back() {
    let policy = GravitySettling::BoundedNumerical;
    let mut previous = moving(0.0, 4.0, 10.0);
    super::super::super::instant(&mut previous, policy).unwrap();
    for (speed, limits) in [
        (
            0.0,
            Limits {
                projections: 0,
                ..Limits::default()
            },
        ),
        (
            0.0,
            Limits {
                projections: 1,
                ..Limits::default()
            },
        ),
        (
            0.0,
            Limits {
                support_captures: 0,
                ..Limits::default()
            },
        ),
        (
            0.01,
            Limits {
                projections: 0,
                ..Limits::default()
            },
        ),
        (
            0.01,
            Limits {
                projections: 1,
                ..Limits::default()
            },
        ),
        (
            0.01,
            Limits {
                projections: 2,
                ..Limits::default()
            },
        ),
    ] {
        let mut w = moving(0.0, speed, 10.0);
        let old = previous.loading.as_ref().unwrap();
        let loading = w.loading.as_mut().unwrap();
        loading.batches = old.batches.clone();
        loading.diagnostics = old.diagnostics;
        loading.response.as_mut().unwrap().batches = old.response.as_ref().unwrap().batches.clone();
        loading.response.as_mut().unwrap().diagnostics = old.response.as_ref().unwrap().diagnostics;
        w.contact_events = previous.contact_events.clone();
        refused_policy(&mut w, 0.01, policy, limits, Rejection::SolveLimit);
    }
    for speed in [0.0, 0.01] {
        let mut accepted = moving(0.0, speed, 10.0);
        let trace = replayed_policy(&mut accepted, 0.01, policy);
        assert!(matches!(
            trace.records.last().unwrap().record,
            Record::Support(_)
        ));
        let records = accepted.loading.as_ref().unwrap().records;
        for unavailable in 1..=if speed == 0.0 { 1 } else { 2 } {
            let mut w = moving(0.0, speed, 10.0);
            w.loading.as_mut().unwrap().config.max_records_per_step = records - unavailable;
            refused_policy(
                &mut w,
                0.01,
                policy,
                Limits::default(),
                Rejection::RecordLimit,
            );
        }
        let mut w = moving(0.0, speed, 10.0);
        refused_policy(
            &mut w,
            0.01,
            policy,
            Limits {
                intervals: 0,
                ..Limits::default()
            },
            Rejection::AdvanceLimit,
        );
    }
}

#[test]
fn gravity_support_tiny_held_remainder_reports_its_own_reaction() {
    let mut w = moving(0.0, 0.01, 10.0);
    w.raw.integration_parameters.num_solver_iterations = 1;
    let trace = replayed_policy(&mut w, 0.01, GravitySettling::BoundedNumerical);
    let held: Vec<_> = trace
        .records
        .iter()
        .filter_map(|r| match &r.record {
            Record::Support(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(held.len(), 2);
    assert!(held[1].duration > 0.0 && held[1].duration < 1.0e-8);
    assert!(held[1].impulse > 0.0);
    for s in held {
        assert!(
            (s.impulse + s.rounding_impulse - s.force * f64::from(s.duration)).abs()
                <= 1.0e-5 * s.force * f64::from(s.duration)
        );
    }
    assert_eq!(
        trace.durations.iter().map(|d| f64::from(*d)).sum::<f64>(),
        f64::from(0.01_f32)
    );
    assert!(w.clone().contact_events().is_empty());
}

#[test]
fn gravity_support_refuses_unsafe_loads_unrepresentable_kicks_and_ambiguous_returns() {
    for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
        let mut overloaded = moving(0.0, 0.0, 10_000.0);
        refused_policy(
            &mut overloaded,
            0.01,
            policy,
            Limits::default(),
            Rejection::UnsafeSupport,
        );
        let mut stopped = moving(0.0, 0.05, 10_000.0);
        full_damping(&mut stopped);
        refused_policy(
            &mut stopped,
            0.01,
            policy,
            Limits::default(),
            Rejection::UnsafeSupport,
        );
        let mut failed = moving(0.0, 0.0, 10.0);
        failed
            .loading
            .as_mut()
            .unwrap()
            .response
            .as_mut()
            .unwrap()
            .materials
            .values_mut()
            .next()
            .unwrap()
            .remaining = 0.0;
        refused_policy(
            &mut failed,
            0.01,
            policy,
            Limits::default(),
            Rejection::FailedRegion,
        );
        let mut tiny = moving(0.0, 0.0, f32::from_bits(1));
        refused_policy(
            &mut tiny,
            0.01,
            policy,
            Limits::default(),
            Rejection::Numerical,
        );
    }
    let mut small = moving(0.0, 0.01, 10.0);
    refused_policy(
        &mut small,
        0.01,
        GravitySettling::Refuse,
        Limits::default(),
        Rejection::SettlingDisabled,
    );
    let policy = GravitySettling::BoundedNumerical;
    let mut reference = moving(0.0, 0.5, 10.0);
    let response = super::super::super::instant(&mut reference, policy).unwrap();
    let ratio = -f64::from(response.outgoing) / 0.5;
    let mut ambiguous = moving(0.0, (libm::sqrt(20.0 * 2.0e-5) / ratio) as f32, 10.0);
    let impact = super::super::super::instant(&mut ambiguous, policy).unwrap();
    replayed_policy(
        &mut ambiguous,
        apex(f64::from(impact.outgoing), 10.0).unwrap(),
        policy,
    );
    refused_policy(
        &mut ambiguous,
        0.01,
        policy,
        Limits::default(),
        Rejection::AmbiguousDeparture,
    );
}
