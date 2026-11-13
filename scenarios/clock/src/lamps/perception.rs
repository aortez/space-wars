//! What the lamps perceive each tick, from the physics worlds the scene
//! simulates in and from the scene itself: the bodies moving in view, the
//! knocks bodies take, and what changed. Nothing here names an event, a
//! visitor or a resident, so whatever is a body is perceived, including
//! things added later. Perception only reads: it adds nothing to a world,
//! applies no forces and claims nothing, so the scene runs the same whether
//! or not the lamps look.
//!
//! A knock is a body whose contacts changed its velocity sharply in one step.
//! The lamps read the impulses, normal and friction, that the solver gave the
//! body's contacts, and count the change in its velocity beyond gravity along
//! their push, up to the push. That counts the body struck as well as the one
//! that struck. It leaves out a body slowed without a blow, as by a script,
//! and one its contacts only hold against some other pull, which hardly
//! moves. Kinematic and fixed bodies move as they are told, so they are never
//! knocked, though what they strike is.

use engine_common::{
    ClockEventKind, ClockFont, ClockLampImpact, ClockLampMover, ClockLampPercepts,
};
use engine_core::Vec2;
use engine_rapier::world::{BodyId, PhysicsWorld};

use crate::{ClockReading, FIXED_HZ, digits::DIGIT_SLOT_COUNT};

/// The most movers and impacts perceived in a tick.
const MAX_MOVERS: usize = 6;
const MAX_IMPACTS: usize = 4;
/// The most bodies looked at in a tick, across every world. The arena holds
/// at most 255, or 335 under Tanks.
const MAX_BODIES: usize = 512;
/// A body moving at this many cells a second or more is a mover.
const MOVING: f32 = 1.0;
/// A body whose contacts change its velocity by this many cells a second or
/// more in one step takes a knock.
const KNOCK: f32 = 6.0;
/// A body counts as new for about this long after it starts moving.
const NOVELTY_TICKS: f32 = 2.0 * FIXED_HZ as f32;
/// A body that has not moved for this long has stopped, and is new again
/// when it next moves; a shorter pause is part of its moving.
const FORGET_TICKS: u64 = 5 * FIXED_HZ as u64;

/// Where the lamps look: the view, and the size of a cell in it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct View {
    pub min: Vec2,
    pub max: Vec2,
    pub pitch: f32,
}

impl View {
    fn contains(self, point: Vec2) -> bool {
        self.overlaps(point, point)
    }

    fn overlaps(self, min: Vec2, max: Vec2) -> bool {
        max.x >= self.min.x && min.x <= self.max.x && max.y >= self.min.y && min.y <= self.max.y
    }
}

/// What the scene shows the lamps besides its bodies.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Facts {
    pub event: Option<ClockEventKind>,
    /// How many events have started, so a new one is told from the last
    /// even when it is of the same kind.
    pub events_started: u64,
    /// The face's font, and the digit in each of its slots.
    pub font: ClockFont,
    pub digits: [Option<u8>; DIGIT_SLOT_COUNT],
    pub ceiling_open: bool,
    pub reading: Option<ClockReading>,
}

/// A body moving in view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Mover {
    /// The world it moves in, by its place in the scene's list, and its body.
    pub world: usize,
    pub body: BodyId,
    /// The middle of its solid bounds, and its velocity there.
    pub position: Vec2,
    pub velocity: Vec2,
    /// Half the longer side of its solid bounds.
    pub size: f32,
    /// Ticks since it started moving.
    pub age: u64,
    /// Its speed in cells a second times its size in cells, up to twice
    /// that while it is new.
    pub salience: f32,
}

/// A body its contacts knocked in one step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Impact {
    pub world: usize,
    pub body: BodyId,
    /// Where the contact that pushed it hardest touches it, in view.
    pub position: Vec2,
    /// How much its velocity changed along its contacts' push, and no more
    /// than the push, in cells a second.
    pub strength: f32,
}

/// What changed in the scene this tick.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Signals {
    pub event: Option<ClockEventKind>,
    /// The event under way began this tick, though the one before was of
    /// its kind.
    pub event_began: bool,
    /// The event that ended this tick.
    pub event_ended: Option<ClockEventKind>,
    /// The slots whose digit changed this tick, and when the font changed,
    /// every slot that shows a digit.
    pub changed_digits: [bool; DIGIT_SLOT_COUNT],
    pub ceiling_open: bool,
    /// The fraction of the day gone, when the clock has a reading.
    pub time_of_day: Option<f32>,
}

