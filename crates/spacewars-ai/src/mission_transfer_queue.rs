//! Bounded scheduling of historical transfer predictions. Ready is not a live
//! remaining-time estimate, a collision certificate or permission to act.
use super::*;
use crate::mission_evaluation::MAX_RESULT_AGE;
use engine_core::planning::{
    JobLimits, JobPhase, JobPoll, PlanningJob, PlanningQueue, PlanningReport, RequestToken, Work,
};
use scenario_spacewars::surface_sortie::{
    LandingPhase, SpacelingId, VehicleId, mission::MissionBoundary,
    transfer_environment::TransferEnvironment,
};
use std::collections::BTreeMap;

#[cfg(test)]
#[path = "mission_neutral_comparison_tests.rs"]
mod neutral_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferQueuePhase {
    Pending,
    Ready,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferForecastState {
    pub token: RequestToken,
    pub actor: PlayerId,
    pub source_tick: u64,
    /// Real controller destination; alternatives never replace this anchor.
    pub destination: usize,
    pub validated_tick: Option<u64>,
    pub completed_tick: Option<u64>,
    pub cancelled_tick: Option<u64>,
    pub phase: TransferQueuePhase,
    pub reason: Option<&'static str>,
    pub charged_graph: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub neutral_validation: Option<crate::mission_evaluation::NeutralTimingValidation>,
}

#[derive(Clone, Copy, PartialEq)]
enum SourceKind {
    Nominated,
    Comparison,
}

#[derive(Clone)]
struct Source {
    local_reference: Option<crate::mission_evaluation::LocalReferenceContext>,
    neutral_reference: Option<crate::mission_evaluation::NeutralTimingContext>,
    arrival_reference: Option<remote_arrival::ArrivalScreenContext>,
    kind: SourceKind,
    actor: PlayerId,
    vehicle: VehicleId,
    spaceling: SpacelingId,
    episode_seed: u64,
    policy: crate::mission_policy::MissionPolicy,
    breaks: CombatBreakSettings,
    bounded_acquisition: bool,
    disengagement: Option<(bool, bool, bool)>,
    selected_tick: u64,
    destination_switched: bool,
    capture_site: Option<Option<LandingSiteId>>,
    health: f32,
    planets: Vec<PilotPlanetObservation>,
    environment: TransferEnvironment,
    boundary: MissionBoundary,
    match_rules: bool,
    match_deadline: Option<Option<f64>>,
}

impl Source {
    fn disengagement_config(bot: &MaterialMissionPilot) -> Option<(bool, bool, bool)> {
        bot.telemetry
            .disengagement
            .as_ref()
            .map(|d| (d.cover_probe, d.boundary_aware, d.handoff_probe))
    }

    fn read(
        bot: &MaterialMissionPilot,
        o: &MissionObservationV1,
        environment: TransferEnvironment,
        kind: SourceKind,
    ) -> Self {
        let p = &o.local.combat.recovery.flight.pilot;
        Self {
            local_reference: None,
            neutral_reference: None,
            arrival_reference: None,
            kind,
            actor: p.owner,
            vehicle: p.vehicle,
            spaceling: p.spaceling,
            episode_seed: bot.context.episode_seed,
            policy: bot.policy,
            breaks: bot.breaks,
            bounded_acquisition: bot.bounded_acquisition,
            disengagement: Self::disengagement_config(bot),
            selected_tick: bot.selected_tick,
            destination_switched: bot.destination_switched,
            capture_site: bot.capture.as_ref().map(|c| c.telemetry().site),
            health: p.ship_health,
            planets: o.planets.clone(),
            environment,
            boundary: o.boundary,
            match_rules: o.match_rules,
            match_deadline: o.match_context.as_ref().map(|m| {
                m.remaining_seconds
                    .map(|s| s + p.tick as f64 * 0.016_666_667)
            }),
        }
    }

