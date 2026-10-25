//! Complete public scheduling and reports versus the independent physical fixture.
use super::*;
use crate::world::loading::response::gravity as public;

pub(super) fn check_report(
    world: &PhysicsWorld,
    expected: &Trace,
    step: &public::GravityStep,
    fresh: bool,
) {
    let mut actual = Trace::default();
    let mut captures = usize::from(fresh);
    let expected_sources =
        departure::bound_sources(world, &Plates::new(world).unwrap().handles).unwrap();
    for (index, entry) in step.records.iter().enumerate() {
        let public::GravityRoles::Stack { bodies, sources } = entry.roles else {
            panic!("stack roles")
        };
        assert_eq!(bodies, [body(1), body(2), body(3)]);
        assert_eq!(sources.as_slice(), expected_sources);
        assert!(entry.phase < step.advances.len());
        let at = entry.at;
        match &entry.record {
            public::GravityRecord::Flight(f) => actual.flights.push(Flight {
                duration: f.duration,
                gravity_work: f.gravity_work,
                work_error: f.errors[0],
                impulse_error: f.errors[1],
                position_error: f.errors[2],
                predicted_x: f.predicted_x,
                uncertainty: f.uncertainty,
                gap_upper: f.gap_upper,
            }),
            public::GravityRecord::Episode(e) => {
                if fresh && index == 0 {
                    assert_eq!(
                        (at, entry.phase, e.transition),
                        (0.0, 0, EpisodeTransition::Support)
                    );
                    let mut pair = [sources[1], sources[2]];
                    pair.sort();
                    assert_eq!(e.sources, pair);
                    continue;
                }
                captures += usize::from(e.transition == EpisodeTransition::Support);
                actual.events.push(TimedEvent { at, event: *e });
            }
            public::GravityRecord::Impact(i) => {
                assert_eq!(
                    world.material_response_batches()[i.response_batch]
                        .result
                        .as_ref()
                        .unwrap(),
                    &i.response
                );
                assert!(
                    matches!(&step.records[index-1].record, public::GravityRecord::Episode(e) if *e == i.event)
                );
                actual.impacts.push(Impact {
                    at,
                    incoming: i.incoming,
                    outgoing: i.outgoing,
                    response: i.response.clone(),
                });
            }
            public::GravityRecord::Support(s) => {
                let next = &step.records[index + 1];
                assert!(
                    matches!(&next.record, public::GravityRecord::Flight(f) if f.duration == s.duration)
                );
                assert_eq!(next.at, at + f64::from(s.duration));
                assert_eq!(next.phase, entry.phase + 1);
                actual.supports.push(Support {
                    at,
                    duration: s.duration,
                    episode: s.episode,
                    force: s.force,
                    impulse: s.impulse,
                    rounding_impulse: s.rounding_impulse,
                    rounding_loss: s.rounding_loss,
                    gravity_work: s.gravity_work,
                    kick_energy: s.kick_energy,
                    projection_energy: s.projection_energy,
                });
            }
            public::GravityRecord::StackSupport(s) => actual.stack_supports.push(StackSupport {
                at,
                duration: s.duration,
                episodes: s.episodes,
                forces: s.forces,
                impulses: s.impulses,
                rounding_impulses: s.rounding_impulses,
                rounding_loss: s.rounding_loss,
                gravity_work: s.gravity_work,
                kick_energy: s.kick_energy,
                projection_energy: s.projection_energy,
                solves: s.solves,
            }),
            public::GravityRecord::StackSettlement(s) => {
                actual.stack_settlements.push(StackSettlement {
                    at,
                    episodes: s.episodes,
                    available: s.available,
                    dissipated: s.dissipated,
                    impulses: s.impulses,
                    rounding_impulses: s.rounding_impulses,
                    rounding_loss: s.rounding_loss,
                    solves: s.solves,
                })
            }
            public::GravityRecord::SupportCorrection(c) => {
                assert!(matches!(
                    step.records[index - 1].record,
                    public::GravityRecord::Impact(_)
                ));
                actual.support_corrections.push(SupportCorrection {
                    at,
                    episode: c.episode,
                    impulse: c.impulse,
                    dissipated: c.dissipated,
                });
            }
            public::GravityRecord::Settlement(_) => panic!("pair settlement in stack"),
        }
    }
    assert_eq!(&actual, expected);
    assert!(
        step.records
            .windows(2)
            .all(|r| r[0].at <= r[1].at && r[0].phase <= r[1].phase)
    );
    let durations: Vec<_> = step
        .records
        .iter()
        .filter_map(|r| match &r.record {
            public::GravityRecord::Flight(f) => Some(f.duration),
            public::GravityRecord::StackSupport(s) => Some(s.duration),
            _ => None,
        })
        .collect();
    assert_eq!(durations, step.durations);
    assert_eq!(
        step.advances.len(),
        captures
            + expected.impacts.len()
            + 3 * expected.flights.len()
            + expected
                .stack_supports
                .iter()
                .map(|s| s.solves + 1)
                .sum::<usize>()
            + expected
                .stack_settlements
                .iter()
                .map(|s| s.solves)
                .sum::<usize>()
    );
}

