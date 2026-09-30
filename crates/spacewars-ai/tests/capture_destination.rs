//! Native-host ordering, cadenced real surveys and ordinary physical controls.
use engine_common::Scenario;
use engine_core::planning::Work;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        SurfaceSortieScenario, SurfaceSortieState, live_planning::LiveObjectivePlanner,
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
            assert!(other.total_seconds < current.total_seconds);
        }
        if let Some(capture) = &self.bot.telemetry().capture {
            self.first_claim = self.first_claim.or(capture.landing.claimed_tick);
            self.boarded |= capture.landing.boarded_tick.is_some();
        }
        let request = self.evaluator.alternative_request(&o, self.bot.telemetry());
        self.surveys
            .observe_destination_cover(&self.state, seat, &mut o, request);
        self.evaluator.observe(&o, self.bot.telemetry());
        let used = self
            .surveys
            .advance_with_state(&self.state)
            .unwrap()
            .charged;
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
        SurfaceSortieScenario::step(&mut self.state, &intent.encode(actor), DT);
        intent
    }
}

#[test]
fn native_selector_uses_a_measured_farther_trip_and_physically_claims_boards_and_departs() {
    for (seat, mirror) in [(0, false), (1, true)] {
        let mut baseline = Run::new(MissionPolicy::Planner, seat, mirror);
        let mut experiment = Run::new(MissionPolicy::DestinationPlanner, seat, mirror);
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
                .any(|e| e.kind == "replan" && e.reason == Some("shorter supported capture trip"))
        );
    }
}

#[test]
fn no_completed_comparison_is_exact_v10_fallback_through_flag_capture() {
    let mut run = Run::new(MissionPolicy::Planner, 0, false);
    let mut experiment = MissionBot::new(
        MissionPolicy::DestinationPlanner,
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
