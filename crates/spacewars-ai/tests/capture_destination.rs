//! Native-host ordering, cadenced real surveys and ordinary physical controls.
use engine_common::Scenario;
use engine_core::planning::Work;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        SurfaceSortieScenario, SurfaceSortieState,
        live_planning::{FlagSurveyPlanner, LiveObjectivePlanner},
    },
};
use spacewars_ai::{
    BrainReset,
    mission_evaluation::MissionEvaluator,
    mission_policy::{MissionBot, MissionPolicy},
};
use std::time::Duration;

const DT: Duration = Duration::from_nanos(16_666_667);
const WORK: Work = Work {
    graph: 4,
    physics_queries: 384,
};

struct Run {
    state: SurfaceSortieState,
    bot: MissionBot,
    evaluator: MissionEvaluator,
    surveys: LiveObjectivePlanner,
    first_claim: Option<u64>,
    boarded: bool,
    farther_choice: bool,
    flags: Option<FlagSurveyPlanner>,
    enemy_value_choice: bool,
}
impl Run {
    fn new(policy: MissionPolicy, seat: usize, mirror: bool) -> Self {
        Self {
            state: SurfaceSortieScenario::init_capture_destination_trial(42, seat, mirror, 0.8),
            bot: MissionBot::new(
                policy,
                BrainReset {
                    actor: PlayerId::from_index(seat).unwrap(),
                    episode_seed: 42,
                },
                Default::default(),
            ),
            evaluator: MissionEvaluator::new(2),
            surveys: LiveObjectivePlanner::new(2, WORK),
            first_claim: None,
            boarded: false,
            farther_choice: false,
            flags: None,
            enemy_value_choice: false,
        }
    }
    fn step(&mut self, seat: usize) -> spacewars_ai::combat_pilot::CombatIntent {
        let actor = PlayerId::from_index(seat).unwrap();
        let mut o = self.state.mission_observation_with_cadence(
            seat,
            self.bot.sensor_request(),
            Default::default(),
        );
        let before = self.bot.telemetry().target;
        let intent = self.bot.intent_with_evaluation(&o, &self.evaluator);
        if self.bot.telemetry().target != before
            && self
                .bot
                .telemetry()
                .destination_planning
                .as_ref()
                .is_some_and(|t| t.last_switch.is_some_and(|s| s.tick == self.state.tick()))
        {
            let r = self.evaluator.latest(actor).unwrap();
            let current = r.candidates.iter().find(|c| c.current).unwrap();
            let other = r
                .candidates
                .iter()
                .find(|c| Some(c.planet) == self.bot.telemetry().target)
                .unwrap();
            self.farther_choice = other.distance > current.distance;
            if self.bot.policy().consumes_flag_surveys() {
                self.enemy_value_choice = other.observed_owner == Some(actor.opponent())
                    && current.observed_owner.is_none()
                    && other.total_seconds > current.total_seconds
                    && r.preferred_by_time == Some(current.planet);
            } else {
                assert!(other.total_seconds < current.total_seconds);
            }
        }
        if let Some(capture) = &self.bot.telemetry().capture {
            self.first_claim = self.first_claim.or(capture.landing.claimed_tick);
            self.boarded |= capture.landing.boarded_tick.is_some();
        }
        let request = self.evaluator.alternative_request(&o, self.bot.telemetry());
        self.surveys
            .observe_destination_cover(&self.state, seat, &mut o, request);
        if let Some(flags) = &mut self.flags {
            let request = self.evaluator.flag_request(&o, self.bot.telemetry());
            flags.observe(&self.state, seat, &o, request);
            self.evaluator.observe_with_flag_surveys(
                &o,
                self.bot.telemetry(),
                request,
                &flags.samples(),
            );
        } else {
            self.evaluator.observe(&o, self.bot.telemetry());
        }
        let allocation = self.surveys.advance_with_state(&self.state).unwrap();
        let used = allocation.charged;
        let graph = self.evaluator.advance(
            self.state.tick(),
            Work {
                graph: WORK.graph - used.graph,
                physics_queries: WORK.physics_queries - used.physics_queries,
            },
        );
        assert!(
            used.graph + graph.graph <= WORK.graph && used.physics_queries <= WORK.physics_queries
        );
        if let Some(flags) = &mut self.flags {
            let busy: Vec<_> = allocation
                .jobs
                .iter()
                .filter(|j| j.charged.physics_queries > 0)
                .map(|j| j.request.actor as usize)
                .collect();
            let used_flags = flags
                .advance(
                    &self.state,
                    Work {
                        graph: WORK.graph - used.graph - graph.graph,
                        physics_queries: WORK.physics_queries - used.physics_queries,
                    },
                    &busy,
                )
                .unwrap()
                .charged;
            assert!(used.graph + graph.graph + used_flags.graph <= WORK.graph);
            assert!(used.physics_queries + used_flags.physics_queries <= WORK.physics_queries);
        }
        SurfaceSortieScenario::step(&mut self.state, &intent.encode(actor), DT);
        intent
    }
}

