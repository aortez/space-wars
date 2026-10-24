use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Witness {
    episode: EpisodeId,
    endpoints: [Vec2; 2],
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct Tracker {
    pending: BTreeMap<[ContactSource; 2], Witness>,
}

pub(super) fn retire(
    world: &mut PhysicsWorld,
    tracker: &mut Tracker,
    start: [Vector; 3],
    speed: [Vector; 3],
    dt: f32,
) -> Result<Vec<EpisodeEvent>, Refusal> {
    if !dt.is_finite() || dt <= 0.0 || start.iter().chain(&speed).any(|v| !v.is_finite()) {
        return Err(Refusal::Numerical);
    }
    let end = positions(world);
    let predicted: [[f64; 2]; 3] = std::array::from_fn(|i| {
        [
            f64::from(start[i].x) + f64::from(speed[i].x) * f64::from(dt),
            f64::from(start[i].y) + f64::from(speed[i].y) * f64::from(dt),
        ]
    });
    let error: [[f64; 2]; 3] = std::array::from_fn(|i| {
        let p = [start[i].x, start[i].y];
        let v = [speed[i].x, speed[i].y];
        std::array::from_fn(|axis| {
            8.0 * f64::from(f32::EPSILON)
                * (f64::from(p[axis]).abs() + (f64::from(v[axis]) * f64::from(dt)).abs() + 1.0)
        })
    });
    for i in 0..3 {
        let actual = [end[i].x, end[i].y];
        if (0..2).any(|axis| {
            !actual[axis].is_finite()
                || (f64::from(actual[axis]) - predicted[i][axis]).abs() > error[i][axis]
        }) {
            return Err(Refusal::Numerical);
        }
    }
    let loading = world.loading.as_mut().unwrap();
    let sources = [1, 2, 3].map(|n| loading.bindings[&collider(n, 0)]);
    let mut pending = tracker.pending.clone();
    pending.retain(|pair, witness| {
        loading
            .episodes
            .get(pair)
            .is_some_and(|e| e.id == witness.episode)
    });
    let mut events = Vec::new();
    let exit = 2.0 * f64::from(loading.config.touch_tolerance);
    for i in 0..3 {
        for j in i + 1..3 {
            let order = if sources[i] < sources[j] {
                [i, j]
            } else {
                [j, i]
            };
            let pair = order.map(|k| sources[k]);
            let Some(episode) = loading.episodes.get(&pair) else {
                continue;
            };
            let continued = pending.remove(&pair).is_some_and(|w| {
                w.episode == episode.id && w.endpoints == order.map(|k| from_rapier(start[k]))
            });
            let d = delta(start[j], start[i]);
            let v = delta(speed[j], speed[i]);
            // d(t).v = d(0).v + |v|^2*t: a strictly outward start
            // proves the distance increases throughout this constant drift.
            let uncertainty = length(error[i]) + length(error[j]);
            if dot(d, v) <= 0.0 || (!continued && length(d) - 1.0 > exit + uncertainty) {
                continue;
            }
            let predicted_gap = length([
                predicted[j][0] - predicted[i][0],
                predicted[j][1] - predicted[i][1],
            ]) - 1.0;
            // Reverse triangle inequality bounds distance error by the sum
            // of both endpoint error-vector lengths, in any orientation.
            let lower = (length(delta(end[j], end[i])) - 1.0).min(predicted_gap) - uncertainty;
            if lower > exit {
                events.push(EpisodeEvent {
                    id: episode.id,
                    sources: pair,
                    transition: EpisodeTransition::End,
                    closing_speed: 0.0,
                });
            } else {
                pending.insert(
                    pair,
                    Witness {
                        episode: episode.id,
                        endpoints: order.map(|k| from_rapier(end[k])),
                    },
                );
            }
        }
    }
    if loading.records + events.len() > loading.config.max_records_per_step {
        return Err(Refusal::Response(ResponseRejection::Capture(
            CaptureRejection::StepLimit,
        )));
    }
    events.sort_by_key(|e| e.sources);
    loading.records += events.len();
    for event in &events {
        loading.episodes.remove(&event.sources);
    }
    tracker.pending = pending;
    Ok(events)
}

#[test]
fn oblique_pending_crossing_replays_and_rolls_back() {
    for ns in [5540.0_f32, 5560.0, 5562.0, 5564.0, 5580.0] {
        for parts in [1, 2, 4] {
            for reverse in [false, true] {
                let (mut w, _) =
                    fixture(Layout::Rapid, parts, reverse, 0.4, Vec2::new(17.0, -11.0));
                let mut native = w.world.clone();
                let arrival =
                    next_time(positions(&w), handles(&w).map(|h| w.raw.bodies[h].linvel()))
                        .unwrap()
                        .unwrap() as f32;
                w.step(arrival, MAX_ADVANCES).unwrap();
                native.step_material_three_ball_arrivals(arrival).unwrap();
                let prefix = w.step(ns * 1.0e-9, MAX_ADVANCES).unwrap();
                let native_prefix = native
                    .step_material_three_ball_arrivals(ns * 1.0e-9)
                    .unwrap();
                assert!(w.tracker.pending.len() <= 1);
                if ns == 5562.0 {
                    assert_eq!(w.tracker.pending.len(), 1);
                }
                let before = w.snapshot();
                let loading = w.loading_batches().to_vec();
                let response = w.material_response_batches().to_vec();
                assert!(matches!(w.step(0.003, 1), Err(Refusal::Limit { .. })));
                assert_eq!(w.snapshot(), before);
                assert_eq!(w.loading_batches(), loading);
                assert_eq!(w.material_response_batches(), response);
                if ns == 5562.0 {
                    let mut lost = Experiment::restore(&before);
                    lost.tracker = Tracker::default();
                    assert_eq!(lost.step(0.003, MAX_ADVANCES), Err(Refusal::Unaccounted));
                }
                let mut replay = Experiment::restore(&before);
                let native_before = native.snapshot_bytes().unwrap();
                let native_loading = native.loading_batches().to_vec();
                let native_response = native.material_response_batches().to_vec();
                assert_eq!(native.step_balls_with_limit(0.003, 1, false).unwrap_err(),
                    crate::world::loading::response::ball_arrival::BallArrivalRejection::AdvanceLimit);
                assert_eq!(native.snapshot_bytes().unwrap(), native_before);
                assert_eq!(native.loading_batches(), native_loading);
                assert_eq!(native.material_response_batches(), native_response);
                let mut native_replay = PhysicsWorld::from_snapshot_bytes(&native_before).unwrap();
                let native_result = native.step_material_three_ball_arrivals(0.003).unwrap();
                assert_eq!(
                    native_result.events,
                    native_replay
                        .step_material_three_ball_arrivals(0.003)
                        .unwrap()
                        .events
                );
                assert_eq!(
                    native.snapshot_bytes().unwrap(),
                    native_replay.snapshot_bytes().unwrap()
                );
                let result = w.step(0.003, MAX_ADVANCES).unwrap();
                assert_eq!(result, replay.step(0.003, MAX_ADVANCES).unwrap());
                assert_eq!(w.snapshot(), replay.snapshot());
                assert_eq!(w.material_response_batches().len(), 3);
                assert_eq!(
                    native.material_response_batches(),
                    w.material_response_batches()
                );
                assert_eq!(
                    native_prefix
                        .events
                        .iter()
                        .chain(&native_result.events)
                        .filter(|e| e.event.transition == EpisodeTransition::End)
                        .count(),
                    4
                );
                assert_eq!(
                    prefix
                        .events
                        .iter()
                        .chain(&result.events)
                        .filter(|e| e.event.transition == EpisodeTransition::End)
                        .count(),
                    4
                );
            }
        }
    }
}

#[test]
fn oblique_pending_requires_full_endpoint_continuity_and_outward_motion() {
    let (mut original, _) = fixture(Layout::Rapid, 1, false, 0.4, Vec2::new(17.0, -11.0));
    let arrival = next_time(
        positions(&original),
        handles(&original).map(|h| original.raw.bodies[h].linvel()),
    )
    .unwrap()
    .unwrap() as f32;
    original.step(arrival, MAX_ADVANCES).unwrap();
    original.step(5562.0e-9, MAX_ADVANCES).unwrap();
    assert_eq!(original.tracker.pending.len(), 1);
    let (&pair, witness) = original.tracker.pending.iter().next().unwrap();
    let episode = witness.episode;
    for axis in [Vector::X, Vector::Y] {
        let mut w = Experiment::restore(&original.snapshot());
        let p = positions(&w);
        // Preserve x in one variant: checking only the old horizontal endpoint
        // would incorrectly continue evidence through this perpendicular edit.
        w.set_pose(body(1), from_rapier(p[0] - axis * 0.1), 0.0, true);
        w.step(0.00001, MAX_ADVANCES).unwrap();
        assert!(w.tracker.pending.is_empty());
        assert_eq!(w.loading.as_ref().unwrap().episodes[&pair].id, episode);
    }
    let mut w = Experiment::restore(&original.snapshot());
    let start = positions(&w);
    let d = start[1] - start[0];
    let speed = [d * 0.001, Vector::ZERO, Vector::ZERO];
    for (i, v) in speed.into_iter().enumerate() {
        w.set_velocity(body(i as u64 + 1), from_rapier(v), 0.0, true);
    }
    let start = positions(&w);
    let actual = advance(&mut w, 1.0e-6, false).unwrap();
    let events = retire(&mut w.world, &mut w.tracker, start, actual, 1.0e-6).unwrap();
    assert!(events.is_empty() && w.tracker.pending.is_empty());
    assert_eq!(w.loading.as_ref().unwrap().episodes[&pair].id, episode);
}
