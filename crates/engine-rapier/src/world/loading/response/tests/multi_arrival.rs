//! Fixture-only scheduling evidence for three collinear, rotation-locked balls.
//! Preview response on a disposable clone before choosing an advance duration.
//! This does not widen native admission or claim general scene completeness.
use super::*;
use rapier2d::prelude::ActiveHooks;
use std::sync::atomic::{AtomicBool, Ordering};

const TOUCH: f64 = 1.0e-6;
const MAX_ADVANCES: usize = 8;

#[derive(Debug, Clone, PartialEq)]
enum Refusal {
    InvalidTime,
    Numerical,
    Unaccounted,
    PreviewChanged,
    Response(ResponseRejection),
    Limit { applied: usize },
}

#[derive(Debug, PartialEq)]
struct Trace {
    durations: Vec<f32>,
    responses_at: Vec<f64>,
    departures: Vec<Departure>,
}

mod departure;
mod native;
mod oblique;
use departure::{Departure, DepartureTracker, retire_departures};

struct Hooks<'a> {
    loading: Mutex<&'a mut LoadingState>,
    handles: [RigidBodyHandle; 3],
    observation_dt: f32,
    after_response: Mutex<Option<[f32; 3]>>,
    invalid: AtomicBool,
}

impl PhysicsHooks for Hooks<'_> {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        context.solver_contacts.retain(|contact| {
            let velocities = [context.rigid_body1, context.rigid_body2].map(|h| {
                h.map_or(Vector::ZERO, |h| {
                    context.bodies[h].velocity_at_point(contact.point)
                })
            });
            let approach = (velocities[0] - velocities[1]).dot(*context.normal);
            if !contact.dist.is_finite() || contact.dist < -1.0e-5 || !approach.is_finite() {
                self.invalid.store(true, Ordering::Relaxed);
            }
            // Complete collinear geometry supplies the next-arrival bound.
            // Separating pairs diverge; resting contacts remain in the coupled
            // response, including contacts present before the incoming impact.
            f64::from(contact.dist) <= TOUCH && approach >= 0.0
        });
    }

    fn modify_solver_contacts_batch(&self, context: &mut SolverContactBatchContext) {
        // A preview observes contact without aging absence. Its entire state is
        // discarded; real capture alone accounts for the accepted duration.
        context.dt = self.observation_dt;
        self.loading.lock().unwrap().capture(context, false);
        *self.after_response.lock().unwrap() =
            Some(self.handles.map(|h| context.bodies()[h].linvel().x));
    }
}

fn handles(world: &PhysicsWorld) -> [RigidBodyHandle; 3] {
    [1, 2, 3].map(|n| world.body_handle(body(n)).unwrap())
}

fn velocities(world: &PhysicsWorld) -> [f32; 3] {
    handles(world).map(|h| world.raw.bodies[h].linvel().x)
}

fn advance(world: &mut PhysicsWorld, dt: f32, preview: bool) -> Result<[f32; 3], Refusal> {
    let handles = handles(world);
    let flags: Vec<_> = world
        .raw
        .colliders
        .iter()
        .map(|(h, c)| (h, c.active_hooks()))
        .collect();
    for (h, f) in &flags {
        world.raw.colliders[*h].set_active_hooks(*f | ActiveHooks::MODIFY_SOLVER_CONTACTS);
    }
    world.raw.integration_parameters.dt = dt;
    let hooks = Hooks {
        loading: Mutex::new(world.loading.as_mut().unwrap()),
        handles,
        observation_dt: if preview { 0.0 } else { dt },
        after_response: Mutex::new(None),
        invalid: AtomicBool::new(false),
    };
    world.raw.step_with_events(&hooks, &());
    let invalid = hooks.invalid.load(Ordering::Relaxed);
    let after_response = hooks.after_response.into_inner().unwrap().unwrap();
    for (h, f) in flags {
        world.raw.colliders[h].set_active_hooks(f);
    }
    if invalid {
        return Err(Refusal::Numerical);
    }
    if let Some(reason) = world.material_response_diagnostics().last_rejection {
        return Err(Refusal::Response(reason));
    }
    if !preview && velocities(world) != after_response {
        return Err(Refusal::Unaccounted);
    }
    Ok(after_response)
}

