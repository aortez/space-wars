//! Host-side Spacewars controllers built on explicit scenario observations.
//!
//! The scenario remains authoritative for physics and gameplay. A brain sees a
//! versioned [`ShipObservationV1`], returns a canonical [`ShipIntent`], and has
//! no reference through which it could mutate the simulation. Keeping this in
//! a small library also lets the desktop client and a future headless agent
//! runner use exactly the same guidance code.

#![forbid(unsafe_code)]

use core::f32::consts::PI;

use engine_core::Vec2;
use scenario_spacewars::{
    DebrisId, HazardObservationV1, PlanetId, PlanetObservationV1, PlayerId, ShipForm, ShipIntent,
    ShipObservationV1,
};
use serde::Serialize;

const TARGET_EPSILON: f32 = 1.0e-5;

/// Stable policy identity stored in episode artifacts.
///
/// Bump this when rule-brain decision semantics change enough that evaluation
/// results should no longer be compared as the same policy version.
pub const RULE_SHIP_BRAIN_POLICY_ID: &str = "rule_ship_v5";

/// Episode context supplied whenever a host installs or restarts a brain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrainReset {
    pub actor: PlayerId,
    pub episode_seed: u64,
}

/// Stable controller boundary shared by interactive and headless hosts.
pub trait ShipBrain {
    fn reset(&mut self, reset: BrainReset);

    fn intent(&mut self, observation: &ShipObservationV1) -> ShipIntent;

    fn telemetry(&self) -> BrainTelemetry;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrainGoal {
    #[default]
    Idle,
    Attack,
    AvoidBody,
    AvoidHazard,
    Survive,
    Capture,
    Repair,
    Defend,
    Rebuild,
}

/// Slow, persistent objective selected above the per-tick guidance layer.
///
/// Unlike [`BrainGoal`], this never reports temporary body or debris
/// avoidance. Those maneuvers can take over the controls without erasing the
/// objective the ship should resume afterward.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicGoal {
    #[default]
    Idle,
    Survive,
    Attack,
    Capture,
    Repair,
    Defend,
    Rebuild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategySelectionReason {
    Initial,
    Mandatory,
    Urgent,
    Invalidated,
    HigherUtility,
    DockingContact,
}

/// Best utility currently available for each strategic goal class.
///
/// `None` means the goal is not legal or has no target in this observation.
/// Scores are intentionally inspectable tuning values rather than gameplay
/// state, and remain finite so telemetry is always JSON-serializable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct StrategyScores {
    pub survive: Option<f32>,
    pub attack: Option<f32>,
    pub capture: Option<f32>,
    pub repair: Option<f32>,
    pub defend: Option<f32>,
    pub rebuild: Option<f32>,
}

