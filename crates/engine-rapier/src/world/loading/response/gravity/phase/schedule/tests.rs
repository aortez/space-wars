use super::*;
use crate::world::loading::response::gravity::tests::world;

mod support;

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}

fn moving(gap: f32, speed: f32, acceleration: f32) -> PhysicsWorld {
    let mut w = world();
    w.set_pose(body(11), Vec2::new(-gap, 0.0), 0.0, true);
    w.set_velocity(body(11), Vec2::X * speed, 0.0, true);
    w.set_gravity(Vec2::X * acceleration);
    w
}

fn replayed(w: &mut PhysicsWorld, dt: f32) -> Trace {
    let mut replay = w.clone();
    let trace = step(w, dt, GravitySettling::Refuse).unwrap();
    let restored = step(&mut replay, dt, GravitySettling::Refuse).unwrap();
    assert_eq!(trace.durations, restored.durations);
    assert_eq!(trace.records, restored.records);
    assert_eq!(trace.numerical, restored.numerical);
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
    assert!(trace.durations.iter().all(|d| *d > 0.0));
    assert!(
        trace
            .records
            .windows(2)
            .all(|r| r[0].at <= r[1].at && r[0].phase <= r[1].phase)
    );
    assert!(trace.wall_time >= trace.advances.iter().map(|m| m.wall_time).sum());
    for record in &trace.records {
        let GravityRoles::Pair { bodies, sources } = record.roles else {
            panic!("pair roles")
        };
        assert_eq!(bodies, [body(11), body(2)]);
        assert!(sources[0] > sources[1]);
        if let Record::Episode(event) = record.record {
            assert_eq!(event.sources, [sources[1], sources[0]]);
        }
    }
    assert!(
        trace
            .records
            .iter()
            .all(|r| r.at <= f64::from(dt) && r.phase < trace.advances.len())
    );
    trace
}

fn refused(w: &mut PhysicsWorld, dt: f32, intervals: usize, impacts: usize, reason: Rejection) {
    let bytes = w.snapshot_bytes().unwrap();
    let loading = w.loading_batches().to_vec();
    let response = w.material_response_batches().to_vec();
    let diagnostics = (w.loading_diagnostics(), w.material_response_diagnostics());
    let events = w.contact_events().to_vec();
    assert_eq!(
        with_limits(w, dt, GravitySettling::Refuse, intervals, impacts).unwrap_err(),
        reason
    );
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading_batches(), loading);
    assert_eq!(w.material_response_batches(), response);
    assert_eq!(
        (w.loading_diagnostics(), w.material_response_diagnostics()),
        diagnostics
    );
    assert_eq!(w.contact_events(), events);
}

#[test]
fn gravity_schedule_retains_rounded_remainders_and_rejects_lost_time() {
    let small = 2.0_f32.powi(-25);
    let mut w = moving(1.0, -small, 1.0);
    let trace = replayed(&mut w, 1.0);
    // 1-small rounds up to 1 in f32. Consume its lower neighbor and then
    // the remaining small interval; neither overshoot nor drop that interval.
    assert_eq!(trace.durations, vec![small, 1.0 - 2.0_f32.powi(-24), small]);
    assert!(
        trace
            .records
            .iter()
            .all(|r| matches!(r.record, Record::Flight(_)))
    );
    let motion = w.motion(body(11)).unwrap();
    assert!((f64::from(motion.position.x) - (-0.5 - f64::from(small))).abs() < 2.0e-6);
    assert!((f64::from(motion.linear_velocity.x) - (1.0 - f64::from(small))).abs() < 2.0e-6);
    let mut limited = moving(1.0, -small, 1.0);
    refused(&mut limited, 1.0, 2, 8, Rejection::AdvanceLimit);
    let mut unrepresentable = moving(1.0, -2.0_f32.powi(-80), 1.0);
    refused(&mut unrepresentable, 1.0, 8, 8, Rejection::TimePrecision);

    let mut clock = time::Clock::new(1.0);
    clock.advance(0.5).unwrap();
    assert_eq!(
        clock.advance(2.0_f32.powi(-80)),
        Err(Rejection::TimePrecision)
    );
    assert_eq!(clock.elapsed(), 0.5);
    assert_eq!(clock.remaining().unwrap(), 0.5);
}

#[test]
fn gravity_schedule_frame_endpoint_and_tiny_tail_are_accounted() {
    let mut w = moving(0.015625, 0.0, 8.0);
    let trace = replayed(&mut w, 0.0625);
    assert_eq!(trace.durations, vec![0.0625]);
    assert_eq!(trace.advances.len(), 2);
    assert!(matches!(
        &trace.records.last().unwrap().record,
        Record::Impact(_)
    ));
    assert_eq!(trace.records.last().unwrap().at, 0.0625);
    let before = w.snapshot_bytes().unwrap();
    let pause = step(&mut w, 0.0, GravitySettling::BoundedNumerical).unwrap();
    assert!(pause.records.is_empty() && pause.durations.is_empty());
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    assert_eq!(pause.numerical, trace.numerical);
    replayed(&mut w, 0.001);

    let mut tiny = moving(0.016631931, 1.9916679, 1.0);
    let trace = replayed(&mut tiny, 1.0 / 120.0);
    assert_eq!(trace.durations.len(), 2);
    assert!(trace.durations[1] > 0.0 && trace.durations[1] < 2.0e-8);
    let Record::Flight(tail) = &trace.records.last().unwrap().record else {
        panic!("missing tail")
    };
    assert!(tail.errors[1] > 0.0);
    assert_ne!(trace.numerical.as_ref().unwrap().errors, [0.0; 3]);
    let stored = trace.numerical.unwrap();
    let pause = step(&mut tiny, 0.0, GravitySettling::Refuse).unwrap();
    assert_eq!(pause.numerical.as_ref(), Some(&stored));
    replayed(&mut tiny, 0.001);
}

