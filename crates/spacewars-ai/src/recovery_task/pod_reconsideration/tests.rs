use super::*;
use engine_common::Scenario;
use scenario_spacewars::{PlayerId, surface_sortie::SurfaceSortieScenario};
use std::time::Duration;

fn blocked(enabled: bool) -> (RecoverShipTask, RecoveryTaskObservationV1) {
    let mut state = SurfaceSortieScenario::init_material(42, 1);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let mut o = state.recovery_task_observation(0, None);
    let p = &mut o.flight.pilot;
    p.controls_armed = true;
    p.queries_ready = true;
    p.ship_available = true;
    p.ship_form = ShipForm::EscapePod;
    p.location = PilotLocation::Aboard(p.vehicle);
    p.ship.position = Vec2::new(0.0, 160.0);
    p.ship.velocity = Vec2::ZERO;
    p.ship.spin = 0.0;
    p.ship.angle = 0.0;
    p.planet.motion.position = Vec2::ZERO;
    p.planet.motion.velocity = Vec2::ZERO;
    p.planet.motion.spin = 0.0;
    p.gravity = Vec2::ZERO;
    p.landing.phase = LandingPhase::Flying;
    p.landing.altitude = 26.0;
    p.landing.assist_strength = 0.0;
    p.recovery.as_mut().unwrap().rebuilds = 2;
    let mut old = o.sites[0];
    old.id = LandingSiteId {
        planet: p.planet.index,
        bearing: 0,
    };
    old.revision = p.planet.revision;
    old.vehicle_position = Vec2::new(0.0, 60.0);
    old.local_position = Vec2::new(0.0, 60.0);
    old.normal = Vec2::Y;
    let mut alternative = old;
    alternative.id.bearing = 1;
    alternative.vehicle_position = Vec2::new(20.0, 100.0);
    alternative.local_position = Vec2::new(20.0, 60.0);
    o.sites = vec![old, alternative];
    let mut task = RecoverShipTask::new(BrainReset {
        actor: p.owner,
        episode_seed: 42,
    });
    task.site = Some(old);
    task.telemetry.site = Some(old.id);
    task.telemetry.started_tick = Some(0);
    task.telemetry.landing_retries = 3;
    task.telemetry.relocations = 2;
    task.telemetry.scuttle_attempts = 1;
    task.telemetry.landing_rejections = vec![PodLandingRejection {
        site: LandingSiteId {
            planet: p.planet.index,
            bearing: 2,
        },
        revision: p.planet.revision,
        local_position: Vec2::new(-60.0, 0.0),
        tick: 7000,
        retry_after_tick: 7900,
    }];
    task.seen_rebuilds = Some((p.vehicle, 2));
    task.stabilized = true;
    task.set_pod_reconsideration(enabled);
    p.tick = 7201;
    p.site_query = LandingSiteQuery::Selected(old.id);
    task.step(&o);
    assert_eq!(task.telemetry.status, TaskStatus::Blocked);
    assert_eq!(task.telemetry.reason, Some(FLIGHT_BUDGET_REASON));
    p_prepare(&mut o, 7202);
    (task, o)
}

fn p_prepare(o: &mut RecoveryTaskObservationV1, tick: u64) {
    o.flight.pilot.tick = tick;
    o.flight.pilot.site_query = LandingSiteQuery::Survey;
}

#[test]
fn alternative_restarts_only_approach_and_preserves_history_and_duplicate_ticks() {
    let (mut task, o) = blocked(true);
    let old = task.telemetry.clone();
    let seen = task.seen_rebuilds;
    assert_eq!(task.site_request(), None);
    let action = task.step(&o);
    let t = &task.telemetry;
    assert_eq!(t.status, TaskStatus::Running);
    assert_eq!(t.started_tick, old.started_tick);
    assert_eq!(t.landing_rejections, old.landing_rejections);
    assert_eq!(t.landing_retries, 3);
    assert_eq!(t.relocations, 2);
    assert_eq!(t.scuttle_attempts, 1);
    assert_eq!(task.seen_rebuilds, seen);
    assert!(t.rebuild_boarding.is_none());
    let review = t.pod_reconsideration.as_ref().unwrap();
    assert_eq!(review.surveys, 1);
    assert_eq!(review.started_tick, Some(7202));
    assert_eq!(review.deadline_tick, Some(7202 + ATTEMPT_TICKS));
    assert_eq!(review.alternative.unwrap().id.bearing, 1);
    let after = task.telemetry.clone();
    assert_eq!(task.step(&o), action);
    assert_eq!(task.telemetry, after);
}

#[test]
fn disabled_profile_and_late_activation_do_not_reopen_expired_review_window() {
    let (mut task, mut o) = blocked(false);
    assert!(task.site_request().is_some());
    task.step(&o);
    assert!(task.telemetry.pod_reconsideration.is_none());
    o.flight.pilot.tick = 9000;
    task.set_pod_reconsideration(true);
    task.step(&o);
    assert_eq!(task.telemetry.status, TaskStatus::Blocked);
    assert_eq!(
        task.telemetry.pod_reconsideration.as_ref().unwrap().surveys,
        0
    );
    assert!(task.site_request().is_some());
}

