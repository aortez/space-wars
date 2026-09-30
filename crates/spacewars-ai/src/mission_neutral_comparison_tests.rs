use super::*;
use crate::mission_evaluation::MissionEvaluator;
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{SurfaceSortieScenario, pilot::LandingSiteQuery};
use std::time::Duration;

fn fixture() -> (
    MaterialMissionPilot,
    MaterialMissionPilot,
    MissionObservationV1,
    TransferEnvironment,
) {
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    let mut bot = MaterialMissionPilot::with_policy(
        BrainReset {
            actor: PlayerId::from_index(0).unwrap(),
            episode_seed: 42,
        },
        Default::default(),
        crate::mission_policy::MissionPolicy::ValuePlanner,
    );
    for _ in 0..120 {
        let mut o = state.mission_observation(0, bot.site_request());
        // Isolate the unexposed domain; sites/environment remain measured.
        o.local.combat.target = None;
        o.opponent = None;
        let before = bot.clone();
        let intent = bot.intent(&o);
        if let Some(site) = bot.telemetry.capture.as_ref().and_then(|c| c.site)
            && let Ok((_, record)) = bot.landing_choice_with_neutral_timing(&o, site)
            && record.total_seconds.is_some()
        {
            return (before, bot, o, state.transfer_environment().unwrap());
        }
        SurfaceSortieScenario::step(
            &mut state,
            &intent.encode(PlayerId::from_index(0).unwrap()),
            Duration::from_nanos(16_666_667),
        );
    }
    panic!("no native neutral choice");
}

fn work(graph: u32) -> Work {
    Work {
        graph,
        physics_queries: 0,
    }
}

#[test]
fn physical_orbit_refreshes_selected_geometry_and_solar_without_retiming() {
    for ready in [false, true] {
        let seed = 14157161121037389508;
        let mut state = SurfaceSortieScenario::init_material_arena(seed);
        let mut bot = MaterialMissionPilot::with_policy(
            BrainReset {
                actor: PlayerId::PLAYER_1,
                episode_seed: seed,
            },
            Default::default(),
            crate::mission_policy::MissionPolicy::ValuePlanner,
        );
        let mut checked = false;
        for _ in 0..1000 {
            let mut o = state.mission_observation(0, bot.site_request());
            o.local.combat.target = None;
            o.opponent = None;
            let before = bot.clone();
            let intent = bot.intent(&o);
            let record = bot
                .telemetry
                .capture
                .as_ref()
                .and_then(|c| c.site)
                .and_then(|s| bot.landing_choice_with_neutral_timing(&o, s).ok())
                .map(|(_, r)| r)
                .filter(|r| r.total_seconds.is_some());
            if let Some(record) = record {
                assert!(record.selected.solar.is_some());
                let e = state.transfer_environment().unwrap();
                let mut q = TransferComparisonQueue::new(1);
                let token = q
                    .submit_comparison_with_neutral_timing(
                        &before,
                        &bot,
                        &o,
                        &MissionEvaluator::new(1),
                        e.clone(),
                        Some(false),
                    )
                    .unwrap();
                q.advance(e.tick, work(if ready { 10801 } else { 0 }));
                let initial = q.snapshot(token, e.tick).unwrap();
                SurfaceSortieScenario::step(
                    &mut state,
                    &intent.encode(PlayerId::PLAYER_1),
                    Duration::from_nanos(16_666_667),
                );
                let mut now = state.mission_observation(0, bot.site_request());
                now.local.combat.target = None;
                now.opponent = None;
                bot.intent(&now);
                let p = &now.local.combat.recovery.flight.pilot;
                assert_eq!(
                    p.site_query,
                    LandingSiteQuery::Selected(record.selected.site)
                );
                assert_ne!(p.planet.motion, record.planet.motion);
                assert_ne!(
                    p.sites
                        .iter()
                        .find(|s| s.id == record.selected.site)
                        .unwrap()
                        .position,
                    record.site.unwrap().position
                );
                q.observe(
                    token,
                    &bot,
                    &now,
                    &state.transfer_environment().unwrap(),
                    Some(false),
                );
                let status = q.state(PlayerId::PLAYER_1).unwrap();
                assert_ne!(status.phase, TransferQueuePhase::Stale, "{status:?}");
                assert_eq!(
                    status
                        .neutral_validation
                        .as_ref()
                        .unwrap()
                        .solar
                        .unwrap()
                        .forecast_tick,
                    p.tick
                );
                assert_eq!(q.snapshot(token, p.tick).unwrap(), initial);
                checked = true;
                break;
            }
            SurfaceSortieScenario::step(
                &mut state,
                &intent.encode(PlayerId::PLAYER_1),
                Duration::from_nanos(16_666_667),
            );
        }
        assert!(checked, "no orbiting native choice");
    }
}

