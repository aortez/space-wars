use engine_common::Scenario;
use engine_core::Vec2;
use scenario_spacewars::spaceling_geometry::HALF_HEIGHT;
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        LandingPhase, PilotLocation, SurfaceSortieAction, SurfaceSortieScenario,
        SurfaceSortieState,
        impact::{RecoveryDisruption, SurfaceImpactAction},
        recovery_sensors::RecoveryTaskObservationV1,
    },
};
use spacewars_ai::{
    BrainReset,
    flight_pilot::FlightIntent,
    recovery_pilot::RulePilotV3,
    recovery_task::{RecoverShipTask, RecoveryGoal, TaskStatus},
};
use std::time::Duration;
const DT: Duration = Duration::from_nanos(16_666_667);

#[test]
fn tipped_pod_uses_shared_lift_and_releases_before_rearming() {
    use scenario_spacewars::surface_sortie::pod_righting::PodRightingObservation;
    let (mut task, mut o) = airborne_pod();
    o.flight.pilot.ship.angle = 1.7;
    o.flight.pilot.landing.altitude = 0.2;
    o.pod_righting = Some(PodRightingObservation {
        eligible: true,
        ..Default::default()
    });
    let intent = task.step(&o);
    assert!(intent.controls.brake_held && intent.controls.primary_held);
    assert!(!intent.controls.interact_held);
    let telemetry = task.telemetry().clone();
    assert_eq!(task.step(&o), intent);
    assert_eq!(task.telemetry(), &telemetry);
    let mut clone = task.clone();
    o.flight.pilot.tick += 1;
    o.pod_righting.as_mut().unwrap().needs_release = true;
    assert_eq!(task.step(&o), clone.step(&o));
    assert!(!task.step(&o).controls.primary_held);
    o.flight.pilot.tick += 1;
    o.pod_righting.as_mut().unwrap().remaining_seconds = 1.0;
    assert!(task.step(&o).controls.primary_held);
    assert_eq!(task.telemetry().started_tick, telemetry.started_tick);
}

// Observation fixtures isolate the task's progress/deadline contract. Physical
// collision and full recovery are exercised separately below and in the soak.
fn airborne_pod() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    let mut s = SurfaceSortieScenario::init_material(42, 1);
    SurfaceSortieScenario::step(&mut s, &[], DT);
    let mut o = s.recovery_task_observation(0, None);
    let p = &mut o.flight.pilot;
    p.controls_armed = true;
    p.ship_form = ShipForm::EscapePod;
    p.ship_available = true;
    p.location = PilotLocation::Aboard(p.vehicle);
    p.planet.motion.spin = 0.0;
    p.planet.motion.velocity = Vec2::ZERO;
    p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 100.0);
    p.ship.velocity = Vec2::ZERO;
    p.ship.spin = 0.0;
    p.ship.angle = 0.0;
    p.gravity = Vec2::ZERO;
    p.landing.phase = LandingPhase::Flying;
    p.landing.altitude = 26.0;
    p.landing.assist_strength = 0.0;
    (
        RecoverShipTask::new(BrainReset {
            actor: p.owner,
            episode_seed: 42,
        }),
        o,
    )
}

fn rejected_pod_site() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    let (mut task, mut o) = airborne_pod();
    o.sites.truncate(1);
    assert_eq!(o.sites.len(), 1);
    for tick in 0..=13 {
        o.flight.pilot.tick = tick;
        task.step(&o);
    }
    assert!(task.site_request().is_some());
    // A stationary approach exhausts the existing ten-second progress budget.
    o.flight.pilot.tick = 614;
    task.step(&o);
    assert_eq!(task.telemetry().landing_retries, 1);
    assert_eq!(task.telemetry().landing_rejections.len(), 1);
    (task, o)
}

#[test]
fn deferred_pod_survey_keeps_stabilizing_and_does_not_report_missing_ground() {
    use scenario_spacewars::surface_sortie::pilot::LandingSiteQuery;
    let (mut task, mut o) = airborne_pod();
    let sites = o.sites.clone();
    o.sites.clear();
    for tick in 0..30 {
        o.flight.pilot.tick = tick;
        o.flight.pilot.site_query = LandingSiteQuery::Deferred { next_tick: 30 };
        let action = task.step(&o);
        assert!(action.controls.brake_held);
        assert!(!action.controls.interact_held);
        assert_eq!(task.telemetry().status, TaskStatus::Running);
        assert_eq!(task.telemetry().invalidations, 0);
        assert_eq!(task.telemetry().landing_retries, 0);
        assert_eq!(task.telemetry().site_search_since, None);
    }
    assert!(
        task.telemetry()
            .stabilization
            .as_ref()
            .unwrap()
            .settled_tick
            .is_some()
    );
    o.flight.pilot.tick = 30;
    o.flight.pilot.site_query = LandingSiteQuery::Survey;
    o.sites = sites;
    task.step(&o);
    assert!(task.site_request().is_some());
    assert_eq!(task.telemetry().goal, RecoveryGoal::LandPod);
}

#[test]
fn landing_retry_reuses_pod_righting_and_accepts_actual_hatch_access() {
    use scenario_spacewars::surface_sortie::{
        TransferResult, pod_righting::PodRightingObservation,
    };
    let (mut task, mut o) = rejected_pod_site();
    let started = task.telemetry().started_tick;
    o.flight.pilot.tick += 1;
    o.flight.pilot.ship.angle = 1.7;
    o.flight.pilot.landing.altitude = 1.0;
    o.pod_righting = Some(PodRightingObservation {
        eligible: true,
        ..Default::default()
    });
    let intent = task.step(&o);
    assert!(intent.controls.primary_held && intent.controls.brake_held);
    assert_eq!(task.telemetry().goal, RecoveryGoal::StabilizePod);
    assert_eq!(task.telemetry().stabilization.as_ref().unwrap().attempts, 2);
    let telemetry = task.telemetry().clone();
    assert_eq!(task.step(&o), intent);
    assert_eq!(task.telemetry(), &telemetry);
    let mut clone = task.clone();
    o.flight.pilot.tick += 1;
    o.flight.pilot.landing.phase = LandingPhase::Landed;
    o.flight.pilot.transfer = TransferResult::Ready;
    o.sites.clear();
    let intent = task.step(&o);
    assert_eq!(intent, clone.step(&o));
    assert!(intent.controls.interact_held && !intent.controls.primary_held);
    assert_eq!(task.telemetry().goal, RecoveryGoal::ExitPod);
    assert_eq!(task.telemetry().started_tick, started);
}

#[test]
fn failed_footing_is_reconsidered_after_cooldown_or_local_geometry_change() {
    for changed in [false, true] {
        let (mut task, mut o) = rejected_pod_site();
        for tick in 615..=628 {
            o.flight.pilot.tick = tick;
            task.step(&o);
        }
        assert_eq!(task.telemetry().goal, RecoveryGoal::SurveyPod);
        assert!(task.site_request().is_none());
        assert_eq!(task.telemetry().site_search_since, Some(628));
        let retry_after = task.telemetry().landing_rejections[0].retry_after_tick;
        o.flight.pilot.tick = 629;
        o.flight.pilot.queries_ready = false;
        task.step(&o);
        assert_eq!(task.telemetry().status, TaskStatus::Running);
        o.flight.pilot.queries_ready = true;
        if changed {
            o.sites[0].local_position.x += 0.6;
            o.sites[0].revision += 1;
            o.flight.pilot.tick += 1;
        } else {
            o.flight.pilot.tick = retry_after;
        }
        task.step(&o);
        assert_eq!(task.site_request(), Some(o.sites[0].id));
        assert_eq!(task.telemetry().goal, RecoveryGoal::LandPod);
        assert_eq!(task.telemetry().landing_retries, 1);
        assert_eq!(task.telemetry().site_search_since, None);
        assert_eq!(task.telemetry().started_tick, Some(0));
    }
}

#[test]
fn empty_pod_survey_waits_for_landing_but_retains_a_finite_failure_budget() {
    use scenario_spacewars::surface_sortie::TransferResult;
    let (mut task, mut o) = airborne_pod();
    o.sites.clear();
    for tick in 0..=13 {
        o.flight.pilot.tick = tick;
        task.step(&o);
    }
    assert_eq!(task.telemetry().goal, RecoveryGoal::SurveyPod);
    let mut landed = task.clone();
    o.flight.pilot.tick = 13 + 6 * 60;
    o.flight.pilot.landing.phase = LandingPhase::Landed;
    o.flight.pilot.transfer = TransferResult::Ready;
    assert!(landed.step(&o).controls.interact_held);
    assert_eq!(landed.telemetry().goal, RecoveryGoal::ExitPod);
    o.flight.pilot.landing.phase = LandingPhase::Flying;
    o.flight.pilot.tick = 13 + 15 * 60 + 1;
    task.step(&o);
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(
        task.telemetry().reason,
        Some("no suitable pod landing site")
    );
    let mut expired = landed.clone();
    o.flight.pilot.tick = 120 * 60 + 1;
    expired.step(&o);
    assert_eq!(
        expired.telemetry().reason,
        Some("recovery exceeded two-minute task budget")
    );
}

#[test]
fn exhausted_pod_retries_allow_a_ready_exit_but_cannot_restart_forever() {
    use scenario_spacewars::surface_sortie::TransferResult;
    let (mut task, mut o) = airborne_pod();
    o.flight.pilot.landing.phase = LandingPhase::Landed;
    o.flight.pilot.transfer = TransferResult::ExitBlocked;
    for retry in 0..4 {
        o.flight.pilot.tick = retry * 122;
        task.step(&o);
        o.flight.pilot.tick += 121;
        task.step(&o);
    }
    assert_eq!(task.telemetry().landing_retries, 4);
    let mut ready = task.clone();
    o.flight.pilot.tick += 1;
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("pod landing retries exhausted")
    );
    o.flight.pilot.transfer = TransferResult::Ready;
    assert!(ready.step(&o).controls.interact_held);
    for tick in 600..800 {
        o.flight.pilot.tick = tick;
        task.step(&o);
    }
    assert_eq!(task.telemetry().landing_retries, 4);
}

#[test]
fn rebuild_flight_keeps_its_destination_until_ownership_changes() {
    use scenario_spacewars::surface_sortie::jetpack::{
        CrossingAnchor, CrossingDirection, CrossingPlan, JetpackNavigationObservation,
    };
    use spacewars_ai::ground_task::GroundDestination;
    let (mut task, mut o) = rebuild_relocation_fixture();
    o.ground.as_mut().unwrap().edges.clear();
    let site = o.rebuild.as_ref().unwrap().site.unwrap();
    o.jetpack = Some(JetpackNavigationObservation {
        terrain_flight: None,
        vehicle_continuation: None,
        vehicle_forecast: None,
        reference_velocity: Vec2::ZERO,
        charge: 1.0,
        burning: false,
        burn_seconds: 0.0,
        gravity: -Vec2::Y * 18.0,
        surveyed: true,
        crossing: None,
        terrain_crossings: vec![CrossingPlan {
            planet: site.planet,
            revision: site.revision,
            direction: CrossingDirection::Left,
            start: Vec2::new(0.0, 60.0),
            destination: site.position,
            cruise_radius: 64.0,
            anchor: CrossingAnchor::GroundGap { from: 0, to: 2 },
        }],
    });
    task.step(&o);
    o.rebuild = None;
    for tick in 1..=4 {
        o.flight.pilot.tick = tick;
        o.ground.as_mut().unwrap().tick = tick;
        let mut replay = task.clone();
        assert_eq!(task.step(&o), replay.step(&o));
        let ground = task.telemetry().ground.as_ref().unwrap();
        assert!(ground.crossing.is_some());
        assert_eq!(
            ground.destination,
            GroundDestination::Rebuild {
                planet: site.planet,
                position: site.position
            }
        );
    }
    o.flight.pilot.tick = 5;
    o.flight.pilot.planet.claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_2);
    task.step(&o);
    assert_eq!(
        task.telemetry().ground.as_ref().unwrap().destination,
        GroundDestination::Flag
    );
}