fn next_time(world: &PhysicsWorld, speed: [f32; 3]) -> Result<Option<f64>, Refusal> {
    let positions = handles(world).map(|h| f64::from(world.raw.bodies[h].translation().x));
    let mut next: Option<f64> = None;
    for i in 0..3 {
        for j in i + 1..3 {
            let gap = positions[j] - positions[i] - 1.0;
            let approach = f64::from(speed[i]) - f64::from(speed[j]);
            if !gap.is_finite() || gap < -1.0e-5 || !approach.is_finite() {
                return Err(Refusal::Numerical);
            }
            if approach <= 0.0 {
                continue;
            }
            if gap <= TOUCH {
                return Err(Refusal::Unaccounted);
            }
            let time = gap / approach;
            next = Some(next.map_or(time, |previous| previous.min(time)));
        }
    }
    Ok(next)
}

fn step(world: &mut PhysicsWorld, dt: f32, limit: usize) -> Result<Trace, Refusal> {
    step_with_departures(world, dt, limit, None)
}

fn step_with_departures(
    world: &mut PhysicsWorld,
    dt: f32,
    limit: usize,
    tracker: Option<&mut DepartureTracker>,
) -> Result<Trace, Refusal> {
    if !dt.is_finite() || dt < 0.0 {
        return Err(Refusal::InvalidTime);
    }
    let mut trace = Trace {
        durations: Vec::new(),
        responses_at: Vec::new(),
        departures: Vec::new(),
    };
    if dt == 0.0 {
        world.step(0.0);
        return Ok(trace);
    }
    // Construction, not a production admission checker, proves these fixtures
    // have exactly three radius-.5 balls, no rotation/forces/constraints/CCD,
    // and one source per body. Snapshot staging preserves original outputs on
    // any late failure as well as poses, capacities, identities and episodes.
    assert_eq!(world.raw.bodies.len(), 3);
    assert!((3..=12).contains(&world.raw.colliders.len()));
    let mut staged = world.clone();
    let mut staged_tracker = tracker.as_deref().cloned();
    staged.loading.as_mut().unwrap().begin_step();
    let mut elapsed = 0.0_f64;
    while elapsed < f64::from(dt) {
        if trace.durations.len() >= limit.min(MAX_ADVANCES) {
            return Err(Refusal::Limit {
                applied: staged.material_response_diagnostics().applied_batches,
            });
        }
        let remaining = (f64::from(dt) - elapsed) as f32;
        let mut preview = staged.clone();
        let velocity = advance(&mut preview, remaining, true)?;
        let time = next_time(&staged, velocity)?;
        let duration = time
            .unwrap_or(f64::from(remaining))
            .min(f64::from(remaining)) as f32;
        if !duration.is_finite() || duration <= 0.0 {
            return Err(Refusal::Numerical);
        }
        let count = staged.material_response_batches().len();
        let start = handles(&staged).map(|h| staged.raw.bodies[h].translation().x);
        let actual_velocity = advance(&mut staged, duration, false)?;
        if actual_velocity != velocity
            || staged.material_response_batches()[count..]
                .iter()
                .map(|b| &b.result)
                .collect::<Vec<_>>()
                != preview
                    .material_response_batches()
                    .iter()
                    .map(|b| &b.result)
                    .collect::<Vec<_>>()
        {
            return Err(Refusal::PreviewChanged);
        }
        if staged.material_response_batches().len() > count {
            trace.responses_at.push(elapsed);
        }
        elapsed += f64::from(duration);
        trace.durations.push(duration);
        // Check final geometry even if the next impact is exactly at frame end.
        let positions = handles(&staged).map(|h| staged.raw.bodies[h].translation());
        if positions.iter().any(|p| !p.is_finite())
            || positions
                .windows(2)
                .any(|p| f64::from(p[1].x) - f64::from(p[0].x) < 1.0 - 1.0e-5)
        {
            return Err(Refusal::Numerical);
        }
        if let Some(tracker) = &mut staged_tracker {
            trace.departures.extend(retire_departures(
                &mut staged,
                tracker,
                start,
                actual_velocity,
                duration,
                elapsed,
            )?);
        }
        // Consume the rounded f32 remainder without a sub-ULP extra solve.
        if duration == remaining {
            break;
        }
    }
    *world = staged;
    if let (Some(tracker), Some(staged)) = (tracker, staged_tracker) {
        *tracker = staged;
    }
    Ok(trace)
}

#[derive(Debug, Clone, Copy)]
enum Case {
    Sequential,
    Simultaneous,
    Supported,
}

