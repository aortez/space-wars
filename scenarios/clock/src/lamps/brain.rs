//! What the lamps do with what they perceive. Each lamp keeps to one
//! activity at a time: it rests, notices a mover, tracks it, searches where
//! it was last seen, presents the digits that changed on its side, or is out.
//! A lamp watches a mover it follows settle once it stops and searches for
//! one that leaves the view, unless something else catches its eye first. A
//! knock near a lamp startles it over whatever it is doing. Three drives
//! steer it: interest in what it sees, boredom with what it follows and the
//! heat of its bulb, which builds while it burns bright and cools at rest. A
//! bulb that gets too hot sputters out, and a knock big enough dips the power
//! to both lamps, which takes a hot bulb out too. One lamp is eager and the
//! other lazy, as the run's seed decides.
//!
//! The lamps share the work. Neither follows the mover the other has unless
//! that mover is by far the most salient in view; a mover that crosses the
//! middle passes to the lamp on its new side when that lamp is free; and the
//! two are never out together. A beam's aim and width move under top speeds
//! and accelerations, so it swings with some weight.
//!
//! The brain draws from the lamps' own stream and only reads what they
//! perceive, so it never changes the scene. It tells perception which bodies
//! the lamps hold, so they stay in sight however they rank. Nothing here
//! names an event, visitor or resident.

use engine_common::{
    ClockLampActivity as Activity, ClockLampBrainState, ClockLampDrives, ClockLampTarget,
    ClockLampTemperament,
};
use engine_core::Vec2;
use rand::{Rng, rngs::StdRng};

use super::perception::{Key, MOVING, Mover, Percepts};
use super::{Look, REST_BRIGHTNESS, SPARK_TICKS, SPOT_PEAK, Subject, WARM_UP_FROM, WARM_UP_TICKS};
use crate::{FIXED_HZ, layout::DIGIT_ORIGINS};

#[cfg(test)]
mod tests;

const DT: f32 = 1.0 / FIXED_HZ as f32;

// What a temperament sets, lazy first and eager second.

/// The salience a mover on a lamp's side needs to catch its eye.
const NOTICE_SALIENCE: [f32; 2] = [6.0, 3.0];
/// How quickly interest follows what a lamp sees: the seconds it takes to
/// close most of the gap.
const INTEREST_SECONDS: [f32; 2] = [0.35, 0.12];
/// How long a lamp turns toward a mover before it follows it.
const NOTICE_TICKS: [u32; 2] = [21, 9];
/// How long a lamp follows one mover before it tires of it.
const PATIENCE_SECONDS: [f32; 2] = [6.0, 10.0];
/// The aim's top speed in cells a second, and its acceleration in cells a
/// second squared.
const AIM_SPEED: [f32; 2] = [30.0, 45.0];
const AIM_ACCELERATION: [f32; 2] = [110.0, 180.0];
/// How likely a lamp at rest is to present the digits that changed on its
/// side.
const PRESENT_CHANCE: [f64; 2] = [0.5, 0.9];
/// The seconds of rest between fidgets.
const FIDGET_SECONDS: [[f32; 2]; 2] = [[90.0, 200.0], [45.0, 120.0]];

/// A mover on the other lamp's side catches a lamp's eye at this fraction
/// of its salience.
const FAR_SIDE: f32 = 0.5;
/// Interest below which a lamp lets go of a mover it noticed.
const LOSE_INTEREST: f32 = 0.4;
/// How long a lamp leaves alone a mover it tired of.
const TIRED_TICKS: u64 = 10 * FIXED_HZ as u64;
/// The beam leads a mover by this much of its motion.
const LEAD_SECONDS: f32 = 0.12;
/// The beam's width changes by at most this much of its span a second, and
/// that rate by at most this much a second.
const NARROW_SPEED: f32 = 2.5;
const NARROW_ACCELERATION: f32 = 10.0;
/// How narrow the beam is while a lamp notices, searches and presents. It
/// tracks at its narrowest.
const NOTICE_NARROW: f32 = 0.3;
const SEARCH_NARROW: f32 = 0.75;
pub(crate) const PRESENT_NARROW: f32 = 0.85;
/// A search sweeps this many cells either side of where the mover was last
/// seen, once every `SWEEP_TICKS`, and gives up after `SEARCH_TICKS`.
const SEARCH_CELLS: f32 = 3.0;
const SWEEP_TICKS: f32 = 72.0;
const SEARCH_TICKS: u32 = 2 * FIXED_HZ;
/// A lamp watches a mover it follows settle for this long once it stops,
/// unless something else catches its eye first.
const SETTLE_TICKS: u32 = 45;
/// A present holds its light this long, then widens back to rest.
const PRESENT_TICKS: u32 = 150;
const PRESENT_WIDEN_TICKS: u32 = 40;
/// A mover is on a lamp's side once it is this many cells past the middle.
const MIDDLE_CELLS: f32 = 1.0;
/// Both lamps may follow one mover when it is at least this salient and
/// this many times as salient as the next.
const SHARE_SALIENCE: f32 = 30.0;
const SHARE_RATIO: f32 = 3.0;
/// A knock this strong, in cells a second, startles a lamp at most
/// `STARTLE_NEAR` of the room's height away, and one farther off only as
/// much stronger as it is farther. A lamp startles at most once in
/// `STARTLE_REST` ticks.
const STARTLE_KNOCK: f32 = 12.0;
const STARTLE_NEAR: f32 = 0.5;
const STARTLE_TICKS: u32 = 18;
const STARTLE_REST: u64 = FIXED_HZ as u64;
/// A knock this strong dips the power to both lamps for `DIP_TICKS`, at most
/// once in `DIP_REST` ticks, and takes out a bulb at `DIP_HEAT` or hotter.
const BIG_KNOCK: f32 = 30.0;
const DIP_TICKS: u32 = 16;
const DIP_REST: u64 = 20 * FIXED_HZ as u64;
const DIP_HEAT: f32 = 0.7;
/// Seconds at full spotlight that bring a cold bulb to going out. At rest a
/// bulb sheds nearly two thirds of its heat every `COOL_SECONDS`, so only
/// long bright work takes it out; an outage cools it four times as fast.
const HEAT_SECONDS: f32 = 12.0;
const COOL_SECONDS: f32 = 30.0;
/// A bulb this hot sputters now and then.
const SPUTTER_HEAT: f32 = 0.85;
/// Minutes of rest between outages that come of nothing.
const IDLE_OUTAGE_MINUTES: [f32; 2] = [30.0, 80.0];
/// How far from straight down a resting lamp's light falls, and how far, as
/// a fraction of the room's height.
const HOME_TILT: f32 = 0.6;
const HOME_REACH: f32 = 0.3;