#[test]
fn source_total_is_separate_and_old_reports_and_work_stay_identical() {
    let (before, bot, o, e) = fixture();
    let mut ordinary =
        TransferComparisonJob::new(&before, &bot, &o, &MissionEvaluator::new(1), e.clone())
            .unwrap();
    let mut joined = ordinary.clone().with_neutral_timing(&bot, &o);
    assert_eq!(joined.snapshot().neutral_timing_costs, Some(vec![]));
    let context = joined.neutral_context().unwrap();
    let mut queue = TransferComparisonQueue::new(1);
    let token = queue
        .submit_comparison_with_neutral_timing(
            &before,
            &bot,
            &o,
            &MissionEvaluator::new(1),
            e.clone(),
            Some(false),
        )
        .unwrap();
    while ordinary.next_work().is_some() {
        ordinary.step();
        joined.step();
    }
    let mut output = joined.output().unwrap().clone();
    let totals = output.neutral_timing_costs.take().unwrap();
    assert_eq!(totals[0].source_tick_total_seconds, context.total_seconds);
    assert_eq!(totals[0].travel_seconds, Some(0.0));
    assert!(
        totals
            .iter()
            .skip(1)
            .all(|c| c.source_tick_total_seconds.is_none() && c.unknown.is_some())
    );
    for candidate in &mut output.candidates {
        candidate.neutral_timing = None;
    }
    assert_eq!(Some(&output), ordinary.output());
    queue.advance(e.tick, work(10801));
    assert!(matches!(queue.poll(token, e.tick), JobPoll::Ready(_)));
    assert_eq!(queue.charged_total, output.charged_graph);
    assert_eq!(
        queue
            .state(bot.context.actor)
            .unwrap()
            .neutral_validation
            .as_ref()
            .unwrap()
            .tick,
        e.tick
    );
}

