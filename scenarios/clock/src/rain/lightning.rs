//! Lightning in a heavier shower. A few times a shower a bolt leaves the
//! canopy for the highest surface below it, the top of a lit cell or the
//! water and the floor, and its flash lights the room. A strike is one to
//! three strokes down the same channel, each a flash that peaks for a tick and
//! fades over a few, so a strike flickers. Strikes come at least `STRIKE_GAP`
//! apart and have at most three strokes, so the room never flashes more than
//! three times in a second, the most that guidance on flashing content
//! allows for people with photosensitive epilepsy (WCAG 2.3.1).
//!
//! The storm plans its strikes when the shower begins, from a seeded stream
//! of its own rather than the shower's, and only reads the water, so the rain
//! falls as it would without it. A light shower has none.

use std::ops::RangeInclusive;

use engine_common::{ClockRainAmount, ClockRainStrike, ClockStrikeTarget};
use engine_core::Vec2;
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::RAINING_TICKS;
use crate::layout::Layout;

#[cfg(test)]
mod tests;

/// The storm's stream, apart from the shower's.
const STREAM: u64 = 0x4c49_4748_544e;
/// Strikes come at least this many ticks apart.
pub(crate) const STRIKE_GAP: u64 = 150;
/// How many strokes a strike has, drawn from these alike: one in five has
/// one, and the others two or three. They come this many ticks apart.
const STROKE_COUNTS: [usize; 5] = [1, 2, 2, 3, 3];
const STROKE_GAP: RangeInclusive<u32> = 4..=7;
/// Strikes fall in this part of the shower's rain, more of them where it
/// rains hardest.
const FIRST: f64 = 0.12;
const LAST: f64 = 0.92;
/// A stroke's flash fades by a factor of e in this many ticks, and its
/// channel's light more slowly.
const FLASH_FADE: f32 = 3.0;
const CHANNEL_FADE: f32 = 4.5;
/// Light this faint is gone.
const GONE: f32 = 0.02;
/// The longest a spark lives, in ticks, and how fast sparks fall, in cells a
/// second squared.
pub(crate) const SPARK_TICKS: u32 = 34;
const SPARK_GRAVITY: f32 = 24.0;
/// How far a bolt drops in each step of its walk down, in cells, and the
/// most steps it takes.
const STEP_CELLS: f32 = 0.55;
const MAX_STEPS: usize = 32;
/// How far a bolt wanders sideways in a step, as a fraction of the step's
/// drop, and how often it kinks twice as far.
const WANDER: f32 = 0.6;
const KINK_CHANCE: f64 = 0.2;
/// How far the bolt's top may lean from above where it strikes, in cells.
const LEAN_CELLS: f32 = 1.5;
/// Struck water must be at least this deep, in cells, to count as water.
const WATER_CELLS: f64 = 0.05;

/// How many strikes a shower of `amount` plans.
fn strike_count(amount: ClockRainAmount) -> RangeInclusive<u32> {
    match amount {
        ClockRainAmount::Heavy => 3..=5,
        ClockRainAmount::Medium => 1..=2,
        ClockRainAmount::Light | ClockRainAmount::Varied => 0..=0,
    }
}

/// One stroke of a strike: when it comes, in ticks after the strike's
/// first, and how bright, 1 at full.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Stroke {
    pub at: u32,
    pub peak: f32,
}

/// A strike the storm has planned but not struck.
#[derive(Debug, Clone, PartialEq)]
struct Planned {
    /// The shower's tick it strikes on.
    tick: u64,
    /// Where across the room it strikes, -1 at the left of the open room and
    /// 1 at the right.
    across: f32,
    /// How far the bolt's top leans from above where it strikes, in cells.
    lean: f32,
    strokes: Vec<Stroke>,
    /// The bolt's own stream, for its shape and its sparks.
    seed: u64,
}

/// A bolt's path. Every point of the channel and of each fork lies lower
/// than the one before it, so neither turns back on itself.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Bolt {
    /// From under the canopy down to where it struck.
    pub channel: Vec<Vec2>,
    /// Branches off the channel that die out in the air.
    pub forks: Vec<Vec<Vec2>>,
    /// How far the channel drops in each step, in world units.
    pub step: f32,
}

