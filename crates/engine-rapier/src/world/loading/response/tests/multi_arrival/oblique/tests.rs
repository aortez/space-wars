use super::*;
use crate::world::loading::response::ball_arrival::{BallArrivalRejection, BallEventPhase};

fn kinetic(world: &PhysicsWorld, shift: Vec2) -> f64 {
    world
        .raw
        .bodies
        .iter()
        .filter(|(_, b)| b.is_dynamic())
        .map(|(_, b)| {
            let v = delta(b.linvel(), to_rapier(shift));
            0.5 * f64::from(b.mass()) * dot(v, v)
        })
        .sum()
}

fn momentum(world: &PhysicsWorld, shift: Vec2) -> [f64; 2] {
    let mut result = [0.0; 2];
    for (_, b) in world.raw.bodies.iter().filter(|(_, b)| b.is_dynamic()) {
        let v = delta(b.linvel(), to_rapier(shift));
        for axis in 0..2 {
            result[axis] += f64::from(b.mass()) * v[axis];
        }
    }
    result
}

#[test]
fn oblique_arrivals_preserve_work_order_partition_and_replay() {
    for case in [Layout::Sequential, Layout::Simultaneous, Layout::Rapid] {
        let mut reference: Option<Vec<f64>> = None;
        for hz in [30.0, 60.0, 120.0, 240.0] {
            for angle in [0.0, 0.4, 1.4, 2.8, -1.1] {
                for shift in [Vec2::ZERO, Vec2::new(17.0, -11.0)] {
                    for parts in [1, 2, 4] {
                        for reverse in [false, true] {
                            let (mut w, tokens) = fixture(case, parts, reverse, angle, shift);
                            let mut native = w.world.clone();
                            let initial = kinetic(&w, shift);
                            let initial_p = momentum(&w, shift);
                            let mut responses = Vec::new();
                            let mut events = Vec::new();
                            let mut elapsed = 0.0;
                            for frame in 0..8 {
                                let mut replay = Experiment::restore(&w.snapshot());
                                let dt = 1.0 / hz;
                                let trace = w.step(dt, MAX_ADVANCES).unwrap_or_else(|e|
                                    panic!("{case:?} hz={hz} angle={angle} shift={shift:?} parts={parts} reverse={reverse} frame={frame}: {e:?}"));
                                assert_eq!(trace, replay.step(dt, MAX_ADVANCES).unwrap());
                                let mut native_replay = native.clone();
                                let actual = native.step_material_three_ball_arrivals(dt).unwrap_or_else(|e|
                                    panic!("native {case:?} hz={hz} angle={angle} shift={shift:?} parts={parts} reverse={reverse} frame={frame}: {e:?}"));
                                let repeated =
                                    native_replay.step_material_three_ball_arrivals(dt).unwrap();
                                assert_eq!(actual.durations, trace.durations);
                                assert_eq!(actual.durations, repeated.durations);
                                assert_eq!(actual.events, repeated.events);
                                assert_eq!(
                                    native.snapshot_bytes().unwrap(),
                                    native_replay.snapshot_bytes().unwrap()
                                );
                                assert_eq!(
                                    actual
                                        .events
                                        .iter()
                                        .map(|e| Observation {
                                            event: e.event,
                                            at: e.time_seconds,
                                            after_drift: e.phase == BallEventPhase::AfterDrift,
                                        })
                                        .collect::<Vec<_>>(),
                                    trace.events
                                );
                                assert_eq!(native.loading_batches(), w.loading_batches());
                                assert_eq!(
                                    native.material_response_batches(),
                                    w.material_response_batches()
                                );
                                assert_eq!(balances(&native, &tokens), balances(&w, &tokens));
                                for n in 1..=3 {
                                    assert_eq!(native.motion(body(n)), w.motion(body(n)));
                                }
                                assert_eq!(w.snapshot(), replay.snapshot());
                                assert_eq!(w.loading_batches(), replay.loading_batches());
                                assert_eq!(
                                    w.material_response_batches(),
                                    replay.material_response_batches()
                                );
                                assert_eq!(trace.durations.len(), w.loading_batches().len());
                                assert!(trace.durations.len() <= MAX_ADVANCES);
                                let duration: f64 =
                                    trace.durations.iter().map(|t| f64::from(*t)).sum();
                                assert!(
                                    (duration - f64::from(dt)).abs()
                                        < 2.0 * f64::from(f32::EPSILON * dt)
                                );
                                for (i, batch) in w.loading_batches().iter().enumerate() {
                                    assert_eq!(batch.substep, i);
                                    assert_eq!(batch.duration, trace.durations[i]);
                                }
                                events.extend(trace.events.into_iter().map(|mut e| {
                                    e.at += elapsed;
                                    e
                                }));
                                elapsed += f64::from(dt);
                                responses.extend(
                                    w.material_response_batches()
                                        .iter()
                                        .map(|b| b.result.as_ref().unwrap().clone()),
                                );
                            }
                            let expected = match case {
                                Layout::Sequential => 2,
                                Layout::Simultaneous => 3,
                                Layout::Rapid => 4,
                            };
                            assert_eq!(
                                responses.iter().map(|r| r.roots.len()).sum::<usize>(),
                                expected,
                                "{case:?} hz={hz} angle={angle} shift={shift:?} parts={parts} reverse={reverse}: {events:?}"
                            );
                            assert_eq!(
                                responses.len(),
                                if matches!(case, Layout::Simultaneous) {
                                    2
                                } else {
                                    expected
                                }
                            );
                            let starts: Vec<_> = events
                                .iter()
                                .filter(|e| e.event.transition == EpisodeTransition::Impact)
                                .collect();
                            let ends: Vec<_> = events
                                .iter()
                                .filter(|e| e.event.transition == EpisodeTransition::End)
                                .collect();
                            assert_eq!(starts.len(), expected);
                            assert_eq!(ends.len(), expected);
                            for (i, start) in starts.iter().enumerate() {
                                let pair = start.event.sources.map(|s| s.region.id.entity);
                                assert_eq!(
                                    pair,
                                    if matches!(case, Layout::Simultaneous) && i == 2 {
                                        [PhysicsId::new(1), PhysicsId::new(3)]
                                    } else if i % 2 == 0 {
                                        [PhysicsId::new(1), PhysicsId::new(2)]
                                    } else {
                                        [PhysicsId::new(2), PhysicsId::new(3)]
                                    }
                                );
                                let end = ends
                                    .iter()
                                    .find(|end| end.event.id == start.event.id)
                                    .unwrap();
                                assert!(end.at > start.at);
                            }
                            assert!(events.windows(2).all(|e| e[0].at <= e[1].at));
                            let lost: f64 = responses.iter().map(loss).sum();
                            assert!((initial - kinetic(&w, shift) - lost).abs() < initial * 1.0e-5);
                            let final_p = momentum(&w, shift);
                            assert!(
                                (0..2).all(|axis| (final_p[axis] - initial_p[axis]).abs() < 0.002)
                            );
                            let remaining = balances(&w, &tokens);
                            if let Some(reference) = &reference {
                                for (a, b) in remaining.iter().zip(reference) {
                                    assert!(
                                        (a - b).abs() < 1.0e-5 * initial,
                                        "{case:?} balance {a} != {b}"
                                    );
                                }
                            } else {
                                reference = Some(remaining);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn oblique_arrival_times_match_independent_bracketing_and_refuse_grazing() {
    for y in [0.0, 0.2, 0.7, 0.99] {
        let p = [Vector::new(-2.0, y), Vector::ZERO, Vector::new(10.0, 10.0)];
        let v = [Vector::X * 40.0, Vector::ZERO, Vector::ZERO];
        let time = next_time(p, v).unwrap().unwrap();
        // Independently bisect the physical distance crossing, without using
        // the quadratic/discriminant or the scheduler's candidate formula.
        let (mut lo, mut hi) = (0.0, 0.05);
        for _ in 0..80 {
            let middle = (lo + hi) * 0.5;
            if libm::hypot(2.0 - 40.0 * middle, f64::from(y)) > 1.0 {
                lo = middle;
            } else {
                hi = middle;
            }
        }
        assert!((time - hi).abs() < 1.0e-14);
    }
    for y in [1.0 - 1.0e-6, 1.0, 1.0 + 1.0e-6] {
        let p = [Vector::new(-2.0, y), Vector::ZERO, Vector::new(10.0, 10.0)];
        assert_eq!(
            next_time(p, [Vector::X * 40.0, Vector::ZERO, Vector::ZERO]),
            Err(Refusal::Numerical)
        );
    }
    let miss = [
        Vector::new(-2.0, 1.01),
        Vector::ZERO,
        Vector::new(10.0, 10.0),
    ];
    assert_eq!(
        next_time(miss, [Vector::X * 40.0, Vector::ZERO, Vector::ZERO]),
        Ok(None)
    );
    let touch = [
        Vector::new(-1.0, 0.0),
        Vector::ZERO,
        Vector::new(10.0, 10.0),
    ];
    assert_eq!(
        next_time(touch, [Vector::X * 0.00005, Vector::ZERO, Vector::ZERO]),
        Err(Refusal::Unaccounted)
    );
    assert_eq!(
        next_time(touch, [-Vector::X, Vector::ZERO, Vector::ZERO]),
        Ok(None)
    );
}

#[test]
fn oblique_native_preserves_refusals_outputs_pause_and_line_admission() {
    use crate::world::loading::response::arrival::ArrivalRejection;
    use BallArrivalRejection as R;
    for case in 0..11 {
        let (experiment, _) = fixture(Layout::Rapid, 1, false, 0.4, Vec2::new(17.0, -11.0));
        let mut w = experiment.world;
        let arrival = next_time(positions(&w), handles(&w).map(|h| w.raw.bodies[h].linvel()))
            .unwrap()
            .unwrap() as f32;
        // Preserve real previous output even when a later call refuses.
        w.step_material_three_ball_arrivals(arrival).unwrap();
        w.step_material_three_ball_arrivals(5562.0e-9).unwrap();
        assert!(!w.material_response_batches().is_empty());
        let mut expected = R::Admission(ArrivalRejection::UnsupportedScene);
        let h = w.body_handle(body(1)).unwrap();
        let c = w.collider_handle(collider(1, 0)).unwrap();
        match case {
            0 => w.raw.bodies[h].lock_rotations(false, true),
            1 => w.raw.colliders[c].set_friction(0.1),
            2 => {
                w.set_gravity(Vec2::Y);
            }
            3 => w.raw.bodies[h].enable_ccd(true),
            4 => w.raw.colliders[c].set_shape(SharedShape::ball(0.6)),
            5 => w.raw.colliders[c].set_translation_wrt_parent(Vector::X * 0.01),
            6 => {
                w.insert_body(
                    body(4),
                    BodySpec::default(),
                    &[ColliderSpec::ball(collider(4, 0), 0.5)],
                );
            }
            7 => w.raw.bodies[h].add_force(Vector::Y, true),
            8 => {
                w.loading.as_mut().unwrap().config.max_batches_per_step = 1;
                expected = R::Response(ResponseRejection::Capture(CaptureRejection::StepLimit));
            }
            9 => {
                expected = R::InvalidTimestep;
            }
            _ => {
                expected = R::AdvanceLimit;
            }
        }
        let before = w.snapshot_bytes().unwrap();
        let loading = w.loading_batches().to_vec();
        let response = w.material_response_batches().to_vec();
        let result = match case {
            9 => w.step_material_three_ball_arrivals(f32::NAN),
            10 => w.step_balls_with_limit(0.003, 1, false),
            _ => w.step_material_three_ball_arrivals(0.003),
        };
        assert_eq!(result.unwrap_err(), expected, "case {case}");
        assert_unchanged(&w, &before, &loading, &response);
    }
    let (experiment, _) = fixture(Layout::Rapid, 1, false, 0.4, Vec2::ZERO);
    let mut w = experiment.world;
    let before = w.snapshot_bytes().unwrap();
    assert_eq!(
        w.step_material_line_arrivals(DT).unwrap_err(),
        R::Admission(ArrivalRejection::UnsupportedScene)
    );
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    let arrival = next_time(positions(&w), handles(&w).map(|h| w.raw.bodies[h].linvel()))
        .unwrap()
        .unwrap() as f32;
    assert!(
        w.step_material_three_ball_arrivals(arrival)
            .unwrap()
            .events
            .is_empty()
    );
    let next = w.step_material_three_ball_arrivals(1.0e-6).unwrap();
    assert_eq!(next.events[0].phase, BallEventPhase::BeforeSolve);
    assert_eq!(next.events[0].time_seconds, 0.0);
    let before = w.snapshot_bytes().unwrap();
    let paused = w.step_material_three_ball_arrivals(0.0).unwrap();
    assert!(paused.events.is_empty() && paused.advances.is_empty());
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    assert!(w.loading_batches().is_empty());
}

#[test]
fn oblique_native_grazing_and_numerical_energy_refuse_atomically() {
    for y in [1.0 - 1.0e-6, 1.0, 1.0 + 1.0e-6] {
        let (experiment, _) = fixture(Layout::Sequential, 1, false, 0.0, Vec2::ZERO);
        let mut w = experiment.world;
        for (n, p) in [
            (1, Vec2::new(-2.0, y)),
            (2, Vec2::ZERO),
            (3, Vec2::new(10.0, 10.0)),
        ] {
            w.set_pose(body(n), p, 0.0, true);
        }
        let before = w.snapshot_bytes().unwrap();
        assert_eq!(
            w.step_material_three_ball_arrivals(0.1).unwrap_err(),
            BallArrivalRejection::Numerical
        );
        assert_unchanged(&w, &before, &[], &[]);
    }
    for shift in [-1_000_000.0, 1_000_000.0] {
        let (experiment, _) = fixture(Layout::Rapid, 1, false, 0.0, Vec2::ZERO);
        let mut w = experiment.world;
        for (n, p) in [(1, Vec2::ZERO), (2, Vec2::X), (3, Vec2::X * 3.0)] {
            w.set_pose(body(n), p, 0.0, true);
            w.set_velocity(
                body(n),
                Vec2::new(shift + if n == 1 { 0.25 } else { 0.0 }, -shift),
                0.0,
                true,
            );
        }
        let before = w.snapshot_bytes().unwrap();
        assert_eq!(
            w.step_material_three_ball_arrivals(DT).unwrap_err(),
            BallArrivalRejection::Response(ResponseRejection::Backend)
        );
        assert_unchanged(&w, &before, &[], &[]);
    }
}

#[test]
fn oblique_native_support_work_and_simultaneous_end_overflow() {
    for shift in [Vec2::ZERO, Vec2::new(17.0, -11.0)] {
        let (experiment, _) = fixture(Layout::Sequential, 2, true, 0.4, shift);
        let mut w = experiment.world;
        let h = w.body_handle(body(3)).unwrap();
        w.raw.bodies[h].set_body_type(
            if shift == Vec2::ZERO {
                RigidBodyType::Fixed
            } else {
                RigidBodyType::KinematicVelocityBased
            },
            true,
        );
        w.refresh_mass_properties(body(3));
        let initial = kinetic(&w, shift);
        let global = kinetic(&w, Vec2::ZERO);
        let result = w.step_material_three_ball_arrivals(1.0 / 30.0).unwrap();
        assert!(result.advances.len() <= MAX_ADVANCES);
        assert_eq!(result.advances.len(), result.previews.len());
        let backend: Duration = result
            .advances
            .iter()
            .chain(&result.previews)
            .map(|m| m.wall_time)
            .sum();
        assert!(result.wall_time >= backend);
        let responses: Vec<_> = w
            .material_response_batches()
            .iter()
            .map(|r| r.result.as_ref().unwrap())
            .collect();
        assert!(responses.len() >= 2);
        let lost: f64 = responses.iter().map(|r| loss(r)).sum();
        let driven: f64 = responses.iter().map(|r| r.prescribed_work).sum();
        assert!((initial - kinetic(&w, shift) - lost).abs() < 1.0e-5 * initial);
        assert!((global - kinetic(&w, Vec2::ZERO) + driven - lost).abs() < 1.0e-5 * initial);
    }
    for cap in [5, 6] {
        let (experiment, _) = fixture(Layout::Simultaneous, 1, true, 0.4, Vec2::ZERO);
        let mut w = experiment.world;
        w.loading.as_mut().unwrap().config.max_records_per_step = cap;
        let before = w.snapshot_bytes().unwrap();
        let result = w.step_material_three_ball_arrivals(0.001);
        if cap == 5 {
            assert_eq!(
                result.unwrap_err(),
                BallArrivalRejection::Response(ResponseRejection::Capture(
                    CaptureRejection::StepLimit
                ))
            );
            assert_unchanged(&w, &before, &[], &[]);
        } else {
            let result = result.unwrap();
            assert_eq!(result.events.len(), 4);
            assert_eq!(result.events[2].phase, BallEventPhase::AfterDrift);
            assert_eq!(result.events[2].time_seconds, result.events[3].time_seconds);
            assert!(result.events[2].event.sources < result.events[3].event.sources);
        }
    }
}

#[test]
fn oblique_fixed_support_return_without_clearance_refuses_atomically() {
    // With equal masses and fixed C the successive returns accelerate. After
    // five impacts B returns to C around 2.214 ms without proved departure of
    // that episode. An unspent incoming budget cannot be invented from a turn.
    let (mut experiment, _) = fixture(Layout::Rapid, 2, true, 0.4, Vec2::ZERO);
    let h = experiment.body_handle(body(3)).unwrap();
    experiment.raw.bodies[h].set_body_type(RigidBodyType::Fixed, true);
    experiment.refresh_mass_properties(body(3));
    let mut native = experiment.world.clone();
    let before = native.snapshot_bytes().unwrap();
    let fixture_before = experiment.snapshot();
    assert_eq!(experiment.step(DT, MAX_ADVANCES), Err(Refusal::Unaccounted));
    assert_eq!(experiment.snapshot(), fixture_before);
    assert_eq!(
        native.step_material_three_ball_arrivals(DT).unwrap_err(),
        BallArrivalRejection::Unaccounted
    );
    assert_unchanged(&native, &before, &[], &[]);
}
