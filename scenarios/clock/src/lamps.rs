//! The two lamps in the upper corners of the frame, and their moods. Now and
//! then, while the scene is quiet, one lamp flickers, goes out for a few
//! seconds, or narrows its light into a spotlight that sweeps across the
//! reading. The lamps draw from a seeded stream of their own, never the
//! event schedule's, so they never move an event. A mood starts only in a
//! gap long enough to hold it before the next scheduled event, and it gives
//! way within half a second to anything that starts anyway: an event, a
//! visit, a notice or a profile switched to Off.

use engine_common::{
    ClockEventProfile, ClockLampMood, ClockLampMoodState, ClockLampSide, ClockLampsState,
};
use engine_core::Vec2;
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{FIXED_HZ, layout::DIGIT_ORIGINS};

/// Quiet seconds between moods, by profile. Off keeps the lamps steady.
const CALM_WAIT_SECONDS: [u32; 2] = [45, 120];
const DEMO_WAIT_SECONDS: [u32; 2] = [12, 24];
/// A mood that gives way is back to the steady look this quickly.
pub(crate) const YIELD_TICKS: u32 = 30;
/// Room a mood leaves before the next scheduled event.
const MARGIN_TICKS: u64 = 30;

/// The spotlight's beats: narrowing onto the digit nearest its lamp, resting
/// there, sweeping across the face, holding at the far end and widening back.
const SPOT_NARROW: u32 = 48;
const SPOT_REST: u32 = 48;
const SPOT_SWEEP: u32 = 180;
const SPOT_HOLD: u32 = 60;
const SPOT_WIDEN: u32 = 84;
pub(crate) const SPOTLIGHT_TICKS: u32 =
    SPOT_NARROW + SPOT_REST + SPOT_SWEEP + SPOT_HOLD + SPOT_WIDEN;
/// A bulb coming back from an outage warms up from this brightness.
const WARM_UP_FROM: f32 = 0.55;
const WARM_UP_TICKS: u32 = 24;
pub(crate) const SPARK_TICKS: u32 = 24;

/// How a lamp looks this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Look {
    /// The bulb's brightness: 1 is steady, 0 is out.
    pub brightness: f32,
    /// 0 is the lamp's steady wash of light, 1 a full spotlight.
    pub narrow: f32,
    /// Where the spotlight points, in face units from the face's lower-left
    /// corner. It means nothing while `narrow` is zero.
    pub target: Vec2,
}

impl Look {
    pub const STEADY: Self = Self {
        brightness: 1.0,
        narrow: 0.0,
        target: Vec2::ZERO,
    };
}

/// What the lamps need to know about the rest of the scene each tick.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Scene {
    pub profile: ClockEventProfile,
    /// The clock has a reading and nothing else is going on: no event or
    /// its cooldown, no visit, no notice and no ceiling opening.
    pub quiet: bool,
    /// Ticks until the schedule's next event, when one is scheduled.
    pub next_event_in: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mood {
    pub kind: ClockLampMood,
    /// 0 is the left lamp, 1 the right.
    pub lamp: usize,
    pub tick: u32,
    pub ticks: u32,
    /// Asked for by name rather than drawn, so it runs even while the
    /// profile is Off, as an event preview does.
    preview: bool,
    /// Flicker and outage: the bulb's brightness from each tick on, until
    /// the next step.
    steps: Vec<(u32, f32)>,
    /// From this tick on, the bulb warms back up to steady.
    warm_from: u32,
    /// An occasional spark burst as the filament finally goes dark.
    spark_at: Option<u32>,
    /// Spotlight: where the beam settles first, and where its sweep ends.
    sweep: [Vec2; 2],
    /// The tick at which it gave way, and how it looked then.
    yielded: Option<(u32, Look)>,
}

