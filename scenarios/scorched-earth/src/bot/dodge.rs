//! Dodging after a hit. A hit shows that the other tank has this tank's range,
//! so its next shell is likely to land near where this tank stands. The tank
//! waits for that shell, traces where it will burst, drives away from that spot
//! while the shell flies, and waits where it stops until the shell lands.
use super::*;
use crate::armor::BLAST_REACH;
use crate::driving::BRAKING_DISTANCE;
use crate::duel::{flight_end, shell_ray_through};

/// How long after the other tank fires a dodging tank starts to move.
const REACTION_TICKS: u64 = FIXED_HZ as u64 / 4;
/// The hull's ends and the wheels lie within this distance of its middle.
const HALF_LENGTH: f32 = 1.9;
/// A shell bursting this far from the tank's middle cannot reach it.
const CLEAR: f32 = BLAST_REACH + HALF_LENGTH;
/// A dodge shorter than this leaves the tank nearly as close to the burst, so
/// the tank stays where it is unless less than this takes it clear.
const SHORTEST: f32 = 2.0;
/// The longest a dodge lasts, whatever becomes of the shell.
const DEADLINE_TICKS: u64 = 6 * FIXED_HZ as u64;

/// What a dodging CPU keeps, from its first hit on.
#[derive(Clone, Default, Serialize)]
pub(super) struct Dodging {
    /// The tick of the last hit that has not yet earned a dodge.
    after: Option<u64>,
    /// The dodge under way.
    active: Option<Dodge>,
    /// Dodges begun.
    pub dodges: u64,
    /// Shells the tank let fly because they would burst out of its reach.
    pub passed: u64,
    /// Shells the tank did not dodge for want of room or support.
    pub refused: u64,
    last: Option<DodgeOutcome>,
}

/// A dodge under way.
#[derive(Clone, Serialize)]
struct Dodge {
    /// The launch the tank is dodging.
    shell: u64,
    started: u64,
    origin: Vec2,
    /// Where the traced shell bursts.
    burst: Vec2,
    direction: f32,
    /// How far the tank must drive to be out of the burst's reach.
    needed: f32,
    /// How far the tank may drive.
    room: f32,
    /// What sets `room`: ground the drive check refuses, or else the range the
    /// tank keeps and the edge of the field.
    limit: &'static str,
    /// Why the tank stopped short of where the shell lands, once it has.
    stopped: Option<&'static str>,
}

/// How the last dodge, or the last shell the tank did not dodge, went.
#[derive(Clone, Serialize)]
struct DodgeOutcome {
    shell: u64,
    started: u64,
    ended: u64,
    origin: Vec2,
    position: Vec2,
    reason: &'static str,
}

impl Bot {
    /// Whether the tank is dodging a shell now.
    pub(super) fn dodging(&self) -> bool {
        self.dodging
            .as_ref()
            .is_some_and(|dodging| dodging.active.is_some())
    }

    /// Earns a dodge with each hit, begins it once the other tank's next shell
    /// has flown for the reaction time, and drives it. Returns the drive axis
    /// while the dodge lasts, which is a stop once the tank has driven as far
    /// as it will, and a stop when the dodge ends.
    pub(super) fn dodge(
        &mut self,
        state: &Duel,
        physics: &PhysicsWorld,
        player: usize,
    ) -> Option<f32> {
        if self.hit_serial != self.considered_hit {
            self.considered_hit = self.hit_serial;
            // A hit during a dodge earns the next one.
            self.dodging.get_or_insert_default().after = self.last_hit.map(|(tick, _)| tick);
        }
        let dodging = self.dodging.as_mut()?;
        if dodging.active.is_none() {
            let shell = state.incoming(player, dodging.after?)?;
            let launch = shell.flight.expect("an incoming shell was fired").launch;
            if state.tick < launch.tick + REACTION_TICKS {
                return None;
            }
            dodging.after = None;
            let geometry = Geometry::read(state, physics, player);
            let origin = geometry.shooter;
            let outcome = |reason| DodgeOutcome {
                shell: launch.id,
                started: state.tick,
                ended: state.tick,
                origin,
                position: origin,
                reason,
            };
            let Some(burst) = state
                .landing(physics, player, shell)
                .filter(|burst| burst.distance_to(origin) < CLEAR)
            else {
                dodging.passed += 1;
                dodging.last = Some(outcome("will miss"));
                return None;
            };
            let offset = origin.x - burst.x;
            let needed = CLEAR - offset.abs();
            let toward = (geometry.target.x - origin.x).signum();
            // Away from the burst. One nearly under the tank may be left
            // either way, toward the shooter first.
            let directions = if offset.abs() > 0.5 {
                [offset.signum(), 0.0]
            } else {
                [toward, -toward]
            };
            let supported = state.has_track_support(physics, player);
            let way = directions
                .into_iter()
                .filter(|&direction| supported && direction != 0.0)
                .map(|direction| {
                    let room = state.dodge_room(physics, player, direction, needed);
                    (direction, room)
                })
                .find(|&(_, room)| room >= needed.min(SHORTEST));
            let Some((direction, room)) = way else {
                dodging.refused += 1;
                dodging.last = Some(outcome(if supported { "no room" } else { "no support" }));
                return None;
            };
            dodging.dodges += 1;
            dodging.active = Some(Dodge {
                shell: launch.id,
                started: state.tick,
                origin,
                burst,
                direction,
                needed,
                room,
                limit: "out of room",
                stopped: None,
            });
        }
        let dodge = dodging.active.as_mut().unwrap();
        let position = Geometry::read(state, physics, player).shooter;
        let travel = (position.x - dodge.origin.x).abs();
        if dodge.stopped.is_none() {
            // The ground ahead is checked again as the tank drives, but only
            // the ground the rest of the dodge crosses can stop it.
            let (ground, refusal) = state.ground_ahead(physics, player, dodge.direction);
            if let Some(refusal) = refusal.filter(|_| travel + ground <= dodge.room) {
                dodge.room = travel + ground;
                dodge.limit = refusal;
            }
            dodge.stopped = if travel >= dodge.needed {
                Some("out of reach")
            } else if travel >= dodge.room {
                Some(dodge.limit)
            } else {
                None
            };
        }
        let landed = !state.in_flight(dodge.shell);
        if !landed && state.tick < dodge.started + DEADLINE_TICKS {
            // Once it stops, the tank waits for the shell where it is, so
            // that its ordinary driving does not take it back under it.
            self.mode = "dodging";
            return Some(if dodge.stopped.is_some() {
                0.0
            } else {
                dodge.direction
            });
        }
        let reason = dodge
            .stopped
            .unwrap_or(if landed { "shell landed" } else { "deadline" });
        dodging.last = Some(DodgeOutcome {
            shell: dodge.shell,
            started: dodge.started,
            ended: state.tick,
            origin: dodge.origin,
            position,
            reason,
        });
        dodging.active = None;
        Some(0.0)
    }
}