#[test]
fn native_selector_uses_a_measured_farther_trip_and_physically_claims_boards_and_departs() {
    for (seat, mirror, policy) in [
        (0, false, MissionPolicy::DestinationPlanner),
        (1, true, MissionPolicy::DestinationPlanner),
        (0, false, MissionPolicy::ValuePlanner),
        (1, true, MissionPolicy::ValuePlanner),
    ] {
        let mut baseline = Run::new(MissionPolicy::Planner, seat, mirror);
        let mut experiment = Run::new(policy, seat, mirror);
        for _ in 0..100 * 60 {
            let a = baseline.step(seat);
            let b = experiment.step(seat);
            if experiment
                .bot
                .telemetry()
                .destination_planning
                .as_ref()
                .unwrap()
                .switches
                == 0
            {
                assert_eq!(a, b, "controls agree until an accepted destination switch");
            }
            if baseline.bot.telemetry().completed_sorties > 0
                && experiment.bot.telemetry().completed_sorties > 0
            {
                break;
            }
        }
        assert!(baseline.first_claim.is_some() && experiment.first_claim.is_some());
        assert!(experiment.first_claim.unwrap() < baseline.first_claim.unwrap());
        assert!(experiment.boarded && experiment.bot.telemetry().completed_sorties > 0);
        assert!(experiment.farther_choice);
        assert_eq!(
            experiment
                .bot
                .telemetry()
                .destination_planning
                .as_ref()
                .unwrap()
                .switches,
            1
        );
        assert!(experiment.state.terrain_diagnostics().issues.is_empty());
        assert!(
            experiment
                .bot
                .telemetry()
                .events
                .iter()
                .any(|e| e.kind == "replan"
                    && e.reason
                        == Some(if policy == MissionPolicy::ValuePlanner {
                            "better supported capture value"
                        } else {
                            "shorter supported capture trip"
                        }))
        );
    }
}

#[test]
fn no_completed_comparison_is_exact_v10_fallback_through_flag_capture() {
    for policy in [
        MissionPolicy::DestinationPlanner,
        MissionPolicy::ValuePlanner,
    ] {
        let mut run = Run::new(MissionPolicy::Planner, 0, false);
        let mut experiment = MissionBot::new(
            policy,
            BrainReset {
                actor: PlayerId::PLAYER_1,
                episode_seed: 42,
            },
            Default::default(),
        );
        let mut evaluator = MissionEvaluator::new(1);
        for _ in 0..90 * 60 {
            let a = run.bot.sensor_request();
            let b = experiment.sensor_request();
            assert_eq!(a.site, b.site);
            assert_eq!(a.objective_planning, b.objective_planning);
            assert_eq!(a.last_survey, b.last_survey);
            assert_eq!(a.destination_cover, b.destination_cover);
            let o = run.state.mission_observation_with_cadence(
                0,
                run.bot.sensor_request(),
                Default::default(),
            );
            let expected = run.bot.intent(&o);
            assert_eq!(experiment.intent_with_evaluation(&o, &evaluator), expected);
            evaluator.observe(&o, experiment.telemetry());
            assert_eq!(
                evaluator.advance(run.state.tick(), Work::default()),
                Work::default()
            );
            SurfaceSortieScenario::step(&mut run.state, &expected.encode(PlayerId::PLAYER_1), DT);
        }
        assert!(run.bot.telemetry().completed_sorties > 0);
        assert_eq!(
            experiment.telemetry().completed_sorties,
            run.bot.telemetry().completed_sorties
        );
        assert_eq!(
            experiment
                .telemetry()
                .destination_planning
                .as_ref()
                .unwrap()
                .switches,
            0
        );
    }
}

#[test]
fn owned_foothold_fixture_keeps_real_enemy_and_neutral_choices_in_both_seats() {
    for seat in 0..2 {
        let mut run = Run::new(MissionPolicy::ValuePlanner, seat, seat == 1);
        run.state = SurfaceSortieScenario::init_capture_destination_match_trial(
            42,
            seat,
            seat == 1,
            0.8,
            true,
            Some(Duration::from_secs(600)),
        );
        let initial = run.state.mission_observation(seat, None);
        assert_eq!(initial.planets.len(), 3);
        assert_eq!(
            initial.match_context.as_ref().unwrap().owned_planets,
            [1, 1]
        );
        assert!(
            initial.planets[seat]
                .claim
                .as_ref()
                .unwrap()
                .owner
                .is_none()
        );
        let enemy = initial.planets[1 - seat].claim.as_ref().unwrap();
        assert_eq!(enemy.owner, PlayerId::from_index(1 - seat));
        assert!(enemy.flag.is_some());
        let home = initial.planets[2].claim.as_ref().unwrap();
        assert_eq!(home.owner, PlayerId::from_index(seat));
        assert!(home.flag.is_some());
        for _ in 0..60 {
            run.step(seat);
            assert!(run.state.terrain_diagnostics().issues.is_empty());
        }
    }
}