#[test]
fn current_closer_unrejected_site_is_required_not_a_revision_change() {
    let (template, original) = blocked(true);
    for fault in 0..9 {
        let mut task = template.clone();
        let mut o = original.clone();
        match fault {
            0 => o.sites.truncate(1),
            1 => o.sites[1].revision += 1,
            2 => o.sites[1].id.planet += 1,
            3 => o.sites[1].vehicle_position = Vec2::new(200.0, 0.0),
            4 => o.sites[1].normal = Vec2::ZERO,
            5 => o.sites[1].local_position.x = f32::NAN,
            6 => task.telemetry.landing_rejections.push(PodLandingRejection {
                site: o.sites[1].id,
                revision: o.sites[1].revision,
                local_position: o.sites[1].local_position,
                tick: 7201,
                retry_after_tick: 8101,
            }),
            7 => o.flight.pilot.controls_armed = false,
            _ => o.flight.pilot.location = PilotLocation::OnFoot,
        }
        task.step(&o);
        assert_eq!(task.telemetry.status, TaskStatus::Blocked, "fault {fault}");
        assert!(
            task.telemetry
                .pod_reconsideration
                .as_ref()
                .unwrap()
                .started_tick
                .is_none()
        );
    }
}

#[test]
fn dirty_and_deferred_reads_do_not_consume_a_survey_and_review_work_is_bounded() {
    let (mut task, mut o) = blocked(true);
    o.flight.pilot.queries_ready = false;
    task.step(&o);
    p_prepare(&mut o, 7203);
    o.flight.pilot.queries_ready = true;
    o.flight.pilot.site_query = LandingSiteQuery::Deferred { next_tick: 7204 };
    task.step(&o);
    assert_eq!(
        task.telemetry.pod_reconsideration.as_ref().unwrap().surveys,
        0
    );
    o.sites.truncate(1);
    for n in 0..MAX_REVIEWS {
        p_prepare(&mut o, 7204 + u64::from(n) * REVIEW_INTERVAL);
        task.step(&o);
        assert_eq!(
            task.telemetry.pod_reconsideration.as_ref().unwrap().surveys,
            n + 1
        );
        assert!(
            task.site_request().is_some(),
            "no repeated full survey between review ticks"
        );
    }
    let review = task.telemetry.pod_reconsideration.clone();
    task.set_pod_reconsideration(false);
    task.set_pod_reconsideration(true);
    p_prepare(&mut o, 10000);
    task.step(&o);
    assert_eq!(task.telemetry.pod_reconsideration, review);
    assert!(task.site_request().is_some());
}

#[test]
fn extra_deadline_is_absolute_including_disarmed_ticks_and_cannot_be_renewed() {
    let (mut task, mut o) = blocked(true);
    task.step(&o);
    let review = task.telemetry.pod_reconsideration.clone().unwrap();
    o.flight.pilot.controls_armed = false;
    o.flight.pilot.queries_ready = false;
    p_prepare(&mut o, review.deadline_tick.unwrap());
    task.step(&o);
    assert_eq!(task.telemetry.status, TaskStatus::Running);
    o.flight.pilot.tick += 1;
    task.step(&o);
    assert_eq!(task.telemetry.status, TaskStatus::Blocked);
    assert_eq!(
        task.telemetry.reason,
        Some("reconsidered pod recovery exceeded ninety seconds")
    );
    task.set_pod_reconsideration(false);
    task.set_pod_reconsideration(true);
    o.flight.pilot.tick += 60;
    o.flight.pilot.controls_armed = true;
    o.flight.pilot.queries_ready = true;
    task.step(&o);
    assert_eq!(task.telemetry.status, TaskStatus::Blocked);
    assert_eq!(task.telemetry.pod_reconsideration.as_ref(), Some(&review));
}

#[test]
fn physical_completion_at_extra_deadline_is_accepted_without_rearming() {
    let (mut task, mut o) = blocked(true);
    task.step(&o);
    o.flight.pilot.tick = task
        .telemetry
        .pod_reconsideration
        .as_ref()
        .unwrap()
        .deadline_tick
        .unwrap()
        + 1;
    o.flight.pilot.controls_armed = false;
    o.flight.pilot.ship_form = ShipForm::Ship;
    task.step(&o);
    assert_eq!(task.telemetry.status, TaskStatus::Succeeded);
}

#[test]
fn invalid_observations_stay_terminal_even_with_a_valid_alternative() {
    let (template, original) = blocked(true);
    for fault in 0..6 {
        let mut task = template.clone();
        let mut o = original.clone();
        match fault {
            0 => o.version += 1,
            1 => o.flight.version += 1,
            2 => o.flight.pilot.version += 1,
            3 => o.flight.pilot.owner = PlayerId::PLAYER_2,
            4 => o.flight.pilot.vehicle.0 += 1,
            _ => o.flight.pilot.tick = 7200,
        }
        task.step(&o);
        let reason = task.telemetry.reason;
        task.step(&original);
        assert_eq!(task.telemetry.status, TaskStatus::Blocked);
        assert_eq!(task.telemetry.reason, reason);
        assert!(
            task.telemetry
                .pod_reconsideration
                .as_ref()
                .unwrap()
                .started_tick
                .is_none()
        );
    }
}

#[test]
fn retry_exhaustion_and_other_blocking_reasons_do_not_arm_pod_review() {
    let (template, _) = blocked(false);
    for reason in [
        "no supported claim progress",
        "pod stabilization stopped making progress",
        FLIGHT_BUDGET_REASON,
    ] {
        let mut task = template.clone();
        task.telemetry.reason = Some(reason);
        if reason == FLIGHT_BUDGET_REASON {
            task.telemetry.landing_retries = 4;
        }
        task.set_pod_reconsideration(true);
        assert!(task.telemetry.pod_reconsideration.is_none());
    }
}
