//! Historical travel comparisons. These predict a free-flight handoff, never
//! landing/capture value, and are not consumed by the playing controller.
use super::*;
use crate::mission_evaluation::MissionEvaluator;
use engine_common::Action;
use engine_core::planning::{PlanningJob, WorkKind};
use scenario_spacewars::surface_sortie::transfer_environment::TransferEnvironment;

const MAX_CANDIDATES: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferCandidateForecast {
    pub destination: usize,
    pub current: bool,
    pub nomination: Option<DestinationProbeResult>,
    pub source_actions: [Action; 3],
    pub unknown: Option<&'static str>,
    pub forecast: Option<TransferForecastReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferComparisonReport {
    pub model: &'static str,
    pub actor: PlayerId,
    pub source_tick: u64,
    pub current_destination: usize,
    pub current_selected_tick: u64,
    pub current_goal: MissionGoal,
    pub pre_intent_destination: Option<usize>,
    pub evaluator_source_tick: Option<u64>,
    pub candidates_truncated: bool,
    pub candidates: Vec<TransferCandidateForecast>,
    pub charged_graph: u64,
    pub ranked: bool,
    /// Equal-tick ties in destination order, among known handoffs only.
    pub fastest_known_handoffs: Vec<usize>,
    /// Only populated when the whole shortlist has numeric handoffs. This is
    /// a travel comparison, not a capture-value preference or permission.
    pub preferred_handoffs: Vec<usize>,
}

#[derive(Clone)]
pub struct TransferComparisonJob {
    report: TransferComparisonReport,
    jobs: Vec<Option<TransferForecastJob>>,
    cursor: usize,
}

impl TransferComparisonJob {
    pub(super) fn new(
        before: &MaterialMissionPilot,
        actual: &MaterialMissionPilot,
        o: &MissionObservationV1,
        evaluator: &MissionEvaluator,
        environment: TransferEnvironment,
    ) -> Result<Self, &'static str> {
        let p = &o.local.combat.recovery.flight.pilot;
        let config = |bot: &MaterialMissionPilot| {
            bot.telemetry
                .disengagement
                .as_ref()
                .map(|d| (d.cover_probe, d.boundary_aware, d.handoff_probe))
        };
        if before.context.actor != actual.context.actor
            || before.context.episode_seed != actual.context.episode_seed
            || before.policy != actual.policy
            || before.breaks != actual.breaks
            || before.bounded_acquisition != actual.bounded_acquisition
            || config(before) != config(actual)
            || before.previous_tick.is_some_and(|tick| tick >= p.tick)
            || actual.previous_tick != Some(p.tick)
            || !transfer_forecast::source_schema_matches(o, actual.context.actor)
            || !environment.matches_source(o)
        {
            return Err("comparison needs pre-intent and current source controllers");
        }
        let current = actual.telemetry.target.ok_or("no current destination")?;
        let mut alternatives: Vec<_> = o
            .planets
            .iter()
            .filter(|planet| {
                planet.index != current
                    && planet
                        .claim
                        .as_ref()
                        .is_none_or(|c| c.owner != Some(p.owner))
            })
            .map(|planet| planet.index)
            .collect();
        alternatives.sort_unstable();
        let truncated = alternatives.len() >= MAX_CANDIDATES;
        alternatives.truncate(MAX_CANDIDATES - 1);
        let mut job = Self {
            report: TransferComparisonReport {
                model: "guided_transfer_comparison_v1",
                actor: p.owner,
                source_tick: p.tick,
                current_destination: current,
                current_selected_tick: actual.selected_tick,
                current_goal: actual.telemetry.goal,
                pre_intent_destination: before.telemetry.target,
                evaluator_source_tick: evaluator.latest(p.owner).map(|r| r.source_tick),
                candidates_truncated: truncated,
                candidates: Vec::with_capacity(MAX_CANDIDATES),
                charged_graph: 0,
                ranked: false,
                fastest_known_handoffs: Vec::new(),
                preferred_handoffs: Vec::new(),
            },
            jobs: Vec::with_capacity(MAX_CANDIDATES),
            cursor: 0,
        };
        job.add(
            current,
            true,
            None,
            actual.previous_intent.encode(p.owner),
            actual.forecast_current_transfer(o, environment.clone(), transfer_forecast::MAX_TICKS),
        );
        for destination in alternatives {
            // Every hypothetical command starts from the same pre-intent state.
            // Rejections retain ordinary controls and remain explicit unknowns.
            let mut candidate = before.clone();
            let (intent, nomination) =
                candidate.intent_with_destination_probe(o, evaluator, destination);
            let forecast = if nomination.accepted {
                candidate.forecast_nominated_transfer(
                    o,
                    environment.clone(),
                    transfer_forecast::MAX_TICKS,
                )
            } else {
                Err(nomination.reason.unwrap_or("nomination rejected"))
            };
            job.add(
                destination,
                false,
                Some(nomination),
                intent.encode(p.owner),
                forecast,
            );
        }
        Ok(job)
    }

    fn add(
        &mut self,
        destination: usize,
        current: bool,
        nomination: Option<DestinationProbeResult>,
        source_actions: [Action; 3],
        result: Result<TransferForecastJob, &'static str>,
    ) {
        let (forecast, unknown) = match result {
            Ok(job) => (Some(job), None),
            Err(reason) => (None, Some(reason)),
        };
        self.report.candidates.push(TransferCandidateForecast {
            destination,
            current,
            nomination,
            source_actions,
            unknown,
            forecast: forecast.as_ref().map(|job| job.report().clone()),
        });
        self.jobs.push(forecast);
    }

    pub fn snapshot(&self) -> TransferComparisonReport {
        let mut report = self.report.clone();
        for (candidate, job) in report.candidates.iter_mut().zip(&self.jobs) {
            if let Some(job) = job {
                candidate.forecast = Some(job.report().clone());
            }
        }
        report
    }

    fn rank(&mut self) {
        let known: Vec<_> = self
            .report
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .forecast
                    .as_ref()
                    .filter(|f| f.end == Some(TransferForecastEnd::KinematicHandoff))
                    .map(|f| (f.ticks, candidate.destination))
            })
            .collect();
        if let Some(minimum) = known.iter().map(|(ticks, _)| *ticks).min() {
            self.report.fastest_known_handoffs = known
                .iter()
                .filter(|(ticks, _)| *ticks == minimum)
                .map(|(_, destination)| *destination)
                .collect();
            self.report.fastest_known_handoffs.sort_unstable();
        }
        if !self.report.candidates_truncated
            && known.len() == self.report.candidates.len()
            && known.len() >= 2
        {
            self.report.preferred_handoffs = self.report.fastest_known_handoffs.clone();
        }
        self.report.ranked = true;
    }
}