    fn validate(
        &mut self,
        state: &mut TransferForecastState,
        bot: &MaterialMissionPilot,
        o: &MissionObservationV1,
        environment: &TransferEnvironment,
        contact: Option<bool>,
    ) -> Result<(), &'static str> {
        let p = &o.local.combat.recovery.flight.pilot;
        if !transfer_forecast::source_schema_matches(o, self.actor)
            || bot.context.actor != self.actor
            || bot.context.episode_seed != self.episode_seed
            || bot.policy != self.policy
            || p.vehicle != self.vehicle
            || p.spaceling != self.spaceling
        {
            return Err("actor or episode changed");
        }
        if bot.breaks != self.breaks
            || bot.bounded_acquisition != self.bounded_acquisition
            || Self::disengagement_config(bot) != self.disengagement
        {
            return Err("controller configuration changed");
        }
        if p.tick < self.environment.tick {
            return Err("clock regressed");
        }
        if p.tick - self.environment.tick > 1 {
            return Err("observation gap");
        }
        if p.tick - state.source_tick > MAX_RESULT_AGE {
            return Err("source expired");
        }
        if o.match_context
            .as_ref()
            .is_some_and(|m| m.finished || m.pilots_alive.iter().any(|alive| !alive))
        {
            return Err("match ended");
        }
        let deadline = o.match_context.as_ref().map(|m| {
            m.remaining_seconds
                .map(|s| s + p.tick as f64 * 0.016_666_667)
        });
        let same_deadline = match (self.match_deadline, deadline) {
            (Some(Some(a)), Some(Some(b))) => {
                a.is_finite() && b.is_finite() && (a - b).abs() < 0.000_001
            }
            (a, b) => a == b,
        };
        if self.match_rules != o.match_rules
            || !same_deadline
            || self.boundary != o.boundary
            || !o.boundary.center.x.is_finite()
            || !o.boundary.center.y.is_finite()
            || !o.boundary.radius.is_finite()
            || o.boundary.radius <= 0.0
        {
            return Err("match or boundary changed");
        }
        if !p.ship_available
            || p.ship_form != ShipForm::Ship
            || p.location != PilotLocation::Aboard(self.vehicle)
            || !p.controls_armed
            || !p.ship_health.is_finite()
            || p.ship_health <= 0.0
            || p.ship_health != self.health
        {
            return Err("ship or pilot changed");
        }
        if contact != Some(false) {
            return Err(if contact == Some(true) {
                "physical contact"
            } else {
                "contact unmeasured"
            });
        }
        if p.landing.phase != LandingPhase::Flying
            || p.landing.supported_feet != 0
            || p.landing.assist_strength != 0.0
            || !o.local.combat.recovery.flight.flight.enabled
        {
            return Err("unassisted flight ended");
        }
        let flight = o.local.combat.recovery.flight.flight;
        if !(0.0..=1.0).contains(&flight.sweep)
            || flight.limits
                != scenario_spacewars::surface_sortie::flight::FlightControlLimits::for_sweep(
                    flight.sweep,
                )
        {
            return Err("flight dynamics unsupported");
        }
        if bot.previous_tick != Some(p.tick)
            || bot.telemetry.target != Some(state.destination)
            || bot.selected_tick != self.selected_tick
            || bot.destination_switched != self.destination_switched
            || bot.capture.as_ref().map(|c| c.telemetry().site) != self.capture_site
            || bot.capture.as_ref().is_some_and(|c| {
                self.kind == SourceKind::Nominated || !destination::uncommitted(c.telemetry())
            })
            || bot.recovery.is_some()
            || bot.telemetry.pursuit.is_some()
            || bot.disengaging()
            || !(matches!(
                bot.telemetry.goal,
                MissionGoal::Launch | MissionGoal::Transfer | MissionGoal::AvoidSun
            ) || (self.kind == SourceKind::Comparison
                && bot.telemetry.goal == MissionGoal::Capture))
        {
            return Err("transfer changed");
        }
        let finite = |v: Vec2| v.x.is_finite() && v.y.is_finite();
        if !finite(p.ship.position)
            || !finite(p.ship.velocity)
            || !finite(p.gravity)
            || !p.ship.angle.is_finite()
            || !p.ship.spin.is_finite()
        {
            return Err("nonfinite observation");
        }
        let ownership = |planet: &PilotPlanetObservation| {
            planet.claim.as_ref().map(|c| {
                (
                    c.owner,
                    c.flag.map(|f| f.player),
                    c.captures,
                    c.neutralizations,
                )
            })
        };
        if self.planets.len() != o.planets.len()
            || self.planets.iter().zip(&o.planets).any(|(a, b)| {
                a.index != b.index
                    || a.radius != b.radius
                    || a.revision != b.revision
                    || ownership(a) != ownership(b)
            })
        {
            return Err("planet material or ownership changed");
        }
        if !environment.matches_source(o) {
            return Err("environment observation mismatch");
        }
        if self
            .local_reference
            .as_ref()
            .is_some_and(|source| !source.matches(o))
        {
            return Err("local reference dependencies changed or expired");
        }
        if p.tick > self.environment.tick {
            self.environment.advance(&mut self.planets);
        }
        if !self.environment.matches_advanced_environment(environment) {
            return Err("environment dynamics changed");
        }
        if let Some(reference) = &self.neutral_reference {
            state.neutral_validation = Some(reference.validate_current(
                o,
                bot.telemetry(),
                bot.capture.as_ref().and_then(|c| c.selected_approach()),
            )?);
        }
        if self
            .arrival_reference
            .as_ref()
            .is_some_and(|r| !r.matches(o))
        {
            return Err("remote arrival solar context changed or source samples expired");
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Slot {
    state: TransferForecastState,
    source: Option<Source>,
}

/// One immutable forecast per actor. The host supplies a current observation
/// and measured contact status after controls and before dispatch/publication.
/// Validation is bounded by eight planets and one ephemeris step, separately
/// from charged prediction work. Missing ticks cancel; there is no catch-up.
#[derive(Clone)]
pub struct TransferForecastQueue<J: PlanningJob = TransferForecastJob> {
    queue: PlanningQueue<(), J>,
    actors: BTreeMap<u64, Slot>,
    capacity: usize,
    last_advance: Option<u64>,
    pub submitted_total: u64,
    pub completed_total: u64,
    pub cancelled_total: u64,
    pub charged_total: u64,
}

impl TransferForecastQueue {
    pub fn submit(
        &mut self,
        bot: &MaterialMissionPilot,
        o: &MissionObservationV1,
        environment: TransferEnvironment,
        contact: Option<bool>,
    ) -> Result<RequestToken, &'static str> {
        let job =
            bot.forecast_nominated_transfer(o, environment.clone(), transfer_forecast::MAX_TICKS)?;
        let source = Source::read(bot, o, environment.clone(), SourceKind::Nominated);
        self.submit_job(bot, o, environment, contact, job, source)
    }
}

impl TransferForecastQueue<TransferComparisonJob> {
    /// Historical geometry/solar screen only; no acquisition-time or threat
    /// estimate and no live candidate ranking uses this record.
    #[allow(clippy::too_many_arguments)] // Keep diagnostics separate from controller input.
    pub fn submit_comparison_with_remote_arrival(
        &mut self,
        before: &MaterialMissionPilot,
        actual: &MaterialMissionPilot,
        o: &MissionObservationV1,
        evaluator: &crate::mission_evaluation::MissionEvaluator,
        environment: TransferEnvironment,
        contact: Option<bool>,
        cover: Option<
            &scenario_spacewars::surface_sortie::destination_cover::DestinationCoverObservation,
        >,
    ) -> Result<RequestToken, &'static str> {
        let job = TransferComparisonJob::new(before, actual, o, evaluator, environment.clone())?
            .with_remote_arrival(o, cover, &environment);
        let mut source = Source::read(actual, o, environment.clone(), SourceKind::Comparison);
        source.local_reference = Some(job.local_context(o));
        source.arrival_reference = job.arrival_context(o);
        self.submit_job(actual, o, environment, contact, job, source)
    }

