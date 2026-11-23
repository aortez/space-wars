//! Plotted aim. Before each shot, the tank traces one shell for each whole
//! degree of elevation through the current world, flying it as a fired shell
//! flies, and fires the middle of the widest band of arcs that reach the
//! target. An opening between obstacles shows up as such a band.
use super::*;
use crate::duel::{flight_end, shell_ray, struck};

/// The flattest and steepest arcs traced, in degrees. One arc per degree finds
/// an opening a few degrees wide.
pub(super) const LOWEST_DEGREES: u16 = 10;
pub(super) const HIGHEST_DEGREES: u16 = 80;
/// Rays one tank may cast for its plot in a tick. A shell casts one ray a
/// tick, and a plot of every arc casts about 10,000, so a plot spreads over
/// about 20 ticks instead of stalling one.
pub(super) const CASTS_PER_TICK: u32 = 512;
/// Plotting starts this long before the next shot is due, three times what a
/// plot usually takes.
pub(super) const LEAD_TICKS: u64 = 60;
/// A trace ends this long after its arc should have passed the target.
const OVERRUN_TICKS: u32 = 30;
/// A traced burst this close to the target's center reaches the target.
const REACH: f32 = 1.5;
/// A plot that found nothing it could fire is redone this much later.
const RETRY_TICKS: u64 = 60;

/// One traced arc: its elevation and power, and where its shell stopped.
#[derive(Debug, Clone, Copy, Serialize)]
pub(super) struct TracedArc {
    pub elevation: f32,
    pub power: f32,
    pub end: Vec2,
    pub termination: &'static str,
    /// How far from the target's center the shell burst. A hit on the target
    /// is zero. A shell that leaves, expires, passes the target or hits its own
    /// tank bursts nowhere useful and has none.
    pub miss: Option<f32>,
}

impl TracedArc {
    pub fn reaches(self) -> bool {
        self.miss.is_some_and(|miss| miss <= REACH)
    }
}

/// A shell part way through its trace.
#[derive(Debug, Clone, Copy)]
struct Trace {
    elevation: f32,
    power: f32,
    position: Vec2,
    velocity: Vec2,
    age: u32,
    /// The age at which the arc is well past the target.
    limit: u32,
}

/// The tank's pose when its plot began. Every arc launches from it.
#[derive(Debug, Clone, Copy, Serialize)]
struct Pose {
    position: Vec2,
    angle: f32,
    velocity: Vec2,
}

#[derive(Clone, Serialize)]
pub(super) struct Plot {
    shooter: Pose,
    target: Vec2,
    facing: f32,
    pub started: u64,
    pub finished: Option<u64>,
    /// Rays cast so far, barrels included.
    pub casts: u32,
    /// How many traced arcs reach the target.
    pub reaching: u16,
    pub chosen: Option<TracedArc>,
    /// Set once the chosen arc is fired. The next shot gets a new plot.
    pub fired: bool,
    /// The next elevation to launch, in degrees.
    #[serde(skip)]
    next: u16,
    #[serde(skip)]
    trace: Option<Trace>,
    /// Every arc traced so far, from the flattest.
    #[serde(skip)]
    pub arcs: Vec<TracedArc>,
}

impl Plot {
    fn new(state: &Duel, physics: &PhysicsWorld, player: usize) -> Self {
        let motion = state.motion(physics, state.tanks[player].body).unwrap();
        let target = state
            .motion(physics, state.tanks[1 - player].body)
            .unwrap()
            .position;
        Self {
            shooter: Pose {
                position: motion.position,
                angle: motion.angle,
                velocity: motion.linear_velocity,
            },
            target,
            facing: (target.x - motion.position.x).signum(),
            started: state.tick,
            finished: None,
            casts: 0,
            reaching: 0,
            chosen: None,
            fired: false,
            next: LOWEST_DEGREES,
            trace: None,
            arcs: Vec::with_capacity(usize::from(HIGHEST_DEGREES - LOWEST_DEGREES + 1)),
        }
    }

    /// Whether the tank has moved off the plot's pose, by the margins that
    /// also reset a learned aim.
    fn moved(&self, state: &Duel, physics: &PhysicsWorld, player: usize) -> bool {
        let motion = state.motion(physics, state.tanks[player].body).unwrap();
        motion.position.distance_to(self.shooter.position) > 1.0
            || (motion.angle - self.shooter.angle).abs() > 8.0_f32.to_radians()
    }

