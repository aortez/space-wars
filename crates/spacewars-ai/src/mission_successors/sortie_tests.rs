use super::*;

fn fixture() -> (
    MaterialMissionPilot,
    MissionObservationV1,
    SuccessorContinuation,
) {
    let (bot, o) = super::super::super::tests::fixture();
    let site = o.destination_cover.as_ref().unwrap().candidates[0].id;
    let trial = SuccessorContinuation::for_capture_trip(&bot, &o, site).unwrap();
    (bot, o, trial)
}

fn reach_entry(o: &mut MissionObservationV1, trial: &SuccessorContinuation) {
    let entry = trial.report.proposal.approach.unwrap().position(o);
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick += 1;
    p.planet = o.planets[1].clone();
    p.ship.position = entry;
    p.ship.velocity = Vec2::ZERO;
    p.sites.clear();
    p.site_query = LandingSiteQuery::NotRequested;
}

#[test]
fn trip_keeps_the_common_prefix_and_waits_for_the_current_destination_frame() {
    let (mut bot, mut o, mut trial) = fixture();
    let source = o.local.combat.recovery.flight.pilot.tick;
    let before = bot.telemetry.clone();
    assert_eq!(trial.report.deadline_tick, source + 10800);
    assert_eq!(
        trial.report.sortie.as_ref().unwrap().approach_deadline_tick,
        source + 3600
    );
    assert_eq!(trial.intent(&mut bot, &o), bot.previous_intent);
    assert_eq!(bot.telemetry, before);
    assert_eq!(trial.report.applied_controls, 0);
    reach_entry(&mut o, &trial);
    o.local.combat.recovery.flight.pilot.queries_ready = false;
    trial.intent(&mut bot, &o);
    assert!(bot.capture.is_none());
    assert_eq!(
        trial.report.sortie.as_ref().unwrap().arrived_tick,
        Some(source + 1)
    );
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.local.combat.recovery.flight.pilot.queries_ready = true;
    assert_eq!(trial.intent(&mut bot, &o), CombatIntent::default());
    assert!(bot.capture.is_some());
    assert_eq!(
        bot.sensor_request().site,
        Some(trial.report.proposal.approach.unwrap().site)
    );
    let trip = trial.report.sortie.as_ref().unwrap();
    assert_eq!(trip.surface_started_tick, Some(source + 2));
    assert!(trip.selected_site.is_none() && trip.landed_tick.is_none());
    let report = trial.report.clone();
    assert_eq!(trial.intent(&mut bot, &o), CombatIntent::default());
    assert_eq!(trial.report, report);
}

#[test]
fn trip_invalidates_a_changed_approach_and_cannot_credit_a_later_capture() {
    let (mut bot, mut o, mut trial) = fixture();
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.planets[1].revision += 1;
    trial.intent(&mut bot, &o);
    assert_eq!(
        trial.report.stop_reason,
        Some("destination material changed")
    );
    assert_eq!(trial.report.applied_controls, 0);
    let report = trial.report.clone();
    bot.telemetry.completed_sorties += 1;
    bot.telemetry
        .events
        .push(crate::mission_pilot::MissionEvent {
            tick: o.local.combat.recovery.flight.pilot.tick,
            planet: Some(1),
            kind: "departed",
            reason: None,
        });
    trial.record_sortie(&mut bot, &o);
    assert_eq!(trial.report, report);
}

#[test]
fn trip_progress_and_hard_approach_clocks_are_bounded() {
    for progress_limit in [true, false] {
        let (mut bot, mut o, mut trial) = fixture();
        let source = o.local.combat.recovery.flight.pilot.tick;
        let limit = if progress_limit { 1200 } else { 3 };
        if !progress_limit {
            trial.report.sortie.as_mut().unwrap().approach_deadline_tick = source + limit;
        }
        for tick in source + 1..=source + limit {
            o.local.combat.recovery.flight.pilot.tick = tick;
            trial.intent(&mut bot, &o);
        }
        assert_eq!(trial.report.stopped_tick, Some(source + limit));
        assert_eq!(trial.report.applied_controls, limit - 1);
        assert_eq!(
            trial.report.stop_reason,
            Some(if progress_limit {
                "approach made no distance progress for twenty seconds"
            } else {
                "approach deadline"
            })
        );
    }
}

