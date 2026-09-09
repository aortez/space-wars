//! Shared, bounded spaceling traversal over measured material ground.
use crate::BrainReset;
use crate::jetpack_crossing::{CrossingTelemetry, JetpackCrossingPilot};
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::jetpack::CrossingPlan;
use scenario_spacewars::surface_sortie::{
    PilotLocation, SurfaceSortieAction,
    ground_navigation::{
        GROUND_NEIGHBOR_SPAN, GROUND_SAMPLES, GroundEdgeKind, GroundMap, GroundRouteDiagnostics,
        HATCH_APPROACH_RANGE,
    },
    recovery_sensors::RecoveryTaskObservationV1,
};
use serde::Serialize;

mod jetpack;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundDestination {
    Flag,
    Hatch,
    Rebuild { planet: usize, position: Vec2 },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundGoal {
    Survey,
    Walk,
    Jump,
    GetUp,
    Recharge,
    JetpackLift,
    JetpackCross,
    JetpackLand,
    Settle,
    WaitForShip,
    Arrived,
    Blocked,
}
impl GroundGoal {
    pub fn label(self) -> &'static str {
        match self {
            Self::Survey => "checking a ground route",
            Self::Walk => "walking along the ground route",
            Self::Jump => "jumping a ground obstacle",
            Self::GetUp => "getting up on the route",
            Self::Recharge => "recharging for the ground route",
            Self::JetpackLift => "jetpack: climbing over an obstacle",
            Self::JetpackCross => "jetpack: crossing toward the objective",
            Self::JetpackLand => "jetpack: landing to resume the route",
            Self::Settle => "waiting for stable footing",
            Self::WaitForShip => "waiting for the assigned ship to land",
            Self::Arrived => "at the ground destination",
            Self::Blocked => "ground route blocked",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroundTelemetry {
    pub policy: &'static str,
    pub destination: GroundDestination,
    pub goal: GroundGoal,
    pub reason: Option<&'static str>,
    pub started_tick: Option<u64>,
    pub last_progress_tick: u64,
    pub revision: Option<u64>,
    pub target: Option<Vec2>,
    pub path: Vec<u16>,
    pub waypoint: usize,
    pub replans: u32,
    pub invalidations: u32,
    pub jumps: u32,
    pub route: Option<GroundRouteDiagnostics>,
    pub crossing: Option<CrossingTelemetry>,
    pub jetpack_crossings: u32,
    pub flight_interruptions: u32,
    pub partial_routes: u32,
    pub displacements: u32,
}
#[derive(Debug, Clone)]
pub struct GroundNavigationTask {
    context: BrainReset,
    telemetry: GroundTelemetry,
    map: Option<GroundMap>,
    best_distance: f32,
    missing_since: Option<u64>,
    jump_tick: Option<u64>,
    was_jumping: bool,
    previous_tick: Option<u64>,
    previous_action: SurfaceSortieAction,
    last_plan_tick: Option<u64>,
    crossing_plan: Option<CrossingPlan>,
    crossing_task: Option<JetpackCrossingPilot>,
    settling_after_interrupt: bool,
    settling_after_displacement: bool,
}
impl GroundNavigationTask {
    pub fn new(context: BrainReset, destination: GroundDestination) -> Self {
        Self {
            context,
            telemetry: GroundTelemetry {
                policy: "ground_navigation_v5",
                destination,
                goal: GroundGoal::Survey,
                reason: None,
                started_tick: None,
                last_progress_tick: 0,
                revision: None,
                target: None,
                path: Vec::new(),
                waypoint: 0,
                replans: 0,
                invalidations: 0,
                jumps: 0,
                route: None,
                crossing: None,
                jetpack_crossings: 0,
                flight_interruptions: 0,
                partial_routes: 0,
                displacements: 0,
            },
            map: None,
            best_distance: f32::INFINITY,
            missing_since: None,
            jump_tick: None,
            was_jumping: false,
            previous_tick: None,
            previous_action: SurfaceSortieAction::default(),
            last_plan_tick: None,
            crossing_plan: None,
            crossing_task: None,
            settling_after_interrupt: false,
            settling_after_displacement: false,
        }
    }
    pub fn telemetry(&self) -> &GroundTelemetry {
        &self.telemetry
    }
    pub fn reset(&mut self, context: BrainReset) {
        *self = Self::new(context, self.telemetry.destination);
    }
    pub fn is_crossing(&self) -> bool {
        self.crossing_task.is_some() || self.settling_after_interrupt
    }
    /// Finish an active landing before following a changed objective.
    pub fn retarget(&mut self, destination: GroundDestination) {
        self.telemetry.destination = destination;
        self.telemetry.target = None;
        self.clear_route();
    }
    fn block(&mut self, reason: &'static str) {
        self.telemetry.goal = GroundGoal::Blocked;
        self.telemetry.reason = Some(reason);
    }
    fn clear_route(&mut self) {
        self.telemetry.path.clear();
        self.telemetry.waypoint = 0;
        self.best_distance = f32::INFINITY;
        self.last_plan_tick = None;
        self.crossing_plan = None;
    }
    pub fn step(&mut self, o: &RecoveryTaskObservationV1) -> SurfaceSortieAction {
        let p = &o.flight.pilot;
        if o.version != 1
            || o.flight.version != 2
            || o.flight.flight.version != 1
            || p.version != 1
            || p.owner != self.context.actor
        {
            self.block("ground observation identity/version mismatch");
            return SurfaceSortieAction::default();
        }
        if self.previous_tick == Some(p.tick) {
            return self.previous_action;
        }
        if self.previous_tick.is_some_and(|tick| p.tick < tick) {
            self.block("ground observation tick moved backwards");
            return SurfaceSortieAction::default();
        }
        let action = self.choose(o);
        self.was_jumping = action.primary_held;
        self.previous_tick = Some(p.tick);
        self.previous_action = action;
        action
    }
    fn choose(&mut self, o: &RecoveryTaskObservationV1) -> SurfaceSortieAction {
        let p = &o.flight.pilot;
        let mut action = SurfaceSortieAction::default();
        if self.telemetry.goal == GroundGoal::Blocked || !p.controls_armed {
            return action;
        }
        let start = *self.telemetry.started_tick.get_or_insert(p.tick);
        if p.location != PilotLocation::OnFoot {
            return action;
        }
        let Some(actor) = p.actor else {
            self.block("spaceling unavailable");
            return action;
        };
        if !p.queries_ready {
            self.telemetry.goal = GroundGoal::Survey;
            if p.tick.saturating_sub(start) > 90 * 60 {
                self.block("ground traversal exceeded ninety seconds");
            }
            return action;
        }
        if self.is_crossing() {
            if p.tick.saturating_sub(start) > 90 * 60 {
                self.block("ground traversal exceeded ninety seconds");
                return action;
            }
            if self.crossing_task.is_some() {
                return self.follow_crossing(o);
            }
            if p.supported_planet != Some(p.planet.index) {
                self.telemetry.goal = GroundGoal::Settle;
                return action;
            }
            self.settling_after_interrupt = false;
            self.telemetry.last_progress_tick = p.tick;
        }
        let target = match self.telemetry.destination {
            GroundDestination::Flag => p
                .planet
                .claim
                .as_ref()
                .filter(|c| c.owner != Some(p.owner))
                .and_then(|c| c.flag)
                .map(|flag| flag.position),
            GroundDestination::Hatch => p.hatch,
            GroundDestination::Rebuild { planet, position } => {
                if planet != p.planet.index {
                    self.block("rebuild footing is on another planet");
                    return action;
                }
                Some(p.planet.motion.position + position.rotate_radians(p.planet.motion.angle))
            }
        };
        let local =
            |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let target_local = target.map(local);
        let foot = local(actor.position - p.actor_up * 0.9);
        if let Some(map) = &self.map
            && !self.telemetry.path.is_empty()
            && self
                .telemetry
                .path
                .iter()
                .skip(self.telemetry.waypoint.saturating_sub(1))
                .filter_map(|id| map.nodes.iter().find(|n| n.id == *id))
                .map(|n| foot.distance_to(n.position))
                .min_by(f32::total_cmp)
                .is_some_and(|distance| distance > 6.0)
        {
            // A collision can throw the actor far beyond an ordinary jump.
            // Let actual contacts settle it, then plan from its new footing.
            self.telemetry.displacements += 1;
            self.telemetry.invalidations += 1;
            self.clear_route();
            self.map = None;
            self.settling_after_displacement = true;
        }
        if self.settling_after_displacement {
            if p.tick.saturating_sub(start) > 90 * 60 {
                self.block("ground traversal exceeded ninety seconds");
                return action;
            }
            if p.supported_planet != Some(p.planet.index) {
                self.telemetry.goal = GroundGoal::Settle;
                return action;
            }
            self.settling_after_displacement = false;
            self.telemetry.last_progress_tick = p.tick;
        }
        if self
            .telemetry
            .target
            .zip(target_local)
            .is_some_and(|(a, b)| a.distance_to(b) > 0.5)
            || self.telemetry.target.is_some() != target_local.is_some()
        {
            self.clear_route();
            self.missing_since = None;
            self.telemetry.last_progress_tick = p.tick;
            self.telemetry.target = target_local;
        }
        if !p.balanced && p.supported_planet == Some(p.planet.index) {
            if p.tick.saturating_sub(start) > 90 * 60 {
                self.block("ground traversal exceeded ninety seconds");
                return action;
            }
            self.telemetry.goal = GroundGoal::GetUp;
            action.primary_held =
                !self.was_jumping && self.jump_tick.is_none_or(|t| p.tick - t > 60);
            if action.primary_held {
                self.jump_tick = Some(p.tick);
            }
            return action;
        }
        if self.telemetry.goal == GroundGoal::GetUp && p.balanced {
            self.telemetry.last_progress_tick = p.tick;
            self.best_distance = f32::INFINITY;
        }
        let Some(target) = target else {
            self.telemetry.goal = if self.telemetry.destination == GroundDestination::Flag {
                GroundGoal::Arrived
            } else if self.telemetry.destination == GroundDestination::Hatch {
                GroundGoal::WaitForShip
            } else {
                GroundGoal::Survey
            };
            if self.telemetry.destination == GroundDestination::Hatch && p.tick - start > 15 * 60 {
                self.block("assigned ship has no grounded hatch");
            }
            return action;
        };
        let range = match self.telemetry.destination {
            GroundDestination::Flag => p
                .planet
                .claim
                .as_ref()
                .map_or(2.8, |claim| claim.flag_interaction_range - 0.2),
            GroundDestination::Hatch => HATCH_APPROACH_RANGE,
            GroundDestination::Rebuild { .. } => 1.4,
        };
        if actor.position.distance_to(target) < range {
            self.telemetry.goal = if p.supported_planet == Some(p.planet.index) {
                self.telemetry.last_progress_tick = p.tick;
                GroundGoal::Arrived
            } else {
                // Brief contact loss at the destination needs a quiet landing.
                // Resuming an old jump edge here repeatedly interrupts claims.
                GroundGoal::Settle
            };
            if p.tick.saturating_sub(start) > 90 * 60 && self.telemetry.goal != GroundGoal::Arrived
            {
                self.block("ground traversal exceeded ninety seconds");
            }
            return action;
        }
        if p.tick.saturating_sub(start) > 90 * 60 {
            self.block("ground traversal exceeded ninety seconds");
            return action;
        }
        if self
            .map
            .as_ref()
            .is_some_and(|m| m.planet != p.planet.index || m.revision != p.planet.revision)
        {
            self.telemetry.invalidations += 1;
            self.clear_route();
            self.map = None;
        }
        if let Some(map) = &o.ground {
            if map.version != 1
                || map.actor != p.owner
                || map.planet != p.planet.index
                || map.revision != p.planet.revision
                || map.nodes.len() > GROUND_SAMPLES
                || map.edges.len() > GROUND_SAMPLES * GROUND_NEIGHBOR_SPAN * 2
            {
                self.block("ground map identity/version or bounds mismatch");
                return action;
            }
            let mut ids = [false; GROUND_SAMPLES];
            let invalid_nodes = map.nodes.iter().any(|n| {
                let id = usize::from(n.id);
                if id >= GROUND_SAMPLES
                    || ids[id]
                    || !n.position.x.is_finite()
                    || !n.position.y.is_finite()
                    || !n.normal.x.is_finite()
                    || !n.normal.y.is_finite()
                {
                    return true;
                }
                ids[id] = true;
                false
            });
            if map.tick != p.tick
                || invalid_nodes
                || map.edges.iter().any(|e| {
                    usize::from(e.from) >= GROUND_SAMPLES
                        || usize::from(e.to) >= GROUND_SAMPLES
                        || !ids[usize::from(e.from)]
                        || !ids[usize::from(e.to)]
                        || !e.length.is_finite()
                        || e.length <= 0.0
                        || e.kind == GroundEdgeKind::Jetpack
                })
            {
                self.block("invalid ground map geometry");
                return action;
            }
            if self
                .telemetry
                .path
                .windows(2)
                .skip(self.telemetry.waypoint.saturating_sub(1))
                .any(|pair| {
                    !map.edges
                        .iter()
                        .any(|e| e.from == pair[0] && e.to == pair[1])
                })
            {
                self.telemetry.invalidations += 1;
                self.clear_route();
            }
            self.map = Some(map.clone());
            self.telemetry.revision = Some(map.revision);
        }
        let Some(map) = &self.map else {
            self.telemetry.goal = GroundGoal::Survey;
            return action;
        };
        if self.telemetry.path.is_empty() {
            self.telemetry.goal = GroundGoal::Survey;
            if o.jetpack.as_ref().is_some_and(|pack| !pack.surveyed) {
                // Compare walking and powered routes from the same completed
                // survey, including immediately after a landing or interruption.
                return action;
            }
            if self.last_plan_tick == Some(map.tick) {
                return action;
            }
            self.last_plan_tick = Some(map.tick);
            self.telemetry.replans += 1;
            let (route, crossing) =
                self.route_with_jetpack(map, foot, target_local.unwrap(), range, o);
            self.crossing_plan = crossing;
            self.telemetry.route = Some(route.diagnostics.clone());
            if !route.path.is_empty() {
                self.telemetry.partial_routes += u32::from(route.diagnostics.partial);
                self.telemetry.path = route.path;
                self.telemetry.waypoint = 0;
                self.telemetry.last_progress_tick = p.tick;
                self.missing_since = None;
            } else {
                let since = *self.missing_since.get_or_insert(p.tick);
                self.telemetry.goal = GroundGoal::Survey;
                if p.tick - since > 5 * 60 {
                    self.block(if o.jetpack.is_some() {
                        "no measured walk/jump/jetpack route to destination"
                    } else {
                        "no measured walk/jump route to destination"
                    });
                }
                return action;
            }
        }
        let index = self.telemetry.waypoint;
        if let Some(plan) = &self.crossing_plan
            && index + 1 == self.telemetry.path.len()
            && foot.distance_to(plan.start) < 1.5
            && p.supported_planet == Some(p.planet.index)
            && p.balanced
        {
            self.crossing_task = Some(JetpackCrossingPilot::traversal(self.context, plan.clone()));
            return self.follow_crossing(o);
        }
        let node_id = self.telemetry.path[index];
        let Some(node) = map.nodes.iter().find(|n| n.id == node_id) else {
            self.clear_route();
            return action;
        };
        let next = p.planet.motion.position + node.position.rotate_radians(p.planet.motion.angle);
        let right = Vec2::new(p.actor_up.y, -p.actor_up.x);
        let error = (next - actor.position).dot(right);
        let distance = foot.distance_to(node.position);
        if distance < 0.85
            && p.supported_planet == Some(p.planet.index)
            && index + 1 == self.telemetry.path.len()
            && self.crossing_plan.is_none()
            && self.telemetry.route.as_ref().is_some_and(|r| r.partial)
        {
            self.clear_route();
            self.telemetry.goal = GroundGoal::Survey;
            return action;
        }
        if distance < 0.85
            && p.supported_planet == Some(p.planet.index)
            && index + 1 < self.telemetry.path.len()
        {
            self.telemetry.waypoint += 1;
            self.best_distance = f32::INFINITY;
            self.telemetry.last_progress_tick = p.tick;
            return action;
        }
        if distance < self.best_distance - 0.2 {
            self.best_distance = distance;
            self.telemetry.last_progress_tick = p.tick;
        }
        if p.tick.saturating_sub(self.telemetry.last_progress_tick) > 8 * 60 {
            self.block("spaceling stopped making progress on ground route");
            return action;
        }
        let jump_edge = index > 0
            && map.edges.iter().any(|e| {
                e.from == self.telemetry.path[index - 1]
                    && e.to == node_id
                    && e.kind == GroundEdgeKind::Jump
            });
        let stuck = p.tick.saturating_sub(self.telemetry.last_progress_tick) > 45;
        action.horizontal = (error * 1.8 / 5.0).clamp(-1.0, 1.0);
        self.telemetry.goal = if p.supported_planet.is_some() {
            GroundGoal::Walk
        } else {
            GroundGoal::Jump
        };
        if p.supported_planet == Some(p.planet.index)
            && (jump_edge && error.abs() > 0.9 || stuck)
            && error.abs() > 0.25
            && !self.was_jumping
            && self.jump_tick.is_none_or(|t| p.tick - t > 36)
        {
            action.primary_held = true;
            self.jump_tick = Some(p.tick);
            self.telemetry.jumps += 1;
            self.telemetry.goal = GroundGoal::Jump;
        }
        action
    }
}
