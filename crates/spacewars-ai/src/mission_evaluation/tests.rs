use super::*;
use crate::{
    BrainReset,
    mission_policy::{MissionBot, MissionPolicy},
};
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{SurfaceSortieScenario, SurfaceSortieState};
use std::time::Duration;

pub(super) fn fixture() -> (SurfaceSortieState, MissionObservationV1, MissionBot) {
    let mut state = SurfaceSortieScenario::init_material_match(42);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let bot = MissionBot::new(
        MissionPolicy::Planner,
        BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        },
        Default::default(),
    );
    let o = state.mission_observation(0, None);
    (state, o, bot)
}
pub(super) fn known(planet: &PilotPlanetObservation, tick: u64, seconds: f32) -> LocalEvidence {
    LocalEvidence {
        remote: false,
        key: PlanetKey::read(planet),
        site: LandingSiteId {
            planet: planet.index,
            bearing: 0,
        },
        tick,
        gravity: 0.0,
        choice: None,
        route_source_tick: None,
        route_validated_tick: None,
        costs: Some(PhaseCosts {
            landing: seconds,
            exit: 0.0,
            outbound: 0.0,
            claim: 0.0,
            return_board: 0.0,
            departure: 0.0,
        }),
        reason: None,
    }
}
pub(super) fn finish(report: MissionEvaluation) -> MissionEvaluation {
    let mut job = EvaluationJob {
        report,
        cursor: 0,
        complete: false,
    };
    while job.next_work().is_some() {
        assert!(job.output().is_none());
        job.step();
    }
    job.output().unwrap().clone()
}

#[test]
fn farther_complete_trip_beats_nearest_and_current_elapsed_time_is_not_sunk_cost() {
    let (_, mut o, bot) = fixture();
    o.planets.truncate(2);
    let position = o.local.combat.recovery.flight.pilot.ship.position;
    o.planets[0].motion.position = position + Vec2::X * 200.0;
    o.planets[1].motion.position = position + Vec2::X * 400.0;
    for planet in &mut o.planets {
        planet.radius = 80.0;
    }
    let mut mission = bot.telemetry().clone();
    mission.target = Some(o.planets[0].index);
    let tick = o.local.combat.recovery.flight.pilot.tick;
    let evidence = vec![
        known(&o.planets[0], tick, 60.0),
        known(&o.planets[1], tick, 10.0),
    ];
    let result = finish(snapshot(&o, &mission, &evidence));
    assert_eq!(result.preferred_by_time, Some(o.planets[1].index));
    assert!(result.candidates[1].distance > result.candidates[0].distance);
    assert!(result.candidates[1].total_seconds < result.candidates[0].total_seconds);
    assert_eq!(result.charged_work, 3);
    o.local.combat.recovery.flight.pilot.tick += 50 * 60;
    let mut aged = evidence;
    aged[0].choice = Some((tick, tick));
    mission.events.push(crate::mission_pilot::MissionEvent {
        tick,
        planet: mission.target,
        kind: "selected",
        reason: None,
    });
    let mut capture = crate::tactical_capture::TacticalCapturePilot::new(
        BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        },
        Default::default(),
    )
    .telemetry()
    .clone();
    capture.sortie.site = Some(aged[0].site);
    mission.capture = Some(capture);
    let result = finish(snapshot(&o, &mission, &aged));
    assert_eq!(result.preferred_by_time, Some(o.planets[0].index));
    assert_eq!(result.candidates[0].local.as_ref().unwrap().landing, 10.0);
    assert_eq!(result.candidates[1].local.as_ref().unwrap().landing, 10.0);
    assert_eq!(result.candidates[0].travel_seconds, 0.0);
    mission.events.push(crate::mission_pilot::MissionEvent {
        tick: tick + 3000,
        planet: mission.target,
        kind: "selected",
        reason: None,
    });
    let revisited = finish(snapshot(&o, &mission, &aged));
    assert_eq!(
        revisited.candidates[0].local.as_ref().unwrap().landing,
        60.0
    );
    assert!(revisited.candidates[0].travel_seconds > 0.0);
}

