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
pub const RULE_SHIP_BRAIN_POLICY_ID: &str = "rule_ship_v1";

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
    Rebuild,
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BrainTelemetry {
    pub actor: Option<PlayerId>,
    pub goal: BrainGoal,
    pub target: Option<PlayerId>,
    pub target_planet: Option<PlanetId>,
    pub port_phase: Option<PortNavigationPhase>,
    pub hazard: Option<DebrisId>,
    pub target_distance: f32,
    pub heading_error: f32,
    pub desired_speed: f32,
    pub relative_speed: f32,
}

impl Default for BrainTelemetry {
    fn default() -> Self {
        Self {
            actor: None,
            goal: BrainGoal::Idle,
            target: None,
            target_planet: None,
            port_phase: None,
            hazard: None,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuleShipBrainConfig {
    pub body_clearance: f32,
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
    pub spaceport_cruise_distance: f32,
    pub spaceport_approach_speed: f32,
    pub spaceport_ingress_speed: f32,
    pub spaceport_departure_speed: f32,
    pub spaceport_departure_alignment_radians: f32,
}

impl Default for RuleShipBrainConfig {
    fn default() -> Self {
        Self {
            body_clearance: 90.0,
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
            spaceport_cruise_distance: 650.0,
            spaceport_approach_speed: 120.0,
            spaceport_ingress_speed: 36.0,
            spaceport_departure_speed: 80.0,
            spaceport_departure_alignment_radians: 0.35,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PortNavigation {
    goal: BrainGoal,
    planet: PlanetId,
    phase: PortNavigationPhase,
    departure_burn_started: bool,
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
    port_navigation: Option<PortNavigation>,
}

impl RuleShipBrain {
    pub fn new(config: RuleShipBrainConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    fn set_telemetry(&mut self, telemetry: BrainTelemetry) {
        self.telemetry = BrainTelemetry {
            actor: self.actor,
            ..telemetry
        };
    }

    fn select_port_navigation(&self, observation: &ShipObservationV1) -> Option<PortNavigation> {
        let goal = match observation.own_ship.form {
            ShipForm::Ship => BrainGoal::Capture,
            ShipForm::EscapePod => BrainGoal::Rebuild,
        };
        observation
            .planets
            .iter()
            .filter(|planet| match goal {
                BrainGoal::Capture => planet.owner != Some(observation.actor),
                BrainGoal::Rebuild => planet.owner == Some(observation.actor),
                _ => false,
            })
            .min_by(|left, right| {
                let clearance =
                    observation.own_ship.collision_radius + self.config.spaceport_staging_margin;
                let left_distance = left.spaceport_approach(clearance).local_position.length();
                let right_distance = right.spaceport_approach(clearance).local_position.length();
                left_distance
                    .total_cmp(&right_distance)
                    .then_with(|| left.id.cmp(&right.id))
            })
            .map(|planet| PortNavigation {
                goal,
                planet: planet.id,
                phase: PortNavigationPhase::Rendezvous,
                departure_burn_started: false,
            })
    }

    fn refresh_port_navigation(&mut self, observation: &ShipObservationV1) {
        if let Some(docked_planet) = observation.own_ship.docked_planet {
            let already_tracking_contact = self
                .port_navigation
                .is_some_and(|navigation| navigation.planet == docked_planet);
            if !already_tracking_contact {
                let docked = observation
                    .planets
                    .iter()
                    .find(|planet| planet.id == docked_planet);
                self.port_navigation = match (observation.own_ship.form, docked) {
                    (ShipForm::Ship, Some(planet)) if planet.owner == Some(observation.actor) => {
                        Some(PortNavigation {
                            goal: self
                                .port_navigation
                                .map_or(BrainGoal::Capture, |navigation| navigation.goal),
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
                _ => false,
            });

            if !keep_target {
                self.port_navigation = None;
            } else {
                if observation.own_ship.docked_planet == Some(navigation.planet)
                    && navigation.phase != PortNavigationPhase::Depart
                {
                    navigation.phase = PortNavigationPhase::Docked;
                }
                let completed = planet.is_some_and(|planet| match navigation.goal {
                    BrainGoal::Capture => planet.owner == Some(observation.actor),
                    BrainGoal::Rebuild => observation.own_ship.form == ShipForm::Ship,
                    _ => false,
                });
                if navigation.phase == PortNavigationPhase::Docked && completed {
                    navigation.phase = PortNavigationPhase::Depart;
                }
                self.port_navigation = Some(navigation);
            }
        }

        if self.port_navigation.is_none() {
            self.port_navigation = self.select_port_navigation(observation);
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
        if target.local_position.length() <= self.config.spaceport_staging_tolerance
            && target.local_velocity.length() <= self.config.spaceport_staging_velocity_tolerance
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
        &self,
        observation: &ShipObservationV1,
        port_navigation: Option<PortNavigation>,
    ) -> Option<Vec2> {
        let mut nearest: Option<(f32, Vec2)> = None;
        let mut consider = |position: Vec2, velocity: Vec2, radius: f32, clearance: f32| {
            let surface_distance =
                position.length() - radius - observation.own_ship.collision_radius;
            let closing = position.dot(velocity) < 0.0;
            if surface_distance > clearance || (!closing && surface_distance > clearance * 0.35) {
                return;
            }
            if nearest.is_none_or(|(distance, _)| surface_distance < distance) {
                nearest = Some((surface_distance, -position));
            }
        };

        if let Some(sun) = observation.sun {
            consider(
                sun.local_position,
                sun.local_velocity,
                sun.radius,
                self.config.body_clearance,
            );
        }
        for planet in &observation.planets {
            let clearance = if let Some(navigation) =
                port_navigation.filter(|navigation| navigation.planet == planet.id)
            {
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
                planet.local_position,
                planet.local_velocity,
                planet.radius,
                clearance,
            );
        }
        nearest.map(|(_, direction)| direction)
    }

    fn avoidance_intent(
        &mut self,
        observation: &ShipObservationV1,
        direction: Vec2,
        goal: BrainGoal,
        hazard: Option<DebrisId>,
    ) -> ShipIntent {
        let heading = guide_heading(direction, observation.own_ship.angular_velocity);
        let navigation = self.port_navigation;
        self.set_telemetry(BrainTelemetry {
            goal,
            target_planet: navigation.map(|navigation| navigation.planet),
            port_phase: navigation.map(|navigation| navigation.phase),
            hazard,
            target_distance: direction.length(),
            heading_error: heading.error_radians,
            ..BrainTelemetry::default()
        });
        ShipIntent {
            turn: heading.turn,
            thrust: if heading.error_radians.abs() < 0.65 {
                1.0
            } else {
                0.0
            },
            brake: if heading.error_radians.abs() >= 0.65 {
                1.0
            } else {
                0.0
            },
            ..ShipIntent::default()
        }
    }

    fn docked_intent(
        &mut self,
        observation: &ShipObservationV1,
        navigation: PortNavigation,
    ) -> ShipIntent {
        let heading = guide_heading(Vec2::Y, observation.own_ship.angular_velocity);
        self.set_telemetry(BrainTelemetry {
            goal: navigation.goal,
            target_planet: Some(navigation.planet),
            port_phase: Some(PortNavigationPhase::Docked),
            heading_error: heading.error_radians,
            ..BrainTelemetry::default()
        });
        ShipIntent::default()
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
            self.set_telemetry(BrainTelemetry {
                goal: navigation.goal,
                target_planet: Some(navigation.planet),
                port_phase: Some(navigation.phase),
                target_distance: (departure_clearance - surface_clearance).max(0.0),
                heading_error: heading.error_radians,
                desired_speed: self.config.spaceport_departure_speed,
                relative_speed: approach.local_velocity.length(),
                ..BrainTelemetry::default()
            });
            return ShipIntent {
                turn: heading.turn,
                thrust: if departure_burn_started { 1.0 } else { 0.0 },
                brake: if departure_burn_started { 0.0 } else { 1.0 },
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

        self.set_telemetry(BrainTelemetry {
            goal: navigation.goal,
            target_planet: Some(navigation.planet),
            port_phase: Some(navigation.phase),
            target_distance: guidance.target_distance,
            heading_error: guidance.heading.error_radians,
            desired_speed: guidance.desired_closing_speed,
            relative_speed: guidance.relative_speed,
            ..BrainTelemetry::default()
        });

        let wings_closed = observation.own_ship.form == ShipForm::Ship
            && navigation.phase == PortNavigationPhase::Rendezvous
            && guidance.target_distance > self.config.spaceport_cruise_distance
            && guidance.heading.error_radians.abs() < 0.12;
        let (thrust, brake) = if observation.own_ship.form == ShipForm::EscapePod {
            if guidance.thrust >= 0.75 {
                (1.0, 0.0)
            } else {
                (0.0, 1.0)
            }
        } else {
            (guidance.thrust, guidance.brake)
        };
        ShipIntent {
            turn: guidance.heading.turn,
            thrust,
            brake,
            wings_closed,
            ..ShipIntent::default()
        }
        .normalized()
    }
}

impl ShipBrain for RuleShipBrain {
    fn reset(&mut self, reset: BrainReset) {
        self.actor = Some(reset.actor);
        self.episode_seed = reset.episode_seed;
        self.port_navigation = None;
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
                    // into the cavity wall or cancel the launch burn.
                    return self.port_navigation_intent(observation, navigation, planet);
                }
                self.port_navigation = None;
            }
            _ => {}
        }

        if let Some(direction) = self.body_avoidance(observation, self.port_navigation) {
            return self.avoidance_intent(observation, direction, BrainGoal::AvoidBody, None);
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

        let Some(opponent) = observation.opponent.filter(|opponent| !opponent.eliminated) else {
            self.telemetry = BrainTelemetry {
                actor: self.actor,
                goal: BrainGoal::Idle,
                ..BrainTelemetry::default()
            };
            return ShipIntent::default();
        };

        if observation.own_ship.form == ShipForm::EscapePod {
            let heading = guide_heading(
                -opponent.local_position,
                observation.own_ship.angular_velocity,
            );
            self.set_telemetry(BrainTelemetry {
                goal: BrainGoal::Survive,
                target: Some(opponent.id),
                target_distance: opponent.local_position.length(),
                heading_error: heading.error_radians,
                relative_speed: opponent.local_velocity.length(),
                ..BrainTelemetry::default()
            });
            let aligned = heading.error_radians.abs() < 0.65;
            return ShipIntent {
                turn: heading.turn,
                thrust: if aligned { 1.0 } else { 0.0 },
                brake: if aligned { 0.0 } else { 1.0 },
                ..ShipIntent::default()
            };
        }

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

        self.set_telemetry(BrainTelemetry {
            goal: BrainGoal::Attack,
            target: Some(opponent.id),
            target_distance: distance,
            heading_error: heading.error_radians,
            relative_speed: opponent.local_velocity.length(),
            ..BrainTelemetry::default()
        });
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
        assert_eq!(brain.telemetry().target_planet, Some(expected_planet));
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Rendezvous)
        );
        assert!(!intent.laser);
        assert!(!intent.cannon);
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

        brain.intent(&observation);

        assert_eq!(brain.telemetry().target_planet, Some(docked_planet));
        assert_eq!(
            brain.telemetry().port_phase,
            Some(PortNavigationPhase::Depart)
        );

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
        assert_eq!(intent, ShipIntent::default());
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
        let mut saw_ingress = false;
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
            saw_ingress |= telemetry.port_phase == Some(PortNavigationPhase::Ingress);
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
            saw_ingress,
            "brain never advanced from staging to ingress; minimum distance {minimum_target_distance}; final telemetry: {:?}; ship position {:?}; velocity {:?}",
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

            for _ in 0..7_200 {
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
                    let selected_next_target = saw_depart
                        && telemetry.target_planet.is_some()
                        && telemetry.target_planet != Some(captured_planet)
                        && telemetry.port_phase == Some(PortNavigationPhase::Rendezvous);
                    if selected_next_target {
                        let captured = observation
                            .planets
                            .iter()
                            .find(|planet| planet.id == captured_planet)
                            .expect("captured planet remains observable");
                        let surface_clearance = captured.local_position.length()
                            - captured.radius
                            - observation.own_ship.collision_radius;
                        saw_safe_clearance = observation.own_ship.docked_planet
                            != Some(captured_planet)
                            && surface_clearance >= brain.config.body_clearance;
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
                "brain selected its next target before safely clearing the captured planet for seed {seed}"
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
