//! Deterministic, headless episode execution for Spacewars controllers.
//!
//! The evaluator owns authoritative scenario state for measurement, while
//! controllers receive only typed observations and emit canonical actions.

#![forbid(unsafe_code)]

use std::{
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
use serde::Serialize;
use sha2::{Digest, Sha256};
use spacewars_ai::{
    BrainGoal, BrainReset, BrainTelemetry, PortNavigationPhase, RULE_SHIP_BRAIN_POLICY_ID,
    RuleShipBrain, ShipBrain,
};

pub const REPORT_SCHEMA_VERSION: u32 = 1;
pub const NAVIGATION_V1_SUITE_ID: &str = "navigation_v1";
pub const NAVIGATION_V1_SEEDS: [u64; 6] = [0, 1, 2, 3, 4, 5];
pub const NAVIGATION_V1_MAX_TICKS: u64 = 36_000;
const NAVIGATION_HEALTH_PERCENT: u32 = 100_000;
const NAVIGATION_SAFE_DEPARTURE_CLEARANCE: f32 = 90.0;
const CONTACT_INCIDENT_REARM_TICKS: u64 = 30;
const NAVIGATION_TRACE_HEARTBEAT_TICKS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerKind {
    Idle,
    Rule,
}

impl ControllerKind {
    pub const fn policy_id(self) -> &'static str {
        match self {
            Self::Idle => "idle_v1",
            Self::Rule => RULE_SHIP_BRAIN_POLICY_ID,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
}

impl EvaluationSuite {
    pub const fn id(self) -> &'static str {
        match self {
            Self::NavigationV1 => NAVIGATION_V1_SUITE_ID,
        }
    }

    pub fn episode_configs(self) -> Vec<EpisodeConfig> {
        match self {
            Self::NavigationV1 => NAVIGATION_V1_SEEDS
                .into_iter()
                .map(|seed| EpisodeConfig {
                    seed,
                    preset: SpacewarsPreset::Navigation,
                    controllers: [ControllerKind::Rule; SPACEWARS_PLAYER_COUNT],
                    max_ticks: NAVIGATION_V1_MAX_TICKS,
                    trace_player: None,
                })
                .collect(),
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
            controllers: [ControllerKind::Idle, ControllerKind::Rule],
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
    pub goal: BrainGoal,
    pub target_planet: Option<usize>,
    pub port_phase: Option<PortNavigationPhase>,
    pub docked_planet: Option<usize>,
    pub focus_planet: Option<usize>,
    pub pending_capture_planet: Option<usize>,
    pub pending_capture_ticks: Option<u64>,
    pub surface_clearance: Option<f32>,
    pub outward_speed: Option<f32>,
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
    pub rebuilds: [u64; SPACEWARS_PLAYER_COUNT],
    pub body_contacts: [u64; SPACEWARS_PLAYER_COUNT],
    pub ship_contacts: [u64; SPACEWARS_PLAYER_COUNT],
    pub debris_impacts: [u64; SPACEWARS_PLAYER_COUNT],
    pub laser_hits_received: [u64; SPACEWARS_PLAYER_COUNT],
    pub port_dockings: [u64; SPACEWARS_PLAYER_COUNT],
    pub port_departures: [u64; SPACEWARS_PLAYER_COUNT],
    pub safe_capture_departures: [u64; SPACEWARS_PLAYER_COUNT],
    pub safe_rebuild_departures: [u64; SPACEWARS_PLAYER_COUNT],
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
    Rule(RuleShipBrain),
}

impl SeatController {
    fn new(kind: ControllerKind, actor: PlayerId, episode_seed: u64) -> Self {
        match kind {
            ControllerKind::Idle => Self::Idle,
            ControllerKind::Rule => {
                let mut brain = RuleShipBrain::default();
                brain.reset(BrainReset {
                    actor,
                    episode_seed,
                });
                Self::Rule(brain)
            }
        }
    }

    fn intent(&mut self, state: &SpacewarsState, actor: PlayerId) -> Result<ShipIntent, RunError> {
        match self {
            Self::Idle => Ok(ShipIntent::default()),
            Self::Rule(brain) => {
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
            Self::Rule(brain) => brain.telemetry(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NavigationTraceSemantic {
    goal: BrainGoal,
    target_planet: Option<usize>,
    port_phase: Option<PortNavigationPhase>,
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
            goal: telemetry.goal,
            target_planet: telemetry.target_planet.map(|planet| planet.index()),
            port_phase: telemetry.port_phase,
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
                goal: telemetry.goal,
                target_planet: semantic.target_planet,
                port_phase: telemetry.port_phase,
                docked_planet,
                focus_planet,
                pending_capture_planet: pending_capture.map(|pending| pending.planet),
                pending_capture_ticks: pending_capture
                    .map(|pending| state.tick.saturating_sub(pending.tick)),
                surface_clearance: geometry.map(|geometry| geometry.surface_clearance),
                outward_speed: geometry.map(|geometry| geometry.outward_speed),
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
        spaceport_angular_speed,
        spaceport_rotation,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DockingSession {
    planet: usize,
    captured: bool,
    rebuilt: bool,
    in_contact: bool,
}

#[derive(Debug)]
struct TransitionTracker {
    previous_planet_owners: Vec<Option<usize>>,
    previous_forms: [ShipForm; SPACEWARS_PLAYER_COUNT],
    previous_eliminated: [bool; SPACEWARS_PLAYER_COUNT],
    body_contact_history: Vec<(BodyCollision, u64)>,
    ship_contact_history: Vec<(ShipCollision, u64)>,
    docking_sessions: [Option<DockingSession>; SPACEWARS_PLAYER_COUNT],
    captures: [u64; SPACEWARS_PLAYER_COUNT],
    ship_losses: [u64; SPACEWARS_PLAYER_COUNT],
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
            docking_sessions: std::array::from_fn(|player| {
                docked_planet(state, player).map(|planet| DockingSession {
                    planet,
                    captured: false,
                    rebuilt: false,
                    in_contact: true,
                })
            }),
            captures: [0; SPACEWARS_PLAYER_COUNT],
            ship_losses: [0; SPACEWARS_PLAYER_COUNT],
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
                    false
                }
                (ShipForm::EscapePod, ShipForm::Ship) => {
                    self.rebuilds[player] += 1;
                    true
                }
                _ => false,
            };
            self.previous_forms[player] = form;

            self.update_docking_session(state, player, rebuilt, captured_planet);

            let eliminated = state.players[player].eliminated;
            if eliminated && !self.previous_eliminated[player] {
                self.eliminations[player] += 1;
            }
            self.previous_eliminated[player] = eliminated;
        }
    }

    fn update_docking_session(
        &mut self,
        state: &SpacewarsState,
        player: usize,
        rebuilt: bool,
        captured_planet: Option<usize>,
    ) {
        let current_planet = docked_planet(state, player);
        let mut completed = None;
        if let Some(session) = &mut self.docking_sessions[player] {
            session.captured |= captured_planet == Some(session.planet);
            session.rebuilt |= rebuilt;

            let in_contact = current_planet == Some(session.planet);
            if session.in_contact && !in_contact {
                self.port_departures[player] += 1;
            } else if !session.in_contact && in_contact {
                self.port_dockings[player] += 1;
            }
            session.in_contact = in_contact;

            if !in_contact && has_safe_planet_clearance(state, player, session.planet) {
                completed = Some(*session);
            }
        }

        if let Some(completed) = completed {
            self.safe_capture_departures[player] += u64::from(completed.captured);
            self.safe_rebuild_departures[player] += u64::from(completed.rebuilt);
            self.docking_sessions[player] = None;
        }

        if self.docking_sessions[player].is_some() {
            return;
        }
        let Some(planet) = current_planet else {
            return;
        };
        self.port_dockings[player] += 1;
        self.docking_sessions[player] = Some(DockingSession {
            planet,
            captured: captured_planet == Some(planet),
            rebuilt,
            in_contact: true,
        });
    }
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
    let mut encoder = ShipIntentEncoder::default();
    let mut transitions = TransitionTracker::new(&state);
    let mut navigation_trace = config
        .trace_player
        .map(|player| NavigationTraceCollector::new(&state, player));
    let mut last_traced_decision = None;
    let mut trace = Sha256::new();
    initialize_trace(&mut trace, config);
    let mut actions_emitted = 0_u64;

    while state.winner.is_none() && state.tick < config.max_ticks {
        let tick = state.tick;
        let mut actions = Vec::new();
        for (player, controller) in controllers.iter_mut().enumerate() {
            let actor = PlayerId::from_index(player).expect("Spacewars has exactly two players");
            let intent = controller.intent(&state, actor)?;
            if config.trace_player == Some(actor) {
                let telemetry = controller.telemetry();
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
        controllers: config.controllers.map(ControllerDescriptor::from),
        max_ticks: config.max_ticks,
        ticks: state.tick,
        simulated_seconds: state.tick as f64 / f64::from(ticks_per_second),
        outcome,
        captures: transitions.captures,
        ship_losses: transitions.ship_losses,
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
            if ship.life_max > 0.0 {
                (ship.life / ship.life_max).clamp(0.0, 1.0)
            } else {
                0.0
            }
        }),
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
    let mut rebuilds = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut body_contacts = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut ship_contacts = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut debris_impacts = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut laser_hits_received = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut port_dockings = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut port_departures = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut safe_capture_departures = [0_u64; SPACEWARS_PLAYER_COUNT];
    let mut safe_rebuild_departures = [0_u64; SPACEWARS_PLAYER_COUNT];
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
            captures[player] += episode.captures[player];
            ship_losses[player] += episode.ship_losses[player];
            rebuilds[player] += episode.rebuilds[player];
            body_contacts[player] += episode.body_contacts[player];
            ship_contacts[player] += episode.ship_contacts[player];
            debris_impacts[player] += episode.debris_impacts[player];
            laser_hits_received[player] += episode.laser_hits_received[player];
            port_dockings[player] += episode.port_dockings[player];
            port_departures[player] += episode.port_departures[player];
            safe_capture_departures[player] += episode.safe_capture_departures[player];
            safe_rebuild_departures[player] += episode.safe_rebuild_departures[player];
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
        rebuilds,
        body_contacts,
        ship_contacts,
        debris_impacts,
        laser_hits_received,
        port_dockings,
        port_departures,
        safe_capture_departures,
        safe_rebuild_departures,
        wall_seconds,
        ticks_per_wall_second: total_ticks as f64 / measured_seconds,
        simulated_seconds_per_wall_second: total_simulated_seconds / measured_seconds,
    }
}

fn initialize_trace(trace: &mut Sha256, config: EpisodeConfig) {
    trace.update(b"spacewars-episode-trace-v1");
    trace.update(config.seed.to_le_bytes());
    trace.update(config.max_ticks.to_le_bytes());
    trace.update([match config.preset {
        SpacewarsPreset::Standard => 0,
        SpacewarsPreset::StandardNoAsteroids => 1,
        SpacewarsPreset::Navigation => 2,
        SpacewarsPreset::Deathmatch => 3,
    }]);
    for controller in config.controllers {
        let policy_id = controller.policy_id().as_bytes();
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
    fn navigation_preset_removes_asteroids_and_raises_ship_health() {
        let config = SpacewarsPreset::Navigation.config();

        assert_eq!(config.asteroid_probability_per_sec, 0.0);
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
                && config.controllers == [ControllerKind::Rule; SPACEWARS_PLAYER_COUNT]
                && config.max_ticks == NAVIGATION_V1_MAX_TICKS
                && config.trace_player.is_none()
        }));
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
    fn seed_four_reproduces_player_two_departure_deadlock() {
        // Characterize the known failure before changing departure guidance.
        // Once repaired, this fixture should be inverted to require a safe
        // departure within the same bounded episode.
        let episode = run_episode(EpisodeConfig {
            seed: 4,
            preset: SpacewarsPreset::Navigation,
            controllers: [ControllerKind::Rule; SPACEWARS_PLAYER_COUNT],
            max_ticks: 4_000,
            trace_player: Some(PlayerId::PLAYER_2),
        })
        .unwrap();

        assert_eq!(episode.outcome, EpisodeOutcome::TickLimit);
        assert_eq!(episode.captures[PlayerId::PLAYER_2.index()], 1);
        assert_eq!(
            episode.safe_capture_departures[PlayerId::PLAYER_2.index()],
            0
        );

        let capture = episode
            .navigation_trace
            .iter()
            .find(|event| event.reasons.contains(&NavigationTraceReason::Capture))
            .unwrap();
        assert_eq!(capture.tick, 2_714);
        assert_eq!(capture.docked_planet, Some(3));
        assert_eq!(capture.pending_capture_planet, Some(3));
        assert_eq!(capture.pending_capture_ticks, Some(0));

        let terminal = episode.navigation_trace.last().unwrap();
        assert!(
            terminal
                .reasons
                .contains(&NavigationTraceReason::EpisodeEnd)
        );
        assert_eq!(terminal.port_phase, Some(PortNavigationPhase::Depart));
        assert_eq!(terminal.docked_planet, Some(3));
        assert_eq!(terminal.pending_capture_planet, Some(3));
        assert_eq!(terminal.pending_capture_ticks, Some(1_286));
        assert!(terminal.surface_clearance.unwrap() < 0.0);
        assert_eq!(terminal.intent.turn, -1.0);
        assert_eq!(terminal.intent.brake, 1.0);
        assert_eq!(terminal.intent.thrust, 0.0);
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
        assert_eq!(tracker.rebuilds, [0, 1]);
        assert_eq!(tracker.eliminations, [0, 1]);
        assert_eq!(tracker.port_dockings, [0, 1]);
        assert_eq!(tracker.port_departures, [0, 1]);
        assert_eq!(tracker.safe_capture_departures, [0, 1]);
        assert_eq!(tracker.safe_rebuild_departures, [0, 1]);
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
            controllers: [ControllerKind::Idle; SPACEWARS_PLAYER_COUNT],
            max_ticks: 1,
            ..BatchConfig::default()
        })
        .unwrap();
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("\"policy_id\":\"idle_v1\""));
        assert!(json.contains("\"trace_sha256\""));
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
