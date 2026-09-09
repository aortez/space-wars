//! One measured out-and-back ship crossing using ordinary shared pilot actions.
use crate::BrainReset;
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{
    PilotLocation, SurfaceSortieAction, TransferResult,
    jetpack::{CrossingDirection, CrossingPlan, JetpackCrossingObservation},
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CrossingGoal {
    Exit,
    Survey,
    Recharge,
    Approach,
    Lift,
    Cross,
    Descend,
    Claim,
    Board,
    Complete,
    Blocked,
}
impl CrossingGoal {
    pub fn label(self) -> &'static str {
        match self {
            Self::Exit => "landing / exiting for jetpack trial",
            Self::Survey => "checking flight over the ship",
            Self::Recharge => "recharging the jetpack",
            Self::Approach => "walking to takeoff footing",
            Self::Lift => "jetpack: climbing above the ship",
            Self::Cross => "jetpack: crossing the ship",
            Self::Descend => "jetpack: landing on the other side",
            Self::Claim => "claiming after the crossing",
            Self::Board => "boarding after both crossings",
            Self::Complete => "crossed both ways / back aboard",
            Self::Blocked => "jetpack crossing blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrossingTelemetry {
    pub policy: &'static str,
    pub goal: CrossingGoal,
    pub reason: Option<&'static str>,
    pub started_tick: Option<u64>,
    pub completed_tick: Option<u64>,
    pub crossings: u32,
    pub claimed: bool,
    pub plan: Option<CrossingPlan>,
    pub lowest_charge: f32,
}

#[derive(Debug, Clone)]
pub struct JetpackCrossingPilot {
    context: BrainReset,
    telemetry: CrossingTelemetry,
    previous_tick: Option<u64>,
    previous_action: SurfaceSortieAction,
    missing_since: Option<u64>,
    traversal: bool,
}
impl JetpackCrossingPilot {
    pub fn new(context: BrainReset) -> Self {
        Self {
            context,
            telemetry: CrossingTelemetry {
                policy: "jetpack_crossing_v1",
                goal: CrossingGoal::Exit,
                reason: None,
                started_tick: None,
                completed_tick: None,
                crossings: 0,
                claimed: false,
                plan: None,
                lowest_charge: 1.0,
            },
            previous_tick: None,
            previous_action: SurfaceSortieAction::default(),
            missing_since: None,
            traversal: false,
        }
    }
    pub fn reset(&mut self, context: BrainReset) {
        *self = Self::new(context);
    }
    /// Reuse the trial's physical maneuver as one route leg. The caller owns
    /// approach routing, the objective, interruption and the total deadline.
    pub(crate) fn traversal(context: BrainReset, plan: CrossingPlan) -> Self {
        let mut task = Self::new(context);
        task.traversal = true;
        task.telemetry.goal = CrossingGoal::Recharge;
        task.telemetry.plan = Some(plan);
        task
    }

    pub(crate) fn revalidate(&mut self, plan: &CrossingPlan) -> bool {
        if ![plan.start, plan.destination, plan.ship_position]
            .iter()
            .all(|p| p.x.is_finite() && p.y.is_finite())
            || !plan.cruise_radius.is_finite()
            || !plan.ship_angle.is_finite()
        {
            return false;
        }
        let Some(old) = &self.telemetry.plan else {
            return false;
        };
        if old.planet != plan.planet
            || old.direction != plan.direction
            || old.start.distance_to(plan.start) > 0.5
            || old.destination.distance_to(plan.destination) > 0.5
            || (old.cruise_radius - plan.cruise_radius).abs() > 0.25
            || old.ship_position.distance_to(plan.ship_position) > 0.5
            || angle_difference(old.ship_angle, plan.ship_angle).abs() > 0.1
        {
            return false;
        }
        self.telemetry.plan = Some(plan.clone());
        true
    }
    pub fn telemetry(&self) -> &CrossingTelemetry {
        &self.telemetry
    }
    pub fn direction(&self) -> CrossingDirection {
        if self.traversal {
            return self.telemetry.plan.as_ref().unwrap().direction;
        }
        if self.telemetry.crossings == 0 {
            CrossingDirection::Left
        } else {
            CrossingDirection::Right
        }
    }
    pub fn label(&self) -> &'static str {
        self.telemetry.reason.unwrap_or(self.telemetry.goal.label())
    }
    fn block(&mut self, reason: &'static str) {
        self.telemetry.goal = CrossingGoal::Blocked;
        self.telemetry.reason = Some(reason);
    }
    pub fn step(&mut self, o: &JetpackCrossingObservation) -> SurfaceSortieAction {
        let p = &o.pilot;
        if o.version != 1 || p.version != 1 || p.owner != self.context.actor {
            self.block("jetpack observation identity/version mismatch");
            return SurfaceSortieAction::default();
        }
        if self.previous_tick == Some(p.tick) {
            return self.previous_action;
        }
        if self.previous_tick.is_some_and(|tick| p.tick < tick) {
            self.block("jetpack tick moved backwards");
            return SurfaceSortieAction::default();
        }
        let action = self.choose(o);
        self.previous_tick = Some(p.tick);
        self.previous_action = action;
        action
    }
    fn choose(&mut self, o: &JetpackCrossingObservation) -> SurfaceSortieAction {
        let mut a = SurfaceSortieAction::default();
        let p = &o.pilot;
        if matches!(
            self.telemetry.goal,
            CrossingGoal::Complete | CrossingGoal::Blocked
        ) {
            return a;
        }
        let start = *self.telemetry.started_tick.get_or_insert(p.tick);
        if p.tick - start > 90 * 60 {
            self.block("jetpack trial exceeded ninety seconds");
            return a;
        }
        let Some(charge) = o.charge else {
            self.block("pilot has no jetpack");
            return a;
        };
        self.telemetry.lowest_charge = self.telemetry.lowest_charge.min(charge);
        if !p.controls_armed || !p.queries_ready {
            return a;
        }
        if p.location != PilotLocation::OnFoot {
            if self.telemetry.crossings == 2 && self.telemetry.goal == CrossingGoal::Board {
                self.telemetry.goal = CrossingGoal::Complete;
                self.telemetry.completed_tick = Some(p.tick);
            } else if self.telemetry.goal == CrossingGoal::Exit
                && p.transfer == TransferResult::Ready
            {
                a.interact_held = true;
            }
            return a;
        }
        if self.telemetry.goal == CrossingGoal::Exit {
            self.telemetry.goal = CrossingGoal::Survey;
        }
        if self.telemetry.goal == CrossingGoal::Board {
            if p.transfer == TransferResult::Ready {
                a.interact_held = true;
            } else if let (Some(actor), Some(hatch)) = (p.actor, p.hatch) {
                let right = Vec2::new(p.actor_up.y, -p.actor_up.x);
                a.horizontal = ((hatch - actor.position).dot(right) * 1.5 / 5.0).clamp(-1.0, 1.0);
            }
            return a;
        }
        if self.telemetry.goal == CrossingGoal::Claim {
            if p.planet
                .claim
                .as_ref()
                .is_some_and(|c| c.owner == Some(p.owner))
            {
                self.telemetry.claimed = true;
                self.telemetry.goal = CrossingGoal::Survey;
                self.telemetry.plan = None;
            }
            return a;
        }
        if !p.balanced && p.supported_planet.is_some() {
            a.primary_held = !self.previous_action.primary_held;
            return a;
        }
        if self
            .telemetry
            .plan
            .as_ref()
            .is_some_and(|plan| plan.planet != p.planet.index || plan.revision != p.planet.revision)
        {
            self.block("crossing ground changed during the maneuver");
            return a;
        }
        if o.surveyed {
            if let Some(plan) = &o.plan {
                if plan.direction != self.direction() {
                    return a;
                }
                if self.telemetry.plan.as_ref().is_some_and(|old| {
                    old.ship_position.distance_to(plan.ship_position) > 0.5
                        || angle_difference(old.ship_angle, plan.ship_angle).abs() > 0.1
                }) {
                    self.block("parked ship moved across the flight route");
                    return a;
                }
                if self.telemetry.plan.is_none() {
                    self.telemetry.plan = Some(plan.clone());
                    self.telemetry.goal = CrossingGoal::Recharge;
                }
                self.missing_since = None;
            } else {
                let since = *self.missing_since.get_or_insert(p.tick);
                if self.telemetry.plan.is_some() || p.tick - since > 5 * 60 {
                    self.block("no clear jetpack corridor over the ship");
                }
                return a;
            }
        }
        let Some(plan) = &self.telemetry.plan else {
            return a;
        };
        let Some(actor) = p.actor else {
            self.block("spaceling unavailable");
            return a;
        };
        let local =
            (actor.position - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let right = Vec2::new(p.actor_up.y, -p.actor_up.x);
        let relative = actor.velocity - p.planet.velocity_at(actor.position);
        if self.telemetry.goal == CrossingGoal::Recharge {
            if charge >= 0.98 {
                self.telemetry.goal = CrossingGoal::Approach;
            }
            return a;
        }
        let local_target = match self.telemetry.goal {
            CrossingGoal::Approach | CrossingGoal::Lift => plan.start,
            _ => plan.destination,
        };
        let target = p.planet.motion.position + local_target.rotate_radians(p.planet.motion.angle);
        let error = (target - actor.position).dot(right);
        if self.telemetry.goal == CrossingGoal::Approach {
            a.horizontal = (error * 1.6 / 5.0).clamp(-1.0, 1.0);
            if error.abs() < 0.6
                && p.supported_planet == Some(plan.planet)
                && p.relative_speed.abs() < 1.0
            {
                self.telemetry.goal = CrossingGoal::Lift;
                a.horizontal = 0.0;
            }
            return a;
        }
        let radial_speed = relative.dot(p.actor_up);
        if self.telemetry.goal == CrossingGoal::Lift && local.length() > plan.cruise_radius - 0.4 {
            self.telemetry.goal = CrossingGoal::Cross;
        } else if self.telemetry.goal == CrossingGoal::Cross
            && error.abs() < 0.5
            && relative.dot(right).abs() < 1.0
        {
            self.telemetry.goal = CrossingGoal::Descend;
        } else if self.telemetry.goal == CrossingGoal::Descend
            && p.supported_planet == Some(plan.planet)
            && p.balanced
            && relative.length() < 1.0
            && error.abs() < 1.0
        {
            self.telemetry.crossings += 1;
            if self.traversal {
                self.telemetry.goal = CrossingGoal::Complete;
                self.telemetry.completed_tick = Some(p.tick);
                return a;
            }
            self.telemetry.goal = if self.telemetry.crossings == 1 {
                CrossingGoal::Claim
            } else {
                CrossingGoal::Board
            };
            return a;
        }
        let target_radius = if self.telemetry.goal == CrossingGoal::Descend {
            // Aim slightly into the standing envelope so contact, rather than
            // a hovering equilibrium just above it, ends the descent.
            plan.destination.length() + 0.55
        } else {
            plan.cruise_radius
        };
        let desired_rise = ((target_radius - local.length()) * 1.8).clamp(-6.0, 7.0);
        a.primary_held = radial_speed < desired_rise;
        if self.telemetry.goal == CrossingGoal::Descend
            && local.length() < plan.destination.length() + 1.5
            && error.abs() < 1.0
        {
            // Once aligned just above the measured footing, commit to contact.
            // Trying to hover at capsule height wastes the landing reserve on
            // stepped/sloping terrain. Charged lateral steering still brakes.
            a.primary_held = false;
        }
        a.horizontal = (error * 1.8).clamp(-8.0, 8.0) / o.air_speed;
        if charge <= 0.0 && p.supported_planet.is_none() {
            self.block("jetpack charge exhausted before landing");
        }
        a
    }
}

fn angle_difference(a: f32, b: f32) -> f32 {
    (a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}