impl StrategyScores {
    fn record(&mut self, goal: StrategicGoal, score: f32) {
        let slot = match goal {
            StrategicGoal::Survive => &mut self.survive,
            StrategicGoal::Attack => &mut self.attack,
            StrategicGoal::Capture => &mut self.capture,
            StrategicGoal::Repair => &mut self.repair,
            StrategicGoal::Defend => &mut self.defend,
            StrategicGoal::Rebuild => &mut self.rebuild,
            StrategicGoal::Idle => return,
        };
        if slot.is_none_or(|current| score > current) {
            *slot = Some(score);
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct StrategyTelemetry {
    pub goal: StrategicGoal,
    pub target: Option<PlayerId>,
    pub target_planet: Option<PlanetId>,
    pub selected_score: Option<f32>,
    pub scores: StrategyScores,
    pub selected_at_tick: u64,
    pub age_ticks: u64,
    pub selection_reason: Option<StrategySelectionReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortNavigationPhase {
    Rendezvous,
    Approach,
    Ingress,
    Docked,
    Depart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AvoidanceBody {
    Sun,
    Planet(PlanetId),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BrainTelemetry {
    pub actor: Option<PlayerId>,
    pub strategy: StrategyTelemetry,
    pub goal: BrainGoal,
    pub target: Option<PlayerId>,
    pub target_planet: Option<PlanetId>,
    pub port_phase: Option<PortNavigationPhase>,
    pub hazard: Option<DebrisId>,
    pub avoided_body: Option<AvoidanceBody>,
    pub avoidance_surface_clearance: Option<f32>,
    pub avoidance_outward_speed: Option<f32>,
    pub avoidance_predictive: bool,
    pub avoidance_seconds_until_closest: Option<f32>,
    pub avoidance_predicted_surface_clearance: Option<f32>,
    pub avoidance_age_ticks: u64,
    pub avoidance_stalled_ticks: u64,
    pub avoidance_escape_assist: bool,
    pub avoidance_emergency_escape_assist: bool,
    pub target_distance: f32,
    pub heading_error: f32,
    pub desired_speed: f32,
    pub relative_speed: f32,
}

impl Default for BrainTelemetry {
    fn default() -> Self {
        Self {
            actor: None,
            strategy: StrategyTelemetry::default(),
            goal: BrainGoal::Idle,
            target: None,
            target_planet: None,
            port_phase: None,
            hazard: None,
            avoided_body: None,
            avoidance_surface_clearance: None,
            avoidance_outward_speed: None,
            avoidance_predictive: false,
            avoidance_seconds_until_closest: None,
            avoidance_predicted_surface_clearance: None,
            avoidance_age_ticks: 0,
            avoidance_stalled_ticks: 0,
            avoidance_escape_assist: false,
            avoidance_emergency_escape_assist: false,
            target_distance: 0.0,
            heading_error: 0.0,
            desired_speed: 0.0,
            relative_speed: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadingGuidance {
    pub error_radians: f32,
    pub turn: f32,
    pub aligned: bool,
}

/// Return the signed shortest heading error to a local-frame target.
///
/// Positive angles are to the ship's right, matching positive Spacewars turn
/// input. The result is always in `-PI..=PI`.
pub fn shortest_heading_error(local_target: Vec2) -> f32 {
    if local_target.length_squared() <= TARGET_EPSILON {
        0.0
    } else {
        local_target.x.atan2(local_target.y).clamp(-PI, PI)
    }
}

/// Pure proportional/damping steering usable by rule-based and trained agents.
pub fn guide_heading(local_target: Vec2, angular_velocity: f32) -> HeadingGuidance {
    let error_radians = shortest_heading_error(local_target);
    let desired_omega = (error_radians * 2.0).clamp(-1.0, 1.0);
    let turn = ((desired_omega - angular_velocity) * 2.0).clamp(-1.0, 1.0);
    HeadingGuidance {
        error_radians,
        turn,
        aligned: error_radians.abs() <= 0.08,
    }
}

/// Preserve a pod's angular authority while it is turning toward a target.
///
/// Spacewars pods cruise automatically when the brake is released, while the
/// brake damps both linear and angular velocity. Combining a meaningful
/// heading correction with braking therefore makes the pod cancel its own
/// escape turn. Ships have independent thrust and steering, so their requested
/// brake remains unchanged.
fn steering_safe_brake(form: ShipForm, heading: HeadingGuidance, requested_brake: f32) -> f32 {
    if form == ShipForm::EscapePod && !heading.aligned {
        0.0
    } else {
        requested_brake
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrivalGuidanceConfig {
    /// Maximum commanded speed relative to the moving target.
    pub max_closing_speed: f32,
    /// Time constant used to reduce desired speed as the target gets close.
    pub arrival_time_seconds: f32,
    pub position_tolerance: f32,
    pub velocity_tolerance: f32,
    /// Velocity error large enough to request full forward thrust or braking.
    pub full_control_velocity_error: f32,
    /// Maximum heading error at which forward thrust is useful.
    pub thrust_alignment_radians: f32,
}

impl Default for ArrivalGuidanceConfig {
    fn default() -> Self {
        Self {
            max_closing_speed: 120.0,
            arrival_time_seconds: 1.5,
            position_tolerance: 12.0,
            velocity_tolerance: 8.0,
            full_control_velocity_error: 30.0,
            thrust_alignment_radians: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrivalGuidance {
    pub heading: HeadingGuidance,
    pub target_distance: f32,
    pub desired_closing_speed: f32,
    pub closing_speed: f32,
    pub relative_speed: f32,
    pub velocity_error: Vec2,
    pub thrust: f32,
    pub brake: f32,
    pub arrived: bool,
}

/// Steer toward a position while matching the velocity of its moving frame.
///
/// `local_target_velocity` is target velocity minus craft velocity, matching
/// Spacewars observation semantics. The controller first chooses a desired
/// closing velocity, then points the ship at the difference between that and
/// its current target-relative velocity. Braking is selected only when the
/// game's omnidirectional brake would reduce that same velocity error.
pub fn guide_arrival(
    local_target_position: Vec2,
    local_target_velocity: Vec2,
    own_local_velocity: Vec2,
    angular_velocity: f32,
    config: ArrivalGuidanceConfig,
) -> ArrivalGuidance {
    let target_distance = local_target_position.length();
    let direction = if target_distance > TARGET_EPSILON {
        local_target_position / target_distance
    } else {
        Vec2::Y
    };
    let remaining_distance = (target_distance - config.position_tolerance.max(0.0)).max(0.0);
    let desired_closing_speed = (remaining_distance
        / config.arrival_time_seconds.max(TARGET_EPSILON))
    .min(config.max_closing_speed.max(0.0));
    let closing_speed = -local_target_velocity.dot(direction);
    let relative_speed = local_target_velocity.length();
    let desired_relative_velocity = direction * desired_closing_speed;
    let current_relative_velocity = -local_target_velocity;
    let velocity_error = desired_relative_velocity - current_relative_velocity;
    let error_magnitude = velocity_error.length();
    let heading_target = if error_magnitude > TARGET_EPSILON {
        velocity_error
    } else {
        Vec2::Y
    };
    let heading = guide_heading(heading_target, angular_velocity);
    let arrived = target_distance <= config.position_tolerance.max(0.0)
        && relative_speed <= config.velocity_tolerance.max(0.0);

    let full_control_error = config
        .full_control_velocity_error
        .max(config.velocity_tolerance)
        .max(TARGET_EPSILON);
    let control_amount = (error_magnitude / full_control_error).clamp(0.0, 1.0);
    let brake_direction = -own_local_velocity;
    let brake_helps = brake_direction.length_squared() > TARGET_EPSILON
        && velocity_error.length_squared() > TARGET_EPSILON
        && brake_direction
            .normalized()
            .dot(velocity_error.normalized())
            > 0.35;
    let moving_too_fast = closing_speed > desired_closing_speed + config.velocity_tolerance
        || current_relative_velocity.length()
            > desired_relative_velocity.length() + config.velocity_tolerance;
    let brake = if brake_helps && moving_too_fast {
        control_amount
    } else {
        0.0
    };
    let thrust = if !arrived
        && brake == 0.0
        && heading.error_radians.abs() <= config.thrust_alignment_radians.max(0.0)
    {
        control_amount
    } else {
        0.0
    };

    ArrivalGuidance {
        heading,
        target_distance,
        desired_closing_speed,
        closing_speed,
        relative_speed,
        velocity_error,
        thrust,
        brake,
        arrived,
    }
}

/// Lead a moving contact using a bounded constant-speed intercept estimate.
pub fn intercept_position(
    local_position: Vec2,
    local_velocity: Vec2,
    projectile_speed: f32,
    max_lead_seconds: f32,
) -> Vec2 {
    if projectile_speed <= 0.0 || !projectile_speed.is_finite() {
        return local_position;
    }
    let lead_seconds =
        (local_position.length() / projectile_speed).clamp(0.0, max_lead_seconds.max(0.0));
    local_position + local_velocity * lead_seconds
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionThreat {
    pub hazard: DebrisId,
    pub seconds_until_closest: f32,
    pub closest_separation: f32,
    pub avoidance_direction: Vec2,
}

/// Find the earliest debris contact predicted inside a short tactical horizon.
pub fn earliest_collision_threat(
    hazards: &[HazardObservationV1],
    own_radius: f32,
    horizon_seconds: f32,
    clearance: f32,
) -> Option<CollisionThreat> {
    let horizon_seconds = horizon_seconds.max(0.0);
    hazards
        .iter()
        .filter_map(|hazard| {
            let relative_speed_squared = hazard.local_velocity.length_squared();
            let seconds_until_closest = if relative_speed_squared <= TARGET_EPSILON {
                0.0
            } else {
                (-hazard.local_position.dot(hazard.local_velocity) / relative_speed_squared)
                    .clamp(0.0, horizon_seconds)
            };
            let closest = hazard.local_position + hazard.local_velocity * seconds_until_closest;
            let closest_separation = closest.length();
            let required_separation = own_radius + hazard.radius + clearance.max(0.0);
            if closest_separation > required_separation {
                return None;
            }

            let avoidance_direction = if closest.length_squared() > TARGET_EPSILON {
                -closest
            } else if hazard.local_position.length_squared() > TARGET_EPSILON {
                -hazard.local_position
            } else {
                Vec2::X
            };
            Some(CollisionThreat {
                hazard: hazard.id,
                seconds_until_closest,
                closest_separation,
                avoidance_direction,
            })
        })
        .min_by(|a, b| {
            a.seconds_until_closest
                .total_cmp(&b.seconds_until_closest)
                .then_with(|| a.closest_separation.total_cmp(&b.closest_separation))
                .then_with(|| a.hazard.cmp(&b.hazard))
        })
}

/// Choose the tangent side requiring less change from the current direction
/// of body-relative travel. Exact ties consistently choose positive rotation.
fn preferred_body_tangent_sign(local_position: Vec2, local_velocity: Vec2) -> f32 {
    let center_direction = if local_position.length_squared() > TARGET_EPSILON {
        local_position.normalized()
    } else {
        Vec2::Y
    };
    let current_travel = if local_velocity.length_squared() > TARGET_EPSILON {
        -local_velocity.normalized()
    } else {
        Vec2::Y
    };
    let positive_score = center_direction
        .rotate_radians(PI * 0.25)
        .dot(current_travel);
    let negative_score = center_direction
        .rotate_radians(-PI * 0.25)
        .dot(current_travel);
    if positive_score >= negative_score {
        1.0
    } else {
        -1.0
    }
}

fn body_tangent_direction(
    local_position: Vec2,
    required_separation: f32,
    margin_radians: f32,
    tangent_sign: f32,
) -> Vec2 {
    let distance = local_position.length();
    if distance <= required_separation.max(0.0) || distance <= TARGET_EPSILON {
        return -local_position;
    }
    let center_direction = local_position / distance;
    let tangent_offset = ((required_separation.max(0.0) / distance).clamp(0.0, 1.0)).asin()
        + margin_radians.max(0.0);
    center_direction.rotate_radians(tangent_offset * tangent_sign.signum()) * distance
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuleStrategyConfig {
    /// Strategic utility is intentionally slower than 60 Hz guidance.
    pub evaluation_interval_ticks: u64,
    /// A valid objective cannot be displaced before this many ticks elapse.
    pub minimum_commitment_ticks: u64,
    /// A challenger must exceed the current utility by this amount.
    pub switch_margin: f32,
    pub repair_enter_life_fraction: f32,
    pub repair_exit_life_fraction: f32,
    pub attack_weight: f32,
    pub capture_weight: f32,
    pub repair_weight: f32,
    pub defend_weight: f32,
}

impl Default for RuleStrategyConfig {
    fn default() -> Self {
        Self {
            evaluation_interval_ticks: 60,
            minimum_commitment_ticks: 180,
            switch_margin: 0.08,
            repair_enter_life_fraction: 0.5,
            repair_exit_life_fraction: 0.9,
            attack_weight: 1.0,
            capture_weight: 1.0,
            repair_weight: 1.0,
            defend_weight: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuleShipBrainConfig {
    pub strategy: RuleStrategyConfig,
    pub body_clearance: f32,
    pub body_avoidance_horizon_seconds: f32,
    pub body_avoidance_tangent_margin_radians: f32,
    pub departure_reentry_guard_ticks: u64,
    pub body_avoidance_memory_ticks: u64,
    pub body_avoidance_stall_ticks: u64,
    pub body_avoidance_stall_clearance: f32,
    pub body_avoidance_emergency_stall_ticks: u64,
    pub body_avoidance_emergency_life_fraction: f32,
    pub body_avoidance_progress_distance: f32,
    pub body_avoidance_turn_brake: f32,
    pub hazard_clearance: f32,
    pub hazard_horizon_seconds: f32,
    pub arrival_distance: f32,
    pub fast_pursuit_distance: f32,
    pub laser_range: f32,
    pub cannon_min_range: f32,
    pub cannon_max_range: f32,
    pub spaceport_staging_margin: f32,
    pub spaceport_staging_tolerance: f32,
    pub spaceport_staging_velocity_tolerance: f32,
    pub spaceport_corridor_half_width: f32,
    pub spaceport_contact_loss_grace_ticks: u64,
    pub spaceport_cruise_distance: f32,
    pub spaceport_approach_speed: f32,
    pub spaceport_ingress_speed: f32,
    pub spaceport_departure_speed: f32,
    pub spaceport_departure_alignment_radians: f32,
}

impl Default for RuleShipBrainConfig {
    fn default() -> Self {
        Self {
            strategy: RuleStrategyConfig::default(),
            body_clearance: 90.0,
            body_avoidance_horizon_seconds: 3.0,
            body_avoidance_tangent_margin_radians: 0.12,
            departure_reentry_guard_ticks: 30,
            body_avoidance_memory_ticks: 60,
            body_avoidance_stall_ticks: 300,
            body_avoidance_stall_clearance: 35.0,
            body_avoidance_emergency_stall_ticks: 60,
            body_avoidance_emergency_life_fraction: 0.5,
            body_avoidance_progress_distance: 5.0,
            body_avoidance_turn_brake: 0.35,
            hazard_clearance: 18.0,
            hazard_horizon_seconds: 1.25,
            arrival_distance: 120.0,
            fast_pursuit_distance: 800.0,
            laser_range: 1_000.0,
            cannon_min_range: 250.0,
            cannon_max_range: 600.0,
            spaceport_staging_margin: 35.0,
            spaceport_staging_tolerance: 40.0,
            spaceport_staging_velocity_tolerance: 35.0,
            spaceport_corridor_half_width: 42.0,
            spaceport_contact_loss_grace_ticks: 30,
            spaceport_cruise_distance: 650.0,
            spaceport_approach_speed: 120.0,
            spaceport_ingress_speed: 36.0,
            spaceport_departure_speed: 80.0,
            spaceport_departure_alignment_radians: 0.35,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum StrategicObjective {
    Rebuild(PlanetId),
    Repair(PlanetId),
    Defend {
        planet: PlanetId,
        opponent: PlayerId,
    },
    Attack(PlayerId),
    Capture(PlanetId),
    Survive(Option<PlayerId>),
    Idle,
}

impl StrategicObjective {
    const fn goal(self) -> StrategicGoal {
        match self {
            Self::Rebuild(_) => StrategicGoal::Rebuild,
            Self::Repair(_) => StrategicGoal::Repair,
            Self::Defend { .. } => StrategicGoal::Defend,
            Self::Attack(_) => StrategicGoal::Attack,
            Self::Capture(_) => StrategicGoal::Capture,
            Self::Survive(_) => StrategicGoal::Survive,
            Self::Idle => StrategicGoal::Idle,
        }
    }

    const fn brain_goal(self) -> BrainGoal {
        match self {
            Self::Rebuild(_) => BrainGoal::Rebuild,
            Self::Repair(_) => BrainGoal::Repair,
            Self::Defend { .. } => BrainGoal::Defend,
            Self::Attack(_) => BrainGoal::Attack,
            Self::Capture(_) => BrainGoal::Capture,
            Self::Survive(_) => BrainGoal::Survive,
            Self::Idle => BrainGoal::Idle,
        }
    }

    const fn target(self) -> Option<PlayerId> {
        match self {
            Self::Defend { opponent, .. } | Self::Attack(opponent) => Some(opponent),
            Self::Survive(opponent) => opponent,
            _ => None,
        }
    }

    const fn target_planet(self) -> Option<PlanetId> {
        match self {
            Self::Rebuild(planet)
            | Self::Repair(planet)
            | Self::Capture(planet)
            | Self::Defend { planet, .. } => Some(planet),
            Self::Attack(_) | Self::Survive(_) | Self::Idle => None,
        }
    }

    const fn port_goal(self) -> Option<(BrainGoal, PlanetId)> {
        match self {
            Self::Rebuild(planet) => Some((BrainGoal::Rebuild, planet)),
            Self::Repair(planet) => Some((BrainGoal::Repair, planet)),
            Self::Capture(planet) => Some((BrainGoal::Capture, planet)),
            _ => None,
        }
    }

    const fn is_urgent(self) -> bool {
        matches!(
            self,
            Self::Rebuild(_) | Self::Repair(_) | Self::Defend { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct StrategyCandidate {
    objective: StrategicObjective,
    score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct StrategyState {
    objective: StrategicObjective,
    selected_score: f32,
    scores: StrategyScores,
    selected_at_tick: u64,
    last_evaluated_tick: u64,
    selection_reason: StrategySelectionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PortNavigation {
    goal: BrainGoal,
    planet: PlanetId,
    phase: PortNavigationPhase,
    departure_burn_started: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BodyAvoidance {
    body: AvoidanceBody,
    trajectory_direction: Vec2,
    direction: Vec2,
    surface_clearance: f32,
    outward_speed: f32,
    predictive: bool,
    seconds_until_closest: f32,
    predicted_surface_clearance: f32,
    tangent_sign: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BodyAvoidanceStatus {
    avoidance: BodyAvoidance,
    started_tick: u64,
    last_seen_tick: u64,
    last_progress_tick: u64,
    best_surface_clearance: f32,
    escape_assist: bool,
    emergency_escape_assist: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PredictiveBodyAvoidancePlan {
    body: AvoidanceBody,
    tangent_sign: f32,
    last_threat_tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RecentDeparture {
    planet: PlanetId,
    cleared_tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PortContactLoss {
    planet: PlanetId,
    started_tick: u64,
}

/// Deterministic first-generation Spacewars opponent.
///
/// It avoids imminent collisions, fights in planet-free worlds, and uses a
/// persistent staged maneuver to enter, hold, and leave moving spaceports.
/// Broader utility scoring and personality-driven strategy remain later work.
#[derive(Debug, Clone, Default)]
pub struct RuleShipBrain {
    config: RuleShipBrainConfig,
    actor: Option<PlayerId>,
    episode_seed: u64,
    telemetry: BrainTelemetry,
    strategy: Option<StrategyState>,
    port_navigation: Option<PortNavigation>,
    body_avoidance_status: Option<BodyAvoidanceStatus>,
    predictive_body_avoidance_plan: Option<PredictiveBodyAvoidancePlan>,
    recent_departure: Option<RecentDeparture>,
    port_contact_loss: Option<PortContactLoss>,
}

impl RuleShipBrain {
    pub fn new(config: RuleShipBrainConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    fn set_telemetry(&mut self, observation: &ShipObservationV1, telemetry: BrainTelemetry) {
        self.telemetry = BrainTelemetry {
            actor: self.actor,
            strategy: self.strategy_telemetry(observation.tick),
            ..telemetry
        };
    }

    fn strategy_telemetry(&self, tick: u64) -> StrategyTelemetry {
        self.strategy
            .map_or_else(StrategyTelemetry::default, |state| StrategyTelemetry {
                goal: state.objective.goal(),
                target: state.objective.target(),
                target_planet: state.objective.target_planet(),
                selected_score: Some(state.selected_score),
                scores: state.scores,
                selected_at_tick: state.selected_at_tick,
                age_ticks: tick.saturating_sub(state.selected_at_tick),
                selection_reason: Some(state.selection_reason),
            })
    }

    fn normalized_proximity(distance: f32, scale: f32) -> f32 {
        1.0 - (distance / scale.max(1.0)).clamp(0.0, 1.0)
    }

    fn weighted_score(utility: f32, weight: f32) -> f32 {
        let score = utility * weight.max(0.0);
        if score.is_finite() { score } else { 0.0 }
    }

    fn opponent_has_neutral_port_priority(
        &self,
        observation: &ShipObservationV1,
        planet: PlanetObservationV1,
    ) -> bool {
        if planet.owner.is_some() {
            return false;
        }
        let Some(opponent) = observation
            .opponent
            .filter(|opponent| !opponent.eliminated && opponent.form == ShipForm::Ship)
        else {
            return false;
        };

        let own_distance = planet.local_spaceport_position.length();
        let opponent_distance =
            (planet.local_spaceport_position - opponent.local_position).length();
        let tie_span = observation.own_ship.collision_radius + opponent.collision_radius;
        let immediate_staging_range =
            self.config.spaceport_staging_tolerance.max(0.0) * 2.0 + tie_span;
        if opponent_distance > immediate_staging_range {
            return false;
        }
        if opponent_distance + tie_span < own_distance {
            return true;
        }
        if own_distance + tie_span < opponent_distance {
            return false;
        }

        // Near-equal arrivals need a stable asymmetry or identical rule brains
        // can select, enter, and be ejected from the same neutral port forever.
        // Alternating priority by planet distributes those ties without giving
        // either player a global advantage.
        planet.id.index() % 2 != observation.actor.index()
    }

    fn strategy_candidates(
        &self,
        observation: &ShipObservationV1,
    ) -> (Vec<StrategyCandidate>, StrategyScores) {
        let mut candidates = Vec::with_capacity(observation.planets.len() * 3 + 1);
        let mut scores = StrategyScores::default();
        let actor = observation.actor;
        let opponent = observation.opponent.filter(|opponent| !opponent.eliminated);
        let clearance =
            observation.own_ship.collision_radius + self.config.spaceport_staging_margin;

        if observation.own_ship.form == ShipForm::EscapePod {
            for planet in observation
                .planets
                .iter()
                .filter(|planet| planet.owner == Some(actor))
            {
                let distance = planet.spaceport_approach(clearance).local_position.length();
                let proximity = Self::normalized_proximity(distance, observation.universe.radius);
                let candidate = StrategyCandidate {
                    objective: StrategicObjective::Rebuild(planet.id),
                    score: 2.0 + proximity * 0.1,
                };
                scores.record(candidate.objective.goal(), candidate.score);
                candidates.push(candidate);
            }
            if candidates.is_empty() {
                if let Some(opponent) = opponent {
                    let candidate = StrategyCandidate {
                        objective: StrategicObjective::Survive(Some(opponent.id)),
                        score: 0.5,
                    };
                    scores.record(candidate.objective.goal(), candidate.score);
                    candidates.push(candidate);
                } else {
                    candidates.push(StrategyCandidate {
                        objective: StrategicObjective::Idle,
                        score: 0.0,
                    });
                }
            }
            return (candidates, scores);
        }

        let continuing_repair = self.strategy.is_some_and(|state| {
            matches!(state.objective, StrategicObjective::Repair(_))
                && observation.own_ship.life_fraction
                    < self.config.strategy.repair_exit_life_fraction
        });
        if observation.own_ship.life_fraction < self.config.strategy.repair_enter_life_fraction
            || continuing_repair
        {
            let damage = (1.0 - observation.own_ship.life_fraction).clamp(0.0, 1.0);
            for planet in observation
                .planets
                .iter()
                .filter(|planet| planet.owner == Some(actor))
            {
                let distance = planet.spaceport_approach(clearance).local_position.length();
                let proximity = Self::normalized_proximity(distance, observation.universe.radius);
                let score = Self::weighted_score(
                    0.95 + damage * 0.25 + proximity * 0.05,
                    self.config.strategy.repair_weight,
                );
                let candidate = StrategyCandidate {
                    objective: StrategicObjective::Repair(planet.id),
                    score,
                };
                scores.record(candidate.objective.goal(), candidate.score);
                candidates.push(candidate);
            }
        }

        if let Some(opponent) = opponent {
            for planet in observation.planets.iter().filter(|planet| {
                planet.owner == Some(actor) && planet.capturing_player == Some(opponent.id)
            }) {
                let distance = planet.local_position.length();
                let proximity = Self::normalized_proximity(distance, observation.universe.radius);
                let score = Self::weighted_score(
                    1.2 + planet.capture_progress.clamp(0.0, 1.0) * 0.1 + proximity * 0.05,
                    self.config.strategy.defend_weight,
                );
                let candidate = StrategyCandidate {
                    objective: StrategicObjective::Defend {
                        planet: planet.id,
                        opponent: opponent.id,
                    },
                    score,
                };
                scores.record(candidate.objective.goal(), candidate.score);
                candidates.push(candidate);
            }

            let distance = opponent.local_position.length();
            let proximity = Self::normalized_proximity(
                distance,
                observation
                    .universe
                    .radius
                    .max(self.config.fast_pursuit_distance)
                    * 0.5,
            );
            let vulnerability = (1.0 - opponent.life_fraction).clamp(0.0, 1.0);
            let attack_utility = if opponent.form == ShipForm::EscapePod {
                // A pod with an owned planet can rebuild indefinitely. Taking
                // that territory is strategically decisive; interpreting the
                // pod's health as an easy ship kill creates endless chases.
                0.15 + proximity * 0.1
            } else {
                // Healthy opponents are a tactical opportunity, not a reason
                // to abandon the capture game. Combat overtakes a valid
                // capture primarily when a nearby opponent is already
                // vulnerable enough to finish decisively.
                0.25 + proximity * 0.25 + vulnerability * 0.45
            };
            let candidate = StrategyCandidate {
                objective: StrategicObjective::Attack(opponent.id),
                score: Self::weighted_score(attack_utility, self.config.strategy.attack_weight),
            };
            scores.record(candidate.objective.goal(), candidate.score);
            candidates.push(candidate);
        }

        for planet in observation
            .planets
            .iter()
            .filter(|planet| planet.owner != Some(actor))
        {
            if self.opponent_has_neutral_port_priority(observation, *planet) {
                continue;
            }
            let distance = planet.spaceport_approach(clearance).local_position.length();
            let proximity = Self::normalized_proximity(distance, observation.universe.radius);
            let neutral_bonus = if planet.owner.is_none() { 0.08 } else { 0.0 };
            let progress_bonus = if planet.capturing_player == Some(actor) {
                planet.capture_progress.clamp(0.0, 1.0) * 0.12
            } else {
                0.0
            };
            let candidate = StrategyCandidate {
                objective: StrategicObjective::Capture(planet.id),
                score: Self::weighted_score(
                    0.55 + proximity * 0.25 + neutral_bonus + progress_bonus,
                    self.config.strategy.capture_weight,
                ),
            };
            scores.record(candidate.objective.goal(), candidate.score);
            candidates.push(candidate);
        }

        if candidates.is_empty() {
            candidates.push(StrategyCandidate {
                objective: StrategicObjective::Idle,
                score: 0.0,
            });
        }
        (candidates, scores)
    }

    fn best_candidate(candidates: &[StrategyCandidate]) -> StrategyCandidate {
        let mut best = candidates[0];
        for &candidate in &candidates[1..] {
            let better_score = candidate.score.total_cmp(&best.score).is_gt();
            let deterministic_tie = candidate.score.total_cmp(&best.score).is_eq()
                && candidate.objective < best.objective;
            if better_score || deterministic_tie {
                best = candidate;
            }
        }
        best
    }

    fn objective_is_valid(
        &self,
        observation: &ShipObservationV1,
        objective: StrategicObjective,
    ) -> bool {
        let planet = |target| {
            observation
                .planets
                .iter()
                .find(|planet| planet.id == target)
        };
        let opponent_exists = |target| {
            observation
                .opponent
                .is_some_and(|opponent| opponent.id == target && !opponent.eliminated)
        };
        match objective {
            StrategicObjective::Rebuild(target) => {
                observation.own_ship.form == ShipForm::EscapePod
                    && planet(target).is_some_and(|planet| planet.owner == Some(observation.actor))
            }
            StrategicObjective::Repair(target) => {
                observation.own_ship.form == ShipForm::Ship
                    && observation.own_ship.life_fraction
                        < self.config.strategy.repair_exit_life_fraction
                    && planet(target).is_some_and(|planet| planet.owner == Some(observation.actor))
            }
            StrategicObjective::Defend {
                planet: target,
                opponent,
            } => {
                observation.own_ship.form == ShipForm::Ship
                    && opponent_exists(opponent)
                    && planet(target).is_some_and(|planet| {
                        planet.owner == Some(observation.actor)
                            && planet.capturing_player == Some(opponent)
                    })
            }
            StrategicObjective::Attack(target) => {
                observation.own_ship.form == ShipForm::Ship && opponent_exists(target)
            }
            StrategicObjective::Capture(target) => {
                observation.own_ship.form == ShipForm::Ship
                    && planet(target).is_some_and(|planet| planet.owner != Some(observation.actor))
            }
            StrategicObjective::Survive(_) => {
                observation.own_ship.form == ShipForm::EscapePod
                    && !observation
                        .planets
                        .iter()
                        .any(|planet| planet.owner == Some(observation.actor))
            }
            StrategicObjective::Idle => true,
        }
    }

    fn evaluate_strategy(
        &mut self,
        observation: &ShipObservationV1,
        forced_reason: Option<StrategySelectionReason>,
    ) {
        let (candidates, scores) = self.strategy_candidates(observation);
        let best = Self::best_candidate(&candidates);
        let Some(mut current) = self.strategy else {
            self.strategy = Some(StrategyState {
                objective: best.objective,
                selected_score: best.score,
                scores,
                selected_at_tick: observation.tick,
                last_evaluated_tick: observation.tick,
                selection_reason: forced_reason.unwrap_or(StrategySelectionReason::Initial),
            });
            return;
        };

        let current_candidate = candidates
            .iter()
            .find(|candidate| candidate.objective == current.objective)
            .copied();
        let current_valid = self.objective_is_valid(observation, current.objective);
        let continuing_repair =
            matches!(current.objective, StrategicObjective::Repair(_)) && current_valid;
        let committed = observation.tick.saturating_sub(current.selected_at_tick)
            < self.config.strategy.minimum_commitment_ticks;
        let current_score = current_candidate.map_or(0.0, |candidate| candidate.score);
        let urgent_challenger = best.objective != current.objective
            && best.objective.is_urgent()
            && best.score > current_score;
        let should_switch = !current_valid
            || (!continuing_repair
                && best.objective != current.objective
                && (urgent_challenger
                    || (!committed
                        && best.score
                            > current_score + self.config.strategy.switch_margin.max(0.0))));

        if should_switch {
            let reason = forced_reason.unwrap_or_else(|| {
                if !current_valid {
                    if observation.own_ship.form == ShipForm::EscapePod
                        && matches!(best.objective, StrategicObjective::Rebuild(_))
                    {
                        StrategySelectionReason::Mandatory
                    } else {
                        StrategySelectionReason::Invalidated
                    }
                } else if urgent_challenger {
                    StrategySelectionReason::Urgent
                } else {
                    StrategySelectionReason::HigherUtility
                }
            });
            current.objective = best.objective;
            current.selected_at_tick = observation.tick;
            current.selection_reason = reason;
            current.selected_score = best.score;
        } else {
            current.selected_score = current_score;
        }
        current.scores = scores;
        current.last_evaluated_tick = observation.tick;
        self.strategy = Some(current);
    }

    fn maybe_evaluate_strategy(&mut self, observation: &ShipObservationV1) {
        let Some(strategy) = self.strategy else {
            self.evaluate_strategy(observation, None);
            return;
        };
        let invalid = !self.objective_is_valid(observation, strategy.objective);
        let urgent = match observation.own_ship.form {
            ShipForm::EscapePod => {
                !matches!(strategy.objective, StrategicObjective::Rebuild(_))
                    && observation
                        .planets
                        .iter()
                        .any(|planet| planet.owner == Some(observation.actor))
            }
            ShipForm::Ship => {
                (!matches!(strategy.objective, StrategicObjective::Repair(_))
                    && observation.own_ship.life_fraction
                        < self.config.strategy.repair_enter_life_fraction
                    && observation
                        .planets
                        .iter()
                        .any(|planet| planet.owner == Some(observation.actor)))
                    || (!matches!(strategy.objective, StrategicObjective::Defend { .. })
                        && observation.opponent.is_some_and(|opponent| {
                            !opponent.eliminated
                                && observation.planets.iter().any(|planet| {
                                    planet.owner == Some(observation.actor)
                                        && planet.capturing_player == Some(opponent.id)
                                })
                        }))
            }
        };
        let due = observation
            .tick
            .saturating_sub(strategy.last_evaluated_tick)
            >= self.config.strategy.evaluation_interval_ticks.max(1);
        if invalid || urgent || due {
            self.evaluate_strategy(observation, None);
        }
    }

    fn adopt_docking_objective(
        &mut self,
        observation: &ShipObservationV1,
        objective: StrategicObjective,
    ) {
        if self
            .strategy
            .is_some_and(|strategy| strategy.objective == objective)
        {
            return;
        }
        let scores = self
            .strategy
            .map_or_else(StrategyScores::default, |strategy| strategy.scores);
        self.strategy = Some(StrategyState {
            objective,
            selected_score: 0.0,
            scores,
            selected_at_tick: observation.tick,
            last_evaluated_tick: observation.tick,
            selection_reason: StrategySelectionReason::DockingContact,
        });
    }

    fn sync_port_navigation_to_strategy(&mut self) {
        if self.port_navigation.is_some_and(|navigation| {
            matches!(
                navigation.phase,
                PortNavigationPhase::Docked | PortNavigationPhase::Depart
            )
        }) {
            return;
        }
        let target = self
            .strategy
            .and_then(|strategy| strategy.objective.port_goal());
        self.port_navigation = match (self.port_navigation, target) {
            (Some(navigation), Some((goal, planet)))
                if navigation.goal == goal && navigation.planet == planet =>
            {
                Some(navigation)
            }
            (_, Some((goal, planet))) => Some(PortNavigation {
                goal,
                planet,
                phase: PortNavigationPhase::Rendezvous,
                departure_burn_started: false,
            }),
            (_, None) => None,
        };
    }

    fn refresh_port_navigation(&mut self, observation: &ShipObservationV1) {
        if observation.own_ship.form == ShipForm::EscapePod
            && self
                .port_navigation
                .is_some_and(|navigation| navigation.phase == PortNavigationPhase::Depart)
        {
            // Depart is a full-ship state reached after capture, repair, or
            // rebuild completes. If that ship is destroyed during launch,
            // the replacement pod must choose an owned rebuild port instead
            // of inheriting a departure it can never complete.
            self.port_navigation = None;
            self.port_contact_loss = None;
        }

        if let Some(docked_planet) = observation.own_ship.docked_planet {
            let already_tracking_contact = self
                .port_navigation
                .is_some_and(|navigation| navigation.planet == docked_planet);
            if !already_tracking_contact {
                let docked = observation
                    .planets
                    .iter()
                    .find(|planet| planet.id == docked_planet);
                let navigation = match (observation.own_ship.form, docked) {
                    (ShipForm::Ship, Some(planet)) if planet.owner == Some(observation.actor) => {
                        Some(PortNavigation {
                            goal: BrainGoal::Capture,
                            planet: docked_planet,
                            phase: PortNavigationPhase::Depart,
                            departure_burn_started: false,
                        })
                    }
                    (ShipForm::Ship, Some(_)) => Some(PortNavigation {
                        goal: BrainGoal::Capture,
                        planet: docked_planet,
                        phase: PortNavigationPhase::Docked,
                        departure_burn_started: false,
                    }),
                    (ShipForm::EscapePod, Some(planet))
                        if planet.owner == Some(observation.actor) =>
                    {
                        Some(PortNavigation {
                            goal: BrainGoal::Rebuild,
                            planet: docked_planet,
                            phase: PortNavigationPhase::Docked,
                            departure_burn_started: false,
                        })
                    }
                    _ => self.port_navigation,
                };
                self.port_navigation = navigation;
                match navigation.map(|navigation| navigation.goal) {
                    Some(BrainGoal::Capture)
                        if docked.is_some_and(|planet| planet.owner != Some(observation.actor)) =>
                    {
                        self.adopt_docking_objective(
                            observation,
                            StrategicObjective::Capture(docked_planet),
                        )
                    }
                    Some(BrainGoal::Rebuild) => self.adopt_docking_objective(
                        observation,
                        StrategicObjective::Rebuild(docked_planet),
                    ),
                    _ => {}
                }
            }
        }

        if let Some(mut navigation) = self.port_navigation {
            let planet = observation
                .planets
                .iter()
                .find(|planet| planet.id == navigation.planet);
            let keep_target = planet.is_some_and(|planet| match navigation.goal {
                BrainGoal::Capture => {
                    (observation.own_ship.form == ShipForm::Ship
                        && planet.owner != Some(observation.actor))
                        || navigation.phase == PortNavigationPhase::Depart
                        || (navigation.phase == PortNavigationPhase::Docked
                            && planet.owner == Some(observation.actor))
                }
                BrainGoal::Rebuild => {
                    (observation.own_ship.form == ShipForm::EscapePod
                        && planet.owner == Some(observation.actor))
                        || navigation.phase == PortNavigationPhase::Depart
                        || (navigation.phase == PortNavigationPhase::Docked
                            && observation.own_ship.form == ShipForm::Ship)
                }
                BrainGoal::Repair => {
                    (observation.own_ship.form == ShipForm::Ship
                        && planet.owner == Some(observation.actor)
                        && observation.own_ship.life_fraction
                            < self.config.strategy.repair_exit_life_fraction)
                        || navigation.phase == PortNavigationPhase::Depart
                }
                _ => false,
            });

            if !keep_target {
                self.port_navigation = None;
                self.port_contact_loss = None;
            } else {
                let docked_here = observation.own_ship.docked_planet == Some(navigation.planet);
                if docked_here && navigation.phase != PortNavigationPhase::Depart {
                    navigation.phase = PortNavigationPhase::Docked;
                    self.port_contact_loss = None;
                }
                let completed = planet.is_some_and(|planet| match navigation.goal {
                    BrainGoal::Capture => planet.owner == Some(observation.actor),
                    BrainGoal::Rebuild => observation.own_ship.form == ShipForm::Ship,
                    BrainGoal::Repair => {
                        planet.owner == Some(observation.actor)
                            && observation.own_ship.life_fraction
                                >= self.config.strategy.repair_exit_life_fraction
                    }
                    _ => false,
                });
                if navigation.phase == PortNavigationPhase::Docked && completed {
                    navigation.phase = PortNavigationPhase::Depart;
                    self.port_contact_loss = None;
                } else if navigation.phase == PortNavigationPhase::Docked && !docked_here {
                    // Full-ship sensor contact can flicker while the craft is
                    // still settling inside the port. Hold the existing dock
                    // through a bounded gap, then treat continued absence as
                    // authoritative and reacquire. Pods get no grace because
                    // their released brake is also automatic forward thrust.
                    let contact_loss = self
                        .port_contact_loss
                        .filter(|loss| loss.planet == navigation.planet)
                        .unwrap_or(PortContactLoss {
                            planet: navigation.planet,
                            started_tick: observation.tick,
                        });
                    self.port_contact_loss = Some(contact_loss);
                    let grace_ticks = if observation.own_ship.form == ShipForm::EscapePod {
                        0
                    } else {
                        self.config.spaceport_contact_loss_grace_ticks
                    };
                    if observation.tick.saturating_sub(contact_loss.started_tick) >= grace_ticks {
                        navigation.phase = PortNavigationPhase::Ingress;
                        self.port_contact_loss = None;
                    }
                } else if navigation.phase != PortNavigationPhase::Docked {
                    self.port_contact_loss = None;
                }
                self.port_navigation = Some(navigation);
            }
        }

        let Some(mut navigation) = self.port_navigation else {
            return;
        };
        let Some(planet) = observation
            .planets
            .iter()
            .find(|planet| planet.id == navigation.planet)
        else {
            return;
        };
        let clearance =
            observation.own_ship.collision_radius + self.config.spaceport_staging_margin;
        let target = match navigation.phase {
            PortNavigationPhase::Rendezvous => {
                let ship_from_center = -planet.local_position;
                planet.moving_surface_approach(ship_from_center, clearance)
            }
            PortNavigationPhase::Approach => planet.spaceport_approach(clearance),
            _ => return,
        };
        let staging_position_tolerance = self.config.spaceport_staging_tolerance
            + if observation.own_ship.form == ShipForm::EscapePod {
                // A pod cannot hover at an exact center point. Treat its hull
                // reaching the configured staging disk as arrival instead of
                // making its auto-cruise orbit a point just outside the edge.
                observation.own_ship.collision_radius
            } else {
                0.0
            };
        let reached_staging_position = target.local_position.length() <= staging_position_tolerance;
        let matched_staging_velocity =
            target.local_velocity.length() <= self.config.spaceport_staging_velocity_tolerance;
        // A full ship can independently steer and throttle, so it should
        // establish a stable position-and-velocity match before changing
        // targets. A pod auto-cruises whenever its brake is released and
        // cannot hold that same moving-ring match. Reaching the ring is enough
        // to advance it toward the concrete spaceport instead of orbiting the
        // radial rendezvous point forever.
        if reached_staging_position
            && (observation.own_ship.form == ShipForm::EscapePod || matched_staging_velocity)
        {
            navigation.phase = match navigation.phase {
                PortNavigationPhase::Rendezvous => PortNavigationPhase::Approach,
                PortNavigationPhase::Approach => PortNavigationPhase::Ingress,
                _ => unreachable!(),
            };
            self.port_navigation = Some(navigation);
        }
    }

    fn port_corridor_contains_ship(
        &self,
        observation: &ShipObservationV1,
        planet: &PlanetObservationV1,
    ) -> bool {
        let clearance =
            observation.own_ship.collision_radius + self.config.spaceport_staging_margin;
        let approach = planet.spaceport_approach(clearance);
        let ship_from_center = -planet.local_position;
        let axial_distance = ship_from_center.dot(approach.local_outward);
        let lateral_distance =
            (ship_from_center - approach.local_outward * axial_distance).length();
        let port_radius = (planet.local_spaceport_position - planet.local_position).length();
        axial_distance >= port_radius - observation.own_ship.collision_radius * 2.0
            && lateral_distance <= self.config.spaceport_corridor_half_width
    }

    fn body_avoidance(
        &mut self,
        observation: &ShipObservationV1,
        port_navigation: Option<PortNavigation>,
    ) -> Option<BodyAvoidance> {
        let guarded_departure = self.recent_departure.filter(|departure| {
            observation.tick
                <= departure
                    .cleared_tick
                    .saturating_add(self.config.departure_reentry_guard_ticks)
        });
        self.recent_departure = guarded_departure;
        let remembered_plan = self.predictive_body_avoidance_plan.filter(|plan| {
            observation.tick
                <= plan
                    .last_threat_tick
                    .saturating_add(self.config.body_avoidance_memory_ticks)
        });
        let mut nearest: Option<BodyAvoidance> = None;
        let mut consider = |body: AvoidanceBody,
                            position: Vec2,
                            velocity: Vec2,
                            radius: f32,
                            clearance: f32,
                            allow_prediction: bool| {
            let surface_distance =
                position.length() - radius - observation.own_ship.collision_radius;
            let outward_speed = if position.length_squared() > TARGET_EPSILON {
                position.normalized().dot(velocity)
            } else {
                0.0
            };
            let reactive = surface_distance <= clearance
                && !(outward_speed >= 0.0 && surface_distance > clearance * 0.35);
            let candidate = if reactive {
                BodyAvoidance {
                    body,
                    trajectory_direction: -position,
                    direction: -position,
                    surface_clearance: surface_distance,
                    outward_speed,
                    predictive: false,
                    seconds_until_closest: 0.0,
                    predicted_surface_clearance: surface_distance,
                    tangent_sign: 0.0,
                }
            } else {
                if !allow_prediction {
                    return;
                }
                let relative_speed_squared = velocity.length_squared();
                if relative_speed_squared <= TARGET_EPSILON {
                    return;
                }
                let horizon = self.config.body_avoidance_horizon_seconds.max(0.0);
                let seconds_until_closest = -position.dot(velocity) / relative_speed_squared;
                if !(0.0..=horizon).contains(&seconds_until_closest) {
                    return;
                }
                let closest = position + velocity * seconds_until_closest;
                let predicted_surface_clearance =
                    closest.length() - radius - observation.own_ship.collision_radius;
                if predicted_surface_clearance > clearance {
                    return;
                }
                let required_separation =
                    radius + observation.own_ship.collision_radius + clearance.max(0.0);
                let tangent_sign = remembered_plan
                    .filter(|plan| plan.body == body)
                    .map_or_else(
                        || preferred_body_tangent_sign(position, velocity),
                        |plan| plan.tangent_sign,
                    );
                let trajectory_direction = body_tangent_direction(
                    position,
                    required_separation,
                    self.config.body_avoidance_tangent_margin_radians,
                    tangent_sign,
                );
                let current_travel = -velocity;
                let desired_travel = trajectory_direction.normalized() * current_travel.length();
                let velocity_error = desired_travel - current_travel;
                BodyAvoidance {
                    body,
                    trajectory_direction,
                    direction: if velocity_error.length_squared() > TARGET_EPSILON {
                        velocity_error
                    } else {
                        trajectory_direction
                    },
                    surface_clearance: surface_distance,
                    outward_speed,
                    predictive: true,
                    seconds_until_closest,
                    predicted_surface_clearance,
                    tangent_sign,
                }
            };
            let precedes = nearest.is_none_or(|avoidance| {
                match (candidate.predictive, avoidance.predictive) {
                    (false, true) => true,
                    (true, false) => false,
                    (false, false) => candidate.surface_clearance < avoidance.surface_clearance,
                    (true, true) => {
                        candidate
                            .seconds_until_closest
                            .total_cmp(&avoidance.seconds_until_closest)
                            .is_lt()
                            || (candidate
                                .seconds_until_closest
                                .total_cmp(&avoidance.seconds_until_closest)
                                .is_eq()
                                && candidate.predicted_surface_clearance
                                    < avoidance.predicted_surface_clearance)
                    }
                }
            });
            if precedes {
                nearest = Some(candidate);
            }
        };

        if let Some(sun) = observation.sun {
            consider(
                AvoidanceBody::Sun,
                sun.local_position,
                sun.local_velocity,
                sun.radius,
                self.config.body_clearance,
                false,
            );
        }
        for planet in &observation.planets {
            let navigation = port_navigation.filter(|navigation| navigation.planet == planet.id);
            let clearance = if let Some(navigation) = navigation {
                if navigation.phase != PortNavigationPhase::Rendezvous
                    && self.port_corridor_contains_ship(observation, planet)
                {
                    continue;
                }
                self.config.spaceport_staging_margin
            } else {
                self.config.body_clearance
            };
            consider(
                AvoidanceBody::Planet(planet.id),
                planet.local_position,
                planet.local_velocity,
                planet.radius,
                clearance,
                navigation.is_none()
                    && guarded_departure.is_some_and(|departure| departure.planet == planet.id),
            );
        }
        if let Some(avoidance) = nearest.filter(|avoidance| avoidance.predictive) {
            self.predictive_body_avoidance_plan = Some(PredictiveBodyAvoidancePlan {
                body: avoidance.body,
                tangent_sign: avoidance.tangent_sign,
                last_threat_tick: observation.tick,
            });
        } else if remembered_plan.is_none() {
            self.predictive_body_avoidance_plan = None;
        }
        nearest
    }

    fn update_body_avoidance_status(
        &mut self,
        tick: u64,
        life_fraction: f32,
        avoidance: BodyAvoidance,
    ) -> BodyAvoidanceStatus {
        let progress_distance = self.config.body_avoidance_progress_distance.max(0.0);
        let mut status = self
            .body_avoidance_status
            .filter(|status| {
                status.avoidance.body == avoidance.body
                    && tick
                        <= status
                            .last_seen_tick
                            .saturating_add(self.config.body_avoidance_memory_ticks)
            })
            .unwrap_or(BodyAvoidanceStatus {
                avoidance,
                started_tick: tick,
                last_seen_tick: tick,
                last_progress_tick: tick,
                best_surface_clearance: avoidance.surface_clearance,
                escape_assist: false,
                emergency_escape_assist: false,
            });
        status.avoidance = avoidance;
        status.last_seen_tick = tick;
        if avoidance.surface_clearance >= status.best_surface_clearance + progress_distance {
            status.best_surface_clearance = avoidance.surface_clearance;
            status.last_progress_tick = tick;
        }
        let stalled_ticks = tick.saturating_sub(status.last_progress_tick);
        let emergency_trap = life_fraction
            <= self
                .config
                .body_avoidance_emergency_life_fraction
                .clamp(0.0, 1.0)
            && avoidance.surface_clearance <= 0.0
            && avoidance.outward_speed <= 0.0
            && stalled_ticks >= self.config.body_avoidance_emergency_stall_ticks;
        // A low-health ship that remains penetrated and moving inward cannot
        // afford the full nearby-body timeout. Ordinary grazes and healthy
        // navigation retain the established braking behavior.
        if emergency_trap {
            status.emergency_escape_assist = true;
            status.escape_assist = true;
        } else if stalled_ticks >= self.config.body_avoidance_stall_ticks {
            status.escape_assist = true;
        }
        self.body_avoidance_status = Some(status);
        status
    }

    fn avoidance_intent(
        &mut self,
        observation: &ShipObservationV1,
        direction: Vec2,
        goal: BrainGoal,
        hazard: Option<DebrisId>,
        body_status: Option<BodyAvoidanceStatus>,
    ) -> ShipIntent {
        let heading = guide_heading(direction, observation.own_ship.angular_velocity);
        let navigation = self.port_navigation;
        self.set_telemetry(
            observation,
            BrainTelemetry {
                goal,
                target_planet: navigation.map(|navigation| navigation.planet),
                port_phase: navigation.map(|navigation| navigation.phase),
                hazard,
                avoided_body: body_status.map(|status| status.avoidance.body),
                avoidance_surface_clearance: body_status
                    .map(|status| status.avoidance.surface_clearance),
                avoidance_outward_speed: body_status.map(|status| status.avoidance.outward_speed),
                avoidance_predictive: body_status.is_some_and(|status| status.avoidance.predictive),
                avoidance_seconds_until_closest: body_status
                    .map(|status| status.avoidance.seconds_until_closest),
                avoidance_predicted_surface_clearance: body_status
                    .map(|status| status.avoidance.predicted_surface_clearance),
                avoidance_age_ticks: body_status.map_or(0, |status| {
                    observation.tick.saturating_sub(status.started_tick)
                }),
                avoidance_stalled_ticks: body_status.map_or(0, |status| {
                    observation.tick.saturating_sub(status.last_progress_tick)
                }),
                avoidance_escape_assist: body_status.is_some_and(|status| status.escape_assist),
                avoidance_emergency_escape_assist: body_status
                    .is_some_and(|status| status.emergency_escape_assist),
                target_distance: direction.length(),
                heading_error: heading.error_radians,
                ..BrainTelemetry::default()
            },
        );
        let heading_error = heading.error_radians.abs();
        let predictive = body_status.is_some_and(|status| status.avoidance.predictive);
        // A forecast maneuver should shed velocity while establishing its
        // tangent instead of accelerating merely because the desired heading
        // is within the broader emergency-escape cone.
        let forward_aligned = heading_error < if predictive { 0.2 } else { 0.65 };
        let reverse_escape = observation.own_ship.form == ShipForm::Ship
            && body_status.is_some_and(|status| status.emergency_escape_assist)
            && PI - heading_error < 0.65;
        let requested_brake = if forward_aligned || reverse_escape {
            0.0
        } else if predictive || body_status.is_some_and(|status| status.escape_assist) {
            self.config.body_avoidance_turn_brake.clamp(0.0, 0.9)
        } else {
            1.0
        };
        ShipIntent {
            turn: heading.turn,
            thrust: if forward_aligned {
                1.0
            } else if reverse_escape {
                -1.0
            } else {
                0.0
            },
            // A stalled escape drops to a partial brake so the turn command
            // can build angular velocity. A low-health emergency also uses a
            // rear-aligned reverse burn immediately rather than waiting to
            // point the nose outward. Pods suppress braking whenever they
            // steer because releasing their brake is also their thrust.
            brake: steering_safe_brake(observation.own_ship.form, heading, requested_brake),
            ..ShipIntent::default()
        }
    }

    fn docked_intent(
        &mut self,
        observation: &ShipObservationV1,
        navigation: PortNavigation,
    ) -> ShipIntent {
        let heading = guide_heading(Vec2::Y, observation.own_ship.angular_velocity);
        self.set_telemetry(
            observation,
            BrainTelemetry {
                goal: navigation.goal,
                target_planet: Some(navigation.planet),
                port_phase: Some(PortNavigationPhase::Docked),
                heading_error: heading.error_radians,
                ..BrainTelemetry::default()
            },
        );
        ShipIntent {
            // Holding the brake prevents an arriving fast-mode ship from
            // converting the wing-opening animation into forward speed while
            // it is supposed to be settling and capturing. Depart explicitly
            // releases the brake before beginning its launch burn.
            brake: if observation.own_ship.form == ShipForm::Ship {
                1.0
            } else {
                0.0
            },
            ..ShipIntent::default()
        }
    }

    fn port_navigation_intent(
        &mut self,
        observation: &ShipObservationV1,
        navigation: PortNavigation,
        planet: PlanetObservationV1,
    ) -> ShipIntent {
        if navigation.phase == PortNavigationPhase::Docked {
            return self.docked_intent(observation, navigation);
        }

        let clearance =
            observation.own_ship.collision_radius + self.config.spaceport_staging_margin;
        let approach = planet.spaceport_approach(clearance);
        let rendezvous = planet.moving_surface_approach(-planet.local_position, clearance);
        if navigation.phase == PortNavigationPhase::Depart {
            let surface_clearance = planet.local_position.length()
                - planet.radius
                - observation.own_ship.collision_radius;
            let departure_clearance = self
                .config
                .body_clearance
                .max(self.config.spaceport_staging_margin);
            let cleared = observation.own_ship.docked_planet != Some(navigation.planet)
                && surface_clearance >= departure_clearance;
            if cleared {
                self.port_navigation = None;
                self.recent_departure = Some(RecentDeparture {
                    planet: navigation.planet,
                    cleared_tick: observation.tick,
                });
            }
            let heading = guide_heading(
                approach.local_outward,
                observation.own_ship.angular_velocity,
            );
            let launch_aligned =
                heading.error_radians.abs() <= self.config.spaceport_departure_alignment_radians;
            let departure_burn_started = navigation.departure_burn_started || launch_aligned;
            if !cleared && departure_burn_started != navigation.departure_burn_started {
                self.port_navigation = Some(PortNavigation {
                    departure_burn_started,
                    ..navigation
                });
            }
            self.set_telemetry(
                observation,
                BrainTelemetry {
                    goal: navigation.goal,
                    target_planet: Some(navigation.planet),
                    port_phase: Some(navigation.phase),
                    target_distance: (departure_clearance - surface_clearance).max(0.0),
                    heading_error: heading.error_radians,
                    desired_speed: self.config.spaceport_departure_speed,
                    relative_speed: approach.local_velocity.length(),
                    ..BrainTelemetry::default()
                },
            );
            return ShipIntent {
                turn: heading.turn,
                thrust: if departure_burn_started { 1.0 } else { 0.0 },
                // Spaceport contact already damps and centers linear motion.
                // Leaving the general brake off preserves angular authority
                // while the ship turns inside the bay before launch.
                brake: 0.0,
                wings_closed: observation.own_ship.form == ShipForm::Ship && departure_burn_started,
                ..ShipIntent::default()
            };
        }
        let (target_position, target_velocity, max_closing_speed, position_tolerance) =
            match navigation.phase {
                PortNavigationPhase::Rendezvous => (
                    rendezvous.local_position,
                    rendezvous.local_velocity,
                    self.config.spaceport_approach_speed,
                    self.config.spaceport_staging_tolerance,
                ),
                PortNavigationPhase::Approach => (
                    approach.local_position,
                    approach.local_velocity,
                    self.config.spaceport_approach_speed,
                    self.config.spaceport_staging_tolerance,
                ),
                PortNavigationPhase::Ingress => (
                    planet.local_spaceport_position,
                    planet.local_spaceport_velocity,
                    self.config.spaceport_ingress_speed,
                    observation.own_ship.collision_radius.max(3.0),
                ),
                PortNavigationPhase::Docked | PortNavigationPhase::Depart => unreachable!(),
            };
        let guidance = guide_arrival(
            target_position,
            target_velocity,
            observation.own_ship.local_velocity,
            observation.own_ship.angular_velocity,
            ArrivalGuidanceConfig {
                max_closing_speed,
                arrival_time_seconds: if navigation.phase == PortNavigationPhase::Ingress {
                    1.0
                } else {
                    1.5
                },
                position_tolerance,
                velocity_tolerance: if navigation.phase == PortNavigationPhase::Ingress {
                    12.0
                } else {
                    self.config.spaceport_staging_velocity_tolerance
                },
                full_control_velocity_error: if observation.own_ship.form == ShipForm::EscapePod {
                    120.0
                } else {
                    30.0
                },
                thrust_alignment_radians: match navigation.phase {
                    PortNavigationPhase::Rendezvous => 1.4,
                    PortNavigationPhase::Approach => 1.5,
                    _ => 0.5,
                },
            },
        );

        self.set_telemetry(
            observation,
            BrainTelemetry {
                goal: navigation.goal,
                target_planet: Some(navigation.planet),
                port_phase: Some(navigation.phase),
                target_distance: guidance.target_distance,
                heading_error: guidance.heading.error_radians,
                desired_speed: guidance.desired_closing_speed,
                relative_speed: guidance.relative_speed,
                ..BrainTelemetry::default()
            },
        );

        let wings_closed = observation.own_ship.form == ShipForm::Ship
            && navigation.phase == PortNavigationPhase::Rendezvous
            && guidance.target_distance > self.config.spaceport_cruise_distance
            && guidance.heading.error_radians.abs() < 0.12;
        let (thrust, requested_brake) = if observation.own_ship.form == ShipForm::EscapePod {
            if guidance.thrust >= 0.75 {
                (1.0, 0.0)
            } else {
                (0.0, 1.0)
            }
        } else {
            (guidance.thrust, guidance.brake)
        };
        let brake =
            steering_safe_brake(observation.own_ship.form, guidance.heading, requested_brake);
        ShipIntent {
            turn: guidance.heading.turn,
            thrust,
            brake,
            wings_closed,
            ..ShipIntent::default()
        }
        .normalized()
    }

    fn body_avoidance_intent(
        &mut self,
        observation: &ShipObservationV1,
        avoidance: BodyAvoidance,
    ) -> ShipIntent {
        // `body_avoidance` has already excluded valid spaceport-corridor
        // contact. Any planet it returns here is an unsafe solid-body
        // encounter, even when another planet is the strategic destination.
        // Navigation intent must never suppress physical escape.
        let escape_assist_eligible = matches!(avoidance.body, AvoidanceBody::Planet(_))
            && avoidance.surface_clearance <= self.config.body_avoidance_stall_clearance.max(0.0);
        let status = if escape_assist_eligible {
            self.update_body_avoidance_status(
                observation.tick,
                observation.own_ship.life_fraction,
                avoidance,
            )
        } else {
            self.body_avoidance_status = None;
            BodyAvoidanceStatus {
                avoidance,
                started_tick: observation.tick,
                last_seen_tick: observation.tick,
                last_progress_tick: observation.tick,
                best_surface_clearance: avoidance.surface_clearance,
                escape_assist: false,
                emergency_escape_assist: false,
            }
        };
        self.avoidance_intent(
            observation,
            avoidance.direction,
            BrainGoal::AvoidBody,
            None,
            Some(status),
        )
    }

    fn survival_intent(&mut self, observation: &ShipObservationV1) -> ShipIntent {
        let Some(opponent) = observation.opponent.filter(|opponent| !opponent.eliminated) else {
            self.set_telemetry(observation, BrainTelemetry::default());
            return ShipIntent::default();
        };
        let heading = guide_heading(
            -opponent.local_position,
            observation.own_ship.angular_velocity,
        );
        self.set_telemetry(
            observation,
            BrainTelemetry {
                goal: BrainGoal::Survive,
                target: Some(opponent.id),
                target_distance: opponent.local_position.length(),
                heading_error: heading.error_radians,
                relative_speed: opponent.local_velocity.length(),
                ..BrainTelemetry::default()
            },
        );
        let aligned = heading.error_radians.abs() < 0.65;
        let requested_brake = if aligned { 0.0 } else { 1.0 };
        ShipIntent {
            turn: heading.turn,
            thrust: if aligned { 1.0 } else { 0.0 },
            brake: steering_safe_brake(observation.own_ship.form, heading, requested_brake),
            ..ShipIntent::default()
        }
    }

    fn combat_intent(
        &mut self,
        observation: &ShipObservationV1,
        goal: BrainGoal,
        defended_planet: Option<PlanetId>,
    ) -> ShipIntent {
        let Some(opponent) = observation.opponent.filter(|opponent| !opponent.eliminated) else {
            self.set_telemetry(observation, BrainTelemetry::default());
            return ShipIntent::default();
        };
        let distance = opponent.local_position.length();
        let aim_position =
            intercept_position(opponent.local_position, opponent.local_velocity, 300.0, 1.0);
        let heading = guide_heading(aim_position, observation.own_ship.angular_velocity);
        let line_to_target = opponent.local_position.normalized();
        let closing_speed = -opponent.local_velocity.dot(line_to_target);
        let should_brake = distance < self.config.arrival_distance && closing_speed > 20.0;
        let should_advance = !should_brake
            && distance > self.config.arrival_distance
            && heading.error_radians.abs() < 0.65;
        let fast_pursuit =
            distance > self.config.fast_pursuit_distance && heading.error_radians.abs() < 0.12;
        let weapon_aligned = heading.error_radians.abs() < 0.08;

        self.set_telemetry(
            observation,
            BrainTelemetry {
                goal,
                target: Some(opponent.id),
                target_planet: defended_planet,
                target_distance: distance,
                heading_error: heading.error_radians,
                relative_speed: opponent.local_velocity.length(),
                ..BrainTelemetry::default()
            },
        );
        ShipIntent {
            turn: heading.turn,
            thrust: if should_advance { 1.0 } else { 0.0 },
            brake: if should_brake { 1.0 } else { 0.0 },
            wings_closed: fast_pursuit,
            laser: observation.own_ship.laser_available
                && weapon_aligned
                && distance <= self.config.laser_range,
            cannon: observation.own_ship.cannon_ready
                && weapon_aligned
                && distance >= self.config.cannon_min_range
                && distance <= self.config.cannon_max_range,
        }
        .normalized()
    }
}

impl ShipBrain for RuleShipBrain {
    fn reset(&mut self, reset: BrainReset) {
        self.actor = Some(reset.actor);
        self.episode_seed = reset.episode_seed;
        self.strategy = None;
        self.port_navigation = None;
        self.body_avoidance_status = None;
        self.predictive_body_avoidance_plan = None;
        self.recent_departure = None;
        self.port_contact_loss = None;
        self.telemetry = BrainTelemetry {
            actor: self.actor,
            ..BrainTelemetry::default()
        };
    }

    fn intent(&mut self, observation: &ShipObservationV1) -> ShipIntent {
        if self.actor != Some(observation.actor) || observation.own_ship.eliminated {
            self.telemetry = BrainTelemetry {
                actor: self.actor,
                ..BrainTelemetry::default()
            };
            return ShipIntent::default();
        }

        let port_was_locked = self.port_navigation.is_some_and(|navigation| {
            matches!(
                navigation.phase,
                PortNavigationPhase::Docked | PortNavigationPhase::Depart
            )
        });
        if !port_was_locked || self.strategy.is_none() {
            self.maybe_evaluate_strategy(observation);
        }
        self.sync_port_navigation_to_strategy();
        self.refresh_port_navigation(observation);
        match self.port_navigation {
            Some(navigation) if navigation.phase == PortNavigationPhase::Docked => {
                return self.docked_intent(observation, navigation);
            }
            Some(navigation) if navigation.phase == PortNavigationPhase::Depart => {
                if let Some(planet) = observation
                    .planets
                    .iter()
                    .find(|planet| planet.id == navigation.planet)
                    .copied()
                {
                    // A ship inside the planet must follow the known-safe port
                    // corridor. Generic body and hazard avoidance can point it
                    // into the cavity wall or cancel the launch burn. Once
                    // this atomic departure clears, ordinary avoidance and
                    // the bounded origin re-entry guard take over.
                    return self.port_navigation_intent(observation, navigation, planet);
                }
                self.port_navigation = None;
            }
            _ => {}
        }

        // Refreshing contact can invalidate or complete a target. Reassess
        // immediately when it is safe to leave the port-navigation state.
        self.maybe_evaluate_strategy(observation);
        self.sync_port_navigation_to_strategy();
        self.refresh_port_navigation(observation);
        match self.port_navigation {
            Some(navigation) if navigation.phase == PortNavigationPhase::Docked => {
                return self.docked_intent(observation, navigation);
            }
            Some(navigation) if navigation.phase == PortNavigationPhase::Depart => {
                if let Some(planet) = observation
                    .planets
                    .iter()
                    .find(|planet| planet.id == navigation.planet)
                    .copied()
                {
                    return self.port_navigation_intent(observation, navigation, planet);
                }
                self.port_navigation = None;
            }
            _ => {}
        }

        if let Some(avoidance) = self.body_avoidance(observation, self.port_navigation) {
            return self.body_avoidance_intent(observation, avoidance);
        }
        if let Some(threat) = earliest_collision_threat(
            &observation.hazards,
            observation.own_ship.collision_radius,
            self.config.hazard_horizon_seconds,
            self.config.hazard_clearance,
        ) {
            return self.avoidance_intent(
                observation,
                threat.avoidance_direction,
                BrainGoal::AvoidHazard,
                Some(threat.hazard),
                None,
            );
        }

        if let Some(navigation) = self.port_navigation {
            if let Some(planet) = observation
                .planets
                .iter()
                .find(|planet| planet.id == navigation.planet)
                .copied()
            {
                return self.port_navigation_intent(observation, navigation, planet);
            }
            self.port_navigation = None;
        }

        match self.strategy.map(|strategy| strategy.objective) {
            Some(StrategicObjective::Attack(_)) => {
                self.combat_intent(observation, BrainGoal::Attack, None)
            }
            Some(StrategicObjective::Defend { planet, .. }) => {
                self.combat_intent(observation, BrainGoal::Defend, Some(planet))
            }
            Some(StrategicObjective::Survive(_)) => self.survival_intent(observation),
            Some(objective) => {
                self.set_telemetry(
                    observation,
                    BrainTelemetry {
                        goal: objective.brain_goal(),
                        target: objective.target(),
                        target_planet: objective.target_planet(),
                        ..BrainTelemetry::default()
                    },
                );
                ShipIntent::default()
            }
            None => {
                self.set_telemetry(observation, BrainTelemetry::default());
                ShipIntent::default()
            }
        }
    }

    fn telemetry(&self) -> BrainTelemetry {
        self.telemetry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use engine_core::SpacewarsConfig;
    use scenario_spacewars::{ShipIntentEncoder, ShipSensorProfile, SpacewarsScenario};
    use std::time::Duration;

    const DT: Duration = Duration::from_nanos(16_666_667);

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "actual {actual}, expected {expected}"
        );
    }

    #[test]
    fn heading_error_matches_controller_turn_signs() {
        assert_close(shortest_heading_error(Vec2::Y), 0.0);
        assert_close(
            shortest_heading_error(Vec2::X),
            core::f32::consts::FRAC_PI_2,
        );
        assert_close(
            shortest_heading_error(-Vec2::X),
            -core::f32::consts::FRAC_PI_2,
        );
        assert_close(shortest_heading_error(-Vec2::Y).abs(), PI);
    }

    #[test]
    fn intercept_leads_in_the_contacts_direction_of_travel() {
        assert_eq!(
            intercept_position(Vec2::new(0.0, 300.0), Vec2::new(20.0, 0.0), 300.0, 2.0),
            Vec2::new(20.0, 300.0)
        );
    }

    #[test]
    fn arrival_guidance_accelerates_toward_a_distant_stationary_target() {
        let guidance = guide_arrival(
            Vec2::new(0.0, 300.0),
            Vec2::ZERO,
            Vec2::ZERO,
            0.0,
            ArrivalGuidanceConfig::default(),
        );

        assert_close(guidance.desired_closing_speed, 120.0);
        assert_close(guidance.heading.error_radians, 0.0);
        assert_close(guidance.thrust, 1.0);
        assert_close(guidance.brake, 0.0);
        assert!(!guidance.arrived);
    }

    #[test]
    fn arrival_guidance_brakes_when_closing_faster_than_requested() {
        let guidance = guide_arrival(
            Vec2::new(0.0, 50.0),
            Vec2::new(0.0, -100.0),
            Vec2::new(0.0, 100.0),
            0.0,
            ArrivalGuidanceConfig::default(),
        );

        assert!(guidance.closing_speed > guidance.desired_closing_speed);
        assert_close(guidance.thrust, 0.0);
        assert!(guidance.brake > 0.0);
    }

    #[test]
    fn arrival_guidance_accepts_a_position_and_velocity_match() {
        let guidance = guide_arrival(
            Vec2::new(2.0, -1.0),
            Vec2::ZERO,
            Vec2::new(40.0, 10.0),
            0.25,
            ArrivalGuidanceConfig::default(),
        );

        assert!(guidance.arrived);
        assert_close(guidance.thrust, 0.0);
        assert_close(guidance.brake, 0.0);
        assert!(guidance.heading.turn < 0.0);
    }

    #[test]
    fn aligned_distant_target_uses_thrust_and_fast_pursuit() {
        let mut config = SpacewarsConfig::deathmatch();
        config.asteroid_probability_per_sec = 0.0;
        let state = SpacewarsScenario::init(config, 11);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.opponent.as_mut().unwrap().local_position = Vec2::new(0.0, 900.0);
        observation.opponent.as_mut().unwrap().local_velocity = Vec2::ZERO;
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let intent = brain.intent(&observation);

        assert_eq!(intent.turn, 0.0);
        assert_eq!(intent.thrust, 1.0);
        assert_eq!(intent.brake, 0.0);
        assert!(intent.wings_closed);
        assert!(intent.laser);
        assert_eq!(brain.telemetry().goal, BrainGoal::Attack);
    }

    #[test]
    fn closing_inside_arrival_distance_uses_the_brake() {
        let mut config = SpacewarsConfig::deathmatch();
        config.asteroid_probability_per_sec = 0.0;
        let state = SpacewarsScenario::init(config, 13);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.opponent.as_mut().unwrap().local_position = Vec2::new(0.0, 100.0);
        observation.opponent.as_mut().unwrap().local_velocity = Vec2::new(0.0, -50.0);
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let intent = brain.intent(&observation);

        assert_eq!(intent.thrust, 0.0);
        assert_eq!(intent.brake, 1.0);
        assert!(intent.laser);
    }

    #[test]
    fn capture_strategy_yields_a_neutral_port_to_a_closer_opponent() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 13);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.hazards.clear();
        observation.planets.truncate(2);
        let yielded_planet = observation.planets[0].id;
        observation.planets[0].owner = None;
        observation.planets[0].local_position = Vec2::Y * 600.0;
        observation.planets[0].local_spaceport_position = Vec2::Y * 550.0;
        let selected_planet = observation.planets[1].id;
        observation.planets[1].owner = None;
        observation.planets[1].local_position = -Vec2::Y * 700.0;
        observation.planets[1].local_spaceport_position = -Vec2::Y * 650.0;
        observation.opponent.as_mut().unwrap().local_position = Vec2::Y * 550.0;
        let mut brain = RuleShipBrain::default();

        assert!(brain.opponent_has_neutral_port_priority(&observation, observation.planets[0],));
        assert!(!brain.opponent_has_neutral_port_priority(&observation, observation.planets[1],));

        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        let _ = brain.intent(&observation);

        assert_ne!(
            brain.telemetry().strategy.target_planet,
            Some(yielded_planet)
        );
        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Capture);
        assert_eq!(
            brain.telemetry().strategy.target_planet,
            Some(selected_planet)
        );
    }

    #[test]
    fn strategy_commits_before_switching_to_a_vulnerable_opponent() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 14);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.hazards.clear();
        let opponent = observation.opponent.as_mut().unwrap();
        opponent.local_position = Vec2::Y * observation.universe.radius;
        opponent.life_fraction = 1.0;

        let mut brain_config = RuleShipBrainConfig::default();
        brain_config.strategy.capture_weight = 0.8;
        brain_config.strategy.switch_margin = 0.0;
        let mut brain = RuleShipBrain::new(brain_config);
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let _ = brain.intent(&observation);
        let capture_target = brain.telemetry().strategy.target_planet;
        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Capture);
        assert!(capture_target.is_some());
        assert_eq!(
            brain.telemetry().strategy.selection_reason,
            Some(StrategySelectionReason::Initial)
        );

        let opponent = observation.opponent.as_mut().unwrap();
        opponent.local_position = Vec2::Y * 50.0;
        opponent.life_fraction = 0.0;
        observation.tick = brain_config.strategy.minimum_commitment_ticks - 1;
        let _ = brain.intent(&observation);

        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Capture);
        assert_eq!(brain.telemetry().strategy.target_planet, capture_target);

        observation.tick = brain_config.strategy.minimum_commitment_ticks
            + brain_config.strategy.evaluation_interval_ticks;
        let _ = brain.intent(&observation);

        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Attack);
        assert_eq!(
            brain.telemetry().strategy.selection_reason,
            Some(StrategySelectionReason::HigherUtility)
        );
        assert!(brain.telemetry().strategy.scores.attack.is_some());
        assert!(brain.telemetry().strategy.scores.capture.is_some());
    }

    #[test]
    fn damaged_ship_repairs_until_the_exit_threshold() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 16);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.hazards.clear();
        observation.own_ship.life_fraction = 0.3;
        observation.planets[0].owner = Some(PlayerId::PLAYER_2);
        let repair_planet = observation.planets[0].id;
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let _ = brain.intent(&observation);

        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Repair);
        assert_eq!(
            brain.telemetry().strategy.target_planet,
            Some(repair_planet)
        );
        assert_eq!(brain.telemetry().goal, BrainGoal::Repair);
        assert!(brain.telemetry().strategy.scores.repair.is_some());

        observation.own_ship.life_fraction = 0.7;
        observation.tick = 600;
        let _ = brain.intent(&observation);
        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Repair);
        assert_eq!(
            brain.telemetry().strategy.target_planet,
            Some(repair_planet)
        );

        observation.own_ship.life_fraction = 0.95;
        observation.tick += 1;
        let _ = brain.intent(&observation);
        assert_ne!(brain.telemetry().strategy.goal, StrategicGoal::Repair);
        assert_eq!(
            brain.telemetry().strategy.selection_reason,
            Some(StrategySelectionReason::Invalidated)
        );
    }

    #[test]
    fn active_capture_threat_selects_defense() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 18);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.hazards.clear();
        observation.planets.truncate(1);
        let opponent = observation.opponent.unwrap().id;
        let threatened = &mut observation.planets[0];
        threatened.owner = Some(PlayerId::PLAYER_2);
        threatened.capturing_player = Some(opponent);
        threatened.capture_progress = 0.4;
        threatened.local_position = Vec2::new(1_000.0, 1_000.0);
        let threatened_planet = threatened.id;
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let _ = brain.intent(&observation);

        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Defend);
        assert_eq!(brain.telemetry().strategy.target, Some(opponent));
        assert_eq!(
            brain.telemetry().strategy.target_planet,
            Some(threatened_planet)
        );
        assert_eq!(brain.telemetry().goal, BrainGoal::Defend);
        assert!(brain.telemetry().strategy.scores.defend.is_some());
    }

    #[test]
    fn defense_interrupts_a_new_capture_commitment() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 19);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.hazards.clear();
        let opponent = observation.opponent.unwrap().id;
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let _ = brain.intent(&observation);
        let capture_target = brain.telemetry().strategy.target_planet.unwrap();
        let threatened = observation
            .planets
            .iter_mut()
            .find(|planet| planet.id != capture_target)
            .unwrap();
        threatened.owner = Some(PlayerId::PLAYER_2);
        threatened.capturing_player = Some(opponent);
        threatened.capture_progress = 0.1;
        let threatened_planet = threatened.id;
        observation.tick = 1;

        let _ = brain.intent(&observation);

        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Defend);
        assert_eq!(
            brain.telemetry().strategy.target_planet,
            Some(threatened_planet)
        );
        assert_eq!(
            brain.telemetry().strategy.selection_reason,
            Some(StrategySelectionReason::Urgent)
        );
    }

    #[test]
    fn enemy_pod_does_not_distract_from_its_owned_planet() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 20);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.hazards.clear();
        for planet in &mut observation.planets {
            planet.owner = Some(PlayerId::PLAYER_2);
        }
        let opponent = observation.opponent.as_mut().unwrap();
        opponent.form = ShipForm::EscapePod;
        opponent.life_fraction = 0.0;
        opponent.local_position = Vec2::Y * 50.0;
        observation.planets[0].owner = Some(opponent.id);
        let decisive_planet = observation.planets[0].id;
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let _ = brain.intent(&observation);

        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Capture);
        assert_eq!(
            brain.telemetry().strategy.target_planet,
            Some(decisive_planet)
        );
        assert!(
            brain.telemetry().strategy.scores.capture.unwrap()
                > brain.telemetry().strategy.scores.attack.unwrap()
        );
    }

    #[test]
    fn escape_pod_selects_an_owned_spaceport_for_rebuild() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 19);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.own_ship.form = ShipForm::EscapePod;
        observation.sun = None;
        observation.hazards.clear();
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            let offset = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
            let port_axis = (planet.local_spaceport_position - planet.local_position).normalized();
            planet.local_position = offset;
            planet.local_spaceport_position = offset + port_axis * planet.radius * 0.7;
            planet.local_velocity = Vec2::ZERO;
            planet.local_spaceport_velocity = Vec2::ZERO;
        }
        observation.planets[1].owner = Some(PlayerId::PLAYER_2);
        let expected_planet = observation.planets[1].id;
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let intent = brain.intent(&observation);

        assert_eq!(brain.telemetry().goal, BrainGoal::Rebuild);
        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Rebuild);
        assert_eq!(brain.telemetry().target_planet, Some(expected_planet));
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Rendezvous)
        );
        assert!(!intent.laser);
        assert!(!intent.cannon);
    }

    #[test]
    fn pod_staging_is_position_driven_while_ship_staging_requires_velocity_match() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 21);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        let target_planet = observation.planets[0].id;
        let brain_config = RuleShipBrainConfig::default();
        let staging_radius = observation.planets[0].radius
            + observation.own_ship.collision_radius
            + brain_config.spaceport_staging_margin;
        observation.planets[0].local_position = -Vec2::Y * staging_radius;
        observation.planets[0].local_spaceport_position =
            observation.planets[0].local_position + Vec2::X * observation.planets[0].radius;
        observation.planets[0].local_velocity = Vec2::X * 100.0;
        observation.planets[0].local_spaceport_velocity = Vec2::X * 100.0;

        observation.own_ship.form = ShipForm::EscapePod;
        observation.planets[0].owner = Some(PlayerId::PLAYER_2);
        let mut pod_brain = RuleShipBrain::new(brain_config);
        pod_brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        pod_brain.port_navigation = Some(PortNavigation {
            goal: BrainGoal::Rebuild,
            planet: target_planet,
            phase: PortNavigationPhase::Rendezvous,
            departure_burn_started: false,
        });

        pod_brain.refresh_port_navigation(&observation);

        assert_eq!(
            pod_brain.port_navigation.unwrap().phase,
            PortNavigationPhase::Approach
        );

        observation.own_ship.form = ShipForm::Ship;
        observation.planets[0].owner = None;
        let mut ship_brain = RuleShipBrain::new(brain_config);
        ship_brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        ship_brain.port_navigation = Some(PortNavigation {
            goal: BrainGoal::Capture,
            planet: target_planet,
            phase: PortNavigationPhase::Rendezvous,
            departure_burn_started: false,
        });

        ship_brain.refresh_port_navigation(&observation);

        assert_eq!(
            ship_brain.port_navigation.unwrap().phase,
            PortNavigationPhase::Rendezvous
        );
    }

    #[test]
    fn escape_pod_does_not_inherit_a_destroyed_ships_departure() {
        let state = SpacewarsScenario::init(SpacewarsConfig::default(), 22);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.own_ship.form = ShipForm::EscapePod;
        observation.own_ship.docked_planet = None;
        observation.sun = None;
        observation.opponent = None;
        observation.hazards.clear();
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            let port_axis = (planet.local_spaceport_position - planet.local_position).normalized();
            planet.local_position = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
            planet.local_spaceport_position =
                planet.local_position + port_axis * planet.radius * 0.7;
            planet.local_velocity = Vec2::ZERO;
            planet.local_spaceport_velocity = Vec2::ZERO;
        }
        let rebuild_planet = observation.planets[0].id;
        observation.planets[0].owner = Some(PlayerId::PLAYER_2);

        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        brain.port_navigation = Some(PortNavigation {
            goal: BrainGoal::Capture,
            planet: rebuild_planet,
            phase: PortNavigationPhase::Depart,
            departure_burn_started: true,
        });

        let intent = brain.intent(&observation);

        assert_eq!(brain.telemetry().goal, BrainGoal::Rebuild);
        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Rebuild);
        assert_eq!(brain.telemetry().target_planet, Some(rebuild_planet));
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Rendezvous)
        );
        assert!(!intent.laser);
        assert!(!intent.cannon);
    }

