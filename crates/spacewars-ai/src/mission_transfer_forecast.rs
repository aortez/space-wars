//! Conditional free-flight prediction after an accepted experimental nomination.
//! No result is consumed by the playing evaluator or controller.
use super::*;
use crate::flight_prediction::{BodyOriginMotor, advance_motor};
use engine_core::planning::{PlanningJob, WorkKind};
use scenario_spacewars::surface_sortie::{
    LandingPhase,
    combat::TacticalSortieObservationV1,
    pilot::PilotMotion,
    transfer_environment::{MAX_TRANSFER_PLANETS, TransferEnvironment},
};

const DT: f32 = 1.0 / 60.0;
pub const MAX_TICKS: u64 = 3600;
const MAX_SEGMENTS: usize = 128;

pub(super) fn source_schema_matches(o: &MissionObservationV1, actor: PlayerId) -> bool {
    let f = &o.local.combat.recovery.flight;
    o.version == 1
        && o.local.version == 1
        && o.local.combat.version == 2
        && o.local.combat.recovery.version == 1
        && f.version == 2
        && f.flight.version == 1
        && f.pilot.version == 1
        && f.pilot.owner == actor
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferForecastEnd {
    KinematicHandoff,
    PlanetEnvelope,
    SunEnvelope,
    BoundaryEnvelope,
    ControllerInterrupted,
    MatchTimeLimit,
    Horizon,
    NonFinite,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferForecastPhase {
    pub start_tick: u64,
    pub end_tick: u64,
    pub goal: MissionGoal,
    pub frame: usize,
    pub obstacle: Option<MissionObstacleId>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferForecastSample {
    pub after_ticks: u64,
    pub ship: PilotMotion,
    pub frame: usize,
    pub gravity: Vec2,
    pub target: PilotMotion,
    pub target_range: f32,
    pub target_relative_speed: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferForecastReport {
    pub model: &'static str,
    pub source_tick: u64,
    pub destination: usize,
    pub horizon_ticks: u64,
    pub ticks: u64,
    pub charged_graph: u64,
    pub end: Option<TransferForecastEnd>,
    /// Conditional model endpoint, without future query readiness or contacts.
    pub handoff_seconds: Option<f32>,
    pub launch_ticks: u64,
    pub transfer_ticks: u64,
    pub solar_escape_ticks: u64,
    pub avoidance_ticks: u64,
    pub boundary_ticks: u64,
    pub frame_changes: u64,
    pub minimum_planet_clearance: f32,
    pub minimum_sun_clearance: Option<f32>,
    pub minimum_boundary_clearance: f32,
    pub phases_truncated: bool,
    pub phases: Vec<TransferForecastPhase>,
    pub samples: Vec<TransferForecastSample>,
}

#[derive(Clone)]
pub struct TransferForecastJob {
    bot: MaterialMissionPilot,
    predicted: MissionObservationV1,
    environment: TransferEnvironment,
    intent: CombatIntent,
    match_ticks: Option<u64>,
    body_motor: Option<BodyOriginMotor>,
    report: TransferForecastReport,
}

impl MaterialMissionPilot {
    /// Capture after source intent but before physics. The first command and
    /// PWM/progress/avoidance memory must come from that accepted nomination.
    pub fn forecast_nominated_transfer(
        &self,
        o: &MissionObservationV1,
        environment: TransferEnvironment,
        horizon_ticks: u64,
    ) -> Result<TransferForecastJob, &'static str> {
        let p = &o.local.combat.recovery.flight.pilot;
        if !source_schema_matches(o, self.context.actor) {
            return Err("observation version or actor mismatch");
        }
        self.telemetry.target.ok_or("no nominated destination")?;
        if !self.destination_switched
            || self.selected_tick != p.tick
            || self.previous_tick != Some(p.tick)
        {
            return Err("forecast needs the accepted source command");
        }
        self.forecast_current_transfer(o, environment, horizon_ticks)
    }

    /// Snapshot the command actually selected at this source, including its PWM
    /// and progress memory. Only comparison jobs may bypass the nomination gate.
    pub(super) fn forecast_current_transfer(
        &self,
        o: &MissionObservationV1,
        environment: TransferEnvironment,
        horizon_ticks: u64,
    ) -> Result<TransferForecastJob, &'static str> {
        let p = &o.local.combat.recovery.flight.pilot;
        if !source_schema_matches(o, self.context.actor) || self.previous_tick != Some(p.tick) {
            return Err("forecast needs the current source command");
        }
        let destination = self.telemetry.target.ok_or("no current destination")?;
        if !(1..=MAX_TICKS).contains(&horizon_ticks) || !environment.matches_source(o) {
            return Err("forecast horizon or environment mismatch");
        }
        let finite = |v: Vec2| v.x.is_finite() && v.y.is_finite();
        let flight = o.local.combat.recovery.flight.flight;
        if !finite(p.ship.position)
            || !finite(p.ship.velocity)
            || !finite(p.gravity)
            || !p.ship.angle.is_finite()
            || !p.ship.spin.is_finite()
            || !p.ship_health.is_finite()
            || !(0.0..=1.0).contains(&flight.sweep)
            || flight.limits
                != scenario_spacewars::surface_sortie::flight::FlightControlLimits::for_sweep(
                    flight.sweep,
                )
        {
            return Err("forecast source dynamics unsupported");
        }
        if !p.controls_armed
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || p.ship_health <= 0.0
            || p.location == PilotLocation::OnFoot
            || p.landing.phase != LandingPhase::Flying
            || p.landing.supported_feet != 0
            || p.landing.assist_strength != 0.0
            || !o.local.combat.recovery.flight.flight.enabled
            || self.capture.is_some()
            || self.recovery.is_some()
            || self.telemetry.pursuit.is_some()
            || self.disengaging()
            || !matches!(
                self.telemetry.goal,
                MissionGoal::Launch | MissionGoal::Transfer | MissionGoal::AvoidSun
            )
            || !o.planets.iter().any(|planet| planet.index == destination)
        {
            return Err("source is not an unassisted nominated flight");
        }
        let mut predicted = o.clone();
        // No old local permission or weapon query may authorize predicted work.
        predicted.destination_cover = None;
        predicted.local.cover.clear();
        predicted.local.landing_objective = None;
        predicted.local.objective_work = None;
        predicted.local.combat.recovery.ground = None;
        predicted.local.combat.target = None;
        predicted.opponent = None;
        let pilot = &mut predicted.local.combat.recovery.flight.pilot;
        pilot.sites.clear();
        pilot.queries_ready = false;
        pilot.site_query = LandingSiteQuery::NotRequested;
        let mut job = TransferForecastJob {
            bot: self.clone(),
            predicted,
            environment,
            intent: self.previous_intent,
            body_motor: None,
            match_ticks: o
                .match_context
                .as_ref()
                .and_then(|m| m.remaining_seconds)
                .map(|seconds| (seconds.max(0.0) / (16_666_667.0 / 1_000_000_000.0)).ceil() as u64),
            report: TransferForecastReport {
                model: "guided_transfer_forecast_v1",
                source_tick: p.tick,
                destination,
                horizon_ticks,
                ticks: 0,
                charged_graph: 0,
                end: None,
                handoff_seconds: None,
                launch_ticks: 0,
                transfer_ticks: 0,
                solar_escape_ticks: 0,
                avoidance_ticks: 0,
                boundary_ticks: 0,
                frame_changes: 0,
                minimum_planet_clearance: f32::MAX,
                minimum_sun_clearance: None,
                minimum_boundary_clearance: f32::MAX,
                phases_truncated: false,
                phases: Vec::new(),
                samples: Vec::new(),
            },
        };
        job.record_sample();
        Ok(job)
    }
}

impl TransferForecastJob {
    /// Opt-in body-origin transport for a newly captured transfer job. Keep the
    /// original model available for paired diagnostics and historical callers.
    pub fn with_body_origin_motion(mut self) -> Self {
        assert_eq!(
            self.report.ticks, 0,
            "select motion model before forecasting"
        );
        self.body_motor = Some(BodyOriginMotor::default());
        self.report.model = "guided_transfer_forecast_body_v2";
        self
    }

    /// Conditional endpoint only. Queries, sites and opponents remain absent;
    /// callers must not treat this as a native arrival observation.
    pub(super) fn arrival_frame(&self) -> Option<TacticalSortieObservationV1> {
        if self.report.end != Some(TransferForecastEnd::KinematicHandoff) {
            return None;
        }
        let mut local = self.predicted.local.clone();
        local.planet_orbit_omega = self.environment.planet_orbit_omega(self.report.destination);
        Some(local)
    }

    pub fn report(&self) -> &TransferForecastReport {
        &self.report
    }

    fn record_sample(&mut self) {
        let p = &self.predicted.local.combat.recovery.flight.pilot;
        let target = self
            .predicted
            .planets
            .iter()
            .find(|planet| planet.index == self.report.destination)
            .unwrap();
        self.report.samples.push(TransferForecastSample {
            after_ticks: self.report.ticks,
            ship: p.ship,
            frame: p.planet.index,
            gravity: p.gravity,
            target: target.motion,
            target_range: p.ship.position.distance_to(target.motion.position),
            target_relative_speed: (p.ship.velocity - target.motion.velocity).length(),
        });
    }

    fn record_phase(&mut self) {
        let t = &self.bot.telemetry;
        let frame = self
            .predicted
            .local
            .combat
            .recovery
            .flight
            .pilot
            .planet
            .index;
        let obstacle = t.avoidance.map(|a| a.obstacle);
        let r = &mut self.report;
        r.launch_ticks += u64::from(t.goal == MissionGoal::Launch);
        r.transfer_ticks += u64::from(t.goal == MissionGoal::Transfer);
        r.solar_escape_ticks += u64::from(t.goal == MissionGoal::AvoidSun);
        r.avoidance_ticks += u64::from(obstacle.is_some());
        r.boundary_ticks += u64::from(
            t.disengagement
                .as_ref()
                .and_then(|d| d.boundary)
                .is_some_and(|b| b.active),
        );
        if r.phases_truncated {
            return;
        }
        if let Some(last) = r.phases.last_mut()
            && last.goal == t.goal
            && last.frame == frame
            && last.obstacle == obstacle
        {
            last.end_tick += 1;
        } else if r.phases.len() < MAX_SEGMENTS {
            r.phases.push(TransferForecastPhase {
                start_tick: r.ticks,
                end_tick: r.ticks + 1,
                goal: t.goal,
                frame,
                obstacle,
            });
        } else {
            r.phases_truncated = true;
        }
    }

    fn envelope(&mut self, start: Vec2, centers: &[Vec2]) -> Option<TransferForecastEnd> {
        let end = self
            .predicted
            .local
            .combat
            .recovery
            .flight
            .pilot
            .ship
            .position;
        let r = &mut self.report;
        let distance = crate::landing_safety::distance_to_segment;
        for (a, b) in centers.iter().zip(&self.predicted.planets) {
            let clearance =
                distance(Vec2::ZERO, start - *a, end - b.motion.position) - b.radius - 35.0;
            r.minimum_planet_clearance = r.minimum_planet_clearance.min(clearance);
        }
        if let Some(sun) = self.predicted.sun {
            let clearance = distance(sun.position, start, end) - sun.radius - 32.0;
            r.minimum_sun_clearance = Some(
                r.minimum_sun_clearance
                    .map_or(clearance, |old| old.min(clearance)),
            );
        }
        let boundary = self.predicted.boundary;
        let clearance = boundary.radius
            - start
                .distance_to(boundary.center)
                .max(end.distance_to(boundary.center))
            - 20.0;
        r.minimum_boundary_clearance = r.minimum_boundary_clearance.min(clearance);
        if r.minimum_planet_clearance <= 0.0 {
            Some(TransferForecastEnd::PlanetEnvelope)
        } else if r.minimum_sun_clearance.is_some_and(|c| c <= 0.0) {
            Some(TransferForecastEnd::SunEnvelope)
        } else if r.minimum_boundary_clearance <= 0.0 {
            Some(TransferForecastEnd::BoundaryEnvelope)
        } else {
            None
        }
    }
}

impl PlanningJob for TransferForecastJob {
    type Output = TransferForecastReport;
    fn next_work(&self) -> Option<WorkKind> {
        self.report.end.is_none().then_some(WorkKind::Graph)
    }
    fn step(&mut self) {
        assert!(self.report.end.is_none(), "finished transfer forecast");
        self.record_phase();
        let before_centers: [Vec2; MAX_TRANSFER_PLANETS] = std::array::from_fn(|index| {
            self.predicted
                .planets
                .get(index)
                .map_or(Vec2::ZERO, |p| p.motion.position)
        });
        let f = &mut self.predicted.local.combat.recovery.flight;
        let p = &mut f.pilot;
        let before = p.ship.position;
        let gravity = self.environment.ship_gravity(p.ship.position);
        let frame_velocity = p.planet.velocity_at(p.ship.position);
        // Real gravity changes velocity before the motor computes braking and
        // governor strength. Guidance already used the previous-step reading.
        p.ship.velocity += gravity * DT;
        f.flight.limits = if let Some(motor) = &mut self.body_motor {
            motor.advance(
                &mut p.ship,
                &mut f.flight.sweep,
                self.intent.flight,
                frame_velocity,
                Vec2::ZERO,
            )
        } else {
            advance_motor(
                &mut p.ship,
                &mut f.flight.sweep,
                self.intent.flight,
                frame_velocity,
                Vec2::ZERO,
            )
        };
        p.gravity = gravity;
        p.tick += 1;
        self.environment.advance(&mut self.predicted.planets);
        let index = self
            .environment
            .approach_frame(p.ship.position, p.planet.index);
        let frame = &self.predicted.planets[index];
        self.report.frame_changes += u64::from(frame.index != p.planet.index);
        p.planet = frame.clone();
        let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
        f.flight.wings_closed = self.intent.flight.wings.closed;
        f.flight.relative_speed = relative.length();
        f.flight.forward_speed = relative.dot(Vec2::Y.rotate_radians(p.ship.angle));
        f.flight.stopping_distance =
            relative.length_squared() / (2.0 * f.flight.limits.brake_acceleration);
        self.report.ticks += 1;
        self.report.charged_graph += 1;
        let finite = |v: Vec2| v.x.is_finite() && v.y.is_finite();
        let valid = finite(p.ship.position)
            && finite(p.ship.velocity)
            && finite(p.gravity)
            && p.ship.angle.is_finite()
            && p.ship.spin.is_finite();
        let target = self
            .predicted
            .planets
            .iter()
            .find(|planet| planet.index == self.report.destination)
            .unwrap();
        let handoff = p.planet.index == target.index
            && p.ship.position.distance_to(target.motion.position) < target.radius + 105.0
            && (p.ship.velocity - target.motion.velocity).length() < 18.0;
        self.report.end = if !valid {
            Some(TransferForecastEnd::NonFinite)
        } else if let Some(end) =
            self.envelope(before, &before_centers[..self.predicted.planets.len()])
        {
            Some(end)
        } else if self
            .match_ticks
            .is_some_and(|limit| self.report.ticks >= limit)
        {
            Some(TransferForecastEnd::MatchTimeLimit)
        } else {
            None
        };
        if self.report.end.is_none() {
            self.intent = self
                .bot
                .intent_with_inputs(&self.predicted, None, None, None, true);
            if self.bot.telemetry.target != Some(self.report.destination)
                || !matches!(
                    self.bot.telemetry.goal,
                    MissionGoal::Launch | MissionGoal::Transfer | MissionGoal::AvoidSun
                )
            {
                self.report.end = Some(TransferForecastEnd::ControllerInterrupted);
            } else if handoff && self.bot.telemetry.goal != MissionGoal::AvoidSun {
                self.report.end = Some(TransferForecastEnd::KinematicHandoff);
            } else if self.report.ticks >= self.report.horizon_ticks {
                self.report.end = Some(TransferForecastEnd::Horizon);
            }
        }
        if self.report.end == Some(TransferForecastEnd::KinematicHandoff) {
            self.report.handoff_seconds = Some(self.report.ticks as f32 * DT);
        }
        if self.report.end.is_some() || self.report.ticks.is_multiple_of(60) {
            self.record_sample();
        }
    }
    fn output(&self) -> Option<&Self::Output> {
        self.report.end.is_some().then_some(&self.report)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::{mission_evaluation::MissionEvaluator, mission_policy::MissionPolicy};
    use engine_common::Scenario;
    use engine_core::planning::{JobLimits, JobPoll, PlanningQueue, Work};
    use scenario_spacewars::surface_sortie::{SurfaceSortieScenario, SurfaceSortieState};
    use std::time::Duration;

    pub(crate) fn source() -> (
        SurfaceSortieState,
        MaterialMissionPilot,
        MissionObservationV1,
    ) {
        let (state, _, bot, o) = source_with_before();
        (state, bot, o)
    }

    pub(crate) fn source_with_before() -> (
        SurfaceSortieState,
        MaterialMissionPilot,
        MaterialMissionPilot,
        MissionObservationV1,
    ) {
        let seed = 13100125988314582075;
        let mut state = SurfaceSortieScenario::init_material_arena(seed);
        state.enable_match_rules();
        let mut bot = MaterialMissionPilot::with_policy(
            BrainReset {
                actor: PlayerId::from_index(0).unwrap(),
                episode_seed: seed,
            },
            Default::default(),
            MissionPolicy::ValuePlanner,
        );
        let evaluator = MissionEvaluator::new(1);
        for _ in 0..900 {
            let o = state.mission_observation(0, bot.site_request());
            let p = &o.local.combat.recovery.flight.pilot;
            let alternative = o.planets.iter().find(|planet| {
                p.tick >= 60
                    && planet.index != p.planet.index
                    && bot.transfer_probe_gate(&o, planet.index).is_ok()
            });
            let intent = if let Some(planet) = alternative {
                let before = bot.clone();
                let (intent, result) =
                    bot.intent_with_destination_probe(&o, &evaluator, planet.index);
                if result.accepted {
                    return (state, before, bot, o);
                }
                intent
            } else {
                bot.intent(&o)
            };
            SurfaceSortieScenario::step(
                &mut state,
                &intent.encode(p.owner),
                Duration::from_nanos(16_666_667),
            );
        }
        panic!("no nominated source");
    }

    #[test]
    fn native_first_motor_tick_preserves_nomination_and_gravity_epochs() {
        for (thrust, brake, turn, swept) in [
            (true, false, 0.0, false),
            (false, true, 1.0, false),
            (false, false, -1.0, true),
        ] {
            let (mut state, mut bot, o) = source();
            bot.previous_intent.flight.controls.primary_held = thrust;
            bot.previous_intent.flight.controls.brake_held = brake;
            bot.previous_intent.flight.controls.horizontal = turn;
            bot.previous_intent.flight.wings.closed = swept;
            let before = bot.clone();
            let environment = state.transfer_environment().unwrap();
            let gravity =
                environment.ship_gravity(o.local.combat.recovery.flight.pilot.ship.position);
            let mut job = bot
                .forecast_nominated_transfer(&o, environment, 3600)
                .unwrap();
            assert_eq!(
                job.predicted.local.combat.recovery.flight.pilot.gravity,
                o.local.combat.recovery.flight.pilot.gravity
            );
            assert_eq!(job.intent, before.previous_intent);
            assert!(
                !job.predicted
                    .local
                    .combat
                    .recovery
                    .flight
                    .pilot
                    .queries_ready
            );
            job.step();
            SurfaceSortieScenario::step(
                &mut state,
                &before
                    .previous_intent
                    .encode(o.local.combat.recovery.flight.pilot.owner),
                Duration::from_nanos(16_666_667),
            );
            let actual = state.mission_observation(0, None);
            let p = &job.predicted.local.combat.recovery.flight.pilot;
            let a = &actual.local.combat.recovery.flight.pilot;
            assert!(
                p.ship.position.distance_to(a.ship.position) < 0.03,
                "predicted={:?} actual={:?}",
                p.ship,
                a.ship
            );
            assert!(
                p.ship.velocity.distance_to(a.ship.velocity) < 0.15,
                "predicted={:?} actual={:?}",
                p.ship,
                a.ship
            );
            assert!((p.ship.spin - a.ship.spin).abs() < 0.02);
            assert!(gravity.distance_to(a.gravity) < 0.01);
            assert_eq!(p.planet.index, a.planet.index);
            assert_eq!(job.report.charged_graph, 1);
            assert_eq!(bot.telemetry, before.telemetry);
            assert_eq!(bot.pwm, before.pwm);
            assert_eq!(bot.previous_intent, before.previous_intent);
            assert!(
                !job.bot
                    .telemetry
                    .events
                    .iter()
                    .any(|e| e.kind == "arrived"
                        && e.tick > o.local.combat.recovery.flight.pilot.tick)
            );
        }
    }

    #[test]
    fn body_origin_motor_matches_native_turns_and_wing_transitions() {
        for (turn, closed) in [
            (-1.0, false),
            (1.0, false),
            (0.0, false),
            (-0.5, true),
            (0.5, true),
        ] {
            let (mut state, bot, _) = source();
            let mut motor = BodyOriginMotor::default();
            for tick in 0..90 {
                let o = state.mission_observation(0, None);
                let p = &o.local.combat.recovery.flight.pilot;
                assert_eq!(p.landing.assist_strength, 0.0);
                assert_eq!(state.transfer_solver_contact(0), Some(false));
                let mut intent = bot.previous_intent;
                intent.flight.controls.primary_held = false;
                intent.flight.controls.brake_held = false;
                intent.flight.controls.horizontal = turn;
                intent.flight.wings.closed = closed && tick < 45;
                let gravity = state
                    .transfer_environment()
                    .unwrap()
                    .ship_gravity(p.ship.position);
                let mut predicted = p.ship;
                predicted.velocity += gravity * DT;
                let mut sweep = o.local.combat.recovery.flight.flight.sweep;
                motor.advance(
                    &mut predicted,
                    &mut sweep,
                    intent.flight,
                    p.planet.velocity_at(p.ship.position),
                    Vec2::ZERO,
                );
                SurfaceSortieScenario::step(
                    &mut state,
                    &intent.encode(p.owner),
                    Duration::from_nanos(16_666_667),
                );
                let actual = state.mission_observation(0, None);
                let a = &actual.local.combat.recovery.flight.pilot;
                assert!(
                    predicted.position.distance_to(a.ship.position) < 0.002,
                    "tick={tick}, turn={turn}, swept={closed}: {predicted:?} != {:?}",
                    a.ship
                );
                assert!(
                    predicted.velocity.distance_to(a.ship.velocity) < 0.003,
                    "tick={tick}, turn={turn}, swept={closed}: {predicted:?} != {:?}",
                    a.ship
                );
                assert!((predicted.spin - a.ship.spin).abs() < 0.00002);
                let error = predicted.angle - a.ship.angle;
                assert!(error.sin().atan2(error.cos()).abs() < 0.00002);
                assert!(
                    (sweep - actual.local.combat.recovery.flight.flight.sweep).abs() < 0.000002
                );
            }
        }
    }

    #[test]
    fn body_origin_fixed_control_tape_reduces_accumulated_motor_error() {
        let (mut state, bot, source) = source();
        let mut body = source.local.combat.recovery.flight.pilot.ship;
        let mut point = body;
        let mut body_sweep = source.local.combat.recovery.flight.flight.sweep;
        let mut point_sweep = body_sweep;
        let mut motor = BodyOriginMotor::default();
        let mut worst_body = 0.0_f32;
        let mut worst_point = 0.0_f32;
        for tick in 0..90 {
            let o = state.mission_observation(0, None);
            let p = &o.local.combat.recovery.flight.pilot;
            assert_eq!(p.landing.assist_strength, 0.0);
            assert_eq!(state.transfer_solver_contact(0), Some(false));
            let mut intent = bot.previous_intent;
            intent.flight.controls.horizontal = if tick < 45 { 0.7 } else { -0.6 };
            intent.flight.controls.primary_held = false;
            intent.flight.controls.brake_held = tick >= 60;
            intent.flight.wings.closed = (15..60).contains(&tick);
            // Both motors receive the same actual external forcing. Their own
            // motion evolves without resets; guidance feedback is excluded.
            let gravity = state
                .transfer_environment()
                .unwrap()
                .ship_gravity(p.ship.position);
            let frame = p.planet.velocity_at(p.ship.position);
            body.velocity += gravity * DT;
            point.velocity += gravity * DT;
            motor.advance(&mut body, &mut body_sweep, intent.flight, frame, Vec2::ZERO);
            advance_motor(
                &mut point,
                &mut point_sweep,
                intent.flight,
                frame,
                Vec2::ZERO,
            );
            SurfaceSortieScenario::step(
                &mut state,
                &intent.encode(p.owner),
                Duration::from_nanos(16_666_667),
            );
            let actual = state
                .mission_observation(0, None)
                .local
                .combat
                .recovery
                .flight
                .pilot
                .ship;
            worst_body = worst_body.max(body.position.distance_to(actual.position));
            worst_point = worst_point.max(point.position.distance_to(actual.position));
            assert!(body.velocity.distance_to(actual.velocity) < 0.01);
        }
        assert!(worst_body < 0.02, "corrected position error {worst_body}");
        assert!(
            worst_point > 0.1 && worst_body < worst_point * 0.1,
            "body={worst_body}, point={worst_point}"
        );
    }

    #[test]
    fn body_origin_forecast_is_explicit_and_preserves_the_legacy_source() {
        let (state, bot, o) = source();
        let legacy = bot
            .forecast_nominated_transfer(&o, state.transfer_environment().unwrap(), 3600)
            .unwrap();
        let mut body = legacy.clone().with_body_origin_motion();
        assert_eq!(legacy.report.model, "guided_transfer_forecast_v1");
        assert_eq!(body.report.model, "guided_transfer_forecast_body_v2");
        assert_eq!(legacy.report.samples, body.report.samples);
        assert_eq!(legacy.intent, body.intent);
        assert!(legacy.body_motor.is_none());
        body.step();
        assert_eq!(legacy.report.ticks, 0);
        assert_eq!(body.report.charged_graph, 1);
    }

    #[test]
    fn zero_single_and_chunked_work_produce_identical_bounded_results() {
        let (state, bot, o) = source();
        let environment = state.transfer_environment().unwrap();
        for body_origin in [false, true] {
            let job = |horizon| {
                let job = bot
                    .forecast_nominated_transfer(&o, environment.clone(), horizon)
                    .unwrap();
                if body_origin {
                    job.with_body_origin_motion()
                } else {
                    job
                }
            };
            let mut direct = job(3600);
            while direct.next_work().is_some() {
                direct.step();
            }
            for allowance in [1, 7, 100] {
                let mut queue = PlanningQueue::new(1);
                let token = queue
                    .submit(0, (), JobLimits::default(), job(3600))
                    .unwrap();
                assert_eq!(queue.advance(Work::default()).charged, Work::default());
                assert_eq!(queue.job(token).unwrap().report.ticks, 0);
                let mut charged = 0;
                loop {
                    let allocation = queue.advance(Work {
                        graph: allowance,
                        physics_queries: 0,
                    });
                    charged += allocation.charged.graph as u64;
                    assert!(allocation.charged.graph <= allowance);
                    if let JobPoll::Ready(report) = queue.poll(token, &()) {
                        assert_eq!(report, direct.output().unwrap());
                        assert_eq!(report.ticks, charged);
                        assert!(report.samples.len() <= 62 && report.phases.len() <= MAX_SEGMENTS);
                        assert_eq!(
                            report.launch_ticks + report.transfer_ticks + report.solar_escape_ticks,
                            report.ticks
                        );
                        break;
                    }
                }
            }
            let mut short = job(1);
            short.step();
            assert_eq!(short.report.end, Some(TransferForecastEnd::Horizon));
            assert_eq!(short.report.handoff_seconds, None);
            assert_eq!(short.next_work(), None);
        }
    }

    #[test]
    fn source_mismatch_and_nonflight_sources_are_withheld() {
        let (state, bot, o) = source();
        let environment = state.transfer_environment().unwrap();
        for horizon in [0, MAX_TICKS + 1] {
            assert!(
                bot.forecast_nominated_transfer(&o, environment.clone(), horizon)
                    .is_err()
            );
        }
        let mut stale = environment.clone();
        stale.tick += 1;
        assert!(bot.forecast_nominated_transfer(&o, stale, 60).is_err());
        let mut changed = o.clone();
        changed
            .local
            .combat
            .recovery
            .flight
            .pilot
            .landing
            .assist_strength = 0.1;
        assert!(
            bot.forecast_nominated_transfer(&changed, environment.clone(), 60)
                .is_err()
        );
        changed = o.clone();
        changed.planets[0].radius += 1.0;
        assert!(
            bot.forecast_nominated_transfer(&changed, environment.clone(), 60)
                .is_err()
        );
        for field in 0..9 {
            let mut changed = o.clone();
            match field {
                0 => changed.version += 1,
                1 => changed.local.version += 1,
                2 => changed.local.combat.version += 1,
                3 => changed.local.combat.recovery.version += 1,
                4 => changed.local.combat.recovery.flight.version += 1,
                5 => changed.local.combat.recovery.flight.flight.version += 1,
                6 => changed.local.combat.recovery.flight.pilot.version += 1,
                7 => {
                    changed.local.combat.recovery.flight.pilot.owner =
                        PlayerId::from_index(1).unwrap()
                }
                _ => changed.local.combat.recovery.flight.pilot.ship_health = f32::NAN,
            }
            assert!(
                bot.forecast_nominated_transfer(&changed, environment.clone(), 60)
                    .is_err(),
                "accepted invalid source field {field}"
            );
        }
    }

    #[test]
    fn envelope_and_match_limits_cannot_publish_handoff_costs() {
        let (state, bot, o) = source();
        let environment = state.transfer_environment().unwrap();
        let mut job = bot
            .forecast_nominated_transfer(&o, environment.clone(), 3600)
            .unwrap();
        job.predicted.boundary.radius = 1.0;
        job.step();
        assert_eq!(job.report.end, Some(TransferForecastEnd::BoundaryEnvelope));
        assert_eq!(job.report.handoff_seconds, None);
        let mut job = bot
            .forecast_nominated_transfer(&o, environment, 3600)
            .unwrap();
        job.match_ticks = Some(1);
        job.step();
        assert_eq!(job.report.end, Some(TransferForecastEnd::MatchTimeLimit));
        assert_eq!(job.report.handoff_seconds, None);
    }

    #[test]
    fn kinematic_completion_at_horizon_grants_no_material_permission() {
        let (state, bot, o) = source();
        let mut job = bot
            .forecast_nominated_transfer(&o, state.transfer_environment().unwrap(), 1)
            .unwrap();
        let target = job.predicted.planets[job.report.destination].clone();
        let outward = (target.motion.position - job.predicted.sun.unwrap().position).normalized();
        let p = &mut job.predicted.local.combat.recovery.flight.pilot;
        p.planet = target.clone();
        p.ship.position = target.motion.position + outward * (target.radius + 85.0);
        p.ship.velocity = target.motion.velocity;
        p.ship.spin = 0.0;
        job.intent = CombatIntent::default();
        // The original frame's local rate must not leak into the destination
        // solar screen, even though it is irrelevant to free-flight guidance.
        job.predicted.local.planet_orbit_omega = Some(f32::NAN);
        let mut expiring = job.clone();
        expiring.match_ticks = Some(1);
        job.step();
        assert_eq!(job.report.end, Some(TransferForecastEnd::KinematicHandoff));
        assert_eq!(job.report.handoff_seconds, Some(DT));
        assert_eq!(job.report.charged_graph, 1);
        let arrival = job.arrival_frame().unwrap();
        assert_eq!(
            arrival.planet_orbit_omega,
            job.environment.planet_orbit_omega(job.report.destination)
        );
        assert_eq!(
            arrival.combat.recovery.flight.pilot.planet.index,
            job.report.destination
        );
        assert!(!arrival.combat.recovery.flight.pilot.queries_ready);
        assert!(arrival.combat.recovery.flight.pilot.sites.is_empty());
        assert!(arrival.combat.target.is_none());
        assert!(
            !job.predicted
                .local
                .combat
                .recovery
                .flight
                .pilot
                .queries_ready
        );
        assert!(job.bot.capture.is_none());
        assert!(
            !job.bot
                .telemetry
                .events
                .iter()
                .any(|event| event.kind == "arrived" && event.tick > job.report.source_tick)
        );
        expiring.step();
        assert_eq!(
            expiring.report.end,
            Some(TransferForecastEnd::MatchTimeLimit)
        );
        assert_eq!(expiring.report.handoff_seconds, None);
    }

    #[test]
    fn excess_phase_detail_is_explicitly_truncated() {
        let (state, bot, o) = source();
        let mut job = bot
            .forecast_nominated_transfer(&o, state.transfer_environment().unwrap(), 3600)
            .unwrap();
        for tick in 0..MAX_SEGMENTS + 5 {
            job.bot.telemetry.goal = if tick % 2 == 0 {
                MissionGoal::Launch
            } else {
                MissionGoal::Transfer
            };
            job.report.ticks = tick as u64;
            job.record_phase();
        }
        assert!(job.report.phases_truncated);
        assert_eq!(job.report.phases.len(), MAX_SEGMENTS);
        assert_eq!(
            job.report.launch_ticks + job.report.transfer_ticks,
            (MAX_SEGMENTS + 5) as u64
        );
    }
}
