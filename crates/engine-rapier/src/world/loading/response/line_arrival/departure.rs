use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingDeparture {
    episode: EpisodeId,
    endpoints: [Vec2; 2],
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(in crate::world) struct DepartureTracker {
    pending: BTreeMap<[ContactSource; 2], PendingDeparture>,
}

impl DepartureTracker {
    pub(in crate::world) fn clear(&mut self) {
        self.pending.clear();
    }

    pub(in crate::world) fn valid_for(&self, loading: &LoadingState) -> bool {
        self.pending.len() <= 3
            && self.pending.iter().all(|(pair, witness)| {
                pair[0] < pair[1]
                    && pair[0].region != pair[1].region
                    && pair.iter().all(|s| {
                        loading.live(s.region) && loading.bindings.values().any(|bound| bound == s)
                    })
                    && loading
                        .episodes
                        .get(pair)
                        .is_some_and(|e| e.id == witness.episode)
                    && witness
                        .endpoints
                        .iter()
                        .all(|p| p.x.is_finite() && p.y.is_finite())
            })
    }
}

pub(super) fn retire(
    world: &mut PhysicsWorld,
    scene: &Scene,
    start: [Vector; 3],
    speed: [Vector; 3],
    dt: f32,
) -> Result<Vec<EpisodeEvent>, LineArrivalRejection> {
    let end = scene.positions(world);
    let predicted: [f64; 3] =
        std::array::from_fn(|i| f64::from(start[i].x) + f64::from(speed[i].x) * f64::from(dt));
    let error: [f64; 3] = std::array::from_fn(|i| {
        8.0 * f64::from(f32::EPSILON)
            * (f64::from(start[i].x).abs() + (f64::from(speed[i].x) * f64::from(dt)).abs() + 1.0)
    });
    if (0..3).any(|i| {
        !end[i].is_finite()
            || end[i].y != start[i].y
            || (f64::from(end[i].x) - predicted[i]).abs() > error[i]
    }) {
        return Err(LineArrivalRejection::Numerical);
    }
    let loading = world.loading.as_mut().unwrap();
    let mut pending = loading.line_arrival.as_ref().unwrap().pending.clone();
    pending.retain(|pair, witness| {
        loading
            .episodes
            .get(pair)
            .is_some_and(|e| e.id == witness.episode)
    });
    let exit = 2.0 * f64::from(loading.config.touch_tolerance);
    let mut events = Vec::new();
    for i in 0..3 {
        for j in i + 1..3 {
            let gap_start = f64::from(start[j].x) - f64::from(start[i].x) - 1.0;
            let gap_end = f64::from(end[j].x) - f64::from(end[i].x) - 1.0;
            let uncertainty = error[i] + error[j];
            let lower = gap_end.min(predicted[j] - predicted[i] - 1.0) - uncertainty;
            let order = if scene.sources[i] < scene.sources[j] {
                [i, j]
            } else {
                [j, i]
            };
            let pair = order.map(|k| scene.sources[k]);
            let Some(episode) = loading.episodes.get(&pair) else {
                continue;
            };
            let continued = pending.remove(&pair).is_some_and(|witness| {
                witness.episode == episode.id
                    && witness.endpoints == order.map(|k| from_rapier(start[k]))
            });
            if speed[j].x <= speed[i].x || (!continued && gap_start > exit + uncertainty) {
                continue;
            }
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
                    PendingDeparture {
                        episode: episode.id,
                        endpoints: order.map(|k| from_rapier(end[k])),
                    },
                );
            }
        }
    }
    if loading.records + events.len() > loading.config.max_records_per_step {
        return Err(LineArrivalRejection::Response(ResponseRejection::Capture(
            CaptureRejection::StepLimit,
        )));
    }
    events.sort_by_key(|e| e.sources);
    loading.records += events.len();
    for event in &events {
        loading.episodes.remove(&event.sources);
    }
    loading.line_arrival.as_mut().unwrap().pending = pending;
    Ok(events)
}