    /// Traces arcs until this tick's rays are spent or every arc is traced.
    fn advance(&mut self, state: &Duel, physics: &PhysicsWorld, player: usize) {
        let entities = state.tanks.map(|tank| tank.body.entity);
        let mut casts = 0;
        while casts < CASTS_PER_TICK {
            let Some(trace) = &mut self.trace else {
                if self.next > HIGHEST_DEGREES {
                    self.finish(state.tick);
                    break;
                }
                let elevation = f32::from(self.next).to_radians();
                self.next += 1;
                match self.launch(state, physics, player, elevation, &mut casts) {
                    Ok(trace) => self.trace = Some(trace),
                    Err(termination) => self.arcs.push(TracedArc {
                        elevation,
                        power: 0.0,
                        end: self.shooter.position,
                        termination,
                        miss: None,
                    }),
                }
                continue;
            };
            casts += 1;
            let (movement, hit) = shell_ray(
                physics,
                &state.placement,
                entities[player],
                trace.position,
                trace.velocity,
                trace.age,
            );
            let end = if let Some(hit) = hit {
                Some((
                    state.placement.point_from_world(hit.point),
                    struck(entities, player, &hit),
                ))
            } else {
                trace.position += movement;
                trace.velocity.y -= GRAVITY * DT;
                trace.age += 1;
                flight_end(&state.placement, trace.position, trace.age)
                    .or((trace.age >= trace.limit).then_some("passed"))
                    .map(|termination| (trace.position, termination))
            };
            if let Some((end, termination)) = end {
                let miss = match termination {
                    "opponent" => Some(0.0),
                    "terrain" => Some(end.distance_to(self.target)),
                    _ => None,
                };
                self.arcs.push(TracedArc {
                    elevation: trace.elevation,
                    power: trace.power,
                    end,
                    termination,
                    miss,
                });
                self.trace = None;
            }
        }
        self.casts += casts;
    }

    /// The shell this plot fires at `elevation`, as `Duel::fire` would launch
    /// it, unless the barrel is blocked or no power reaches the target.
    fn launch(
        &self,
        state: &Duel,
        physics: &PhysicsWorld,
        player: usize,
        elevation: f32,
        casts: &mut u32,
    ) -> Result<Trace, &'static str> {
        let (pivot, muzzle, direction) = barrel(self.shooter, self.facing, elevation);
        let power = firing_power(muzzle, direction, self.shooter.velocity, self.target)
            .ok_or("unreachable")?
            .clamp(12.0, state.placement.max_power());
        *casts += 1;
        // As in `fire`, the barrel cannot reach through the ground or a tank.
        if state
            .cast_ray(
                physics,
                pivot,
                muzzle - pivot,
                RayCastOptions {
                    max_distance: (muzzle - pivot).length(),
                    exclude_entity: Some(state.tanks[player].body.entity),
                    ..Default::default()
                },
            )
            .is_some()
        {
            return Err("blocked barrel");
        }
        let velocity = direction * power + self.shooter.velocity;
        // Horizontal speed is constant, so the arc reaches the target's X then.
        let ticks = ((self.target.x - muzzle.x) / velocity.x / DT).ceil();
        let limit = if ticks.is_finite() && ticks > 0.0 {
            (ticks as u32).saturating_add(OVERRUN_TICKS)
        } else {
            SHELL_LIFETIME_TICKS
        };
        Ok(Trace {
            elevation,
            power,
            position: muzzle,
            velocity,
            age: 0,
            limit,
        })
    }

    fn finish(&mut self, tick: u64) {
        self.finished = Some(tick);
        self.reaching = self.arcs.iter().filter(|arc| arc.reaches()).count() as u16;
        self.chosen = choose(&self.arcs);
    }

    pub fn mode(&self) -> &'static str {
        match (self.finished, self.chosen) {
            _ if self.fired => "waiting to plot",
            (None, _) => "plotting",
            (Some(_), Some(arc)) if arc.reaches() => "plotted hit",
            (Some(_), Some(_)) => "plotted near miss",
            (Some(_), None) => "no plotted shot",
        }
    }
}

/// The barrel's pivot and muzzle at `elevation` from `pose`, and its direction,
/// as `Duel::muzzle` places them.
fn barrel(pose: Pose, facing: f32, elevation: f32) -> (Vec2, Vec2, Vec2) {
    let direction = Vec2::new(facing * elevation.cos(), elevation.sin());
    let pivot = pose.position + Vec2::Y.rotate_radians(pose.angle) * 0.85;
    (pivot, pivot + direction * 2.8, direction)
}

