//! Bounded inelastic mixing of opposing automatic outfalls. Swept AABB sorting
//! and oriented-footprint tests join local slices, not whole distant streams. Rain,
//! splashes and already mixed jets are deliberately outside this first model.
use crate::{
    MAX_PARCELS, Parcel, WaterConfig,
    spill::{Section, Spill, SpillSource},
};
use engine_core::Vec2;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct Stats {
    pub pairs: u64,
    pub volume: f64,
    pub checks: u64,
}

#[derive(Clone, Copy)]
struct Candidate {
    index: usize,
    outlet: usize,
    direction: Vec2,
    half_length: f32,
    half_width: f32,
    min: Vec2,
    max: Vec2,
}

struct Group {
    outlets: [usize; 2],
    contact_min: Vec2,
    contact_max: Vec2,
    volume: f64,
    position: [f64; 2],
    momentum: [f64; 2],
    durations: [f64; 2],
    upstream_first: [u64; 2],
    upstream_last: [u64; 2],
    upstream_count: [u64; 2],
    bounds: Option<[f64; 2]>,
}

impl Group {
    fn add(&mut self, parcel: Parcel, outlet: usize, emission_tick: u64) {
        self.volume += parcel.volume;
        self.position[0] += parcel.position.x as f64 * parcel.volume;
        self.position[1] += parcel.position.y as f64 * parcel.volume;
        self.momentum[0] += parcel.velocity.x as f64 * parcel.volume;
        self.momentum[1] += parcel.velocity.y as f64 * parcel.volume;
        let side = usize::from(outlet == self.outlets[1]);
        self.durations[side] += parcel.duration;
        self.upstream_first[side] = self.upstream_first[side].min(emission_tick);
        self.upstream_last[side] = self.upstream_last[side].max(emission_tick);
        self.upstream_count[side] += 1;
    }

    fn contiguous(&self) -> bool {
        (0..2).all(|side| {
            self.upstream_last[side]
                .wrapping_sub(self.upstream_first[side])
                .wrapping_add(1)
                == self.upstream_count[side]
        })
    }
}

pub(crate) struct Scratch {
    candidates: Vec<Candidate>,
    groups: Vec<Group>,
}

impl Scratch {
    pub fn new(capacity: usize) -> Self {
        Self {
            candidates: Vec::with_capacity(capacity),
            groups: Vec::with_capacity(capacity / 2),
        }
    }
}

fn normal(dir: Vec2) -> Vec2 {
    Vec2::new(-dir.y, dir.x)
}

fn footprint(index: usize, outlet: usize, p: Parcel, config: WaterConfig, dt: f64) -> Candidate {
    let velocity = p.velocity + Vec2::new(0.0, (-config.gravity * dt * 0.5) as f32);
    let speed = velocity.length().max(1e-6);
    let direction = velocity / speed;
    let half_length = (speed * p.duration as f32 * 0.5).max(0.001);
    let half_width = p.volume as f32 / (4.0 * half_length);
    let across = normal(direction);
    let extent = Vec2::new(
        direction.x.abs() * half_length + across.x.abs() * half_width,
        direction.y.abs() * half_length + across.y.abs() * half_width,
    );
    let end = p.position + velocity * dt as f32;
    Candidate {
        index,
        outlet,
        direction,
        half_length,
        half_width,
        min: Vec2::new(p.position.x.min(end.x), p.position.y.min(end.y)) - extent,
        max: Vec2::new(p.position.x.max(end.x), p.position.y.max(end.y)) + extent,
    }
}