    /// Opt-in observational join. Numeric timing is created internally from
    /// the actual fresh choice and adds current-state guards to the queue.
    pub fn submit_comparison_with_neutral_timing(
        &mut self,
        before: &MaterialMissionPilot,
        actual: &MaterialMissionPilot,
        o: &MissionObservationV1,
        evaluator: &crate::mission_evaluation::MissionEvaluator,
        environment: TransferEnvironment,
        contact: Option<bool>,
    ) -> Result<RequestToken, &'static str> {
        let job = TransferComparisonJob::new(before, actual, o, evaluator, environment.clone())?
            .with_neutral_timing(actual, o);
        let mut source = Source::read(actual, o, environment.clone(), SourceKind::Comparison);
        source.local_reference = Some(job.local_context(o));
        source.neutral_reference = job
            .neutral_context()
            .map(|r| crate::mission_evaluation::NeutralTimingContext::new(r, o));
        self.submit_job(actual, o, environment, contact, job, source)
    }

    pub fn submit_comparison(
        &mut self,
        before: &MaterialMissionPilot,
        actual: &MaterialMissionPilot,
        o: &MissionObservationV1,
        evaluator: &crate::mission_evaluation::MissionEvaluator,
        environment: TransferEnvironment,
        contact: Option<bool>,
    ) -> Result<RequestToken, &'static str> {
        let job = TransferComparisonJob::new(before, actual, o, evaluator, environment.clone())?;
        let mut source = Source::read(actual, o, environment.clone(), SourceKind::Comparison);
        source.local_reference = Some(job.local_context(o));
        self.submit_job(actual, o, environment, contact, job, source)
    }

    /// Diagnostic progress only, with the same current-observation barrier as
    /// publication. A partial snapshot contains no completed comparison.
    pub fn snapshot(&mut self, token: RequestToken, tick: u64) -> Option<TransferComparisonReport> {
        if matches!(self.poll(token, tick), JobPoll::Stale) {
            return None;
        }
        self.queue.job(token).map(TransferComparisonJob::snapshot)
    }
}