#[test]
fn unknown_alternative_prevents_full_comparison_and_deadline_is_not_infeasibility() {
    let (_, mut o, bot) = fixture();
    o.planets.truncate(2);
    let tick = o.local.combat.recovery.flight.pilot.tick;
    let mut evidence = vec![known(&o.planets[0], tick, 10.0)];
    o.match_context.as_mut().unwrap().remaining_seconds = Some(0.1);
    let result = finish(snapshot(&o, bot.telemetry(), &evidence));
    assert!(result.fastest_supported.is_some());
    assert!(result.preferred_by_time.is_none());
    assert!(result.candidates.iter().any(|c| c.total_seconds.is_none()));
    evidence.push(known(&o.planets[1], tick, 20.0));
    let result = finish(snapshot(&o, bot.telemetry(), &evidence));
    assert!(result.preferred_by_time.is_none());
    assert!(
        result
            .candidates
            .iter()
            .all(|c| c.reference_exceeds_match_time == Some(true))
    );
    assert!(result.candidates.iter().all(|c| c.unknown_reason.is_none()));
    o.match_context.as_mut().unwrap().remaining_seconds = None;
    let result = finish(snapshot(&o, bot.telemetry(), &evidence));
    assert!(result.preferred_by_time.is_some());
    assert!(
        result
            .candidates
            .iter()
            .all(|c| c.reference_exceeds_match_time.is_none())
    );
}

#[test]
fn shared_budget_zero_partial_completion_clone_and_multiple_jobs_are_deterministic() {
    let (_, o, bot) = fixture();
    let report = snapshot(&o, bot.telemetry(), &[]);
    let mut queue = PlanningQueue::new(5);
    let mut tokens = Vec::new();
    for actor in 0..5 {
        tokens.push(
            queue
                .submit(
                    actor,
                    (),
                    Default::default(),
                    EvaluationJob {
                        report: report.clone(),
                        cursor: 0,
                        complete: false,
                    },
                )
                .unwrap(),
        );
    }
    assert_eq!(queue.advance(Work::default()).charged, Work::default());
    let mut copy = queue.clone();
    let mut total = 0;
    for _ in 0..20 {
        let work = Work {
            graph: 1,
            physics_queries: 0,
        };
        let a = queue.advance(work);
        let b = copy.advance(work);
        assert_eq!(a, b);
        assert!(a.charged.graph <= 1);
        total += a.charged.graph;
    }
    assert_eq!(total, 5 * (report.candidates.len() as u32 + 1));
    for token in tokens {
        assert_eq!(
            queue.job(token).unwrap().output(),
            copy.job(token).unwrap().output()
        );
        assert!(queue.job(token).unwrap().output().is_some());
    }
}

#[test]
fn host_cancels_material_flag_gravity_and_expired_results_without_rewriting_frozen_output() {
    let (_, mut o, bot) = fixture();
    let mut host = MissionEvaluator::new(2);
    for tick in 1..=3 {
        o.local.combat.recovery.flight.pilot.tick = tick;
        host.observe(&o, bot.telemetry());
        host.advance(tick, DEFAULT_WORK);
    }
    let frozen = host.latest(PlayerId::PLAYER_1).unwrap().clone();
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.planets[0].revision += 1;
    host.observe(&o, bot.telemetry());
    assert!(host.latest(PlayerId::PLAYER_1).is_none());
    assert!(host.pending(PlayerId::PLAYER_1));
    host.advance(4, Work::default());
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.planets[0].claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_2);
    host.observe(&o, bot.telemetry());
    assert_eq!(host.cancelled_total, 1);
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.local.combat.recovery.flight.pilot.gravity += Vec2::X * 100.0;
    host.observe(&o, bot.telemetry());
    assert_eq!(
        host.cancelled_total, 1,
        "unmeasured ground does not depend on own flight gravity"
    );
    o.local.combat.recovery.flight.pilot.tick += MAX_RESULT_AGE + 1;
    host.observe(&o, bot.telemetry());
    assert_eq!(host.cancelled_total, 2);
    assert_eq!(frozen.source_tick, 1);
    assert!(frozen.completed_tick.is_some());
    host.reset();
    assert!(!host.pending(PlayerId::PLAYER_1));
    assert!(host.latest(PlayerId::PLAYER_1).is_none());
    assert_eq!(host.charged_total, 0);
}

