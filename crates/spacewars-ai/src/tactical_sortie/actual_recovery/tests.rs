use super::*;
use crate::tactical_sortie::tests::{context, observation};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        LandingPhase, PlanetFlagObservation,
        ground_navigation::GroundRouteDiagnostics,
        landing_objective::{LandingObjectiveSurvey, ObjectivePlanning},
        live_planning::{ActualLanding, ObjectiveWorkEvidence},
    },
};

fn fixture() -> (TacticalSortiePilot, TacticalSortieObservationV1) {
    let mut o = observation();
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick = 200;
    p.controls_armed = true;
    p.queries_ready = true;
    p.landing.phase = LandingPhase::Landed;
    p.transfer = TransferResult::Ready;
    p.hatch = Some(p.ship.position + Vec2::X);
    p.boarding_hatches = [p.hatch, None];
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: PlayerId::PLAYER_2,
        position: p.ship.position + Vec2::X * 30.0,
        normal: Vec2::Y,
        raised_fraction: 1.0,
    });
    let objective = LandingObjective::read(p).unwrap();
    let local = |v: Vec2| (v - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
    o.objective_evidence = Some(ObjectiveWorkEvidence {
        tick: p.tick,
        objective,
        source_objective: Some(objective),
        generation: Some(29),
        request_tick: Some(180),
        measurement_tick: Some(180),
        invalidated_by: None,
        submission_deferred_by: None,
        publication: None,
        exhausted_walk: None,
        unsupported_walk: None,
        covered_handoff: None,
        actual_local_failure: Some(ActualLocalAttemptFailure {
            actor: p.owner,
            reason: "arrival_window",
            pose: ActualLanding {
                vehicle: local(p.ship.position),
                angle: p.ship.angle - p.planet.motion.angle,
                exit: local(p.hatch.unwrap()),
                boarding_hatches: p.boarding_hatches.map(|h| h.map(local)),
            },
        }),
    });
    o.objective_work = Some(ObjectiveWorkState::Pending);
    o.landing_objective = None;
    let mut pilot = TacticalSortiePilot::with_committed_descent(context(), Default::default());
    pilot.enable_actual_route_recovery(true);
    (pilot, o)
}

#[test]
fn current_local_failure_aborts_once_and_reset_preserves_only_configuration() {
    let (mut pilot, o) = fixture();
    let action = pilot.intent(&o);
    assert_eq!(action, CombatIntent::default());
    assert_eq!(pilot.telemetry.failure, Some(ACTUAL_ROUTE_ABORT_REASON));
    assert_eq!(pilot.telemetry.failed_tick, Some(200));
    assert_eq!(pilot.telemetry.goal, TacticalGoal::Blocked);
    let abort = pilot
        .telemetry
        .actual_route_recovery
        .as_ref()
        .unwrap()
        .abort
        .unwrap();
    assert_eq!(
        (abort.generation, abort.request_tick, abort.measurement_tick),
        (29, 180, 180)
    );
    assert_eq!(pilot.intent(&o), action);
    assert_eq!(pilot.clone().telemetry, pilot.telemetry);
    assert!(pilot.rejected_sites.is_empty());
    pilot.reset(context());
    assert_eq!(
        pilot.telemetry.actual_route_recovery,
        Some(ActualRouteRecovery::default())
    );
    assert!(pilot.telemetry.failed_tick.is_none());
    pilot.enable_actual_route_recovery(false);
    pilot.intent(&o);
    assert!(pilot.telemetry.failure.is_none());
    assert_eq!(
        pilot.telemetry.acquisition.unwrap().reason,
        "actual_route_unavailable"
    );
}

