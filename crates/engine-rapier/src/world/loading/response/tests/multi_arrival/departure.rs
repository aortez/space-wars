//! Verified departure evidence; deliberately confined to the collinear fixture.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingDeparture {
    episode: EpisodeId,
    endpoints: [f32; 2],
}

/// At most one pending witness per retained source-pair episode. This belongs
/// to the fixture's authoritative snapshot, not a transient solver cache.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct DepartureTracker {
    pending: BTreeMap<[ContactSource; 2], PendingDeparture>,
}

#[derive(Debug, PartialEq)]
pub(super) struct Departure {
    event: EpisodeEvent,
    at: f64,
    clearance_lower_bound: f64,
}

/// Fixture-only evidence from a completed, verified constant-velocity flight.
/// No cached episode is cleared just because a response or substep completed.
pub(super) fn retire_departures(
    world: &mut PhysicsWorld,
    tracker: &mut DepartureTracker,
    start: [f32; 3],
    speed: [f32; 3],
    dt: f32,
    at: f64,
) -> Result<Vec<Departure>, Refusal> {
    if !dt.is_finite()
        || dt <= 0.0
        || !at.is_finite()
        || start.iter().chain(&speed).any(|v| !v.is_finite())
    {
        return Err(Refusal::Numerical);
    }
    let end = handles(world).map(|h| world.raw.bodies[h].translation().x);
    let predicted: [f64; 3] =
        std::array::from_fn(|i| f64::from(start[i]) + f64::from(speed[i]) * f64::from(dt));
    // Conservative f32 integration/position allowance. Check actual endpoints
    // against the predicted motion, and subtract both bodies' uncertainty from
    // the gap proof. Transport/large coordinates can only make proof harder.
    let error: [f64; 3] = std::array::from_fn(|i| {
        8.0 * f64::from(f32::EPSILON)
            * (f64::from(start[i]).abs() + (f64::from(speed[i]) * f64::from(dt)).abs() + 1.0)
    });
    if (0..3).any(|i| !end[i].is_finite() || (f64::from(end[i]) - predicted[i]).abs() > error[i]) {
        return Err(Refusal::Numerical);
    }
    let loading = world.loading.as_mut().unwrap();
    let exit = 2.0 * f64::from(loading.config.touch_tolerance);
    let sources = [1, 2, 3].map(|n| loading.bindings[&collider(n, 0)]);
    let mut pending = tracker.pending.clone();
    pending.retain(|pair, witness| {
        loading
            .episodes
            .get(pair)
            .is_some_and(|e| e.id == witness.episode)
    });
    let mut departures = Vec::new();
    for i in 0..3 {
        for j in i + 1..3 {
            let gap_start = f64::from(start[j]) - f64::from(start[i]) - 1.0;
            let gap_end = f64::from(end[j]) - f64::from(end[i]) - 1.0;
            let uncertainty = error[i] + error[j];
            let lower = gap_end.min(predicted[j] - predicted[i] - 1.0) - uncertainty;
            let mut pair = [sources[i], sources[j]];
            pair.sort();
            let Some(episode) = loading.episodes.get(&pair) else {
                continue;
            };
            let continued = pending.remove(&pair).is_some_and(|witness| {
                witness.episode == episode.id && witness.endpoints == [start[i], start[j]]
            });
            // Start in the contact uncertainty band, or continue evidence from
            // the exact prior accepted endpoints. A rounded endpoint can pass
            // the entry gate before its predicted lower bound proves departure.
            // Dropping that witness would make ownership depend on subdivision.
            if speed[j] <= speed[i] || (!continued && gap_start > exit + uncertainty) {
                continue;
            }
            if lower > exit {
                departures.push(Departure {
                    event: EpisodeEvent {
                        id: episode.id,
                        sources: pair,
                        transition: EpisodeTransition::End,
                        closing_speed: 0.0,
                    },
                    at,
                    clearance_lower_bound: lower,
                });
            } else {
                pending.insert(
                    pair,
                    PendingDeparture {
                        episode: episode.id,
                        endpoints: [end[i], end[j]],
                    },
                );
            }
        }
    }
    // End notifications are separate endpoint observations, not fabricated
    // pre-solve contacts. Include them in the same per-frame record limit.
    if loading.records + departures.len() > loading.config.max_records_per_step {
        return Err(Refusal::Response(ResponseRejection::Capture(
            CaptureRejection::StepLimit,
        )));
    }
    departures.sort_by_key(|d| d.event.sources);
    loading.records += departures.len();
    for d in &departures {
        loading.episodes.remove(&d.event.sources);
    }
    tracker.pending = pending;
    Ok(departures)
}