#[test]
fn gravity_schedule_rounded_departure_rearms_only_after_proof() {
    let mut w = moving(0.0, 0.5, 10.0);
    w.raw.integration_parameters.num_solver_iterations = 4;
    let first = replayed(&mut w, 0.000002);
    let root = first
        .records
        .iter()
        .find_map(|r| match r.record {
            Record::Episode(e) if e.transition == EpisodeTransition::Impact => Some(e.id),
            _ => None,
        })
        .unwrap();
    assert_eq!(w.loading.as_ref().unwrap().episodes.len(), 1);
    let crossing = replayed(&mut w, f32::from_bits(0x383b2000));
    assert!(crossing.records.iter().any(|r| matches!(r.record, Record::Episode(e) if e.id==root && e.transition==EpisodeTransition::End)));
    assert!(w.loading.as_ref().unwrap().episodes.is_empty());
    let later = replayed(&mut w, 0.1);
    assert!(later.records.iter().any(|r| matches!(r.record, Record::Episode(e) if e.id!=root && e.transition==EpisodeTransition::Impact)));
}

#[test]
fn gravity_schedule_refuses_ambiguous_excursions() {
    let mut reference = moving(0.0, 0.5, 10.0);
    let impact = super::super::instant(&mut reference, GravitySettling::Refuse).unwrap();
    let ratio = -f64::from(impact.outgoing) / 0.5;
    let speed = (libm::sqrt(2.0 * 10.0 * 2.0e-5) / ratio) as f32;
    let mut w = moving(0.0, speed, 10.0);
    let impact = super::super::instant(&mut w, GravitySettling::Refuse).unwrap();
    let dt = apex(f64::from(impact.outgoing), 10.0).unwrap();
    replayed(&mut w, dt);
    let Status::Active(active) = &w.loading.as_ref().unwrap().gravity.as_ref().unwrap().status
    else {
        panic!()
    };
    assert!(active.episode.as_ref().unwrap().gap_upper > 2.0e-5);
    assert_eq!(w.loading.as_ref().unwrap().episodes.len(), 1);
    refused(&mut w, 0.01, 8, 8, Rejection::SettlingDisabled);
}

#[test]
fn gravity_schedule_late_limits_roll_back_damage_end_and_previous_outputs() {
    let mut prior = moving(0.0, 4.0, 10.0);
    super::super::instant(&mut prior, GravitySettling::Refuse).unwrap();
    let capture_records = prior.loading.as_ref().unwrap().records;
    for case in 0..4 {
        let mut w = moving(0.0, 4.0, 10.0);
        let old = prior.loading.as_ref().unwrap();
        let l = w.loading.as_mut().unwrap();
        l.batches = old.batches.clone();
        l.diagnostics = old.diagnostics;
        l.response.as_mut().unwrap().batches = old.response.as_ref().unwrap().batches.clone();
        l.response.as_mut().unwrap().diagnostics = old.response.as_ref().unwrap().diagnostics;
        w.contact_events = prior.contact_events.clone();
        match case {
            0 => refused(&mut w, 0.9, 0, 8, Rejection::AdvanceLimit),
            1 => refused(&mut w, 0.9, 8, 1, Rejection::SolveLimit),
            2 => {
                w.loading.as_mut().unwrap().config.max_records_per_step = capture_records;
                refused(&mut w, 0.9, 8, 8, Rejection::RecordLimit);
            }
            _ => {
                w.loading.as_mut().unwrap().config.max_batches_per_step = 1;
                refused(
                    &mut w,
                    0.9,
                    8,
                    8,
                    Rejection::Response(ResponseRejection::Capture(CaptureRejection::StepLimit)),
                );
            }
        }
    }
    for dt in [-1.0, f32::NAN, f32::INFINITY] {
        refused(&mut prior, dt, 8, 8, Rejection::InvalidTimestep);
    }
}

#[test]
fn gravity_schedule_numerical_exhaustion_survives_pause_and_restore() {
    for iterations in [1, 4] {
        let mut w = moving(1.0, 1.0, 1.0);
        w.raw.integration_parameters.num_solver_iterations = iterations;
        let mut accepted = 0;
        loop {
            let bytes = w.snapshot_bytes().unwrap();
            match step(&mut w, 5.0e-8, GravitySettling::Refuse) {
                Ok(trace) => {
                    accepted += 1;
                    let summary = trace.numerical.unwrap();
                    assert!(
                        summary
                            .errors
                            .iter()
                            .zip(summary.scales)
                            .all(|(e, s)| e.abs() <= 1.0e-5 * s)
                    );
                    w = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
                }
                Err(reason @ Rejection::NumericalAllowance { .. }) => {
                    assert!((50..500).contains(&accepted));
                    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
                    step(&mut w, 0.0, GravitySettling::Refuse).unwrap();
                    refused(&mut w, 5.0e-8, 8, 8, reason);
                    break;
                }
                Err(other) => panic!("unexpected refusal {other:?} at {accepted}"),
            }
            assert!(accepted < 500);
        }
    }
}