impl<J: PlanningJob> TransferForecastQueue<J> {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: PlanningQueue::new(capacity),
            actors: BTreeMap::new(),
            capacity,
            last_advance: None,
            submitted_total: 0,
            completed_total: 0,
            cancelled_total: 0,
            charged_total: 0,
        }
    }

    pub fn reset(&mut self) {
        self.queue.reset(); // Preserve token generations across episodes.
        self.actors.clear();
        self.last_advance = None;
        self.submitted_total = 0;
        self.completed_total = 0;
        self.cancelled_total = 0;
        self.charged_total = 0;
    }

    pub fn state(&self, actor: PlayerId) -> Option<&TransferForecastState> {
        self.actors.get(&(actor.index() as u64)).map(|s| &s.state)
    }

    fn submit_job(
        &mut self,
        bot: &MaterialMissionPilot,
        o: &MissionObservationV1,
        environment: TransferEnvironment,
        contact: Option<bool>,
        job: J,
        mut source: Source,
    ) -> Result<RequestToken, &'static str> {
        let p = &o.local.combat.recovery.flight.pilot;
        let actor = p.owner.index() as u64;
        if self.last_advance.is_some_and(|tick| tick >= p.tick) {
            return Err("submission must precede dispatch");
        }
        if !self.actors.contains_key(&actor) && self.actors.len() >= self.capacity {
            return Err("forecast capacity");
        }
        if self
            .actors
            .get(&actor)
            .is_some_and(|s| s.state.source_tick == p.tick)
        {
            return Err("source already submitted");
        }
        let mut placeholder = TransferForecastState {
            token: RequestToken {
                actor,
                generation: 0,
            },
            actor: p.owner,
            source_tick: p.tick,
            destination: bot.telemetry.target.ok_or("no current destination")?,
            validated_tick: Some(p.tick),
            completed_tick: None,
            cancelled_tick: None,
            phase: TransferQueuePhase::Pending,
            reason: None,
            charged_graph: 0,
            neutral_validation: None,
        };
        source.validate(&mut placeholder, bot, o, &environment, contact)?;
        if let Some(old) = self.state(p.owner) {
            self.cancel(old.token, p.tick, "replaced");
        }
        let token = self
            .queue
            .submit(actor, (), JobLimits::default(), job)
            .map_err(|_| "forecast capacity")?;
        self.actors.insert(
            actor,
            Slot {
                state: TransferForecastState {
                    token,
                    ..placeholder
                },
                source: Some(source),
            },
        );
        self.submitted_total += 1;
        Ok(token)
    }

    pub fn cancel(&mut self, token: RequestToken, tick: u64, reason: &'static str) {
        if self.queue.cancel(token) {
            let slot = self.actors.get_mut(&token.actor).unwrap();
            slot.state.phase = TransferQueuePhase::Stale;
            slot.state.reason = Some(reason);
            slot.state.cancelled_tick = Some(tick);
            slot.state.validated_tick = None;
            slot.state.neutral_validation = None;
            slot.source = None;
            self.cancelled_total += 1;
        }
    }

    pub fn observe(
        &mut self,
        token: RequestToken,
        bot: &MaterialMissionPilot,
        o: &MissionObservationV1,
        environment: &TransferEnvironment,
        contact: Option<bool>,
    ) {
        let p = &o.local.combat.recovery.flight.pilot;
        let Some(slot) = self
            .actors
            .get_mut(&token.actor)
            .filter(|s| s.state.token == token)
        else {
            return;
        };
        let Some(source) = &mut slot.source else {
            return;
        };
        match source.validate(&mut slot.state, bot, o, environment, contact) {
            Ok(()) => slot.state.validated_tick = Some(p.tick),
            Err(reason) => {
                let token = slot.state.token;
                self.cancel(token, p.tick, reason);
            }
        }
    }

    /// The tick must be the current real observation tick, never the source tick
    /// retained by a caller. Historical report duration is not time remaining.
    pub fn poll(&mut self, token: RequestToken, tick: u64) -> JobPoll<'_, J::Output> {
        if self
            .actors
            .get(&token.actor)
            .is_some_and(|s| s.state.token == token && s.state.validated_tick != Some(tick))
        {
            self.cancel(token, tick, "current observation missing");
        }
        self.queue.poll(token, &())
    }

    /// Consume only the host's residual allowance, once per real tick. Retire
    /// missing/old observations even when there is no work available.
    pub fn advance(&mut self, tick: u64, remaining: Work) -> Option<PlanningReport> {
        let reversed = self.last_advance.is_some_and(|old| tick < old);
        let stale: Vec<_> = self
            .actors
            .values()
            .filter(|s| s.source.is_some() && (reversed || s.state.validated_tick != Some(tick)))
            .map(|s| s.state.token)
            .collect();
        for token in stale {
            self.cancel(
                token,
                tick,
                if reversed {
                    "clock regressed"
                } else {
                    "current observation missing"
                },
            );
        }
        if self.last_advance.is_some_and(|old| tick <= old) {
            return None;
        }
        self.last_advance = Some(tick);
        let allocation = self.queue.advance(Work {
            graph: remaining.graph,
            physics_queries: 0,
        });
        self.charged_total += u64::from(allocation.charged.graph);
        for a in &allocation.jobs {
            let state = &mut self.actors.get_mut(&a.request.actor).unwrap().state;
            state.charged_graph += u64::from(a.charged.graph);
            if a.phase == JobPhase::Ready && state.phase == TransferQueuePhase::Pending {
                state.phase = TransferQueuePhase::Ready;
                state.completed_tick = Some(tick);
                self.completed_total += 1;
            }
        }
        Some(allocation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::planning::PlanningJob;

    fn work(graph: u32) -> Work {
        Work {
            graph,
            physics_queries: 999,
        }
    }

    // Synthetic observations isolate lifetime/accounting from controller/physics
    // behavior; native world agreement is tested in transfer_environment.
    pub(super) fn next(
        bot: &mut MaterialMissionPilot,
        o: &mut MissionObservationV1,
        e: &mut TransferEnvironment,
    ) {
        e.advance(&mut o.planets);
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = e.tick;
        p.planet = o.planets[p.planet.index].clone();
        bot.previous_tick = Some(p.tick);
        if let Some(seconds) = o
            .match_context
            .as_mut()
            .and_then(|m| m.remaining_seconds.as_mut())
        {
            *seconds -= 0.016_666_667;
        }
    }

    #[test]
    fn expiry_applies_to_pending_and_ready_and_completion_at_age_120_is_valid() {
        let (state, bot, o) = transfer_forecast::tests::source();
        let e = state.transfer_environment().unwrap();
        let mut direct = bot
            .forecast_nominated_transfer(&o, e.clone(), 3600)
            .unwrap();
        while direct.next_work().is_some() {
            direct.step();
        }
        let expected = direct.output().unwrap();
        for start_work in [0, expected.charged_graph as u32 - 1, 3600] {
            let (mut bot, mut o, mut e) = (bot.clone(), o.clone(), e.clone());
            let mut q = TransferForecastQueue::new(1);
            let token = q.submit(&bot, &o, e.clone(), Some(false)).unwrap();
            q.advance(e.tick, work(start_work));
            for age in 1..=121 {
                next(&mut bot, &mut o, &mut e);
                q.observe(token, &bot, &o, &e, Some(false));
                let allocation = q
                    .advance(e.tick, work(u32::from(age == 120 && start_work > 0)))
                    .unwrap();
                assert_eq!(allocation.charged.physics_queries, 0);
                if age <= 120 {
                    if start_work > 0 && (start_work == 3600 || age == 120) {
                        assert_eq!(q.poll(token, e.tick), JobPoll::Ready(expected));
                    } else {
                        assert_eq!(q.poll(token, e.tick), JobPoll::Pending);
                    }
                } else {
                    assert_eq!(allocation.charged.graph, 0);
                    assert_eq!(q.poll(token, e.tick), JobPoll::Stale);
                    assert_eq!(
                        q.state(bot.context.actor).unwrap().reason,
                        Some("source expired")
                    );
                }
            }
            assert_eq!(q.cancelled_total, 1);
            assert_eq!(q.completed_total, u64::from(start_work > 0));
            if start_work > 0 {
                assert_eq!(q.charged_total, expected.charged_graph);
                assert_eq!(
                    q.state(bot.context.actor).unwrap().completed_tick,
                    Some(state.tick() + if start_work == 3600 { 0 } else { 120 })
                );
            }
        }
    }

    #[test]
    fn one_unit_residual_is_fair_and_repeated_ticks_cannot_renew_work() {
        let (state, b0, o0) = transfer_forecast::tests::source();
        let mut b1 = b0.clone();
        b1.context.actor = PlayerId::PLAYER_2;
        let mut o1 = o0.clone();
        let p = &mut o1.local.combat.recovery.flight.pilot;
        p.owner = b1.context.actor;
        p.vehicle = VehicleId(1);
        p.spaceling = SpacelingId(1);
        p.location = PilotLocation::Aboard(p.vehicle);
        for reverse in [false, true] {
            let mut bots = [b0.clone(), b1.clone()];
            let mut obs = [o0.clone(), o1.clone()];
            let mut envs = [
                state.transfer_environment().unwrap(),
                state.transfer_environment().unwrap(),
            ];
            let mut q = TransferForecastQueue::new(2);
            let mut tokens = [None; 2];
            for i in if reverse { [1, 0] } else { [0, 1] } {
                tokens[i] = Some(
                    q.submit(&bots[i], &obs[i], envs[i].clone(), Some(false))
                        .unwrap(),
                );
            }
            for age in 0..10 {
                for i in 0..2 {
                    if age > 0 {
                        next(&mut bots[i], &mut obs[i], &mut envs[i]);
                    }
                    for _ in 0..2 {
                        q.observe(tokens[i].unwrap(), &bots[i], &obs[i], &envs[i], Some(false));
                    }
                }
                let tick = envs[0].tick;
                let a = q.advance(tick, work(1)).unwrap();
                assert_eq!(
                    a.charged,
                    Work {
                        graph: 1,
                        physics_queries: 0
                    }
                );
                assert!(q.advance(tick, work(100)).is_none());
                assert_eq!(
                    a.jobs
                        .iter()
                        .find(|j| j.charged.graph == 1)
                        .unwrap()
                        .request
                        .actor,
                    age % 2
                );
            }
            assert_eq!(q.charged_total, 10);
            assert_eq!(q.state(PlayerId::PLAYER_1).unwrap().charged_graph, 5);
            assert_eq!(q.state(PlayerId::PLAYER_2).unwrap().charged_graph, 5);
            let tick = envs[0].tick;
            q.advance(tick + 1, work(0)); // Missing observations cancel both, even without work.
            assert_eq!(q.cancelled_total, 2);
            for token in tokens {
                assert_eq!(q.poll(token.unwrap(), tick + 1), JobPoll::Stale);
            }
            assert!(q.advance(tick, work(100)).is_none());
            assert_eq!(q.charged_total, 10);
        }
    }

    #[test]
    fn reset_keeps_generations_and_old_tokens_cannot_cancel_replacements() {
        let (state, bot, o) = transfer_forecast::tests::source();
        let e = state.transfer_environment().unwrap();
        let mut q = TransferForecastQueue::new(1);
        let old = q.submit(&bot, &o, e.clone(), Some(false)).unwrap();
        assert!(q.submit(&bot, &o, e.clone(), Some(false)).is_err());
        q.advance(e.tick, work(3600));
        q.reset();
        let new = q.submit(&bot, &o, e.clone(), Some(false)).unwrap();
        assert_ne!(old, new);
        q.cancel(old, e.tick, "old cancellation");
        q.observe(old, &bot, &o, &e, Some(true));
        assert_eq!(q.poll(old, e.tick), JobPoll::Stale);
        assert_eq!(q.poll(new, e.tick), JobPoll::Pending);
        assert_eq!(q.advance(e.tick, work(1)).unwrap().charged.graph, 1);
        assert!(q.submit(&bot, &o, e, Some(false)).is_err());
        assert_eq!(q.poll(new, state.tick() + 1), JobPoll::Stale); // No stale-ready publication.
    }

    #[test]
    fn changed_context_cancels_pending_and_ready_even_on_the_same_tick() {
        type Mutation = fn(&mut MaterialMissionPilot, &mut MissionObservationV1);
        let changes: &[Mutation] = &[
            |b, _| b.context.actor = PlayerId::PLAYER_2,
            |b, _| b.context.episode_seed += 1,
            |b, _| b.policy = crate::mission_policy::MissionPolicy::Legacy,
            |b, _| b.breaks.interval_seconds += 1,
            |b, _| b.bounded_acquisition = !b.bounded_acquisition,
            |b, _| b.enable_pursuit_disengagement(true),
            |b, _| b.selected_tick += 1,
            |b, _| b.telemetry.target = None,
            |b, _| b.telemetry.goal = MissionGoal::Recover,
            |_, o| o.local.combat.recovery.flight.pilot.owner = PlayerId::PLAYER_2,
            |_, o| o.local.combat.recovery.flight.pilot.vehicle = VehicleId(9),
            |_, o| o.local.combat.recovery.flight.pilot.spaceling = SpacelingId(9),
            |_, o| o.local.combat.recovery.flight.pilot.ship_health -= 0.01,
            |_, o| o.local.combat.recovery.flight.pilot.ship.position.x = f32::NAN,
            |_, o| o.local.combat.recovery.flight.pilot.landing.supported_feet = 1,
            |_, o| o.local.combat.recovery.flight.pilot.location = PilotLocation::OnFoot,
            |_, o| {
                o.local
                    .combat
                    .recovery
                    .flight
                    .flight
                    .limits
                    .thrust_acceleration += 1.0
            },
            |_, o| o.local.combat.recovery.flight.flight.sweep = f32::NAN,
            |_, o| o.planets[0].radius += 1.0,
            |_, o| o.planets[0].claim.as_mut().unwrap().captures += 1,
            |_, o| o.boundary.radius += 1.0,
            |_, o| o.match_rules = !o.match_rules,
            |_, o| o.match_context.as_mut().unwrap().remaining_seconds = Some(1.0),
            |_, o| o.match_context.as_mut().unwrap().finished = true,
            |_, o| o.version += 1,
        ];
        let (state, bot, o) = transfer_forecast::tests::source();
        let e = state.transfer_environment().unwrap();
        for initial in [1, 3600] {
            for change in changes {
                let mut q = TransferForecastQueue::new(1);
                let token = q.submit(&bot, &o, e.clone(), Some(false)).unwrap();
                q.advance(e.tick, work(initial));
                let (mut b, mut changed) = (bot.clone(), o.clone());
                change(&mut b, &mut changed);
                q.observe(token, &b, &changed, &e, Some(false));
                assert_eq!(q.poll(token, e.tick), JobPoll::Stale);
                assert!(q.advance(e.tick, work(3600)).is_none());
                assert_eq!(q.cancelled_total, 1);
            }
            for contact in [None, Some(true)] {
                let mut q = TransferForecastQueue::new(1);
                assert!(q.submit(&bot, &o, e.clone(), contact).is_err());
                let token = q.submit(&bot, &o, e.clone(), Some(false)).unwrap();
                q.advance(e.tick, work(initial));
                q.observe(token, &bot, &o, &e, contact);
                assert_eq!(q.poll(token, e.tick), JobPoll::Stale);
            }
        }
    }

    #[test]
    fn missing_or_regressed_observations_are_never_caught_up_or_recharged() {
        let (state, bot, o) = transfer_forecast::tests::source();
        let e = state.transfer_environment().unwrap();
        for offset in [-1_i64, 2] {
            let mut q = TransferForecastQueue::new(1);
            let token = q.submit(&bot, &o, e.clone(), Some(false)).unwrap();
            q.advance(e.tick, work(1));
            let mut changed = o.clone();
            changed.local.combat.recovery.flight.pilot.tick = (e.tick as i64 + offset) as u64;
            q.observe(token, &bot, &changed, &e, Some(false));
            assert_eq!(q.poll(token, e.tick), JobPoll::Stale);
            assert_eq!(q.charged_total, 1);
        }
    }

    #[test]
    fn local_evidence_expiry_revokes_pending_and_ready_comparisons_without_more_work() {
        use crate::mission_evaluation::{
            LocalEvidenceSource, LocalReferenceContext, MissionEvaluator, PhaseCosts,
        };
        let (state, before, bot, o) = transfer_forecast::tests::source_with_before();
        for ready in [false, true] {
            let (mut bot, mut o, mut environment) = (
                bot.clone(),
                o.clone(),
                state.transfer_environment().unwrap(),
            );
            // Put a valid flight source exactly on the evidence-age boundary.
            o.local.combat.recovery.flight.pilot.tick = 1800;
            bot.previous_tick = Some(1800);
            environment.tick = 1800;
            let mut q = TransferForecastQueue::<TransferComparisonJob>::new(1);
            let token = q
                .submit_comparison(
                    &before,
                    &bot,
                    &o,
                    &MissionEvaluator::new(1),
                    environment.clone(),
                    Some(false),
                )
                .unwrap();
            let mut local = q.snapshot(token, 1800).unwrap().candidates[0]
                .local_reference
                .clone();
            local.remaining = Some(PhaseCosts {
                landing: 1.0,
                exit: 0.0,
                outbound: 0.0,
                claim: 3.0,
                return_board: 0.0,
                departure: 1.0,
            });
            let planet = &o.planets[0];
            local.evidence = Some(LocalEvidenceSource {
                site: LandingSiteId {
                    planet: planet.index,
                    bearing: 0,
                },
                revision: planet.revision,
                observed_owner: None,
                radius: planet.radius,
                stage_seconds: 3.0,
                flag_range: 3.0,
                remote: true,
                tick: 0,
                age_ticks: 1800,
                gravity: 0.0,
                route_source_tick: None,
                route_validated_tick: None,
                route_objective: None,
                choice: None,
            });
            q.actors
                .get_mut(&0)
                .unwrap()
                .source
                .as_mut()
                .unwrap()
                .local_reference = Some(LocalReferenceContext::read(&o, [&local].into_iter()));
            q.advance(1800, work(if ready { 10801 } else { 0 }));
            assert_eq!(matches!(q.poll(token, 1800), JobPoll::Ready(_)), ready);
            let charged = q.charged_total;
            next(&mut bot, &mut o, &mut environment);
            q.observe(token, &bot, &o, &environment, Some(false));
            assert_eq!(q.advance(1801, work(10801)).unwrap().charged.graph, 0);
            assert_eq!(q.poll(token, 1801), JobPoll::Stale);
            assert_eq!(q.charged_total, charged);
            assert_eq!(
                q.state(bot.context.actor).unwrap().reason,
                Some("local reference dependencies changed or expired")
            );
        }
    }
}