fn rebuild_relocation_fixture() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    use scenario_spacewars::surface_sortie::{
        SurfaceRecoveryStatus,
        ground_navigation::{GroundEdge, GroundEdgeKind, GroundMap, GroundNode},
        pilot::PilotMotion,
        rebuild_placement::{RebuildRelocationSurvey, RebuildStandingSite},
    };
    let (task, mut o) = airborne_pod();
    let p = &mut o.flight.pilot;
    p.tick = 0;
    p.location = PilotLocation::OnFoot;
    p.ship_available = false;
    p.balanced = true;
    p.supported_planet = Some(p.planet.index);
    p.actor_up = Vec2::Y;
    p.planet.motion.position = Vec2::ZERO;
    p.planet.motion.angle = 0.0;
    p.actor = Some(PilotMotion {
        position: Vec2::new(0.0, 60.0 + HALF_HEIGHT),
        velocity: Vec2::ZERO,
        angle: 0.0,
        spin: 0.0,
    });
    p.planet.claim.as_mut().unwrap().owner = Some(p.owner);
    p.recovery.as_mut().unwrap().status = SurfaceRecoveryStatus::HatchBlocked;
    o.ground = Some(GroundMap {
        version: 1,
        actor: p.owner,
        planet: p.planet.index,
        revision: p.planet.revision,
        tick: 0,
        nodes: (0..3)
            .map(|id| GroundNode {
                id,
                position: Vec2::new(f32::from(id) * 2.0, 60.0),
                normal: Vec2::Y,
            })
            .collect(),
        edges: (0..2)
            .map(|from| GroundEdge {
                from,
                to: from + 1,
                kind: GroundEdgeKind::Walk,
                length: 2.0,
            })
            .collect(),
        rejected: Vec::new(),
    });
    o.rebuild = Some(RebuildRelocationSurvey {
        tick: 0,
        checked: 2,
        attempts: Vec::new(),
        site: Some(RebuildStandingSite {
            precise: false,
            planet: p.planet.index,
            revision: p.planet.revision,
            position: Vec2::new(4.0, 60.0),
            walk_length: 4.0,
            flight_length: 0.0,
            jetpack_flights: 0,
            hatch_walk_length: 0.0,
        }),
        refinement: None,
        search: None,
        staging: None,
        staging_map: None,
    });
    (task, o)
}

#[test]
fn rebuild_relocation_uses_measured_walk_controls_and_invalidates_with_terrain() {
    let (mut task, mut o) = rebuild_relocation_fixture();
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().relocations, 1);
    let before = task.telemetry().clone();
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry(), &before);
    o.rebuild = None;
    for tick in 1..=2 {
        o.flight.pilot.tick = tick;
        o.ground.as_mut().unwrap().tick = tick;
        let action = task.step(&o);
        if tick == 2 {
            assert!(action.controls.horizontal > 0.0);
            assert!(!action.controls.interact_held);
        }
    }
    o.flight.pilot.tick = 3;
    o.flight.pilot.planet.revision += 1;
    o.ground = None;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert!(task.telemetry().relocation_site.is_none());
    assert_eq!(task.telemetry().invalidations, 1);
    o.flight.pilot.tick = 304;
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("no reachable standing site with hatch access")
    );
    o.flight.pilot.tick = 305;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
}

#[test]
#[cfg(feature = "sensor-profile")]
fn coarse_rebuild_precision_survives_reset_and_requires_actual_foot_arrival() {
    for enabled in [false, true] {
        let (mut task, mut o) = rebuild_relocation_fixture();
        task.set_rebuild_precise_arrival(enabled);
        task.reset(BrainReset {
            actor: o.flight.pilot.owner,
            episode_seed: 7,
        });
        o.jetpack = None;
        task.step(&o);
        assert!(!task.telemetry().relocation_site.unwrap().precise);
        o.rebuild = None;
        o.flight.pilot.tick = 1;
        o.ground.as_mut().unwrap().tick = 1;
        o.flight.pilot.actor.as_mut().unwrap().position.x = 3.6;
        let action = task.step(&o);
        assert_eq!(
            task.telemetry().ground.as_ref().unwrap().precise_rebuild,
            enabled
        );
        assert_eq!(task.telemetry().relocation_site.is_some(), enabled);
        if enabled {
            assert!(action.controls.horizontal > 0.0);
            let before = task.telemetry().clone();
            assert_eq!(task.step(&o), action);
            assert_eq!(task.telemetry(), &before);
            let mut copy = task.clone();
            o.flight.pilot.tick = 2;
            o.ground.as_mut().unwrap().tick = 2;
            o.flight.pilot.actor.as_mut().unwrap().position.x = 4.05;
            assert_eq!(task.step(&o), copy.step(&o));
            assert_eq!(task.telemetry(), copy.telemetry());
        }
        assert!(task.telemetry().relocation_site.is_none());
        assert!(task.telemetry().rebuild_footing.is_none());
        assert_eq!(task.telemetry().started_tick, Some(0));
        assert_eq!(task.telemetry().relocations, 1);
        assert_eq!(o.flight.pilot.recovery.as_ref().unwrap().rebuilds, 0);
    }
}

#[test]
fn refined_rebuild_reaches_the_measured_footing_without_resetting_recovery() {
    let (mut task, mut o) = rebuild_relocation_fixture();
    o.jetpack = None;
    o.rebuild.as_mut().unwrap().site.as_mut().unwrap().precise = true;
    task.step(&o);
    o.rebuild = None;
    o.flight.pilot.tick = 1;
    o.ground.as_mut().unwrap().tick = 1;
    o.flight.pilot.actor.as_mut().unwrap().position.x = 3.6;
    let action = task.step(&o);
    assert!(action.controls.horizontal > 0.0);
    assert!(task.telemetry().relocation_site.is_some());
    assert!(task.telemetry().ground.as_ref().unwrap().precise_rebuild);
    assert_eq!(
        task.telemetry().ground.as_ref().unwrap().path.last(),
        Some(&2)
    );
    let before = task.telemetry().clone();
    assert_eq!(task.step(&o), action);
    assert_eq!(task.telemetry(), &before);
    let mut copy = task.clone();
    o.flight.pilot.tick = 2;
    o.ground.as_mut().unwrap().tick = 2;
    o.flight.pilot.actor.as_mut().unwrap().position.x = 4.05;
    assert_eq!(task.step(&o), copy.step(&o));
    assert_eq!(task.telemetry(), copy.telemetry());
    assert!(task.telemetry().relocation_site.is_none());
    assert_eq!(task.telemetry().started_tick, Some(0));
    assert_eq!(task.telemetry().relocations, 1);
    assert_eq!(task.telemetry().status, TaskStatus::Running);
    assert_eq!(o.flight.pilot.recovery.as_ref().unwrap().rebuilds, 0);
}

#[cfg(feature = "sensor-profile")]
fn staged_rebuild_fixture() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    use scenario_spacewars::surface_sortie::rebuild_placement::{
        RebuildSearchProgress, RebuildStagingProposal,
    };
    let (mut task, mut o) = rebuild_relocation_fixture();
    task.set_rebuild_search(true);
    o.jetpack = None;
    let survey = o.rebuild.as_mut().unwrap();
    survey.site = None;
    survey.search = Some(RebuildSearchProgress {
        planet: o.flight.pilot.planet.index,
        revision: o.flight.pilot.planet.revision,
        origin: Vec2::new(0.0, 60.0),
        started_tick: 0,
        visited: vec![2],
        preferred: None,
        recheck_preferred: false,
        include_staging_map: false,
    });
    task.step(&o); // The original missing-site allowance begins here.
    o.flight.pilot.tick = 90;
    let survey = o.rebuild.as_mut().unwrap();
    survey.tick = 90;
    survey.staging = Some(RebuildStagingProposal {
        planet: o.flight.pilot.planet.index,
        revision: o.flight.pilot.planet.revision,
        position: Vec2::new(2.0, 60.0),
        walk_length: 2.0,
        target_bearing: 2,
        target_position: Vec2::new(4.0, 60.0),
        remaining_length: 2.0,
        hatch_walk_length: 1.0,
    });
    (task, o)
}

#[test]
#[cfg(feature = "sensor-profile")]
fn staging_requires_actual_supported_arrival_and_keeps_the_original_search_deadline() {
    let (mut task, mut o) = staged_rebuild_fixture();
    task.step(&o);
    assert_eq!(task.telemetry().relocations, 1);
    assert!(task.telemetry().relocation_site.is_none());
    let before = task.telemetry().clone();
    task.step(&o);
    assert_eq!(task.telemetry(), &before);
    let mut cloned = task.clone();
    o.rebuild = None;
    for tick in 91..=93 {
        o.flight.pilot.tick = tick;
        o.ground.as_mut().unwrap().tick = tick;
        o.flight.pilot.actor.as_mut().unwrap().position.x = if tick == 91 { 1.6 } else { 2.02 };
        o.flight.pilot.supported_planet = (tick != 92).then_some(o.flight.pilot.planet.index);
        let action = task.step(&o);
        assert_eq!(action, cloned.step(&o));
        assert_eq!(task.telemetry(), cloned.telemetry());
        if tick == 91 {
            assert!(action.controls.horizontal > 0.0);
        }
        let stage = task.telemetry().rebuild_staging.as_ref().unwrap();
        assert_eq!(stage.search_since, 0);
        assert_eq!(stage.arrived_tick, (tick == 93).then_some(93));
    }
    assert_eq!(task.rebuild_search_request().unwrap().preferred, Some(2));
    assert_eq!(task.telemetry().started_tick, Some(0));
    assert_eq!(task.telemetry().relocations, 1);
    assert_eq!(o.flight.pilot.recovery.as_ref().unwrap().rebuilds, 0);
    // Moving resets the native eight-second build timer. Waiting for that
    // timer cannot renew or postpone the unresolved placement search.
    o.flight.pilot.tick = 94;
    o.flight.pilot.recovery.as_mut().unwrap().status =
        scenario_spacewars::surface_sortie::SurfaceRecoveryStatus::Rebuilding;
    let mut rechecked = o.clone();
    rechecked.rebuild = rebuild_relocation_fixture().1.rebuild;
    let survey = rechecked.rebuild.as_mut().unwrap();
    survey.tick = 94;
    survey.site.as_mut().unwrap().precise = true;
    cloned.step(&rechecked);
    assert_eq!(cloned.telemetry().relocations, 2);
    assert!(cloned.telemetry().relocation_site.unwrap().precise);
    assert_eq!(cloned.telemetry().started_tick, Some(0));
    task.step(&o);
    assert_eq!(task.telemetry().goal, RecoveryGoal::FindBuildSpace);
    o.flight.pilot.tick = 301;
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("no reachable standing site with hatch access")
    );
    task.reset(BrainReset {
        actor: o.flight.pilot.owner,
        episode_seed: 7,
    });
    assert!(task.rebuild_search_request().unwrap().visited.is_empty());
    assert!(task.telemetry().rebuild_staging.is_none());
    assert!(task.telemetry().started_tick.is_none());
}