#[test]
fn changed_dependencies_cancel_pending_and_ready_without_work_or_revival() {
    let (before, original, observation, environment) = fixture();
    for ready in [false, true] {
        for change in 0..18 {
            let (mut bot, mut o, mut e) =
                (original.clone(), observation.clone(), environment.clone());
            let mut q = TransferComparisonQueue::new(1);
            let token = q
                .submit_comparison_with_neutral_timing(
                    &before,
                    &bot,
                    &o,
                    &MissionEvaluator::new(1),
                    e.clone(),
                    Some(false),
                )
                .unwrap();
            q.advance(e.tick, work(if ready { 10801 } else { 0 }));
            assert_eq!(matches!(q.poll(token, e.tick), JobPoll::Ready(_)), ready);
            super::tests::next(&mut bot, &mut o, &mut e);
            let p = &mut o.local.combat.recovery.flight.pilot;
            let site = bot.telemetry.capture.as_ref().unwrap().site.unwrap();
            p.site_query = LandingSiteQuery::Selected(site);
            match change {
                0 => p.queries_ready = false,
                1 => {
                    p.site_query = LandingSiteQuery::Deferred {
                        next_tick: p.tick + 1,
                    }
                }
                2 => p.site_query = LandingSiteQuery::NotRequested,
                3 => p.sites.clear(),
                4 => {
                    p.sites
                        .iter_mut()
                        .find(|s| s.id == site)
                        .unwrap()
                        .boarding_hatches = [None; 2]
                }
                5 => {
                    p.sites
                        .iter_mut()
                        .find(|s| s.id == site)
                        .unwrap()
                        .local_position
                        .x += 1.0
                }
                6 => {
                    bot.telemetry
                        .capture
                        .as_mut()
                        .unwrap()
                        .sortie
                        .landing
                        .landing_retries += 1
                }
                7 => bot.telemetry.capture.as_mut().unwrap().sortie.started_tick = Some(p.tick),
                8 => bot.telemetry.completed_sorties += 1,
                9 => {
                    p.planet.claim.as_mut().unwrap().claimant = Some(p.owner);
                    o.planets[p.planet.index] = p.planet.clone();
                }
                10 => p.sites.iter_mut().find(|s| s.id == site).unwrap().normal.x = f32::NAN,
                11 => {
                    let state = SurfaceSortieScenario::init_material_combat(42);
                    let mut t = state
                        .mission_observation(0, None)
                        .local
                        .combat
                        .target
                        .unwrap();
                    t.motion = p.ship;
                    t.ground_occluded = false;
                    o.local.combat.target = Some(t);
                }
                12 => {
                    bot.telemetry.capture.as_mut().unwrap().sortie.solar =
                        Some(crate::landing_safety::SolarLandingPlan {
                            forecast_tick: p.tick,
                            arrival_seconds: 1.0,
                            surface_seconds: 25.0,
                            side: 1.0,
                            approach_clearance: 1.0,
                            parked_clearance: 1.0,
                            departure_clearance: 1.0,
                            departure_side: 1.0,
                        })
                }
                13 => {
                    let state = SurfaceSortieScenario::init_material_combat(42);
                    let mut t = state
                        .mission_observation(0, None)
                        .local
                        .combat
                        .target
                        .unwrap();
                    t.motion.position.x = f32::MAX;
                    o.local.combat.target = Some(t);
                }
                14 => o.local.planet_orbit_omega = Some(0.001),
                15 => {
                    bot.telemetry
                        .events
                        .iter_mut()
                        .rfind(|e| e.kind == "selected")
                        .unwrap()
                        .tick += 1
                }
                16 => p
                    .sites
                    .push(*p.sites.iter().find(|s| s.id == site).unwrap()),
                17 => {
                    p.sites
                        .iter_mut()
                        .find(|s| s.id == site)
                        .unwrap()
                        .boarding_hatches[0] = Some(Vec2::new(f32::NAN, 0.0))
                }
                _ => unreachable!(),
            }
            let charged = q.charged_total;
            q.observe(token, &bot, &o, &e, Some(false));
            assert_eq!(q.poll(token, e.tick), JobPoll::Stale, "change {change}");
            assert!(
                q.state(bot.context.actor)
                    .unwrap()
                    .reason
                    .unwrap()
                    .starts_with("neutral"),
                "change {change}: {:?}",
                q.state(bot.context.actor)
            );
            assert_eq!(q.advance(e.tick, work(0)).unwrap().charged.graph, 0);
            assert_eq!(q.charged_total, charged);
            assert!(
                q.state(bot.context.actor)
                    .unwrap()
                    .neutral_validation
                    .is_none()
            );
            q.observe(token, &original, &observation, &environment, Some(false));
            assert_eq!(q.poll(token, e.tick), JobPoll::Stale);
        }
    }
}

