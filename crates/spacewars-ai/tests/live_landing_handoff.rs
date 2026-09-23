//! Phase-offset handoff between the bounded objective job and current landing
//! scans. The world advances while controls are withheld to isolate delivery.
use engine_common::{CombatBreakSettings, Scenario};
use engine_core::planning::Work;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        LandingPhase, PlanetFlagObservation, SurfaceSortieScenario, SurfaceSortieState,
        combat::TacticalSortieObservationV1,
        landing_objective::ObjectivePlanning,
        live_planning::{LiveObjectivePlanner, MAX_SURVEY_AGE_TICKS, ObjectiveWorkState},
        pilot::LandingSiteQuery,
    },
};
use spacewars_ai::{BrainReset, tactical_capture::TacticalCapturePilot};
use std::time::Duration;

fn fixture(
    planning: ObjectivePlanning,
) -> (
    SurfaceSortieState,
    TacticalSortieObservationV1,
    TacticalCapturePilot,
) {
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    for _ in 0..180 {
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    }
    let mut source =
        state.tactical_sortie_observation_for_live_profile(0, LandingSiteQuery::Survey, planning);
    source.sun = None;
    source.combat.target = None;
    let p = &mut source.combat.recovery.flight.pilot;
    p.controls_armed = true;
    p.landing.phase = LandingPhase::Flying;
    let site = p.sites[0];
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: PlayerId::PLAYER_2,
        position: site.hatch_position,
        normal: site.normal,
        raised_fraction: 1.0,
    });
    let bot = TacticalCapturePilot::with_planning(
        BrainReset {
            actor: p.owner,
            episode_seed: 42,
        },
        CombatBreakSettings::default(),
        planning,
    );
    (state, source, bot)
}

#[test]
fn a_positive_route_with_no_joint_endpoint_reports_the_native_guard() {
    for planning in [
        ObjectivePlanning::JointRoundTrip,
        ObjectivePlanning::JetpackRoundTrip,
    ] {
        let (state, source, mut bot) = fixture(planning);
        let mut live = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_route_dependencies();
        let mut o = source.clone();
        live.observe_with_planning(&state, 0, &mut o, planning);
        live.advance(state.tick());
        o = source;
        live.observe_with_planning(&state, 0, &mut o, planning);
        let survey = o.landing_objective.as_mut().unwrap();
        let route = survey
            .sites
            .iter_mut()
            .find(|r| r.cost().is_some())
            .unwrap();
        route.endpoint = None;
        let measurement_tick = survey.tick;
        assert_eq!(bot.intent(&o), Default::default());
        let evidence = bot.telemetry().acquisition.unwrap();
        assert_eq!(evidence.reason, "joint_endpoint_missing");
        assert_eq!(evidence.measurement_tick, Some(measurement_tick));
        assert!(bot.telemetry().site.is_none());
        assert_eq!(bot.telemetry().replans, 0);
    }
}

fn run_handoff(planning: ObjectivePlanning, scan_age: u64, fault: &str) {
    let (mut state, source, mut bot) = fixture(planning);
    let source_tick = state.tick();
    let source_p = &source.combat.recovery.flight.pilot;
    let source_flag = source_p.planet.claim.as_ref().unwrap().flag.unwrap();
    let local_flag = (source_flag.position - source_p.planet.motion.position)
        .rotate_radians(-source_p.planet.motion.angle);
    let allowance = Work {
        graph: 16384,
        physics_queries: 1024,
    };
    let mut live = LiveObjectivePlanner::new(1, allowance).with_route_dependencies();
    let mut first_ready = None;
    for age in 0..=scan_age {
        assert_eq!(state.tick(), source_tick + age);
        let query = if age > 0 && age < scan_age {
            LandingSiteQuery::Deferred {
                next_tick: source_tick + scan_age,
            }
        } else {
            LandingSiteQuery::Survey
        };
        let mut o = state.tactical_sortie_observation_for_live_profile(0, query, planning);
        o.sun = None;
        o.combat.target = None;
        let p = &mut o.combat.recovery.flight.pilot;
        p.controls_armed = true;
        p.landing.phase = LandingPhase::Flying;
        let claim = p.planet.claim.as_mut().unwrap();
        claim.owner = Some(PlayerId::PLAYER_2);
        claim.flag = Some(PlanetFlagObservation {
            position: p.planet.motion.position + local_flag.rotate_radians(p.planet.motion.angle),
            ..source_flag
        });
        if age == scan_age {
            match fault {
                "missing_sites" => p.sites.clear(),
                "dirty_queries" => p.queries_ready = false,
                _ => (),
            }
        }
        live.observe_with_planning(&state, 0, &mut o, planning);
        if let Some(s) = &o.landing_objective {
            first_ready.get_or_insert(age);
            assert_eq!(s.tick, source_tick, "holding must not renew evidence age");
            assert_eq!(s.validated_tick, Some(state.tick()));
        }
        bot.intent(&o);
        if age < scan_age {
            assert!(
                bot.site_request().is_none(),
                "deferred scans grant no clearance"
            );
        } else if fault == "none" && scan_age <= MAX_SURVEY_AGE_TICKS {
            assert_eq!(o.objective_work, Some(ObjectiveWorkState::Ready));
            let selected = bot
                .site_request()
                .expect("fresh sites must meet the completed survey");
            assert!(
                o.combat
                    .recovery
                    .flight
                    .pilot
                    .sites
                    .iter()
                    .any(|s| s.id == selected)
            );
            let route = bot.telemetry().objective_route.as_ref().unwrap();
            assert_eq!(route.site, Some(selected));
            assert!(route.cost().is_some());
            assert_eq!(
                live.telemetry().submitted,
                2,
                "replacement work starts without losing delivery"
            );
        } else {
            assert!(bot.site_request().is_none(), "{fault} age={scan_age}");
            if fault == "dirty_queries" || scan_age > MAX_SURVEY_AGE_TICKS {
                assert!(o.landing_objective.is_none());
            }
        }
        let report = live.advance(state.tick()).unwrap();
        assert!(report.charged.graph <= allowance.graph);
        assert!(report.charged.physics_queries <= allowance.physics_queries);
        assert!(live.advance(state.tick()).is_none());
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    }
    assert!(
        first_ready.is_some_and(|age| age > 0 && age < scan_age),
        "{first_ready:?}"
    );
    assert!(live.telemetry().max_retained_requests <= 1);
}