#[test]
#[cfg(feature = "sensor-profile")]
fn staging_invalidates_with_terrain_and_cannot_extend_its_missing_site_allowance() {
    let (mut task, mut o) = staged_rebuild_fixture();
    task.step(&o);
    o.rebuild = None;
    o.ground = None;
    o.flight.pilot.tick = 91;
    o.flight.pilot.planet.revision += 1;
    task.step(&o);
    assert_eq!(
        task.telemetry()
            .rebuild_staging
            .as_ref()
            .unwrap()
            .invalidated_tick,
        Some(91)
    );
    assert_eq!(task.telemetry().relocations, 1);
    o.flight.pilot.tick = 301;
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("no reachable standing site with hatch access")
    );

    let (mut task, mut o) = staged_rebuild_fixture();
    task.step(&o);
    o.rebuild = None;
    o.flight.pilot.tick = 301;
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("rebuild staging exceeded the original search deadline")
    );
}

#[test]
#[cfg(feature = "sensor-profile")]
fn native_build_takes_precedence_over_an_expired_staging_proposal() {
    let (mut task, mut o) = staged_rebuild_fixture();
    task.step(&o);
    o.rebuild = None;
    let p = &mut o.flight.pilot;
    p.tick = 301;
    p.ship_available = true;
    p.ship_form = ShipForm::Ship;
    p.transfer = scenario_spacewars::surface_sortie::TransferResult::Ready;
    p.recovery.as_mut().unwrap().rebuilds = 1;
    p.recovery.as_mut().unwrap().status =
        scenario_spacewars::surface_sortie::SurfaceRecoveryStatus::ShipAvailable;
    let mut disarmed = o.clone();
    disarmed.flight.pilot.controls_armed = false;
    let mut copy = task.clone();
    assert_eq!(copy.step(&disarmed), FlightIntent::default());
    assert_eq!(
        copy.telemetry()
            .rebuild_staging
            .as_ref()
            .unwrap()
            .invalidated_tick,
        Some(301)
    );
    task.step(&o);
    assert_eq!(task.telemetry().goal, RecoveryGoal::Board);
    assert_eq!(task.telemetry().status, TaskStatus::Running);
    assert_eq!(task.telemetry().rebuilt_tick, Some(301));
    assert_eq!(
        task.telemetry()
            .rebuild_staging
            .as_ref()
            .unwrap()
            .invalidated_tick,
        Some(301)
    );
}

#[cfg(feature = "sensor-profile")]
fn staging_walk_fixture(handoff: bool) -> (RecoverShipTask, RecoveryTaskObservationV1) {
    use scenario_spacewars::surface_sortie::ground_navigation::{
        GroundEdge, GroundEdgeKind, GroundNode,
    };
    let (mut task, mut o) = staged_rebuild_fixture();
    task.set_staging_execution(true, handoff);
    let map = o.ground.as_mut().unwrap();
    map.tick = o.flight.pilot.tick;
    map.nodes = (0..=4)
        .map(|id| GroundNode {
            id,
            position: Vec2::new(f32::from(id), 60.0),
            normal: Vec2::Y,
        })
        .collect();
    map.edges = (0..4)
        .map(|from| GroundEdge {
            from,
            to: from + 1,
            kind: GroundEdgeKind::Walk,
            length: 1.0,
        })
        .collect();
    let survey = o.rebuild.as_mut().unwrap();
    survey.staging.as_mut().unwrap().target_bearing = 4;
    survey.staging_map = Some(map.clone());
    (task, o)
}

#[test]
#[cfg(feature = "sensor-profile")]
fn continuous_staging_changes_only_route_interiors_and_preserves_supported_arrival() {
    let (mut task, mut o) = staging_walk_fixture(false);
    let mut control = task.clone();
    control.set_staging_execution(false, false);
    task.step(&o);
    control.step(&o);
    o.rebuild = None;
    for (tick, x) in [
        (91, 0.0),
        (92, 0.0),
        (93, 1.2),
        (94, 1.5),
        (95, 2.02),
        (96, 2.02),
    ] {
        o.flight.pilot.tick = tick;
        o.ground.as_mut().unwrap().tick = tick;
        o.flight.pilot.actor.as_mut().unwrap().position.x = x;
        o.flight.pilot.supported_planet = (tick != 95).then_some(o.flight.pilot.planet.index);
        let action = task.step(&o);
        let old = control.step(&o);
        if tick == 92 {
            assert_eq!(action.controls.horizontal, 1.0);
            assert!(old.controls.horizontal > 0.0 && old.controls.horizontal < 1.0);
        } else {
            assert_eq!(action, old);
        }
        if tick == 94 {
            assert!(action.controls.horizontal > 0.0 && action.controls.horizontal < 1.0);
        }
        assert_eq!(
            task.telemetry()
                .rebuild_staging
                .as_ref()
                .unwrap()
                .arrived_tick,
            (tick == 96).then_some(96)
        );
    }
    o.flight.pilot.tick = 97;
    o.rebuild = rebuild_relocation_fixture().1.rebuild;
    let survey = o.rebuild.as_mut().unwrap();
    survey.tick = 97;
    survey.site.as_mut().unwrap().precise = true;
    task.step(&o);
    assert_eq!(task.telemetry().relocations, 2);
    o.rebuild = None;
    o.flight.pilot.tick = 98;
    o.ground.as_mut().unwrap().tick = 98;
    task.step(&o);
    assert!(!task.telemetry().ground.as_ref().unwrap().continuous_walk);
    assert!(task.telemetry().ground.as_ref().unwrap().precise_rebuild);
    assert_eq!(task.telemetry().started_tick, Some(0));
}

#[test]
#[cfg(feature = "sensor-profile")]
fn staging_handoff_walks_before_the_next_survey_and_keeps_task_state_and_deadlines() {
    let (mut task, mut o) = staging_walk_fixture(true);
    o.jetpack = Some(
        scenario_spacewars::surface_sortie::jetpack::JetpackNavigationObservation {
            terrain_flight: None,
            vehicle_continuation: None,
            vehicle_forecast: None,
            reference_velocity: Vec2::ZERO,
            charge: 1.0,
            burning: false,
            burn_seconds: 0.0,
            gravity: -Vec2::Y * 18.0,
            surveyed: false,
            crossing: None,
            terrain_crossings: Vec::new(),
        },
    );
    let mut control = task.clone();
    control.set_staging_execution(true, false);
    task.step(&o);
    control.step(&o);
    let ground = task.telemetry().ground.as_ref().unwrap();
    assert_eq!(ground.staging_seed_tick, Some(90));
    assert_eq!(ground.started_tick, None);
    assert_eq!(ground.path, vec![0, 1, 2]);
    assert_eq!(ground.route.as_ref().unwrap().jumps, 0);
    let mut copy = task.clone();
    o.rebuild = None;
    o.ground = None;
    for tick in 91..=92 {
        o.flight.pilot.tick = tick;
        let action = task.step(&o);
        assert_eq!(action, copy.step(&o));
        assert_eq!(task.telemetry(), copy.telemetry());
        let before = task.telemetry().clone();
        assert_eq!(task.step(&o), action);
        assert_eq!(task.telemetry(), &before);
        assert_eq!(control.step(&o), FlightIntent::default());
        if tick == 92 {
            assert_eq!(action.controls.horizontal, 1.0);
        }
    }
    assert_eq!(task.telemetry().started_tick, Some(0));
    assert_eq!(
        task.telemetry().ground.as_ref().unwrap().started_tick,
        Some(91)
    );
    o.flight.pilot.tick = 301;
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("rebuild staging exceeded the original search deadline")
    );
    task.reset(BrainReset {
        actor: o.flight.pilot.owner,
        episode_seed: 7,
    });
    let request = task.rebuild_search_request().unwrap();
    assert!(request.include_staging_map && request.visited.is_empty());
    assert!(task.telemetry().ground.is_none());
    let (_, fresh) = staging_walk_fixture(true);
    task.step(&fresh);
    assert_eq!(
        task.telemetry().ground.as_ref().unwrap().staging_seed_tick,
        Some(90)
    );
    assert!(task.telemetry().ground.as_ref().unwrap().continuous_walk);
}

#[test]
#[cfg(feature = "sensor-profile")]
fn staging_handoff_rejects_stale_identity_geometry_and_nonwalking_routes() {
    use scenario_spacewars::surface_sortie::ground_navigation::GroundEdgeKind;
    for fault in 0..12 {
        let (mut task, mut o) = staging_walk_fixture(true);
        let map = o.rebuild.as_mut().unwrap().staging_map.as_mut().unwrap();
        match fault {
            0 => map.tick -= 1,
            1 => map.tick += 1,
            2 => map.actor = PlayerId::from_index(1).unwrap(),
            3 => map.planet += 1,
            4 => map.revision += 1,
            5 => map.version += 1,
            6 => map.nodes[1].position.x = f32::NAN,
            7 => map.nodes[1].id = map.nodes[0].id,
            8 => map.edges[0].kind = GroundEdgeKind::Jump,
            9 => map.edges[0].kind = GroundEdgeKind::Jetpack,
            10 => map.edges[0].length = f32::INFINITY,
            11 => map.edges.clear(),
            _ => unreachable!(),
        }
        task.step(&o);
        assert_eq!(task.telemetry().relocations, 1, "fault {fault}");
        assert!(
            task.telemetry()
                .ground
                .as_ref()
                .is_none_or(|g| g.staging_seed_tick.is_none()),
            "fault {fault}"
        );
        o.rebuild = None;
        o.ground = None;
        o.flight.pilot.tick = 91;
        assert_eq!(task.step(&o), FlightIntent::default());
        assert_eq!(task.telemetry().status, TaskStatus::Running);
    }
}

#[test]
#[cfg(feature = "sensor-profile")]
fn seeded_staging_honors_dirty_queries_and_invalidates_with_terrain() {
    let (mut task, mut o) = staging_walk_fixture(true);
    task.step(&o);
    o.rebuild = None;
    o.ground = None;
    o.flight.pilot.tick = 91;
    o.flight.pilot.queries_ready = false;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(
        task.telemetry()
            .rebuild_staging
            .as_ref()
            .unwrap()
            .arrived_tick,
        None
    );
    o.flight.pilot.tick = 92;
    o.flight.pilot.queries_ready = true;
    o.flight.pilot.planet.revision += 1;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(
        task.telemetry()
            .rebuild_staging
            .as_ref()
            .unwrap()
            .invalidated_tick,
        Some(92)
    );
    o.flight.pilot.tick = 301;
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("no reachable standing site with hatch access")
    );
}

#[cfg(feature = "sensor-profile")]
fn held_rebuild_fixture() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    let (mut task, mut o) = rebuild_relocation_fixture();
    task.set_rebuild_search(true);
    task.set_rebuild_footing_hold(true);
    o.rebuild.as_mut().unwrap().site.as_mut().unwrap().precise = true;
    task.step(&o);
    o.rebuild = None;
    o.flight.pilot.tick = 1;
    o.ground.as_mut().unwrap().tick = 1;
    task.step(&o);
    o.flight.pilot.tick = 2;
    o.ground.as_mut().unwrap().tick = 2;
    o.flight.pilot.actor.as_mut().unwrap().position.x = 4.05;
    o.flight.pilot.recovery.as_mut().unwrap().status =
        scenario_spacewars::surface_sortie::SurfaceRecoveryStatus::Rebuilding;
    task.step(&o);
    assert_eq!(
        task.telemetry()
            .rebuild_footing
            .as_ref()
            .unwrap()
            .started_tick,
        2
    );
    assert_eq!(
        task.telemetry().ground.as_ref().unwrap().started_tick,
        Some(1)
    );
    (task, o)
}

