//! Fixture-only proof for departure across fixed parallel face planes.
//! Production stepping and snapshot formats do not use this tracker.
use super::tests as flat_tests;
use super::*;

mod native;
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Pending {
    episode: EpisodeId,
    endpoints: [Vec2; 2],
    normal: [f64; 2],
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct Tracker {
    pending: BTreeMap<[ContactSource; 2], Pending>,
}

#[derive(Debug, PartialEq)]
pub(super) struct Departure {
    pub(super) event: EpisodeEvent,
    pub(super) at: f64,
    lower: f64,
}

pub(super) fn retire(
    world: &mut PhysicsWorld,
    tracker: &mut Tracker,
    plates: &Plates,
    start: &[Vector],
    velocity: &[Vector],
    dt: f32,
    at: f64,
) -> Result<Vec<Departure>, Refusal> {
    if !dt.is_finite()
        || dt <= 0.0
        || !at.is_finite()
        || at <= 0.0
        || start.len() != plates.handles.len()
        || velocity.len() != start.len()
        || start.iter().chain(velocity).any(|v| !v.is_finite())
    {
        return Err(Refusal::Geometry);
    }
    // Recheck the completed drift before trusting it as episode evidence.
    plates.check_drift(world, start, velocity, dt)?;
    let end = plates.positions(world);
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
            [
                8.0 * f64::from(f32::EPSILON)
                    * (f64::from(p.x).abs() + (f64::from(v.x) * f64::from(dt)).abs() + 1.0),
                8.0 * f64::from(f32::EPSILON)
                    * (f64::from(p.y).abs() + (f64::from(v.y) * f64::from(dt)).abs() + 1.0),
            ]
        })
        .collect();
    let loading = world.loading.as_mut().unwrap();
    let sources: Vec<_> = (1..=plates.handles.len())
        .map(|n| loading.bindings[&collider(n as u64, 0)])
        .collect();
    let exit = 2.0 * f64::from(loading.config.touch_tolerance);
    let mut pending = tracker.pending.clone();
    pending.retain(|pair, witness| {
        loading
            .episodes
            .get(pair)
            .is_some_and(|e| e.id == witness.episode)
    });
    let mut departures = Vec::new();
    for i in 0..start.len() {
        for j in i + 1..start.len() {
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
                w.episode == episode.id
                    && w.normal == plates.normal
                    && w.endpoints == order.map(|k| from_rapier(start[k]))
            });
            // The gap is linear along the fixed face normal. Project both
            // coordinate error boxes onto that normal, not onto center distance.
            let uncertainty = (0..2)
                .map(|axis| plates.normal[axis].abs() * (error[i][axis] + error[j][axis]))
                .sum::<f64>();
            let gap = dot(delta(start[j], start[i]), plates.normal) - 1.0;
            if dot(delta(velocity[j], velocity[i]), plates.normal) <= 0.0
                || (!continued && gap > exit + uncertainty)
            {
                continue;
            }
            let predicted_gap = dot(
                [
                    predicted[j][0] - predicted[i][0],
                    predicted[j][1] - predicted[i][1],
                ],
                plates.normal,
            ) - 1.0;
            let actual_gap = dot(delta(end[j], end[i]), plates.normal) - 1.0;
            let lower = actual_gap.min(predicted_gap) - uncertainty;
            if lower > exit {
                departures.push(Departure {
                    event: EpisodeEvent {
                        id: episode.id,
                        sources: pair,
                        transition: EpisodeTransition::End,
                        closing_speed: 0.0,
                    },
                    at,
                    lower,
                });
            } else {
                pending.insert(
                    pair,
                    Pending {
                        episode: episode.id,
                        normal: plates.normal,
                        endpoints: order.map(|k| from_rapier(end[k])),
                    },
                );
            }
        }
    }
    if loading.records + departures.len() > loading.config.max_records_per_step {
        return Err(Refusal::Response(ResponseRejection::Capture(
            CaptureRejection::StepLimit,
        )));
    }
    departures.sort_by_key(|d| d.event.sources);
    loading.records += departures.len();
    for departure in &departures {
        loading.episodes.remove(&departure.event.sources);
    }
    tracker.pending = pending;
    Ok(departures)
}

struct Verified {
    world: PhysicsWorld,
    tracker: Tracker,
}

impl Verified {
    fn snapshot(&self) -> Vec<u8> {
        bincode::serialize(&(self.world.snapshot_bytes().unwrap(), &self.tracker)).unwrap()
    }

    fn restore(bytes: &[u8]) -> Self {
        let (physics, tracker): (Vec<u8>, Tracker) = bincode::deserialize(bytes).unwrap();
        Self {
            world: PhysicsWorld::from_snapshot_bytes(&physics).unwrap(),
            tracker,
        }
    }

    fn step(&mut self, dt: f32, limit: usize) -> Result<Trace, Refusal> {
        step_tracked(&mut self.world, dt, limit, true, Some(&mut self.tracker))
    }
}

fn rapid(
    parts: &[u16],
    copies: u16,
    reverse: bool,
    angle: f32,
    shift: Vec2,
    offset: f32,
    dynamic: bool,
) -> (Verified, Vec<RegionToken>) {
    let (mut world, tokens) = fixture(parts, copies, reverse, angle, shift, offset, dynamic);
    world.set_velocity(body(1), shift, 0.0, true);
    world.set_pose(body(3), rotate(Vec2::new(2.02, offset), angle), angle, true);
    // Start with genuine fresh gaps. Let the initial fixture's resting support
    // expire normally before resetting the separated layout and launching A.
    super::step(&mut world, 0.05, MAX_ADVANCES).unwrap();
    assert!(world.loading.as_ref().unwrap().episodes.is_empty());
    for (n, x) in [(1, -0.01), (2, 1.0), (3, 2.02)] {
        world.set_pose(
            body(n),
            rotate(Vec2::new(x, if n == 1 { 0.0 } else { offset }), angle),
            angle,
            true,
        );
    }
    world.set_velocity(body(1), shift + rotate(Vec2::X * 40.0, angle), 0.0, true);
    (
        Verified {
            world,
            tracker: Tracker::default(),
        },
        tokens,
    )
}