#[test]
fn solar_validation_has_its_own_clock_and_uses_the_current_approach_phase() {
    use scenario_spacewars::surface_sortie::SolarHazard;
    let (_, bot, mut o, _) = fixture();
    let site_id = bot.telemetry.capture.as_ref().unwrap().site.unwrap();
    let (_, mut record) = bot.landing_choice_with_neutral_timing(&o, site_id).unwrap();
    let site = record.site.unwrap();
    let side = record.selected.side.unwrap();
    o.local.sun = Some(SolarHazard {
        position: Vec2::new(10000.0, 10000.0),
        radius: 10.0,
        heat_radius: 20.0,
    });
    record.selected.solar = crate::landing_safety::assess(&o.local, site, side, true);
    let mut m = bot.telemetry.clone();
    m.capture.as_mut().unwrap().sortie.solar = record.selected.solar;
    o.local.combat.recovery.flight.pilot.tick += 1;
    let tick = o.local.combat.recovery.flight.pilot.tick;
    for circling in [false, true] {
        let v = record
            .validate_current(&o, &m, Some((side, circling)))
            .unwrap();
        assert_eq!(v.tick, tick);
        assert_eq!(v.solar.unwrap().forecast_tick, tick);
        assert_eq!(
            v.solar,
            crate::landing_safety::assess(&o.local, site, side, circling)
        );
        assert_eq!(
            record.selected.solar.unwrap().forecast_tick,
            record.source_tick
        );
    }
    assert!(
        record
            .validate_current(&o, &m, Some((-side, true)))
            .is_err()
    );
    assert!(record.validate_current(&o, &m, None).is_err());
    o.local.sun.as_mut().unwrap().heat_radius = 100000.0;
    assert_eq!(
        record
            .validate_current(&o, &m, Some((side, false)))
            .unwrap_err(),
        "neutral current solar assessment unavailable or unsafe"
    );
}

#[test]
fn ordinary_and_unsupported_comparisons_do_not_add_neutral_guards() {
    let (state, before, bot, o) = transfer_forecast::tests::source_with_before();
    let e = state.transfer_environment().unwrap();
    let evaluator = MissionEvaluator::new(1);
    let mut q = TransferComparisonQueue::new(1);
    let token = q
        .submit_comparison_with_neutral_timing(
            &before,
            &bot,
            &o,
            &evaluator,
            e.clone(),
            Some(false),
        )
        .unwrap();
    assert!(
        q.actors
            .get(&token.actor)
            .unwrap()
            .source
            .as_ref()
            .unwrap()
            .neutral_reference
            .is_none()
    );
    let joined = q.snapshot(token, e.tick).unwrap();
    assert!(
        joined
            .candidates
            .iter()
            .all(|c| c.neutral_timing.as_ref().unwrap().record.is_none())
    );
    let mut ordinary = TransferComparisonQueue::new(1);
    let token = ordinary
        .submit_comparison(&before, &bot, &o, &evaluator, e.clone(), Some(false))
        .unwrap();
    let json = serde_json::to_string(&ordinary.snapshot(token, e.tick).unwrap()).unwrap();
    assert!(!json.contains("neutral_timing"));
    assert!(
        !serde_json::to_string(ordinary.state(bot.context.actor).unwrap())
            .unwrap()
            .contains("neutral_validation")
    );
}

#[test]
fn selected_site_motion_and_gravity_do_not_retime_the_source() {
    let (before, mut bot, mut o, mut e) = fixture();
    let mut q = TransferComparisonQueue::new(1);
    let token = q
        .submit_comparison_with_neutral_timing(
            &before,
            &bot,
            &o,
            &MissionEvaluator::new(1),
            e.clone(),
            Some(false),
        )
        .unwrap();
    q.advance(e.tick, work(10801));
    let source = q.snapshot(token, e.tick).unwrap();
    super::tests::next(&mut bot, &mut o, &mut e);
    let p = &mut o.local.combat.recovery.flight.pilot;
    let id = bot.telemetry.capture.as_ref().unwrap().site.unwrap();
    p.site_query = LandingSiteQuery::Selected(id);
    let s = p.sites.iter_mut().find(|s| s.id == id).unwrap();
    s.position.x += 0.1;
    s.vehicle_position.x += 0.1;
    s.hatch_position.x += 0.1;
    s.boarding_hatches[0] = None;
    p.gravity.x += 0.01;
    q.observe(token, &bot, &o, &e, Some(false));
    assert_eq!(q.poll(token, e.tick), JobPoll::Ready(&source));
    assert_eq!(
        q.state(bot.context.actor)
            .unwrap()
            .neutral_validation
            .as_ref()
            .unwrap()
            .tick,
        e.tick
    );
    let later = TransferComparisonJob::new(&before, &bot, &o, &MissionEvaluator::new(1), e)
        .unwrap()
        .with_neutral_timing(&bot, &o);
    assert!(
        later.neutral_context().is_none(),
        "an old choice must not be backfilled"
    );
}