impl Mood {
    fn draw(rng: &mut StdRng, kind: ClockLampMood, lamp: usize, preview: bool) -> Self {
        let mut mood = Self {
            kind,
            lamp,
            tick: 0,
            ticks: 0,
            preview,
            steps: Vec::new(),
            warm_from: u32::MAX,
            spark_at: None,
            sweep: [Vec2::ZERO; 2],
            yielded: None,
        };
        let mut at = 0;
        match kind {
            ClockLampMood::Flicker => {
                for _ in 0..rng.random_range(2..=5) {
                    mood.steps.push((at, rng.random_range(0.45..0.85)));
                    at += rng.random_range(2..=6);
                    mood.steps.push((at, 1.0));
                    at += rng.random_range(4..=14);
                }
            }
            ClockLampMood::Outage => {
                // The filament catches a few times on the way out and on the
                // way back.
                for flash in 0..rng.random_range(3..=6) {
                    let level = if flash % 2 == 0 {
                        rng.random_range(0.0..0.3)
                    } else {
                        rng.random_range(0.55..1.0)
                    };
                    mood.steps.push((at, level));
                    at += rng.random_range(3..=9);
                }
                let dark_from = at;
                mood.steps.push((at, 0.0));
                at += rng.random_range(150..=300);
                for flash in 0..rng.random_range(2..=4) {
                    let level = if flash % 2 == 0 {
                        rng.random_range(0.4..0.9)
                    } else {
                        rng.random_range(0.0..0.15)
                    };
                    mood.steps.push((at, level));
                    at += rng.random_range(3..=8);
                }
                mood.warm_from = at;
                at += WARM_UP_TICKS;
                if rng.random_bool(0.5) {
                    mood.spark_at = Some(dark_from);
                }
            }
            ClockLampMood::Spotlight => {
                // Settle on the digit nearest the lamp, then sweep to one of
                // the far two.
                let middle = |slot: usize| DIGIT_ORIGINS[slot] + 3.0;
                let (near, far) = if lamp == 0 {
                    (0, rng.random_range(2..=3))
                } else {
                    (3, rng.random_range(0..=1))
                };
                mood.sweep = [near, far]
                    .map(|slot| Vec2::new(middle(slot), 4.5 + rng.random_range(-1.0..1.0)));
                at = SPOTLIGHT_TICKS;
            }
        }
        mood.ticks = at;
        mood
    }

    pub fn look(&self) -> Look {
        match self.yielded {
            Some((at, from)) => giving_way(from, (self.tick - at) as f32 / YIELD_TICKS as f32),
            None => self.planned(self.tick),
        }
    }

    pub fn yielding(&self) -> bool {
        self.yielded.is_some()
    }

    fn planned(&self, tick: u32) -> Look {
        match self.kind {
            ClockLampMood::Spotlight => {
                let (narrow, target) = self.beam(tick);
                Look {
                    brightness: 1.0 + 0.15 * narrow,
                    narrow,
                    target,
                }
            }
            ClockLampMood::Flicker | ClockLampMood::Outage => Look {
                brightness: self.level(tick),
                ..Look::STEADY
            },
        }
    }

    fn level(&self, tick: u32) -> f32 {
        if tick >= self.warm_from {
            let t = (tick - self.warm_from) as f32 / WARM_UP_TICKS as f32;
            return WARM_UP_FROM + (1.0 - WARM_UP_FROM) * t.min(1.0);
        }
        self.steps
            .iter()
            .rev()
            .find(|(at, _)| *at <= tick)
            .map_or(1.0, |(_, level)| *level)
    }

    fn beam(&self, tick: u32) -> (f32, Vec2) {
        let [first, last] = self.sweep;
        let mut t = tick;
        if t < SPOT_NARROW {
            return (ease(t as f32 / SPOT_NARROW as f32), first);
        }
        t -= SPOT_NARROW;
        if t < SPOT_REST {
            return (1.0, first);
        }
        t -= SPOT_REST;
        if t < SPOT_SWEEP {
            return (
                1.0,
                first + (last - first) * ease(t as f32 / SPOT_SWEEP as f32),
            );
        }
        t -= SPOT_SWEEP;
        if t < SPOT_HOLD {
            return (1.0, last);
        }
        t -= SPOT_HOLD;
        (1.0 - ease(t as f32 / SPOT_WIDEN as f32), last)
    }

    /// The return always takes the whole `YIELD_TICKS`, even from the last
    /// few ticks of a mood, so a dark bulb never snaps back on.
    fn give_way(&mut self) {
        if self.yielded.is_none() {
            self.yielded = Some((self.tick, self.look()));
            self.ticks = self.tick + YIELD_TICKS;
        }
    }
}