#[test]
#[cfg(feature = "sensor-profile")]
fn a_fresh_preview_already_at_the_foot_can_be_held_before_a_ground_route_is_needed() {
    use scenario_spacewars::surface_sortie::rebuild_placement::{
        RebuildPlacementReport, RebuildRelocationAttempt,
    };
    let (mut task, mut o) = rebuild_relocation_fixture();
    task.set_rebuild_search(true);
    task.set_rebuild_footing_hold(true);
    let survey = o.rebuild.as_mut().unwrap();
    let site = survey.site.as_mut().unwrap();
    site.precise = true;
    survey.attempts.push(RebuildRelocationAttempt {
        bearing: 2,
        route: None,
        placement: Some(RebuildPlacementReport {
            anchor: None,
            anchor_up: None,
            tick: 0,
            planet: site.planet,
            revision: Some(site.revision),
            standing: site.position,
            radial_up: None,
            #[cfg(feature = "sensor-profile")]
            preview_normal: None,
            selected_offset: Some(-10.0),
            attempts: Vec::new(),
        }),
    });
    o.flight.pilot.actor.as_mut().unwrap().position.x = 4.02;
    task.step(&o);
    o.rebuild = None;
    o.ground = None;
    o.flight.pilot.tick = 1;
    o.flight.pilot.recovery.as_mut().unwrap().status =
        scenario_spacewars::surface_sortie::SurfaceRecoveryStatus::Rebuilding;
    task.step(&o);
    assert_eq!(
        task.telemetry().rebuild_footing.as_ref().unwrap().bearing,
        2
    );
    assert!(task.telemetry().ground.as_ref().unwrap().path.is_empty());
    o.flight.pilot.tick = 2;
    o.flight.pilot.actor.as_mut().unwrap().position.x = 3.8;
    o.ground = rebuild_relocation_fixture().1.ground;
    o.ground.as_mut().unwrap().tick = 2;
    let action = task.step(&o);
    assert!(action.controls.horizontal > 0.0);
    assert_eq!(
        task.telemetry().ground.as_ref().unwrap().started_tick,
        Some(1)
    );
    assert_eq!(task.telemetry().relocations, 1);
}

#[test]
#[cfg(feature = "sensor-profile")]
fn held_footing_corrects_supported_drift_and_keeps_arrival_and_ground_clocks() {
    let (mut task, mut o) = held_rebuild_fixture();
    let mut copy = task.clone();
    o.ground = None;
    o.flight.pilot.tick = 3;
    o.flight.pilot.actor.as_mut().unwrap().position.x = 3.8;
    let action = task.step(&o);
    assert_eq!(action, copy.step(&o));
    assert_eq!(task.telemetry(), copy.telemetry());
    assert!(action.controls.horizontal > 0.0 && action.controls.horizontal < 0.1);
    assert!(!action.controls.primary_held);
    let before = task.telemetry().clone();
    assert_eq!(task.step(&o), action);
    assert_eq!(task.telemetry(), &before);
    assert_eq!(task.telemetry().started_tick, Some(0));
    assert_eq!(
        task.telemetry().ground.as_ref().unwrap().started_tick,
        Some(1)
    );
    assert!(task.telemetry().ground.as_ref().unwrap().precise_rebuild);
    assert_eq!(task.telemetry().relocations, 1);
    assert!(task.telemetry().relocation_site.is_none());
    o.flight.pilot.tick = 4;
    o.flight.pilot.supported_planet = None;
    assert_eq!(task.step(&o), FlightIntent::default());
    o.flight.pilot.tick = 5;
    o.flight.pilot.supported_planet = Some(o.flight.pilot.planet.index);
    o.flight.pilot.queries_ready = false;
    assert_eq!(task.step(&o), FlightIntent::default());
    o.flight.pilot.tick = 6;
    o.flight.pilot.queries_ready = true;
    o.flight.pilot.actor.as_mut().unwrap().position.x = 4.02;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(
        task.telemetry()
            .rebuild_footing
            .as_ref()
            .unwrap()
            .started_tick,
        2
    );
    assert_eq!(
        task.telemetry().ground.as_ref().unwrap().goal,
        spacewars_ai::ground_task::GroundGoal::Arrived
    );
}

#[test]
#[cfg(feature = "sensor-profile")]
fn held_footing_revision_requires_a_fresh_counted_preview_with_a_fixed_search_deadline() {
    let (mut task, mut o) = held_rebuild_fixture();
    o.ground = None;
    o.flight.pilot.tick = 3;
    o.flight.pilot.planet.revision += 1;
    assert_eq!(task.step(&o), FlightIntent::default());
    let held = task.telemetry().rebuild_footing.as_ref().unwrap();
    assert_eq!(held.ended_tick, Some(3));
    assert_eq!(held.reason, Some("terrain changed"));
    assert_eq!(task.rebuild_search_request().unwrap().preferred, Some(2));
    assert_eq!(task.telemetry().relocations, 1);
    let mut missing = task.clone();
    let mut absent = o.clone();
    absent.flight.pilot.tick = 304;
    missing.step(&absent);
    assert_eq!(
        missing.telemetry().reason,
        Some("no reachable standing site with hatch access")
    );
    o.flight.pilot.tick = 4;
    o.rebuild = rebuild_relocation_fixture().1.rebuild;
    let survey = o.rebuild.as_mut().unwrap();
    survey.tick = 4;
    survey.site.as_mut().unwrap().revision = o.flight.pilot.planet.revision;
    survey.site.as_mut().unwrap().precise = true;
    task.step(&o);
    assert_eq!(task.telemetry().relocations, 2);
    assert_eq!(
        task.telemetry().relocation_site.unwrap().revision,
        o.flight.pilot.planet.revision
    );
    assert_eq!(task.rebuild_search_request().unwrap().preferred, None);
    assert_eq!(task.telemetry().started_tick, Some(0));
}

#[test]
#[cfg(feature = "sensor-profile")]
fn held_footing_releases_large_displacements_and_native_placement_failures() {
    use scenario_spacewars::surface_sortie::SurfaceRecoveryStatus;
    for displaced in [true, false] {
        let (mut task, mut o) = held_rebuild_fixture();
        o.flight.pilot.tick = 3;
        o.ground = None;
        if displaced {
            o.flight.pilot.actor.as_mut().unwrap().position.x = 1.5;
        } else {
            let site = task.telemetry().rebuild_footing.as_ref().unwrap().site;
            let recovery = o.flight.pilot.recovery.as_mut().unwrap();
            recovery.status = SurfaceRecoveryStatus::HatchBlocked;
            recovery.placement = Some(
                scenario_spacewars::surface_sortie::rebuild_placement::RebuildPlacementReport {
                    anchor: None,
                    anchor_up: None,
                    tick: 3,
                    planet: site.planet,
                    revision: Some(site.revision),
                    standing: site.position,
                    radial_up: None,
                    #[cfg(feature = "sensor-profile")]
                    preview_normal: None,
                    selected_offset: None,
                    attempts: Vec::new(),
                },
            );
        }
        assert_eq!(task.step(&o), FlightIntent::default());
        assert_eq!(
            task.telemetry().rebuild_footing.as_ref().unwrap().reason,
            Some(if displaced {
                "footing displaced"
            } else {
                "native placement rejected"
            })
        );
        assert_eq!(
            task.rebuild_search_request().unwrap().preferred,
            displaced.then_some(2)
        );
        assert_eq!(task.telemetry().relocations, 1);
        assert!(task.telemetry().relocation_site.is_none());
    }
}

#[test]
#[cfg(feature = "sensor-profile")]
fn held_footing_ignores_latched_failures_until_a_current_attempt_rejects_it() {
    use scenario_spacewars::surface_sortie::{
        SurfaceRecoveryStatus, rebuild_placement::RebuildPlacementReport,
    };
    let (task, original) = held_rebuild_fixture();
    let site = task.telemetry().rebuild_footing.as_ref().unwrap().site;
    for status in [
        SurfaceRecoveryStatus::ClearanceBlocked,
        SurfaceRecoveryStatus::HatchBlocked,
    ] {
        for fault in 0..6 {
            let mut task = task.clone();
            let mut o = original.clone();
            o.ground = None;
            o.flight.pilot.tick = 10;
            o.flight.pilot.actor.as_mut().unwrap().position.x = 3.8;
            let recovery = o.flight.pilot.recovery.as_mut().unwrap();
            recovery.status = status;
            let mut report = RebuildPlacementReport {
                anchor: None,
                anchor_up: None,
                tick: 9,
                planet: site.planet,
                revision: Some(site.revision),
                standing: site.position,
                radial_up: None,
                #[cfg(feature = "sensor-profile")]
                preview_normal: None,
                selected_offset: None,
                attempts: Vec::new(),
            };
            match fault {
                0 => report.tick = 1,
                1 => report.tick = 11,
                2 => report.planet += 1,
                3 => report.revision = Some(site.revision + 1),
                4 => report.selected_offset = Some(-10.0),
                _ => (),
            }
            recovery.placement = (fault != 5).then_some(report);
            let mut copy = task.clone();
            let action = task.step(&o);
            assert!(action.controls.horizontal > 0.0, "{status:?}/{fault}");
            assert_eq!(copy.step(&o), action);
            assert_eq!(copy.telemetry(), task.telemetry());
            assert_eq!(
                task.telemetry()
                    .rebuild_footing
                    .as_ref()
                    .unwrap()
                    .ended_tick,
                None
            );
            let before = task.telemetry().clone();
            assert_eq!(task.step(&o), action);
            assert_eq!(task.telemetry(), &before);
            o.flight.pilot.tick = 11;
            o.flight.pilot.recovery.as_mut().unwrap().placement = Some(RebuildPlacementReport {
                anchor: None,
                anchor_up: None,
                tick: 11,
                planet: site.planet,
                revision: Some(site.revision),
                standing: site.position,
                radial_up: None,
                #[cfg(feature = "sensor-profile")]
                preview_normal: None,
                selected_offset: None,
                attempts: Vec::new(),
            });
            assert_eq!(task.step(&o), FlightIntent::default());
            let held = task.telemetry().rebuild_footing.as_ref().unwrap();
            assert_eq!(held.ended_tick, Some(11));
            assert_eq!(held.reason, Some("native placement rejected"));
            assert_eq!(task.telemetry().relocations, 1);
        }
    }
}

#[test]
#[cfg(feature = "sensor-profile")]
fn footing_recheck_is_explicit_until_a_new_counted_site_or_reset() {
    let (mut task, mut o) = held_rebuild_fixture();
    task.set_rebuild_footing_recheck(true);
    assert!(!task.rebuild_search_request().unwrap().recheck_preferred);
    o.ground = None;
    o.flight.pilot.tick = 3;
    o.flight.pilot.planet.revision += 1;
    assert_eq!(task.step(&o), FlightIntent::default());
    let request = task.rebuild_search_request().unwrap();
    assert!(request.recheck_preferred);
    assert_eq!(request.preferred, Some(2));
    assert_eq!(task.telemetry().relocations, 1);
    let mut copy = task.clone();
    assert_eq!(copy.rebuild_search_request(), task.rebuild_search_request());
    o.flight.pilot.tick = 4;
    assert_eq!(task.step(&o), copy.step(&o));
    let mut expired = task.clone();
    let mut late = o.clone();
    late.flight.pilot.tick = 304;
    expired.step(&late);
    assert_eq!(
        expired.telemetry().reason,
        Some("no reachable standing site with hatch access")
    );
    o.flight.pilot.tick = 5;
    o.rebuild = rebuild_relocation_fixture().1.rebuild;
    let survey = o.rebuild.as_mut().unwrap();
    survey.tick = 5;
    survey.site.as_mut().unwrap().revision = o.flight.pilot.planet.revision;
    task.step(&o);
    assert_eq!(task.telemetry().relocations, 2);
    assert!(!task.rebuild_search_request().unwrap().recheck_preferred);
    assert_eq!(task.rebuild_search_request().unwrap().preferred, None);
    task.reset(BrainReset {
        actor: o.flight.pilot.owner,
        episode_seed: 42,
    });
    assert!(!task.rebuild_search_request().unwrap().recheck_preferred);
    assert!(task.telemetry().rebuild_footing.is_none());
    copy.set_rebuild_footing_recheck(false);
    assert!(!copy.rebuild_search_request().unwrap().recheck_preferred);
    assert_eq!(copy.rebuild_search_request().unwrap().preferred, Some(2));
}