#[test]
fn completed_routes_reach_selection_on_the_next_fresh_scan() {
    for planning in [
        ObjectivePlanning::JointRoundTrip,
        ObjectivePlanning::JetpackRoundTrip,
    ] {
        for age in [45, 60, MAX_SURVEY_AGE_TICKS] {
            run_handoff(planning, age, "none");
        }
    }
}

#[test]
fn waiting_for_a_scan_does_not_restore_missing_sites_dirty_queries_or_expired_evidence() {
    for fault in ["missing_sites", "dirty_queries"] {
        run_handoff(ObjectivePlanning::JointRoundTrip, 60, fault);
    }
    run_handoff(
        ObjectivePlanning::JointRoundTrip,
        MAX_SURVEY_AGE_TICKS + 1,
        "none",
    );
}

#[test]
fn early_candidate_and_current_clearance_reach_selection_before_the_survey_finishes() {
    for planning in [
        ObjectivePlanning::JointRoundTrip,
        ObjectivePlanning::JetpackRoundTrip,
    ] {
        let (mut state, source, mut bot) = fixture(planning);
        let source_tick = state.tick();
        let p = &source.combat.recovery.flight.pilot;
        let flag = p.planet.claim.as_ref().unwrap().flag.unwrap();
        let local_flag =
            (flag.position - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let allowance = Work {
            graph: 512,
            physics_queries: 1024,
        };
        let mut live = LiveObjectivePlanner::new(1, allowance)
            .with_route_dependencies()
            .with_early_candidates();
        let mut selected = false;
        for age in 0..=MAX_SURVEY_AGE_TICKS {
            let query = if age == 0 {
                LandingSiteQuery::Survey
            } else {
                LandingSiteQuery::Deferred {
                    next_tick: source_tick + MAX_SURVEY_AGE_TICKS,
                }
            };
            let mut o = state.tactical_sortie_observation_for_live_profile(0, query, planning);
            o.sun = None;
            o.combat.target = None;
            let p = &mut o.combat.recovery.flight.pilot;
            p.controls_armed = true;
            p.landing.phase = LandingPhase::Flying;
            let claim = p.planet.claim.as_mut().unwrap();
            claim.owner = Some(PlayerId::PLAYER_2);
            claim.flag = Some(PlanetFlagObservation {
                position: p.planet.motion.position
                    + local_flag.rotate_radians(p.planet.motion.angle),
                ..flag
            });
            live.observe_with_planning(&state, 0, &mut o, planning);
            bot.intent(&o);
            if let Some(id) = bot.site_request() {
                let p = &o.combat.recovery.flight.pilot;
                assert_eq!(p.site_query, LandingSiteQuery::Selected(id));
                assert!(p.sites.iter().any(|s| s.id == id));
                let survey = o.landing_objective.as_ref().unwrap();
                assert!(survey.validated_routes_only);
                assert_eq!(survey.tick, source_tick);
                assert_eq!(survey.validated_tick, Some(state.tick()));
                assert!(
                    survey
                        .sites
                        .iter()
                        .any(|r| r.site == Some(id) && r.cost().is_some())
                );
                assert_eq!(live.telemetry().completed, 0);
                assert_eq!(live.telemetry().submitted, 1);
                assert_eq!(live.telemetry().early_candidates.published_requests, 1);
                assert!(age < MAX_SURVEY_AGE_TICKS);
                selected = true;
            }
            let report = live.advance(state.tick()).unwrap();
            assert!(report.charged.graph <= allowance.graph);
            assert!(report.charged.physics_queries <= allowance.physics_queries);
            assert_eq!(
                report.charged.physics_queries,
                report
                    .jobs
                    .iter()
                    .map(|j| j.charged.physics_queries)
                    .sum::<u32>()
            );
            assert!(live.advance(state.tick()).is_none());
            if selected {
                break;
            }
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        assert!(selected, "{planning:?}: {:?}", live.telemetry());
        assert!(live.telemetry().early_candidates.clear_sites > 0);
    }
}