/// Everything the lamps perceive in a tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Percepts {
    /// The most salient movers, most salient first.
    pub movers: Vec<Mover>,
    /// The strongest impacts, strongest first.
    pub impacts: Vec<Impact>,
    pub signals: Signals,
    /// The bodies looked at, across every world.
    pub visited: usize,
}

impl Percepts {
    pub fn state(&self) -> ClockLampPercepts {
        let milli = |value: f32| (value * 1000.0).round() as i32;
        let point = |value: Vec2| [milli(value.x), milli(value.y)];
        ClockLampPercepts {
            movers: self
                .movers
                .iter()
                .map(|mover| ClockLampMover {
                    world: mover.world as u8,
                    entity: mover.body.entity.value(),
                    role: mover.body.role.value(),
                    position_milli: point(mover.position),
                    velocity_milli: point(mover.velocity),
                    size_milli: milli(mover.size),
                    age_ticks: mover.age,
                    salience_milli: milli(mover.salience),
                })
                .collect(),
            impacts: self
                .impacts
                .iter()
                .map(|impact| ClockLampImpact {
                    world: impact.world as u8,
                    entity: impact.body.entity.value(),
                    role: impact.body.role.value(),
                    position_milli: point(impact.position),
                    strength_milli: milli(impact.strength),
                })
                .collect(),
            event: self.signals.event,
            event_began: self.signals.event_began,
            event_ended: self.signals.event_ended,
            changed_digits: (0..DIGIT_SLOT_COUNT)
                .filter(|slot| self.signals.changed_digits[*slot])
                .map(|slot| slot as u8)
                .collect(),
            ceiling_open: self.signals.ceiling_open,
            // Truncated, so the last minutes of the day stay below 1000.
            time_of_day_milli: self.signals.time_of_day.map(|day| (day * 1000.0) as u32),
            bodies_visited: self.visited as u32,
        }
    }
}

/// A body is known by its world's place in the scene's list and its id,
/// since two worlds may use the same ids. A world keeps its place, empty
/// while it is gone. A world that moves to another place, as an event's own
/// world does when the arena adopts it, brings bodies the lamps take for
/// new ones.
type Key = (usize, BodyId);

/// When a body started moving and when it last moved.
#[derive(Debug, Clone, Copy)]
struct Moving {
    since: u64,
    last: u64,
}

/// A body as the lamps saw it at their last look: its velocity, and when it
/// was moving, if it has not stopped since.
#[derive(Debug, Clone, Copy)]
struct Memory {
    key: Key,
    velocity: Vec2,
    moving: Option<Moving>,
}

#[derive(Debug, Default)]
pub(crate) struct Perception {
    tick: u64,
    /// The bodies the last look saw, sorted by key.
    before: Vec<Memory>,
    /// This look's, which becomes `before` for the next.
    now: Vec<Memory>,
    /// The scene at the last look, for telling what changed: its event, by
    /// kind and by how many events had started.
    event: Option<(ClockEventKind, u64)>,
    face: Option<(ClockFont, [Option<u8>; DIGIT_SLOT_COUNT])>,
    percepts: Percepts,
    /// Looks at nothing, for comparing a scene with and without perception.
    #[cfg(test)]
    blind: bool,
}

impl Perception {
    pub fn percepts(&self) -> &Percepts {
        &self.percepts
    }

    /// Looks at the bodies of `worlds` inside `view`, and at the scene.
    /// Each world keeps its place in `worlds`, empty while it is gone.
    pub fn perceive(&mut self, worlds: &[Option<&PhysicsWorld>], view: View, facts: Facts) {
        #[cfg(test)]
        if self.blind {
            return;
        }
        self.tick += 1;
        self.percepts.movers.clear();
        self.percepts.impacts.clear();
        self.percepts.visited = 0;
        self.now.clear();
        for (index, world) in worlds.iter().enumerate() {
            if let Some(world) = world {
                self.look(index, world, view);
            }
        }
        std::mem::swap(&mut self.before, &mut self.now);
        self.before.sort_unstable_by_key(|memory| memory.key);
        self.percepts.signals = self.signals(facts);
    }