/// The experiment's snapshot bundles physics and pending flight evidence.
/// Production snapshot formats remain unchanged.
struct VerifiedWorld {
    world: PhysicsWorld,
    tracker: DepartureTracker,
}

impl std::ops::Deref for VerifiedWorld {
    type Target = PhysicsWorld;
    fn deref(&self) -> &Self::Target {
        &self.world
    }
}

impl std::ops::DerefMut for VerifiedWorld {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.world
    }
}

impl VerifiedWorld {
    fn snapshot_bytes(&self) -> Result<Vec<u8>, PhysicsWorldError> {
        bincode::serialize(&(self.world.snapshot_bytes()?, &self.tracker))
            .map_err(|e| PhysicsWorldError(e.to_string()))
    }
    fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, PhysicsWorldError> {
        let (physics, tracker): (Vec<u8>, DepartureTracker) =
            bincode::deserialize(bytes).map_err(|e| PhysicsWorldError(e.to_string()))?;
        Ok(Self {
            world: PhysicsWorld::from_snapshot_bytes(&physics)?,
            tracker,
        })
    }
}

impl Clone for VerifiedWorld {
    fn clone(&self) -> Self {
        Self::from_snapshot_bytes(&self.snapshot_bytes().unwrap()).unwrap()
    }
}

fn fixture(case: Case, parts: u16, reverse: bool, shift: f32) -> (VerifiedWorld, Vec<RegionToken>) {
    let (world, tokens) = super::fixture(case, parts, reverse, shift);
    (
        VerifiedWorld {
            world,
            tracker: DepartureTracker::default(),
        },
        tokens,
    )
}

fn step_with_departures(
    world: &mut VerifiedWorld,
    dt: f32,
    limit: usize,
    verified: bool,
) -> Result<Trace, Refusal> {
    assert!(verified);
    super::step_with_departures(&mut world.world, dt, limit, Some(&mut world.tracker))
}

fn assert_unchanged(
    world: &VerifiedWorld,
    snapshot: &[u8],
    loading: &[ContactBatch],
    response: &[ResponseBatch],
) {
    assert_eq!(world.snapshot_bytes().unwrap(), snapshot);
    assert_eq!(world.loading_batches(), loading);
    assert_eq!(world.material_response_batches(), response);
}

fn rapid_fixture(parts: u16, reverse: bool, shift: f32) -> (VerifiedWorld, Vec<RegionToken>) {
    let (mut world, tokens) = fixture(Case::Sequential, parts, reverse, shift);
    for part in 0..parts {
        let h = world.collider_handle(collider(1, part)).unwrap();
        world.raw.colliders[h].set_mass(20.0 / f32::from(parts));
    }
    world.refresh_mass_properties(body(1));
    world.set_pose(body(3), Vec2::X * 2.02, 0.0, true);
    (world, tokens)
}

