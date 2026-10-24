use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingDeparture {
    episode: EpisodeId,
    endpoints: [Vec2; 2],
    normal: [f64; 2],
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(in crate::world) struct DepartureTracker {
    pending: BTreeMap<[ContactSource; 2], PendingDeparture>,
}

impl DepartureTracker {
    pub(in crate::world) fn clear(&mut self) {
        self.pending.clear();
    }

    pub(in crate::world) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(in crate::world) fn valid_for(&self, loading: &LoadingState) -> bool {
        // Structural validation permits stale poses/shapes: admission and exact
        // continuity decide whether evidence can be used on the next call.
        self.pending.len() <= 3
            && self.pending.iter().all(|(pair, witness)| {
                pair[0] < pair[1]
                    && pair[0].region != pair[1].region
                    && pair.iter().all(|s| {
                        loading.live(s.region) && loading.bindings.values().any(|b| b == s)
                    })
                    && loading.regions[&pair[0].region.id].body
                        != loading.regions[&pair[1].region.id].body
                    && loading
                        .episodes
                        .get(pair)
                        .is_some_and(|e| e.id == witness.episode)
                    && witness
                        .endpoints
                        .iter()
                        .all(|p| p.x.is_finite() && p.y.is_finite())
                    && witness.normal.iter().all(|n| n.is_finite())
                    && (libm::hypot(witness.normal[0], witness.normal[1]) - 1.0).abs()
                        <= 4.0 * f64::EPSILON
            })
    }
}

pub(super) fn retire(
    world: &mut PhysicsWorld,
    scene: &Scene,
    start: &[Vector],
    velocity: &[Vector],
    dt: f32,
) -> Result<Vec<EpisodeEvent>, FlatArrivalRejection> {
    use FlatArrivalRejection as R;
    if !dt.is_finite()
        || dt <= 0.0
        || start.len() != scene.bodies.len()
        || velocity.len() != start.len()
        || start.iter().chain(velocity).any(|v| !v.is_finite())
    {
        return Err(R::Numerical);
    }
    scene.check_drift(world, start, velocity, dt)?;
    let end = scene.positions(world);
    let predicted: Vec<[f64; 2]> = start
        .iter()
        .zip(velocity)
        .map(|(p, v)| {
            [
                f64::from(p.x) + f64::from(v.x) * f64::from(dt),
                f64::from(p.y) + f64::from(v.y) * f64::from(dt),
            ]
        })
        .collect();
    let error: Vec<[f64; 2]> = start
        .iter()
        .zip(velocity)
        .map(|(p, v)| {
            let p = [p.x, p.y];
            let v = [v.x, v.y];
            std::array::from_fn(|axis| {
                8.0 * f64::from(f32::EPSILON)
                    * (f64::from(p[axis]).abs() + (f64::from(v[axis]) * f64::from(dt)).abs() + 1.0)
            })
        })
        .collect();
    let loading = world.loading.as_mut().unwrap();
    let mut pending = loading.flat_departures.as_ref().unwrap().pending.clone();
    pending.retain(|pair, w| {
        loading
            .episodes
            .get(pair)
            .is_some_and(|e| e.id == w.episode)
    });
    let exit = 2.0 * f64::from(loading.config.touch_tolerance);
    let project = |v: [f64; 2]| v[0] * scene.normal[0] + v[1] * scene.normal[1];
    let gap = |a: Vector, b: Vector| {
        project([
            f64::from(b.x) - f64::from(a.x),
            f64::from(b.y) - f64::from(a.y),
        ]) - 1.0
    };
    let mut events = Vec::new();
    for i in 0..start.len() {
        for j in i + 1..start.len() {
            // Scene admission sorts by position and certifies each source's
            // filled rectangle. Witness endpoints instead follow source order.
            let order = if scene.sources[i] < scene.sources[j] {
                [i, j]
            } else {
                [j, i]
            };
            let pair = order.map(|k| scene.sources[k]);
            let Some(episode) = loading.episodes.get(&pair) else {
                continue;
            };
            let continued = pending.remove(&pair).is_some_and(|w| {
                w.episode == episode.id
                    && w.normal == scene.normal
                    && w.endpoints == order.map(|k| from_rapier(start[k]))
            });
            // Project both coordinate error boxes onto the fixed face normal.
            // Positive outward speed makes this gap monotone during the drift.
            let uncertainty: f64 = (0..2)
                .map(|axis| scene.normal[axis].abs() * (error[i][axis] + error[j][axis]))
                .sum();
            let outward = project([
                f64::from(velocity[j].x) - f64::from(velocity[i].x),
                f64::from(velocity[j].y) - f64::from(velocity[i].y),
            ]);
            if outward <= 0.0 || (!continued && gap(start[i], start[j]) > exit + uncertainty) {
                continue;
            }
            let predicted_gap = project([
                predicted[j][0] - predicted[i][0],
                predicted[j][1] - predicted[i][1],
            ]) - 1.0;
            let lower = gap(end[i], end[j]).min(predicted_gap) - uncertainty;
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
                        normal: scene.normal,
                    },
                );
            }
        }
    }
    if loading.records + events.len() > loading.config.max_records_per_step {
        return Err(R::Response(ResponseRejection::Capture(
            CaptureRejection::StepLimit,
        )));
    }
    events.sort_by_key(|e| e.sources);
    loading.records += events.len();
    for event in &events {
        loading.episodes.remove(&event.sources);
    }
    loading.flat_departures.as_mut().unwrap().pending = pending;
    Ok(events)
}