/// A spark thrown from where a bolt struck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spark {
    /// Its velocity as it leaves, in cells a second.
    pub velocity: Vec2,
    /// How many ticks it glows.
    pub life: u32,
}

impl Spark {
    /// How far the spark is from where the bolt struck, `seconds` after the
    /// strike, in cells.
    pub fn offset(self, seconds: f32) -> Vec2 {
        self.velocity * seconds + Vec2::new(0.0, -0.5 * SPARK_GRAVITY * seconds * seconds)
    }
}

/// A strike: when it came, what it struck, its strokes, its bolt and the
/// sparks it threw.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Strike {
    /// The shower's tick it struck on.
    pub tick: u64,
    pub target: ClockStrikeTarget,
    pub strokes: Vec<Stroke>,
    pub bolt: Bolt,
    pub sparks: Vec<Spark>,
}

impl Strike {
    /// Where the bolt left the canopy.
    pub fn top(&self) -> Vec2 {
        self.bolt.channel[0]
    }

    /// Where it struck.
    pub fn ground(&self) -> Vec2 {
        *self.bolt.channel.last().expect("a bolt has a channel")
    }

    /// How bright the room flashes at the shower's `tick`, 1 at full.
    pub fn flash(&self, tick: u64) -> f32 {
        tick.checked_sub(self.tick)
            .map_or(0.0, |age| light(&self.strokes, age, FLASH_FADE))
    }

    /// How bright the bolt's channel glows at the shower's `tick`, 1 at full.
    pub fn channel(&self, tick: u64) -> f32 {
        tick.checked_sub(self.tick)
            .map_or(0.0, |age| light(&self.strokes, age, CHANNEL_FADE))
    }
}

/// The light of `strokes` `age` ticks after the first: each stroke at its
/// peak on its tick and fading by e every `fade` ticks, the brightest of
/// them, and nothing once it is faint.
fn light(strokes: &[Stroke], age: u64, fade: f32) -> f32 {
    let light = strokes
        .iter()
        .filter_map(|stroke| {
            let since = age.checked_sub(u64::from(stroke.at))?;
            Some(stroke.peak * (-(since as f32) / fade).exp())
        })
        .fold(0.0, f32::max);
    if light < GONE { 0.0 } else { light }
}

/// A shower's lightning: the strikes it plans and the latest it struck.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Storm {
    planned: Vec<Planned>,
    next: usize,
    latest: Option<Strike>,
}

impl Storm {
    /// The strikes a shower of `amount` (already resolved from Varied) plans,
    /// from its `seed`.
    pub fn new(seed: u64, amount: ClockRainAmount) -> Self {
        let mut rng = StdRng::seed_from_u64(seed ^ STREAM);
        Self {
            planned: plan(&mut rng, amount),
            next: 0,
            latest: None,
        }
    }

    /// A storm with no lightning.
    pub fn calm() -> Self {
        Self {
            planned: Vec::new(),
            next: 0,
            latest: None,
        }
    }

    /// The strikes planned, struck or not.
    pub fn planned(&self) -> u32 {
        self.planned.len() as u32
    }

    /// The strikes struck so far.
    pub fn struck(&self) -> u32 {
        self.next as u32
    }

    /// The shower's ticks the strikes come on.
    #[cfg(test)]
    pub fn ticks(&self) -> Vec<u64> {
        self.planned.iter().map(|strike| strike.tick).collect()
    }

    /// The latest strike, which shows until its light and sparks are gone.
    pub fn latest(&self) -> Option<&Strike> {
        self.latest.as_ref()
    }

    /// The strike that struck on the shower's `tick`, if one did.
    pub fn began(&self, tick: u64) -> Option<&Strike> {
        self.latest.as_ref().filter(|strike| strike.tick == tick)
    }

    /// How bright the room flashes at the shower's `tick`, 1 at full.
    pub fn flash(&self, tick: u64) -> f32 {
        self.latest
            .as_ref()
            .map_or(0.0, |strike| strike.flash(tick))
    }