#[test]
fn published_result_expires_without_cancelling_its_starved_refresh() {
    let (_, mut o, bot) = fixture();
    o.local.combat.recovery.flight.pilot.queries_ready = false;
    let mut host = MissionEvaluator::new(2);
    for tick in 1..=3 {
        o.local.combat.recovery.flight.pilot.tick = tick;
        host.observe(&o, bot.telemetry());
        host.advance(tick, DEFAULT_WORK);
    }
    assert_eq!(host.latest(PlayerId::PLAYER_1).unwrap().source_tick, 1);

    let refresh_tick = 1 + REFRESH_TICKS;
    for tick in 4..=MAX_RESULT_AGE + 2 {
        o.local.combat.recovery.flight.pilot.tick = tick;
        host.observe(&o, bot.telemetry());
        // Start the refresh, then let higher-priority work consume the quota.
        host.advance(
            tick,
            Work {
                graph: u32::from(tick == refresh_tick),
                physics_queries: 0,
            },
        );
        if tick <= MAX_RESULT_AGE + 1 {
            assert_eq!(host.latest(PlayerId::PLAYER_1).unwrap().source_tick, 1);
        } else {
            assert!(host.latest(PlayerId::PLAYER_1).is_none());
        }
    }
    assert!(host.pending(PlayerId::PLAYER_1));
    assert_eq!(host.cancelled_total, 0);

    for tick in MAX_RESULT_AGE + 3..=MAX_RESULT_AGE + 6 {
        o.local.combat.recovery.flight.pilot.tick = tick;
        host.observe(&o, bot.telemetry());
        host.advance(tick, DEFAULT_WORK);
        if let Some(report) = host.latest(PlayerId::PLAYER_1) {
            assert_eq!(report.source_tick, refresh_tick);
            assert_eq!(host.completed_total, 2);
            assert_eq!(host.cancelled_total, 0);
            return;
        }
    }
    panic!("the still-valid refresh should finish when its budget returns");
}

#[test]
fn pending_reports_use_pinned_dependencies_and_cancel_changed_route_support() {
    let (_, mut o, bot) = fixture();
    o.local.combat.recovery.flight.pilot.queries_ready = false;
    o.local.objective_gravity = 0.0;
    let mut host = MissionEvaluator::new(2);
    let local = o
        .planets
        .iter()
        .find(|planet| planet.index == o.local.combat.recovery.flight.pilot.planet.index)
        .unwrap();
    let local_index = local.index;
    let mut sample = known(local, 1, 20.0);
    sample.route_source_tick = Some(0);
    sample.route_validated_tick = Some(1);
    host.actors.insert(
        0,
        ActorState {
            evidence: vec![sample],
            ..Default::default()
        },
    );
    host.observe(&o, bot.telemetry());
    host.advance(1, Work::default());
    for tick in 2..=4 {
        o.local.combat.recovery.flight.pilot.tick = tick;
        o.local.objective_gravity += 0.004;
        host.observe(&o, bot.telemetry());
    }
    assert_eq!(
        host.cancelled_total, 1,
        "small changes accumulate against submitted gravity"
    );
    let mut rejected = known(local, 1, 20.0);
    rejected.gravity = o.local.objective_gravity;
    rejected.costs = None;
    rejected.reason = Some("newly rejected route");
    rejected.route_source_tick = Some(0);
    rejected.route_validated_tick = Some(1);
    host.actors.get_mut(&0).unwrap().evidence = vec![rejected];
    o.local.combat.recovery.flight.pilot.tick += 1;
    host.observe(&o, bot.telemetry());
    assert_eq!(host.cancelled_total, 2);
    host.advance(5, DEFAULT_WORK);
    o.local.combat.recovery.flight.pilot.tick += 1;
    host.observe(&o, bot.telemetry());
    host.advance(6, DEFAULT_WORK);
    let result = host.latest(PlayerId::PLAYER_1).unwrap();
    let candidate = result
        .candidates
        .iter()
        .find(|c| c.planet == local_index)
        .unwrap();
    assert!(candidate.total_seconds.is_none());
    assert_eq!(candidate.route_source_tick, Some(0));
    assert_eq!(candidate.route_validated_tick, Some(1));
}