    fn look(&mut self, index: usize, world: &PhysicsWorld, view: View) {
        let tick = self.tick;
        for record in world.motions() {
            if self.percepts.visited == MAX_BODIES {
                return;
            }
            self.percepts.visited += 1;
            let key = (index, record.id);
            let velocity = record.motion.linear_velocity;
            let before = self
                .before
                .binary_search_by_key(&key, |memory| memory.key)
                .ok()
                .map(|found| self.before[found]);
            let lately = before
                .and_then(|memory| memory.moving)
                .filter(|moving| tick - moving.last <= FORGET_TICKS);
            let fast = velocity.length() >= MOVING * view.pitch;
            let moving = if fast {
                Some(Moving {
                    since: lately.map_or(tick, |moving| moving.since),
                    last: tick,
                })
            } else {
                lately
            };
            self.now.push(Memory {
                key,
                velocity,
                moving,
            });
            if let Some(moving) = moving.filter(|_| fast) {
                self.consider(world, key, tick - moving.since, view);
            }
            // A body the lamps did not see before has no change to judge.
            if let Some(before) = before {
                self.knock(world, key, before.velocity, velocity, view);
            }
        }
    }

    /// Whether the body's contacts knocked it in the last step, where the
    /// lamps can see it.
    fn knock(&mut self, world: &PhysicsWorld, key: Key, before: Vec2, after: Vec2, view: View) {
        let Some(push) = world.body_contact_push(key.1) else {
            return;
        };
        let pushed = push.velocity_change.length();
        if pushed < KNOCK * view.pitch || !view.contains(push.strongest.position) {
            return;
        }
        // How much its velocity changed beyond gravity along the push, and
        // no more than the push: a body its contacts only hold against some
        // other pull hardly moves.
        let scale = world.gravity_scale(key.1).unwrap_or(1.0);
        let change = after - before - world.gravity() * (scale / FIXED_HZ as f32);
        let strength = change.dot(push.velocity_change / pushed).min(pushed) / view.pitch;
        if strength < KNOCK {
            return;
        }
        let impact = Impact {
            world: key.0,
            body: key.1,
            position: push.strongest.position,
            strength,
        };
        let impacts = &mut self.percepts.impacts;
        let at = impacts
            .iter()
            .position(|other| other.strength < impact.strength)
            .unwrap_or(impacts.len());
        if at < MAX_IMPACTS {
            impacts.insert(at, impact);
            impacts.truncate(MAX_IMPACTS);
        }
    }

    /// Ranks a moving body that is at least partly in view among the movers.
    fn consider(&mut self, world: &PhysicsWorld, key: Key, age: u64, view: View) {
        let Some((min, max)) = world.body_solid_bounds(key.1) else {
            return;
        };
        if !view.overlaps(min, max) {
            return;
        }
        let position = (min + max) * 0.5;
        let size = (max.x - min.x).max(max.y - min.y) * 0.5;
        let velocity = world
            .velocity_at_point(key.1, position)
            .unwrap_or(Vec2::ZERO);
        let novelty = 1.0 + (-(age as f32) / NOVELTY_TICKS).exp();
        let mover = Mover {
            world: key.0,
            body: key.1,
            position,
            velocity,
            size,
            age,
            salience: velocity.length() / view.pitch * (size / view.pitch) * novelty,
        };
        let movers = &mut self.percepts.movers;
        let at = movers
            .iter()
            .position(|other| other.salience < mover.salience)
            .unwrap_or(movers.len());
        if at < MAX_MOVERS {
            movers.insert(at, mover);
            movers.truncate(MAX_MOVERS);
        }
    }

    fn signals(&mut self, facts: Facts) -> Signals {
        // A new font redraws every digit the face shows.
        let changed_digits = match self.face {
            Some((font, digits)) => std::array::from_fn(|slot| {
                let shown = digits[slot].is_some() || facts.digits[slot].is_some();
                digits[slot] != facts.digits[slot] || (font != facts.font && shown)
            }),
            None => [false; DIGIT_SLOT_COUNT],
        };
        let event = facts.event.map(|kind| (kind, facts.events_started));
        let changed = event != self.event;
        let signals = Signals {
            event: facts.event,
            event_began: changed && event.is_some(),
            event_ended: self.event.filter(|_| changed).map(|(kind, _)| kind),
            changed_digits,
            ceiling_open: facts.ceiling_open,
            time_of_day: facts.reading.map(|reading| {
                let seconds = u32::from(reading.hour()) * 3600
                    + u32::from(reading.minute()) * 60
                    + u32::from(reading.second());
                seconds as f32 / 86_400.0
            }),
        };
        self.event = event;
        self.face = Some((facts.font, facts.digits));
        signals
    }
}

#[cfg(test)]
mod tests;