#[test]
#[cfg(feature = "sensor-profile")]
fn held_footing_has_one_deadline_and_real_build_wins_on_a_disarmed_tick() {
    let (mut task, mut o) = held_rebuild_fixture();
    o.ground = None;
    for tick in [3, 100, 1000, 1202] {
        o.flight.pilot.tick = tick;
        task.step(&o);
        assert_eq!(task.telemetry().status, TaskStatus::Running);
    }
    let mut built = task.clone();
    o.flight.pilot.tick = 1203;
    o.flight.pilot.controls_armed = false;
    o.flight.pilot.queries_ready = false;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(
        task.telemetry().reason,
        Some("rebuild footing exceeded twenty seconds")
    );
    o.flight.pilot.ship_available = true;
    o.flight.pilot.ship_form = ShipForm::Ship;
    o.flight.pilot.recovery.as_mut().unwrap().rebuilds = 1;
    assert_eq!(built.step(&o), FlightIntent::default());
    assert_eq!(built.telemetry().status, TaskStatus::Running);
    assert_eq!(
        built.telemetry().rebuild_footing.as_ref().unwrap().reason,
        Some("ship available")
    );
    assert_eq!(built.rebuild_search_request().unwrap().preferred, None);
}

#[test]
#[cfg(feature = "sensor-profile")]
fn held_footing_reset_and_ownership_loss_clear_the_old_target() {
    let (mut task, mut o) = held_rebuild_fixture();
    let mut lost = task.clone();
    o.flight.pilot.tick = 3;
    o.ground.as_mut().unwrap().tick = 3;
    o.flight.pilot.planet.claim.as_mut().unwrap().owner = None;
    lost.step(&o);
    assert_eq!(
        lost.telemetry().rebuild_footing.as_ref().unwrap().reason,
        Some("ownership changed")
    );
    assert_eq!(lost.rebuild_search_request().unwrap().preferred, None);
    task.reset(BrainReset {
        actor: o.flight.pilot.owner,
        episode_seed: 42,
    });
    assert!(task.telemetry().rebuild_footing.is_none());
    assert!(task.telemetry().ground.is_none());
    assert_eq!(task.rebuild_search_request().unwrap().preferred, None);
    let (_, mut fresh) = rebuild_relocation_fixture();
    fresh
        .rebuild
        .as_mut()
        .unwrap()
        .site
        .as_mut()
        .unwrap()
        .precise = true;
    task.step(&fresh);
    fresh.rebuild = None;
    fresh.flight.pilot.tick = 1;
    fresh.ground.as_mut().unwrap().tick = 1;
    task.step(&fresh);
    fresh.flight.pilot.tick = 2;
    fresh.ground.as_mut().unwrap().tick = 2;
    fresh.flight.pilot.actor.as_mut().unwrap().position.x = 4.05;
    task.step(&fresh);
    assert_eq!(
        task.telemetry()
            .rebuild_footing
            .as_ref()
            .unwrap()
            .started_tick,
        2
    );
}

fn exhausted_rebuild_relocations() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    let (mut task, mut o) = rebuild_relocation_fixture();
    for relocation in 0..4 {
        let tick = 3220 + relocation * 2;
        o.flight.pilot.tick = tick;
        o.rebuild.as_mut().unwrap().tick = tick;
        o.ground.as_mut().unwrap().tick = tick;
        task.step(&o);
        assert_eq!(task.telemetry().relocations, relocation as u32 + 1);
        o.flight.pilot.tick += 1;
        o.flight.pilot.actor.as_mut().unwrap().position.x = 4.0;
        o.ground.as_mut().unwrap().tick += 1;
        task.step(&o);
        assert!(task.telemetry().relocation_site.is_none());
    }
    // The recorded native failure starts at 3220, exhausts four relocations
    // at 8328, and physically rebuilds much later, at 21948.
    o.flight.pilot.tick = 8328;
    task.step(&o);
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(
        task.telemetry().reason,
        Some("no accessible rebuild after four measured relocations")
    );
    (task, o)
}

fn observe_completed_rebuild(o: &mut RecoveryTaskObservationV1) {
    let p = &mut o.flight.pilot;
    p.tick = 21948;
    p.ship_form = ShipForm::Ship;
    p.ship_available = true;
    p.controls_armed = false; // Native rebuilding requires a neutral rearm.
    p.recovery.as_mut().unwrap().rebuilds += 1;
    p.recovery.as_mut().unwrap().status =
        scenario_spacewars::surface_sortie::SurfaceRecoveryStatus::ShipAvailable;
}

#[test]
fn native_rebuild_releases_exhausted_recovery_after_the_original_deadline() {
    use scenario_spacewars::surface_sortie::TransferResult;
    let (mut task, mut o) = exhausted_rebuild_relocations();
    observe_completed_rebuild(&mut o);
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().goal, RecoveryGoal::Board);
    assert_eq!(task.telemetry().started_tick, Some(3220));
    assert_eq!(task.telemetry().relocations, 4);
    assert!(task.telemetry().ground.is_none());
    o.flight.pilot.tick += 1;
    o.flight.pilot.controls_armed = true;
    o.flight.pilot.transfer = TransferResult::Ready;
    assert!(task.step(&o).controls.interact_held);
    let telemetry = task.telemetry().clone();
    assert!(task.step(&o).controls.interact_held);
    assert_eq!(task.telemetry(), &telemetry);
    o.flight.pilot.tick += 1;
    o.flight.pilot.location = PilotLocation::Aboard(o.flight.pilot.vehicle);
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().status, TaskStatus::Succeeded);
    assert_eq!(task.telemetry().completed_tick, Some(21950));
}

#[test]
fn recovery_requires_a_new_full_rebuild_not_availability_or_counter_replay() {
    let (mut task, mut o) = exhausted_rebuild_relocations();
    observe_completed_rebuild(&mut o);
    o.flight.pilot.recovery.as_mut().unwrap().rebuilds = 0;
    task.step(&o);
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    // A counter without a full available ship is not a completed rebuild.
    o.flight.pilot.tick += 1;
    o.flight.pilot.ship_form = ShipForm::EscapePod;
    o.flight.pilot.recovery.as_mut().unwrap().rebuilds = 1;
    task.step(&o);
    o.flight.pilot.ship_form = ShipForm::Ship;
    for count in [1, 0, 1] {
        o.flight.pilot.tick += 1;
        o.flight.pilot.recovery.as_mut().unwrap().rebuilds = count;
        task.step(&o);
        assert_eq!(task.telemetry().status, TaskStatus::Blocked);
        assert!(task.telemetry().rebuild_boarding.is_none());
    }
    o.flight.pilot.tick += 1;
    o.flight.pilot.recovery.as_mut().unwrap().rebuilds = 2;
    task.step(&o);
    assert_eq!(task.telemetry().goal, RecoveryGoal::Board);
}

#[test]
fn rebuilt_ship_boarding_has_one_deadline_including_unarmed_and_dirty_ticks() {
    let (mut task, mut o) = exhausted_rebuild_relocations();
    observe_completed_rebuild(&mut o);
    o.flight.pilot.queries_ready = false;
    task.step(&o);
    let receipt = task.telemetry().rebuild_boarding.clone().unwrap();
    assert_eq!(receipt.started_tick, 21948);
    assert_eq!(receipt.deadline_tick, 21948 + 90 * 60);
    for tick in [21949, receipt.deadline_tick] {
        o.flight.pilot.tick = tick;
        o.flight.pilot.controls_armed = true;
        assert_eq!(task.step(&o), FlightIntent::default());
        assert_eq!(task.telemetry().status, TaskStatus::Running);
        assert_eq!(task.telemetry().rebuild_boarding.as_ref(), Some(&receipt));
    }
    o.flight.pilot.tick += 1;
    o.flight.pilot.controls_armed = false;
    task.step(&o);
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(
        task.telemetry().reason,
        Some("rebuilt ship boarding exceeded ninety seconds")
    );
    // Even another counter advance cannot extend this task's one opportunity.
    o.flight.pilot.tick += 1;
    o.flight.pilot.controls_armed = true;
    o.flight.pilot.queries_ready = true;
    o.flight.pilot.recovery.as_mut().unwrap().rebuilds += 1;
    o.flight.pilot.transfer = scenario_spacewars::surface_sortie::TransferResult::Ready;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(task.telemetry().rebuild_boarding.as_ref(), Some(&receipt));
    task.reset(BrainReset {
        actor: o.flight.pilot.owner,
        episode_seed: 42,
    });
    task.step(&o);
    assert!(task.telemetry().rebuild_boarding.is_none());
}

#[test]
fn native_rebuild_respects_observation_identity_versions_and_monotonic_ticks() {
    let (task, original) = exhausted_rebuild_relocations();
    for fault in 0..6 {
        let mut task = task.clone();
        let mut o = original.clone();
        match fault {
            0 => o.version += 1,
            1 => o.flight.version += 1,
            2 => o.flight.pilot.version += 1,
            3 => o.flight.pilot.owner = PlayerId::PLAYER_2,
            4 => o.flight.pilot.vehicle.0 += 1,
            _ => o.flight.pilot.tick -= 1,
        }
        assert_eq!(task.step(&o), FlightIntent::default());
        let reason = task.telemetry().reason;
        let mut o = original.clone();
        observe_completed_rebuild(&mut o);
        assert_eq!(task.step(&o), FlightIntent::default());
        assert_eq!(task.telemetry().status, TaskStatus::Blocked);
        assert_eq!(task.telemetry().reason, reason);
        assert!(task.telemetry().rebuild_boarding.is_none());
    }
}

#[test]
fn rebuilt_ship_boarding_accepts_physical_completion_at_the_deadline_and_latches_bad_identity() {
    use scenario_spacewars::surface_sortie::TransferResult;
    let (mut task, mut o) = exhausted_rebuild_relocations();
    observe_completed_rebuild(&mut o);
    task.step(&o);
    o.flight.pilot.tick = task
        .telemetry()
        .rebuild_boarding
        .as_ref()
        .unwrap()
        .deadline_tick;
    o.flight.pilot.controls_armed = true;
    o.flight.pilot.transfer = TransferResult::Ready;
    // Keep the existing no-progress check satisfied while isolating the final
    // deadline edge; dirty queries have no bearing on physical completion.
    o.flight.pilot.queries_ready = false;
    task.step(&o);
    o.flight.pilot.tick += 1;
    o.flight.pilot.location = PilotLocation::Aboard(o.flight.pilot.vehicle);
    o.flight.pilot.controls_armed = false;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().status, TaskStatus::Succeeded);

    let (mut task, mut o) = exhausted_rebuild_relocations();
    observe_completed_rebuild(&mut o);
    task.step(&o);
    o.flight.pilot.tick += 1;
    o.flight.pilot.controls_armed = true;
    o.flight.pilot.transfer = TransferResult::Ready;
    assert!(task.step(&o).controls.interact_held);
    let mut bad = o.clone();
    bad.flight.pilot.owner = PlayerId::PLAYER_2;
    assert_eq!(task.step(&bad), FlightIntent::default());
    assert_eq!(
        task.step(&o),
        FlightIntent::default(),
        "do not replay cached input after invalid identity"
    );
    o.flight.pilot.tick += 1;
    o.flight.pilot.location = PilotLocation::Aboard(o.flight.pilot.vehicle);
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
}

