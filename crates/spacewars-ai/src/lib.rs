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
    DebrisId, HazardObservationV1, PlayerId, ShipForm, ShipIntent, ShipObservationV1,
};

const TARGET_EPSILON: f32 = 1.0e-5;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BrainGoal {
    #[default]
    Idle,
    Attack,
    AvoidBody,
    AvoidHazard,
    Survive,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrainTelemetry {
    pub actor: Option<PlayerId>,
    pub goal: BrainGoal,
    pub target: Option<PlayerId>,
    pub hazard: Option<DebrisId>,
    pub target_distance: f32,
    pub heading_error: f32,
}

impl Default for BrainTelemetry {
    fn default() -> Self {
        Self {
            actor: None,
            goal: BrainGoal::Idle,
            target: None,
            hazard: None,
            target_distance: 0.0,
            heading_error: 0.0,
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
        }
    }
}

/// Deterministic first-generation opponent for a one-on-one Spacewars duel.
///
/// It deliberately has a narrow strategy: avoid imminent collisions, face the
/// opponent, manage closing speed, and fire when aligned. Planet selection and
/// capture strategy belong to later slices, while the guidance primitives here
/// can be reused for them.
#[derive(Debug, Clone, Default)]
pub struct RuleShipBrain {
    config: RuleShipBrainConfig,
    actor: Option<PlayerId>,
    episode_seed: u64,
    telemetry: BrainTelemetry,
}

impl RuleShipBrain {
    pub fn new(config: RuleShipBrainConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    fn set_telemetry(
        &mut self,
        goal: BrainGoal,
        target: Option<PlayerId>,
        hazard: Option<DebrisId>,
        distance: f32,
        heading: HeadingGuidance,
    ) {
        self.telemetry = BrainTelemetry {
            actor: self.actor,
            goal,
            target,
            hazard,
            target_distance: distance,
            heading_error: heading.error_radians,
        };
    }

    fn body_avoidance(&self, observation: &ShipObservationV1) -> Option<Vec2> {
        let mut nearest: Option<(f32, Vec2)> = None;
        let mut consider = |position: Vec2, velocity: Vec2, radius: f32| {
            let surface_distance =
                position.length() - radius - observation.own_ship.collision_radius;
            let closing = position.dot(velocity) < 0.0;
            if surface_distance > self.config.body_clearance
                || (!closing && surface_distance > self.config.body_clearance * 0.35)
            {
                return;
            }
            if nearest.is_none_or(|(distance, _)| surface_distance < distance) {
                nearest = Some((surface_distance, -position));
            }
        };

        if let Some(sun) = observation.sun {
            consider(sun.local_position, sun.local_velocity, sun.radius);
        }
        for planet in &observation.planets {
            consider(planet.local_position, planet.local_velocity, planet.radius);
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
        self.set_telemetry(goal, None, hazard, direction.length(), heading);
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
}

impl ShipBrain for RuleShipBrain {
    fn reset(&mut self, reset: BrainReset) {
        self.actor = Some(reset.actor);
        self.episode_seed = reset.episode_seed;
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

        if let Some(direction) = self.body_avoidance(observation) {
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
            self.set_telemetry(
                BrainGoal::Survive,
                Some(opponent.id),
                None,
                opponent.local_position.length(),
                heading,
            );
            return ShipIntent {
                turn: heading.turn,
                thrust: if heading.error_radians.abs() < 0.65 {
                    1.0
                } else {
                    0.0
                },
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

        self.set_telemetry(
            BrainGoal::Attack,
            Some(opponent.id),
            None,
            distance,
            heading,
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
}