/// Where the lamps hang and what they light, in world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Stage {
    /// Each lamp's head, left then right.
    pub heads: [Vec2; 2],
    /// The room in view.
    pub min: Vec2,
    pub max: Vec2,
    /// The width of a cell.
    pub pitch: f32,
    /// The face's lower-left corner, from which a look's target counts in
    /// cells.
    pub face_origin: Vec2,
}

impl Stage {
    fn middle(&self) -> f32 {
        (self.min.x + self.max.x) * 0.5
    }

    /// The lamp on whose side `x` lies, or None within a cell of the middle.
    fn side(&self, x: f32) -> Option<usize> {
        let margin = MIDDLE_CELLS * self.pitch;
        if x < self.middle() - margin {
            Some(0)
        } else if x > self.middle() + margin {
            Some(1)
        } else {
            None
        }
    }

    /// Where a resting lamp's light falls.
    fn home(&self, lamp: usize) -> Vec2 {
        let inward = if lamp == 0 { 1.0 } else { -1.0 };
        let reach = (self.max.y - self.min.y) * HOME_REACH;
        self.heads[lamp] + Vec2::new(inward * HOME_TILT.sin(), -HOME_TILT.cos()) * reach
    }

    fn within(&self, point: Vec2) -> Vec2 {
        Vec2::new(
            point.x.clamp(self.min.x, self.max.x),
            point.y.clamp(self.min.y, self.max.y),
        )
    }

    fn face(&self, cells: Vec2) -> Vec2 {
        self.face_origin + cells * self.pitch
    }
}

/// How many times the run's lamps have done each thing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub notices: u32,
    pub tracks: u32,
    pub handoffs: u32,
    pub startles: u32,
    pub outages: u32,
    pub presents: u32,
    pub fidgets: u32,
    pub dips: u32,
}

/// A bulb's brightness from each tick on, as a fraction of its resting
/// brightness.
#[derive(Debug, Clone, PartialEq)]
struct Steps {
    steps: Vec<(u32, f32)>,
    tick: u32,
    ticks: u32,
}

impl Steps {
    /// A few quick dips.
    fn flicker(rng: &mut StdRng, dips: std::ops::RangeInclusive<u32>) -> Self {
        let mut steps = Vec::new();
        let mut at = 0;
        for _ in 0..rng.random_range(dips) {
            steps.push((at, rng.random_range(0.45..0.85)));
            at += rng.random_range(2..=6);
            steps.push((at, 1.0));
            at += rng.random_range(4..=14);
        }
        Self {
            steps,
            tick: 0,
            ticks: at,
        }
    }

    fn level(&self) -> f32 {
        self.steps
            .iter()
            .rev()
            .find(|(at, _)| *at <= self.tick)
            .map_or(1.0, |(_, level)| *level)
    }
}

/// A bulb going out: it catches a few times on the way out, stays dark,
/// catches a few times on the way back and warms up.
#[derive(Debug, Clone, PartialEq)]
struct Outage {
    steps: Steps,
    /// From this tick on, the bulb warms back up to rest.
    warm_from: u32,
    /// The tick the filament goes dark, when it throws sparks.
    spark_at: Option<u32>,
}

impl Outage {
    fn draw(rng: &mut StdRng) -> Self {
        let mut steps = Vec::new();
        let mut at = 0;
        for flash in 0..rng.random_range(3..=6) {
            let level = if flash % 2 == 0 {
                rng.random_range(0.0..0.3)
            } else {
                rng.random_range(0.55..1.0)
            };
            steps.push((at, level));
            at += rng.random_range(3..=9);
        }
        let dark_from = at;
        steps.push((at, 0.0));
        at += rng.random_range(150..=300);
        for flash in 0..rng.random_range(2..=4) {
            let level = if flash % 2 == 0 {
                rng.random_range(0.4..0.9)
            } else {
                rng.random_range(0.0..0.15)
            };
            steps.push((at, level));
            at += rng.random_range(3..=8);
        }
        let warm_from = at;
        let spark_at = rng.random_bool(0.5).then_some(dark_from);
        Self {
            steps: Steps {
                steps,
                tick: 0,
                ticks: warm_from + WARM_UP_TICKS,
            },
            warm_from,
            spark_at,
        }
    }