fn frame(scene: &mut Verified, world: &mut PhysicsWorld, dt: f32) -> Trace {
    frame_with_policy(scene, world, dt, GravitySettling::BoundedNumerical)
}

pub(super) fn frame_with_policy(
    scene: &mut Verified,
    world: &mut PhysicsWorld,
    dt: f32,
    policy: GravitySettling,
) -> Trace {
    let trace = scene.step(dt, MAX_ADVANCES).unwrap();
    let mut replay = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    let fresh = world.loading.as_ref().unwrap().gravity.is_none();
    let step = world.step_material_flat_gravity(dt, policy).unwrap();
    let again = replay.step_material_flat_gravity(dt, policy).unwrap();
    check_report(world, &trace, &step, fresh);
    assert_eq!(step.records, again.records);
    assert_eq!(step.durations, again.durations);
    assert_eq!(step.numerical, again.numerical);
    assert_eq!(
        step.durations.iter().map(|d| f64::from(*d)).sum::<f64>(),
        f64::from(dt)
    );
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    assert_eq!(world.loading_batches(), replay.loading_batches());
    assert_eq!(
        world.material_response_batches(),
        replay.material_response_batches()
    );
    assert_eq!(
        motion(world, &Plates::new(world).unwrap()),
        motion(&scene.world, &Plates::new(&scene.world).unwrap())
    );
    assert_eq!(
        bincode::serialize(
            &world
                .loading
                .as_ref()
                .unwrap()
                .response
                .as_ref()
                .unwrap()
                .materials
        )
        .unwrap(),
        bincode::serialize(
            &scene
                .world
                .loading
                .as_ref()
                .unwrap()
                .response
                .as_ref()
                .unwrap()
                .materials
        )
        .unwrap()
    );
    let mut paused = world.clone();
    let numerical = paused
        .step_material_flat_gravity(0.0, policy)
        .unwrap()
        .numerical
        .unwrap();
    let corrections = numerical.support_corrections.as_ref().unwrap();
    let history = scene.witness.stack.as_ref().unwrap();
    assert_eq!(corrections.impulse, history.correction_impulse);
    assert_eq!(corrections.dissipated, history.correction_work);
    assert_eq!(corrections.impulse_bound, 1.0e-5 * numerical.scales[1]);
    assert_eq!(corrections.energy_bound, 1.0e-5 * numerical.scales[0]);
    let expected = &scene.witness.rounding;
    assert_eq!(
        numerical.scales,
        [
            expected.energy_scale,
            expected.momentum_scale,
            expected.distance_scale
        ]
    );
    assert_eq!(
        numerical.errors,
        [
            expected.work_error,
            expected.impulse_error,
            expected.position_error
        ]
    );
    assert_eq!(
        paused.snapshot_bytes().unwrap(),
        world.snapshot_bytes().unwrap()
    );
    trace
}