#[test]
fn surveyed_value_choice_physically_takes_enemy_flag_before_neutral_ground() {
    let mut run = Run::new(MissionPolicy::SurveyValuePlanner, 0, false);
    run.state = SurfaceSortieScenario::init_capture_destination_match_trial(
        42,
        0,
        false,
        0.8,
        true,
        Some(Duration::from_secs(600)),
    );
    run.flags = Some(FlagSurveyPlanner::new(2));
    let mut claimed_enemy = false;
    for _ in 0..180 * 60 {
        run.step(0);
        if run.first_claim.is_some() {
            let o = run.state.mission_observation(0, None);
            claimed_enemy = o.planets[1].claim.as_ref().unwrap().owner == Some(PlayerId::PLAYER_1)
                && o.planets[0].claim.as_ref().unwrap().owner.is_none();
        }
        if run.bot.telemetry().completed_sorties > 0 {
            break;
        }
    }
    assert!(run.enemy_value_choice && claimed_enemy);
    assert!(run.boarded && run.bot.telemetry().completed_sorties > 0);
    assert_eq!(
        run.bot
            .telemetry()
            .destination_planning
            .as_ref()
            .unwrap()
            .switches,
        1
    );
    assert!(run.state.terrain_diagnostics().issues.is_empty());
}

#[test]
fn costed_landing_site_is_freshly_acquired_and_physically_completed() {
    let mut run = Run::new(MissionPolicy::LandingPlanPlanner, 0, false);
    run.state = SurfaceSortieScenario::init_capture_destination_match_trial(
        42,
        0,
        false,
        0.8,
        false,
        Some(Duration::from_secs(600)),
    );
    run.flags = Some(FlagSurveyPlanner::new(2));
    let mut native_touchdown = false;
    for _ in 0..90 * 60 {
        run.step(0);
        let t = run.bot.telemetry();
        if let Some(h) = &t.destination_planning.as_ref().unwrap().landing_handoff {
            if h.landed_tick.is_some() && h.completed_tick.is_none() {
                let capture = t.capture.as_ref().unwrap();
                assert_eq!(capture.site, Some(h.site));
                assert!(capture.landing.landed_tick.is_some());
                native_touchdown = true;
            }
            if h.completed_tick.is_some() {
                break;
            }
        }
    }
    let t = run.bot.telemetry();
    let h = t
        .destination_planning
        .as_ref()
        .unwrap()
        .landing_handoff
        .as_ref()
        .unwrap();
    assert!(h.invalidated_tick.is_none());
    assert!(h.accepted_tick.unwrap() < h.started_tick.unwrap() + 120);
    assert!(h.accepted_tick.unwrap() <= h.landed_tick.unwrap());
    assert!(h.landed_tick.unwrap() < h.completed_tick.unwrap());
    assert!(native_touchdown && run.first_claim.is_some() && run.boarded);
    assert_eq!(t.completed_sorties, 1);
    assert_eq!(
        run.state.mission_observation(0, None).planets[h.site.planet]
            .claim
            .as_ref()
            .unwrap()
            .owner,
        Some(PlayerId::PLAYER_1)
    );
    assert!(run.state.terrain_diagnostics().issues.is_empty());
}

#[test]
fn near_expiry_value_selection_preserves_v10_controls_and_match_outcome() {
    for seat in 0..2 {
        let mut baseline = Run::new(MissionPolicy::Planner, seat, seat == 1);
        let mut candidate = Run::new(MissionPolicy::ValuePlanner, seat, seat == 1);
        for run in [&mut baseline, &mut candidate] {
            run.state = SurfaceSortieScenario::init_capture_destination_match_trial(
                42,
                seat,
                seat == 1,
                0.8,
                true,
                Some(Duration::from_secs(2)),
            );
        }
        for _ in 0..120 {
            assert_eq!(baseline.step(seat), candidate.step(seat));
        }
        assert_eq!(
            candidate
                .bot
                .telemetry()
                .destination_planning
                .as_ref()
                .unwrap()
                .switches,
            0
        );
        let a = baseline
            .state
            .mission_observation(seat, None)
            .match_context
            .unwrap();
        let b = candidate
            .state
            .mission_observation(seat, None)
            .match_context
            .unwrap();
        assert!(a.finished && b.finished);
        assert_eq!(a.owned_planets, b.owned_planets);
        assert_eq!(a.pilots_alive, b.pilots_alive);
        assert!(candidate.state.terrain_diagnostics().issues.is_empty());
    }
}