/// A lamp returning to steady from wherever its mood had it. A dark bulb
/// catches once before it warms up, as an outage's does.
fn giving_way(from: Look, t: f32) -> Look {
    let t = t.clamp(0.0, 1.0);
    let brightness = if from.brightness < 0.5 {
        if t < 0.25 {
            0.75
        } else if t < 0.4 {
            0.15
        } else {
            WARM_UP_FROM + (1.0 - WARM_UP_FROM) * (t - 0.4) / 0.6
        }
    } else {
        from.brightness + (1.0 - from.brightness) * ease(t)
    };
    Look {
        brightness,
        narrow: from.narrow * (1.0 - ease(t)),
        target: from.target,
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) struct Lamps {
    rng: StdRng,
    /// Quiet ticks since the last mood ended, or since the run began.
    quiet: u32,
    /// Where the next wait falls within its profile's range, 0..1.
    wait: f32,
    /// The mood that came due, kept until a gap can hold it.
    pending: Option<Mood>,
    mood: Option<Mood>,
    started: [u32; ClockLampMood::ALL.len()],
}

impl Lamps {
    pub fn new(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed ^ 0x4c41_4d50_5321);
        let wait = rng.random();
        Self {
            rng,
            quiet: 0,
            wait,
            pending: None,
            mood: None,
            started: [0; ClockLampMood::ALL.len()],
        }
    }

    #[cfg(test)]
    pub fn mood(&self) -> Option<&Mood> {
        self.mood.as_ref()
    }

    /// How `lamp` looks this tick: 0 is the left lamp, 1 the right.
    pub fn look(&self, lamp: usize) -> Look {
        self.mood
            .as_ref()
            .filter(|mood| mood.lamp == lamp)
            .map_or(Look::STEADY, Mood::look)
    }

    pub fn spark_tick(&self, lamp: usize) -> Option<u32> {
        let mood = self.mood.as_ref()?;
        if mood.lamp != lamp || mood.yielding() {
            return None;
        }
        let tick = mood.tick.checked_sub(mood.spark_at?)?;
        (tick < SPARK_TICKS).then_some(tick)
    }

    pub fn step(&mut self, scene: Scene) {
        let lively = scene.profile != ClockEventProfile::Off;
        if let Some(mood) = &mut self.mood {
            if !scene.quiet || !(lively || mood.preview) {
                mood.give_way();
            }
            mood.tick += 1;
            if mood.tick >= mood.ticks {
                self.mood = None;
                self.quiet = 0;
                self.wait = self.rng.random();
            }
            return;
        }
        let Some(due) = wait_ticks(scene.profile, self.wait) else {
            // Off holds the lamps steady, and a fresh wait starts when it ends.
            self.pending = None;
            self.quiet = 0;
            return;
        };
        if !scene.quiet {
            return;
        }
        self.quiet = self.quiet.saturating_add(1);
        if self.quiet < due {
            return;
        }
        let rng = &mut self.rng;
        let mood = self.pending.get_or_insert_with(|| {
            let kind = choose(rng, scene.profile);
            let lamp = rng.random_range(0..2);
            Mood::draw(rng, kind, lamp, false)
        });
        let ticks = u64::from(mood.ticks) + MARGIN_TICKS;
        if scene.next_event_in.is_none_or(|left| ticks <= left) {
            let mood = self.pending.take().expect("the mood that came due");
            self.begin(mood);
        }
    }

    /// Starts `kind` on `lamp` now, whatever the profile, for previews,
    /// fixtures and captures. It still gives way to anything else starting.
    pub fn preview(&mut self, kind: ClockLampMood, lamp: ClockLampSide) {
        let lamp = match lamp {
            ClockLampSide::Left => 0,
            ClockLampSide::Right => 1,
        };
        let mood = Mood::draw(&mut self.rng, kind, lamp, true);
        self.pending = None;
        self.begin(mood);
    }

    fn begin(&mut self, mood: Mood) {
        self.started[mood.kind as usize] += 1;
        self.mood = Some(mood);
    }

    pub fn state(&self, profile: ClockEventProfile) -> ClockLampsState {
        ClockLampsState {
            mood: self.mood.as_ref().map(|mood| ClockLampMoodState {
                kind: mood.kind,
                lamp: if mood.lamp == 0 {
                    ClockLampSide::Left
                } else {
                    ClockLampSide::Right
                },
                tick: mood.tick,
                ticks: mood.ticks,
                brightness_milli: (mood.look().brightness.max(0.0) * 1000.0).round() as u16,
                yielding: mood.yielding(),
            }),
            flickers: self.started[ClockLampMood::Flicker as usize],
            outages: self.started[ClockLampMood::Outage as usize],
            spotlights: self.started[ClockLampMood::Spotlight as usize],
            due_in_quiet_ticks: self
                .mood
                .is_none()
                .then(|| wait_ticks(profile, self.wait))
                .flatten()
                .map(|due| due.saturating_sub(self.quiet)),
        }
    }
}

fn wait_ticks(profile: ClockEventProfile, wait: f32) -> Option<u32> {
    let [low, high] = match profile {
        ClockEventProfile::Off => return None,
        ClockEventProfile::Calm => CALM_WAIT_SECONDS,
        ClockEventProfile::Demo => DEMO_WAIT_SECONDS,
    };
    let seconds = low as f32 + (high - low) as f32 * wait;
    Some((seconds * FIXED_HZ as f32) as u32)
}

/// Calm keeps the bigger moods occasional; Demo shows them off.
fn choose(rng: &mut StdRng, profile: ClockEventProfile) -> ClockLampMood {
    let [flicker, outage] = match profile {
        ClockEventProfile::Demo => [0.3, 0.65],
        ClockEventProfile::Calm | ClockEventProfile::Off => [0.5, 0.75],
    };
    let roll: f32 = rng.random();
    if roll < flicker {
        ClockLampMood::Flicker
    } else if roll < outage {
        ClockLampMood::Outage
    } else {
        ClockLampMood::Spotlight
    }
}
