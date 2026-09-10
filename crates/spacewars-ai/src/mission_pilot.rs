//! A world-level coordinator around the existing physical surface tasks.
use crate::{
    BrainReset,
    combat_pilot::{CombatIntent, CombatPilotTelemetry, RulePilotV4},
    flight_pilot::FlightIntent,
    recovery_task::{RecoverShipTask, RecoveryTelemetry, TaskStatus},
    shortest_heading_error,
    tactical_capture::{CaptureTelemetry, TacticalCapturePilot},
};
use engine_common::CombatBreakSettings;
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        PilotLocation, SurfaceSortieAction,
        mission::MissionObservationV1,
        pilot::{LANDING_SITE_COUNT, LandingSiteId, PilotPlanetObservation},
    },
};
use serde::Serialize;

pub const MISSION_POLICY: &str = "material_mission_v3";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionGoal {
    Select,
    Launch,
    Transfer,
    Capture,
    Recover,
    Patrol,
    Hunt,
    Watch,
    AvoidSun,
    Blocked,
}
impl MissionGoal {
    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "choosing a planet",
            Self::Launch => "climbing for transfer",
            Self::Transfer => "travelling to planet",
            Self::Capture => "landing and capture",
            Self::Recover => "recovering ship",
            Self::Patrol => "waiting to resume capture",
            Self::Hunt => "hunting opponent",
            Self::Watch => "following opponent / awaiting ship",
            Self::AvoidSun => "escaping solar heat",
            Self::Blocked => "mission blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissionEvent {
    pub tick: u64,
    pub planet: Option<usize>,
    pub kind: &'static str,
    pub reason: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissionTelemetry {
    pub policy: &'static str,
    pub goal: MissionGoal,
    pub goal_since: u64,
    pub target: Option<usize>,
    pub completed_sorties: u32,
    pub completed_recoveries: u32,
    pub replans: u32,
    pub frame_changes: u32,
    pub reason: Option<&'static str>,
    pub events: Vec<MissionEvent>,
    pub capture: Option<CaptureTelemetry>,
    pub recovery: Option<RecoveryTelemetry>,
    pub avoidance: Option<MissionAvoidance>,
    pub opponent: Option<PlayerId>,
    pub combat: Option<CombatPilotTelemetry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionObstacleId {
    Planet(usize),
    Sun,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MissionAvoidance {
    pub obstacle: MissionObstacleId,
    pub waypoint: Vec2,
}

#[derive(Debug, Clone)]
pub struct MaterialMissionPilot {
    context: BrainReset,
    breaks: CombatBreakSettings,
    telemetry: MissionTelemetry,
    capture: Option<TacticalCapturePilot>,
    recovery: Option<RecoverShipTask>,
    patrol: RulePilotV4,
    deferred: Vec<(usize, u64)>,
    selected_tick: u64,
    best_distance: f32,
    progress_tick: u64,
    seen_losses: u64,
    last_frame: Option<usize>,
    pwm: f32,
    escaping_sun: bool,
    previous_tick: Option<u64>,
    previous_intent: CombatIntent,
}

impl MaterialMissionPilot {
    pub fn new(context: BrainReset, breaks: CombatBreakSettings) -> Self {
        Self {
            context,
            breaks,
            telemetry: MissionTelemetry {
                policy: MISSION_POLICY,
                goal: MissionGoal::Select,
                goal_since: 0,
                target: None,
                completed_sorties: 0,
                completed_recoveries: 0,
                replans: 0,
                frame_changes: 0,
                reason: None,
                events: Vec::new(),
                capture: None,
                recovery: None,
                avoidance: None,
                opponent: None,
                combat: None,
            },
            capture: None,
            recovery: None,
            patrol: RulePilotV4::with_combat_breaks(context, breaks),
            deferred: Vec::new(),
            selected_tick: 0,
            best_distance: f32::INFINITY,
            progress_tick: 0,
            seen_losses: 0,
            last_frame: None,
            pwm: 0.0,
            escaping_sun: false,
            previous_tick: None,
            previous_intent: CombatIntent::default(),
        }
    }
    pub fn reset(&mut self, context: BrainReset) {
        *self = Self::new(context, self.breaks);
    }
    pub fn telemetry(&self) -> &MissionTelemetry {
        &self.telemetry
    }
    pub fn label(&self) -> String {
        let task = if self.telemetry.goal == MissionGoal::Capture {
            self.capture
                .as_ref()
                .map_or(self.telemetry.goal.label(), |c| c.label())
        } else if self.telemetry.goal == MissionGoal::Recover {
            self.telemetry
                .recovery
                .as_ref()
                .map_or(self.telemetry.goal.label(), |r| r.label())
        } else if let Some(combat) = &self.telemetry.combat {
            combat.goal
        } else {
            self.telemetry.reason.unwrap_or(self.telemetry.goal.label())
        };
        self.telemetry
            .target
            .map_or_else(|| task.to_owned(), |p| format!("Planet {p}: {task}"))
    }
    pub fn site_request(&self) -> Option<LandingSiteId> {
        if let Some(task) = &self.recovery {
            return task.site_request();
        }
        if let Some(task) = &self.capture {
            return task.site_request();
        }
        // No surface survey during travel. This invalid bearing explicitly
        // requests no site; the sensor still publishes all physical gates.
        Some(LandingSiteId {
            planet: self.telemetry.target.unwrap_or(0),
            bearing: LANDING_SITE_COUNT,
        })
    }
    fn goal(&mut self, goal: MissionGoal, tick: u64) {
        if self.telemetry.goal != goal {
            self.telemetry.goal = goal;
            self.telemetry.goal_since = tick;
        }
    }
    fn event(&mut self, tick: u64, kind: &'static str, reason: Option<&'static str>) {
        // Bounded history for an indefinitely running interactive scene.
        if self.telemetry.events.len() == 128 {
            self.telemetry.events.remove(0);
        }
        self.telemetry.events.push(MissionEvent {
            tick,
            planet: self.telemetry.target,
            kind,
            reason,
        });
    }
    fn reconsider(&mut self, tick: u64, reason: &'static str, defer: bool) {
        self.event(tick, "replan", Some(reason));
        if defer && let Some(planet) = self.telemetry.target {
            self.deferred.retain(|(p, _)| *p != planet);
            self.deferred.push((planet, tick + 30 * 60));
        }
        self.telemetry.replans += 1;
        self.telemetry.target = None;
        self.capture = None;
        self.goal(MissionGoal::Select, tick);
    }
    pub fn intent(&mut self, o: &MissionObservationV1) -> CombatIntent {
        let c = &o.local.combat;
        let f = &c.recovery.flight;
        let p = &f.pilot;
        if o.version != 1
            || o.local.version != 1
            || c.version != 2
            || c.recovery.version != 1
            || f.version != 2
            || f.flight.version != 1
            || p.version != 1
            || p.owner != self.context.actor
        {
            return CombatIntent::default();
        }
        if self.previous_tick == Some(p.tick) {
            return self.previous_intent;
        }
        if self.last_frame.is_some_and(|index| index != p.planet.index) {
            self.telemetry.frame_changes += 1;
        }
        self.last_frame = Some(p.planet.index);
        let result = self.choose(o);
        self.telemetry.capture = self.capture.as_ref().map(|c| c.telemetry().clone());
        self.telemetry.recovery = self.recovery.as_ref().map(|r| r.telemetry().clone());
        self.previous_tick = Some(p.tick);
        self.previous_intent = result;
        result
    }
    fn choose(&mut self, o: &MissionObservationV1) -> CombatIntent {
        self.telemetry.avoidance = None;
        self.telemetry.opponent = None;
        self.telemetry.combat = None;
        self.telemetry.reason = None;
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        let losses = p.recovery.as_ref().map_or(0, |r| r.ships_lost);
        let replacing = self.recovery.as_ref().is_some_and(|task| {
            task.telemetry().goal == crate::recovery_task::RecoveryGoal::Scuttle
        });
        if losses > self.seen_losses && !replacing
            || self.recovery.is_none()
                && (!p.ship_available
                    || p.ship_form != ShipForm::Ship
                    || p.location == PilotLocation::OnFoot && self.capture.is_none())
        {
            self.reconsider(p.tick, "ship or surface recovery required", false);
            self.recovery = Some(RecoverShipTask::new(self.context));
        }
        self.seen_losses = losses;
        if p.controls_armed
            && p.ship_available
            && p.location != PilotLocation::OnFoot
            && let Some(sun) = o.sun
        {
            let radial = p.ship.position - sun.position;
            let up = radial.normalized();
            // A fast tangential pass can be safe even beside the inner planet.
            // Predict closest approach instead of treating all nearby flight as
            // an inward fall, and finish one escape before resuming the task.
            let closest_time = (-radial.dot(p.ship.velocity)
                / p.ship.velocity.length_squared().max(0.01))
            .clamp(0.0, 2.0);
            let closest = (radial + p.ship.velocity * closest_time).length();
            if closest < sun.radius + 32.0 {
                self.escaping_sun = true;
            } else if radial.length() > sun.radius + 48.0 && p.ship.velocity.dot(up) >= 0.0 {
                self.escaping_sun = false;
            }
            if self.escaping_sun {
                self.goal(MissionGoal::AvoidSun, p.tick);
                self.telemetry.avoidance = Some(MissionAvoidance {
                    obstacle: MissionObstacleId::Sun,
                    waypoint: sun.position + up * (sun.radius + 110.0),
                });
                return self.guide(o, up * 25.0);
            }
        }
        if let Some(recovery) = &mut self.recovery {
            let flight = recovery.step(&c.recovery);
            let status = recovery.telemetry().status;
            let reason = recovery.telemetry().reason;
            self.goal(
                if status == TaskStatus::Blocked {
                    MissionGoal::Blocked
                } else {
                    MissionGoal::Recover
                },
                p.tick,
            );
            if status == TaskStatus::Succeeded {
                self.telemetry.completed_recoveries += 1;
                self.event(p.tick, "recovered", None);
                self.recovery = None;
                self.goal(MissionGoal::Select, p.tick);
            } else if status == TaskStatus::Blocked {
                self.telemetry.reason = reason;
                self.goal(MissionGoal::Blocked, p.tick);
            }
            return CombatIntent {
                flight,
                ..Default::default()
            };
        }
        if !p.controls_armed {
            return CombatIntent::default();
        }
        self.telemetry.reason = None;
        let owned = |planet: &PilotPlanetObservation| {
            planet
                .claim
                .as_ref()
                .is_some_and(|c| c.owner == Some(p.owner))
        };
        if let Some(target) = self.telemetry.target {
            let planet = o.planets.iter().find(|planet| planet.index == target);
            if planet.is_none() {
                self.reconsider(p.tick, "destination disappeared", true);
            } else if self.capture.is_none() && planet.is_some_and(owned) {
                self.reconsider(p.tick, "destination already secured", false);
            }
        }
        if self.telemetry.target.is_none() {
            self.deferred.retain(|(_, until)| p.tick < *until);
            let candidates: Vec<_> = o
                .planets
                .iter()
                .filter(|planet| {
                    !owned(planet)
                        && !self
                            .deferred
                            .iter()
                            .any(|(index, _)| *index == planet.index)
                })
                .collect();
            let other = candidates
                .iter()
                .any(|planet| planet.index != p.planet.index);
            let selected = candidates
                .into_iter()
                .filter(|planet| !other || planet.index != p.planet.index)
                .min_by(|a, b| {
                    a.motion
                        .position
                        .distance_to(p.ship.position)
                        .total_cmp(&b.motion.position.distance_to(p.ship.position))
                });
            if let Some(planet) = selected {
                self.telemetry.target = Some(planet.index);
                self.selected_tick = p.tick;
                self.progress_tick = p.tick;
                self.best_distance = f32::INFINITY;
                self.event(p.tick, "selected", None);
            } else {
                if o.planets.iter().any(|planet| !owned(planet)) {
                    self.goal(MissionGoal::Patrol, p.tick);
                    self.telemetry.reason = Some("waiting before another landing attempt");
                    return self.patrol.intent(c);
                }
                return self.hunt(o);
            }
        }
        let target = o
            .planets
            .iter()
            .find(|planet| Some(planet.index) == self.telemetry.target)
            .unwrap();
        let distance = p.ship.position.distance_to(target.motion.position);
        if let Some(capture) = &mut self.capture {
            let t = capture.telemetry();
            // The coordinator owns the interplanetary departure. Once the real
            // claim/boarding milestones are observed and the ship clears this
            // planet, it can route onward without depending on the nearest frame.
            let departed = t.landing.claimed_tick.is_some()
                && t.landing.boarded_tick.is_some()
                && p.location != PilotLocation::OnFoot
                && distance > target.radius + 70.0;
            if t.completed_tick.is_some() || departed {
                self.telemetry.completed_sorties += 1;
                self.event(p.tick, "departed", None);
                self.telemetry.target = None;
                self.capture = None;
                self.goal(MissionGoal::Select, p.tick);
                return CombatIntent::default();
            }
            if let Some(reason) = t.failure {
                self.reconsider(p.tick, reason, true);
                return CombatIntent::default();
            }
            if p.planet.index != target.index && p.location != PilotLocation::OnFoot {
                self.reconsider(p.tick, "left destination approach frame", false);
                return CombatIntent::default();
            }
            let intent = capture.intent(&o.local);
            self.goal(MissionGoal::Capture, p.tick);
            return intent;
        }
        if p.tick.saturating_sub(self.selected_tick) > 60 * 60
            || p.tick.saturating_sub(self.progress_tick) > 20 * 60
        {
            self.reconsider(p.tick, "transfer exhausted its progress budget", true);
            return CombatIntent::default();
        }
        if distance < self.best_distance - 2.0 {
            self.best_distance = distance;
            self.progress_tick = p.tick;
        }
        if p.planet.index == target.index
            && distance < target.radius + 105.0
            && (p.ship.velocity - target.motion.velocity).length() < 18.0
            && p.queries_ready
        {
            self.capture = Some(TacticalCapturePilot::new(self.context, self.breaks));
            self.event(p.tick, "arrived", None);
            self.goal(MissionGoal::Capture, p.tick);
            // Neutral handoff; the next observation surveys local landing sites.
            return CombatIntent::default();
        }
        let up = (p.ship.position - p.planet.motion.position).normalized();
        let altitude = p.ship.position.distance_to(p.planet.motion.position) - p.planet.radius;
        let relative = p.ship.velocity - p.planet.motion.velocity;
        let falling = (-relative.dot(up)).max(0.0);
        let desired = if altitude < 70.0 + falling * falling / 50.0 {
            self.goal(MissionGoal::Launch, p.tick);
            self.progress_tick = p.tick;
            p.planet.motion.velocity + up * 18.0
        } else {
            self.goal(MissionGoal::Transfer, p.tick);
            let entry = target.motion.position
                + (p.ship.position - target.motion.position).normalized() * (target.radius + 85.0);
            let waypoint = self.route_waypoint(o, entry, Some(target.index));
            let delta = waypoint - p.ship.position;
            target.motion.velocity + delta.normalized() * (delta.length() * 0.7).min(55.0)
        };
        self.guide(o, desired)
    }
    fn hunt(&mut self, o: &MissionObservationV1) -> CombatIntent {
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        let Some(opponent) = o.opponent else {
            self.goal(MissionGoal::Watch, p.tick);
            self.telemetry.reason = Some("waiting for opponent");
            return self.patrol.intent(c);
        };
        self.telemetry.opponent = Some(opponent.owner);
        self.goal(
            if c.target.is_some() {
                MissionGoal::Hunt
            } else {
                MissionGoal::Watch
            },
            p.tick,
        );
        let up = (p.ship.position - p.planet.motion.position).normalized();
        let altitude = p.ship.position.distance_to(p.planet.motion.position) - p.planet.radius;
        let relative = p.ship.velocity - p.planet.motion.velocity;
        let falling = (-relative.dot(up)).max(0.0);
        if altitude < 70.0 + falling * falling / 50.0 {
            return self.guide(o, p.planet.motion.velocity + up * 18.0);
        }
        if let Some(target) = c.target
            && !target.ground_occluded
            && target.motion.position.distance_to(p.ship.position) < 250.0
        {
            let intent = self.patrol.intent(c);
            self.telemetry.combat = Some(self.patrol.telemetry().clone());
            return intent;
        }
        // Approach the opponent's actual world position, including their
        // recovery location. Bounds reserve flight room; they never authorize fire.
        let mut entry = opponent.motion.position + opponent.motion.velocity;
        for (position, radius) in o
            .planets
            .iter()
            .map(|planet| (planet.motion.position, planet.radius))
            .chain(o.sun.map(|sun| (sun.position, sun.radius)))
        {
            if entry.distance_to(position) < radius + 110.0 {
                let radial = entry - position;
                let up = if radial.length() > 0.01 {
                    radial.normalized()
                } else {
                    (p.ship.position - position).normalized()
                };
                entry = position + up * (radius + 110.0);
            }
        }
        let waypoint = self.route_waypoint(o, entry, None);
        let delta = waypoint - p.ship.position;
        self.guide(
            o,
            opponent.motion.velocity + delta.normalized() * (delta.length() * 0.7).min(55.0),
        )
    }
    fn route_waypoint(
        &mut self,
        o: &MissionObservationV1,
        entry: Vec2,
        destination: Option<usize>,
    ) -> Vec2 {
        let p = &o.local.combat.recovery.flight.pilot;
        for (id, position, radius) in o
            .planets
            .iter()
            .filter(|planet| Some(planet.index) != destination)
            .map(|planet| {
                (
                    MissionObstacleId::Planet(planet.index),
                    planet.motion.position,
                    planet.radius,
                )
            })
            .chain(
                o.sun
                    .map(|sun| (MissionObstacleId::Sun, sun.position, sun.radius)),
            )
        {
            let delta = entry - p.ship.position;
            let along = ((position - p.ship.position).dot(delta)
                / delta.length_squared().max(0.01))
            .clamp(0.0, 1.0);
            let clearance = position.distance_to(p.ship.position + delta * along);
            if clearance < radius + 65.0 {
                let radial = (p.ship.position - position).normalized();
                let toward = (entry - position).normalized();
                let angle = (radial.x * toward.y - radial.y * toward.x).atan2(radial.dot(toward));
                let turn = if angle.abs() < 0.01 {
                    0.5
                } else {
                    angle.clamp(-0.5, 0.5)
                };
                let waypoint = position + radial.rotate_radians(turn) * (radius + 105.0);
                self.telemetry.avoidance = Some(MissionAvoidance {
                    obstacle: id,
                    waypoint,
                });
                return waypoint;
            }
        }
        entry
    }
    fn guide(&mut self, o: &MissionObservationV1, desired_world: Vec2) -> CombatIntent {
        let f = &o.local.combat.recovery.flight;
        let p = &f.pilot;
        let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
        let brake = desired_world.length() < 10.0
            || p.ship.velocity.length() > desired_world.length() + 4.0;
        let acceleration = (desired_world - p.ship.velocity) * 2.0
            - p.gravity
            - if brake {
                f.flight.limits.braking(relative)
            } else {
                Vec2::ZERO
            };
        let direction = acceleration.normalized();
        let error = shortest_heading_error(direction.rotate_radians(-p.ship.angle));
        let spin = -error * 3.0;
        let assist = if p.landing.assist_strength > 0.0 {
            p.planet.motion.spin
        } else {
            0.0
        };
        let horizontal = ((assist - (spin - (p.ship.spin - spin) * 0.2))
            / f.flight.limits.turn_speed)
            .clamp(-1.0, 1.0);
        let available = f.flight.limits.thrust_acceleration
            * f.flight.limits.thrust_fraction(f.flight.forward_speed);
        let aligned = Vec2::Y.rotate_radians(p.ship.angle).dot(direction) > 0.97;
        let duty = if aligned && available > 0.01 {
            (acceleration.length() / available).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.pwm = (self.pwm + duty).min(2.0);
        let thrust = self.pwm >= 1.0;
        if thrust {
            self.pwm -= 1.0;
        }
        CombatIntent {
            flight: FlightIntent {
                controls: SurfaceSortieAction {
                    horizontal,
                    primary_held: thrust,
                    brake_held: brake,
                    interact_held: false,
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