fn fixture(case: Case, parts: u16, reverse: bool, shift: f32) -> (PhysicsWorld, Vec<RegionToken>) {
    let (mut world, tokens) = scene(parts, reverse, shift, true);
    for h in handles(&world) {
        world.raw.bodies[h].lock_rotations(true, true);
    }
    let positions = match case {
        Case::Sequential => [-0.01, 1.0, 2.002],
        Case::Simultaneous => [-0.01, 1.0, 2.01],
        Case::Supported => [-0.01, 1.0, 2.0],
    };
    for (i, position) in positions.into_iter().enumerate() {
        world.set_pose(body(i as u64 + 1), Vec2::X * position, 0.0, true);
        world.set_velocity(body(i as u64 + 1), Vec2::X * shift, 0.0, true);
    }
    if matches!(case, Case::Sequential) {
        for part in 0..parts {
            let h = world.collider_handle(collider(1, part)).unwrap();
            world.raw.colliders[h].set_mass(1.0 / f32::from(parts));
        }
        world.refresh_mass_properties(body(1));
    }
    if matches!(case, Case::Supported) {
        // Establish the resting B-C episode before A starts moving toward it.
        step(&mut world, DT, MAX_ADVANCES).unwrap();
        for (i, position) in positions.into_iter().enumerate() {
            world.set_pose(body(i as u64 + 1), Vec2::X * position, 0.0, true);
        }
    }
    world.set_velocity(body(1), Vec2::X * (shift + 40.0), 0.0, true);
    if matches!(case, Case::Simultaneous) {
        world.set_velocity(body(3), Vec2::X * (shift - 40.0), 0.0, true);
    }
    (world, tokens)
}

