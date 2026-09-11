//! Wing-aware circuit followed by the established material landing policy.
//! All guidance emits the same held controls used by a human pilot.
use crate::{
    BrainReset,
    pilot::{PilotBrain, PilotGoal, PilotTelemetry, RulePilotV1},
    shortest_heading_error,
};
use engine_common::Action;
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        LandingPhase, SurfaceSortieAction, SurfaceWingAction, flight::PilotObservationV2,
        pilot::LandingSiteId,
    },
};
use serde::Serialize;

pub const RULE_PILOT_V2_POLICY_ID: &str = "rule_pilot_v2";
const ORBIT_HEIGHT: f32 = 200.0;
const ORBIT_SPEED: f32 = 90.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FlightIntent {
    pub controls: SurfaceSortieAction,
    pub wings: SurfaceWingAction,
}
impl FlightIntent {
    pub fn encode(self, owner: PlayerId) -> [Action; 2] {
        [self.controls.encode(owner), self.wings.encode(owner)]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlightGoal {
    #[default]
    Grounded,
    Launch,
    Circuit,
    Brake,
    Return,
    Sortie,
    Complete,
    Blocked,
}
impl FlightGoal {
    pub fn label(self) -> &'static str {
        match self {
            Self::Grounded => "settling before takeoff",
            Self::Launch => "taking off / climbing",
            Self::Circuit => "flying swept-wing circuit",
            Self::Brake => "opening wings / braking",
            Self::Return => "returning to ground",
            Self::Sortie => "landing and capture",
            Self::Complete => "flight complete / holding",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlightTelemetry {
    pub policy: &'static str,
    pub goal: FlightGoal,
    pub direction: f32,
    pub goal_since: u64,
    pub blocked_reason: Option<&'static str>,
    pub grounded_tick: Option<u64>,
    pub circuit_tick: Option<u64>,
    pub braked_tick: Option<u64>,
    pub returned_tick: Option<u64>,
    pub completed_tick: Option<u64>,
    pub circuit_radians: f32,
    pub peak_relative_speed: f32,
    pub fast_swept_ticks: u64,
    pub landing: PilotTelemetry,
}

#[derive(Debug, Clone)]
pub struct RulePilotV2 {
    context: BrainReset,
    telemetry: FlightTelemetry,
    landing: RulePilotV1,
    last_bearing: Option<f32>,
    thrust_fraction: f32,
    previous_tick: Option<u64>,
    previous_intent: FlightIntent,
}
impl RulePilotV2 {
    pub fn new(context: BrainReset) -> Self {
        Self::with_direction(
            context,
            if context.actor.index() == 0 {
                1.0
            } else {
                -1.0
            },
        )
    }
    pub fn with_direction(context: BrainReset, direction: f32) -> Self {
        assert!(direction == 1.0 || direction == -1.0);
        let landing = RulePilotV1::new(context);
        Self {
            context,
            telemetry: FlightTelemetry {
                policy: RULE_PILOT_V2_POLICY_ID,
                goal: FlightGoal::Grounded,
                direction,
                goal_since: 0,
                blocked_reason: None,
                grounded_tick: None,
                circuit_tick: None,
                braked_tick: None,
                returned_tick: None,
                completed_tick: None,
                circuit_radians: 0.0,
                peak_relative_speed: 0.0,
                fast_swept_ticks: 0,
                landing: landing.telemetry().clone(),
            },
            landing,
            last_bearing: None,
            thrust_fraction: 0.0,
            previous_tick: None,
            previous_intent: FlightIntent::default(),
        }
    }
    pub fn reset(&mut self, context: BrainReset) {
        *self = Self::with_direction(context, self.telemetry.direction);
    }
    pub fn telemetry(&self) -> &FlightTelemetry {
        &self.telemetry
    }
    pub fn site_request(&self) -> Option<LandingSiteId> {
        // During cruise no landing survey is needed. One bounded site probe
        // keeps the additive V1 sensor contract without scanning all bearings.
        self.landing.site_request().or_else(|| {
            (!matches!(
                self.telemetry.goal,
                FlightGoal::Sortie | FlightGoal::Complete
            ))
            .then_some(LandingSiteId {
                planet: 0,
                bearing: 0,
            })
        })
    }
    pub fn label(&self) -> &'static str {
        self.telemetry.blocked_reason.unwrap_or_else(|| {
            if self.telemetry.goal == FlightGoal::Sortie {
                self.telemetry.landing.goal.label()
            } else {
                self.telemetry.goal.label()
            }
        })
    }
    fn goal(&mut self, goal: FlightGoal, tick: u64) {
        if self.telemetry.goal != goal {
            self.telemetry.goal = goal;
            self.telemetry.goal_since = tick;
            self.thrust_fraction = 0.0;
        }
    }
    fn blocked(&mut self, reason: &'static str, tick: u64) {
        self.goal(FlightGoal::Blocked, tick);
        self.telemetry.blocked_reason = Some(reason);
    }
    pub fn intent(&mut self, o: &PilotObservationV2) -> FlightIntent {
        if o.version != 2
            || o.flight.version != 1
            || o.pilot.version != 1
            || o.pilot.owner != self.context.actor
        {
            self.blocked("observation identity/version mismatch", o.pilot.tick);
            return FlightIntent::default();
        }
        if self.previous_tick == Some(o.pilot.tick) {
            return self.previous_intent;
        }
        let action = self.choose(o);
        self.previous_tick = Some(o.pilot.tick);
        self.previous_intent = action;
        action
    }
    fn choose(&mut self, o: &PilotObservationV2) -> FlightIntent {
        let p = &o.pilot;
        if !p.controls_armed {
            return FlightIntent::default();
        }
        if !p.ship_available || p.ship_form != ShipForm::Ship || !o.flight.enabled {
            self.blocked("vehicle recovery is outside pilot v2", p.tick);
            return FlightIntent::default();
        }
        let deadline = match self.telemetry.goal {
            FlightGoal::Launch => Some(30 * 60),
            FlightGoal::Brake => Some(15 * 60),
            FlightGoal::Return => Some(45 * 60),
            _ => None,
        };
        if deadline.is_some_and(|limit| p.tick.saturating_sub(self.telemetry.goal_since) > limit) {
            self.blocked("flight phase exceeded its time budget", p.tick);
        }
        self.telemetry.peak_relative_speed = self
            .telemetry
            .peak_relative_speed
            .max(o.flight.relative_speed);
        if o.flight.sweep > 0.95 && o.flight.relative_speed > 70.0 {
            self.telemetry.fast_swept_ticks += 1;
        }
        let offset = p.ship.position - p.planet.motion.position;
        let radius = offset.length();
        let up = offset.normalized();
        let altitude = radius - p.planet.radius;
        let direction = self.telemetry.direction;
        let tangent = Vec2::new(-up.y, up.x) * direction;
        match self.telemetry.goal {
            FlightGoal::Grounded => {
                if p.landing.phase == LandingPhase::Landed {
                    self.telemetry.grounded_tick = Some(p.tick);
                    self.goal(FlightGoal::Launch, p.tick);
                } else if p.tick.saturating_sub(self.telemetry.goal_since) > 900 {
                    self.blocked("initial landing did not settle", p.tick);
                }
                FlightIntent::default()
            }
            FlightGoal::Launch => {
                if altitude > 140.0 {
                    self.goal(FlightGoal::Circuit, p.tick);
                }
                let speed = ((ORBIT_HEIGHT - altitude) * 0.7).clamp(0.0, 30.0);
                self.guide(
                    o,
                    up * speed + p.planet.velocity_at(p.ship.position),
                    Vec2::ZERO,
                    altitude > 40.0,
                    0.0,
                )
            }
            FlightGoal::Circuit => {
                let bearing = offset.y.atan2(offset.x) - p.planet.motion.angle;
                if let Some(previous) = self.last_bearing {
                    let delta = (bearing - previous + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    self.telemetry.circuit_radians += delta * direction;
                }
                if self.last_bearing.is_some()
                    || (altitude - ORBIT_HEIGHT).abs() < 35.0 && o.flight.relative_speed > 70.0
                {
                    self.last_bearing = Some(bearing);
                }
                if self.telemetry.circuit_radians >= std::f32::consts::TAU
                    && self.telemetry.fast_swept_ticks >= 300
                {
                    self.telemetry.circuit_tick = Some(p.tick);
                    self.goal(FlightGoal::Brake, p.tick);
                }
                if p.tick.saturating_sub(self.telemetry.goal_since) > 3600 {
                    self.blocked("circuit made insufficient progress", p.tick);
                }
                let radial_speed =
                    ((p.planet.radius + ORBIT_HEIGHT - radius) * 0.6).clamp(-20.0, 20.0);
                let desired_velocity =
                    p.planet.motion.velocity + tangent * ORBIT_SPEED + up * radial_speed;
                self.guide(
                    o,
                    desired_velocity,
                    -up * (ORBIT_SPEED * ORBIT_SPEED / radius),
                    true,
                    direction * ORBIT_SPEED / radius,
                )
            }
            FlightGoal::Brake => {
                if o.flight.sweep <= 0.001
                    && o.flight.relative_speed < 5.0
                    && Vec2::Y.rotate_radians(p.ship.angle).dot(up) > 0.98
                {
                    self.telemetry.braked_tick = Some(p.tick);
                    self.goal(FlightGoal::Return, p.tick);
                }
                FlightIntent {
                    controls: SurfaceSortieAction {
                        horizontal: turn(o, up, 0.0),
                        brake_held: true,
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }
            FlightGoal::Return => {
                // Start the descent already upright. Let gravity supply it;
                // hand off well above the surface with room for V1's brakes.
                if altitude < 65.0 {
                    self.telemetry.returned_tick = Some(p.tick);
                    self.goal(FlightGoal::Sortie, p.tick);
                }
                FlightIntent {
                    controls: SurfaceSortieAction {
                        horizontal: turn(o, up, p.planet.motion.spin),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }
            FlightGoal::Sortie | FlightGoal::Complete => {
                let controls = self.landing.intent(p);
                self.telemetry.landing = self.landing.telemetry().clone();
                if self.telemetry.landing.goal == PilotGoal::Blocked {
                    self.blocked(
                        self.telemetry
                            .landing
                            .blocked_reason
                            .unwrap_or("landing blocked"),
                        p.tick,
                    );
                } else if let Some(tick) = self.telemetry.landing.completed_tick {
                    self.telemetry.completed_tick = Some(tick);
                    self.goal(FlightGoal::Complete, p.tick);
                }
                FlightIntent {
                    controls,
                    ..Default::default()
                }
            }
            FlightGoal::Blocked => FlightIntent {
                controls: SurfaceSortieAction {
                    brake_held: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }
    fn guide(
        &mut self,
        o: &PilotObservationV2,
        velocity: Vec2,
        feedforward: Vec2,
        swept: bool,
        spin: f32,
    ) -> FlightIntent {
        let p = &o.pilot;
        let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
        let acceleration = (velocity - p.ship.velocity) * 1.5 + feedforward
            - p.gravity
            - o.flight.limits.braking(relative);
        let direction = acceleration.normalized();
        let aligned = Vec2::Y.rotate_radians(p.ship.angle).dot(direction) > 0.97;
        let available = o.flight.limits.thrust_acceleration
            * o.flight.limits.thrust_fraction(o.flight.forward_speed);
        let duty = if aligned && available > 0.01 {
            (acceleration.length() / available).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.thrust_fraction = (self.thrust_fraction + duty).min(2.0);
        let thrust = self.thrust_fraction >= 1.0;
        if thrust {
            self.thrust_fraction -= 1.0;
        }
        FlightIntent {
            wings: SurfaceWingAction { closed: swept },
            controls: SurfaceSortieAction {
                horizontal: turn(o, direction, spin),
                primary_held: thrust,
                brake_held: true,
                interact_held: false,
            },
        }
    }
}
fn turn(o: &PilotObservationV2, direction: Vec2, feedforward: f32) -> f32 {
    let error = shortest_heading_error(direction.rotate_radians(-o.pilot.ship.angle));
    let desired = -error * 3.0 + feedforward;
    let command = desired - (o.pilot.ship.spin - desired) * 0.2;
    let assist = if o.pilot.landing.assist_strength > 0.0 {
        o.pilot.planet.motion.spin
    } else {
        0.0
    };
    ((assist - command) / o.flight.limits.turn_speed).clamp(-1.0, 1.0)
}