#[test]
fn gravity_stack_schedule_matches_independent_separated_arrivals() {
    let mut cases = 0;
    for speed in [0.5, 4.0] {
        let horizon = arrival(f64::from(0.02_f32), speed, 10.0) + 0.005;
        for n in [1, 4, 8] {
            for hz in [30.0_f32, 60.0, 120.0, 240.0] {
                for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut w, _) =
                                fixture(parts, copies, reverse, 0.02, speed as f32, 10.0);
                            w.raw.integration_parameters.num_solver_iterations = n;
                            let mut actual = unenrolled(&w);
                            let mut scene = Verified::with_stack(w, true).unwrap();
                            let mut elapsed = 0.0;
                            let mut hits = 0;
                            let mut ends = 0;
                            loop {
                                let remaining = (horizon - elapsed) as f32;
                                let dt = remaining.min(1.0 / hz);
                                let trace = frame(&mut scene, &mut actual, dt);
                                hits += trace.impacts.len();
                                ends += trace
                                    .events
                                    .iter()
                                    .filter(|e| e.event.transition == EpisodeTransition::End)
                                    .count();
                                elapsed += f64::from(dt);
                                if dt == remaining {
                                    break;
                                }
                            }
                            assert_eq!((hits, ends), (1, 1));
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 192);
}

#[test]
fn gravity_stack_flight_matches_independent_repeated_returns_and_replay() {
    let mut cases = 0;
    for speed in [0.5, 4.0] {
        let (w, _) = fixture(&[1, 1, 1], 1, false, 0.0, speed, 10.0);
        let mut reference = Verified::with_stack(w, true).unwrap();
        let first = reference.step(0.000001, MAX_ADVANCES).unwrap();
        let v1 = f64::from(first.impacts[0].outgoing);
        let (x, v, _) = state(&reference.world);
        let second = reference
            .step((arrival(-x, v, 10.0) + 0.000001) as f32, MAX_ADVANCES)
            .unwrap();
        let v2 = f64::from(second.impacts[0].outgoing);
        let (x, v, _) = state(&reference.world);
        let third = reference
            .step((arrival(-x, v, 10.0) + 0.000001) as f32, MAX_ADVANCES)
            .unwrap();
        let horizon = -2.0 * (v1 + v2) / 10.0 - f64::from(third.impacts[0].outgoing) / 20.0;
        for n in [1, 4, 8] {
            for hz in [0.0_f32, 30.0, 60.0, 120.0, 240.0] {
                for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut w, _) = fixture(parts, copies, reverse, 0.0, speed, 10.0);
                            w.raw.integration_parameters.num_solver_iterations = n;
                            let mut native = unenrolled(&w);
                            let mut scene = Verified::with_stack(w, true).unwrap();
                            let mut elapsed = 0.0;
                            let mut hits = 0;
                            let mut ends = 0;
                            loop {
                                let remaining = (horizon - elapsed) as f32;
                                let dt = if hz == 0.0 {
                                    remaining
                                } else {
                                    remaining.min(1.0 / hz)
                                };
                                let trace = frame(&mut scene, &mut native, dt);
                                hits += trace.impacts.len();
                                ends += trace
                                    .events
                                    .iter()
                                    .filter(|e| e.event.transition == EpisodeTransition::End)
                                    .count();
                                elapsed += f64::from(dt);
                                if dt == remaining {
                                    break;
                                }
                            }
                            assert_eq!((hits, ends), (3, 3));
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 240);
}

#[test]
fn gravity_stack_flight_retains_rounded_departure_across_pause_and_restore() {
    let (mut w, _) = fixture(&[1, 1, 1], 1, false, 0.0, 0.6, 10.0);
    w.raw.integration_parameters.num_solver_iterations = 1;
    let mut native = unenrolled(&w);
    let mut scene = Verified::with_stack(w, true).unwrap();
    let first = frame(&mut scene, &mut native, 2.0e-6);
    let owner = first.impacts[0].response.roots[0];
    let crossed = frame(&mut scene, &mut native, f32::from_bits(0x3811c540));
    assert!(crossed.events.is_empty());
    let flight = &crossed.flights[0];
    let exit = 2.0 * f64::from(native.loading.as_ref().unwrap().config.touch_tolerance);
    assert!(-state(&native).0 > exit + flight.uncertainty);
    assert!(-flight.predicted_x <= exit + flight.uncertainty);
    let bytes = native.snapshot_bytes().unwrap();
    native
        .step_material_flat_gravity(0.0, GravitySettling::BoundedNumerical)
        .unwrap();
    assert_eq!(native.snapshot_bytes().unwrap(), bytes);
    native = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let departed = frame(&mut scene, &mut native, 0.0001);
    assert_eq!(departed.events.len(), 1);
    assert_eq!(departed.events[0].event.id, owner);
    assert_eq!(departed.events[0].event.transition, EpisodeTransition::End);
    let returned = frame(&mut scene, &mut native, 0.1198);
    assert_eq!(returned.impacts.len(), 1);
    assert!(returned.impacts[0].response.roots[0] > owner);
    assert_eq!(
        returned.impacts[0].response.impacts[1].episode,
        scene.witness.support
    );
}

#[test]
fn gravity_stack_flight_rounded_endpoint_arrivals_preserve_incoming_budget() {
    for (gap, dt) in [(0.0001, 0.0003996805), (0.02, 0.07015621), (1.0, 1.1861407)] {
        let (mut w, _) = fixture(&[1, 1, 1], 1, false, gap, 0.25, 1.0);
        w.raw.integration_parameters.num_solver_iterations = 1;
        let mut native = unenrolled(&w);
        let mut scene = Verified::with_stack(w, true).unwrap();
        assert!(arrival(f64::from(gap), 0.25, 1.0) > f64::from(dt));
        let trace = frame(&mut scene, &mut native, dt);
        assert_eq!(trace.impacts.len(), 1);
        assert_eq!(trace.impacts[0].at, f64::from(dt));
        assert!(state(&native).1 < 0.0);
    }
}