#[test]
fn actual_local_measurements_are_required_and_memory_is_bounded() {
    let (_, mut o, bot) = fixture();
    let tick = o.local.combat.recovery.flight.pilot.tick;
    o.local.combat.recovery.flight.pilot.queries_ready = false;
    assert!(model::observe_local(&o, bot.telemetry()).is_none());
    let mut host = MissionEvaluator::new(1);
    host.observe(&o, bot.telemetry());
    assert!(host.actors[&0].evidence.is_empty());
    let mut copy = o.clone();
    copy.local.combat.recovery.flight.pilot.owner = PlayerId::PLAYER_2;
    host.observe(&copy, bot.telemetry());
    assert_eq!(host.actors.len(), 1);
    for i in 3..20 {
        let mut planet = o.planets[0].clone();
        planet.index = i;
        o.planets.push(planet);
    }
    let result = finish(snapshot(&o, bot.telemetry(), &[]));
    assert_eq!(result.candidates.len(), MAX_OPTIONS);
    assert!(result.candidates_truncated);
    assert_eq!(result.source_tick, tick);
}

#[test]
fn match_clock_context_distinguishes_labs_unlimited_and_finished_matches() {
    let lab = SurfaceSortieScenario::init_material_travel(42, false);
    assert!(lab.mission_observation(0, None).match_context.is_none());
    let mut state = SurfaceSortieScenario::init_material_match(42);
    state.set_match_time_limit(None);
    assert!(
        state
            .mission_observation(0, None)
            .match_context
            .unwrap()
            .remaining_seconds
            .is_none()
    );
    let mut state = SurfaceSortieScenario::init_material_match(42);
    state.set_match_time_limit(Some(Duration::from_millis(1)));
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let o = state.mission_observation(0, None);
    let context = o.match_context.as_ref().unwrap();
    assert_eq!(context.remaining_seconds, Some(0.0));
    assert!(context.finished);
    let (_, _, bot) = fixture();
    let result = finish(snapshot(&o, bot.telemetry(), &[]));
    assert!(result.inactive_reason.is_some());
    assert!(result.preferred_by_time.is_none());
}