impl Duel {
    /// Whether each tank's CPU is dodging a shell.
    pub(crate) fn dodging(&self) -> [bool; 2] {
        self.bots.each_ref().map(Bot::dodging)
    }

    /// The first shell the other tank fired at `tick` or later that is still
    /// in flight.
    fn incoming(&self, player: usize, tick: u64) -> Option<&Shell> {
        self.shells
            .iter()
            .filter(|shell| shell.owner != player)
            .filter(|shell| {
                shell
                    .flight
                    .is_some_and(|flight| flight.launch.tick >= tick)
            })
            .min_by_key(|shell| shell.flight.map(|flight| flight.launch.tick))
    }

    /// Whether the shell launched as `id` is still in flight.
    fn in_flight(&self, id: u64) -> bool {
        self.shells
            .iter()
            .any(|shell| shell.flight.is_some_and(|flight| flight.launch.id == id))
    }

    /// Where `shell` will burst if `player`'s hull is not in its way, flown on
    /// from where it is as fired shells fly. The wheels are bodies of their
    /// own, and a shell that would strike one bursts there, close enough to
    /// where it would land. There is none if the shell leaves the field or
    /// expires first.
    fn landing(&self, physics: &PhysicsWorld, player: usize, shell: &Shell) -> Option<Vec2> {
        // The shell is past the age at which it passes through its shooter.
        let hull = Some(self.tanks[player].body.entity);
        let (mut position, mut velocity, mut age) = (shell.position, shell.velocity, shell.age);
        loop {
            let (movement, hit) =
                shell_ray_through(physics, &self.placement, hull, position, velocity);
            if let Some(hit) = hit {
                return Some(self.placement.point_from_world(hit.point));
            }
            position += movement;
            velocity.y -= GRAVITY * DT;
            age += 1;
            if flight_end(&self.placement, position, age).is_some() {
                return None;
            }
        }
    }

    /// How far `player` may drive in `direction` to dodge: up to `needed`,
    /// inside the drive limit and the range it tolerates, and short of ground
    /// the drive check refuses.
    fn dodge_room(
        &self,
        physics: &PhysicsWorld,
        player: usize,
        direction: f32,
        needed: f32,
    ) -> f32 {
        let geometry = Geometry::read(self, physics, player);
        let x = geometry.shooter.x;
        let away = direction != (geometry.target.x - x).signum();
        let range = self
            .config
            .cpu_range
            .leeway((geometry.target.x - x).abs(), away);
        let edge = self.placement.drive_limit() - x * direction;
        let ground = self.ground_ahead(physics, player, direction).0;
        needed.min(range).min(edge).min(ground)
    }

    /// How far `player` may drive in `direction`, braking at the end, before
    /// its leading wheel reaches ground the drive check refuses, with the
    /// refusal. With none in the check's few units, there is no limit yet.
    fn ground_ahead(
        &self,
        physics: &PhysicsWorld,
        player: usize,
        direction: f32,
    ) -> (f32, Option<&'static str>) {
        let check = self.check_drive_direction(physics, player, direction);
        let Some(reason) = check.reason else {
            return (f32::INFINITY, None);
        };
        // The probes run out from under the tank's middle, and the check
        // stops at the one it refuses, so the ground is good to the probe
        // before that.
        let probes: Vec<_> = check.probes.iter().flatten().collect();
        let good = match probes.as_slice() {
            [first, .., before, _] => (before.x - first.x).abs(),
            _ => 0.0,
        };
        (
            (good - HALF_LENGTH - BRAKING_DISTANCE).max(0.0),
            Some(reason),
        )
    }
}

#[cfg(test)]
mod tests;