#[test]
fn rebuild_budget_only_extends_an_exhausted_task_and_stops_if_ship_is_lost() {
    for expired in [false, true] {
        let (mut task, mut o) = rebuild_relocation_fixture();
        task.step(&o);
        observe_completed_rebuild(&mut o);
        o.flight.pilot.tick = if expired { 120 * 60 + 1 } else { 1 };
        task.step(&o);
        assert_eq!(task.telemetry().rebuild_boarding.is_some(), expired);
        o.flight.pilot.tick += 1;
        o.flight.pilot.controls_armed = true;
        o.flight.pilot.transfer = scenario_spacewars::surface_sortie::TransferResult::Ready;
        assert!(task.step(&o).controls.interact_held);
        if expired {
            o.flight.pilot.tick += 1;
            o.flight.pilot.ship_available = false;
            assert_eq!(task.step(&o), FlightIntent::default());
            assert_eq!(
                task.telemetry().reason,
                Some("rebuilt ship no longer available")
            );
        } else {
            o.flight.pilot.tick = 120 * 60 + 1;
            task.step(&o);
            assert_eq!(
                task.telemetry().reason,
                Some("recovery exceeded two-minute task budget")
            );
        }
    }
}

#[test]
fn rebuilt_ship_boarding_keeps_stall_checks_and_cannot_start_another_replacement() {
    let (mut task, mut o) = ship_on_other_planet();
    let started = o.flight.pilot.tick;
    task.step(&o);
    o.flight.pilot.tick += 181;
    o.flight.pilot.ship_available = false;
    task.step(&o);
    o.flight.pilot.tick = started + 120 * 60 + 1;
    task.step(&o);
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(task.telemetry().scuttle_attempts, 1);
    let scuttled = task.telemetry().scuttled_tick;
    observe_completed_rebuild(&mut o);
    task.step(&o);
    o.flight.pilot.tick += 1;
    o.flight.pilot.controls_armed = true;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(
        task.telemetry().reason,
        Some("rebuilt ship has no accessible return")
    );
    assert_eq!(task.telemetry().scuttle_attempts, 1);
    assert_eq!(task.telemetry().scuttled_tick, scuttled);
    assert_eq!(task.telemetry().started_tick, Some(started));

    let (mut task, mut o) = exhausted_rebuild_relocations();
    observe_completed_rebuild(&mut o);
    task.step(&o);
    o.flight.pilot.tick += 1;
    o.flight.pilot.controls_armed = true;
    o.flight.pilot.transfer = scenario_spacewars::surface_sortie::TransferResult::ShipNotSettled;
    o.flight.pilot.boarding_hatches = [None, None];
    task.step(&o);
    o.flight.pilot.tick += 901;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert!(
        o.flight.pilot.tick
            < task
                .telemetry()
                .rebuild_boarding
                .as_ref()
                .unwrap()
                .deadline_tick
    );
    assert_eq!(task.telemetry().scuttle_attempts, 0);
}

#[test]
fn both_hosts_resume_after_a_native_rebuild_without_a_new_loss() {
    use engine_common::CombatBreakSettings;
    use spacewars_ai::mission_pilot::MaterialMissionPilot;
    let (_, mut o) = airborne_pod();
    o.flight.pilot.tick = 3220;
    o.flight.pilot.ship_available = false;
    o.flight.pilot.recovery.as_mut().unwrap().ships_lost = 1;
    let context = BrainReset {
        actor: o.flight.pilot.owner,
        episode_seed: 42,
    };
    let state = SurfaceSortieScenario::init_material_travel(42, false);
    let mut mission_o = state.mission_observation(0, None);
    let mut mission = MaterialMissionPilot::new(context, CombatBreakSettings::default());
    let mut sortie = RulePilotV3::new(context);
    mission_o.local.combat.recovery = o.clone();
    mission.intent(&mission_o);
    sortie.intent(&o);
    assert_eq!(
        mission.telemetry().recovery.as_ref().unwrap().status,
        TaskStatus::Blocked
    );
    assert_eq!(
        sortie.telemetry().recovery.as_ref().unwrap().status,
        TaskStatus::Blocked
    );
    observe_completed_rebuild(&mut o);
    o.flight.pilot.location = PilotLocation::OnFoot;
    for (tick, location, armed) in [
        (21948, PilotLocation::OnFoot, false),
        (21949, PilotLocation::OnFoot, true),
        (21950, PilotLocation::Aboard(o.flight.pilot.vehicle), true),
    ] {
        o.flight.pilot.tick = tick;
        o.flight.pilot.location = location;
        o.flight.pilot.controls_armed = armed;
        o.flight.pilot.transfer = scenario_spacewars::surface_sortie::TransferResult::Ready;
        mission_o.local.combat.recovery = o.clone();
        mission.intent(&mission_o);
        sortie.intent(&o);
        let mission_recovery = mission.telemetry().recovery.as_ref();
        // The mission host retires a completed task; the sortie host retains
        // its final receipt. Both publish one completion below.
        assert_eq!(mission_recovery.is_some(), tick != 21950);
        for t in mission_recovery
            .into_iter()
            .chain(sortie.telemetry().recovery.as_ref())
        {
            assert_eq!(t.started_tick, Some(3220));
            assert_eq!(t.rebuild_boarding.as_ref().unwrap().started_tick, 21948);
            assert_eq!(
                t.goal,
                if tick == 21950 {
                    RecoveryGoal::Complete
                } else {
                    RecoveryGoal::Board
                }
            );
        }
    }
    assert_eq!(mission.telemetry().completed_recoveries, 1);
    assert_eq!(sortie.telemetry().completed_recoveries, 1);
}

#[test]
fn progressing_high_spin_pod_can_take_longer_than_fifteen_seconds() {
    for turn_acceleration in [3.0, 6.0] {
        let (mut task, mut o) = airborne_pod();
        o.flight.flight.limits.turn_acceleration = turn_acceleration;
        // Both actuators are making observable progress, at their real limits.
        for tick in 0..=40 * 60 {
            let p = &mut o.flight.pilot;
            p.tick = tick;
            p.ship.spin = (114.0 - turn_acceleration * tick as f32 / 60.0).max(0.0);
            p.ship.velocity = Vec2::X * (600.0 - 40.0 * tick as f32 / 60.0).max(0.0);
            let intent = task.step(&o);
            assert!(intent.controls.brake_held);
            assert!(!intent.controls.interact_held);
            assert_eq!(task.telemetry().status, TaskStatus::Running);
            let telemetry = task.telemetry().clone();
            assert_eq!(task.step(&o), intent);
            assert_eq!(task.telemetry(), &telemetry);
            let s = telemetry.stabilization.as_ref().unwrap();
            if let Some(settled) = s.settled_tick {
                assert!(settled > 15 * 60);
                assert!(s.relative_spin.abs() < 0.5);
                assert!(s.relative_speed < 6.0);
                break;
            }
        }
        assert!(
            task.telemetry()
                .stabilization
                .as_ref()
                .unwrap()
                .settled_tick
                .is_some()
        );
    }
}

#[test]
fn stalled_pod_blocks_and_a_brief_alignment_does_not_count_as_settled() {
    let (mut task, mut o) = airborne_pod();
    for tick in 0..=902 {
        o.flight.pilot.tick = tick;
        o.flight.pilot.ship.spin = 30.0;
        task.step(&o);
    }
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(
        task.telemetry().reason,
        Some("pod stabilization stopped making progress")
    );
    assert!(
        task.telemetry()
            .stabilization
            .as_ref()
            .unwrap()
            .settled_tick
            .is_none()
    );

    let (mut task, mut o) = airborne_pod();
    for tick in 0..=24 {
        o.flight.pilot.tick = tick;
        o.flight.pilot.ship.spin = if tick == 10 { 1.0 } else { 0.0 };
        task.step(&o);
        let settled = task
            .telemetry()
            .stabilization
            .as_ref()
            .unwrap()
            .settled_tick;
        if tick < 23 {
            assert!(settled.is_none());
        } else {
            assert_eq!(settled, Some(23));
        }
    }
}

#[test]
fn another_strike_restarts_stabilization_without_restarting_task_budget() {
    let (mut task, mut o) = airborne_pod();
    for tick in 0..=13 {
        o.flight.pilot.tick = tick;
        task.step(&o);
    }
    assert_eq!(task.telemetry().goal, RecoveryGoal::LandPod);
    assert!(task.site_request().is_some());
    let started = task.telemetry().started_tick;
    o.flight.pilot.tick = 14;
    o.flight.pilot.ship.spin = -50.0;
    o.flight.pilot.ship.velocity = Vec2::X * 120.0;
    task.step(&o);
    assert_eq!(task.telemetry().goal, RecoveryGoal::StabilizePod);
    assert!(task.site_request().is_none());
    assert_eq!(task.telemetry().stabilization.as_ref().unwrap().attempts, 2);
    assert_eq!(task.telemetry().started_tick, started);
    let mut copy = task.clone();
    o.flight.pilot.tick = 120 * 60 + 1;
    assert_eq!(task.step(&o), copy.step(&o));
    assert_eq!(task.telemetry(), copy.telemetry());
    assert_eq!(
        task.telemetry().reason,
        Some("recovery exceeded two-minute task budget")
    );
    task.reset(BrainReset {
        actor: o.flight.pilot.owner,
        episode_seed: 42,
    });
    assert!(task.telemetry().stabilization.is_none());
}

#[test]
fn real_strike_recovers_and_departs_in_both_seats_and_angles() {
    for seat in 0..2 {
        for oblique in [false, true] {
            let owner = PlayerId::from_index(seat).unwrap();
            let mut s = SurfaceSortieScenario::init_material(42, 2);
            let mut brain = RulePilotV3::new(BrainReset {
                actor: owner,
                episode_seed: 42,
            });
            let mut fired = false;
            for _ in 0..180 * 60 {
                let o = s.recovery_task_observation(seat, brain.site_request());
                let intent = brain.intent(&o);
                assert!(
                    intent.controls.horizontal.is_finite()
                        && intent.controls.horizontal.abs() <= 1.0
                );
                assert!(
                    !(intent.controls.primary_held
                        && intent.controls.brake_held
                        && intent.controls.interact_held)
                );
                if !o.flight.pilot.controls_armed {
                    assert_eq!(intent, FlightIntent::default());
                }
                let strike = !fired && brain.telemetry().flight.completed_tick.is_some();
                fired |= strike;
                let mut actions = intent.encode(owner).to_vec();
                actions.push(
                    SurfaceImpactAction {
                        held: strike,
                        oblique,
                        ..Default::default()
                    }
                    .encode(owner),
                );
                SurfaceSortieScenario::step(&mut s, &actions, DT);
                if brain.telemetry().departed_tick.is_some() {
                    break;
                }
            }
            assert!(
                brain.telemetry().departed_tick.is_some(),
                "seat={seat} oblique={oblique}: {:?}",
                brain.telemetry()
            );
            assert_eq!(brain.telemetry().completed_recoveries, 1);
            let recovery = s.observation(seat).recovery.unwrap();
            assert_eq!(
                (
                    recovery.ships_lost,
                    recovery.pod_ejections,
                    recovery.rebuilds
                ),
                (1, 1, 1)
            );
            assert_eq!(s.damage_observation(seat).last_source, Some("asteroid"));
            assert!(s.terrain_diagnostics().issues.is_empty());
        }
    }
}

fn stranded_on_foot() -> SurfaceSortieState {
    let mut s = SurfaceSortieScenario::init_material(42, 1);
    for _ in 0..120 {
        SurfaceSortieScenario::step(&mut s, &[], DT);
    }
    SurfaceSortieScenario::step(
        &mut s,
        &[SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        }
        .encode(PlayerId::PLAYER_1)],
        DT,
    );
    for _ in 0..240 {
        SurfaceSortieScenario::step(
            &mut s,
            &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
            DT,
        );
    }
    assert_eq!(s.location(0), PilotLocation::OnFoot);
    SurfaceSortieScenario::step(
        &mut s,
        &[SurfaceImpactAction {
            held: true,
            ..Default::default()
        }
        .encode(PlayerId::PLAYER_1)],
        DT,
    );
    for _ in 0..120 {
        SurfaceSortieScenario::step(
            &mut s,
            &[SurfaceImpactAction::default().encode(PlayerId::PLAYER_1)],
            DT,
        );
        if s.observation(0).recovery.unwrap().ships_lost > 0 {
            return s;
        }
    }
    panic!("asteroid did not destroy the parked ship");
}