#[test]
fn verified_departure_preserves_rounded_crossing_evidence() {
    // The exact 5229ns split previously left the actual endpoint outside the
    // start gate while the predicted endpoint still could not prove departure.
    // Exercise both sides of that boundary, representation and snapshot replay.
    for nanos in [5200.0_f32, 5228.0, 5229.0, 5230.0, 5260.0] {
        for parts in [1, 2, 4] {
            for reverse in [false, true] {
                let (mut world, _) = rapid_fixture(parts, reverse, 17.0);
                let initial = energy(&world, 17.0);
                let mut responses = Vec::new();
                let mut ends = Vec::new();
                for (index, dt) in [0.00025, nanos * 1e-9, 0.003].into_iter().enumerate() {
                    if index == 2 && nanos == 5229.0 {
                        assert_eq!(world.tracker.pending.len(), 1);
                        let before = world.snapshot_bytes().unwrap();
                        let loading = world.loading_batches().to_vec();
                        let outputs = world.material_response_batches().to_vec();
                        assert_eq!(
                            step_with_departures(&mut world, dt, 1, true).unwrap_err(),
                            Refusal::Limit { applied: 0 }
                        );
                        assert_unchanged(&world, &before, &loading, &outputs);
                    }
                    let mut restored =
                        VerifiedWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap())
                            .unwrap();
                    let trace = step_with_departures(&mut world, dt, MAX_ADVANCES, true).unwrap();
                    assert_eq!(
                        trace,
                        step_with_departures(&mut restored, dt, MAX_ADVANCES, true).unwrap()
                    );
                    assert_eq!(
                        world.snapshot_bytes().unwrap(),
                        restored.snapshot_bytes().unwrap()
                    );
                    assert_eq!(world.loading_batches(), restored.loading_batches());
                    assert_eq!(
                        world.material_response_batches(),
                        restored.material_response_batches()
                    );
                    ends.extend(trace.departures.into_iter().map(|d| d.event.id));
                    responses.extend(
                        world
                            .material_response_batches()
                            .iter()
                            .map(|b| b.result.as_ref().unwrap().clone()),
                    );
                }
                assert_eq!(responses.len(), 4);
                assert_eq!(ends.len(), 4);
                for response in &responses {
                    assert_eq!(
                        ends.iter().filter(|id| **id == response.roots[0]).count(),
                        1
                    );
                }
                assert!(world.tracker.pending.is_empty());
                assert!(
                    (initial - energy(&world, 17.0) - responses.iter().map(loss).sum::<f64>())
                        .abs()
                        < 1.0e-5 * initial
                );
            }
        }
    }
}

#[test]
fn verified_departure_pending_requires_contiguous_outward_motion() {
    for reverse_velocity in [false, true] {
        let (mut world, _) = rapid_fixture(1, false, 17.0);
        step_with_departures(&mut world, 0.00025, MAX_ADVANCES, true).unwrap();
        step_with_departures(&mut world, 5229.0_f32 * 1e-9, MAX_ADVANCES, true).unwrap();
        let (&pair, witness) = world.tracker.pending.iter().next().unwrap();
        let episode = witness.episode;
        if reverse_velocity {
            // Outward evidence does not survive reversal while clearance is
            // still uncertain. This tiny inward drift does not reach contact.
            let b_speed = world.motion(body(2)).unwrap().linear_velocity.x;
            world.set_velocity(body(1), Vec2::X * (b_speed + 0.001), 0.0, true);
        } else {
            // A pose edit breaks continuity even if the next flight itself is
            // valid and increases separation far beyond the exit threshold.
            let x = world.motion(body(1)).unwrap().position.x;
            world.set_pose(body(1), Vec2::X * (x - 0.1), 0.0, true);
        }
        let trace = step_with_departures(&mut world, 0.0001, MAX_ADVANCES, true).unwrap();
        assert!(trace.departures.is_empty());
        assert!(world.tracker.pending.is_empty());
        assert_eq!(world.loading.as_ref().unwrap().episodes[&pair].id, episode);
    }
}