#[test]
fn unrelated_stale_or_incomplete_receipts_do_not_abort_or_authorize_exit() {
    let (pilot, source) = fixture();
    for change in 0..18 {
        let mut bot = pilot.clone();
        let mut o = source.clone();
        let e = o.objective_evidence.as_mut().unwrap();
        match change {
            0 => e.tick -= 1,
            1 => e.generation = None,
            2 => e.source_objective = None,
            3 => e.source_objective.as_mut().unwrap().revision += 1,
            4 => e.source_objective.as_mut().unwrap().position.x += 0.01,
            5 => e.objective.owner = PlayerId::PLAYER_1,
            6 => e.invalidated_by = Some("hatch_moved"),
            7 => e.submission_deferred_by = Some("capacity"),
            8 => {
                e.measurement_tick = Some(79);
                e.request_tick = Some(79);
            }
            9 => {
                e.measurement_tick = Some(201);
                e.request_tick = Some(201);
            }
            10 => e.measurement_tick = Some(179),
            11 => e.actual_local_failure.as_mut().unwrap().actor = PlayerId::PLAYER_2,
            12 => e.actual_local_failure.as_mut().unwrap().pose.angle += 0.001,
            13 => e.actual_local_failure.as_mut().unwrap().pose.exit.x += 0.01,
            14 => {
                e.actual_local_failure
                    .as_mut()
                    .unwrap()
                    .pose
                    .boarding_hatches = [None; 2]
            }
            15 => e.actual_local_failure = None,
            16 => o.objective_work = Some(ObjectiveWorkState::Stale),
            _ => o.objective_work = Some(ObjectiveWorkState::Ready),
        }
        let action = bot.intent(&o);
        assert!(bot.telemetry.failure.is_none(), "change {change}");
        assert!(!action.flight.controls.interact_held, "change {change}");
        assert!(bot.telemetry.actual_route_recovery.unwrap().abort.is_none());
    }
}

#[test]
fn physical_guards_and_owned_objectives_keep_their_priority() {
    let (pilot, source) = fixture();
    for change in 0..7 {
        let mut bot = pilot.clone();
        let mut o = source.clone();
        let p = &mut o.combat.recovery.flight.pilot;
        match change {
            0 => p.controls_armed = false,
            1 => p.queries_ready = false,
            2 => p.landing.phase = LandingPhase::Flying,
            3 => p.transfer = TransferResult::Boarded,
            4 => p.location = PilotLocation::OnFoot,
            5 => o.combat.recovery.flight.flight.enabled = false,
            _ => p.planet.claim.as_mut().unwrap().owner = Some(p.owner),
        }
        bot.intent(&o);
        assert!(
            bot.telemetry.actual_route_recovery.unwrap().abort.is_none(),
            "change {change}"
        );
    }
}

#[test]
fn positive_actual_route_wins_even_with_an_inconsistent_failure_receipt() {
    let (mut pilot, mut o) = fixture();
    let p = &o.combat.recovery.flight.pilot;
    let leg = GroundRouteDiagnostics {
        failure: None,
        partial: false,
        start_node: Some(0),
        start_distance: Some(0.0),
        destination_nodes: 1,
        nearest_destination_distance: Some(0.0),
        reachable_nodes: 1,
        closest_reachable_distance: Some(0.0),
        length: 10.0,
        jumps: 0,
        flights: 0,
    };
    o.landing_objective = Some(LandingObjectiveSurvey {
        planning: ObjectivePlanning::Legacy,
        version: 1,
        actor: p.owner,
        tick: p.tick,
        validated_tick: Some(p.tick),
        validated_routes_only: true,
        objective: LandingObjective::read(p).unwrap(),
        sites: vec![],
        actual: Some(LandingObjectiveRoute {
            crossing: None,
            site: None,
            outbound: leg.clone(),
            returning: Some(leg),
            endpoint: None,
        }),
    });
    pilot.intent(&o);
    assert!(pilot.telemetry.failure.is_none());
    assert!(
        pilot
            .telemetry
            .actual_route_recovery
            .unwrap()
            .abort
            .is_none()
    );
    assert_eq!(
        pilot.telemetry.acquisition.unwrap().reason,
        "physically_landed"
    );
}