#[test]
fn standalone_task_rebuilds_after_real_loss_and_flag_or_support_removal() {
    for disruption in [
        RecoveryDisruption::FlagFooting,
        RecoveryDisruption::SpacelingSupport,
    ] {
        let mut s = stranded_on_foot();
        let mut task = RecoverShipTask::new(BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        });
        let mut edited = false;
        let mut last_survey = None;
        for _ in 0..120 * 60 {
            let o = s.recovery_task_observation(0, task.site_request());
            if o.rebuild.is_some() {
                last_survey = o.rebuild.clone();
            }
            let intent = task.step(&o);
            if !edited && o.flight.pilot.recovery.as_ref().unwrap().rebuild_progress > 0.3 {
                assert!(s.queue_recovery_disruption(0, disruption));
                edited = true;
            }
            SurfaceSortieScenario::step(&mut s, &intent.encode(PlayerId::PLAYER_1), DT);
            if task.telemetry().status == TaskStatus::Succeeded {
                break;
            }
        }
        assert!(edited);
        assert_eq!(
            task.telemetry().status,
            TaskStatus::Succeeded,
            "{disruption:?}: {:?}\nsurvey: {}\nstate: {:?}",
            task.telemetry(),
            serde_json::to_string_pretty(&last_survey).unwrap(),
            s.observation(0)
        );
        let r = s.observation(0).recovery.unwrap();
        assert!(r.rebuild_interruptions > 0);
        assert_eq!((r.ships_lost, r.pod_ejections, r.rebuilds), (1, 0, 1));
        assert!(matches!(s.location(0), PilotLocation::Aboard(_)));
    }
}

#[test]
fn task_replay_reset_identity_and_bounded_failure_contract() {
    let mut s = stranded_on_foot();
    let context = BrainReset {
        actor: PlayerId::PLAYER_1,
        episode_seed: 42,
    };
    let mut task = RecoverShipTask::new(context);
    for _ in 0..120 {
        let o = s.recovery_task_observation(0, task.site_request());
        let intent = task.step(&o);
        let telemetry = task.telemetry().clone();
        assert_eq!(task.step(&o), intent);
        assert_eq!(task.telemetry(), &telemetry);
        SurfaceSortieScenario::step(&mut s, &intent.encode(context.actor), DT);
    }
    let mut copy = task.clone();
    let mut other = s.clone();
    for _ in 0..120 {
        let a = s.recovery_task_observation(0, task.site_request());
        let b = other.recovery_task_observation(0, copy.site_request());
        assert_eq!(a, b);
        let intent = task.step(&a);
        assert_eq!(intent, copy.step(&b));
        SurfaceSortieScenario::step(&mut s, &intent.encode(context.actor), DT);
        SurfaceSortieScenario::step(&mut other, &intent.encode(context.actor), DT);
    }
    let mut wrong = s.recovery_task_observation(0, task.site_request());
    task.step(&wrong);
    wrong.flight.pilot.owner = PlayerId::PLAYER_2;
    assert_eq!(task.step(&wrong), FlightIntent::default());
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    task.reset(context);
    assert_eq!(task.telemetry(), RecoverShipTask::new(context).telemetry());
    let mut blocked = s.recovery_task_observation(0, None);
    blocked.flight.pilot.ship_available = false;
    blocked.flight.pilot.ship_form = ShipForm::EscapePod;
    blocked.flight.pilot.location = PilotLocation::Aboard(blocked.flight.pilot.vehicle);
    blocked.flight.pilot.controls_armed = true;
    task.step(&blocked);
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    let reason = task.telemetry().reason;
    blocked.flight.pilot.tick += 1000;
    blocked.flight.pilot.ship_available = true;
    task.step(&blocked);
    assert_eq!(
        task.telemetry().reason,
        reason,
        "terminal failure requires caller reset"
    );
}

#[test]
fn dirty_queries_do_not_reject_a_pod_site_but_completed_edits_do() {
    let owner = PlayerId::PLAYER_1;
    let mut s = SurfaceSortieScenario::init_material_flight(
        42,
        1,
        &[(
            owner,
            scenario_spacewars::surface_sortie::pilot::MaterialFlightStart {
                bearing: 0.0,
                altitude: 100.0,
                radial_speed: 0.0,
                lateral_speed: 0.0,
                heading_offset: 0.0,
            },
        )],
    );
    SurfaceSortieScenario::step(&mut s, &[], DT);
    SurfaceSortieScenario::step(
        &mut s,
        &[SurfaceImpactAction {
            held: true,
            ..Default::default()
        }
        .encode(owner)],
        DT,
    );
    for _ in 0..120 {
        SurfaceSortieScenario::step(&mut s, &[SurfaceImpactAction::default().encode(owner)], DT);
        if s.observation(0).recovery.unwrap().ships_lost > 0 {
            break;
        }
    }
    assert_eq!(
        s.recovery_task_observation(0, None).flight.pilot.ship_form,
        ShipForm::EscapePod
    );
    let mut task = RecoverShipTask::new(BrainReset {
        actor: owner,
        episode_seed: 42,
    });
    for _ in 0..60 * 30 {
        let o = s.recovery_task_observation(0, task.site_request());
        let intent = task.step(&o);
        let mut actions = intent.encode(owner).to_vec();
        actions.push(SurfaceImpactAction::default().encode(owner));
        SurfaceSortieScenario::step(&mut s, &actions, DT);
        if task.site_request().is_some() {
            break;
        }
    }
    let site = task
        .site_request()
        .expect("pod must select a surveyed site");
    let mut dirty = s.recovery_task_observation(0, Some(site));
    dirty.flight.pilot.queries_ready = false;
    dirty.sites.clear();
    task.step(&dirty);
    assert_eq!(task.site_request(), Some(site));
    let before = task.telemetry().invalidations;
    assert!(s.queue_recovery_disruption(0, RecoveryDisruption::LandingSite(site)));
    SurfaceSortieScenario::step(&mut s, &[], DT);
    let changed = s.recovery_task_observation(0, Some(site));
    task.step(&changed);
    assert_eq!(task.site_request(), None);
    assert_eq!(task.telemetry().invalidations, before + 1);
}

#[test]
fn hostile_ground_gets_one_fixed_extension_without_restarting_the_recovery_clock() {
    use scenario_spacewars::surface_sortie::PlanetFlagObservation;
    let (mut task, mut o) = airborne_pod();
    let p = &mut o.flight.pilot;
    p.tick = 0;
    p.location = PilotLocation::OnFoot;
    p.ship_available = false;
    p.balanced = true;
    p.supported_planet = Some(p.planet.index);
    p.actor = Some(p.ship);
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: PlayerId::PLAYER_2,
        position: p.actor.unwrap().position + Vec2::X * 20.0,
        normal: Vec2::Y,
        raised_fraction: 1.0,
    });
    task.step(&o);
    assert_eq!(task.telemetry().ground_budget_ticks, 90 * 60);
    assert_eq!(task.telemetry().started_tick, Some(0));
    o.flight.pilot.tick = 30;
    o.flight.pilot.planet.revision += 1;
    o.flight
        .pilot
        .planet
        .claim
        .as_mut()
        .unwrap()
        .flag
        .as_mut()
        .unwrap()
        .position
        .x += 1.0;
    task.step(&o);
    assert_eq!(task.telemetry().ground_budget_ticks, 90 * 60);
    assert_eq!(task.telemetry().started_tick, Some(0));
    o.flight.pilot.tick = 210 * 60 + 1;
    task.step(&o);
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(
        task.telemetry().reason,
        Some("recovery exceeded combined flight and ground budget")
    );
}

fn ship_on_other_planet() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    let (task, mut o) = airborne_pod();
    let p = &mut o.flight.pilot;
    p.location = PilotLocation::OnFoot;
    p.ship_form = ShipForm::Ship;
    p.balanced = true;
    p.relative_speed = 0.0;
    p.supported_planet = Some(p.planet.index);
    p.actor = Some(p.ship);
    p.landing.phase = LandingPhase::Landed;
    p.landing.supported_feet = 2;
    p.landing.planet = Some(p.planet.index + 1);
    p.hatch = Some(p.ship.position);
    p.boarding_hatches = [p.hatch, None];
    p.transfer = scenario_spacewars::surface_sortie::TransferResult::TooFar;
    (task, o)
}

fn cramped_return() -> (RecoverShipTask, RecoveryTaskObservationV1) {
    use scenario_spacewars::surface_sortie::{
        ground_navigation::{GroundMap, GroundNode},
        ground_posture::{GroundPostureObservation, SpacelingBalance, SpacelingGetUpResult},
    };
    let (task, mut o) = ship_on_other_planet();
    let p = &mut o.flight.pilot;
    p.tick = 0;
    p.planet.motion.position = Vec2::ZERO;
    p.planet.motion.velocity = Vec2::ZERO;
    p.planet.motion.angle = 0.0;
    p.planet.motion.spin = 0.0;
    p.ship.position = Vec2::new(0.0, 65.0);
    p.actor.as_mut().unwrap().position = Vec2::new(0.0, 60.0 + HALF_HEIGHT);
    p.actor.as_mut().unwrap().velocity = Vec2::ZERO;
    p.actor_up = Vec2::Y;
    p.hatch = Some(Vec2::new(10.0, 60.0));
    p.boarding_hatches = [p.hatch, None];
    p.landing.planet = Some(p.planet.index);
    o.jetpack = None;
    o.ground = Some(GroundMap {
        version: 1,
        actor: p.owner,
        planet: p.planet.index,
        revision: p.planet.revision,
        tick: 0,
        nodes: vec![GroundNode {
            id: 0,
            position: Vec2::new(5.0, 60.0),
            normal: Vec2::Y,
        }],
        edges: vec![],
        rejected: vec![],
    });
    o.posture = Some(GroundPostureObservation {
        version: 1,
        owner: p.owner,
        planet: p.planet.index,
        revision: p.planet.revision,
        tick: 0,
        balance: SpacelingBalance::Balanced,
        get_up_result: SpacelingGetUpResult::NotRequested,
        get_up_attempts: 0,
        stable: true,
        standing_clear: false,
        crawl: [None, None],
        crawl_clearance: [None, None],
        crawl_floor: [None, None],
    });
    (task, o)
}

fn refresh_return(o: &mut RecoveryTaskObservationV1, tick: u64) {
    o.flight.pilot.tick = tick;
    if let Some(map) = &mut o.ground {
        map.tick = tick;
    }
    o.posture.as_mut().unwrap().tick = tick;
}

#[test]
fn a_grounded_but_inaccessible_hatch_uses_replacement_only_after_bounded_escape() {
    use spacewars_ai::ground_task::ShipReturnFailure;
    let (mut task, mut o) = cramped_return();
    assert!(!task.step(&o).controls.interact_held);
    assert_eq!(task.telemetry().scuttle_attempts, 0);
    refresh_return(&mut o, 8 * 60 + 1);
    let hold = task.step(&o).controls;
    assert!(hold.primary_held && hold.interact_held && hold.brake_held);
    assert_eq!(
        task.telemetry().return_failure,
        Some(ShipReturnFailure::NoStandingRoute)
    );
    assert_eq!(task.telemetry().scuttle_attempts, 1);
    let mut clone = task.clone();
    o.ground = None;
    refresh_return(&mut o, 8 * 60 + 2);
    assert_eq!(task.step(&o), clone.step(&o));
    assert!(
        task.step(&o).controls.interact_held,
        "confirmed obstruction persists between scheduled surveys"
    );
    refresh_return(&mut o, 8 * 60 + 31);
    assert_eq!(
        task.step(&o),
        FlightIntent::default(),
        "expired measurements cannot hold the scuttle chord"
    );
}