#[test]
fn verified_departures_rearm_rapid_impacts_across_frames_and_partitions() {
    let mut reference: Option<Vec<AppliedResponse>> = None;
    for hz in [30.0, 60.0, 120.0, 240.0, 2000.0, 10000.0] {
        for shift in [0.0, 17.0] {
            for parts in [1, 2, 4] {
                for reverse in [false, true] {
                    let (mut world, tokens) = rapid_fixture(parts, reverse, shift);
                    let initial = energy(&world, shift);
                    let momentum_before = momentum(&world, shift);
                    let mut responses = Vec::new();
                    let mut ends = Vec::new();
                    // At least 3ms, long enough for all four physical impacts.
                    for _ in 0..libm::ceilf(0.003 * hz) as usize + 1 {
                        let mut replay = world.clone();
                        let trace = step_with_departures(&mut world, 1.0 / hz, MAX_ADVANCES, true)
                            .unwrap_or_else(|e| {
                                panic!(
                                    "hz={hz} shift={shift} parts={parts} reverse={reverse}: {e:?}"
                                )
                            });
                        assert_eq!(
                            trace,
                            step_with_departures(&mut replay, 1.0 / hz, MAX_ADVANCES, true)
                                .unwrap()
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
                        for d in trace.departures {
                            assert!(
                                d.clearance_lower_bound
                                    > 2.0
                                        * f64::from(
                                            world.loading.as_ref().unwrap().config.touch_tolerance
                                        )
                            );
                            assert!(
                                d.at > 0.0
                                    && d.at
                                        <= f64::from(1.0 / hz) * (1.0 + f64::from(f32::EPSILON))
                            );
                            assert_eq!(d.event.transition, EpisodeTransition::End);
                            ends.push(d.event);
                        }
                        ends.extend(
                            world
                                .loading_batches()
                                .iter()
                                .flat_map(|b| &b.events)
                                .filter(|e| e.transition == EpisodeTransition::End)
                                .copied(),
                        );
                        responses.extend(
                            world
                                .material_response_batches()
                                .iter()
                                .map(|b| b.result.as_ref().unwrap().clone()),
                        );
                    }
                    assert_eq!(responses.len(), 4);
                    assert_eq!(ends.len(), 4);
                    for (i, r) in responses.iter().enumerate() {
                        assert_eq!(r.roots.len(), 1);
                        assert_eq!(r.impacts.len(), 1);
                        let expected_pair = if i % 2 == 0 {
                            [tokens[0], tokens[1]]
                        } else {
                            [tokens[1], tokens[2]]
                        };
                        assert_eq!(r.impacts[0].sources.map(|s| s.region), expected_pair);
                        assert_eq!(ends.iter().filter(|e| e.id == r.roots[0]).count(), 1);
                        assert!(responses[..i].iter().all(|old| old.roots[0] != r.roots[0]));
                    }
                    let lost: f64 = responses.iter().map(loss).sum();
                    assert!((initial - energy(&world, shift) - lost).abs() < 1.0e-5 * initial);
                    assert!((momentum(&world, shift) - momentum_before).abs() < 0.002);
                    if let Some(expected) = &reference {
                        for (a, b) in responses.iter().zip(expected) {
                            assert_eq!(a.roots, b.roots);
                            for side in 0..2 {
                                assert!(
                                    (a.impacts[0].after[side] - b.impacts[0].after[side]).abs()
                                        < 1.0e-5 * initial
                                );
                            }
                            assert!((loss(a) - loss(b)).abs() < 1.0e-5 * initial);
                        }
                    } else {
                        reference = Some(responses);
                    }
                }
            }
        }
    }
}

#[test]
fn verified_departure_crossing_survives_fine_subdivision() {
    let (mut world, _) = rapid_fixture(1, false, 0.0);
    step_with_departures(&mut world, 0.00025, MAX_ADVANCES, true).unwrap();
    // Resolve first impact, then sample its departure with steps small enough
    // that several endpoints remain inside the uncertainty band.
    let mut ended = Vec::new();
    for _ in 0..30 {
        ended.extend(
            step_with_departures(&mut world, 0.000001, MAX_ADVANCES, true)
                .unwrap()
                .departures,
        );
    }
    assert_eq!(ended.len(), 1);
    assert_eq!(
        ended[0].event.sources[0].region.id.entity,
        PhysicsId::new(1)
    );
    let mut restored = world.clone();
    let trace = step_with_departures(&mut world, 0.003, MAX_ADVANCES, true).unwrap();
    assert_eq!(
        trace,
        step_with_departures(&mut restored, 0.003, MAX_ADVANCES, true).unwrap()
    );
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        restored.snapshot_bytes().unwrap()
    );
    assert_eq!(world.material_response_batches().len(), 3);
}