impl PlanningJob for TransferComparisonJob {
    type Output = TransferComparisonReport;

    fn next_work(&self) -> Option<WorkKind> {
        (!self.report.ranked).then_some(WorkKind::Graph)
    }

    fn step(&mut self) {
        if self.report.ranked {
            return;
        }
        self.report.charged_graph += 1;
        for offset in 0..self.jobs.len() {
            let index = (self.cursor + offset) % self.jobs.len();
            if let Some(job) = &mut self.jobs[index] {
                job.step();
                if let Some(output) = job.output() {
                    self.report.candidates[index].forecast = Some(output.clone());
                    self.jobs[index] = None;
                }
                self.cursor = (index + 1) % self.jobs.len();
                return;
            }
        }
        // A separate charged operation, even when every candidate was unknown.
        self.rank();
    }

    fn output(&self) -> Option<&Self::Output> {
        self.report.ranked.then_some(&self.report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::planning::{JobPoll, Work};
    use scenario_spacewars::surface_sortie::{SpacelingId, VehicleId};

    fn work(graph: u32) -> Work {
        Work {
            graph,
            physics_queries: 999,
        }
    }

    fn next(
        bot: &mut MaterialMissionPilot,
        o: &mut MissionObservationV1,
        e: &mut TransferEnvironment,
    ) {
        e.advance(&mut o.planets);
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = e.tick;
        p.planet = o.planets[p.planet.index].clone();
        bot.previous_tick = Some(p.tick);
        if let Some(s) = o
            .match_context
            .as_mut()
            .and_then(|m| m.remaining_seconds.as_mut())
        {
            *s -= 0.016_666_667;
        }
    }

    #[test]
    fn each_branch_preserves_its_command_and_nomination_epoch() {
        let (state, before, mut actual, o) = transfer_forecast::tests::source_with_before();
        // An existing trip need not have been nominated on the source tick.
        actual.destination_switched = false;
        actual.selected_tick -= 1;
        actual.previous_intent.flight.controls.primary_held = false;
        actual.previous_intent.flight.controls.brake_held = true;
        let e = state.transfer_environment().unwrap();
        assert!(
            actual
                .forecast_nominated_transfer(&o, e.clone(), 3600)
                .is_err()
        );
        let evaluator = MissionEvaluator::new(1);
        let mut comparison =
            TransferComparisonJob::new(&before, &actual, &o, &evaluator, e.clone()).unwrap();
        let candidates = comparison.snapshot().candidates;
        assert_eq!(
            candidates[0].source_actions,
            actual.previous_intent.encode(actual.context.actor)
        );
        let mut expected = Vec::new();
        for c in &candidates {
            let result = if c.current {
                actual.forecast_current_transfer(&o, e.clone(), 3600)
            } else {
                let mut independent = before.clone();
                let (intent, nomination) =
                    independent.intent_with_destination_probe(&o, &evaluator, c.destination);
                assert_eq!(c.nomination, Some(nomination.clone()));
                assert_eq!(c.source_actions, intent.encode(actual.context.actor));
                if nomination.accepted {
                    independent.forecast_nominated_transfer(&o, e.clone(), 3600)
                } else {
                    Err(nomination.reason.unwrap())
                }
            };
            let report = result.ok().map(|mut job| {
                while job.next_work().is_some() {
                    job.step();
                }
                job.output().unwrap().clone()
            });
            expected.push(report);
        }
        // One operation per runnable candidate, then round robin again.
        let active = comparison.jobs.iter().flatten().count();
        for _ in 0..active {
            comparison.step();
        }
        assert!(
            comparison
                .snapshot()
                .candidates
                .iter()
                .filter_map(|c| c.forecast.as_ref())
                .all(|f| f.charged_graph == 1)
        );
        assert!(!comparison.snapshot().ranked);
        while comparison.next_work().is_some() {
            comparison.step();
        }
        let r = comparison.output().unwrap();
        assert_eq!(
            r.candidates
                .iter()
                .map(|c| c.forecast.clone())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            r.charged_graph,
            expected
                .iter()
                .flatten()
                .map(|f| f.charged_graph)
                .sum::<u64>()
                + 1
        );
        assert!(r.charged_graph <= 3 * transfer_forecast::MAX_TICKS + 1);
        assert!(TransferComparisonJob::new(&actual, &actual, &o, &evaluator, e).is_err());
    }

    #[test]
    fn unknown_current_and_ties_never_imply_a_complete_preference() {
        let (state, before, actual, o) = transfer_forecast::tests::source_with_before();
        let e = state.transfer_environment().unwrap();
        let evaluator = MissionEvaluator::new(1);
        let mut job = TransferComparisonJob::new(&before, &actual, &o, &evaluator, e).unwrap();
        let template = job.report.candidates[0].forecast.clone().unwrap();
        job.report.candidates.truncate(1);
        let mut alternative = job.report.candidates[0].clone();
        alternative.current = false;
        alternative.destination = (alternative.destination + 1) % 3;
        alternative.forecast = Some(template);
        job.report.candidates.push(alternative);
        for c in &mut job.report.candidates {
            let f = c.forecast.as_mut().unwrap();
            f.end = Some(TransferForecastEnd::KinematicHandoff);
            f.ticks = 100;
            f.handoff_seconds = Some(100.0 / 60.0);
        }
        job.rank();
        assert_eq!(job.report.preferred_handoffs.len(), 2);
        assert!(
            job.report
                .preferred_handoffs
                .windows(2)
                .all(|p| p[0] < p[1])
        );
        job.report.preferred_handoffs.clear();
        job.report.candidates_truncated = true;
        job.rank();
        assert!(job.report.preferred_handoffs.is_empty());
        job.report.candidates_truncated = false;
        job.report.candidates[0].forecast = None;
        job.report.candidates[0].unknown = Some("current local phase unsupported");
        job.rank();
        assert_eq!(job.report.fastest_known_handoffs.len(), 1);
        assert!(job.report.preferred_handoffs.is_empty());
    }

    #[test]
    fn current_local_phase_stays_unknown_while_alternatives_run() {
        let (state, before, mut actual, o) = transfer_forecast::tests::source_with_before();
        actual.telemetry.target = before.telemetry.target;
        actual.capture = Some(TacticalCapturePilot::new(actual.context, actual.breaks));
        actual.telemetry.goal = MissionGoal::Capture;
        let mut job = TransferComparisonJob::new(
            &before,
            &actual,
            &o,
            &MissionEvaluator::new(1),
            state.transfer_environment().unwrap(),
        )
        .unwrap();
        assert!(job.report.candidates[0].unknown.is_some());
        assert!(job.jobs.iter().skip(1).any(Option::is_some));
        while job.next_work().is_some() {
            job.step();
        }
        assert!(job.output().unwrap().preferred_handoffs.is_empty());
    }

    #[test]
    fn final_ranking_is_charged_even_if_no_forecast_can_run() {
        let (state, before, mut actual, o) = transfer_forecast::tests::source_with_before();
        actual.capture = Some(TacticalCapturePilot::new(actual.context, actual.breaks));
        actual.telemetry.goal = MissionGoal::Capture;
        let mut before = before;
        before.destination_switched = true; // Refuses further nominations.
        let mut q = TransferComparisonQueue::new(1);
        let e = state.transfer_environment().unwrap();
        let token = q
            .submit_comparison(
                &before,
                &actual,
                &o,
                &MissionEvaluator::new(1),
                e.clone(),
                Some(false),
            )
            .unwrap();
        assert!(
            q.snapshot(token, e.tick)
                .unwrap()
                .candidates
                .iter()
                .all(|c| c.unknown.is_some())
        );
        assert_eq!(q.advance(e.tick, work(0)).unwrap().charged.graph, 0);
        assert_eq!(q.poll(token, e.tick), JobPoll::Pending);
        let (mut o, mut e) = (o, e);
        next(&mut actual, &mut o, &mut e);
        q.observe(token, &actual, &o, &e, Some(false));
        assert_eq!(q.advance(e.tick, work(1)).unwrap().charged.graph, 1);
        let JobPoll::Ready(r) = q.poll(token, e.tick) else {
            panic!("ranking not complete");
        };
        assert!(r.ranked && r.preferred_handoffs.is_empty() && r.fastest_known_handoffs.is_empty());
        assert_eq!(r.charged_graph, 1);
    }

    #[test]
    fn real_task_guards_cancel_comparisons_independently_of_hypothetical_destinations() {
        let (state, before, actual, o) = transfer_forecast::tests::source_with_before();
        let e = state.transfer_environment().unwrap();
        let evaluator = MissionEvaluator::new(1);
        type Mutation = fn(&mut MaterialMissionPilot);
        let changes: &[Mutation] = &[
            |b| b.selected_tick += 1,
            |b| b.destination_switched = !b.destination_switched,
            |b| b.telemetry.target = Some((b.telemetry.target.unwrap() + 1) % 3),
            |b| b.capture = Some(TacticalCapturePilot::new(b.context, b.breaks)),
            |b| b.telemetry.goal = MissionGoal::Hunt,
        ];
        for change in changes {
            let mut q = TransferComparisonQueue::new(1);
            let token = q
                .submit_comparison(&before, &actual, &o, &evaluator, e.clone(), Some(false))
                .unwrap();
            q.advance(e.tick, work(1));
            let mut changed = actual.clone();
            change(&mut changed);
            q.observe(token, &changed, &o, &e, Some(false));
            assert_eq!(q.poll(token, e.tick), JobPoll::Stale);
            assert!(q.snapshot(token, e.tick).is_none());
        }
        let mut q = TransferComparisonQueue::new(1);
        let token = q
            .submit_comparison(&before, &actual, &o, &evaluator, e.clone(), Some(false))
            .unwrap();
        assert_eq!(
            q.state(actual.context.actor).unwrap().destination,
            actual.telemetry.target.unwrap()
        );
        let (mut actual, mut o, mut e) = (actual, o, e);
        for age in 0..=121 {
            if age > 0 {
                next(&mut actual, &mut o, &mut e);
            }
            q.observe(token, &actual, &o, &e, Some(false));
            assert_eq!(
                q.advance(e.tick, work(if age == 121 { 10801 } else { 0 }))
                    .unwrap()
                    .charged
                    .graph,
                0
            );
            assert_eq!(
                q.poll(token, e.tick),
                if age < 121 {
                    JobPoll::Pending
                } else {
                    JobPoll::Stale
                }
            );
        }
        assert_eq!(
            q.state(actual.context.actor).unwrap().reason,
            Some("source expired")
        );
    }

    #[test]
    fn expiry_cannot_spend_the_last_ranking_operation() {
        let (state, before, mut actual, mut o) = transfer_forecast::tests::source_with_before();
        let mut e = state.transfer_environment().unwrap();
        let evaluator = MissionEvaluator::new(1);
        let mut direct =
            TransferComparisonJob::new(&before, &actual, &o, &evaluator, e.clone()).unwrap();
        while direct.next_work().is_some() {
            direct.step();
        }
        let motor_work = direct.output().unwrap().charged_graph - 1;
        let mut q = TransferComparisonQueue::new(1);
        let token = q
            .submit_comparison(&before, &actual, &o, &evaluator, e.clone(), Some(false))
            .unwrap();
        for age in 0..=121 {
            if age > 0 {
                next(&mut actual, &mut o, &mut e);
            }
            q.observe(token, &actual, &o, &e, Some(false));
            q.advance(
                e.tick,
                work(if age == 120 {
                    motor_work as u32
                } else {
                    u32::from(age == 121)
                }),
            );
            if age == 120 {
                let report = q.snapshot(token, e.tick).unwrap();
                assert!(
                    report
                        .candidates
                        .iter()
                        .filter_map(|c| c.forecast.as_ref())
                        .all(|f| f.end.is_some())
                );
                assert!(!report.ranked);
                assert_eq!(q.poll(token, e.tick), JobPoll::Pending);
            }
        }
        assert_eq!(q.charged_total, motor_work);
        assert_eq!(q.completed_total, 0);
        assert_eq!(q.poll(token, e.tick), JobPoll::Stale);
    }

    #[test]
    fn two_actor_candidate_groups_share_one_allowance() {
        let (state, before, actual, o) = transfer_forecast::tests::source_with_before();
        let mut bots = [actual.clone(), actual];
        let mut befores = [before.clone(), before];
        let mut obs = [o.clone(), o];
        let mut envs = [
            state.transfer_environment().unwrap(),
            state.transfer_environment().unwrap(),
        ];
        bots[1].context.actor = PlayerId::PLAYER_2;
        befores[1].context.actor = PlayerId::PLAYER_2;
        let p = &mut obs[1].local.combat.recovery.flight.pilot;
        p.owner = PlayerId::PLAYER_2;
        p.vehicle = VehicleId(1);
        p.spaceling = SpacelingId(1);
        p.location = PilotLocation::Aboard(p.vehicle);
        let mut q = TransferComparisonQueue::new(2);
        let mut tokens = [None; 2];
        for i in [1, 0] {
            tokens[i] = Some(
                q.submit_comparison(
                    &befores[i],
                    &bots[i],
                    &obs[i],
                    &MissionEvaluator::new(2),
                    envs[i].clone(),
                    Some(false),
                )
                .unwrap(),
            );
        }
        for age in 0..12 {
            for i in 0..2 {
                if age > 0 {
                    next(&mut bots[i], &mut obs[i], &mut envs[i]);
                }
                q.observe(tokens[i].unwrap(), &bots[i], &obs[i], &envs[i], Some(false));
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
            assert_eq!(
                a.jobs
                    .iter()
                    .find(|j| j.charged.graph == 1)
                    .unwrap()
                    .request
                    .actor,
                age % 2
            );
            assert!(q.advance(tick, work(99)).is_none());
        }
        for actor in [PlayerId::PLAYER_1, PlayerId::PLAYER_2] {
            assert_eq!(q.state(actor).unwrap().charged_graph, 6);
        }
        assert_eq!(q.charged_total, 12);
    }
}