    fn level(&self) -> f32 {
        let tick = self.steps.tick;
        if tick >= self.warm_from {
            let t = (tick - self.warm_from) as f32 / WARM_UP_TICKS as f32;
            return WARM_UP_FROM + (1.0 - WARM_UP_FROM) * t.min(1.0);
        }
        self.steps.level()
    }

    fn done(&self) -> bool {
        self.steps.tick >= self.steps.ticks
    }

    /// Skips to the warm-up, for a lamp told to rest.
    fn end(&mut self) {
        if self.steps.tick < self.warm_from {
            self.steps.tick = self.warm_from;
            self.spark_at = None;
        }
    }
}

/// A startle: a flare that fades, or a quick flicker.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Startle {
    tick: u32,
    flare: f32,
}

impl Startle {
    fn gain(&self) -> f32 {
        if self.flare > 0.0 {
            let fade = 1.0 - self.tick as f32 / STARTLE_TICKS as f32;
            return 1.0 + self.flare * fade * fade;
        }
        match self.tick {
            0..3 => 0.55,
            6..9 => 0.7,
            _ => 1.0,
        }
    }
}

fn dip_level(tick: u32) -> f32 {
    const LOW: f32 = 0.35;
    const HOLD: u32 = 4;
    if tick < HOLD {
        LOW
    } else {
        LOW + (1.0 - LOW) * (tick - HOLD) as f32 / (DIP_TICKS - HOLD) as f32
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Lamp {
    eager: bool,
    activity: Activity,
    /// Ticks since the activity began.
    ticks: u32,
    /// The mover it notices, tracks or searches for.
    target: Option<Key>,
    /// Where that mover was last seen, and its velocity then.
    seen: (Vec2, Vec2),
    /// The mover in its light this tick, where it is and how big, while it
    /// notices or tracks one in view.
    subject: Option<(Vec2, f32)>,
    /// Where a present points, in face cells.
    present_at: Vec2,
    /// A mover it tired of, and the tick until which it leaves it alone.
    tired_of: Option<(Key, u64)>,
    /// Ticks the mover it follows has been still.
    still: u32,
    aim: Vec2,
    aim_velocity: Vec2,
    narrow: f32,
    narrow_velocity: f32,
    interest: f32,
    boredom: f32,
    heat: f32,
    outage: Option<Outage>,
    /// A fidget or a sputter.
    flicker: Option<Steps>,
    startle: Option<Startle>,
    startled_at: Option<u64>,
    dip: Option<u32>,
    /// Ticks of rest left before it fidgets, and before an outage that comes
    /// of nothing.
    fidget_in: u32,
    idle_outage_in: u32,
    look: Look,
}

impl Lamp {
    fn new(rng: &mut StdRng, eager: bool) -> Self {
        let mut lamp = Self {
            eager,
            activity: Activity::Rest,
            ticks: 0,
            target: None,
            seen: (Vec2::ZERO, Vec2::ZERO),
            subject: None,
            present_at: Vec2::ZERO,
            tired_of: None,
            still: 0,
            aim: Vec2::ZERO,
            aim_velocity: Vec2::ZERO,
            narrow: 0.0,
            narrow_velocity: 0.0,
            interest: 0.0,
            boredom: 0.0,
            heat: 0.0,
            outage: None,
            flicker: None,
            startle: None,
            startled_at: None,
            dip: None,
            fidget_in: 0,
            idle_outage_in: 0,
            look: Look::STEADY,
        };
        lamp.fidget_in = lamp.draw_fidget(rng);
        lamp.idle_outage_in = draw_idle_outage(rng);
        lamp
    }

    fn temper(&self) -> usize {
        usize::from(self.eager)
    }

    fn draw_fidget(&self, rng: &mut StdRng) -> u32 {
        let [low, high] = FIDGET_SECONDS[self.temper()];
        (rng.random_range(low..high) * FIXED_HZ as f32) as u32
    }

    /// The mover it holds, which the other lamp leaves alone.
    fn claim(&self) -> Option<Key> {
        matches!(
            self.activity,
            Activity::Notice | Activity::Track | Activity::Search
        )
        .then_some(self.target)
        .flatten()
    }

    /// Whether it can take a mover from the other lamp.
    fn free(&self) -> bool {
        matches!(
            self.activity,
            Activity::Rest | Activity::Notice | Activity::Search | Activity::Present
        ) && self.heat < SPUTTER_HEAT
    }

    fn begin(&mut self, activity: Activity) {
        self.activity = activity;
        self.ticks = 0;
        self.still = 0;
    }

    fn notice(&mut self, mover: &Mover, counts: &mut Counts) {
        self.begin(Activity::Notice);
        self.target = Some((mover.world, mover.body));
        self.seen = (mover.position, mover.velocity);
        self.boredom = 0.0;
        counts.notices += 1;
    }

    fn track(&mut self, counts: &mut Counts) {
        self.begin(Activity::Track);
        counts.tracks += 1;
    }

    /// Follows `mover` straight away, its light already narrowed.
    fn retarget(&mut self, mover: &Mover, appeal: f32, counts: &mut Counts) {
        self.target = Some((mover.world, mover.body));
        self.seen = (mover.position, mover.velocity);
        self.interest = appeal;
        self.boredom = 0.0;
        self.track(counts);
    }

    fn rest(&mut self) {
        self.begin(Activity::Rest);
        self.target = None;
    }

    fn go_out(&mut self, rng: &mut StdRng, counts: &mut Counts) {
        self.begin(Activity::Out);
        self.target = None;
        self.outage = Some(Outage::draw(rng));
        self.flicker = None;
        self.startle = None;
        counts.outages += 1;
    }

    /// Moves interest toward `goal` at the lamp's pace.
    fn attend(&mut self, goal: f32) {
        let rate = 1.0 - (-DT / INTEREST_SECONDS[self.temper()]).exp();
        self.interest += (goal - self.interest) * rate;
    }

    /// Whether a mover of `appeal` catches its eye. Interest left over from
    /// a mover it followed makes it quick to notice, but only a mover that
    /// would catch its eye on its own.
    fn catches(&self, appeal: f32) -> bool {
        appeal >= 1.0 && self.interest >= 1.0
    }

    /// How much `mover` would catch this lamp's eye: 1 is enough to notice.
    /// A mover on the other lamp's side is that lamp's to notice while it
    /// is free.
    fn appeal(&self, lamp: usize, mover: &Mover, stage: &Stage, other_free: bool) -> f32 {
        let side = match stage.side(mover.position.x) {
            Some(side) if side != lamp && other_free => 0.0,
            Some(side) if side != lamp => FAR_SIDE,
            _ => 1.0,
        };
        mover.salience * side / NOTICE_SALIENCE[self.temper()]
    }

    /// The mover that would catch its eye most: not one it tired of, nor
    /// one the other lamp holds unless both may follow it.
    fn best<'a>(
        &self,
        lamp: usize,
        percepts: &'a Percepts,
        stage: &Stage,
        taken: Option<Key>,
        other_free: bool,
    ) -> Option<(&'a Mover, f32)> {
        let shared = shared(percepts);
        percepts
            .movers
            .iter()
            .filter(|mover| {
                let key = (mover.world, mover.body);
                self.tired_of.is_none_or(|(tired, _)| tired != key)
                    && (taken != Some(key) || shared == Some(key))
            })
            .map(|mover| (mover, self.appeal(lamp, mover, stage, other_free)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }

    fn present(&mut self, cells: Vec2, counts: &mut Counts) {
        self.begin(Activity::Present);
        self.target = None;
        self.present_at = cells;
        counts.presents += 1;
    }

    /// Where it points, how that point moves, and how narrow its beam is.
    fn goals(&self, lamp: usize, stage: &Stage) -> (Vec2, Vec2, f32) {
        let (position, velocity) = self.seen;
        match self.activity {
            Activity::Rest | Activity::Startle => (stage.home(lamp), Vec2::ZERO, 0.0),
            Activity::Notice => (stage.within(position), Vec2::ZERO, NOTICE_NARROW),
            Activity::Track => (
                stage.within(position + velocity * LEAD_SECONDS),
                velocity,
                1.0,
            ),
            Activity::Search => {
                // Drift on a little the way it went, and sweep either side.
                let drift = velocity * (self.ticks as f32 * DT).min(0.4);
                let phase = self.ticks as f32 / SWEEP_TICKS * std::f32::consts::TAU;
                let sweep = Vec2::new(phase.sin() * SEARCH_CELLS * stage.pitch, 0.0);
                (
                    stage.within(position + drift + sweep),
                    Vec2::ZERO,
                    SEARCH_NARROW,
                )
            }
            Activity::Present => {
                let narrow = if self.ticks + PRESENT_WIDEN_TICKS < PRESENT_TICKS {
                    PRESENT_NARROW
                } else {
                    0.0
                };
                (stage.face(self.present_at), Vec2::ZERO, narrow)
            }
            // The beam collapses where it was.
            Activity::Out => (self.aim, Vec2::ZERO, 0.0),
        }
    }

    /// Moves the light toward its goals, runs the bulb's effects on, and
    /// works out how the lamp looks and how hot its bulb gets.
    fn act(&mut self, lamp: usize, stage: &Stage) {
        let (goal, goal_velocity, narrow) = self.goals(lamp, stage);
        let temper = self.temper();
        steer(
            &mut self.aim,
            &mut self.aim_velocity,
            (goal, goal_velocity),
            AIM_SPEED[temper] * stage.pitch,
            AIM_ACCELERATION[temper] * stage.pitch,
            (stage.min, stage.max),
        );
        steer_width(&mut self.narrow, &mut self.narrow_velocity, narrow);
        // At rest the light settles exactly, so a resting face stays still.
        if self.activity == Activity::Rest && self.narrow == 0.0 {
            self.aim = stage.home(lamp);
            self.aim_velocity = Vec2::ZERO;
        }
        let mut brightness = match &self.outage {
            Some(outage) => REST_BRIGHTNESS * outage.level(),
            None => REST_BRIGHTNESS * (1.0 - self.narrow) + SPOT_PEAK * self.narrow,
        };
        if let Some(flicker) = &self.flicker {
            brightness *= flicker.level();
        }
        if let Some(startle) = &self.startle {
            brightness *= startle.gain();
        }
        if let Some(tick) = self.dip {
            brightness *= dip_level(tick);
        }
        let brightness = brightness.max(0.0);
        // Burning brighter than rest heats the bulb; resting cools it, the
        // faster the hotter it is.
        let above =
            ((brightness - REST_BRIGHTNESS) / (SPOT_PEAK - REST_BRIGHTNESS)).clamp(0.0, 1.0);
        let cooling = if self.outage.is_some() { 4.0 } else { 1.0 };
        self.heat +=
            DT * (above / HEAT_SECONDS - (1.0 - above) * cooling * self.heat / COOL_SECONDS);
        let face = |point: Vec2| (point - stage.face_origin) * (1.0 / stage.pitch);
        self.look = Look {
            brightness,
            narrow: self.narrow,
            target: if self.narrow > 0.0 {
                face(self.aim)
            } else {
                Vec2::ZERO
            },
            subject: self
                .subject
                .filter(|_| self.narrow > 0.0)
                .map(|(position, size)| Subject {
                    position: face(position),
                    size: size / stage.pitch,
                }),
        };
        // The effects run on.
        if let Some(flicker) = &mut self.flicker {
            flicker.tick += 1;
            if flicker.tick >= flicker.ticks {
                self.flicker = None;
            }
        }
        if let Some(startle) = &mut self.startle {
            startle.tick += 1;
            if startle.tick >= STARTLE_TICKS {
                self.startle = None;
            }
        }
        if let Some(tick) = &mut self.dip {
            *tick += 1;
            if *tick >= DIP_TICKS {
                self.dip = None;
            }
        }
        if let Some(outage) = &mut self.outage {
            outage.steps.tick += 1;
        }
    }

    fn state(&self) -> ClockLampBrainState {
        let milli = |value: f32| (value.max(0.0) * 1000.0).round().min(f32::from(u16::MAX)) as u16;
        ClockLampBrainState {
            temperament: if self.eager {
                ClockLampTemperament::Eager
            } else {
                ClockLampTemperament::Lazy
            },
            state: if self.startle.is_some() {
                Activity::Startle
            } else {
                self.activity
            },
            state_ticks: match &self.startle {
                Some(startle) => startle.tick,
                None => self.ticks,
            },
            target: self.target.map(|(world, body)| ClockLampTarget {
                world: world as u8,
                entity: body.entity.value(),
                role: body.role.value(),
            }),
            aim_milli: [
                (self.aim.x * 1000.0).round() as i32,
                (self.aim.y * 1000.0).round() as i32,
            ],
            narrow_milli: milli(self.narrow),
            brightness_milli: milli(self.look.brightness),
            drives: ClockLampDrives {
                interest_milli: milli(self.interest),
                boredom_milli: milli(self.boredom),
                heat_milli: milli(self.heat),
            },
        }
    }
}

fn draw_idle_outage(rng: &mut StdRng) -> u32 {
    let [low, high] = IDLE_OUTAGE_MINUTES;
    (rng.random_range(low..high) * 60.0 * FIXED_HZ as f32) as u32
}

/// The mover both lamps may follow: the most salient, when it is salient
/// enough and far more so than the next.
fn shared(percepts: &Percepts) -> Option<Key> {
    let first = percepts.movers.first()?;
    let next = percepts.movers.get(1).map_or(0.0, |mover| mover.salience);
    (first.salience >= SHARE_SALIENCE && first.salience >= SHARE_RATIO * next)
        .then_some((first.world, first.body))
}

/// The mover `key` names, among the movers or the bodies the lamps follow.
fn find(percepts: &Percepts, key: Option<Key>) -> Option<&Mover> {
    let key = key?;
    percepts
        .movers
        .iter()
        .chain(&percepts.attended)
        .find(|mover| (mover.world, mover.body) == key)
}

/// Moves `position` toward `goal`, which itself moves at `goal_velocity`, as
/// fast as it can and still stop there, at no more than `speed` and
/// changing its velocity by no more than `acceleration`. It stays within
/// `min` and `max`, braking toward them as it would for a goal, so a goal
/// that runs on into an edge does not carry it past.
fn steer(
    position: &mut Vec2,
    velocity: &mut Vec2,
    (goal, goal_velocity): (Vec2, Vec2),
    speed: f32,
    acceleration: f32,
    (min, max): (Vec2, Vec2),
) {
    let offset = goal - *position;
    let distance = offset.length();
    let approach = if distance > 0.0 {
        offset * ((2.0 * acceleration * distance).sqrt().min(speed) / distance)
    } else {
        Vec2::ZERO
    };
    let mut wanted = clamp_length(goal_velocity + approach, speed);
    // Toward an edge, no faster than it could stop there.
    let brake = |room: f32| (2.0 * acceleration * room.max(0.0)).sqrt();
    wanted.x = wanted
        .x
        .clamp(-brake(position.x - min.x), brake(max.x - position.x));
    wanted.y = wanted
        .y
        .clamp(-brake(position.y - min.y), brake(max.y - position.y));
    *velocity += clamp_length(wanted - *velocity, acceleration * DT);
    *position += *velocity * DT;
    // What braking a tick at a time leaves over.
    position.x = position.x.clamp(min.x, max.x);
    position.y = position.y.clamp(min.y, max.y);
}

/// Moves the beam's width toward `goal` as `steer` moves its aim, and
/// settles it there exactly.
fn steer_width(narrow: &mut f32, velocity: &mut f32, goal: f32) {
    let start = *velocity;
    let offset = goal - *narrow;
    let approach = (2.0 * NARROW_ACCELERATION * offset.abs())
        .sqrt()
        .min(NARROW_SPEED)
        .copysign(offset);
    let step = NARROW_ACCELERATION * DT;
    *velocity += (approach - *velocity).clamp(-step, step);
    *narrow = (*narrow + *velocity * DT).clamp(0.0, 1.0);
    // Settled once close, when it could stop within the tick.
    if (goal - *narrow).abs() < 1e-3 && start.abs() <= step {
        *narrow = goal;
        *velocity = 0.0;
    }
}

fn clamp_length(vector: Vec2, most: f32) -> Vec2 {
    let length = vector.length();
    if length > most {
        vector * (most / length)
    } else {
        vector
    }
}

/// Both lamps' minds.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Brain {
    lamps: [Lamp; 2],
    counts: Counts,
    tick: u64,
    /// The tick of the last dip in the power.
    dipped_at: Option<u64>,
}

impl Brain {
    pub fn new(rng: &mut StdRng) -> Self {
        let eager = rng.random_range(0..2);
        let lamps = [0, 1].map(|lamp| Lamp::new(rng, lamp == eager));
        Self {
            lamps,
            counts: Counts::default(),
            tick: 0,
            dipped_at: None,
        }
    }

    pub fn look(&self, lamp: usize) -> Look {
        self.lamps[lamp].look
    }

    /// How far into its burst of sparks the lamp's look is, if it shows
    /// one.
    pub fn spark_tick(&self, lamp: usize) -> Option<u32> {
        let outage = self.lamps[lamp].outage.as_ref()?;
        // The look shows the tick before the one the outage has run on to.
        let shown = outage.steps.tick.checked_sub(1)?;
        let tick = shown.checked_sub(outage.spark_at?)?;
        (tick < SPARK_TICKS).then_some(tick)
    }

    pub fn counts(&self) -> Counts {
        self.counts
    }

    /// The bodies the lamps notice, follow or search for.
    pub fn attending(&self) -> [Option<Key>; 2] {
        self.lamps.each_ref().map(Lamp::claim)
    }

    #[cfg(test)]
    pub fn activity(&self, lamp: usize) -> Activity {
        self.lamps[lamp].activity
    }

    #[cfg(test)]
    pub fn eager(&self) -> usize {
        usize::from(self.lamps[1].eager)
    }

    pub fn state(&self, lamp: usize) -> ClockLampBrainState {
        self.lamps[lamp].state()
    }

    /// One tick of both lamps' minds. While `lively` is false they rest.
    pub fn step(&mut self, rng: &mut StdRng, percepts: &Percepts, stage: Stage, lively: bool) {
        self.tick += 1;
        if lively {
            self.react(rng, percepts, &stage);
            // The eager lamp makes up its mind first.
            let eager = usize::from(self.lamps[1].eager);
            for lamp in [eager, 1 - eager] {
                self.decide(lamp, rng, percepts, &stage);
            }
            self.hand_off(percepts, &stage);
            self.present(rng, percepts);
        } else {
            for lamp in &mut self.lamps {
                lamp.ticks = lamp.ticks.saturating_add(1);
                lamp.calm();
            }
        }
        for (index, lamp) in self.lamps.iter_mut().enumerate() {
            lamp.subject = matches!(lamp.activity, Activity::Notice | Activity::Track)
                .then(|| find(percepts, lamp.target))
                .flatten()
                .map(|mover| (mover.position, mover.size));
            lamp.act(index, &stage);
            if lamp.outage.as_ref().is_some_and(Outage::done) {
                lamp.outage = None;
                lamp.rest();
            }
        }
    }

    /// Knocks: a big one dips the power to both lamps and takes a hot bulb
    /// out, and one near a lamp startles it.
    fn react(&mut self, rng: &mut StdRng, percepts: &Percepts, stage: &Stage) {
        let Some(strongest) = percepts.impacts.first() else {
            return;
        };
        if strongest.strength >= BIG_KNOCK
            && self.dipped_at.is_none_or(|at| self.tick >= at + DIP_REST)
        {
            self.dipped_at = Some(self.tick);
            self.counts.dips += 1;
            for lamp in &mut self.lamps {
                if lamp.activity != Activity::Out {
                    lamp.dip = Some(0);
                }
            }
            // The hotter bulb goes, if it is hot enough and the other is lit.
            let hotter = usize::from(self.lamps[1].heat > self.lamps[0].heat);
            if self.lamps[hotter].heat >= DIP_HEAT
                && self.lamps[hotter].activity != Activity::Out
                && self.lamps[1 - hotter].activity != Activity::Out
            {
                self.lamps[hotter].go_out(rng, &mut self.counts);
            }
        }
        // A knock near a lamp startles it, a flare that fades or a quick
        // flicker; one farther off has to be as much stronger.
        let near = STARTLE_NEAR * (stage.max.y - stage.min.y);
        for (index, lamp) in self.lamps.iter_mut().enumerate() {
            if lamp.activity == Activity::Out
                || lamp
                    .startled_at
                    .is_some_and(|at| self.tick < at + STARTLE_REST)
            {
                continue;
            }
            let head = stage.heads[index];
            let loudness = |strength: f32, position: Vec2| {
                strength * (near / position.distance_to(head).max(near))
            };
            let Some(loud) = percepts
                .impacts
                .iter()
                .map(|impact| loudness(impact.strength, impact.position))
                .filter(|&loud| loud >= STARTLE_KNOCK)
                .max_by(f32::total_cmp)
            else {
                continue;
            };
            let flare = if rng.random_bool(0.5) {
                0.4 + 0.6 * (loud / BIG_KNOCK).min(1.5)
            } else {
                0.0
            };
            lamp.startle = Some(Startle { tick: 0, flare });
            lamp.startled_at = Some(self.tick);
            self.counts.startles += 1;
        }
    }

    fn decide(&mut self, index: usize, rng: &mut StdRng, percepts: &Percepts, stage: &Stage) {
        let taken = self.lamps[1 - index].claim();
        let other_out = self.lamps[1 - index].activity == Activity::Out;
        let other_free = self.lamps[1 - index].free();
        // A hot bulb takes on nothing new while the other is out, since it
        // could not go out in its turn.
        let reluctant = other_out && self.lamps[index].heat >= SPUTTER_HEAT;
        let tick = self.tick;
        let counts = &mut self.counts;
        let lamp = &mut self.lamps[index];
        lamp.ticks = lamp.ticks.saturating_add(1);
        if lamp.tired_of.is_some_and(|(_, until)| tick >= until) {
            lamp.tired_of = None;
        }
        let patience = PATIENCE_SECONDS[lamp.temper()];
        match lamp.activity {
            Activity::Rest | Activity::Startle => {
                lamp.boredom = (lamp.boredom - DT / patience).max(0.0);
                let best = (!reluctant)
                    .then(|| lamp.best(index, percepts, stage, taken, other_free))
                    .flatten();
                lamp.attend(best.map_or(0.0, |(_, appeal)| appeal));
                if let Some((mover, _)) = best.filter(|&(_, appeal)| lamp.catches(appeal)) {
                    lamp.notice(mover, counts);
                } else if lamp.narrow == 0.0 {
                    lamp.fidget_in = lamp.fidget_in.saturating_sub(1);
                    lamp.idle_outage_in = lamp.idle_outage_in.saturating_sub(1);
                    if lamp.idle_outage_in == 0 {
                        // Never while the other is out: it waits for another.
                        lamp.idle_outage_in = draw_idle_outage(rng);
                        if !other_out {
                            lamp.go_out(rng, counts);
                        }
                    } else if lamp.fidget_in == 0 {
                        lamp.fidget_in = lamp.draw_fidget(rng);
                        if lamp.flicker.is_none() {
                            lamp.flicker = Some(Steps::flicker(rng, 2..=4));
                            counts.fidgets += 1;
                        }
                    }
                }
            }
            Activity::Notice => match find(percepts, lamp.target) {
                Some(mover) => {
                    lamp.seen = (mover.position, mover.velocity);
                    lamp.attend(lamp.appeal(index, mover, stage, other_free));
                    if lamp.interest < LOSE_INTEREST {
                        lamp.rest();
                    } else if lamp.ticks >= NOTICE_TICKS[lamp.temper()] {
                        lamp.track(counts);
                    }
                }
                None => {
                    lamp.attend(0.0);
                    lamp.rest();
                }
            },
            Activity::Track => {
                let found = find(percepts, lamp.target);
                let moving = MOVING * stage.pitch;
                if let Some(mover) = found.filter(|mover| mover.velocity.length() >= moving) {
                    lamp.still = 0;
                    lamp.seen = (mover.position, mover.velocity);
                    lamp.attend(lamp.appeal(index, mover, stage, other_free));
                    lamp.boredom += DT / patience;
                    if lamp.boredom >= 1.0 {
                        lamp.tired_of = lamp.target.map(|key| (key, tick + TIRED_TICKS));
                        lamp.rest();
                    }
                } else {
                    // It stopped or is gone. While its light is narrow, the
                    // lamp moves on to whatever else catches its eye; if
                    // nothing does, it watches a stopped mover settle and
                    // searches for one that is gone.
                    let next = (!reluctant)
                        .then(|| lamp.best(index, percepts, stage, taken, other_free))
                        .flatten()
                        .filter(|(mover, appeal)| {
                            *appeal >= 1.0 && Some((mover.world, mover.body)) != lamp.target
                        });
                    if let Some((mover, appeal)) = next {
                        lamp.retarget(mover, appeal, counts);
                    } else if let Some(mover) = found {
                        lamp.seen = (mover.position, Vec2::ZERO);
                        lamp.still += 1;
                        if lamp.still >= SETTLE_TICKS {
                            lamp.rest();
                        }
                    } else {
                        lamp.begin(Activity::Search);
                    }
                }
            }
            Activity::Search => {
                let moving = MOVING * stage.pitch;
                if let Some(mover) =
                    find(percepts, lamp.target).filter(|mover| mover.velocity.length() >= moving)
                {
                    lamp.seen = (mover.position, mover.velocity);
                    lamp.begin(Activity::Track);
                } else {
                    let best = (!reluctant)
                        .then(|| lamp.best(index, percepts, stage, taken, other_free))
                        .flatten();
                    lamp.attend(best.map_or(0.0, |(_, appeal)| appeal));
                    if let Some((mover, appeal)) = best.filter(|&(_, appeal)| lamp.catches(appeal))
                    {
                        lamp.retarget(mover, appeal, counts);
                    } else if lamp.ticks >= SEARCH_TICKS {
                        lamp.rest();
                    }
                }
            }
            Activity::Present => {
                if lamp.ticks >= PRESENT_TICKS {
                    lamp.rest();
                }
            }
            Activity::Out => lamp.attend(0.0),
        }
        // A bulb too hot goes out, unless the other is out: then it lets go
        // of what it follows and rests until it cools.
        if lamp.heat >= 1.0 && lamp.activity != Activity::Out {
            if other_out {
                if lamp.activity != Activity::Rest {
                    lamp.tired_of = lamp.target.map(|key| (key, tick + TIRED_TICKS));
                    lamp.rest();
                }
            } else {
                lamp.go_out(rng, counts);
            }
        } else if lamp.heat >= SPUTTER_HEAT
            && lamp.activity != Activity::Out
            && lamp.flicker.is_none()
            && rng.random_bool(f64::from(
                0.03 * (lamp.heat - SPUTTER_HEAT) / (1.0 - SPUTTER_HEAT),
            ))
        {
            lamp.flicker = Some(Steps::flicker(rng, 1..=2));
        }
    }

    /// A tracked mover on the other lamp's side passes to that lamp when it
    /// is free and not tired of it: as the mover crosses the middle, or as
    /// that lamp comes free while the mover is already over there.
    fn hand_off(&mut self, percepts: &Percepts, stage: &Stage) {
        for from in 0..2 {
            let to = 1 - from;
            let tired = self.lamps[to]
                .tired_of
                .is_some_and(|(key, _)| Some(key) == self.lamps[from].target);
            if self.lamps[from].activity != Activity::Track || !self.lamps[to].free() || tired {
                continue;
            }
            let Some(mover) = find(percepts, self.lamps[from].target) else {
                continue;
            };
            if stage.side(mover.position.x) != Some(to) {
                continue;
            }
            let (giver, taker) = if from == 0 {
                let (left, right) = self.lamps.split_at_mut(1);
                (&mut left[0], &mut right[0])
            } else {
                let (left, right) = self.lamps.split_at_mut(1);
                (&mut right[0], &mut left[0])
            };
            taker.begin(Activity::Track);
            taker.target = giver.target;
            taker.seen = (mover.position, mover.velocity);
            taker.interest = taker.interest.max(1.0);
            taker.boredom = 0.0;
            giver.rest();
            self.counts.handoffs += 1;
            self.counts.tracks += 1;
        }
    }

    /// A lamp at rest may present the digits that changed on its side.
    fn present(&mut self, rng: &mut StdRng, percepts: &Percepts) {
        let changed = percepts.signals.changed_digits;
        for (index, lamp) in self.lamps.iter_mut().enumerate() {
            let slots: Vec<usize> = (0..changed.len())
                .filter(|&slot| changed[slot] && usize::from(slot >= changed.len() / 2) == index)
                .collect();
            if slots.is_empty()
                || lamp.activity != Activity::Rest
                || !rng.random_bool(PRESENT_CHANCE[lamp.temper()])
            {
                continue;
            }
            let middle = slots
                .iter()
                .map(|&slot| DIGIT_ORIGINS[slot] + 3.0)
                .sum::<f32>()
                / slots.len() as f32;
            lamp.present(Vec2::new(middle, 4.5), &mut self.counts);
        }
    }

    /// Starts `activity` on `lamp` now, for previews, fixtures and captures:
    /// an outage, unless the other lamp is out; a startle; a present of the
    /// digits on its side; or rest. The others need a mover.
    pub fn preview(&mut self, rng: &mut StdRng, activity: Activity, lamp: usize) {
        let other_out = self.lamps[1 - lamp].activity == Activity::Out;
        let counts = &mut self.counts;
        let this = &mut self.lamps[lamp];
        match activity {
            Activity::Out if !other_out => this.go_out(rng, counts),
            Activity::Startle => {
                this.startle = Some(Startle {
                    tick: 0,
                    flare: 1.0,
                });
                counts.startles += 1;
            }
            Activity::Present => {
                let slots = if lamp == 0 { [0, 1] } else { [2, 3] };
                let middle = slots
                    .iter()
                    .map(|&slot| DIGIT_ORIGINS[slot] + 3.0)
                    .sum::<f32>()
                    / 2.0;
                this.present(Vec2::new(middle, 4.5), counts);
            }
            Activity::Rest => this.calm(),
            _ => {}
        }
    }
}

impl Lamp {
    /// Back to rest: it lets go of its mover, and a bulb that is out warms
    /// back up.
    fn calm(&mut self) {
        if self.activity != Activity::Rest && self.activity != Activity::Out {
            self.rest();
        }
        if let Some(outage) = &mut self.outage {
            outage.end();
        }
        self.flicker = None;
        self.startle = None;
        self.dip = None;
        self.interest = 0.0;
        self.boredom = 0.0;
    }
}