    #[test]
    fn escape_pod_steers_around_unowned_planet_without_braking() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 23);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.own_ship.form = ShipForm::EscapePod;
        observation.sun = None;
        observation.hazards.clear();
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            let offset = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
            let port_axis = (planet.local_spaceport_position - planet.local_position).normalized();
            planet.local_position = offset;
            planet.local_spaceport_position = offset + port_axis * planet.radius * 0.7;
            planet.local_velocity = Vec2::ZERO;
            planet.local_spaceport_velocity = Vec2::ZERO;
        }
        observation.planets[1].owner = Some(PlayerId::PLAYER_2);
        let rebuild_planet = observation.planets[1].id;
        let avoided_planet = observation.planets[0].id;
        observation.planets[0].radius = 40.0;
        observation.planets[0].local_position = Vec2::new(100.0, 0.0);
        observation.planets[0].local_velocity = Vec2::new(-10.0, 0.0);
        let expected_surface_clearance = 100.0 - 40.0 - observation.own_ship.collision_radius;
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let intent = brain.intent(&observation);

        assert_eq!(brain.telemetry().goal, BrainGoal::AvoidBody);
        assert_eq!(brain.telemetry().target_planet, Some(rebuild_planet));
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Rendezvous)
        );
        assert_eq!(
            brain.telemetry().avoided_body,
            Some(AvoidanceBody::Planet(avoided_planet))
        );
        assert_close(
            brain.telemetry().avoidance_surface_clearance.unwrap(),
            expected_surface_clearance,
        );
        assert!(intent.turn < 0.0);
        assert_eq!(intent.brake, 0.0);
    }

    #[test]
    fn escape_pod_physically_clears_an_unowned_planet_while_seeking_rebuild() {
        let config = SpacewarsConfig {
            universe_radius: 10_000,
            asteroid_probability_per_sec: 0.0,
            use_starfield: false,
            use_sounds: false,
            ..SpacewarsConfig::default()
        };
        let mut state = SpacewarsScenario::init(config, 25);
        state.sun = None;
        for (index, planet) in state.planets.iter_mut().enumerate() {
            planet.position = Vec2::new(2_000.0 + index as f32 * 250.0, 2_000.0);
            planet.owner_id = None;
        }
        state.planets[0].position = Vec2::ZERO;
        state.planets[1].position = Vec2::new(-2_000.0, -1_500.0);
        state.planets[1].owner_id = Some(PlayerId::PLAYER_2.index());
        state.players[1].planet_count = 1;
        state.ships[0].position = Vec2::new(-3_000.0, 3_000.0);
        state.ships[0].velocity = Vec2::ZERO;
        state.ships[1].life = 0.0;
        state.ships[1].dead = true;
        SpacewarsScenario::step(&mut state, &[], DT);
        assert_eq!(state.ships[1].form, ShipForm::EscapePod);
        state.debris.clear();

        let pod_radius = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap()
        .own_ship
        .collision_radius;
        let obstacle_radius = state.planets[0].radius;
        let initial_surface_clearance = 45.0;
        state.ships[1].position =
            Vec2::X * (obstacle_radius + pod_radius + initial_surface_clearance);
        state.ships[1].velocity = Vec2::new(-15.0, 0.0);
        state.ships[1].rotation_radians = 0.0;
        state.ships[1].direction = Vec2::Y;
        state.ships[1].omega = 0.0;

        let obstacle = PlanetId::from_index(0).unwrap();
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        let mut encoder = ShipIntentEncoder::default();
        let mut maximum_surface_clearance = initial_surface_clearance;

        for tick in 0..600 {
            let observation = SpacewarsScenario::observe_ship(
                &state,
                PlayerId::PLAYER_2,
                ShipSensorProfile::FullMapRadar,
            )
            .unwrap();
            let intent = brain.intent(&observation);
            if tick == 0 {
                assert_eq!(brain.telemetry().goal, BrainGoal::AvoidBody);
                assert_eq!(
                    brain.telemetry().avoided_body,
                    Some(AvoidanceBody::Planet(obstacle))
                );
                assert!(intent.turn.abs() > 0.0);
                assert_eq!(intent.brake, 0.0);
            }
            let actions = encoder.encode(PlayerId::PLAYER_2.index(), intent);
            SpacewarsScenario::step(&mut state, &actions, DT);
            let surface_clearance = state.ships[1]
                .position
                .distance_to(state.planets[0].position)
                - obstacle_radius
                - pod_radius;
            maximum_surface_clearance = maximum_surface_clearance.max(surface_clearance);
            if surface_clearance >= brain.config.body_clearance + 20.0 {
                break;
            }
        }

        assert!(
            maximum_surface_clearance >= brain.config.body_clearance + 20.0,
            "pod did not clear the unowned planet; maximum surface clearance {maximum_surface_clearance:.3}, telemetry {:?}, position {:?}, velocity {:?}",
            brain.telemetry(),
            state.ships[1].position,
            state.ships[1].velocity,
        );
    }

    #[test]
    fn stalled_non_target_planet_avoidance_engages_escape_assist() {
        let state = SpacewarsScenario::init(SpacewarsConfig::default(), 0);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.opponent = None;
        observation.hazards.clear();
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            planet.local_position = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
            planet.local_velocity = Vec2::ZERO;
            planet.local_spaceport_position = planet.local_position;
            planet.local_spaceport_velocity = Vec2::ZERO;
        }

        let obstacle = observation.planets[0].id;
        observation.planets[0].owner = Some(PlayerId::PLAYER_2);
        observation.planets[0].local_position = Vec2::X
            * (observation.planets[0].radius + observation.own_ship.collision_radius + 25.0);
        observation.planets[0].local_velocity = -Vec2::X * 5.0;

        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let mut final_intent = ShipIntent::default();
        for tick in 0..=brain.config.body_avoidance_stall_ticks {
            observation.tick = tick;
            final_intent = brain.intent(&observation);
            assert_eq!(brain.telemetry().goal, BrainGoal::AvoidBody);
            assert_eq!(
                brain.telemetry().avoided_body,
                Some(AvoidanceBody::Planet(obstacle))
            );
            assert_ne!(brain.telemetry().target_planet, Some(obstacle));
        }

        let telemetry = brain.telemetry();
        assert_eq!(
            telemetry.avoidance_age_ticks,
            brain.config.body_avoidance_stall_ticks
        );
        assert_eq!(
            telemetry.avoidance_stalled_ticks,
            brain.config.body_avoidance_stall_ticks
        );
        assert!(telemetry.avoidance_escape_assist);
        assert!(!telemetry.avoidance_emergency_escape_assist);
        assert_close(telemetry.avoidance_surface_clearance.unwrap(), 25.0);
        assert_close(telemetry.avoidance_outward_speed.unwrap(), -5.0);
        assert!(final_intent.brake > 0.0 && final_intent.brake < 1.0);
        assert!(final_intent.turn.abs() > 0.0);
    }

    #[test]
    fn captured_interactive_planet_trap_uses_rear_aligned_reverse_escape() {
        let state = SpacewarsScenario::init(SpacewarsConfig::default(), 0);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.opponent = None;
        observation.hazards.clear();
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            planet.local_position = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
            planet.local_velocity = Vec2::ZERO;
            planet.local_spaceport_position = planet.local_position;
            planet.local_spaceport_velocity = Vec2::ZERO;
        }

        let obstacle = observation.planets[0].id;
        let repair_target = observation.planets[1].id;
        observation.planets[0].owner = Some(PlayerId::PLAYER_1);
        observation.planets[1].owner = Some(PlayerId::PLAYER_2);
        let captured_heading_error = -2.55_f32;
        let desired_outward = Vec2::new(captured_heading_error.sin(), captured_heading_error.cos());
        let obstacle_direction = -desired_outward;
        observation.planets[0].local_position = obstacle_direction
            * (observation.planets[0].radius + observation.own_ship.collision_radius - 3.096);
        observation.planets[0].local_velocity = obstacle_direction * -0.195;
        observation.own_ship.angular_velocity = 0.441;
        observation.own_ship.life_fraction = 0.323;

        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let mut intent = ShipIntent::default();
        for tick in 0..=158 {
            observation.tick = tick;
            intent = brain.intent(&observation);
            if tick == 0 {
                assert!(!brain.telemetry().avoidance_escape_assist);
                assert_eq!(intent.brake, 1.0);
            }
        }
        let telemetry = brain.telemetry();
        assert_eq!(telemetry.goal, BrainGoal::AvoidBody);
        assert_eq!(telemetry.strategy.goal, StrategicGoal::Repair);
        assert_eq!(telemetry.target_planet, Some(repair_target));
        assert_eq!(
            telemetry.avoided_body,
            Some(AvoidanceBody::Planet(obstacle))
        );
        assert_eq!(telemetry.avoidance_age_ticks, 158);
        assert_eq!(telemetry.avoidance_stalled_ticks, 158);
        assert_close(telemetry.avoidance_surface_clearance.unwrap(), -3.096);
        assert_close(telemetry.avoidance_outward_speed.unwrap(), -0.195);
        assert_close(telemetry.heading_error, captured_heading_error);
        assert!(telemetry.avoidance_escape_assist);
        assert!(telemetry.avoidance_emergency_escape_assist);
        assert_eq!(intent.turn, -1.0);
        assert_eq!(intent.thrust, -1.0);
        assert_eq!(intent.brake, 0.0);
    }

    #[test]
    fn active_repair_target_does_not_suppress_emergency_escape() {
        let state = SpacewarsScenario::init(SpacewarsConfig::default(), 0);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.opponent = None;
        observation.hazards.clear();
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            planet.local_position = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
            planet.local_velocity = Vec2::ZERO;
            planet.local_spaceport_position = planet.local_position + Vec2::Y * planet.radius;
            planet.local_spaceport_velocity = Vec2::ZERO;
        }

        let repair_target = observation.planets[0].id;
        observation.planets[0].owner = Some(PlayerId::PLAYER_2);
        let captured_heading_error = -2.55_f32;
        let desired_outward = Vec2::new(captured_heading_error.sin(), captured_heading_error.cos());
        let target_direction = -desired_outward;
        observation.planets[0].local_position = target_direction
            * (observation.planets[0].radius + observation.own_ship.collision_radius - 3.096);
        observation.planets[0].local_velocity = target_direction * -0.195;
        observation.planets[0].local_spaceport_position =
            observation.planets[0].local_position + Vec2::Y * observation.planets[0].radius;
        observation.own_ship.angular_velocity = 0.441;
        observation.own_ship.life_fraction = 0.323;

        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let mut intent = ShipIntent::default();
        for tick in 0..=brain.config.body_avoidance_emergency_stall_ticks {
            observation.tick = tick;
            intent = brain.intent(&observation);
        }

        let telemetry = brain.telemetry();
        assert_eq!(telemetry.goal, BrainGoal::AvoidBody);
        assert_eq!(telemetry.strategy.goal, StrategicGoal::Repair);
        assert_eq!(telemetry.target_planet, Some(repair_target));
        assert_eq!(
            telemetry.avoided_body,
            Some(AvoidanceBody::Planet(repair_target))
        );
        assert_eq!(
            telemetry.avoidance_stalled_ticks,
            brain.config.body_avoidance_emergency_stall_ticks
        );
        assert!(telemetry.avoidance_escape_assist);
        assert!(telemetry.avoidance_emergency_escape_assist);
        assert_eq!(intent.turn, -1.0);
        assert_eq!(intent.thrust, -1.0);
        assert_eq!(intent.brake, 0.0);
    }

    #[test]
    fn captured_interactive_reverse_escape_physically_clears_planet() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            use_starfield: false,
            use_sounds: false,
            ..SpacewarsConfig::default()
        };
        let mut state = SpacewarsScenario::init(config, 0);
        let sun = state.sun.as_mut().unwrap();
        sun.radius = 1.0;
        sun.mass = 0.0;
        state.debris.clear();
        for planet in &mut state.planets {
            planet.owner_id = None;
        }
        state.planets[0].owner_id = Some(PlayerId::PLAYER_1.index());
        state.planets[1].owner_id = Some(PlayerId::PLAYER_2.index());
        state.players[0].planet_count = 1;
        state.players[1].planet_count = 1;
        state.ships[0].position = Vec2::ZERO;

        let ship_radius = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap()
        .own_ship
        .collision_radius;
        let obstacle_radius = state.planets[0].radius;
        let initial_surface_clearance = -3.096;
        // Place the ship opposite the port so this is a body collision, not a
        // valid docking contact, while preserving the captured rear alignment.
        let outward = Vec2::from_radians(state.planets[0].wrapper_angle + PI);
        state.ships[1].position = state.planets[0].position
            + outward * (obstacle_radius + ship_radius + initial_surface_clearance);
        let sun_position = state.sun.unwrap().position;
        let orbit_velocity = (state.planets[0].position - sun_position)
            .rotate_radians(core::f32::consts::FRAC_PI_2)
            * state.planets[0].orbit_omega;
        state.ships[1].velocity = orbit_velocity - outward * 0.195
            + outward.rotate_radians(core::f32::consts::FRAC_PI_2) * 24.0;
        let forward = outward.rotate_radians(-2.55);
        state.ships[1].rotation_radians = forward.y.atan2(forward.x) - core::f32::consts::FRAC_PI_2;
        state.ships[1].direction = forward;
        state.ships[1].omega = 0.441;
        state.ships[1].life = state.ships[1].life_max * 0.323;

        let obstacle = PlanetId::from_index(0).unwrap();
        let repair_target = PlanetId::from_index(1).unwrap();
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        // Prime the persistent stall state with the captured 158-tick trap.
        let mut intent = ShipIntent::default();
        for tick in 0..=158 {
            state.tick = tick;
            let observation = SpacewarsScenario::observe_ship(
                &state,
                PlayerId::PLAYER_2,
                ShipSensorProfile::FullMapRadar,
            )
            .unwrap();
            intent = brain.intent(&observation);
        }
        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Repair);
        assert_eq!(brain.telemetry().target_planet, Some(repair_target));
        assert_eq!(
            brain.telemetry().avoided_body,
            Some(AvoidanceBody::Planet(obstacle))
        );
        assert!(brain.telemetry().avoidance_emergency_escape_assist);
        assert_eq!(intent.thrust, -1.0);
        assert_eq!(intent.brake, 0.0);

        let mut legacy_state = state.clone();
        let mut legacy_brain = brain.clone();
        let mut legacy_encoder = ShipIntentEncoder::default();
        let mut legacy_intent = intent;
        let mut legacy_maximum_surface_clearance = initial_surface_clearance;
        for _ in 0..600 {
            if legacy_intent.thrust < 0.0 {
                legacy_intent.thrust = 0.0;
                legacy_intent.brake = legacy_brain.config.body_avoidance_turn_brake;
            }
            let actions = legacy_encoder.encode(PlayerId::PLAYER_2.index(), legacy_intent);
            SpacewarsScenario::step(&mut legacy_state, &actions, DT);
            let surface_clearance = legacy_state.ships[1]
                .position
                .distance_to(legacy_state.planets[0].position)
                - obstacle_radius
                - ship_radius;
            legacy_maximum_surface_clearance =
                legacy_maximum_surface_clearance.max(surface_clearance);
            if surface_clearance >= legacy_brain.config.body_clearance
                || legacy_state.ships[1].form != ShipForm::Ship
            {
                break;
            }
            let observation = SpacewarsScenario::observe_ship(
                &legacy_state,
                PlayerId::PLAYER_2,
                ShipSensorProfile::FullMapRadar,
            )
            .unwrap();
            legacy_intent = legacy_brain.intent(&observation);
        }
        assert!(
            legacy_maximum_surface_clearance < legacy_brain.config.body_clearance,
            "legacy maneuver unexpectedly cleared the captured trap"
        );
        assert_eq!(legacy_state.ships[1].form, ShipForm::EscapePod);

        let mut encoder = ShipIntentEncoder::default();
        let mut maximum_surface_clearance = initial_surface_clearance;
        for _ in 0..600 {
            let actions = encoder.encode(PlayerId::PLAYER_2.index(), intent);
            SpacewarsScenario::step(&mut state, &actions, DT);
            let surface_clearance = state.ships[1]
                .position
                .distance_to(state.planets[0].position)
                - obstacle_radius
                - ship_radius;
            maximum_surface_clearance = maximum_surface_clearance.max(surface_clearance);
            if surface_clearance >= brain.config.body_clearance {
                break;
            }
            let observation = SpacewarsScenario::observe_ship(
                &state,
                PlayerId::PLAYER_2,
                ShipSensorProfile::FullMapRadar,
            )
            .unwrap();
            intent = brain.intent(&observation);
        }

        assert!(
            maximum_surface_clearance >= brain.config.body_clearance,
            "captured trap did not clear the planet; maximum clearance {maximum_surface_clearance:.3}, telemetry {:?}, position {:?}, velocity {:?}, life {:.3}",
            brain.telemetry(),
            state.ships[1].position,
            state.ships[1].velocity,
            state.ships[1].life,
        );
        assert_eq!(state.ships[1].form, ShipForm::Ship);
        assert!(state.ships[1].life > 0.0);
    }

    #[test]
    fn full_ship_physically_clears_a_planet_while_turning_to_avoid_it() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            use_starfield: false,
            use_sounds: false,
            ..SpacewarsConfig::default()
        };
        let mut state = SpacewarsScenario::init(config, 0);
        let sun = state.sun.as_mut().unwrap();
        sun.radius = 1.0;
        sun.mass = 0.0;
        state.debris.clear();
        for planet in &mut state.planets {
            planet.owner_id = None;
        }
        state.planets[0].owner_id = Some(PlayerId::PLAYER_2.index());
        state.players[1].planet_count = 1;
        state.ships[0].position = Vec2::ZERO;

        let ship_radius = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap()
        .own_ship
        .collision_radius;
        let obstacle_radius = state.planets[0].radius;
        let initial_surface_clearance = 25.0;
        let outward = Vec2::X;
        state.ships[1].position = state.planets[0].position
            + outward * (obstacle_radius + ship_radius + initial_surface_clearance);
        let sun_position = state.sun.unwrap().position;
        let orbit_velocity = (state.planets[0].position - sun_position)
            .rotate_radians(core::f32::consts::FRAC_PI_2)
            * state.planets[0].orbit_omega;
        state.ships[1].velocity = orbit_velocity + Vec2::Y * 20.0;
        let forward = outward.rotate_radians(2.2);
        state.ships[1].rotation_radians = forward.y.atan2(forward.x) - core::f32::consts::FRAC_PI_2;
        state.ships[1].direction = forward;
        state.ships[1].omega = 0.0;

        let obstacle = PlanetId::from_index(0).unwrap();
        let mut brain = RuleShipBrain::new(RuleShipBrainConfig {
            body_avoidance_stall_ticks: 120,
            body_avoidance_stall_clearance: 100.0,
            ..RuleShipBrainConfig::default()
        });
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        let mut encoder = ShipIntentEncoder::default();
        let mut maximum_surface_clearance = initial_surface_clearance;

        for tick in 0..600 {
            let observation = SpacewarsScenario::observe_ship(
                &state,
                PlayerId::PLAYER_2,
                ShipSensorProfile::FullMapRadar,
            )
            .unwrap();
            let intent = brain.intent(&observation);
            if tick == 0 {
                assert_eq!(brain.telemetry().goal, BrainGoal::AvoidBody);
                assert_eq!(
                    brain.telemetry().avoided_body,
                    Some(AvoidanceBody::Planet(obstacle))
                );
                assert!(intent.turn.abs() > 0.0);
                assert_eq!(intent.brake, 1.0);
            }
            let actions = encoder.encode(PlayerId::PLAYER_2.index(), intent);
            SpacewarsScenario::step(&mut state, &actions, DT);
            let surface_clearance = state.ships[1]
                .position
                .distance_to(state.planets[0].position)
                - obstacle_radius
                - ship_radius;
            maximum_surface_clearance = maximum_surface_clearance.max(surface_clearance);
            if surface_clearance >= brain.config.body_clearance {
                break;
            }
        }

        assert!(
            maximum_surface_clearance >= brain.config.body_clearance,
            "ship did not clear the owned planet; maximum surface clearance {maximum_surface_clearance:.3}, telemetry {:?}, position {:?}, velocity {:?}, life {:.3}",
            brain.telemetry(),
            state.ships[1].position,
            state.ships[1].velocity,
            state.ships[1].life,
        );
        assert!(state.ships[1].life / state.ships[1].life_max >= 0.95);
    }

    fn assert_rule_brain_pod_rebuild_cycle(seed: u64) {
        let config = SpacewarsConfig {
            universe_radius: 5_000,
            asteroid_probability_per_sec: 0.0,
            use_starfield: false,
            use_sounds: false,
            ..SpacewarsConfig::default()
        };
        let mut state = SpacewarsScenario::init(config, seed);
        let target_planet_index = 9;
        assert!(state.planets.len() > target_planet_index);
        for planet in &mut state.planets {
            planet.owner_id = None;
        }
        // Keep the unrelated opponent eligible to rebuild so its accidental
        // death cannot end and freeze this isolated pod-recovery episode.
        state.planets[0].owner_id = Some(PlayerId::PLAYER_1.index());
        state.players[0].planet_count = 1;
        state.planets[target_planet_index].owner_id = Some(PlayerId::PLAYER_2.index());
        state.players[1].planet_count = 1;
        state.ships[1].life = 0.0;
        state.ships[1].dead = true;
        SpacewarsScenario::step(&mut state, &[], DT);
        assert_eq!(state.ships[1].form, ShipForm::EscapePod);
        state.debris.clear();

        let pod_radius = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap()
        .own_ship
        .collision_radius;
        let target_planet_state = state.planets[target_planet_index];
        state.ships[1].position = target_planet_state.position
            + Vec2::X * (target_planet_state.radius + pod_radius + 60.0);
        state.ships[1].velocity = Vec2::Y * 150.0;
        state.ships[1].rotation_radians = 0.0;
        state.ships[1].direction = Vec2::Y;
        state.ships[1].omega = 0.0;

        let target_planet = PlanetId::from_index(target_planet_index).unwrap();
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        let mut encoder = ShipIntentEncoder::default();
        let mut saw_approach = false;
        let mut saw_docked = false;
        let mut saw_rebuilt = false;
        let mut saw_depart = false;
        let mut saw_safe_departure = false;
        let mut first_docked_tick = None;
        let mut first_rebuilt_tick = None;
        let mut minimum_target_distance = f32::INFINITY;
        let mut docking_entries = 0_u64;
        let mut continuous_docked_ticks = 0_u64;
        let mut maximum_docked_ticks = 0_u64;
        let mut was_docked = false;

        for _ in 0..7_200 {
            let observation = SpacewarsScenario::observe_ship(
                &state,
                PlayerId::PLAYER_2,
                ShipSensorProfile::FullMapRadar,
            )
            .unwrap();
            let intent = brain.intent(&observation);
            let telemetry = brain.telemetry();
            minimum_target_distance = minimum_target_distance.min(telemetry.target_distance);
            saw_approach |= telemetry.port_phase == Some(PortNavigationPhase::Approach);
            let docked = observation.own_ship.docked_planet == Some(target_planet);
            if docked {
                saw_docked = true;
                first_docked_tick.get_or_insert(state.tick);
                if !was_docked {
                    docking_entries += 1;
                }
                continuous_docked_ticks += 1;
                maximum_docked_ticks = maximum_docked_ticks.max(continuous_docked_ticks);
            } else {
                continuous_docked_ticks = 0;
            }
            was_docked = docked;
            if observation.own_ship.form == ShipForm::Ship {
                saw_rebuilt = true;
                first_rebuilt_tick.get_or_insert(state.tick);
            }
            saw_depart |= saw_rebuilt
                && telemetry.target_planet == Some(target_planet)
                && telemetry.port_phase == Some(PortNavigationPhase::Depart);
            if saw_depart && observation.own_ship.docked_planet != Some(target_planet) {
                let target = observation
                    .planets
                    .iter()
                    .find(|planet| planet.id == target_planet)
                    .unwrap();
                let surface_clearance = target.local_position.length()
                    - target.radius
                    - observation.own_ship.collision_radius;
                saw_safe_departure = surface_clearance >= brain.config.body_clearance;
            }
            if saw_safe_departure {
                break;
            }

            let actions = encoder.encode(PlayerId::PLAYER_2.index(), intent);
            SpacewarsScenario::step(&mut state, &actions, DT);
        }

        assert!(saw_approach, "seed {seed} never reached port approach");
        assert!(
            saw_docked,
            "seed {seed} pod never docked; minimum target distance {minimum_target_distance:.3}, telemetry {:?}, position {:?}, velocity {:?}",
            brain.telemetry(),
            state.ships[1].position,
            state.ships[1].velocity,
        );
        assert!(
            saw_rebuilt,
            "seed {seed} docked pod never rebuilt; entries {docking_entries}, longest contact {maximum_docked_ticks} ticks, telemetry {:?}, form {:?}, docked {:?}, planet {:?}, position {:?}, velocity {:?}",
            brain.telemetry(),
            state.ships[1].form,
            SpacewarsScenario::observe_ship(
                &state,
                PlayerId::PLAYER_2,
                ShipSensorProfile::FullMapRadar,
            )
            .unwrap()
            .own_ship
            .docked_planet,
            state.planets[target_planet_index],
            state.ships[1].position,
            state.ships[1].velocity,
        );
        assert!(
            first_rebuilt_tick.unwrap() - first_docked_tick.unwrap() >= 480,
            "seed {seed} rebuild bypassed the eight-second dock timer"
        );
        assert!(saw_depart, "seed {seed} rebuilt ship never began departure");
        assert!(
            saw_safe_departure,
            "seed {seed} rebuilt ship never safely departed"
        );
    }

    #[test]
    fn rule_brain_pod_docks_rebuilds_and_safely_departs() {
        for seed in [0, 1, 2, 3, 4, 5] {
            assert_rule_brain_pod_rebuild_cycle(seed);
        }
    }

    #[test]
    fn escape_pod_rendezvous_turn_does_not_apply_the_brake() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 27);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.own_ship.form = ShipForm::EscapePod;
        observation.own_ship.local_velocity = Vec2::ZERO;
        observation.own_ship.angular_velocity = 0.0;
        observation.sun = None;
        observation.hazards.clear();
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            let offset = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
            let port_axis = (planet.local_spaceport_position - planet.local_position).normalized();
            planet.local_position = offset;
            planet.local_spaceport_position = offset + port_axis * planet.radius * 0.7;
            planet.local_velocity = Vec2::ZERO;
            planet.local_spaceport_velocity = Vec2::ZERO;
        }
        let rebuild_planet = observation.planets[1].id;
        observation.planets[1].owner = Some(PlayerId::PLAYER_2);
        observation.planets[1].local_position = Vec2::new(0.0, -2_000.0);
        observation.planets[1].local_spaceport_position = Vec2::new(0.0, -1_950.0);
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let intent = brain.intent(&observation);

        assert_eq!(brain.telemetry().goal, BrainGoal::Rebuild);
        assert_eq!(brain.telemetry().target_planet, Some(rebuild_planet));
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Rendezvous)
        );
        assert!(intent.turn.abs() > 0.0);
        assert_eq!(intent.brake, 0.0);
    }

    #[test]
    fn escape_pod_survival_turn_does_not_apply_the_brake() {
        let mut config = SpacewarsConfig::deathmatch();
        config.asteroid_probability_per_sec = 0.0;
        let state = SpacewarsScenario::init(config, 29);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.own_ship.form = ShipForm::EscapePod;
        observation.own_ship.angular_velocity = 0.0;
        observation.sun = None;
        observation.hazards.clear();
        observation.opponent.as_mut().unwrap().local_position = Vec2::new(-100.0, 0.0);
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let intent = brain.intent(&observation);

        assert_eq!(brain.telemetry().goal, BrainGoal::Survive);
        assert!(intent.turn > 0.0);
        assert_eq!(intent.brake, 0.0);
    }

    #[test]
    fn unexpected_docking_contact_overrides_the_planned_planet() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 0);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        let planned_planet = observation.planets[0].id;
        let docked_planet = observation.planets[1].id;
        observation.own_ship.docked_planet = Some(docked_planet);
        observation.planets[1].owner = Some(PlayerId::PLAYER_2);
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        brain.port_navigation = Some(PortNavigation {
            goal: BrainGoal::Capture,
            planet: planned_planet,
            phase: PortNavigationPhase::Rendezvous,
            departure_burn_started: false,
        });

        let depart_intent = brain.intent(&observation);

        assert_eq!(brain.telemetry().target_planet, Some(docked_planet));
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Depart)
        );
        assert_eq!(depart_intent.brake, 0.0);

        observation.planets[1].owner = None;
        let mut capture_brain = RuleShipBrain::default();
        capture_brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        capture_brain.port_navigation = Some(PortNavigation {
            goal: BrainGoal::Capture,
            planet: planned_planet,
            phase: PortNavigationPhase::Rendezvous,
            departure_burn_started: false,
        });

        let intent = capture_brain.intent(&observation);

        assert_eq!(capture_brain.telemetry().target_planet, Some(docked_planet));
        assert_eq!(
            capture_brain.telemetry().port_phase,
            Some(PortNavigationPhase::Docked)
        );
        assert_eq!(intent.brake, 1.0);
    }

    #[test]
    fn incomplete_pod_rebuild_reacquires_port_after_contact_loss() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 33);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.own_ship.form = ShipForm::EscapePod;
        observation.sun = None;
        observation.hazards.clear();
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            planet.local_position = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
        }
        observation.planets[0].owner = Some(PlayerId::PLAYER_2);
        let rebuild_planet = observation.planets[0].id;
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let _ = brain.intent(&observation);
        observation.own_ship.docked_planet = Some(rebuild_planet);
        let _ = brain.intent(&observation);
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Docked)
        );

        observation.own_ship.docked_planet = None;
        let _ = brain.intent(&observation);

        assert_eq!(brain.telemetry().goal, BrainGoal::Rebuild);
        assert_eq!(brain.telemetry().target_planet, Some(rebuild_planet));
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Ingress)
        );
    }

    #[test]
    fn incomplete_ship_repair_reacquires_port_after_contact_loss() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 34);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.own_ship.life_fraction = 0.4;
        observation.sun = None;
        observation.hazards.clear();
        observation.opponent = None;
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            planet.local_position = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
        }
        observation.planets[0].owner = Some(PlayerId::PLAYER_2);
        let repair_planet = observation.planets[0].id;
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let _ = brain.intent(&observation);
        observation.own_ship.docked_planet = Some(repair_planet);
        let _ = brain.intent(&observation);
        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Repair);
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Docked)
        );

        observation.own_ship.docked_planet = None;
        let intent = brain.intent(&observation);

        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Docked)
        );
        assert_eq!(intent.brake, 1.0);
        observation.tick += brain.config.spaceport_contact_loss_grace_ticks;
        let _ = brain.intent(&observation);

        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Repair);
        assert_eq!(brain.telemetry().target_planet, Some(repair_planet));
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Ingress)
        );
    }

    #[test]
    fn incomplete_planet_capture_reacquires_port_after_contact_loss() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };
        let state = SpacewarsScenario::init(config, 35);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.hazards.clear();
        observation.opponent = None;
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            planet.local_position = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
        }
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });

        let _ = brain.intent(&observation);
        let capture_planet = brain.telemetry().target_planet.unwrap();
        observation.own_ship.docked_planet = Some(capture_planet);
        let _ = brain.intent(&observation);
        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Capture);
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Docked)
        );

        observation.own_ship.docked_planet = None;
        let intent = brain.intent(&observation);

        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Docked)
        );
        assert_eq!(intent.brake, 1.0);
        observation.tick += brain.config.spaceport_contact_loss_grace_ticks;
        let _ = brain.intent(&observation);

        assert_eq!(brain.telemetry().strategy.goal, StrategicGoal::Capture);
        assert_eq!(brain.telemetry().target_planet, Some(capture_planet));
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Ingress)
        );
    }

    #[test]
    fn predicted_departure_reentry_steers_tangent_before_the_clearance_boundary() {
        let state = SpacewarsScenario::init(SpacewarsConfig::default(), 35);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.opponent = None;
        observation.hazards.clear();
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            planet.local_position = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
            planet.local_velocity = Vec2::ZERO;
            planet.local_spaceport_position = planet.local_position + Vec2::Y * planet.radius;
            planet.local_spaceport_velocity = Vec2::ZERO;
        }

        // Model a just-completed departure whose momentum is carrying it back
        // toward the owned origin while strategy has moved on to a neutral
        // planet. The hull is still far outside reactive body clearance.
        let origin = observation.planets[0].id;
        observation.planets[0].owner = Some(PlayerId::PLAYER_2);
        observation.planets[0].radius = 100.0;
        observation.planets[0].local_position = Vec2::new(0.0, 500.0);
        observation.planets[0].local_velocity = Vec2::new(0.0, -200.0);
        observation.planets[0].local_spaceport_position = Vec2::new(100.0, 500.0);
        let current_clearance = observation.planets[0].local_position.length()
            - observation.planets[0].radius
            - observation.own_ship.collision_radius;

        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        brain.recent_departure = Some(RecentDeparture {
            planet: origin,
            cleared_tick: observation.tick,
        });
        let intent = brain.intent(&observation);
        let telemetry = brain.telemetry();

        assert!(current_clearance > brain.config.body_clearance);
        assert_eq!(telemetry.goal, BrainGoal::AvoidBody);
        assert_eq!(telemetry.strategy.goal, StrategicGoal::Capture);
        assert_ne!(telemetry.strategy.target_planet, Some(origin));
        assert_eq!(telemetry.avoided_body, Some(AvoidanceBody::Planet(origin)));
        assert!(telemetry.avoidance_predictive);
        assert!(telemetry.avoidance_seconds_until_closest.unwrap() > 0.0);
        assert!(
            telemetry.avoidance_predicted_surface_clearance.unwrap() < brain.config.body_clearance
        );
        assert!(intent.turn.abs() > 0.0);
        assert_eq!(intent.thrust, 0.0);
        assert_close(intent.brake, brain.config.body_avoidance_turn_brake);

        let avoidance = brain
            .body_avoidance(&observation, brain.port_navigation)
            .unwrap();
        let ray = avoidance.trajectory_direction.normalized();
        let closest_distance_along_ray = observation.planets[0].local_position.dot(ray).max(0.0);
        let tangent_separation =
            (observation.planets[0].local_position - ray * closest_distance_along_ray).length();
        let required_separation = observation.planets[0].radius
            + observation.own_ship.collision_radius
            + brain.config.body_clearance;
        assert!(tangent_separation > required_separation);
    }

    #[test]
    fn departure_corridor_remains_authoritative_while_other_bodies_overlap() {
        let state = SpacewarsScenario::init(SpacewarsConfig::default(), 36);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.opponent = None;
        observation.hazards.clear();
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = Some(PlayerId::PLAYER_2);
            planet.local_position = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
            planet.local_spaceport_position = planet.local_position + Vec2::Y * planet.radius;
            planet.local_velocity = Vec2::ZERO;
            planet.local_spaceport_velocity = Vec2::ZERO;
        }

        let launch_planet = observation.planets[0].id;
        observation.planets[0].radius = 60.0;
        observation.planets[0].local_position = Vec2::new(0.0, 70.0);
        observation.planets[0].local_spaceport_position = Vec2::new(60.0, 70.0);
        observation.planets[1].radius = 60.0;
        observation.planets[1].local_position = Vec2::new(90.0, 0.0);
        observation.planets[1].local_spaceport_position = Vec2::new(90.0, 60.0);

        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        brain.port_navigation = Some(PortNavigation {
            goal: BrainGoal::Capture,
            planet: launch_planet,
            phase: PortNavigationPhase::Depart,
            departure_burn_started: true,
        });

        let intent = brain.intent(&observation);
        let telemetry = brain.telemetry();

        assert_eq!(telemetry.goal, BrainGoal::Capture);
        assert_eq!(telemetry.target_planet, Some(launch_planet));
        assert_eq!(telemetry.port_phase, Some(PortNavigationPhase::Depart));
        assert_eq!(telemetry.avoided_body, None);
        assert_eq!(intent.thrust, 1.0);
        assert!(intent.wings_closed);

        // The same ownership applies while the launch hold is established.
        // Allowing a second body's controller to repeatedly cancel this
        // maneuver produced a deterministic 24,856-tick departure deadlock.
        observation.own_ship.docked_planet = Some(launch_planet);
        let mut docked_brain = RuleShipBrain::default();
        docked_brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        docked_brain.port_navigation = Some(PortNavigation {
            goal: BrainGoal::Capture,
            planet: launch_planet,
            phase: PortNavigationPhase::Depart,
            departure_burn_started: false,
        });

        let _ = docked_brain.intent(&observation);

        assert_eq!(docked_brain.telemetry().goal, BrainGoal::Capture);
        assert_eq!(
            docked_brain.telemetry().port_phase,
            Some(PortNavigationPhase::Depart)
        );
        assert_eq!(docked_brain.telemetry().avoided_body, None);
    }

    #[test]
    fn prediction_does_not_interrupt_owned_target_rendezvous() {
        let state = SpacewarsScenario::init(SpacewarsConfig::default(), 36);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.opponent = None;
        observation.hazards.clear();
        observation.own_ship.life_fraction = 0.4;
        for (index, planet) in observation.planets.iter_mut().enumerate() {
            planet.owner = None;
            planet.local_position = Vec2::new(2_000.0 + index as f32 * 200.0, 2_000.0);
            planet.local_velocity = Vec2::ZERO;
            planet.local_spaceport_position = planet.local_position + Vec2::Y * planet.radius;
            planet.local_spaceport_velocity = Vec2::ZERO;
        }
        let repair_target = observation.planets[0].id;
        observation.planets[0].owner = Some(PlayerId::PLAYER_2);
        observation.planets[0].radius = 100.0;
        observation.planets[0].local_position = Vec2::new(0.0, 500.0);
        observation.planets[0].local_velocity = Vec2::new(0.0, -200.0);
        observation.planets[0].local_spaceport_position = Vec2::new(100.0, 500.0);

        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        let _ = brain.intent(&observation);

        assert_eq!(brain.telemetry().goal, BrainGoal::Repair);
        assert_eq!(brain.telemetry().target_planet, Some(repair_target));
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Rendezvous)
        );
        assert!(!brain.telemetry().avoidance_predictive);
    }

    #[test]
    fn predictive_avoidance_keeps_its_tangent_side_during_one_threat() {
        let state = SpacewarsScenario::init(SpacewarsConfig::default(), 37);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.planets.truncate(1);
        observation.hazards.clear();
        let planet = &mut observation.planets[0];
        planet.radius = 100.0;
        planet.local_position = Vec2::new(0.0, 500.0);
        planet.local_velocity = Vec2::new(50.0, -200.0);
        let planet_id = planet.id;
        let first_preference =
            preferred_body_tangent_sign(planet.local_position, planet.local_velocity);

        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        brain.recent_departure = Some(RecentDeparture {
            planet: planet_id,
            cleared_tick: observation.tick,
        });
        let first = brain.body_avoidance(&observation, None).unwrap();
        assert!(first.predictive);

        observation.tick += 1;
        let planet = &mut observation.planets[0];
        planet.local_velocity = Vec2::new(-50.0, -200.0);
        let second_preference =
            preferred_body_tangent_sign(planet.local_position, planet.local_velocity);
        assert_ne!(first_preference, second_preference);
        let second = brain.body_avoidance(&observation, None).unwrap();

        assert!(second.predictive);
        assert_eq!(first.tangent_sign, second.tangent_sign);
    }

    #[test]
    fn collision_prediction_chooses_the_earliest_bounded_contact() {
        let hazard = |id, position, velocity| HazardObservationV1 {
            id: DebrisId::from_value(id).unwrap(),
            kind: scenario_spacewars::DebrisKind::Asteroid,
            owner: None,
            local_position: position,
            local_velocity: velocity,
            radius: 5.0,
        };
        let hazards = [
            hazard(1, Vec2::new(0.0, 100.0), Vec2::new(0.0, -100.0)),
            hazard(2, Vec2::new(0.0, 200.0), Vec2::new(0.0, -100.0)),
        ];

        let threat = earliest_collision_threat(&hazards, 5.0, 3.0, 10.0).unwrap();

        assert_eq!(threat.hazard, DebrisId::from_value(1).unwrap());
        assert_close(threat.seconds_until_closest, 1.0);
        assert_eq!(threat.avoidance_direction, Vec2::new(0.0, -100.0));
    }

    #[test]
    fn equal_reset_context_and_observation_produce_equal_output() {
        let mut config = SpacewarsConfig::deathmatch();
        config.asteroid_probability_per_sec = 0.0;
        let state = SpacewarsScenario::init(config, 31);
        let observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        let reset = BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        };
        let mut first = RuleShipBrain::default();
        let mut replay = RuleShipBrain::default();
        first.reset(reset);
        replay.reset(reset);

        assert_eq!(first.intent(&observation), replay.intent(&observation));
        assert_eq!(first.telemetry(), replay.telemetry());
    }

    #[test]
    fn rule_brain_turns_and_damages_a_stationary_opponent() {
        let mut config = SpacewarsConfig::deathmatch();
        config.asteroid_probability_per_sec = 0.0;
        config.players[0].health_percent = 100;
        config.players[1].health_percent = 100;
        let mut state = SpacewarsScenario::init(config, 17);
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        let mut encoder = ShipIntentEncoder::default();
        let starting_life = state.ships[0].life;

        for _ in 0..600 {
            let observation = SpacewarsScenario::observe_ship(
                &state,
                PlayerId::PLAYER_2,
                ShipSensorProfile::FullMapRadar,
            )
            .unwrap();
            let intent = brain.intent(&observation);
            let actions = encoder.encode(PlayerId::PLAYER_2.index(), intent);
            SpacewarsScenario::step(&mut state, &actions, DT);
            if state.ships[0].life < starting_life {
                break;
            }
        }

        assert!(state.ships[0].life < starting_life);
        assert_eq!(brain.telemetry().goal, BrainGoal::Attack);
    }

    #[test]
    fn rule_brain_arrives_at_a_moving_spaceport_and_captures_a_planet() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            use_starfield: false,
            use_sounds: false,
            ..SpacewarsConfig::default()
        };
        let mut state = SpacewarsScenario::init(config, 17);
        state.planets[0].owner_id = Some(PlayerId::PLAYER_1.index());
        state.players[0].planet_count = 1;
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        let mut encoder = ShipIntentEncoder::default();
        let mut saw_approach = false;
        let mut saw_docked = false;
        let mut saw_depart = false;
        let mut saw_next_target = false;
        let mut captured_planet = None;
        let mut minimum_target_distance = f32::INFINITY;

        for _ in 0..7_200 {
            let observation = SpacewarsScenario::observe_ship(
                &state,
                PlayerId::PLAYER_2,
                ShipSensorProfile::FullMapRadar,
            )
            .unwrap();
            let intent = brain.intent(&observation);
            let telemetry = brain.telemetry();
            minimum_target_distance = minimum_target_distance.min(telemetry.target_distance);
            if let Some(captured_planet) = captured_planet {
                saw_depart |= telemetry.target_planet == Some(captured_planet)
                    && telemetry.port_phase == Some(PortNavigationPhase::Depart);
                saw_next_target |= saw_depart
                    && telemetry.target_planet.is_some()
                    && telemetry.target_planet != Some(captured_planet)
                    && telemetry.port_phase == Some(PortNavigationPhase::Rendezvous);
            }
            saw_approach |= matches!(
                telemetry.port_phase,
                Some(PortNavigationPhase::Approach | PortNavigationPhase::Ingress)
            );
            saw_docked |= telemetry.port_phase == Some(PortNavigationPhase::Docked);
            let actions = encoder.encode(PlayerId::PLAYER_2.index(), intent);
            SpacewarsScenario::step(&mut state, &actions, DT);
            if captured_planet.is_none() {
                captured_planet = state
                    .planets
                    .iter()
                    .enumerate()
                    .find_map(|(index, planet)| {
                        if planet.owner_id == Some(PlayerId::PLAYER_2.index()) {
                            PlanetId::from_index(index)
                        } else {
                            None
                        }
                    });
            }
            if saw_next_target {
                break;
            }
        }

        assert!(
            saw_approach,
            "brain never advanced to a concrete spaceport approach; minimum distance {minimum_target_distance}; final telemetry: {:?}; ship position {:?}; velocity {:?}",
            brain.telemetry(),
            state.ships[1].position,
            state.ships[1].velocity,
        );
        assert!(saw_docked, "brain never established a spaceport contact");
        assert!(
            captured_planet.is_some(),
            "brain did not capture a planet; final telemetry: {:?}",
            brain.telemetry()
        );
        assert!(saw_depart, "brain did not depart after capture");
        assert!(
            saw_next_target,
            "brain did not select another uncaptured planet after departure; final telemetry: {:?}",
            brain.telemetry()
        );
    }

    #[test]
    fn rule_brain_captures_and_departs_across_multiple_generated_worlds() {
        for seed in [0, 3, 29, 47] {
            let mut config = SpacewarsConfig {
                asteroid_probability_per_sec: 0.0,
                use_starfield: false,
                use_sounds: false,
                ..SpacewarsConfig::default()
            };
            for player in &mut config.players {
                player.health_percent = 100_000;
            }
            let mut state = SpacewarsScenario::init(config, seed);
            state.planets[0].owner_id = Some(PlayerId::PLAYER_1.index());
            state.players[0].planet_count = 1;
            let mut brain = RuleShipBrain::default();
            brain.reset(BrainReset {
                actor: PlayerId::PLAYER_2,
                episode_seed: state.seed,
            });
            let mut encoder = ShipIntentEncoder::default();
            let mut captured_planet = None;
            let mut saw_depart = false;
            let mut saw_launch_burn = false;
            let mut saw_safe_clearance = false;
            let mut saw_next_target = false;
            let mut next_target_surface_clearance = None;

            for _ in 0..10_000 {
                let observation = SpacewarsScenario::observe_ship(
                    &state,
                    PlayerId::PLAYER_2,
                    ShipSensorProfile::FullMapRadar,
                )
                .unwrap();
                let intent = brain.intent(&observation);
                let telemetry = brain.telemetry();
                if let Some(captured_planet) = captured_planet {
                    saw_depart |= telemetry.target_planet == Some(captured_planet)
                        && telemetry.port_phase == Some(PortNavigationPhase::Depart);
                    saw_launch_burn |= telemetry.target_planet == Some(captured_planet)
                        && telemetry.port_phase == Some(PortNavigationPhase::Depart)
                        && intent.wings_closed
                        && intent.thrust > 0.0;
                    let captured = observation
                        .planets
                        .iter()
                        .find(|planet| planet.id == captured_planet)
                        .expect("captured planet remains observable");
                    let surface_clearance = captured.local_position.length()
                        - captured.radius
                        - observation.own_ship.collision_radius;
                    saw_safe_clearance |= saw_depart
                        && observation.own_ship.docked_planet != Some(captured_planet)
                        && surface_clearance >= brain.config.body_clearance;
                    let selected_next_target = saw_depart
                        && telemetry.target_planet.is_some()
                        && telemetry.target_planet != Some(captured_planet)
                        && telemetry.port_phase == Some(PortNavigationPhase::Rendezvous);
                    if selected_next_target {
                        next_target_surface_clearance = Some(surface_clearance);
                        saw_next_target = true;
                    }
                }
                let actions = encoder.encode(PlayerId::PLAYER_2.index(), intent);
                SpacewarsScenario::step(&mut state, &actions, DT);
                if captured_planet.is_none() {
                    captured_planet =
                        state
                            .planets
                            .iter()
                            .enumerate()
                            .find_map(|(index, planet)| {
                                if planet.owner_id == Some(PlayerId::PLAYER_2.index()) {
                                    PlanetId::from_index(index)
                                } else {
                                    None
                                }
                            });
                }
                if saw_next_target {
                    break;
                }
            }
            assert!(
                captured_planet.is_some(),
                "brain did not capture a planet for seed {seed}; final telemetry: {:?}",
                brain.telemetry()
            );
            assert!(
                saw_depart,
                "brain did not begin departure for seed {seed}; final telemetry: {:?}",
                brain.telemetry()
            );
            assert!(
                saw_launch_burn,
                "brain did not commit a wings-closed launch for seed {seed}; final telemetry: {:?}",
                brain.telemetry()
            );
            assert!(
                saw_safe_clearance,
                "brain selected its next target at surface clearance {next_target_surface_clearance:?} before ever reaching {} for seed {seed}",
                brain.config.body_clearance,
            );
            assert!(
                saw_next_target,
                "brain did not physically depart for seed {seed}; final telemetry: {:?}; ship position {:?}; velocity {:?}",
                brain.telemetry(),
                state.ships[1].position,
                state.ships[1].velocity,
            );
            assert_eq!(state.winner, None, "seed {seed} ended before departure");
        }
    }

    #[test]
    fn rule_brain_repeats_the_capture_cycle_in_seed_zero_world() {
        let mut config = SpacewarsConfig {
            universe_radius: 3_200,
            asteroid_probability_per_sec: 0.0,
            use_starfield: false,
            use_sounds: false,
            ..SpacewarsConfig::default()
        };
        for player in &mut config.players {
            player.health_percent = 100_000;
        }
        let mut state = SpacewarsScenario::init(config, 0);
        state.planets[0].owner_id = Some(PlayerId::PLAYER_1.index());
        state.players[0].planet_count = 1;
        let mut brain = RuleShipBrain::default();
        brain.reset(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: state.seed,
        });
        let mut encoder = ShipIntentEncoder::default();
        let mut final_observation = None;
        let mut trace = Vec::new();
        let mut last_signature = None;

        for _ in 0..21_600 {
            let observation = SpacewarsScenario::observe_ship(
                &state,
                PlayerId::PLAYER_2,
                ShipSensorProfile::FullMapRadar,
            )
            .unwrap();
            let intent = brain.intent(&observation);
            let telemetry = brain.telemetry();
            let signature = (
                telemetry.goal,
                telemetry.target_planet,
                telemetry.port_phase,
                observation.own_ship.docked_planet,
                intent.wings_closed,
            );
            if last_signature != Some(signature) {
                let dock_geometry = observation.own_ship.docked_planet.and_then(|docked| {
                    observation
                        .planets
                        .iter()
                        .find(|planet| planet.id == docked)
                        .map(|planet| {
                            (
                                planet.id,
                                planet.local_position.length()
                                    - planet.radius
                                    - observation.own_ship.collision_radius,
                                planet.local_spaceport_position.length(),
                            )
                        })
                });
                trace.push(format!(
                    "tick={} planets={} goal={:?} target={:?} phase={:?} docked={:?} dock_geometry={dock_geometry:?} intent={intent:?}",
                    state.tick,
                    state.players[PlayerId::PLAYER_2.index()].planet_count,
                    telemetry.goal,
                    telemetry.target_planet,
                    telemetry.port_phase,
                    observation.own_ship.docked_planet,
                ));
                last_signature = Some(signature);
            }
            let actions = encoder.encode(PlayerId::PLAYER_2.index(), intent);
            SpacewarsScenario::step(&mut state, &actions, DT);
            final_observation = Some(observation);
            if state.players[PlayerId::PLAYER_2.index()].planet_count >= 3 {
                break;
            }
        }

        let observation = final_observation.expect("simulation produced an observation");
        let telemetry = brain.telemetry();
        let target_planet = telemetry.target_planet.and_then(|target| {
            observation
                .planets
                .iter()
                .find(|planet| planet.id == target)
        });
        assert!(
            state.players[PlayerId::PLAYER_2.index()].planet_count >= 3,
            "brain did not complete three captures; final telemetry: {telemetry:?}; own ship: {:?}; target planet: {target_planet:?}; winner: {:?}; trace:\n{}",
            observation.own_ship,
            state.winner,
            trace.join("\n"),
        );
    }
}