#[test]
fn three_body_arrivals_preserve_work_order_partition_time_and_replay() {
    for case in [Case::Sequential, Case::Simultaneous, Case::Supported] {
        let mut expected: Option<(Vec<f64>, Vec<f64>)> = None;
        for hz in [30.0, 60.0, 120.0, 240.0] {
            for shift in [0.0, 17.0] {
                for parts in [1, 2, 4] {
                    for reverse in [false, true] {
                        let (mut world, tokens) = fixture(case, parts, reverse, shift);
                        let initial = energy(&world, shift);
                        let initial_momentum = momentum(&world, shift);
                        let support = world
                            .loading_batches()
                            .iter()
                            .flat_map(|b| &b.events)
                            .find(|e| e.transition == EpisodeTransition::Support)
                            .map(|e| e.id);
                        let mut responses = Vec::new();
                        let mut times = Vec::new();
                        for frame in 0..4 {
                            let mut replay = world.clone();
                            let trace = step(&mut world, 1.0 / hz, MAX_ADVANCES)
                                .unwrap_or_else(|e| panic!("{case:?} hz={hz} shift={shift} parts={parts} reverse={reverse} frame={frame}: {e:?}"));
                            assert_eq!(trace, step(&mut replay, 1.0 / hz, MAX_ADVANCES).unwrap());
                            assert_eq!(
                                world.snapshot_bytes().unwrap(),
                                replay.snapshot_bytes().unwrap()
                            );
                            assert_eq!(world.loading_batches(), replay.loading_batches());
                            assert_eq!(
                                world.material_response_batches(),
                                replay.material_response_batches()
                            );
                            assert!(
                                (trace.durations.iter().map(|d| f64::from(*d)).sum::<f64>()
                                    - f64::from(1.0 / hz))
                                .abs()
                                    < 2.0 * f64::from(f32::EPSILON / hz)
                            );
                            assert_eq!(trace.durations.len(), world.loading_batches().len());
                            for (i, batch) in world.loading_batches().iter().enumerate() {
                                assert_eq!(batch.substep, i);
                                assert_eq!(batch.duration, trace.durations[i]);
                            }
                            times.extend(
                                trace
                                    .responses_at
                                    .into_iter()
                                    .map(|t| t + f64::from(frame as f32 / hz)),
                            );
                            responses.extend(
                                world
                                    .material_response_batches()
                                    .iter()
                                    .map(|b| b.result.as_ref().unwrap().clone()),
                            );
                        }
                        let lost: f64 = responses.iter().map(loss).sum();
                        assert!((initial - energy(&world, shift) - lost).abs() < 1.0e-5 * initial);
                        assert!((times[0] - 0.01 / 40.0).abs() * 40.0 < TOUCH);
                        assert!((momentum(&world, shift) - initial_momentum).abs() < 0.002);
                        assert!(responses.iter().all(|r| r.budget.components.len() == 1));
                        match case {
                            Case::Sequential => {
                                assert_eq!(responses.len(), 2);
                                assert!(
                                    responses
                                        .iter()
                                        .all(|r| r.roots.len() == 1 && r.impacts.len() == 1)
                                );
                                assert!(times[0] < times[1] && times[1] < 1.0 / 240.0);
                                // Independent two-body momentum/restitution
                                // solution: mass 1 strikes resting mass 20.
                                let target_speed =
                                    (1.0 + f64::from(responses[0].coefficients[0])) * 40.0 / 21.0;
                                assert!(
                                    ((times[1] - times[0]) * target_speed - 0.002).abs()
                                        < 2.0 * TOUCH
                                );
                                assert_eq!(
                                    responses[0].impacts[0].sources.map(|s| s.region),
                                    [tokens[0], tokens[1]]
                                );
                                assert_eq!(
                                    responses[1].impacts[0].sources.map(|s| s.region),
                                    [tokens[1], tokens[2]]
                                );
                                assert_eq!(
                                    responses[0].impacts[0].after[1],
                                    responses[1].impacts[0].before[0]
                                );
                            }
                            Case::Simultaneous => {
                                assert_eq!(responses.len(), 1);
                                assert_eq!(responses[0].roots.len(), 2);
                                assert_eq!(responses[0].impacts.len(), 2);
                            }
                            Case::Supported => {
                                assert_eq!(responses.len(), 1);
                                assert_eq!(responses[0].roots.len(), 1);
                                assert_eq!(responses[0].impacts.len(), 2);
                                assert_eq!(Some(responses[0].impacts[1].episode), support);
                                assert!(!responses[0].roots.contains(&support.unwrap()));
                                assert_eq!(responses[0].budget.contacts[1].closing_speed, 0.0);
                                assert!(responses[0].budget.contacts[1].energy > 0.0);
                            }
                        }
                        let capacities = balances(&world, &tokens);
                        if let Some((expected_capacities, expected_times)) = &expected {
                            for (a, b) in capacities.iter().zip(expected_capacities) {
                                assert!((a - b).abs() < 1.0e-5 * initial);
                            }
                            assert_eq!(times.len(), expected_times.len());
                            for ((a, b), response) in
                                times.iter().zip(expected_times).zip(&responses)
                            {
                                // Two flight endpoints round world positions in
                                // f32. Measure timing error as a spatial error,
                                // using the same contact tolerance at both ends.
                                let approach = response
                                    .budget
                                    .contacts
                                    .iter()
                                    .map(|c| c.closing_speed)
                                    .fold(0.0, f64::max);
                                assert!((a - b).abs() * approach < 2.0 * TOUCH);
                            }
                        } else {
                            expected = Some((capacities, times));
                        }
                    }
                }
            }
        }
    }
}

fn momentum(world: &PhysicsWorld, shift: f32) -> f64 {
    world
        .raw
        .bodies
        .iter()
        .filter(|(_, b)| b.is_dynamic())
        .map(|(_, b)| f64::from(b.mass()) * (f64::from(b.linvel().x) - f64::from(shift)))
        .sum()
}

fn assert_unchanged(
    world: &PhysicsWorld,
    snapshot: &[u8],
    loading: &[ContactBatch],
    response: &[ResponseBatch],
) {
    assert_eq!(world.snapshot_bytes().unwrap(), snapshot);
    assert_eq!(world.loading_batches(), loading);
    assert_eq!(world.material_response_batches(), response);
}