#[test]
fn verified_departures_do_not_rearm_rest_chatter_or_unproved_kicks() {
    let (mut world, tokens) = fixture(Case::Supported, 2, true, 0.0);
    world.set_pose(body(1), Vec2::X * -10.0, 0.0, true);
    world.set_velocity(body(1), Vec2::ZERO, 0.0, true);
    let before = balances(&world, &tokens);
    let original_episode = world
        .loading
        .as_ref()
        .unwrap()
        .episodes
        .values()
        .next()
        .unwrap()
        .id;
    for i in 0..64 {
        let jitter = if i % 2 == 0 { 0.25e-6 } else { -0.25e-6 };
        world.set_pose(body(2), Vec2::X * (1.0 + jitter), 0.0, true);
        let trace = step_with_departures(&mut world, 1.0 / 240.0, MAX_ADVANCES, true).unwrap();
        assert!(trace.departures.is_empty());
        assert!(world.material_response_batches().is_empty());
        assert_eq!(
            world
                .loading
                .as_ref()
                .unwrap()
                .episodes
                .values()
                .next()
                .unwrap()
                .id,
            original_episode
        );
        assert_eq!(balances(&world, &tokens), before);
        assert!(world.loading_batches().iter().all(|b| b.events.is_empty()));
    }
    world.set_pose(body(2), Vec2::X, 0.0, true);
    for speed in [0.00005, 1.0] {
        world.set_velocity(body(2), Vec2::X * speed, 0.0, true);
        let snapshot = world.snapshot_bytes().unwrap();
        let loading = world.loading_batches().to_vec();
        let outputs = world.material_response_batches().to_vec();
        assert_eq!(
            step_with_departures(&mut world, DT, MAX_ADVANCES, true).unwrap_err(),
            Refusal::Unaccounted
        );
        assert_unchanged(&world, &snapshot, &loading, &outputs);
    }
}

#[test]
fn verified_departure_limits_restore_ended_episodes_and_outputs() {
    let (mut world, _) = rapid_fixture(1, false, 0.0);
    // End A-B and snapshot its persistent removal before its later return.
    let trace = step_with_departures(&mut world, 0.001, MAX_ADVANCES, true).unwrap();
    assert_eq!(trace.departures.len(), 1);
    let mut paused = world.clone();
    let paused_before = paused.snapshot_bytes().unwrap();
    assert!(
        step_with_departures(&mut paused, 0.0, MAX_ADVANCES, true)
            .unwrap()
            .departures
            .is_empty()
    );
    assert_eq!(paused.snapshot_bytes().unwrap(), paused_before);
    let before = world.snapshot_bytes().unwrap();
    let loading = world.loading_batches().to_vec();
    let outputs = world.material_response_batches().to_vec();
    assert_eq!(
        step_with_departures(&mut world, DT, 2, true).unwrap_err(),
        Refusal::Limit { applied: 1 }
    );
    assert_unchanged(&world, &before, &loading, &outputs);
    let mut replay = world.clone();
    assert_eq!(
        step_with_departures(&mut world, DT, MAX_ADVANCES, true).unwrap(),
        step_with_departures(&mut replay, DT, MAX_ADVANCES, true).unwrap()
    );
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    assert_eq!(world.material_response_batches().len(), 3);

    // The capture stream alone fits exactly; the extra endpoint End record
    // exceeds capacity. Refuse before publishing or losing that notification.
    let (mut world, _) = rapid_fixture(1, false, 0.0);
    world.loading.as_mut().unwrap().config.max_records_per_step = 2;
    let before = world.snapshot_bytes().unwrap();
    assert_eq!(
        step_with_departures(&mut world, 0.001, MAX_ADVANCES, true).unwrap_err(),
        Refusal::Response(ResponseRejection::Capture(CaptureRejection::StepLimit))
    );
    assert_unchanged(&world, &before, &[], &[]);
}