#[test]
fn regained_footing_or_changed_evidence_cancels_a_cramped_return_hold() {
    for fault in 0..7 {
        let (mut task, mut o) = cramped_return();
        task.step(&o);
        refresh_return(&mut o, 8 * 60 + 1);
        assert!(task.step(&o).controls.interact_held);
        refresh_return(&mut o, 8 * 60 + 2);
        match fault {
            0 => o.flight.pilot.actor.as_mut().unwrap().position.x = 3.0,
            1 => o.posture.as_mut().unwrap().standing_clear = true,
            2 => {
                o.flight.pilot.transfer = scenario_spacewars::surface_sortie::TransferResult::Ready
            }
            3 => o.flight.pilot.supported_planet = None,
            4 => o.flight.pilot.queries_ready = false,
            5 => {
                o.flight.pilot.planet.revision += 1;
                o.ground = None;
            }
            _ => o.ground.as_mut().unwrap().tick += 1,
        }
        let action = task.step(&o).controls;
        assert!(
            !(action.primary_held && action.interact_held && action.brake_held),
            "fault {fault}"
        );
        assert_eq!(task.telemetry().scuttle_attempts, 1);
        assert!(task.telemetry().scuttled_tick.is_none());
    }
}

#[test]
fn unreachable_ship_uses_shared_scuttle_then_releases_and_retains_recovery_budget() {
    let (mut task, mut o) = ship_on_other_planet();
    let first = task.step(&o);
    assert!(
        first.controls.primary_held && first.controls.interact_held && first.controls.brake_held
    );
    assert_eq!(task.telemetry().goal, RecoveryGoal::Scuttle);
    assert_eq!(task.telemetry().scuttle_attempts, 1);
    let initial = task.telemetry().started_tick;
    assert_eq!(task.step(&o), first);
    let mut copy = task.clone();
    o.flight.pilot.tick += 1;
    o.flight.pilot.queries_ready = false;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().started_tick, initial);
    o.flight.pilot.tick += 1;
    o.flight.pilot.queries_ready = true;
    o.flight.pilot.ship_available = false;
    o.flight.pilot.controls_armed = false;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(copy.step(&o), FlightIntent::default());
    o.flight.pilot.tick += 1;
    o.flight.pilot.controls_armed = true;
    assert_eq!(task.step(&o), FlightIntent::default());
    assert_eq!(task.telemetry().scuttled_tick, Some(o.flight.pilot.tick));
    assert_eq!(task.telemetry().started_tick, initial);
    o.flight.pilot.tick += 1;
    o.flight.pilot.planet.claim.as_mut().unwrap().owner = Some(o.flight.pilot.owner);
    task.step(&o);
    assert_eq!(task.telemetry().goal, RecoveryGoal::Rebuild);
}

#[test]
fn returning_ship_cancels_scuttle_and_support_loss_never_holds_the_chord() {
    use scenario_spacewars::surface_sortie::TransferResult;
    let (mut task, mut o) = ship_on_other_planet();
    task.step(&o);
    o.flight.pilot.tick += 1;
    o.flight.pilot.supported_planet = None;
    assert_eq!(task.step(&o), FlightIntent::default());
    o.flight.pilot.tick += 1;
    o.flight.pilot.supported_planet = Some(o.flight.pilot.planet.index);
    o.flight.pilot.landing.planet = Some(o.flight.pilot.planet.index);
    o.flight.pilot.transfer = TransferResult::Ready;
    assert_eq!(
        task.step(&o),
        FlightIntent::default(),
        "release the cancelled chord"
    );
    o.flight.pilot.tick += 1;
    let board = task.step(&o).controls;
    assert!(board.interact_held && !board.primary_held && !board.brake_held);
    assert!(task.telemetry().scuttled_tick.is_none());
}

#[test]
fn an_unsettled_nearby_ship_waits_before_replacement_but_moving_ships_do_not_qualify() {
    for moving in [false, true] {
        let (mut task, mut o) = ship_on_other_planet();
        let p = &mut o.flight.pilot;
        p.landing.planet = Some(p.planet.index);
        p.landing.phase = LandingPhase::Assisted;
        p.landing.supported_feet = 0;
        p.landing.lateral_speed = if moving { 5.0 } else { 0.0 };
        p.hatch = None;
        p.boarding_hatches = [p.hatch, None];
        task.step(&o);
        assert_eq!(task.telemetry().scuttle_attempts, 0);
        o.flight.pilot.tick += 901;
        let action = task.step(&o).controls;
        assert_eq!(
            action.interact_held && action.brake_held && action.primary_held,
            !moving
        );
        if moving {
            o.flight.pilot.tick += 901;
            task.step(&o);
            assert_eq!(task.telemetry().status, TaskStatus::Blocked);
            assert_eq!(task.telemetry().scuttle_attempts, 0);
        }
    }
}

#[test]
fn a_replacement_cannot_trigger_another_scuttle_in_the_same_task() {
    let (mut task, mut o) = ship_on_other_planet();
    task.step(&o);
    o.flight.pilot.tick += 181;
    o.flight.pilot.ship_available = false;
    task.step(&o);
    assert!(task.telemetry().scuttled_tick.is_some());
    o.flight.pilot.tick += 1;
    o.flight.pilot.ship_available = true;
    let action = task.step(&o);
    assert_eq!(action, FlightIntent::default());
    assert_eq!(task.telemetry().status, TaskStatus::Blocked);
    assert_eq!(task.telemetry().scuttle_attempts, 1);
    assert_eq!(
        task.telemetry().reason,
        Some("replacement ship also has no accessible return")
    );
}

#[test]
fn both_host_policies_keep_the_active_recovery_clock_across_a_deliberate_loss() {
    use engine_common::CombatBreakSettings;
    use spacewars_ai::mission_pilot::MaterialMissionPilot;
    let (_, mut o) = ship_on_other_planet();
    let context = BrainReset {
        actor: o.flight.pilot.owner,
        episode_seed: 42,
    };
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut mission_o = state.mission_observation(0, None);
    let mut mission = MaterialMissionPilot::new(context, CombatBreakSettings::default());
    let mut sortie = RulePilotV3::new(context);
    // A prior loss starts the standalone host's recovery. Both hosts then
    // encounter the same unavailable return and ordinary scuttle observation.
    o.flight.pilot.recovery.as_mut().unwrap().ships_lost = 1;
    mission_o.local.combat.recovery = o.clone();
    mission.intent(&mission_o);
    sortie.intent(&o);
    let started = o.flight.pilot.tick;
    for tick in [started + 181, started + 182] {
        o.flight.pilot.tick = tick;
        o.flight.pilot.ship_available = false;
        o.flight.pilot.recovery.as_mut().unwrap().ships_lost = 2;
        mission_o.local.combat.recovery = o.clone();
        mission.intent(&mission_o);
        sortie.intent(&o);
        for telemetry in [
            mission.telemetry().recovery.as_ref().unwrap(),
            sortie.telemetry().recovery.as_ref().unwrap(),
        ] {
            assert_eq!(telemetry.started_tick, Some(started));
            assert_eq!(telemetry.scuttle_attempts, 1);
            assert_eq!(telemetry.scuttled_tick, Some(started + 181));
        }
    }
}

#[test]
fn physical_return_boards_and_departs_with_replacement_only_when_needed() {
    use scenario_spacewars::surface_sortie::return_trial::ReturnTrial;
    for (seat, mirror, bearing, trial) in [
        (0, false, 0.0, ReturnTrial::Reachable),
        (
            1,
            true,
            std::f32::consts::FRAC_PI_2,
            ReturnTrial::TippedShip,
        ),
        (0, true, 0.0, ReturnTrial::OtherPlanet),
    ] {
        let owner = PlayerId::from_index(seat).unwrap();
        let mut state =
            SurfaceSortieScenario::init_material_return_trial(42, seat, mirror, bearing, trial);
        let initial = state.terrain_diagnostics().occupied_cells;
        let mut task = RecoverShipTask::new(BrainReset {
            actor: owner,
            episode_seed: 42,
        });
        let mut claimed = false;
        let mut departed = false;
        for _ in 0..90 * 60 {
            let o = state.recovery_task_observation(seat, task.site_request());
            let p = &o.flight.pilot;
            claimed |= p
                .planet
                .claim
                .as_ref()
                .is_some_and(|c| c.owner == Some(owner));
            let mut intent = if claimed {
                task.step(&o)
            } else {
                FlightIntent::default()
            };
            if task.telemetry().status == TaskStatus::Succeeded {
                assert!(matches!(p.location, PilotLocation::Aboard(_)));
                if p.ship.position.distance_to(p.planet.motion.position) > p.planet.radius + 70.0 {
                    departed = true;
                    break;
                }
                intent.controls.primary_held = true;
            }
            SurfaceSortieScenario::step(&mut state, &intent.encode(owner), DT);
        }
        assert!(departed, "{trial:?}: {:?}", task.telemetry());
        let r = state.observation(seat).recovery.unwrap();
        let expected = u64::from(trial != ReturnTrial::Reachable);
        assert_eq!(
            (r.ships_lost, r.pod_ejections, r.rebuilds),
            (expected, 0, expected),
            "{trial:?}"
        );
        let audit = state.terrain_diagnostics();
        assert!(audit.issues.is_empty());
        assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
    }
}

#[test]
fn a_present_hatch_cannot_cancel_replacement_until_the_ship_actually_settles() {
    use scenario_spacewars::surface_sortie::TransferResult;
    for moving in [false, true] {
        let (mut task, mut o) = ship_on_other_planet();
        let p = &mut o.flight.pilot;
        p.landing.planet = Some(p.planet.index);
        p.landing.phase = LandingPhase::Settling;
        p.landing.supported_feet = 1;
        p.landing.lateral_speed = if moving { 5.0 } else { 0.0 };
        p.transfer = TransferResult::ShipNotSettled;
        task.step(&o);
        assert_eq!(task.telemetry().scuttle_attempts, 0);
        o.flight.pilot.tick += 901;
        let action = task.step(&o).controls;
        assert_eq!(
            action.primary_held && action.interact_held && action.brake_held,
            !moving
        );
        if moving {
            o.flight.pilot.tick += 901;
            task.step(&o);
            assert_eq!(task.telemetry().status, TaskStatus::Blocked);
            assert_eq!(task.telemetry().scuttle_attempts, 0);
        } else {
            let mut copy = task.clone();
            o.flight.pilot.tick += 1;
            assert_eq!(task.step(&o), copy.step(&o));
            assert!(
                task.step(&o).controls.brake_held,
                "visible but unsettled hatch must not cancel"
            );
            o.flight.pilot.tick += 1;
            o.flight.pilot.supported_planet = None;
            assert_eq!(task.step(&o), FlightIntent::default());
            o.flight.pilot.tick += 1;
            o.flight.pilot.supported_planet = Some(o.flight.pilot.planet.index);
            o.flight.pilot.landing.phase = LandingPhase::Landed;
            o.flight.pilot.landing.supported_feet = 2;
            o.flight.pilot.transfer = TransferResult::Ready;
            assert_eq!(
                task.step(&o),
                FlightIntent::default(),
                "release cancelled chord"
            );
            o.flight.pilot.tick += 1;
            let action = task.step(&o).controls;
            assert!(action.interact_held && !action.primary_held && !action.brake_held);
            assert!(task.telemetry().scuttled_tick.is_none());
        }
    }
}
