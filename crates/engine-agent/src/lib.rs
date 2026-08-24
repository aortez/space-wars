//! Deterministic, headless episode execution for Spacewars controllers.
//!
//! The evaluator owns authoritative scenario state for measurement, while
//! controllers receive only typed observations and emit canonical actions.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    fmt,
    time::{Duration, Instant},
};

use engine_common::{Action, PointerPhase, Scenario};
use engine_core::SpacewarsConfig;
use scenario_spacewars::{
    BodyCollision, BodyId, DebrisKind, LaserTarget, PlayerId, SPACEWARS_PLAYER_COUNT,
    ShipCollision, ShipForm, ShipIntent, ShipIntentEncoder, ShipSensorProfile, SpacewarsScenario,
    SpacewarsState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spacewars_ai::{
    AvoidanceBody, BrainGoal, BrainReset, BrainTelemetry, BuiltInPolicy, PortNavigationPhase,
    ShipBrain, StrategicGoal, StrategyTelemetry,
};

pub const REPORT_SCHEMA_VERSION: u32 = 4;
pub const BASELINE_SCHEMA_VERSION: u32 = 1;
pub const POLICY_COMPARISON_SCHEMA_VERSION: u32 = 2;
pub const NAVIGATION_V1_SUITE_ID: &str = "navigation_v1";
pub const NAVIGATION_V1_SEEDS: [u64; 6] = [0, 1, 2, 3, 4, 5];
pub const NAVIGATION_V1_MAX_TICKS: u64 = 36_000;
pub const STRATEGY_V1_SUITE_ID: &str = "strategy_v1";
pub const STRATEGY_V1_SEEDS: [u64; 4] = [0, 1, 2, 3];
pub const STRATEGY_V1_MAX_TICKS: u64 = 18_000;
const NAVIGATION_HEALTH_PERCENT: u32 = 200;
const NAVIGATION_SAFE_DEPARTURE_CLEARANCE: f32 = 90.0;
const CONTACT_INCIDENT_REARM_TICKS: u64 = 30;
const NAVIGATION_TRACE_HEARTBEAT_TICKS: u64 = 300;
const NAVIGATION_V1_BASELINE_JSON: &str = include_str!("../baselines/navigation-v1.json");
const STRATEGY_V1_BASELINE_JSON: &str = include_str!("../baselines/strategy-v1.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerKind {
    Idle,
    RuleV5,
    RuleV6,
    RuleV7,
}

impl ControllerKind {
    pub const fn built_in_policy(self) -> Option<BuiltInPolicy> {
        match self {
            Self::Idle => None,
            Self::RuleV5 => Some(BuiltInPolicy::RuleV5),
            Self::RuleV6 => Some(BuiltInPolicy::RuleV6),
            Self::RuleV7 => Some(BuiltInPolicy::RuleV7),
        }
    }

    pub const fn policy_id(self) -> &'static str {
        match self {
            Self::Idle => "idle_v1",
            Self::RuleV5 => BuiltInPolicy::RuleV5.descriptor().policy_id,
            Self::RuleV6 => BuiltInPolicy::RuleV6.descriptor().policy_id,
            Self::RuleV7 => BuiltInPolicy::RuleV7.descriptor().policy_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacewarsPreset {
    Standard,
    StandardNoAsteroids,
    Navigation,
    Deathmatch,
}

impl SpacewarsPreset {
    fn config(self) -> SpacewarsConfig {
        match self {
            Self::Standard => SpacewarsConfig::default(),
            Self::StandardNoAsteroids => SpacewarsConfig {
                asteroid_probability_per_sec: 0.0,
                ..SpacewarsConfig::default()
            },
            Self::Navigation => {
                let mut config = Self::StandardNoAsteroids.config();
                for player in &mut config.players {
                    player.health_percent = NAVIGATION_HEALTH_PERCENT;
                }
                config
            }
            Self::Deathmatch => SpacewarsConfig::deathmatch(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationSuite {
    NavigationV1,
    StrategyV1,
}

impl EvaluationSuite {
    pub const fn id(self) -> &'static str {
        match self {
            Self::NavigationV1 => NAVIGATION_V1_SUITE_ID,
            Self::StrategyV1 => STRATEGY_V1_SUITE_ID,
        }
    }

    pub fn episode_configs(self) -> Vec<EpisodeConfig> {
        match self {
            Self::NavigationV1 => NAVIGATION_V1_SEEDS
                .into_iter()
                .map(|seed| EpisodeConfig {
                    seed,
                    preset: SpacewarsPreset::Navigation,
                    controllers: [ControllerKind::RuleV5; SPACEWARS_PLAYER_COUNT],
                    max_ticks: NAVIGATION_V1_MAX_TICKS,
                    trace_player: None,
                })
                .collect(),
            Self::StrategyV1 => {
                let mut configs = Vec::with_capacity(STRATEGY_V1_SEEDS.len() * 3);
                for seed in STRATEGY_V1_SEEDS {
                    for controllers in [
                        [ControllerKind::RuleV5, ControllerKind::Idle],
                        [ControllerKind::Idle, ControllerKind::RuleV5],
                        [ControllerKind::RuleV5, ControllerKind::RuleV5],
                    ] {
                        configs.push(EpisodeConfig {
                            seed,
                            preset: SpacewarsPreset::StandardNoAsteroids,
                            controllers,
                            max_ticks: STRATEGY_V1_MAX_TICKS,
                            trace_player: None,
                        });
                    }
                }
                configs
            }
        }
    }

    const fn baseline_json(self) -> &'static str {
        match self {
            Self::NavigationV1 => NAVIGATION_V1_BASELINE_JSON,
            Self::StrategyV1 => STRATEGY_V1_BASELINE_JSON,
        }
    }
}

/// Fixed world workload used for paired policy comparisons.
///
/// Unlike [`EvaluationSuite`], a profile does not pin either controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyComparisonProfile {
    NavigationV1,
    StrategyV1,
}

impl PolicyComparisonProfile {
    pub const fn id(self) -> &'static str {
        match self {
            Self::NavigationV1 => NAVIGATION_V1_SUITE_ID,
            Self::StrategyV1 => STRATEGY_V1_SUITE_ID,
        }
    }

    pub const fn config(
        self,
        baseline: ControllerKind,
        candidate: ControllerKind,
    ) -> PolicyComparisonConfig {
        match self {
            Self::NavigationV1 => PolicyComparisonConfig {
                workload_id: Some(NAVIGATION_V1_SUITE_ID),
                start_seed: NAVIGATION_V1_SEEDS[0],
                seed_step: 1,
                episodes: NAVIGATION_V1_SEEDS.len() as u32,
                preset: SpacewarsPreset::Navigation,
                baseline,
                candidate,
                max_ticks: NAVIGATION_V1_MAX_TICKS,
            },
            Self::StrategyV1 => PolicyComparisonConfig {
                workload_id: Some(STRATEGY_V1_SUITE_ID),
                start_seed: STRATEGY_V1_SEEDS[0],
                seed_step: 1,
                episodes: STRATEGY_V1_SEEDS.len() as u32,
                preset: SpacewarsPreset::StandardNoAsteroids,
                baseline,
                candidate,
                max_ticks: STRATEGY_V1_MAX_TICKS,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ControllerDescriptor {
    pub kind: ControllerKind,
    pub policy_id: &'static str,
}

impl From<ControllerKind> for ControllerDescriptor {
    fn from(kind: ControllerKind) -> Self {
        Self {
            kind,
            policy_id: kind.policy_id(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpisodeConfig {
    pub seed: u64,
    pub preset: SpacewarsPreset,
    pub controllers: [ControllerKind; SPACEWARS_PLAYER_COUNT],
    pub max_ticks: u64,
    pub trace_player: Option<PlayerId>,
}

impl Default for EpisodeConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            preset: SpacewarsPreset::Standard,
            controllers: [ControllerKind::Idle, ControllerKind::RuleV5],
            max_ticks: 36_000,
            trace_player: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchConfig {
    pub start_seed: u64,
    pub seed_step: u64,
    pub episodes: u32,
    pub preset: SpacewarsPreset,
    pub controllers: [ControllerKind; SPACEWARS_PLAYER_COUNT],
    pub max_ticks: u64,
    pub trace_player: Option<PlayerId>,
}

impl Default for BatchConfig {
    fn default() -> Self {
        let episode = EpisodeConfig::default();
        Self {
            start_seed: episode.seed,
            seed_step: 1,
            episodes: 1,
            preset: episode.preset,
            controllers: episode.controllers,
            max_ticks: episode.max_ticks,
            trace_player: episode.trace_player,
        }
    }
}

/// Deterministic paired comparison of two policies over the same seeds.
///
/// Every seed produces two episodes with the controller seats swapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyComparisonConfig {
    pub workload_id: Option<&'static str>,
    pub start_seed: u64,
    pub seed_step: u64,
    pub episodes: u32,
    pub preset: SpacewarsPreset,
    pub baseline: ControllerKind,
    pub candidate: ControllerKind,
    pub max_ticks: u64,
}

impl Default for PolicyComparisonConfig {
    fn default() -> Self {
        PolicyComparisonProfile::StrategyV1.config(ControllerKind::RuleV5, ControllerKind::RuleV5)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EpisodeOutcome {
    Winner { player: PlayerId },
    TickLimit,
}

impl fmt::Display for EpisodeOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Winner { player } => write!(formatter, "winner:p{}", player.index() + 1),
            Self::TickLimit => formatter.write_str("tick_limit"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationTraceReason {
    Start,
    BrainTransition,
    DockingTransition,
    Capture,
    SafeDeparture,
    Heartbeat,
    EpisodeEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NavigationTraceIntent {
    pub turn: f32,
    pub thrust: f32,
    pub brake: f32,
    pub wings_closed: bool,
    pub laser: bool,
    pub cannon: bool,
}

impl From<ShipIntent> for NavigationTraceIntent {
    fn from(intent: ShipIntent) -> Self {
        Self {
            turn: intent.turn,
            thrust: intent.thrust,
            brake: intent.brake,
            wings_closed: intent.wings_closed,
            laser: intent.laser,
            cannon: intent.cannon,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NavigationTraceEvent {
    pub tick: u64,
    pub player: PlayerId,
    pub reasons: Vec<NavigationTraceReason>,
    pub strategy: StrategyTelemetry,
    pub goal: BrainGoal,
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
    pub target_planet: Option<usize>,
    pub port_phase: Option<PortNavigationPhase>,
    pub port_attempt_age_ticks: u64,
    pub port_attempt_stalled_ticks: u64,
    pub port_attempt_obstructed_ticks: u64,
    pub port_replan_count: u64,
    pub cooled_port_planet: Option<usize>,
    pub port_cooldown_remaining_ticks: u64,
    pub multi_body_escape_active: bool,
    pub multi_body_escape_age_ticks: u64,
    pub multi_body_escape_body_count: u32,
    pub multi_body_escape_activations: u64,
    pub docked_planet: Option<usize>,
    pub focus_planet: Option<usize>,
    pub pending_capture_planet: Option<usize>,
    pub pending_capture_ticks: Option<u64>,
    pub surface_clearance: Option<f32>,
    pub outward_speed: Option<f32>,
    pub spaceport_distance: Option<f32>,
    pub spaceport_angular_speed: Option<f32>,
    pub world_velocity: [f32; 2],
    pub world_speed: f32,
    pub world_rotation: f32,
    pub spaceport_rotation: Option<f32>,
    pub angular_velocity: f32,
    pub measured_angular_speed: f32,
    pub target_distance: f32,
    pub heading_error: f32,
    pub desired_speed: f32,
    pub relative_speed: f32,
    pub body_contact: bool,
    pub ship_contact: bool,
    pub intent: NavigationTraceIntent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct StrategyMetrics {
    pub objective_selections: u64,
    pub port_replans: u64,
    pub multi_body_escape_activations: u64,
    pub idle_ticks: u64,
    pub survive_ticks: u64,
    pub attack_ticks: u64,
    pub capture_ticks: u64,
    pub repair_ticks: u64,
    pub defend_ticks: u64,
    pub rebuild_ticks: u64,
}

impl StrategyMetrics {
    fn observe_goal(&mut self, goal: StrategicGoal) {
        let ticks = match goal {
            StrategicGoal::Idle => &mut self.idle_ticks,
            StrategicGoal::Survive => &mut self.survive_ticks,
            StrategicGoal::Attack => &mut self.attack_ticks,
            StrategicGoal::Capture => &mut self.capture_ticks,
            StrategicGoal::Repair => &mut self.repair_ticks,
            StrategicGoal::Defend => &mut self.defend_ticks,
            StrategicGoal::Rebuild => &mut self.rebuild_ticks,
        };
        *ticks += 1;
    }

    fn add_assign(&mut self, other: Self) {
        self.objective_selections += other.objective_selections;
        self.port_replans += other.port_replans;
        self.multi_body_escape_activations += other.multi_body_escape_activations;
        self.idle_ticks += other.idle_ticks;
        self.survive_ticks += other.survive_ticks;
        self.attack_ticks += other.attack_ticks;
        self.capture_ticks += other.capture_ticks;
        self.repair_ticks += other.repair_ticks;
        self.defend_ticks += other.defend_ticks;
        self.rebuild_ticks += other.rebuild_ticks;
    }
}

/// Health and sustained-contact measurements for one controller seat.
///
/// Damage and healing are normalized to the configured maximum ship life, so
/// `1.0` represents one complete health bar and cumulative values may exceed
/// one after repairs or rebuild cycles. Escape-pod rebuild progress is not
/// counted as ship damage or healing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HealthMetrics {
    pub damage_taken_fraction: f64,
    pub damage_while_in_body_contact_fraction: f64,
    pub healing_received_fraction: f64,
    pub minimum_life_fraction: f32,
    pub mean_ship_life_fraction: f64,
    pub ship_ticks: u64,
    pub pod_ticks: u64,
    pub damaged_ticks: u64,
    pub critical_ticks: u64,
    pub longest_damaged_streak_ticks: u64,
    pub longest_critical_streak_ticks: u64,
    pub body_contact_ticks: u64,
    pub longest_body_contact_ticks: u64,
}

impl Default for HealthMetrics {
    fn default() -> Self {
        Self {
            damage_taken_fraction: 0.0,
            damage_while_in_body_contact_fraction: 0.0,
            healing_received_fraction: 0.0,
            minimum_life_fraction: 1.0,
            mean_ship_life_fraction: 1.0,
            ship_ticks: 0,
            pod_ticks: 0,
            damaged_ticks: 0,
            critical_ticks: 0,
            longest_damaged_streak_ticks: 0,
            longest_critical_streak_ticks: 0,
            body_contact_ticks: 0,
            longest_body_contact_ticks: 0,
        }
    }
}

impl HealthMetrics {
    fn add_episode(&mut self, other: Self) {
        let combined_ship_ticks = self.ship_ticks + other.ship_ticks;
        if combined_ship_ticks > 0 {
            self.mean_ship_life_fraction = (self.mean_ship_life_fraction * self.ship_ticks as f64
                + other.mean_ship_life_fraction * other.ship_ticks as f64)
                / combined_ship_ticks as f64;
        }
        self.damage_taken_fraction += other.damage_taken_fraction;
        self.damage_while_in_body_contact_fraction += other.damage_while_in_body_contact_fraction;
        self.healing_received_fraction += other.healing_received_fraction;
        self.minimum_life_fraction = self.minimum_life_fraction.min(other.minimum_life_fraction);
        self.ship_ticks = combined_ship_ticks;
        self.pod_ticks += other.pod_ticks;
        self.damaged_ticks += other.damaged_ticks;
        self.critical_ticks += other.critical_ticks;
        self.longest_damaged_streak_ticks = self
            .longest_damaged_streak_ticks
            .max(other.longest_damaged_streak_ticks);
        self.longest_critical_streak_ticks = self
            .longest_critical_streak_ticks
            .max(other.longest_critical_streak_ticks);
        self.body_contact_ticks += other.body_contact_ticks;
        self.longest_body_contact_ticks = self
            .longest_body_contact_ticks
            .max(other.longest_body_contact_ticks);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolicyMetrics {
    pub controller: ControllerKind,
    pub policy_id: &'static str,
    /// One episode contributes once for each seat using this policy.
    pub seat_episodes: u64,
    pub ticks: u64,
    pub captures: u64,
    pub ship_losses: u64,
    pub planet_impact_losses: u64,
    pub sun_impact_losses: u64,
    pub rebuilds: u64,
    pub body_contacts: u64,
    pub health: HealthMetrics,
}

impl PolicyMetrics {
    fn new(controller: ControllerDescriptor) -> Self {
        Self {
            controller: controller.kind,
            policy_id: controller.policy_id,
            seat_episodes: 0,
            ticks: 0,
            captures: 0,
            ship_losses: 0,
            planet_impact_losses: 0,
            sun_impact_losses: 0,
            rebuilds: 0,
            body_contacts: 0,
            health: HealthMetrics::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EpisodeSummary {
    pub seed: u64,
    pub preset: SpacewarsPreset,
    pub controllers: [ControllerDescriptor; SPACEWARS_PLAYER_COUNT],
    pub max_ticks: u64,
    pub ticks: u64,
    pub simulated_seconds: f64,
    pub outcome: EpisodeOutcome,
    pub captures: [u64; SPACEWARS_PLAYER_COUNT],
    pub ship_losses: [u64; SPACEWARS_PLAYER_COUNT],
    pub planet_impact_losses: [u64; SPACEWARS_PLAYER_COUNT],
    pub sun_impact_losses: [u64; SPACEWARS_PLAYER_COUNT],
    pub rebuilds: [u64; SPACEWARS_PLAYER_COUNT],
    pub eliminations: [u64; SPACEWARS_PLAYER_COUNT],
    pub body_contacts: [u64; SPACEWARS_PLAYER_COUNT],
    pub ship_contacts: [u64; SPACEWARS_PLAYER_COUNT],
    pub debris_impacts: [u64; SPACEWARS_PLAYER_COUNT],
    pub laser_hits_received: [u64; SPACEWARS_PLAYER_COUNT],
    pub port_dockings: [u64; SPACEWARS_PLAYER_COUNT],
    pub port_departures: [u64; SPACEWARS_PLAYER_COUNT],
    pub safe_capture_departures: [u64; SPACEWARS_PLAYER_COUNT],
    pub safe_rebuild_departures: [u64; SPACEWARS_PLAYER_COUNT],
    pub final_planet_counts: [usize; SPACEWARS_PLAYER_COUNT],
    pub final_ship_forms: [ShipForm; SPACEWARS_PLAYER_COUNT],
    pub final_life_fractions: [f32; SPACEWARS_PLAYER_COUNT],
    pub health: [HealthMetrics; SPACEWARS_PLAYER_COUNT],
    pub strategy: [StrategyMetrics; SPACEWARS_PLAYER_COUNT],
    pub actions_emitted: u64,
    pub trace_sha256: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub navigation_trace: Vec<NavigationTraceEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatchSummary {
    pub episodes: u32,
    pub total_ticks: u64,
    pub total_simulated_seconds: f64,
    pub winner_counts: [u32; SPACEWARS_PLAYER_COUNT],
    pub tick_limits: u32,
    pub captures: [u64; SPACEWARS_PLAYER_COUNT],
    pub ship_losses: [u64; SPACEWARS_PLAYER_COUNT],
    pub planet_impact_losses: [u64; SPACEWARS_PLAYER_COUNT],
    pub sun_impact_losses: [u64; SPACEWARS_PLAYER_COUNT],
    pub rebuilds: [u64; SPACEWARS_PLAYER_COUNT],
    pub body_contacts: [u64; SPACEWARS_PLAYER_COUNT],
    pub ship_contacts: [u64; SPACEWARS_PLAYER_COUNT],
    pub debris_impacts: [u64; SPACEWARS_PLAYER_COUNT],
    pub laser_hits_received: [u64; SPACEWARS_PLAYER_COUNT],
    pub port_dockings: [u64; SPACEWARS_PLAYER_COUNT],
    pub port_departures: [u64; SPACEWARS_PLAYER_COUNT],
    pub safe_capture_departures: [u64; SPACEWARS_PLAYER_COUNT],
    pub safe_rebuild_departures: [u64; SPACEWARS_PLAYER_COUNT],
    pub health: [HealthMetrics; SPACEWARS_PLAYER_COUNT],
    pub strategy: [StrategyMetrics; SPACEWARS_PLAYER_COUNT],
    pub policy_metrics: Vec<PolicyMetrics>,
    pub wall_seconds: f64,
    pub ticks_per_wall_second: f64,
    pub simulated_seconds_per_wall_second: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunReport {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suite_id: Option<&'static str>,
    pub episodes: Vec<EpisodeSummary>,
    pub summary: BatchSummary,
}

/// Stable subset of an episode used by checked-in deterministic baselines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodeFingerprint {
    pub seed: u64,
    pub preset: SpacewarsPreset,
    pub controllers: [String; SPACEWARS_PLAYER_COUNT],
    pub max_ticks: u64,
    pub ticks: u64,
    pub trace_sha256: String,
}

impl From<&EpisodeSummary> for EpisodeFingerprint {
    fn from(episode: &EpisodeSummary) -> Self {
        Self {
            seed: episode.seed,
            preset: episode.preset,
            controllers: std::array::from_fn(|player| {
                episode.controllers[player].policy_id.to_owned()
            }),
            max_ticks: episode.max_ticks,
            ticks: episode.ticks,
            trace_sha256: episode.trace_sha256.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SuiteBaselineManifest {
    schema_version: u32,
    suite_id: String,
    episodes: Vec<EpisodeFingerprint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BaselineVerification {
    pub schema_version: u32,
    pub suite_id: &'static str,
    pub episodes: u32,
}

#[derive(Debug)]
pub enum BaselineVerificationError {
    InvalidManifest {
        suite_id: &'static str,
        source: serde_json::Error,
    },
    UnsupportedSchema {
        suite_id: &'static str,
        expected: u32,
        actual: u32,
    },
    ManifestSuiteMismatch {
        expected: &'static str,
        actual: String,
    },
    ReportSuiteMismatch {
        expected: &'static str,
        actual: Option<&'static str>,
    },
    EpisodeCountMismatch {
        suite_id: &'static str,
        expected: usize,
        actual: usize,
    },
    EpisodeMismatch {
        suite_id: &'static str,
        index: usize,
        expected: Box<EpisodeFingerprint>,
        actual: Box<EpisodeFingerprint>,
    },
}

impl fmt::Display for BaselineVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidManifest { suite_id, source } => {
                write!(formatter, "invalid {suite_id} baseline manifest: {source}")
            }
            Self::UnsupportedSchema {
                suite_id,
                expected,
                actual,
            } => write!(
                formatter,
                "unsupported {suite_id} baseline schema {actual}; expected {expected}"
            ),
            Self::ManifestSuiteMismatch { expected, actual } => write!(
                formatter,
                "baseline manifest names suite {actual}; expected {expected}"
            ),
            Self::ReportSuiteMismatch { expected, actual } => write!(
                formatter,
                "cannot verify report for suite {actual:?} against {expected}"
            ),
            Self::EpisodeCountMismatch {
                suite_id,
                expected,
                actual,
            } => write!(
                formatter,
                "{suite_id} baseline contains {expected} episodes but the run produced {actual}"
            ),
            Self::EpisodeMismatch {
                suite_id,
                index,
                expected,
                actual,
            } => write!(
                formatter,
                "{suite_id} episode {index} differs from its baseline\nexpected: {expected:?}\n  actual: {actual:?}"
            ),
        }
    }
}

impl std::error::Error for BaselineVerificationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidManifest { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComparedPolicyMetrics {
    pub controller: ControllerDescriptor,
    pub episodes: u32,
    pub wins: u32,
    pub tick_limits: u32,
    pub ticks: u64,
    pub captures: u64,
    pub ship_losses: u64,
    pub planet_impact_losses: u64,
    pub sun_impact_losses: u64,
    pub rebuilds: u64,
    pub eliminations: u64,
    pub body_contacts: u64,
    pub ship_contacts: u64,
    pub debris_impacts: u64,
    pub laser_hits_received: u64,
    pub port_dockings: u64,
    pub port_departures: u64,
    pub safe_capture_departures: u64,
    pub safe_rebuild_departures: u64,
    pub final_planet_count_sum: u64,
    pub health: HealthMetrics,
    pub strategy: StrategyMetrics,
}

impl ComparedPolicyMetrics {
    fn new(controller: ControllerDescriptor) -> Self {
        Self {
            controller,
            episodes: 0,
            wins: 0,
            tick_limits: 0,
            ticks: 0,
            captures: 0,
            ship_losses: 0,
            planet_impact_losses: 0,
            sun_impact_losses: 0,
            rebuilds: 0,
            eliminations: 0,
            body_contacts: 0,
            ship_contacts: 0,
            debris_impacts: 0,
            laser_hits_received: 0,
            port_dockings: 0,
            port_departures: 0,
            safe_capture_departures: 0,
            safe_rebuild_departures: 0,
            final_planet_count_sum: 0,
            health: HealthMetrics::default(),
            strategy: StrategyMetrics::default(),
        }
    }

    fn observe(&mut self, episode: &EpisodeSummary, player: usize) {
        self.episodes += 1;
        self.ticks += episode.ticks;
        match episode.outcome {
            EpisodeOutcome::Winner { player: winner } if winner.index() == player => {
                self.wins += 1;
            }
            EpisodeOutcome::TickLimit => self.tick_limits += 1,
            EpisodeOutcome::Winner { .. } => {}
        }
        self.captures += episode.captures[player];
        self.ship_losses += episode.ship_losses[player];
        self.planet_impact_losses += episode.planet_impact_losses[player];
        self.sun_impact_losses += episode.sun_impact_losses[player];
        self.rebuilds += episode.rebuilds[player];
        self.eliminations += episode.eliminations[player];
        self.body_contacts += episode.body_contacts[player];
        self.ship_contacts += episode.ship_contacts[player];
        self.debris_impacts += episode.debris_impacts[player];
        self.laser_hits_received += episode.laser_hits_received[player];
        self.port_dockings += episode.port_dockings[player];
        self.port_departures += episode.port_departures[player];
        self.safe_capture_departures += episode.safe_capture_departures[player];
        self.safe_rebuild_departures += episode.safe_rebuild_departures[player];
        self.final_planet_count_sum += episode.final_planet_counts[player] as u64;
        self.health.add_episode(episode.health[player]);
        self.strategy.add_assign(episode.strategy[player]);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolicyComparisonSummary {
    pub seed_pairs: u32,
    pub episode_runs: u32,
    pub baseline: ComparedPolicyMetrics,
    pub candidate: ComparedPolicyMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolicyComparisonReport {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workload_id: Option<&'static str>,
    pub summary: PolicyComparisonSummary,
    pub run: RunReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunError {
    NoEpisodes,
    ZeroTickLimit,
    SeedOverflow { episode_index: u32 },
    MissingObservation { player: PlayerId },
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEpisodes => formatter.write_str("episode count must be greater than zero"),
            Self::ZeroTickLimit => {
                formatter.write_str("maximum tick count must be greater than zero")
            }
            Self::SeedOverflow { episode_index } => {
                write!(formatter, "seed overflow at episode index {episode_index}")
            }
            Self::MissingObservation { player } => {
                write!(
                    formatter,
                    "scenario did not produce an observation for player {}",
                    player.index() + 1
                )
            }
        }
    }
}

impl std::error::Error for RunError {}

enum SeatController {
    Idle,
    Brain {
        kind: ControllerKind,
        brain: Box<dyn ShipBrain>,
    },
}

impl SeatController {
    fn new(kind: ControllerKind, actor: PlayerId, episode_seed: u64) -> Self {
        let Some(policy) = kind.built_in_policy() else {
            return Self::Idle;
        };
        let mut brain = policy.create();
        brain.reset(BrainReset {
            actor,
            episode_seed,
        });
        Self::Brain { kind, brain }
    }

    fn descriptor(&self) -> ControllerDescriptor {
        match self {
            Self::Idle => ControllerKind::Idle.into(),
            Self::Brain { kind, brain } => ControllerDescriptor {
                kind: *kind,
                policy_id: brain.descriptor().policy_id,
            },
        }
    }

    fn intent(&mut self, state: &SpacewarsState, actor: PlayerId) -> Result<ShipIntent, RunError> {
        match self {
            Self::Idle => Ok(ShipIntent::default()),
            Self::Brain { brain, .. } => {
                let observation =
                    SpacewarsScenario::observe_ship(state, actor, ShipSensorProfile::FullMapRadar)
                        .ok_or(RunError::MissingObservation { player: actor })?;
                Ok(brain.intent(&observation))
            }
        }
    }

    fn telemetry(&self) -> BrainTelemetry {
        match self {
            Self::Idle => BrainTelemetry::default(),
            Self::Brain { brain, .. } => brain.telemetry(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StrategyIdentity {
    goal: StrategicGoal,
    target: Option<PlayerId>,
    target_planet: Option<usize>,
}

impl From<StrategyTelemetry> for StrategyIdentity {
    fn from(telemetry: StrategyTelemetry) -> Self {
        Self {
            goal: telemetry.goal,
            target: telemetry.target,
            target_planet: telemetry.target_planet.map(|planet| planet.index()),
        }
    }
}

#[derive(Debug, Default)]
struct StrategyMetricTracker {
    previous: Option<StrategyIdentity>,
    metrics: StrategyMetrics,
}

impl StrategyMetricTracker {
    fn observe(&mut self, telemetry: BrainTelemetry) {
        let identity = telemetry.strategy.into();
        if self.previous != Some(identity) {
            self.metrics.objective_selections += 1;
            self.previous = Some(identity);
        }
        self.metrics.port_replans = self.metrics.port_replans.max(telemetry.port_replan_count);
        self.metrics.multi_body_escape_activations = self
            .metrics
            .multi_body_escape_activations
            .max(telemetry.multi_body_escape_activations);
        self.metrics.observe_goal(telemetry.strategy.goal);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NavigationTraceSemantic {
    strategy: StrategyIdentity,
    goal: BrainGoal,
    avoided_body: Option<AvoidanceBody>,
    avoidance_predictive: bool,
    avoidance_escape_assist: bool,
    avoidance_emergency_escape_assist: bool,
    target_planet: Option<usize>,
    port_phase: Option<PortNavigationPhase>,
    port_replan_count: u64,
    multi_body_escape_active: bool,
    multi_body_escape_activations: u64,
    docked_planet: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct PendingCapture {
    planet: usize,
    tick: u64,
}

#[derive(Debug)]
struct NavigationTraceCollector {
    player: PlayerId,
    previous_planet_owners: Vec<Option<usize>>,
    previous_semantic: Option<NavigationTraceSemantic>,
    previous_docked_planet: Option<usize>,
    pending_capture: Option<PendingCapture>,
    last_heartbeat_tick: Option<u64>,
    previous_rotation: f32,
    previous_rotation_tick: u64,
    events: Vec<NavigationTraceEvent>,
}

impl NavigationTraceCollector {
    fn new(state: &SpacewarsState, player: PlayerId) -> Self {
        Self {
            player,
            previous_planet_owners: state.planets.iter().map(|planet| planet.owner_id).collect(),
            previous_semantic: None,
            previous_docked_planet: docked_planet(state, player.index()),
            pending_capture: None,
            last_heartbeat_tick: None,
            previous_rotation: state.ships[player.index()].rotation_radians,
            previous_rotation_tick: state.tick,
            events: Vec::new(),
        }
    }

    fn observe(
        &mut self,
        state: &SpacewarsState,
        telemetry: BrainTelemetry,
        intent: ShipIntent,
        episode_end: bool,
    ) {
        let player = self.player.index();
        let ship = &state.ships[player];
        let rotation_delta = (ship.rotation_radians - self.previous_rotation
            + core::f32::consts::PI)
            .rem_euclid(core::f32::consts::TAU)
            - core::f32::consts::PI;
        let elapsed_ticks = state.tick.saturating_sub(self.previous_rotation_tick);
        let measured_angular_speed = if elapsed_ticks == 0 {
            0.0
        } else {
            rotation_delta * state.config.fps as f32 / elapsed_ticks as f32
        };
        let docked_planet = docked_planet(state, player);
        let semantic = NavigationTraceSemantic {
            strategy: telemetry.strategy.into(),
            goal: telemetry.goal,
            avoided_body: telemetry.avoided_body,
            avoidance_predictive: telemetry.avoidance_predictive,
            avoidance_escape_assist: telemetry.avoidance_escape_assist,
            avoidance_emergency_escape_assist: telemetry.avoidance_emergency_escape_assist,
            target_planet: telemetry.target_planet.map(|planet| planet.index()),
            port_phase: telemetry.port_phase,
            port_replan_count: telemetry.port_replan_count,
            multi_body_escape_active: telemetry.multi_body_escape_active,
            multi_body_escape_activations: telemetry.multi_body_escape_activations,
            docked_planet,
        };
        let captured_planet = self.capture_transition(state);
        let mut reasons = Vec::new();

        if self.previous_semantic.is_none() {
            reasons.push(NavigationTraceReason::Start);
        } else if self.previous_semantic != Some(semantic) {
            reasons.push(NavigationTraceReason::BrainTransition);
        }
        if docked_planet != self.previous_docked_planet {
            reasons.push(NavigationTraceReason::DockingTransition);
        }
        if let Some(planet) = captured_planet {
            reasons.push(NavigationTraceReason::Capture);
            self.pending_capture = Some(PendingCapture {
                planet,
                tick: state.tick,
            });
            self.last_heartbeat_tick = Some(state.tick);
        }

        let pending_capture = self.pending_capture;
        let focus_planet = pending_capture
            .map(|pending| pending.planet)
            .or(semantic.target_planet)
            .or(docked_planet);
        let geometry =
            focus_planet.and_then(|planet| trace_planet_geometry(state, self.player, planet));
        let safe_departure = pending_capture.is_some_and(|pending| {
            docked_planet != Some(pending.planet)
                && geometry.is_some_and(|geometry| {
                    geometry.surface_clearance >= NAVIGATION_SAFE_DEPARTURE_CLEARANCE
                })
        });
        if safe_departure {
            reasons.push(NavigationTraceReason::SafeDeparture);
        } else if pending_capture.is_some()
            && captured_planet.is_none()
            && self.last_heartbeat_tick.is_none_or(|last| {
                state.tick.saturating_sub(last) >= NAVIGATION_TRACE_HEARTBEAT_TICKS
            })
        {
            reasons.push(NavigationTraceReason::Heartbeat);
            self.last_heartbeat_tick = Some(state.tick);
        }
        if episode_end {
            reasons.push(NavigationTraceReason::EpisodeEnd);
        }

        if !reasons.is_empty() {
            self.events.push(NavigationTraceEvent {
                tick: state.tick,
                player: self.player,
                reasons,
                strategy: telemetry.strategy,
                goal: telemetry.goal,
                avoided_body: telemetry.avoided_body,
                avoidance_surface_clearance: telemetry.avoidance_surface_clearance,
                avoidance_outward_speed: telemetry.avoidance_outward_speed,
                avoidance_predictive: telemetry.avoidance_predictive,
                avoidance_seconds_until_closest: telemetry.avoidance_seconds_until_closest,
                avoidance_predicted_surface_clearance: telemetry
                    .avoidance_predicted_surface_clearance,
                avoidance_age_ticks: telemetry.avoidance_age_ticks,
                avoidance_stalled_ticks: telemetry.avoidance_stalled_ticks,
                avoidance_escape_assist: telemetry.avoidance_escape_assist,
                avoidance_emergency_escape_assist: telemetry.avoidance_emergency_escape_assist,
                target_planet: semantic.target_planet,
                port_phase: telemetry.port_phase,
                port_attempt_age_ticks: telemetry.port_attempt_age_ticks,
                port_attempt_stalled_ticks: telemetry.port_attempt_stalled_ticks,
                port_attempt_obstructed_ticks: telemetry.port_attempt_obstructed_ticks,
                port_replan_count: telemetry.port_replan_count,
                cooled_port_planet: telemetry.cooled_port_planet.map(|planet| planet.index()),
                port_cooldown_remaining_ticks: telemetry.port_cooldown_remaining_ticks,
                multi_body_escape_active: telemetry.multi_body_escape_active,
                multi_body_escape_age_ticks: telemetry.multi_body_escape_age_ticks,
                multi_body_escape_body_count: telemetry.multi_body_escape_body_count,
                multi_body_escape_activations: telemetry.multi_body_escape_activations,
                docked_planet,
                focus_planet,
                pending_capture_planet: pending_capture.map(|pending| pending.planet),
                pending_capture_ticks: pending_capture
                    .map(|pending| state.tick.saturating_sub(pending.tick)),
                surface_clearance: geometry.map(|geometry| geometry.surface_clearance),
                outward_speed: geometry.map(|geometry| geometry.outward_speed),
                spaceport_distance: geometry.map(|geometry| geometry.spaceport_distance),
                spaceport_angular_speed: geometry.map(|geometry| geometry.spaceport_angular_speed),
                world_velocity: [ship.velocity.x, ship.velocity.y],
                world_speed: ship.velocity.length(),
                world_rotation: ship.rotation_radians,
                spaceport_rotation: geometry.map(|geometry| geometry.spaceport_rotation),
                angular_velocity: ship.omega,
                measured_angular_speed,
                target_distance: telemetry.target_distance,
                heading_error: telemetry.heading_error,
                desired_speed: telemetry.desired_speed,
                relative_speed: telemetry.relative_speed,
                body_contact: mechanical_body_contacts(state)
                    .iter()
                    .any(|contact| contact.ship == player),
                ship_contact: state
                    .ship_collisions
                    .iter()
                    .any(|contact| contact.a == player || contact.b == player),
                intent: intent.into(),
            });
        }

        if safe_departure {
            self.pending_capture = None;
            self.last_heartbeat_tick = None;
        }
        self.previous_semantic = Some(semantic);
        self.previous_docked_planet = docked_planet;
        self.previous_rotation = ship.rotation_radians;
        self.previous_rotation_tick = state.tick;
    }

    fn capture_transition(&mut self, state: &SpacewarsState) -> Option<usize> {
        debug_assert_eq!(self.previous_planet_owners.len(), state.planets.len());
        let mut captured_planet = None;
        for (planet, (previous_owner, current)) in self
            .previous_planet_owners
            .iter_mut()
            .zip(&state.planets)
            .enumerate()
        {
            if *previous_owner != current.owner_id && current.owner_id == Some(self.player.index())
            {
                captured_planet = Some(planet);
            }
            *previous_owner = current.owner_id;
        }
        captured_planet
    }
}

#[derive(Debug, Clone, Copy)]
struct TracePlanetGeometry {
    surface_clearance: f32,
    outward_speed: f32,
    spaceport_distance: f32,
    spaceport_angular_speed: f32,
    spaceport_rotation: f32,
}

fn trace_planet_geometry(
    state: &SpacewarsState,
    player: PlayerId,
    planet_index: usize,
) -> Option<TracePlanetGeometry> {
    let spaceport_rotation = state.planets.get(planet_index)?.wrapper_angle;
    let observation =
        SpacewarsScenario::observe_ship(state, player, ShipSensorProfile::FullMapRadar)?;
    let planet = observation
        .planets
        .iter()
        .find(|candidate| candidate.id.index() == planet_index)?;
    let distance = planet.local_position.length();
    let outward_speed = if distance > f32::EPSILON {
        planet.local_velocity.dot(planet.local_position / distance)
    } else {
        0.0
    };
    let port_offset = planet.local_spaceport_position - planet.local_position;
    let port_radius_squared = port_offset.length_squared();
    let port_velocity_delta = planet.local_spaceport_velocity - planet.local_velocity;
    let spaceport_angular_speed = if port_radius_squared > f32::EPSILON {
        (port_offset.x * port_velocity_delta.y - port_offset.y * port_velocity_delta.x)
            / port_radius_squared
    } else {
        0.0
    };
    Some(TracePlanetGeometry {
        surface_clearance: distance - planet.radius - observation.own_ship.collision_radius,
        outward_speed,
        spaceport_distance: planet.local_spaceport_position.length(),
        spaceport_angular_speed,
        spaceport_rotation,
    })
}

#[derive(Debug, Clone, Copy)]
struct PlayerHealthTracker {
    previous_form: ShipForm,
    previous_life_fraction: f32,
    damaged_streak: u64,
    critical_streak: u64,
    body_contact_streak: u64,
    metrics: HealthMetrics,
}

impl PlayerHealthTracker {
    fn new(form: ShipForm, life: f32, life_max: f32) -> Self {
        let life_fraction = normalized_health_fraction(life, life_max);
        let mut metrics = HealthMetrics::default();
        if form == ShipForm::Ship {
            metrics.minimum_life_fraction = life_fraction;
        }
        Self {
            previous_form: form,
            previous_life_fraction: life_fraction,
            damaged_streak: 0,
            critical_streak: 0,
            body_contact_streak: 0,
            metrics,
        }
    }

    fn observe(&mut self, form: ShipForm, life: f32, life_max: f32, body_contact: bool) {
        let life_fraction = normalized_health_fraction(life, life_max);
        let mut damage_taken = 0.0;
        match (self.previous_form, form) {
            (ShipForm::Ship, ShipForm::Ship) => {
                let change = life_fraction - self.previous_life_fraction;
                if change < 0.0 {
                    damage_taken = f64::from(-change);
                } else {
                    self.metrics.healing_received_fraction += f64::from(change);
                }
            }
            (ShipForm::Ship, ShipForm::EscapePod) => {
                // Count destruction as consuming the ship's remaining health,
                // but do not count any overkill or later pod rebuild progress.
                damage_taken = f64::from(self.previous_life_fraction.max(0.0));
                self.metrics.minimum_life_fraction = 0.0;
            }
            (ShipForm::EscapePod, ShipForm::Ship | ShipForm::EscapePod) => {}
        }
        self.metrics.damage_taken_fraction += damage_taken;
        if body_contact {
            self.metrics.damage_while_in_body_contact_fraction += damage_taken;
        }

        if form == ShipForm::Ship {
            let previous_ship_ticks = self.metrics.ship_ticks;
            self.metrics.ship_ticks += 1;
            self.metrics.mean_ship_life_fraction = (self.metrics.mean_ship_life_fraction
                * previous_ship_ticks as f64
                + f64::from(life_fraction))
                / self.metrics.ship_ticks as f64;
            self.metrics.minimum_life_fraction =
                self.metrics.minimum_life_fraction.min(life_fraction);
            if life_fraction < 1.0 - f32::EPSILON {
                self.metrics.damaged_ticks += 1;
                self.damaged_streak = self.damaged_streak.saturating_add(1);
                self.metrics.longest_damaged_streak_ticks = self
                    .metrics
                    .longest_damaged_streak_ticks
                    .max(self.damaged_streak);
            } else {
                self.damaged_streak = 0;
            }
            if life_fraction <= 0.5 {
                self.metrics.critical_ticks += 1;
                self.critical_streak = self.critical_streak.saturating_add(1);
                self.metrics.longest_critical_streak_ticks = self
                    .metrics
                    .longest_critical_streak_ticks
                    .max(self.critical_streak);
            } else {
                self.critical_streak = 0;
            }
        } else {
            self.metrics.pod_ticks += 1;
            self.damaged_streak = 0;
            self.critical_streak = 0;
        }

        if body_contact {
            self.body_contact_streak = self.body_contact_streak.saturating_add(1);
            self.metrics.body_contact_ticks += 1;
            self.metrics.longest_body_contact_ticks = self
                .metrics
                .longest_body_contact_ticks
                .max(self.body_contact_streak);
        } else {
            self.body_contact_streak = 0;
        }
        self.previous_form = form;
        self.previous_life_fraction = life_fraction;
    }
}

fn normalized_health_fraction(life: f32, life_max: f32) -> f32 {
    if life_max > 0.0 {
        (life / life_max).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[derive(Debug)]
struct TransitionTracker {
    previous_planet_owners: Vec<Option<usize>>,
    previous_forms: [ShipForm; SPACEWARS_PLAYER_COUNT],
    previous_eliminated: [bool; SPACEWARS_PLAYER_COUNT],
    body_contact_history: Vec<(BodyCollision, u64)>,
    ship_contact_history: Vec<(ShipCollision, u64)>,
    previous_docked_planets: [Option<usize>; SPACEWARS_PLAYER_COUNT],
    pending_capture_departures: [Vec<usize>; SPACEWARS_PLAYER_COUNT],
    pending_rebuild_departures: [Vec<usize>; SPACEWARS_PLAYER_COUNT],
    captures: [u64; SPACEWARS_PLAYER_COUNT],
    ship_losses: [u64; SPACEWARS_PLAYER_COUNT],
    planet_impact_losses: [u64; SPACEWARS_PLAYER_COUNT],
    sun_impact_losses: [u64; SPACEWARS_PLAYER_COUNT],
    rebuilds: [u64; SPACEWARS_PLAYER_COUNT],
    eliminations: [u64; SPACEWARS_PLAYER_COUNT],
    body_contacts: [u64; SPACEWARS_PLAYER_COUNT],
    ship_contacts: [u64; SPACEWARS_PLAYER_COUNT],
    debris_impacts: [u64; SPACEWARS_PLAYER_COUNT],
    laser_hits_received: [u64; SPACEWARS_PLAYER_COUNT],
    port_dockings: [u64; SPACEWARS_PLAYER_COUNT],
    port_departures: [u64; SPACEWARS_PLAYER_COUNT],
    safe_capture_departures: [u64; SPACEWARS_PLAYER_COUNT],
    safe_rebuild_departures: [u64; SPACEWARS_PLAYER_COUNT],
    health: [PlayerHealthTracker; SPACEWARS_PLAYER_COUNT],
}

impl TransitionTracker {
    fn new(state: &SpacewarsState) -> Self {
        Self {
            previous_planet_owners: state.planets.iter().map(|planet| planet.owner_id).collect(),
            previous_forms: std::array::from_fn(|player| state.ships[player].form),
            previous_eliminated: std::array::from_fn(|player| state.players[player].eliminated),
            body_contact_history: mechanical_body_contacts(state)
                .into_iter()
                .map(|contact| (contact, state.tick))
                .collect(),
            ship_contact_history: state
                .ship_collisions
                .iter()
                .copied()
                .map(|contact| (contact, state.tick))
                .collect(),
            previous_docked_planets: std::array::from_fn(|player| docked_planet(state, player)),
            pending_capture_departures: std::array::from_fn(|_| Vec::new()),
            pending_rebuild_departures: std::array::from_fn(|_| Vec::new()),
            captures: [0; SPACEWARS_PLAYER_COUNT],
            ship_losses: [0; SPACEWARS_PLAYER_COUNT],
            planet_impact_losses: [0; SPACEWARS_PLAYER_COUNT],
            sun_impact_losses: [0; SPACEWARS_PLAYER_COUNT],
            rebuilds: [0; SPACEWARS_PLAYER_COUNT],
            eliminations: [0; SPACEWARS_PLAYER_COUNT],
            body_contacts: [0; SPACEWARS_PLAYER_COUNT],
            ship_contacts: [0; SPACEWARS_PLAYER_COUNT],
            debris_impacts: [0; SPACEWARS_PLAYER_COUNT],
            laser_hits_received: [0; SPACEWARS_PLAYER_COUNT],
            port_dockings: [0; SPACEWARS_PLAYER_COUNT],
            port_departures: [0; SPACEWARS_PLAYER_COUNT],
            safe_capture_departures: [0; SPACEWARS_PLAYER_COUNT],
            safe_rebuild_departures: [0; SPACEWARS_PLAYER_COUNT],
            health: std::array::from_fn(|player| {
                let ship = &state.ships[player];
                PlayerHealthTracker::new(ship.form, ship.life, ship.life_max)
            }),
        }
    }

    fn observe(&mut self, state: &SpacewarsState) {
        debug_assert_eq!(self.previous_planet_owners.len(), state.planets.len());
        let mut captured_planets = [None; SPACEWARS_PLAYER_COUNT];
        for (planet_index, (previous, planet)) in self
            .previous_planet_owners
            .iter_mut()
            .zip(&state.planets)
            .enumerate()
        {
            if *previous != planet.owner_id {
                if let Some(owner) = planet
                    .owner_id
                    .filter(|owner| *owner < SPACEWARS_PLAYER_COUNT)
                {
                    self.captures[owner] += 1;
                    captured_planets[owner] = Some(planet_index);
                }
                *previous = planet.owner_id;
            }
        }

        let body_collisions = mechanical_body_contacts(state);
        for player in 0..SPACEWARS_PLAYER_COUNT {
            let ship = &state.ships[player];
            let body_contact = body_collisions.iter().any(|contact| contact.ship == player);
            self.health[player].observe(ship.form, ship.life, ship.life_max, body_contact);
        }
        for contact in
            rearmed_contacts(&body_collisions, &mut self.body_contact_history, state.tick)
        {
            if contact.ship < SPACEWARS_PLAYER_COUNT {
                self.body_contacts[contact.ship] += 1;
            }
        }

        for contact in rearmed_contacts(
            &state.ship_collisions,
            &mut self.ship_contact_history,
            state.tick,
        ) {
            if contact.a < SPACEWARS_PLAYER_COUNT {
                self.ship_contacts[contact.a] += 1;
            }
            if contact.b < SPACEWARS_PLAYER_COUNT {
                self.ship_contacts[contact.b] += 1;
            }
        }

        for impact in &state.ship_debris_collisions {
            if impact.ship < SPACEWARS_PLAYER_COUNT {
                self.debris_impacts[impact.ship] += 1;
            }
        }
        for hit in &state.laser_hits {
            let LaserTarget::Ship(player) = hit.target else {
                continue;
            };
            if player < SPACEWARS_PLAYER_COUNT {
                self.laser_hits_received[player] += 1;
            }
        }

        for (player, &captured_planet) in captured_planets.iter().enumerate() {
            let form = state.ships[player].form;
            let rebuilt = match (self.previous_forms[player], form) {
                (ShipForm::Ship, ShipForm::EscapePod) => {
                    self.ship_losses[player] += 1;
                    if body_collisions.iter().any(|contact| {
                        contact.ship == player && matches!(contact.body, BodyId::Planet(_))
                    }) {
                        self.planet_impact_losses[player] += 1;
                    }
                    if body_collisions.iter().any(|contact| {
                        contact.ship == player && matches!(contact.body, BodyId::Sun)
                    }) {
                        self.sun_impact_losses[player] += 1;
                    }
                    false
                }
                (ShipForm::EscapePod, ShipForm::Ship) => {
                    self.rebuilds[player] += 1;
                    true
                }
                _ => false,
            };
            self.previous_forms[player] = form;

            if let Some(planet) = captured_planet {
                self.pending_capture_departures[player].push(planet);
            }
            if rebuilt && let Some(planet) = docked_planet(state, player) {
                self.pending_rebuild_departures[player].push(planet);
            }
            self.update_docking_transition(state, player);
            self.safe_capture_departures[player] += complete_safe_departures(
                state,
                player,
                &mut self.pending_capture_departures[player],
            );
            self.safe_rebuild_departures[player] += complete_safe_departures(
                state,
                player,
                &mut self.pending_rebuild_departures[player],
            );

            let eliminated = state.players[player].eliminated;
            if eliminated && !self.previous_eliminated[player] {
                self.eliminations[player] += 1;
            }
            self.previous_eliminated[player] = eliminated;
        }
    }

    fn update_docking_transition(&mut self, state: &SpacewarsState, player: usize) {
        let current_planet = docked_planet(state, player);
        let previous_planet = self.previous_docked_planets[player];
        if previous_planet != current_planet {
            if previous_planet.is_some() {
                self.port_departures[player] += 1;
            }
            if current_planet.is_some() {
                self.port_dockings[player] += 1;
            }
        }
        self.previous_docked_planets[player] = current_planet;
    }
}

fn complete_safe_departures(
    state: &SpacewarsState,
    player: usize,
    pending_planets: &mut Vec<usize>,
) -> u64 {
    let current_planet = docked_planet(state, player);
    let mut completed = 0;
    pending_planets.retain(|&planet| {
        let safely_departed =
            current_planet != Some(planet) && has_safe_planet_clearance(state, player, planet);
        completed += u64::from(safely_departed);
        !safely_departed
    });
    completed
}

fn docked_planet(state: &SpacewarsState, player: usize) -> Option<usize> {
    state
        .spaceport_contacts
        .iter()
        .find(|contact| contact.ship == player)
        .map(|contact| contact.planet)
}

fn mechanical_body_contacts(state: &SpacewarsState) -> Vec<BodyCollision> {
    state
        .body_collisions
        .iter()
        .copied()
        .filter(|collision| {
            let BodyId::Planet(planet) = collision.body else {
                return true;
            };
            !state
                .spaceport_contacts
                .iter()
                .any(|contact| contact.ship == collision.ship && contact.planet == planet)
        })
        .collect()
}

fn has_safe_planet_clearance(state: &SpacewarsState, player: usize, planet: usize) -> bool {
    let Some(ship) = state.ships.get(player) else {
        return false;
    };
    let Some(planet) = state.planets.get(planet) else {
        return false;
    };
    let Some(actor) = PlayerId::from_index(player) else {
        return false;
    };
    let Some(observation) =
        SpacewarsScenario::observe_ship(state, actor, ShipSensorProfile::FullMapRadar)
    else {
        return false;
    };
    ship.position.distance_to(planet.position)
        - planet.radius
        - observation.own_ship.collision_radius
        >= NAVIGATION_SAFE_DEPARTURE_CLEARANCE
}

fn rearmed_contacts<T: Copy + Eq>(
    contacts: &[T],
    history: &mut Vec<(T, u64)>,
    tick: u64,
) -> Vec<T> {
    let mut incidents = Vec::new();
    for &contact in contacts {
        if let Some((_, last_seen)) = history.iter_mut().find(|(known, _)| *known == contact) {
            if tick.saturating_sub(*last_seen) > CONTACT_INCIDENT_REARM_TICKS {
                incidents.push(contact);
            }
            *last_seen = tick;
        } else {
            history.push((contact, tick));
            incidents.push(contact);
        }
    }
    history
        .retain(|(_, last_seen)| tick.saturating_sub(*last_seen) <= CONTACT_INCIDENT_REARM_TICKS);
    incidents
}

pub fn run_episode(config: EpisodeConfig) -> Result<EpisodeSummary, RunError> {
    if config.max_ticks == 0 {
        return Err(RunError::ZeroTickLimit);
    }

    let scenario_config = config.preset.config();
    let ticks_per_second = scenario_config.fps;
    let tick_duration = Duration::from_secs_f64(1.0 / f64::from(ticks_per_second));
    let mut state = SpacewarsScenario::init(scenario_config, config.seed);
    let mut controllers = [
        SeatController::new(config.controllers[0], PlayerId::PLAYER_1, config.seed),
        SeatController::new(config.controllers[1], PlayerId::PLAYER_2, config.seed),
    ];
    let controller_descriptors = controllers.each_ref().map(SeatController::descriptor);
    let mut encoder = ShipIntentEncoder::default();
    let mut transitions = TransitionTracker::new(&state);
    let mut strategy_trackers: [StrategyMetricTracker; SPACEWARS_PLAYER_COUNT] =
        std::array::from_fn(|_| StrategyMetricTracker::default());
    let mut navigation_trace = config
        .trace_player
        .map(|player| NavigationTraceCollector::new(&state, player));
    let mut last_traced_decision = None;
    let mut trace = Sha256::new();
    initialize_trace(&mut trace, config, controller_descriptors);
    let mut actions_emitted = 0_u64;

    while state.winner.is_none() && state.tick < config.max_ticks {
        let tick = state.tick;
        let mut actions = Vec::new();
        for (player, controller) in controllers.iter_mut().enumerate() {
            let actor = PlayerId::from_index(player).expect("Spacewars has exactly two players");
            let intent = controller.intent(&state, actor)?;
            let telemetry = controller.telemetry();
            strategy_trackers[player].observe(telemetry);
            if config.trace_player == Some(actor) {
                if let Some(collector) = &mut navigation_trace {
                    collector.observe(&state, telemetry, intent, false);
                }
                last_traced_decision = Some((telemetry, intent));
            }
            actions.extend(encoder.encode(player, intent));
        }
        actions_emitted += actions.len() as u64;
        hash_actions(&mut trace, tick, &actions);
        SpacewarsScenario::step(&mut state, &actions, tick_duration);
        transitions.observe(&state);
    }

    hash_terminal_state(&mut trace, &state);
    let outcome = state
        .winner
        .map_or(EpisodeOutcome::TickLimit, |winner| EpisodeOutcome::Winner {
            player: PlayerId::from_index(winner).expect("winner must be a Spacewars player"),
        });
    let navigation_trace = if let Some(mut collector) = navigation_trace {
        let (telemetry, intent) =
            last_traced_decision.unwrap_or((BrainTelemetry::default(), ShipIntent::default()));
        collector.observe(&state, telemetry, intent, true);
        collector.events
    } else {
        Vec::new()
    };

    Ok(EpisodeSummary {
        seed: config.seed,
        preset: config.preset,
        controllers: controller_descriptors,
        max_ticks: config.max_ticks,
        ticks: state.tick,
        simulated_seconds: state.tick as f64 / f64::from(ticks_per_second),
        outcome,
        captures: transitions.captures,
        ship_losses: transitions.ship_losses,
        planet_impact_losses: transitions.planet_impact_losses,
        sun_impact_losses: transitions.sun_impact_losses,
        rebuilds: transitions.rebuilds,
        eliminations: transitions.eliminations,
        body_contacts: transitions.body_contacts,
        ship_contacts: transitions.ship_contacts,
        debris_impacts: transitions.debris_impacts,
        laser_hits_received: transitions.laser_hits_received,
        port_dockings: transitions.port_dockings,
        port_departures: transitions.port_departures,
        safe_capture_departures: transitions.safe_capture_departures,
        safe_rebuild_departures: transitions.safe_rebuild_departures,
        final_planet_counts: std::array::from_fn(|player| state.players[player].planet_count),
        final_ship_forms: std::array::from_fn(|player| state.ships[player].form),
        final_life_fractions: std::array::from_fn(|player| {
            let ship = &state.ships[player];
            normalized_health_fraction(ship.life, ship.life_max)
        }),
        health: transitions.health.map(|tracker| tracker.metrics),
        strategy: strategy_trackers.map(|tracker| tracker.metrics),
        actions_emitted,
        trace_sha256: format!("{:x}", trace.finalize()),
        navigation_trace,
    })
}

pub fn run_batch(config: BatchConfig) -> Result<RunReport, RunError> {
    if config.episodes == 0 {
        return Err(RunError::NoEpisodes);
    }
    if config.max_ticks == 0 {
        return Err(RunError::ZeroTickLimit);
    }

    let mut episode_configs = Vec::with_capacity(config.episodes as usize);
    for episode_index in 0..config.episodes {
        let offset = config
            .seed_step
            .checked_mul(u64::from(episode_index))
            .ok_or(RunError::SeedOverflow { episode_index })?;
        let seed = config
            .start_seed
            .checked_add(offset)
            .ok_or(RunError::SeedOverflow { episode_index })?;
        episode_configs.push(EpisodeConfig {
            seed,
            preset: config.preset,
            controllers: config.controllers,
            max_ticks: config.max_ticks,
            trace_player: config.trace_player,
        });
    }
    run_episode_configs(None, episode_configs)
}

pub fn run_suite(suite: EvaluationSuite) -> Result<RunReport, RunError> {
    run_episode_configs(Some(suite.id()), suite.episode_configs())
}

pub fn verify_suite_baseline(
    suite: EvaluationSuite,
    report: &RunReport,
) -> Result<BaselineVerification, BaselineVerificationError> {
    let manifest: SuiteBaselineManifest =
        serde_json::from_str(suite.baseline_json()).map_err(|source| {
            BaselineVerificationError::InvalidManifest {
                suite_id: suite.id(),
                source,
            }
        })?;
    if manifest.schema_version != BASELINE_SCHEMA_VERSION {
        return Err(BaselineVerificationError::UnsupportedSchema {
            suite_id: suite.id(),
            expected: BASELINE_SCHEMA_VERSION,
            actual: manifest.schema_version,
        });
    }
    if manifest.suite_id != suite.id() {
        return Err(BaselineVerificationError::ManifestSuiteMismatch {
            expected: suite.id(),
            actual: manifest.suite_id,
        });
    }
    if report.suite_id != Some(suite.id()) {
        return Err(BaselineVerificationError::ReportSuiteMismatch {
            expected: suite.id(),
            actual: report.suite_id,
        });
    }
    if manifest.episodes.len() != report.episodes.len() {
        return Err(BaselineVerificationError::EpisodeCountMismatch {
            suite_id: suite.id(),
            expected: manifest.episodes.len(),
            actual: report.episodes.len(),
        });
    }
    for (index, (expected, episode)) in manifest
        .episodes
        .into_iter()
        .zip(&report.episodes)
        .enumerate()
    {
        let actual = EpisodeFingerprint::from(episode);
        if actual != expected {
            return Err(BaselineVerificationError::EpisodeMismatch {
                suite_id: suite.id(),
                index,
                expected: Box::new(expected),
                actual: Box::new(actual),
            });
        }
    }
    Ok(BaselineVerification {
        schema_version: BASELINE_SCHEMA_VERSION,
        suite_id: suite.id(),
        episodes: report.episodes.len() as u32,
    })
}

pub fn run_policy_comparison(
    config: PolicyComparisonConfig,
) -> Result<PolicyComparisonReport, RunError> {
    if config.episodes == 0 {
        return Err(RunError::NoEpisodes);
    }
    if config.max_ticks == 0 {
        return Err(RunError::ZeroTickLimit);
    }

    let mut episode_configs = Vec::with_capacity((config.episodes as usize).saturating_mul(2));
    for episode_index in 0..config.episodes {
        let offset = config
            .seed_step
            .checked_mul(u64::from(episode_index))
            .ok_or(RunError::SeedOverflow { episode_index })?;
        let seed = config
            .start_seed
            .checked_add(offset)
            .ok_or(RunError::SeedOverflow { episode_index })?;
        for controllers in [
            [config.baseline, config.candidate],
            [config.candidate, config.baseline],
        ] {
            episode_configs.push(EpisodeConfig {
                seed,
                preset: config.preset,
                controllers,
                max_ticks: config.max_ticks,
                trace_player: None,
            });
        }
    }

    let run = run_episode_configs(None, episode_configs)?;
    let baseline_controller = run.episodes[0].controllers[0];
    let candidate_controller = run.episodes[0].controllers[1];
    let mut baseline = ComparedPolicyMetrics::new(baseline_controller);
    let mut candidate = ComparedPolicyMetrics::new(candidate_controller);
    for episodes in run.episodes.chunks_exact(2) {
        baseline.observe(&episodes[0], 0);
        candidate.observe(&episodes[0], 1);
        baseline.observe(&episodes[1], 1);
        candidate.observe(&episodes[1], 0);
    }

    Ok(PolicyComparisonReport {
        schema_version: POLICY_COMPARISON_SCHEMA_VERSION,
        workload_id: config.workload_id,
        summary: PolicyComparisonSummary {
            seed_pairs: config.episodes,
            episode_runs: run.episodes.len() as u32,
            baseline,
            candidate,
        },
        run,
    })
}

fn run_episode_configs(
    suite_id: Option<&'static str>,
    configs: Vec<EpisodeConfig>,
) -> Result<RunReport, RunError> {
    let started = Instant::now();
    let episodes = configs
        .into_iter()
        .map(run_episode)
        .collect::<Result<Vec<_>, _>>()?;
    let wall_seconds = started.elapsed().as_secs_f64();
    let summary = summarize_batch(&episodes, wall_seconds);
    Ok(RunReport {
        schema_version: REPORT_SCHEMA_VERSION,
        suite_id,
        episodes,
        summary,
    })
}

fn summarize_batch(episodes: &[EpisodeSummary], wall_seconds: f64) -> BatchSummary {
    let mut winner_counts = [0_u32; SPACEWARS_PLAYER_COUNT];
    let mut tick_limits = 0_u32;
    let mut captures = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut ship_losses = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut planet_impact_losses = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut sun_impact_losses = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut rebuilds = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut body_contacts = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut ship_contacts = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut debris_impacts = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut laser_hits_received = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut port_dockings = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut port_departures = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut safe_capture_departures = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut safe_rebuild_departures = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut health = [HealthMetrics::default(); SPACEWARS_PLAYER_COUNT];
    let mut strategy = [StrategyMetrics::default(); SPACEWARS_PLAYER_COUNT];
    let mut policy_metrics = BTreeMap::new();
    let mut total_ticks = 0_u64;
    let mut total_simulated_seconds = 0.0;
    for episode in episodes {
        total_ticks += episode.ticks;
        total_simulated_seconds += episode.simulated_seconds;
        match episode.outcome {
            EpisodeOutcome::Winner { player } => winner_counts[player.index()] += 1,
            EpisodeOutcome::TickLimit => tick_limits += 1,
        }
        for player in 0..SPACEWARS_PLAYER_COUNT {
            let controller = episode.controllers[player];
            let policy = policy_metrics
                .entry(controller.policy_id)
                .or_insert_with(|| PolicyMetrics::new(controller));
            policy.seat_episodes += 1;
            policy.ticks += episode.ticks;
            policy.captures += episode.captures[player];
            policy.ship_losses += episode.ship_losses[player];
            policy.planet_impact_losses += episode.planet_impact_losses[player];
            policy.sun_impact_losses += episode.sun_impact_losses[player];
            policy.rebuilds += episode.rebuilds[player];
            policy.body_contacts += episode.body_contacts[player];
            policy.health.add_episode(episode.health[player]);

            captures[player] += episode.captures[player];
            ship_losses[player] += episode.ship_losses[player];
            planet_impact_losses[player] += episode.planet_impact_losses[player];
            sun_impact_losses[player] += episode.sun_impact_losses[player];
            rebuilds[player] += episode.rebuilds[player];
            body_contacts[player] += episode.body_contacts[player];
            ship_contacts[player] += episode.ship_contacts[player];
            debris_impacts[player] += episode.debris_impacts[player];
            laser_hits_received[player] += episode.laser_hits_received[player];
            port_dockings[player] += episode.port_dockings[player];
            port_departures[player] += episode.port_departures[player];
            safe_capture_departures[player] += episode.safe_capture_departures[player];
            safe_rebuild_departures[player] += episode.safe_rebuild_departures[player];
            health[player].add_episode(episode.health[player]);
            strategy[player].add_assign(episode.strategy[player]);
        }
    }

    let measured_seconds = wall_seconds.max(f64::EPSILON);
    BatchSummary {
        episodes: episodes.len() as u32,
        total_ticks,
        total_simulated_seconds,
        winner_counts,
        tick_limits,
        captures,
        ship_losses,
        planet_impact_losses,
        sun_impact_losses,
        rebuilds,
        body_contacts,
        ship_contacts,
        debris_impacts,
        laser_hits_received,
        port_dockings,
        port_departures,
        safe_capture_departures,
        safe_rebuild_departures,
        health,
        strategy,
        policy_metrics: policy_metrics.into_values().collect(),
        wall_seconds,
        ticks_per_wall_second: total_ticks as f64 / measured_seconds,
        simulated_seconds_per_wall_second: total_simulated_seconds / measured_seconds,
    }
}

fn initialize_trace(
    trace: &mut Sha256,
    config: EpisodeConfig,
    controllers: [ControllerDescriptor; SPACEWARS_PLAYER_COUNT],
) {
    trace.update(b"spacewars-episode-trace-v1");
    trace.update(config.seed.to_le_bytes());
    trace.update(config.max_ticks.to_le_bytes());
    trace.update([match config.preset {
        SpacewarsPreset::Standard => 0,
        SpacewarsPreset::StandardNoAsteroids => 1,
        SpacewarsPreset::Navigation => 2,
        SpacewarsPreset::Deathmatch => 3,
    }]);
    for controller in controllers {
        let policy_id = controller.policy_id.as_bytes();
        trace.update((policy_id.len() as u64).to_le_bytes());
        trace.update(policy_id);
    }
}

fn hash_actions(trace: &mut Sha256, tick: u64, actions: &[Action]) {
    trace.update(tick.to_le_bytes());
    trace.update((actions.len() as u64).to_le_bytes());
    for action in actions {
        match action {
            Action::Scenario { kind, payload } => {
                trace.update([0]);
                trace.update(kind.to_le_bytes());
                trace.update((payload.len() as u64).to_le_bytes());
                trace.update(payload);
            }
            Action::Pointer(pointer) => {
                trace.update([1]);
                trace.update(pointer.position.x.to_bits().to_le_bytes());
                trace.update(pointer.position.y.to_bits().to_le_bytes());
                trace.update([match pointer.phase {
                    PointerPhase::Press => 0,
                    PointerPhase::Drag => 1,
                    PointerPhase::Release => 2,
                    PointerPhase::Cancel => 3,
                }]);
            }
        }
    }
}

fn hash_terminal_state(trace: &mut Sha256, state: &SpacewarsState) {
    trace.update(state.tick.to_le_bytes());
    hash_optional_index(trace, state.winner);
    for player in &state.players {
        trace.update((player.planet_count as u64).to_le_bytes());
        trace.update([u8::from(player.eliminated)]);
    }
    for ship in &state.ships {
        trace.update([match ship.form {
            ShipForm::Ship => 0,
            ShipForm::EscapePod => 1,
        }]);
        for value in [
            ship.position.x,
            ship.position.y,
            ship.velocity.x,
            ship.velocity.y,
            ship.rotation_radians,
            ship.omega,
            ship.life,
        ] {
            trace.update(value.to_bits().to_le_bytes());
        }
        trace.update([u8::from(ship.dead)]);
    }
    trace.update((state.planets.len() as u64).to_le_bytes());
    for planet in &state.planets {
        hash_optional_index(trace, planet.owner_id);
        hash_optional_index(trace, planet.capturing_player_id);
        for value in [
            planet.position.x,
            planet.position.y,
            planet.orbit_angle,
            planet.wrapper_angle,
            planet.taking_ownership_time,
            planet.building_new_ship_time,
        ] {
            trace.update(value.to_bits().to_le_bytes());
        }
    }
    trace.update((state.debris.len() as u64).to_le_bytes());
    for debris in &state.debris {
        trace.update([match debris.kind {
            DebrisKind::Asteroid => 0,
            DebrisKind::Fragment => 1,
            DebrisKind::Shell => 2,
        }]);
        for value in [
            debris.position.x,
            debris.position.y,
            debris.velocity.x,
            debris.velocity.y,
            debris.radius,
            debris.life,
        ] {
            trace.update(value.to_bits().to_le_bytes());
        }
        trace.update([u8::from(debris.dead)]);
        hash_optional_index(trace, debris.owner_id);
    }
}

fn hash_optional_index(trace: &mut Sha256, value: Option<usize>) {
    match value {
        Some(value) => {
            trace.update([1]);
            trace.update((value as u64).to_le_bytes());
        }
        None => trace.update([0]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_baselines_match_their_named_suite_contracts() {
        for suite in [EvaluationSuite::NavigationV1, EvaluationSuite::StrategyV1] {
            let manifest: SuiteBaselineManifest =
                serde_json::from_str(suite.baseline_json()).unwrap();
            let configs = suite.episode_configs();

            assert_eq!(manifest.schema_version, BASELINE_SCHEMA_VERSION);
            assert_eq!(manifest.suite_id, suite.id());
            assert_eq!(manifest.episodes.len(), configs.len());
            for (fingerprint, config) in manifest.episodes.iter().zip(configs) {
                assert_eq!(fingerprint.seed, config.seed);
                assert_eq!(fingerprint.preset, config.preset);
                assert_eq!(fingerprint.max_ticks, config.max_ticks);
                assert_eq!(
                    fingerprint.controllers,
                    config
                        .controllers
                        .map(|controller| controller.policy_id().to_owned())
                );
                assert_eq!(fingerprint.trace_sha256.len(), 64);
            }
        }
    }

    #[test]
    fn baseline_verification_rejects_an_incomplete_suite_run() {
        let suite = EvaluationSuite::NavigationV1;
        let report = run_episode_configs(
            Some(suite.id()),
            vec![EpisodeConfig {
                max_ticks: 1,
                ..suite.episode_configs()[0]
            }],
        )
        .unwrap();

        assert!(matches!(
            verify_suite_baseline(suite, &report),
            Err(BaselineVerificationError::EpisodeCountMismatch {
                expected: 6,
                actual: 1,
                ..
            })
        ));
    }

    #[test]
    fn policy_comparison_pairs_each_seed_and_swaps_controller_seats() {
        let report = run_policy_comparison(PolicyComparisonConfig {
            workload_id: Some("test_v1"),
            start_seed: 5,
            seed_step: 2,
            episodes: 2,
            preset: SpacewarsPreset::StandardNoAsteroids,
            baseline: ControllerKind::RuleV5,
            candidate: ControllerKind::Idle,
            max_ticks: 1,
        })
        .unwrap();

        assert_eq!(report.workload_id, Some("test_v1"));
        assert_eq!(report.run.episodes.len(), 4);
        assert_eq!(
            report
                .run
                .episodes
                .iter()
                .map(|episode| episode.seed)
                .collect::<Vec<_>>(),
            vec![5, 5, 7, 7]
        );
        assert_eq!(
            report
                .run
                .episodes
                .iter()
                .map(|episode| episode.controllers.map(|controller| controller.kind))
                .collect::<Vec<_>>(),
            vec![
                [ControllerKind::RuleV5, ControllerKind::Idle],
                [ControllerKind::Idle, ControllerKind::RuleV5],
                [ControllerKind::RuleV5, ControllerKind::Idle],
                [ControllerKind::Idle, ControllerKind::RuleV5],
            ]
        );
        assert_eq!(report.summary.seed_pairs, 2);
        assert_eq!(report.summary.episode_runs, 4);
        assert_eq!(report.summary.baseline.episodes, 4);
        assert_eq!(report.summary.candidate.episodes, 4);
        assert_eq!(
            report.summary.baseline.ticks,
            report.run.summary.total_ticks
        );
        assert_eq!(
            report.summary.candidate.ticks,
            report.run.summary.total_ticks
        );
    }

    #[test]
    fn same_policy_comparison_is_symmetric_between_roles() {
        let report = run_policy_comparison(PolicyComparisonConfig {
            episodes: 2,
            max_ticks: 30,
            ..PolicyComparisonConfig::default()
        })
        .unwrap();

        assert_eq!(report.summary.baseline, report.summary.candidate);
    }

    #[test]
    fn identical_episode_configs_reproduce_the_same_summary_and_trace() {
        let config = EpisodeConfig {
            max_ticks: 120,
            ..EpisodeConfig::default()
        };

        let first = run_episode(config).unwrap();
        let replay = run_episode(config).unwrap();

        assert_eq!(first, replay);
        assert!(first.actions_emitted > 0);
        assert_eq!(first.trace_sha256.len(), 64);
        for metrics in first.strategy {
            assert!(metrics.objective_selections > 0);
            assert_eq!(
                metrics.idle_ticks
                    + metrics.survive_ticks
                    + metrics.attack_ticks
                    + metrics.capture_ticks
                    + metrics.repair_ticks
                    + metrics.defend_ticks
                    + metrics.rebuild_ticks,
                first.ticks
            );
        }
    }

    #[test]
    fn evaluator_reports_the_instantiated_policy_version() {
        for (kind, policy_id) in [
            (ControllerKind::RuleV5, "rule_ship_v5"),
            (ControllerKind::RuleV6, "rule_ship_v6"),
            (ControllerKind::RuleV7, "rule_ship_v7"),
        ] {
            let controller = SeatController::new(kind, PlayerId::PLAYER_2, 9);

            assert_eq!(
                controller.descriptor(),
                ControllerDescriptor { kind, policy_id }
            );
        }
    }

    #[test]
    fn strategy_metrics_count_cumulative_port_replans_once() {
        let mut tracker = StrategyMetricTracker::default();
        for (port_replan_count, multi_body_escape_activations) in
            [(0, 0), (1, 1), (1, 1), (2, 3), (2, 3)]
        {
            tracker.observe(BrainTelemetry {
                port_replan_count,
                multi_body_escape_activations,
                ..BrainTelemetry::default()
            });
        }

        assert_eq!(tracker.metrics.port_replans, 2);
        assert_eq!(tracker.metrics.multi_body_escape_activations, 3);
        assert_eq!(tracker.metrics.objective_selections, 1);
        assert_eq!(tracker.metrics.idle_ticks, 5);
    }

    #[test]
    fn health_metrics_normalize_damage_healing_and_contact_duration() {
        let mut tracker = PlayerHealthTracker::new(ShipForm::Ship, 100.0, 100.0);

        tracker.observe(ShipForm::Ship, 80.0, 100.0, false);
        tracker.observe(ShipForm::Ship, 90.0, 100.0, true);
        tracker.observe(ShipForm::Ship, 40.0, 100.0, true);
        tracker.observe(ShipForm::EscapePod, 0.0, 100.0, true);
        // Pod rebuild progress and the completed rebuild are not ship healing.
        tracker.observe(ShipForm::EscapePod, 50.0, 100.0, false);
        tracker.observe(ShipForm::Ship, 100.0, 100.0, false);

        assert!((tracker.metrics.damage_taken_fraction - 1.1).abs() < 1.0e-6);
        assert!((tracker.metrics.damage_while_in_body_contact_fraction - 0.9).abs() < 1.0e-6);
        assert!((tracker.metrics.healing_received_fraction - 0.1).abs() < 1.0e-6);
        assert_eq!(tracker.metrics.minimum_life_fraction, 0.0);
        assert!((tracker.metrics.mean_ship_life_fraction - 0.775).abs() < 1.0e-6);
        assert_eq!(tracker.metrics.ship_ticks, 4);
        assert_eq!(tracker.metrics.pod_ticks, 2);
        assert_eq!(tracker.metrics.damaged_ticks, 3);
        assert_eq!(tracker.metrics.critical_ticks, 1);
        assert_eq!(tracker.metrics.longest_damaged_streak_ticks, 3);
        assert_eq!(tracker.metrics.longest_critical_streak_ticks, 1);
        assert_eq!(tracker.metrics.body_contact_ticks, 3);
        assert_eq!(tracker.metrics.longest_body_contact_ticks, 3);
    }

    #[test]
    fn aggregate_health_uses_a_ship_tick_weighted_mean() {
        let mut aggregate = HealthMetrics {
            mean_ship_life_fraction: 0.75,
            ship_ticks: 2,
            ..HealthMetrics::default()
        };
        aggregate.add_episode(HealthMetrics {
            mean_ship_life_fraction: 0.25,
            ship_ticks: 1,
            ..HealthMetrics::default()
        });

        assert_eq!(aggregate.ship_ticks, 3);
        assert!((aggregate.mean_ship_life_fraction - 7.0 / 12.0).abs() < 1.0e-12);
    }

    #[test]
    fn enabling_navigation_trace_does_not_change_episode_behavior() {
        let untraced = run_episode(EpisodeConfig {
            max_ticks: 30,
            ..EpisodeConfig::default()
        })
        .unwrap();
        let traced = run_episode(EpisodeConfig {
            max_ticks: 30,
            trace_player: Some(PlayerId::PLAYER_2),
            ..EpisodeConfig::default()
        })
        .unwrap();

        assert_eq!(traced.trace_sha256, untraced.trace_sha256);
        assert_eq!(traced.actions_emitted, untraced.actions_emitted);
        assert!(!traced.navigation_trace.is_empty());
        assert!(
            traced.navigation_trace[0]
                .reasons
                .contains(&NavigationTraceReason::Start)
        );
        assert!(
            traced
                .navigation_trace
                .last()
                .unwrap()
                .reasons
                .contains(&NavigationTraceReason::EpisodeEnd)
        );
    }

    #[test]
    fn asteroid_free_standard_preset_changes_only_the_spawn_rate() {
        let expected = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            ..SpacewarsConfig::default()
        };

        assert_eq!(SpacewarsPreset::StandardNoAsteroids.config(), expected);
    }

    #[test]
    fn navigation_preset_removes_asteroids_and_doubles_ship_health() {
        let config = SpacewarsPreset::Navigation.config();
        let normal_health = SpacewarsConfig::default().players[0].health_percent;

        assert_eq!(config.asteroid_probability_per_sec, 0.0);
        assert_eq!(NAVIGATION_HEALTH_PERCENT, normal_health * 2);
        assert!(
            config
                .players
                .iter()
                .all(|player| player.health_percent == NAVIGATION_HEALTH_PERCENT)
        );
    }

    #[test]
    fn navigation_suite_has_a_fixed_reproducible_contract() {
        let configs = EvaluationSuite::NavigationV1.episode_configs();

        assert_eq!(EvaluationSuite::NavigationV1.id(), NAVIGATION_V1_SUITE_ID);
        assert_eq!(configs.len(), NAVIGATION_V1_SEEDS.len());
        assert_eq!(
            configs.iter().map(|config| config.seed).collect::<Vec<_>>(),
            NAVIGATION_V1_SEEDS
        );
        assert!(configs.iter().all(|config| {
            config.preset == SpacewarsPreset::Navigation
                && config.controllers == [ControllerKind::RuleV5; SPACEWARS_PLAYER_COUNT]
                && config.max_ticks == NAVIGATION_V1_MAX_TICKS
                && config.trace_player.is_none()
        }));
    }

    #[test]
    fn navigation_comparison_reuses_the_named_world_contract() {
        let config = PolicyComparisonProfile::NavigationV1
            .config(ControllerKind::RuleV5, ControllerKind::RuleV6);

        assert_eq!(config.workload_id, Some(NAVIGATION_V1_SUITE_ID));
        assert_eq!(config.start_seed, NAVIGATION_V1_SEEDS[0]);
        assert_eq!(config.episodes, NAVIGATION_V1_SEEDS.len() as u32);
        assert_eq!(config.preset, SpacewarsPreset::Navigation);
        assert_eq!(config.max_ticks, NAVIGATION_V1_MAX_TICKS);
        assert_eq!(config.baseline, ControllerKind::RuleV5);
        assert_eq!(config.candidate, ControllerKind::RuleV6);
    }

    #[test]
    fn strategy_suite_covers_seeds_side_swaps_and_self_play() {
        let configs = EvaluationSuite::StrategyV1.episode_configs();

        assert_eq!(EvaluationSuite::StrategyV1.id(), STRATEGY_V1_SUITE_ID);
        assert_eq!(configs.len(), STRATEGY_V1_SEEDS.len() * 3);
        for (seed, group) in STRATEGY_V1_SEEDS.into_iter().zip(configs.chunks_exact(3)) {
            assert!(group.iter().all(|config| {
                config.seed == seed
                    && config.preset == SpacewarsPreset::StandardNoAsteroids
                    && config.max_ticks == STRATEGY_V1_MAX_TICKS
                    && config.trace_player.is_none()
            }));
            assert_eq!(
                group
                    .iter()
                    .map(|config| config.controllers)
                    .collect::<Vec<_>>(),
                vec![
                    [ControllerKind::RuleV5, ControllerKind::Idle],
                    [ControllerKind::Idle, ControllerKind::RuleV5],
                    [ControllerKind::RuleV5, ControllerKind::RuleV5],
                ]
            );
        }
    }

    #[test]
    fn navigation_trace_records_capture_heartbeat_and_safe_departure() {
        let mut state = SpacewarsScenario::init(SpacewarsConfig::default(), 12);
        let actor = PlayerId::PLAYER_2;
        let mut trace = NavigationTraceCollector::new(&state, actor);
        let telemetry = |phase| BrainTelemetry {
            actor: Some(actor),
            goal: BrainGoal::Capture,
            target_planet: scenario_spacewars::PlanetId::from_index(0),
            port_phase: Some(phase),
            ..BrainTelemetry::default()
        };

        state.tick = 10;
        state
            .spaceport_contacts
            .push(scenario_spacewars::SpaceportContact { ship: 1, planet: 0 });
        trace.observe(
            &state,
            telemetry(PortNavigationPhase::Docked),
            ShipIntent::default(),
            false,
        );

        state.tick = 20;
        state.planets[0].owner_id = Some(1);
        trace.observe(
            &state,
            telemetry(PortNavigationPhase::Docked),
            ShipIntent::default(),
            false,
        );

        state.tick = 20 + NAVIGATION_TRACE_HEARTBEAT_TICKS;
        trace.observe(
            &state,
            telemetry(PortNavigationPhase::Docked),
            ShipIntent::default(),
            false,
        );

        state.tick += 1;
        state.spaceport_contacts.clear();
        state.ships[1].position = state.planets[0].position + engine_core::Vec2::X * 1_000.0;
        trace.observe(
            &state,
            telemetry(PortNavigationPhase::Depart),
            ShipIntent {
                thrust: 1.0,
                wings_closed: true,
                ..ShipIntent::default()
            },
            false,
        );

        assert!(trace.events.iter().any(|event| {
            event.reasons.contains(&NavigationTraceReason::Capture)
                && event.pending_capture_ticks == Some(0)
        }));
        assert!(trace.events.iter().any(|event| {
            event.reasons.contains(&NavigationTraceReason::Heartbeat)
                && event.pending_capture_ticks == Some(NAVIGATION_TRACE_HEARTBEAT_TICKS)
        }));
        let departure = trace
            .events
            .iter()
            .find(|event| {
                event
                    .reasons
                    .contains(&NavigationTraceReason::SafeDeparture)
            })
            .unwrap();
        assert_eq!(departure.pending_capture_planet, Some(0));
        assert!(departure.surface_clearance.unwrap() >= NAVIGATION_SAFE_DEPARTURE_CLEARANCE);
        assert_eq!(departure.intent.thrust, 1.0);
        assert!(trace.pending_capture.is_none());
    }

    #[test]
    fn seed_four_player_two_safely_departs_within_bounded_episode() {
        let episode = run_episode(EpisodeConfig {
            seed: 4,
            preset: SpacewarsPreset::Navigation,
            controllers: [ControllerKind::RuleV5; SPACEWARS_PLAYER_COUNT],
            max_ticks: 4_000,
            trace_player: Some(PlayerId::PLAYER_2),
        })
        .unwrap();

        assert_eq!(episode.outcome, EpisodeOutcome::TickLimit);
        let player = PlayerId::PLAYER_2.index();
        assert!(episode.captures[player] >= 1);
        assert_eq!(
            episode.safe_capture_departures[player], episode.captures[player],
            "every capture should be followed by safe clearance"
        );

        let capture = episode
            .navigation_trace
            .iter()
            .find(|event| event.reasons.contains(&NavigationTraceReason::Capture))
            .unwrap();
        assert_eq!(capture.docked_planet, Some(3));
        assert_eq!(capture.pending_capture_planet, Some(3));
        assert_eq!(capture.pending_capture_ticks, Some(0));

        let departure = episode
            .navigation_trace
            .iter()
            .find(|event| {
                event
                    .reasons
                    .contains(&NavigationTraceReason::SafeDeparture)
                    && event.pending_capture_planet == Some(3)
            })
            .expect("Player 2 should clear planet 3 after capturing it");
        assert!(departure.tick > capture.tick);
        assert!(departure.tick - capture.tick <= NAVIGATION_TRACE_HEARTBEAT_TICKS);
        assert!(departure.surface_clearance.unwrap() >= NAVIGATION_SAFE_DEPARTURE_CLEARANCE);
        assert_eq!(departure.intent.brake, 0.0);
        assert_eq!(departure.intent.thrust, 1.0);

        let terminal = episode.navigation_trace.last().unwrap();
        assert!(
            terminal
                .reasons
                .contains(&NavigationTraceReason::EpisodeEnd)
        );
        assert_eq!(terminal.pending_capture_planet, None);
        assert_eq!(terminal.pending_capture_ticks, None);
    }

    #[test]
    fn batch_walks_seeds_and_reports_tick_limits() {
        let report = run_batch(BatchConfig {
            start_seed: 5,
            seed_step: 3,
            episodes: 3,
            controllers: [ControllerKind::Idle; SPACEWARS_PLAYER_COUNT],
            max_ticks: 1,
            ..BatchConfig::default()
        })
        .unwrap();

        assert_eq!(
            report
                .episodes
                .iter()
                .map(|episode| episode.seed)
                .collect::<Vec<_>>(),
            vec![5, 8, 11]
        );
        assert_eq!(report.schema_version, REPORT_SCHEMA_VERSION);
        assert_eq!(report.summary.total_ticks, 3);
        assert_eq!(report.summary.tick_limits, 3);
        assert_eq!(report.summary.winner_counts, [0, 0]);
    }

    #[test]
    fn transition_metrics_count_captures_losses_rebuilds_and_eliminations() {
        let mut state = SpacewarsScenario::init(SpacewarsConfig::default(), 9);
        let mut tracker = TransitionTracker::new(&state);
        assert!(!state.planets.is_empty());

        state
            .spaceport_contacts
            .push(scenario_spacewars::SpaceportContact { ship: 1, planet: 0 });
        tracker.observe(&state);

        state.planets[0].owner_id = Some(1);
        state.ships[1].form = ShipForm::EscapePod;
        state.players[1].eliminated = true;
        tracker.observe(&state);

        state.ships[1].form = ShipForm::Ship;
        state.players[1].eliminated = false;
        tracker.observe(&state);

        state.spaceport_contacts.clear();
        state.ships[1].position = state.planets[0].position + engine_core::Vec2::X * 1_000.0;
        tracker.observe(&state);

        assert_eq!(tracker.captures, [0, 1]);
        assert_eq!(tracker.ship_losses, [0, 1]);
        assert_eq!(tracker.planet_impact_losses, [0, 0]);
        assert_eq!(tracker.sun_impact_losses, [0, 0]);
        assert_eq!(tracker.rebuilds, [0, 1]);
        assert_eq!(tracker.eliminations, [0, 1]);
        assert_eq!(tracker.port_dockings, [0, 1]);
        assert_eq!(tracker.port_departures, [0, 1]);
        assert_eq!(tracker.safe_capture_departures, [0, 1]);
        assert_eq!(tracker.safe_rebuild_departures, [0, 1]);
    }

    #[test]
    fn loss_metrics_attribute_same_tick_planet_and_sun_impacts() {
        let mut state = SpacewarsScenario::init(SpacewarsConfig::default(), 27);
        let mut tracker = TransitionTracker::new(&state);

        state.body_collisions = vec![
            BodyCollision {
                ship: 0,
                body: BodyId::Planet(0),
            },
            BodyCollision {
                ship: 1,
                body: BodyId::Sun,
            },
        ];
        state.ships[0].form = ShipForm::EscapePod;
        state.ships[1].form = ShipForm::EscapePod;
        tracker.observe(&state);

        assert_eq!(tracker.ship_losses, [1, 1]);
        assert_eq!(tracker.planet_impact_losses, [1, 0]);
        assert_eq!(tracker.sun_impact_losses, [0, 1]);
    }

    #[test]
    fn leaving_an_already_owned_planet_is_not_a_capture_departure() {
        let mut state = SpacewarsScenario::init(SpacewarsConfig::default(), 11);
        state.planets[0].owner_id = Some(0);
        let mut tracker = TransitionTracker::new(&state);

        state
            .spaceport_contacts
            .push(scenario_spacewars::SpaceportContact { ship: 0, planet: 0 });
        state.body_collisions.push(BodyCollision {
            ship: 0,
            body: BodyId::Planet(0),
        });
        tracker.observe(&state);
        state.spaceport_contacts.clear();
        state.body_collisions.clear();
        state.ships[0].position = state.planets[0].position + engine_core::Vec2::X * 1_000.0;
        tracker.observe(&state);

        assert_eq!(tracker.port_dockings, [1, 0]);
        assert_eq!(tracker.port_departures, [1, 0]);
        assert_eq!(tracker.safe_capture_departures, [0, 0]);
        assert_eq!(tracker.body_contacts, [0, 0]);
    }

    #[test]
    fn back_to_back_captures_keep_independent_safe_departures() {
        let mut state = SpacewarsScenario::init(SpacewarsConfig::default(), 12);
        let mut tracker = TransitionTracker::new(&state);
        state.planets[0].position = engine_core::Vec2::ZERO;
        state.planets[1].position = engine_core::Vec2::X * 20.0;
        state.ships[0].position = engine_core::Vec2::ZERO;

        state.spaceport_contacts =
            vec![scenario_spacewars::SpaceportContact { ship: 0, planet: 0 }];
        state.planets[0].owner_id = Some(0);
        tracker.observe(&state);

        // Reach and capture the next nearby port before attaining the first
        // port's 90-unit safe-clearance threshold.
        state.spaceport_contacts =
            vec![scenario_spacewars::SpaceportContact { ship: 0, planet: 1 }];
        state.planets[1].owner_id = Some(0);
        tracker.observe(&state);
        assert_eq!(tracker.captures, [2, 0]);
        assert_eq!(tracker.safe_capture_departures, [0, 0]);

        state.spaceport_contacts.clear();
        state.ships[0].position = engine_core::Vec2::X * 2_000.0;
        tracker.observe(&state);

        assert_eq!(tracker.port_dockings, [2, 0]);
        assert_eq!(tracker.port_departures, [2, 0]);
        assert_eq!(tracker.safe_capture_departures, [2, 0]);
    }

    #[test]
    fn contact_metrics_count_incidents_instead_of_contact_ticks() {
        let mut state = SpacewarsScenario::init(SpacewarsConfig::default(), 10);
        let mut tracker = TransitionTracker::new(&state);

        state.body_collisions.push(BodyCollision {
            ship: 0,
            body: scenario_spacewars::BodyId::Sun,
        });
        state.ship_collisions.push(ShipCollision { a: 0, b: 1 });
        state
            .ship_debris_collisions
            .push(scenario_spacewars::ShipDebrisCollision { ship: 0, debris: 0 });
        state.laser_hits.push(scenario_spacewars::LaserHit {
            shooter: 0,
            target: LaserTarget::Ship(1),
            point: engine_core::Vec2::ZERO,
            damage: 1.0,
        });
        tracker.observe(&state);

        state.ship_debris_collisions.clear();
        state.laser_hits.clear();
        tracker.observe(&state);

        assert_eq!(tracker.body_contacts, [1, 0]);
        assert_eq!(tracker.ship_contacts, [1, 1]);
        assert_eq!(tracker.debris_impacts, [1, 0]);
        assert_eq!(tracker.laser_hits_received, [0, 1]);

        state.body_collisions.clear();
        state.ship_collisions.clear();
        tracker.observe(&state);
        state.tick += CONTACT_INCIDENT_REARM_TICKS + 1;
        state.body_collisions.push(BodyCollision {
            ship: 0,
            body: scenario_spacewars::BodyId::Sun,
        });
        state.ship_collisions.push(ShipCollision { a: 0, b: 1 });
        tracker.observe(&state);

        assert_eq!(tracker.body_contacts, [2, 0]);
        assert_eq!(tracker.ship_contacts, [2, 2]);
    }

    #[test]
    fn reports_are_machine_serializable() {
        let report = run_batch(BatchConfig {
            controllers: [ControllerKind::Idle, ControllerKind::RuleV5],
            max_ticks: 1,
            ..BatchConfig::default()
        })
        .unwrap();
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"schema_version\":4"));
        assert!(json.contains("\"policy_id\":\"idle_v1\""));
        assert!(json.contains("\"kind\":\"rule_v5\""));
        assert!(json.contains("\"policy_id\":\"rule_ship_v5\""));
        assert!(json.contains("\"trace_sha256\""));
        assert!(json.contains("\"objective_selections\""));
        assert!(json.contains("\"port_replans\""));
        assert!(json.contains("\"policy_metrics\""));
        assert!(json.contains("\"planet_impact_losses\""));
        assert!(json.contains("\"damage_taken_fraction\""));
        assert!(json.contains("\"mean_ship_life_fraction\""));
        assert!(json.contains("\"pod_ticks\""));
        assert!(json.contains("\"longest_body_contact_ticks\""));
    }

    #[test]
    fn batch_metrics_aggregate_losses_by_controller_policy() {
        let report = run_batch(BatchConfig {
            controllers: [ControllerKind::Idle, ControllerKind::RuleV5],
            episodes: 2,
            max_ticks: 1,
            ..BatchConfig::default()
        })
        .unwrap();

        let idle = report
            .summary
            .policy_metrics
            .iter()
            .find(|metrics| metrics.policy_id == ControllerKind::Idle.policy_id())
            .unwrap();
        let rule = report
            .summary
            .policy_metrics
            .iter()
            .find(|metrics| metrics.policy_id == ControllerKind::RuleV5.policy_id())
            .unwrap();
        assert_eq!(idle.seat_episodes, 2);
        assert_eq!(idle.ticks, 2);
        assert_eq!(rule.seat_episodes, 2);
        assert_eq!(rule.ticks, 2);
        assert_eq!(idle.health, report.summary.health[0]);
        assert_eq!(rule.health, report.summary.health[1]);
    }

    #[test]
    fn invalid_batch_limits_are_rejected() {
        assert_eq!(
            run_batch(BatchConfig {
                episodes: 0,
                ..BatchConfig::default()
            }),
            Err(RunError::NoEpisodes)
        );
        assert_eq!(
            run_batch(BatchConfig {
                max_ticks: 0,
                ..BatchConfig::default()
            }),
            Err(RunError::ZeroTickLimit)
        );
    }
}