/// The launch speed along `direction` whose arc from `muzzle` passes through
/// `target` under gravity alone. The tank's own `velocity` carries the shell
/// too, as it does in `fire`. There is none when the barrel points below the
/// target, or away from it.
fn firing_power(muzzle: Vec2, direction: Vec2, velocity: Vec2, target: Vec2) -> Option<f32> {
    let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    let offset = target - muzzle;
    // The shell is at muzzle + (power * direction + velocity) t - (0, g t² / 2)
    // after t seconds. It meets the target when what remains of the offset
    // lies along the barrel: g d.x t² / 2 - (d × v) t + d × offset = 0.
    let a = 0.5 * GRAVITY * direction.x;
    let b = -cross(direction, velocity);
    let c = cross(direction, offset);
    let discriminant = b * b - 4.0 * a * c;
    if a.abs() < 1e-6 || discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let mut times = [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)];
    times.sort_by(f32::total_cmp);
    times.into_iter().filter(|t| *t > 0.0).find_map(|t| {
        let along = offset - velocity * t + Vec2::new(0.0, 0.5 * GRAVITY * t * t);
        let power = along.dot(direction) / t;
        (power > 0.0).then_some(power)
    })
}

/// The middle of the widest band of neighboring arcs that reach the target,
/// which leaves the shot the most room on either side. With no such band, the
/// arc that bursts nearest the target.
fn choose(arcs: &[TracedArc]) -> Option<TracedArc> {
    let mut widest: Option<std::ops::Range<usize>> = None;
    let mut start = 0;
    for (index, arc) in arcs.iter().enumerate() {
        if !arc.reaches() {
            start = index + 1;
        } else if widest
            .as_ref()
            .is_none_or(|band| index + 1 - start > band.len())
        {
            widest = Some(start..index + 1);
        }
    }
    if let Some(band) = widest {
        return Some(arcs[band.start + (band.len() - 1) / 2]);
    }
    arcs.iter()
        .filter(|arc| arc.miss.is_some())
        .min_by(|a, b| a.miss.unwrap().total_cmp(&b.miss.unwrap()))
        .copied()
}

impl Bot {
    /// Advances the plot for this tank's next shot. A plot begins a little
    /// before the shot is due, once the last shell has landed and any dodge is
    /// over, and begins again whenever the tank moves off its pose.
    fn advance_plot(&mut self, state: &Duel, physics: &PhysicsWorld, player: usize) {
        if self.pending.is_some()
            || self.dodging()
            || state.tick + LEAD_TICKS < state.tanks[player].next_shot
        {
            return;
        }
        let redo = self.plot.as_ref().is_none_or(|plot| {
            plot.fired
                || plot.moved(state, physics, player)
                || plot
                    .finished
                    .is_some_and(|tick| plot.chosen.is_none() && state.tick >= tick + RETRY_TICKS)
        });
        if redo {
            self.plot = Some(Plot::new(state, physics, player));
        }
        let plot = self.plot.as_mut().unwrap();
        if plot.finished.is_none() {
            plot.advance(state, physics, player);
        }
        if self.relocation.is_none() && state.tick >= self.hold_until {
            self.mode = plot.mode();
        }
    }

    /// The arc a finished plot chose for the next shot.
    pub(super) fn plotted_arc(&self) -> Option<TracedArc> {
        self.plot
            .as_ref()
            .filter(|plot| plot.finished.is_some() && !plot.fired)
            .and_then(|plot| plot.chosen)
    }

    /// Whether the last finished plot found an arc that reaches the target.
    pub(super) fn plotted_hit(&self) -> bool {
        self.plot
            .as_ref()
            .and_then(|plot| plot.chosen)
            .is_some_and(TracedArc::reaches)
    }
}

impl Duel {
    pub(crate) fn advance_plot(&mut self, physics: &PhysicsWorld, player: usize) {
        if self.config.cpu_policy[player] != CpuPolicy::Plotted
            || !self.calibrated_cpu(player)
            || !(self.config.demo || player != self.selected)
        {
            return;
        }
        let mut bot = std::mem::take(&mut self.bots[player]);
        bot.advance_plot(self, physics, player);
        self.bots[player] = bot;
    }

    /// Points the barrel along the plotted arc. Its power reaches where the
    /// target is now, which is the plotted power unless the target has moved.
    pub(super) fn aim_plotted(&mut self, physics: &PhysicsWorld, player: usize) {
        let arc = self.bots[player]
            .plotted_arc()
            .expect("a plotted tank fires only a plotted arc");
        let facing = self.bots[player].plot.as_ref().unwrap().facing;
        let motion = self.motion(physics, self.tanks[player].body).unwrap();
        let target = self
            .motion(physics, self.tanks[1 - player].body)
            .unwrap()
            .position;
        let pose = Pose {
            position: motion.position,
            angle: motion.angle,
            velocity: motion.linear_velocity,
        };
        let (_, muzzle, direction) = barrel(pose, facing, arc.elevation);
        let tank = &mut self.tanks[player];
        tank.facing = facing;
        tank.elevation = arc.elevation;
        tank.speed = firing_power(muzzle, direction, pose.velocity, target)
            .map_or(arc.power, |power| {
                power.clamp(12.0, self.placement.max_power())
            });
    }
}

#[cfg(test)]
mod tests;