#[test]
fn total_deadline_preserves_the_live_on_foot_return_task() {
    let (mut bot, mut o, mut trial) = fixture();
    let source = o.local.combat.recovery.flight.pilot.tick;
    reach_entry(&mut o, &trial);
    o.local.combat.recovery.flight.pilot.queries_ready = true;
    trial.intent(&mut bot, &o);
    assert!(bot.capture.is_some());
    trial.report.deadline_tick = source + 2;
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick += 1;
    p.location = PilotLocation::OnFoot;
    p.controls_armed = false;
    trial.intent(&mut bot, &o);
    assert_eq!(trial.report.stop_reason, Some("capture trip deadline"));
    assert!(bot.capture.is_some());
    assert!(bot.recovery.is_none());
    assert!(
        trial
            .report
            .sortie
            .as_ref()
            .unwrap()
            .departed_tick
            .is_none()
    );
}

#[test]
fn isolated_trip_physically_reaches_its_site_captures_returns_and_records_departure() {
    use engine_common::Scenario;
    use scenario_spacewars::surface_sortie::{
        SurfaceSortieScenario,
        destination_cover::{
            CoverCandidate, CoverFinding, CoverMeasurement, CoverStatus,
            DestinationCoverObservation,
        },
    };
    use std::time::Duration;
    let dt = Duration::from_nanos(16_666_667);
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    state.enable_match_rules();
    SurfaceSortieScenario::step(&mut state, &[], dt);
    let mut o = state.mission_observation(0, None);
    let p = &o.local.combat.recovery.flight.pilot;
    let site = *p
        .sites
        .iter()
        .filter(|s| {
            o.local
                .cover
                .iter()
                .any(|c| c.site == s.id && c.grounded && c.approach && c.departure)
        })
        .min_by(|a, b| {
            a.vehicle_position
                .distance_to(p.ship.position)
                .total_cmp(&b.vehicle_position.distance_to(p.ship.position))
        })
        .unwrap();
    o.destination_cover = Some(DestinationCoverObservation {
        generation: p.tick,
        candidates: vec![CoverCandidate {
            id: site.id,
            status: CoverStatus::Measured,
            reason: None,
            measurement: Some(CoverMeasurement {
                tick: p.tick,
                revision: p.planet.revision,
                planet: p.planet.motion,
                ship_form: p.ship_form,
                opponent: None,
                queries: 0,
                finding: CoverFinding::Measured,
                site: Some(site),
                cover: None,
            }),
        }],
    });
    let mut bot = MaterialMissionPilot::with_policy(
        BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        },
        engine_common::CombatBreakSettings::default(),
        crate::mission_policy::MissionPolicy::JetpackPlanner,
    );
    bot.enable_pursuit_disengagement(true);
    bot.start_disengagement(&o);
    let prefix = bot.intent(&o);
    // Only the prior handoff history is supplied by this isolated fixture.
    // Sites come from real queries; all subsequent flight, surface actions and
    // milestones run in the unchanged world, with an idle physical opponent.
    bot.end_disengagement(
        o.local.combat.recovery.flight.pilot.tick,
        "separation established",
    );
    let mut trial = SuccessorContinuation::for_capture_trip(&bot, &o, site.id).unwrap();
    SurfaceSortieScenario::step(&mut state, &prefix.encode(PlayerId::PLAYER_1), dt);
    for _ in 0..SuccessorContinuation::MAX_SORTIE_TICKS {
        let o = state.mission_observation_with_cadence(0, bot.sensor_request(), Default::default());
        let intent = trial.intent(&mut bot, &o);
        SurfaceSortieScenario::step(&mut state, &intent.encode(PlayerId::PLAYER_1), dt);
        if trial.report.stopped_tick.is_some() {
            break;
        }
    }
    assert_eq!(
        trial.report.stop_reason,
        Some("capture trip completed"),
        "{:?}",
        trial.report
    );
    let trip = trial.report.sortie.unwrap();
    let arrived = trip.arrived_tick.unwrap();
    let landed = trip.landed_tick.unwrap();
    let exited = trip.exited_tick.unwrap();
    let claimed = trip.claimed_tick.unwrap();
    let boarded = trip.boarded_tick.unwrap();
    let departed = trip.departed_tick.unwrap();
    assert!(
        arrived < landed
            && landed < exited
            && exited <= claimed
            && claimed < boarded
            && boarded < departed
    );
    assert_eq!(trip.selected_site, Some(site.id));
    assert!(trip.landing_position_error.unwrap() <= 10.0);
    assert_eq!(trip.ownership_retained, Some(true));
    assert!(state.terrain_diagnostics().issues.is_empty());
}