#[test]
fn timing_model_rejects_unmeasured_stale_foreign_partial_powered_and_long_routes() {
    use scenario_spacewars::surface_sortie::{
        PlanetFlagObservation,
        ground_navigation::GroundRouteDiagnostics,
        landing_objective::{
            LandingObjective, LandingObjectiveRoute, LandingObjectiveSurvey, ObjectivePlanning,
        },
    };
    let (_, mut o, _) = fixture();
    let p = &mut o.local.combat.recovery.flight.pilot;
    let site = LandingSiteId {
        planet: p.planet.index,
        bearing: 0,
    };
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: PlayerId::PLAYER_2,
        position: p.planet.motion.position + Vec2::Y * p.planet.radius,
        normal: Vec2::Y,
        raised_fraction: 1.0,
    });
    let route = GroundRouteDiagnostics {
        failure: None,
        partial: false,
        start_node: Some(0),
        start_distance: Some(0.0),
        destination_nodes: 1,
        nearest_destination_distance: Some(0.0),
        reachable_nodes: 2,
        closest_reachable_distance: Some(0.0),
        length: 20.0,
        jumps: 0,
        flights: 0,
    };
    o.local.landing_objective = Some(LandingObjectiveSurvey {
        planning: ObjectivePlanning::JointRoundTrip,
        version: 1,
        actor: p.owner,
        tick: p.tick,
        validated_tick: None,
        validated_routes_only: false,
        objective: LandingObjective::read(p).unwrap(),
        sites: vec![LandingObjectiveRoute {
            crossing: None,
            site: Some(site),
            outbound: route.clone(),
            returning: Some(route),
            endpoint: None,
        }],
        actual: None,
    });
    let valid = o.clone();
    assert!(model::local_costs(&o, site).is_ok());
    for mutation in 0..7 {
        o = valid.clone();
        let survey = o.local.landing_objective.as_mut().unwrap();
        match mutation {
            0 => survey.actor = PlayerId::PLAYER_2,
            1 => survey.tick += 1,
            2 => survey.objective.revision += 1,
            3 => survey.sites[0].outbound.partial = true,
            4 => survey.sites[0].outbound.jumps = 1,
            5 => survey.sites[0].returning.as_mut().unwrap().length = 500.0,
            _ => survey.sites[0].returning = None,
        }
        assert!(model::local_costs(&o, site).is_err(), "mutation {mutation}");
    }
    o = valid.clone();
    o.local.landing_objective = None;
    assert!(model::local_costs(&o, site).is_err());
    let mut rotated = valid.local.combat.recovery.flight.pilot.planet.clone();
    let old = PlanetKey::read(&rotated);
    let local_flag =
        rotated.claim.as_ref().unwrap().flag.unwrap().position - rotated.motion.position;
    rotated.motion.angle += 0.2;
    rotated
        .claim
        .as_mut()
        .unwrap()
        .flag
        .as_mut()
        .unwrap()
        .position = rotated.motion.position + local_flag.rotate_radians(0.2);
    assert!(old.matches(&PlanetKey::read(&rotated)));
    rotated
        .claim
        .as_mut()
        .unwrap()
        .flag
        .as_mut()
        .unwrap()
        .position += Vec2::X * 5.0;
    assert!(!old.matches(&PlanetKey::read(&rotated)));
}

#[test]
fn native_route_cadence_gap_does_not_refresh_age_or_hide_stale_work() {
    let (_, mut o, _) = fixture();
    let mut sample = known(&o.local.combat.recovery.flight.pilot.planet, 30, 20.0);
    sample.route_source_tick = Some(30);
    o.local.landing_objective = None;
    o.local.objective_work = None;
    for tick in [31, 45, 59] {
        o.local.combat.recovery.flight.pilot.tick = tick;
        assert!(model::route_cadence_gap(&o, &sample));
        assert_eq!(sample.tick, 30);
    }
    o.local.combat.recovery.flight.pilot.tick = 60;
    assert!(!model::route_cadence_gap(&o, &sample));
    o.local.combat.recovery.flight.pilot.tick = 31;
    o.local.objective_work =
        Some(scenario_spacewars::surface_sortie::live_planning::ObjectiveWorkState::Stale);
    assert!(!model::route_cadence_gap(&o, &sample));
    o.local.objective_work = None;
    sample.reason = Some("round trip incomplete");
    assert!(!model::route_cadence_gap(&o, &sample));
}

#[test]
fn physical_controls_sensor_requests_and_bot_memory_match_with_evaluation_enabled() {
    let dt = Duration::from_nanos(16_666_667);
    for policy in MissionPolicy::ALL {
        let (mut state, _, _) = fixture();
        let mut bot = MissionBot::new(
            policy,
            BrainReset {
                actor: PlayerId::PLAYER_1,
                episode_seed: 42,
            },
            Default::default(),
        );
        let mut reference = bot.clone();
        let mut evaluator = MissionEvaluator::new(2);
        for _ in 0..1200 {
            assert_eq!(bot.site_request(), reference.site_request());
            let o =
                state.mission_observation_with_cadence(0, bot.sensor_request(), Default::default());
            let actual = bot.intent(&o);
            let expected = reference.intent(&o);
            evaluator.observe(&o, bot.telemetry());
            evaluator.advance(state.tick(), DEFAULT_WORK);
            assert_eq!(actual, expected);
            assert_eq!(bot.telemetry(), reference.telemetry());
            SurfaceSortieScenario::step(&mut state, &actual.encode(PlayerId::PLAYER_1), dt);
        }
        assert!(evaluator.completed_total > 0);
    }
}