    /// Strikes on the shower's `tick` if a strike is due. `surface` gives the
    /// highest surface at a point across the room, and what it is; `clear_of`
    /// is a span, its middle and half its width, the bolt keeps out of.
    pub fn step(
        &mut self,
        tick: u64,
        layout: &Layout,
        surface: impl Fn(f32) -> (f32, ClockStrikeTarget),
        clear_of: Option<(f32, f32)>,
    ) -> bool {
        let Some(planned) = self.planned.get(self.next).filter(|p| p.tick == tick) else {
            return false;
        };
        let limit = open_room(layout);
        let mut x = planned.across * limit;
        if let Some((middle, half)) = clear_of
            && (x - middle).abs() < half
        {
            // Step past the span on the side the strike was aimed at, or the
            // other side when that one runs into the wall.
            let side = if x < middle { -1.0 } else { 1.0 };
            x = middle + side * half;
            if x.abs() > limit {
                x = middle - side * half;
            }
            x = x.clamp(-limit, limit);
        }
        let (y, target) = surface(x);
        let ground = Vec2::new(x, y.min(layout.canopy_y - layout.pitch));
        let top = Vec2::new(
            (x + planned.lean * layout.pitch).clamp(-limit, limit),
            layout.canopy_y,
        );
        let mut rng = StdRng::seed_from_u64(planned.seed);
        let bolt = bolt(&mut rng, top, ground, layout);
        let sparks = sparks(&mut rng);
        self.latest = Some(Strike {
            tick,
            target,
            strokes: planned.strokes.clone(),
            bolt,
            sparks,
        });
        self.next += 1;
        true
    }

    /// The latest strike, as `clock state` reports it.
    pub fn state(&self) -> Option<ClockRainStrike> {
        let milli = |point: Vec2| {
            [
                (point.x * 1000.0).round() as i32,
                (point.y * 1000.0).round() as i32,
            ]
        };
        self.latest.as_ref().map(|strike| ClockRainStrike {
            tick: strike.tick,
            strokes: strike.strokes.len() as u32,
            top_milli: milli(strike.top()),
            ground_milli: milli(strike.ground()),
            target: strike.target,
        })
    }
}

/// How far either side of the middle a bolt may come down: inside the frame,
/// half a cell clear of it.
fn open_room(layout: &Layout) -> f32 {
    layout.bounds_max.x - layout.frame_width * 2.0 - layout.pitch * 0.5
}