#[test]
fn verified_departures_require_clearance_and_accounted_motion() {
    // Genuine but tiny separating motion remains in the uncertainty band.
    let (mut world, _) = fixture(Case::Supported, 1, false, 0.0);
    world.set_pose(body(1), Vec2::X * -10.0, 0.0, true);
    world.set_velocity(body(1), Vec2::ZERO, 0.0, true);
    world.set_velocity(body(2), Vec2::X * -0.001, 0.0, true);
    let episode = world
        .loading
        .as_ref()
        .unwrap()
        .episodes
        .values()
        .next()
        .unwrap()
        .id;
    assert!(
        step_with_departures(&mut world, 0.001, MAX_ADVANCES, true)
            .unwrap()
            .departures
            .is_empty()
    );
    assert_eq!(
        world
            .loading
            .as_ref()
            .unwrap()
            .episodes
            .values()
            .next()
            .unwrap()
            .id,
        episode
    );

    // An unexplained displacement cannot be certified as this drift's motion.
    let start = handles(&world).map(|h| world.raw.bodies[h].translation().x);
    world.set_pose(body(2), Vec2::X * 0.9, 0.0, true);
    let before = world.snapshot_bytes().unwrap();
    let loading = world.loading_batches().to_vec();
    let outputs = world.material_response_batches().to_vec();
    let velocity = velocities(&world);
    assert_eq!(
        retire_departures(
            &mut world.world,
            &mut world.tracker,
            start,
            velocity,
            0.001,
            0.001
        )
        .unwrap_err(),
        Refusal::Numerical
    );
    assert_unchanged(&world, &before, &loading, &outputs);

    // Even a valid subsequent drift from an already distant pose does not
    // certify a crossing from this episode's contact uncertainty band.
    assert!(
        step_with_departures(&mut world, 0.001, MAX_ADVANCES, true)
            .unwrap()
            .departures
            .is_empty()
    );
    assert_eq!(
        world
            .loading
            .as_ref()
            .unwrap()
            .episodes
            .values()
            .next()
            .unwrap()
            .id,
        episode
    );
}

#[test]
fn verified_departures_order_simultaneous_ends_and_bound_all_notifications() {
    let mut expected = None;
    for reverse in [false, true] {
        let (mut world, _) = fixture(Case::Simultaneous, 1, reverse, 0.0);
        world.loading.as_mut().unwrap().config.max_records_per_step = 6;
        let trace = step_with_departures(&mut world, DT, MAX_ADVANCES, true).unwrap();
        assert_eq!(trace.departures.len(), 2);
        assert_eq!(trace.departures[0].at, trace.departures[1].at);
        assert!(trace.departures[0].event.sources < trace.departures[1].event.sources);
        assert_eq!(world.loading.as_ref().unwrap().records, 6);
        assert!(world.loading.as_ref().unwrap().episodes.is_empty());
        if let Some(expected) = &expected {
            assert_eq!(&trace.departures, expected);
        } else {
            expected = Some(trace.departures);
        }
    }
    let (mut world, _) = fixture(Case::Simultaneous, 1, false, 0.0);
    world.loading.as_mut().unwrap().config.max_records_per_step = 5;
    let before = world.snapshot_bytes().unwrap();
    assert_eq!(
        step_with_departures(&mut world, DT, MAX_ADVANCES, true).unwrap_err(),
        Refusal::Response(ResponseRejection::Capture(CaptureRejection::StepLimit))
    );
    assert_unchanged(&world, &before, &[], &[]);
}