#[test]
fn three_body_limits_and_rapid_reimpact_refuse_atomically() {
    for limit in [1, 2] {
        let (mut world, _) = fixture(Case::Sequential, 1, false, 0.0);
        let before = world.snapshot_bytes().unwrap();
        assert_eq!(
            step(&mut world, DT, limit).unwrap_err(),
            Refusal::Limit { applied: limit - 1 }
        );
        assert_unchanged(&world, &before, &[], &[]);
    }
    let (mut world, _) = fixture(Case::Sequential, 1, false, 0.0);
    world.loading.as_mut().unwrap().config.max_batches_per_step = 2;
    let before = world.snapshot_bytes().unwrap();
    assert_eq!(
        step(&mut world, DT, MAX_ADVANCES).unwrap_err(),
        Refusal::Response(ResponseRejection::Capture(CaptureRejection::StepLimit))
    );
    assert_unchanged(&world, &before, &[], &[]);

    // Equal masses let A catch B again after B hits C, within the retained
    // A-B episode's separation grace. Do not silently reuse residual motion
    // or rearm the episode simply because the scheduler chose another advance.
    let (mut world, _) = fixture(Case::Sequential, 1, false, 0.0);
    let h = world.collider_handle(collider(1, 0)).unwrap();
    world.raw.colliders[h].set_mass(20.0);
    world.refresh_mass_properties(body(1));
    world.set_pose(body(3), Vec2::X * 2.02, 0.0, true);
    let before = world.snapshot_bytes().unwrap();
    assert_eq!(
        step(&mut world, DT, MAX_ADVANCES).unwrap_err(),
        Refusal::Unaccounted
    );
    assert_unchanged(&world, &before, &[], &[]);
    step(&mut world, 0.0015, MAX_ADVANCES).unwrap();
    assert_eq!(world.material_response_batches().len(), 2);
    let before = world.snapshot_bytes().unwrap();
    let loading = world.loading_batches().to_vec();
    let response = world.material_response_batches().to_vec();
    assert_eq!(
        step(&mut world, DT, MAX_ADVANCES).unwrap_err(),
        Refusal::Unaccounted
    );
    assert_unchanged(&world, &before, &loading, &response);
}

#[test]
fn three_body_endpoint_pause_and_distinct_arrivals() {
    let (mut world, tokens) = fixture(Case::Sequential, 1, false, 0.0);
    world.set_pose(body(1), -Vec2::X, 0.0, true);
    world.set_pose(body(3), Vec2::X * 3.0, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 120.0, 0.0, true);
    let before = balances(&world, &tokens);
    step(&mut world, DT, MAX_ADVANCES).unwrap();
    assert!(world.material_response_batches().is_empty());
    assert_eq!(before, balances(&world, &tokens));
    step(&mut world, DT, MAX_ADVANCES).unwrap();
    assert_eq!(world.material_response_batches().len(), 1);
    let snapshot = world.snapshot_bytes().unwrap();
    let loading = world.loading_batches().to_vec();
    let response = world.material_response_batches().to_vec();
    for dt in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            step(&mut world, dt, MAX_ADVANCES).unwrap_err(),
            Refusal::InvalidTime
        );
        assert_unchanged(&world, &snapshot, &loading, &response);
    }
    step(&mut world, 0.0, MAX_ADVANCES).unwrap();
    assert_eq!(snapshot, world.snapshot_bytes().unwrap());
    assert!(world.loading_batches().is_empty());
    assert!(world.material_response_batches().is_empty());

    let (mut world, tokens) = fixture(Case::Simultaneous, 1, false, 0.0);
    world.set_pose(body(3), Vec2::X * 2.012, 0.0, true);
    let before = balances(&world, &tokens);
    step(&mut world, 0.00026, MAX_ADVANCES).unwrap();
    let response = applied(&world);
    assert_eq!(response.roots.len(), 1);
    assert_eq!(response.impacts.len(), 1);
    assert_eq!(
        response.impacts[0].sources.map(|s| s.region),
        [tokens[0], tokens[1]]
    );
    assert_eq!(
        world.loading_material_remaining(tokens[2]).unwrap(),
        before[2]
    );
}

#[test]
fn three_body_supported_arrivals_account_prescribed_work() {
    for shift in [0.0, 17.0] {
        let (mut world, _) = fixture(Case::Supported, 2, true, shift);
        let h = world.body_handle(body(3)).unwrap();
        world.raw.bodies[h].set_body_type(
            if shift == 0.0 {
                RigidBodyType::Fixed
            } else {
                RigidBodyType::KinematicVelocityBased
            },
            true,
        );
        world.refresh_mass_properties(body(3));
        let initial = energy(&world, shift);
        let world_initial = energy(&world, 0.0);
        step(&mut world, DT, MAX_ADVANCES).unwrap();
        let response = applied(&world);
        assert_eq!(response.impacts.len(), 2);
        assert!((initial - energy(&world, shift) - loss(response)).abs() < 1.0e-5 * initial);
        assert!(
            (world_initial - energy(&world, 0.0) + response.prescribed_work - loss(response)).abs()
                < 1.0e-5 * world_initial
        );
    }
}
