//! Optional physical handoff from a timed-out fight to another surface mission.
use super::*;

#[path = "mission_handoff.rs"]
mod handoff;
pub use handoff::{
    ContinuationReport, HandoffTelemetry, Successor, SuccessorComparison, SuccessorComparisonJob,
    SuccessorContinuation,
};

#[path = "mission_boundary.rs"]
mod boundary;
pub use boundary::BoundaryGuidance;
#[path = "mission_destination_cover.rs"]
mod destination_cover;
use scenario_spacewars::surface_sortie::destination_cover::DestinationCoverRequest;

fn disabled(value: &bool) -> bool {
    !value
}

const DISENGAGEMENT_TICKS: u64 = 12 * 60;
const CLEAR_TICKS: u64 = 60;
const CLEAR_RANGE: f32 = 350.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MissionDisengagement {
    #[serde(skip_serializing_if = "disabled")]
    pub cover_probe: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_request: Option<DestinationCoverRequest>,
    #[serde(skip_serializing_if = "disabled")]
    pub boundary_aware: bool,
    #[serde(skip_serializing_if = "disabled")]
    pub handoff_probe: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff: Option<HandoffTelemetry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary: Option<BoundaryGuidance>,
    pub attempts: u32,
    pub separated: u32,
    pub timed_out: u32,
    pub last: Option<DisengagementAttempt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DisengagementAttempt {
    pub started_tick: u64,
    pub deadline_tick: u64,
    pub finished_tick: Option<u64>,
    pub reason: Option<&'static str>,
    pub direction: Vec2,
    pub estimated_min_range: f32,
    pub estimated_clearance: f32,
    pub start_range: f32,
    pub range: f32,
    pub opening_speed: f32,
    pub clear_since: Option<u64>,
}

// Seven motor-scale alternatives, plus three inward alternatives in the
// boundary experiment, evaluated only at the handoff. This is a
// coarse inertial forecast using observed motion and control limits, not a
// collision oracle or a prediction of the opponent's future choices.
fn escape_direction(o: &MissionObservationV1, boundary_aware: bool) -> (Vec2, f32, f32) {
    let c = &o.local.combat;
    let p = &c.recovery.flight.pilot;
    let target = c.target.unwrap();
    let delta = p.ship.position - target.motion.position;
    let away = if delta.length_squared() > 0.01 {
        delta.normalized()
    } else {
        Vec2::Y.rotate_radians(p.ship.angle)
    };
    let mut best: Option<(Vec2, f32, f32, f32)> = None;
    let inward = (o.boundary.center - p.ship.position).normalized();
    let directions = (-3..=3)
        .map(|side| away.rotate_radians(side as f32 * std::f32::consts::FRAC_PI_6))
        .chain(
            [-1.0, 0.0, 1.0]
                .into_iter()
                .filter(|_| boundary_aware)
                .map(|side| inward.rotate_radians(side * std::f32::consts::FRAC_PI_3)),
        );
    for direction in directions {
        if direction.length_squared() < 0.5 {
            continue;
        }
        let limits = c.recovery.flight.flight.limits;
        let turn = shortest_heading_error(direction.rotate_radians(-p.ship.angle)).abs()
            / limits.turn_speed.max(0.01)
            + limits.turn_speed / limits.turn_acceleration.max(0.01);
        let mut position = p.ship.position;
        let mut velocity = p.ship.velocity;
        let mut min_range = delta.length();
        let mut clearance = f32::INFINITY;
        for step in 1..=12 {
            let elapsed = step as f32 * 0.5;
            let acceleration = if elapsed > turn {
                let error = direction * 110.0 - velocity;
                error.normalized() * (error.length() * 2.0).min(limits.thrust_acceleration)
            } else {
                Vec2::ZERO
            };
            velocity += (acceleration + p.gravity) * 0.5;
            position += velocity * 0.5;
            if boundary_aware {
                clearance = clearance.min(boundary::stopping_clearance(o, position, velocity));
            }
            min_range = min_range.min(
                position.distance_to(target.motion.position + target.motion.velocity * elapsed),
            );
            for (center, radius) in o
                .planets
                .iter()
                .map(|planet| {
                    (
                        planet.motion.position + planet.motion.velocity * elapsed,
                        planet.radius,
                    )
                })
                .chain(o.sun.map(|sun| (sun.position, sun.radius)))
            {
                clearance = clearance.min(position.distance_to(center) - radius - 65.0);
            }
        }
        let final_range =
            position.distance_to(target.motion.position + target.motion.velocity * 6.0);
        let score = min_range + final_range * 0.05;
        let better = best.is_none_or(|(_, _, old_clearance, old_score)| {
            if (clearance >= 0.0) != (old_clearance >= 0.0) {
                clearance >= 0.0
            } else if clearance < 0.0 {
                clearance > old_clearance
            } else {
                score > old_score
            }
        });
        if better {
            best = Some((direction, min_range, clearance, score));
        }
    }
    let (direction, min_range, clearance, _) = best.unwrap();
    (direction, min_range, clearance)
}

impl MaterialMissionPilot {
    pub(crate) fn configure_disengagement_boundary(&mut self, enabled: bool) {
        if let Some(d) = &mut self.telemetry.disengagement {
            d.boundary_aware = enabled;
            d.boundary = None;
        } else {
            assert!(!enabled, "enable disengagement before boundary guidance");
        }
    }

    pub(crate) fn enable_pursuit_disengagement(&mut self, enabled: bool) {
        self.telemetry.disengagement = enabled.then(MissionDisengagement::default);
    }

    pub(super) fn disengaging(&self) -> bool {
        self.telemetry
            .disengagement
            .as_ref()
            .and_then(|d| d.last)
            .is_some_and(|d| d.finished_tick.is_none())
    }

    pub(super) fn start_disengagement(&mut self, o: &MissionObservationV1) {
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        let Some(target) = c.target else { return };
        let delta = p.ship.position - target.motion.position;
        // Visibility is a first-solid query toward the target, independent of
        // aim. A missing target hit alone is not positive evidence of cover.
        if target.ship_form != Some(ShipForm::Ship)
            || target.health <= 0.0
            || delta.length() >= CLEAR_RANGE
            || target.ground_occluded
            || p.landing.supported_feet > 0
            || !o.planets.iter().any(|planet| {
                planet
                    .claim
                    .as_ref()
                    .is_none_or(|c| c.owner != Some(p.owner))
            })
        {
            return;
        }
        let Some(d) = &self.telemetry.disengagement else {
            return;
        };
        let (direction, estimated_min_range, estimated_clearance) =
            escape_direction(o, d.boundary_aware);
        let cover_request = d.cover_probe.then(|| self.destination_shortlist(o));
        let d = self.telemetry.disengagement.as_mut().unwrap();
        d.attempts += 1;
        d.cover_request = cover_request;
        if d.boundary_aware {
            d.boundary.get_or_insert_with(Default::default);
        }
        d.last = Some(DisengagementAttempt {
            started_tick: p.tick,
            deadline_tick: p.tick + DISENGAGEMENT_TICKS,
            finished_tick: None,
            reason: None,
            direction,
            estimated_min_range,
            estimated_clearance,
            start_range: delta.length(),
            range: delta.length(),
            opening_speed: 0.0,
            clear_since: None,
        });
        self.event(
            p.tick,
            "disengagement_started",
            Some("pursuit ended near opponent"),
        );
    }

    pub(super) fn end_disengagement(&mut self, tick: u64, reason: &'static str) {
        if !self.disengaging() {
            return;
        }
        let d = self.telemetry.disengagement.as_mut().unwrap();
        let attempt = d.last.as_mut().unwrap();
        attempt.finished_tick = Some(tick);
        attempt.reason = Some(reason);
        d.separated += u32::from(reason == "separation established");
        d.timed_out += u32::from(reason == "disengagement budget exhausted");
        self.pursuit_climb = None;
        self.solar_detour = None;
        self.event(tick, "disengagement_ended", Some(reason));
    }

    pub(super) fn disengagement_intent(
        &mut self,
        o: &MissionObservationV1,
    ) -> Option<CombatIntent> {
        if !self.disengaging() {
            return None;
        }
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        if !o.match_rules || self.capture.is_some() || p.landing.supported_feet > 0 {
            self.end_disengagement(p.tick, "surface task takes priority");
            return None;
        }
        let Some(target) = c
            .target
            .filter(|t| t.ship_form == Some(ShipForm::Ship) && t.health > 0.0)
        else {
            self.end_disengagement(p.tick, "opponent no longer armed ship");
            return None;
        };
        let delta = p.ship.position - target.motion.position;
        let boundary_ready = !self
            .telemetry
            .disengagement
            .as_ref()
            .unwrap()
            .boundary_aware
            || boundary::stopping_clearance(o, p.ship.position, p.ship.velocity) > 20.0;
        let attempt = self
            .telemetry
            .disengagement
            .as_mut()
            .unwrap()
            .last
            .as_mut()
            .unwrap();
        attempt.range = delta.length();
        attempt.opening_speed = (p.ship.velocity - target.motion.velocity).dot(delta.normalized());
        if boundary_ready
            && (target.ground_occluded
                || attempt.range >= CLEAR_RANGE && attempt.opening_speed >= 0.0)
        {
            attempt.clear_since.get_or_insert(p.tick);
        } else {
            attempt.clear_since = None;
        }
        if attempt
            .clear_since
            .is_some_and(|since| p.tick.saturating_sub(since) >= CLEAR_TICKS)
        {
            self.probe_disengagement_handoff(o);
            self.end_disengagement(p.tick, "separation established");
            return None;
        }
        if p.tick >= attempt.deadline_tick {
            // A failed escape does not authorize a landing beside an attacker.
            // Return to a fresh bounded fight; hits never extend either timer.
            self.end_disengagement(p.tick, "disengagement budget exhausted");
            self.telemetry.pursuit = Some(MissionPursuit {
                started_tick: p.tick,
                last_visible_tick: p.tick,
                reason: "opponent remained close after disengagement",
            });
            self.event(
                p.tick,
                "pursuit_started",
                Some("opponent remained close after disengagement"),
            );
            return Some(self.hunt(o));
        }
        let direction = attempt.direction;
        Some(self.escape_flight(o, direction))
    }

    // The bounded successor probe uses the same motor action, while retaining
    // the original attempt's deadline instead of restarting an escape timer.
    fn escape_flight(&mut self, o: &MissionObservationV1, direction: Vec2) -> CombatIntent {
        let c = &o.local.combat;
        let p = &c.recovery.flight.pilot;
        self.goal(MissionGoal::Disengage, p.tick);
        self.telemetry.opponent = c.target.map(|target| target.owner);
        self.telemetry.reason = Some("opening range before another landing");
        let radial = p.ship.position - p.planet.motion.position;
        let falling =
            (-(p.ship.velocity - p.planet.motion.velocity).dot(radial.normalized())).max(0.0);
        let desired = if radial.length() - p.planet.radius < 70.0 + falling * falling / 50.0 {
            self.pursuit_climb_velocity(o, p.planet.index)
        } else {
            // Keep the escape axis fixed while routing around moving planets
            // and the sun. Re-aiming directly away each tick can orbit a pursuer.
            let waypoint = self.route_waypoint(o, p.ship.position + direction * 450.0, None);
            self.detour_velocity(o, Vec2::ZERO) + (waypoint - p.ship.position).normalized() * 110.0
        };
        let mut intent = self.guide(o, desired);
        // Sweeping reduces turn authority and automatically adds cruise thrust.
        // Finish the turn with open wings, and open again for any local detour.
        intent.flight.wings.closed = self.telemetry.avoidance.is_none()
            && desired.length() > 90.0
            && !intent.flight.controls.brake_held
            && Vec2::Y.rotate_radians(p.ship.angle).dot(direction) > 0.97;
        intent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_policy::MissionPolicy;
    use engine_common::Scenario;
    use scenario_spacewars::surface_sortie::{
        LandingPhase, SurfaceSortieScenario, mission::MissionObstacle,
    };
    use std::time::Duration;

    pub(super) fn fixture(enabled: bool) -> (MaterialMissionPilot, MissionObservationV1) {
        let mut state = SurfaceSortieScenario::init_material_travel(42, false);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let mut o = state.mission_observation(0, None);
        o.match_rules = true;
        o.sun = None;
        // Synthetic planets below span a larger free-flight fixture than the
        // source lab; boundary-specific tests move back to its enclosing edge.
        o.boundary.radius = 5000.0;
        for (planet, x) in o.planets.iter_mut().zip([0.0, 1000.0]) {
            planet.motion.position = Vec2::new(x, 0.0);
            planet.motion.velocity = Vec2::ZERO;
            planet.radius = 60.0;
            planet.claim.as_mut().unwrap().owner = None;
        }
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = PURSUIT_BUDGET_TICKS + 1;
        p.planet = o.planets[0].clone();
        p.ship.position = Vec2::new(500.0, 500.0);
        p.ship.velocity = Vec2::ZERO;
        p.ship.angle = std::f32::consts::FRAC_PI_2;
        p.ship.spin = 0.0;
        p.gravity = Vec2::ZERO;
        p.controls_armed = true;
        p.landing.supported_feet = 0;
        p.landing.phase = LandingPhase::Flying;
        let target = o.local.combat.target.as_mut().unwrap();
        target.motion.position = Vec2::new(620.0, 500.0);
        target.motion.velocity = Vec2::ZERO;
        target.ship_form = Some(ShipForm::Ship);
        target.health = 100.0;
        target.visible = false;
        target.ground_occluded = false;
        o.opponent.as_mut().unwrap().motion = target.motion;
        let mut bot = MaterialMissionPilot::with_policy(
            BrainReset {
                actor: PlayerId::PLAYER_1,
                episode_seed: 42,
            },
            CombatBreakSettings::default(),
            MissionPolicy::JetpackPlanner,
        );
        bot.enable_pursuit_disengagement(enabled);
        bot.telemetry.pursuit = Some(MissionPursuit {
            started_tick: 1,
            last_visible_tick: 1,
            reason: "test pursuit",
        });
        (bot, o)
    }

    #[test]
    fn timeout_escape_is_opt_in_repeatable_and_emits_thrust() {
        let (mut bot, o) = fixture(true);
        let original = o.clone();
        let mut copy = bot.clone();
        let intent = bot.intent(&o);
        assert_eq!(bot.telemetry.goal, MissionGoal::Disengage);
        assert!(intent.flight.controls.primary_held);
        assert_eq!(intent.weapons, Default::default());
        assert_eq!(intent, copy.intent(&o));
        let telemetry = bot.telemetry.clone();
        assert_eq!(intent, bot.intent(&o));
        assert_eq!(telemetry, bot.telemetry);
        assert_eq!(o, original);
        let (mut disabled, _) = fixture(false);
        disabled.intent(&o);
        assert_ne!(disabled.telemetry.goal, MissionGoal::Disengage);
        assert!(disabled.telemetry.disengagement.is_none());
        bot.reset(bot.context);
        assert_eq!(
            bot.telemetry.disengagement,
            Some(MissionDisengagement::default())
        );
        assert_eq!(bot.policy, MissionPolicy::JetpackPlanner);
    }

    #[test]
    fn repeated_hits_and_loss_of_visibility_do_not_extend_the_escape() {
        let (mut bot, mut o) = fixture(true);
        let start = o.local.combat.recovery.flight.pilot.tick;
        for tick in start..=start + DISENGAGEMENT_TICKS {
            o.local.combat.recovery.flight.pilot.tick = tick;
            o.local.combat.weapons.last_hit_taken_tick = Some(tick);
            bot.intent(&o);
        }
        let d = bot.telemetry.disengagement.as_ref().unwrap();
        assert_eq!((d.attempts, d.separated, d.timed_out), (1, 0, 1));
        assert_eq!(
            d.last.unwrap().finished_tick,
            Some(start + DISENGAGEMENT_TICKS)
        );
        assert_eq!(
            bot.telemetry.pursuit.unwrap().started_tick,
            start + DISENGAGEMENT_TICKS
        );
        assert!(bot.capture.is_none());
    }

    #[test]
    fn clearance_requires_sustained_cover_or_opening_range() {
        let (mut bot, mut o) = fixture(true);
        bot.intent(&o);
        let start = o.local.combat.recovery.flight.pilot.tick;
        o.local.combat.target.as_mut().unwrap().motion.position.x = 900.0;
        o.local.combat.recovery.flight.pilot.ship.velocity = Vec2::X * 10.0;
        o.local.combat.recovery.flight.pilot.tick += 1;
        bot.intent(&o);
        assert_eq!(
            bot.telemetry
                .disengagement
                .as_ref()
                .unwrap()
                .last
                .unwrap()
                .clear_since,
            None
        );
        o.local.combat.recovery.flight.pilot.ship.velocity = -Vec2::X * 10.0;
        for tick in start + 2..start + 2 + CLEAR_TICKS {
            o.local.combat.recovery.flight.pilot.tick = tick;
            bot.intent(&o);
            assert!(bot.disengaging());
        }
        o.local.combat.target.as_mut().unwrap().motion.position.x = 620.0;
        o.local.combat.recovery.flight.pilot.tick += 1;
        bot.intent(&o);
        assert_eq!(
            bot.telemetry
                .disengagement
                .as_ref()
                .unwrap()
                .last
                .unwrap()
                .clear_since,
            None
        );
        o.local.combat.target.as_mut().unwrap().ground_occluded = true;
        let covered = o.local.combat.recovery.flight.pilot.tick + 1;
        for tick in covered..=covered + CLEAR_TICKS {
            o.local.combat.recovery.flight.pilot.tick = tick;
            bot.intent(&o);
        }
        assert!(!bot.disengaging());
        assert_eq!(bot.telemetry.disengagement.as_ref().unwrap().separated, 1);
        assert!(bot.telemetry.target.is_some());
    }

    #[test]
    fn escape_keeps_solar_and_recovery_priority_and_the_original_deadline() {
        let (mut bot, mut o) = fixture(true);
        bot.intent(&o);
        let deadline = bot
            .telemetry
            .disengagement
            .as_ref()
            .unwrap()
            .last
            .unwrap()
            .deadline_tick;
        o.local.combat.recovery.flight.pilot.tick += 1;
        o.sun = Some(MissionObstacle {
            position: Vec2::new(500.0, 460.0),
            radius: 30.0,
        });
        bot.intent(&o);
        assert_eq!(bot.telemetry.goal, MissionGoal::AvoidSun);
        assert!(bot.disengaging());
        o.local.combat.recovery.flight.pilot.tick = deadline + 1;
        o.sun = None;
        bot.intent(&o);
        assert!(!bot.disengaging());
        assert_eq!(
            bot.telemetry
                .disengagement
                .as_ref()
                .unwrap()
                .last
                .unwrap()
                .deadline_tick,
            deadline
        );
        let (mut bot, mut o) = fixture(true);
        bot.intent(&o);
        o.local.combat.recovery.flight.pilot.tick += 1;
        o.local.combat.recovery.flight.pilot.ship_form = ShipForm::EscapePod;
        bot.intent(&o);
        assert!(!bot.disengaging());
        assert!(bot.recovery.is_some());
    }

    #[test]
    fn supported_landings_surface_tasks_and_disarmed_opponents_do_not_start_escape() {
        for case in 0..7 {
            let (mut bot, mut o) = fixture(true);
            match case {
                0 => o.local.combat.recovery.flight.pilot.landing.supported_feet = 2,
                1 => bot.capture = Some(TacticalCapturePilot::new(bot.context, bot.breaks)),
                2 => o.local.combat.target.as_mut().unwrap().ship_form = Some(ShipForm::EscapePod),
                3 => o.match_rules = false,
                4 => o.local.combat.target.as_mut().unwrap().ground_occluded = true,
                5 => o.planets.iter_mut().for_each(|p| {
                    p.claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_1);
                }),
                _ => o.local.combat.recovery.flight.pilot.location = PilotLocation::OnFoot,
            }
            bot.intent(&o);
            assert_eq!(
                bot.telemetry.disengagement.as_ref().unwrap().attempts,
                0,
                "case {case}"
            );
        }
    }

    #[test]
    fn initial_direction_accounts_for_turning_and_predicted_obstacles() {
        let (_, mut o) = fixture(true);
        o.boundary.radius = 5000.0;
        o.local.combat.recovery.flight.pilot.ship.angle = 0.0;
        o.local.combat.target.as_mut().unwrap().motion.position = Vec2::new(380.0, 500.0);
        o.planets[0].motion.position = Vec2::new(810.0, 500.0);
        o.sun = Some(MissionObstacle {
            position: Vec2::new(500.0, 320.0),
            radius: 80.0,
        });
        // Running straight away crosses the planet. The opposite turn goes
        // toward the sun. The open corridor also requires the smaller turn.
        let (direction, _, clearance) = escape_direction(&o, false);
        assert!(direction.y > 0.2, "{direction:?}");
        assert!(direction.x >= -0.001);
        assert!(clearance > 0.0);
        let original = direction;
        // The corridor uses world observations, not the selected local frame.
        o.local.combat.recovery.flight.pilot.planet = o.planets[1].clone();
        assert_eq!(escape_direction(&o, false).0, original);
    }
}