fn swept_overlap(a: Candidate, b: Candidate, pa: Parcel, pb: Parcel, dt: f64) -> bool {
    // Freeze orientations at mid-step; shared gravity cancels from relative
    // translation. Intersect the time intervals on all four separating axes.
    let separation = pb.position - pa.position;
    let relative = pb.velocity - pa.velocity;
    let mut enter = 0.0_f64;
    let mut exit = dt;
    for axis in [
        a.direction,
        normal(a.direction),
        b.direction,
        normal(b.direction),
    ] {
        let radius = a.half_length * a.direction.dot(axis).abs()
            + a.half_width * normal(a.direction).dot(axis).abs()
            + b.half_length * b.direction.dot(axis).abs()
            + b.half_width * normal(b.direction).dot(axis).abs();
        let distance = separation.dot(axis) as f64;
        let velocity = relative.dot(axis) as f64;
        if velocity.abs() < 1e-9 {
            if distance.abs() > radius as f64 {
                return false;
            }
        } else {
            let t0 = (-radius as f64 - distance) / velocity;
            let t1 = (radius as f64 - distance) / velocity;
            enter = enter.max(t0.min(t1));
            exit = exit.min(t0.max(t1));
            if enter > exit {
                return false;
            }
        }
    }
    true
}

pub(crate) fn step(
    parcels: &mut Vec<Parcel>,
    spills: &mut Vec<Option<Spill>>,
    scratch: &mut Scratch,
    stats: &mut Stats,
    config: WaterConfig,
    dt: f64,
    tick: u64,
) {
    scratch.candidates.clear();
    scratch.groups.clear();
    let mut directions = 0_u8;
    for (index, (&p, s)) in parcels.iter().zip(spills.iter()).enumerate() {
        let Some(outlet) = s.and_then(|s| s.source.outlet_id()) else {
            continue;
        };
        if p.velocity.x.abs() <= 1e-5 {
            continue;
        }
        directions |= if p.velocity.x > 0.0 { 1 } else { 2 };
        scratch
            .candidates
            .push(footprint(index, outlet, p, config, dt));
    }
    if directions != 3 {
        return;
    }
    // Sorting indices/swept bounds avoids testing every airborne pair. Worst
    // dense overlap is still capped by MAX_PARCELS; each input is consumed once.
    scratch.candidates.sort_unstable_by(|a, b| {
        a.min
            .y
            .total_cmp(&b.min.y)
            .then(a.outlet.cmp(&b.outlet))
            .then(a.index.cmp(&b.index))
    });
    let mut consumed = [false; MAX_PARCELS];
    for (i, &a) in scratch.candidates.iter().enumerate() {
        if consumed[a.index] {
            continue;
        }
        let pa = parcels[a.index];
        for &b in &scratch.candidates[i + 1..] {
            if b.min.y > a.max.y {
                break;
            }
            if consumed[b.index] || a.outlet == b.outlet || a.min.x > b.max.x || b.min.x > a.max.x {
                continue;
            }
            let pb = parcels[b.index];
            if pa.velocity.x * pb.velocity.x >= 0.0
                || (pb.position.x - pa.position.x) * (pb.velocity.x - pa.velocity.x) > 0.0
                || pa.horizontal_bounds != pb.horizontal_bounds
            {
                continue;
            }
            stats.checks += 1;
            if !swept_overlap(a, b, pa, pb, dt) {
                continue;
            }
            consumed[a.index] = true;
            consumed[b.index] = true;
            let outlets = [a.outlet.min(b.outlet), a.outlet.max(b.outlet)];
            let contact_min = Vec2::new(a.min.x.max(b.min.x), a.min.y.max(b.min.y));
            let contact_max = Vec2::new(a.max.x.min(b.max.x), a.max.y.min(b.max.y));
            // Only a shared local contact patch may aggregate several slices.
            // Two remote intersections of the same outlets stay separate.
            let group = if let Some(i) = scratch.groups.iter().position(|g| {
                g.outlets == outlets
                    && g.bounds == pa.horizontal_bounds
                    && contact_min.x <= g.contact_max.x
                    && contact_max.x >= g.contact_min.x
                    && contact_min.y <= g.contact_max.y
                    && contact_max.y >= g.contact_min.y
            }) {
                &mut scratch.groups[i]
            } else {
                scratch.groups.push(Group {
                    outlets,
                    contact_min,
                    contact_max,
                    volume: 0.0,
                    position: [0.0; 2],
                    momentum: [0.0; 2],
                    durations: [0.0; 2],
                    upstream_first: [u64::MAX; 2],
                    upstream_last: [0; 2],
                    upstream_count: [0; 2],
                    bounds: pa.horizontal_bounds,
                });
                scratch.groups.last_mut().unwrap()
            };
            // Intersect rather than expand: a chain of near neighbors cannot
            // turn a local patch into a distant whole-stream merge.
            group.contact_min = Vec2::new(
                group.contact_min.x.max(contact_min.x),
                group.contact_min.y.max(contact_min.y),
            );
            group.contact_max = Vec2::new(
                group.contact_max.x.min(contact_max.x),
                group.contact_max.y.min(contact_max.y),
            );
            group.add(pa, a.outlet, spills[a.index].unwrap().tick);
            group.add(pb, b.outlet, spills[b.index].unwrap().tick);
            stats.pairs += 1;
            stats.volume += pa.volume + pb.volume;
            break;
        }
    }
    if scratch.groups.is_empty() {
        return;
    }
    let mut write = 0;
    for read in 0..parcels.len() {
        if !consumed[read] {
            parcels[write] = parcels[read];
            spills[write] = spills[read];
            write += 1;
        }
    }
    parcels.truncate(write);
    spills.truncate(write);
    let mut linked = [false; MAX_PARCELS];
    for group in &scratch.groups {
        // Several consecutive slices from one outlet represent several time
        // intervals, while simultaneous slices from two outlets do not.
        let duration = group.durations[0].max(group.durations[1]);
        let velocity = Vec2::new(
            (group.momentum[0] / group.volume) as f32,
            (group.momentum[1] / group.volume) as f32,
        );
        let position = Vec2::new(
            (group.position[0] / group.volume) as f32,
            (group.position[1] / group.volume) as f32,
        );
        // Inelastic center-of-mass replacement preserves volume and momentum,
        // dissipating relative motion. The common ballistic step follows. A
        // predicted contact may therefore be resolved up to one tick early.
        let source = SpillSource::Junction {
            outlets: group.outlets,
        };
        let mut tail = Section {
            position: position - velocity * (duration * 0.5) as f32,
            velocity,
            flow: group.volume / duration,
        };
        let previous = spills
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                s.filter(|s| {
                    !linked[i]
                        && s.source == source
                        && group.contiguous()
                        && s.upstream_end.is_some_and(|end| {
                            (0..2)
                                .all(|side| end[side].wrapping_add(1) == group.upstream_first[side])
                        })
                })
                .map(|s| (i, s))
            })
            .filter(|(_, s)| {
                (s.tail.position - tail.position).length()
                    <= velocity.length() * duration as f32 * 4.0
            })
            .min_by(|(_, a), (_, b)| {
                (a.tail.position - tail.position)
                    .length_squared()
                    .total_cmp(&(b.tail.position - tail.position).length_squared())
            });
        let head = if let Some((i, previous)) = previous {
            linked[i] = true;
            // Unlike a fixed lip, the sampled collision center can jump from
            // one slice pair to the next. Space consecutive material faces by
            // throughput, not that sampling jitter. Both end widths still come
            // from flow/speed; the ribbon solver preserves this slice's area.
            let direction =
                (velocity.normalized() + previous.tail.velocity.normalized()).normalized();
            let half_width = |s: Section| s.flow / (2.0 * s.velocity.length().max(1e-6) as f64);
            let spacing = (group.volume / (half_width(tail) + half_width(previous.tail))) as f32;
            let across = normal(direction);
            let offset = (tail.position - previous.tail.position)
                .dot(across)
                .clamp(-spacing, spacing);
            tail.position = previous.tail.position - direction * spacing + across * offset;
            previous.tail
        } else {
            Section {
                position: position + velocity * (duration * 0.5) as f32,
                ..tail
            }
        };
        parcels.push(Parcel {
            position,
            velocity,
            volume: group.volume,
            duration,
            horizontal_bounds: group.bounds,
        });
        spills.push(Some(Spill {
            source,
            tick,
            upstream_end: group.contiguous().then_some(group.upstream_last),
            tail,
            head,
        }));
    }
}
