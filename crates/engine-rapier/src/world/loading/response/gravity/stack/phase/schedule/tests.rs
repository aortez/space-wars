use super::*;
use crate::world::loading::response::gravity::tests::stack_world;

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}

fn moving(gap: f32, speed: f32, acceleration: f32) -> PhysicsWorld {
    let mut w = stack_world([11, 2, 7], false, [1; 3], 1);
    w.set_pose(body(11), Vec2::new(-gap, 0.0), 0.0, true);
    w.set_velocity(body(11), Vec2::X * speed, 0.0, true);
    w.set_gravity(Vec2::X * acceleration);
    w
}

fn limits() -> Limits {
    Limits {
        projections: 256,
        support_captures: 2,
        ..Limits::default()
    }
}

fn refused(w: &mut PhysicsWorld, dt: f32, limits: Limits, expected: Rejection) {
    let bytes = w.snapshot_bytes().unwrap();
    let batches = w.loading_batches().to_vec();
    let responses = w.material_response_batches().to_vec();
    let events = w.contact_events().to_vec();
    let diagnostics = (w.loading_diagnostics(), w.material_response_diagnostics());
    assert_eq!(
        with_work_limits(w, dt, GravitySettling::BoundedNumerical, limits).unwrap_err(),
        expected
    );
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
fn gravity_stack_schedule_retains_positive_tails_and_rejects_lost_time() {
    let small = 2.0_f32.powi(-25);
    let mut w = moving(1.0, -small, 1.0);
    let trace = step(&mut w, 1.0, GravitySettling::Refuse).unwrap();
    assert_eq!(trace.durations, [small, 1.0 - 2.0_f32.powi(-24), small]);
    assert_eq!(trace.advances.len(), 10); // bootstrap plus three supported flights
    refused(
        &mut moving(1.0, -small, 1.0),
        1.0,
        Limits {
            intervals: 2,
            ..limits()
        },
        Rejection::AdvanceLimit,
    );
    refused(
        &mut moving(1.0, -2.0_f32.powi(-80), 1.0),
        1.0,
        limits(),
        Rejection::TimePrecision,
    );
    let mut endpoint = moving(0.015625, 0.0, 8.0);
    let end = step(&mut endpoint, 0.0625, GravitySettling::Refuse).unwrap();
    assert_eq!(end.durations, [0.0625]);
    let hit = end
        .records
        .iter()
        .find(|r| matches!(r.record, Record::Impact(_)))
        .unwrap();
    assert_eq!((hit.at, hit.phase), (0.0625, 4));
    let mut tail = moving(0.016631931, 1.9916679, 1.0);
    let end = step(&mut tail, 1.0 / 120.0, GravitySettling::Refuse).unwrap();
    assert_eq!(end.durations.len(), 2);
    assert!(end.durations[1] > 0.0 && end.durations[1] < 2.0e-8);
}

#[test]
fn gravity_stack_schedule_late_caps_discard_damage_departure_and_prior_outputs() {
    let mut prior = moving(0.0, 4.0, 10.0);
    step(&mut prior, 0.000001, GravitySettling::BoundedNumerical).unwrap();
    for (dt, cap, reason) in [
        (
            0.9,
            Limits {
                intervals: 0,
                ..limits()
            },
            Rejection::AdvanceLimit,
        ),
        (
            0.9,
            Limits {
                impacts: 1,
                ..limits()
            },
            Rejection::SolveLimit,
        ),
        (
            0.9,
            Limits {
                projections: 3,
                ..limits()
            },
            Rejection::SolveLimit,
        ),
        (
            0.9,
            Limits {
                support_captures: 0,
                ..limits()
            },
            Rejection::SolveLimit,
        ),
        (-1.0, limits(), Rejection::InvalidTimestep),
        (f32::NAN, limits(), Rejection::InvalidTimestep),
        (f32::INFINITY, limits(), Rejection::InvalidTimestep),
    ] {
        let mut w = moving(0.0, 4.0, 10.0);
        let old = prior.loading.as_ref().unwrap();
        let l = w.loading.as_mut().unwrap();
        l.batches = old.batches.clone();
        l.diagnostics = old.diagnostics;
        l.response.as_mut().unwrap().batches = old.response.as_ref().unwrap().batches.clone();
        l.response.as_mut().unwrap().diagnostics = old.response.as_ref().unwrap().diagnostics;
        w.contact_events = prior.contact_events.clone();
        refused(&mut w, dt, cap, reason);
    }
    let mut rest = moving(0.0, 0.0, 10.0);
    refused(
        &mut rest,
        0.01,
        Limits {
            support_captures: 1,
            ..limits()
        },
        Rejection::SolveLimit,
    );
    let mut accepted = rest.clone();
    let trace = step(&mut accepted, 0.01, GravitySettling::BoundedNumerical).unwrap();
    let Record::StackSupport(s) = &trace.records.last().unwrap().record else {
        panic!()
    };
    refused(
        &mut rest,
        0.01,
        Limits {
            projections: s.solves - 1,
            ..limits()
        },
        Rejection::SolveLimit,
    );
    rest.loading.as_mut().unwrap().config.max_records_per_step =
        accepted.loading.as_ref().unwrap().records - 1;
    refused(&mut rest, 0.01, limits(), Rejection::RecordLimit);
}