/// The highest surface of `water` at `x`: the water or the bed of the
/// highest column there, and what it is. Pools from `floor_pools` on are the
/// lit cells' tops; where no column lies, the bolt reaches the floor's line.
pub(crate) fn surface(
    water: &engine_water::WaterWorld,
    floor_pools: usize,
    layout: &Layout,
    x: f32,
) -> (f32, ClockStrikeTarget) {
    let at = f64::from(x);
    water
        .pools()
        .iter()
        .enumerate()
        .flat_map(|(index, pool)| {
            // A range around `at`, so a point on a column's edge finds it.
            pool.columns_in_range(at - 1e-6, at + 1e-6)
                .filter(move |column| column.left <= at && at <= column.left + column.width)
                .map(move |column| (index, column))
        })
        .map(|(index, column)| {
            let bed = column.bed_at(at);
            let top = column.surface.max(bed);
            let target = if index >= floor_pools {
                ClockStrikeTarget::Digit
            } else if top - bed >= WATER_CELLS * f64::from(layout.pitch) {
                ClockStrikeTarget::Water
            } else {
                ClockStrikeTarget::Floor
            };
            (top as f32, target)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((layout.floor_y, ClockStrikeTarget::Floor))
}

fn plan(rng: &mut StdRng, amount: ClockRainAmount) -> Vec<Planned> {
    let count = rng.random_range(strike_count(amount)) as usize;
    let mut ticks: Vec<u64> = Vec::with_capacity(count);
    // Draw where it rains hardest, by the shower's rate 6t(1 - t), which
    // peaks at 1.5 halfway through its rain. The draws are bounded, and every
    // shower measured finds its strikes in far fewer.
    for _ in 0..400 {
        if ticks.len() == count {
            break;
        }
        let t = rng.random_range(FIRST..LAST);
        if rng.random_range(0.0..1.5) > 6.0 * t * (1.0 - t) {
            continue;
        }
        let tick = (t * RAINING_TICKS as f64).round() as u64;
        if ticks
            .iter()
            .all(|&other| other.abs_diff(tick) >= STRIKE_GAP)
        {
            ticks.push(tick);
        }
    }
    ticks.sort_unstable();
    ticks
        .into_iter()
        .map(|tick| {
            let count = STROKE_COUNTS[rng.random_range(0..STROKE_COUNTS.len())];
            let mut at = 0;
            let strokes = (0..count)
                .map(|stroke| {
                    let peak = if stroke == 0 {
                        1.0
                    } else {
                        rng.random_range(0.6..1.0)
                    };
                    let this = Stroke { at, peak };
                    at += rng.random_range(STROKE_GAP);
                    this
                })
                .collect();
            Planned {
                tick,
                across: rng.random_range(-1.0..1.0),
                lean: rng.random_range(-LEAN_CELLS..LEAN_CELLS),
                strokes,
                seed: rng.random(),
            }
        })
        .collect()
}

/// A bolt from `top` down to `ground`: a walk that drops the same distance
/// each step, wanders and now and then kinks, and is drawn back toward
/// `ground` so that it arrives there. One to three forks branch off its
/// upper part and die out in the air.
fn bolt(rng: &mut StdRng, top: Vec2, ground: Vec2, layout: &Layout) -> Bolt {
    let limit = open_room(layout);
    let height = top.y - ground.y;
    let steps = ((height / (STEP_CELLS * layout.pitch)).round() as usize).clamp(3, MAX_STEPS);
    let step = height / steps as f32;
    let mut channel = Vec::with_capacity(steps + 1);
    channel.push(top);
    let mut x = top.x;
    for taken in 1..steps {
        let left = steps - taken + 1;
        let pull = (ground.x - x) / left as f32;
        let kink = if rng.random_bool(KINK_CHANCE) {
            2.0
        } else {
            1.0
        };
        let wander = rng.random_range(-1.0..1.0) * WANDER * step * kink;
        // The last steps only close on the ground, so the bolt arrives as
        // steeply as it wandered.
        let wander = if left > 2 { wander } else { 0.0 };
        x = (x + (pull + wander).clamp(-step, step)).clamp(-limit, limit);
        channel.push(Vec2::new(x, top.y - step * taken as f32));
    }
    channel.push(ground);
    // Forks die out above the floor and above what the bolt struck. A long
    // bolt has one to three, a short one fewer or none.
    let lowest = ground.y.max(layout.floor_y) + layout.pitch * 0.3;
    let most = (steps / 8).min(3);
    let forks = (0..rng.random_range(most / 2..=most))
        .filter_map(|_| fork(rng, &channel, step, lowest, limit))
        .collect();
    Bolt {
        channel,
        forks,
        step,
    }
}

/// A fork from somewhere on the upper part of `channel`, leaning to one
/// side as it drops, mostly away from where the channel goes on to strike,
/// and stopping above `lowest`.
fn fork(
    rng: &mut StdRng,
    channel: &[Vec2],
    step: f32,
    lowest: f32,
    limit: f32,
) -> Option<Vec<Vec2>> {
    let segments = channel.len() - 1;
    let first = (segments * 15 / 100).max(1);
    let last = (segments * 60 / 100).max(first + 1);
    let from = rng.random_range(first..last);
    let onward = channel[segments].x - channel[from].x;
    let side = if rng.random_bool(0.75) {
        -onward.signum()
    } else {
        onward.signum()
    };
    let mut point = channel[from];
    let mut fork = vec![point];
    for _ in 0..rng.random_range(3..=7) {
        let drop = step * rng.random_range(0.6..1.0);
        let lean = side * rng.random_range(0.35..0.85) + rng.random_range(-0.4..0.4);
        let next = Vec2::new(
            (point.x + (lean * drop).clamp(-1.1 * drop, 1.1 * drop)).clamp(-limit, limit),
            point.y - drop,
        );
        if next.y < lowest {
            break;
        }
        fork.push(next);
        point = next;
    }
    (fork.len() >= 3).then_some(fork)
}

/// The sparks a strike throws: up and out from where it struck.
fn sparks(rng: &mut StdRng) -> Vec<Spark> {
    (0..rng.random_range(7..=12))
        .map(|_| {
            let angle: f32 = rng.random_range(-1.25..1.25);
            let speed: f32 = rng.random_range(5.0..11.0);
            Spark {
                velocity: Vec2::new(angle.sin(), angle.cos()) * speed,
                life: rng.random_range(16..=SPARK_TICKS),
            }
        })
        .collect()
}
