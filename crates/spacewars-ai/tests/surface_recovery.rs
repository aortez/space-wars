use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        PilotLocation, SurfaceSortieAction, SurfaceSortieScenario, SurfaceSortieState,
        impact::{RecoveryDisruption, SurfaceImpactAction},
    },
};
use spacewars_ai::{
    BrainReset,
    flight_pilot::FlightIntent,
    recovery_pilot::RulePilotV3,
    recovery_task::{RecoverShipTask, TaskStatus},
};
use std::time::Duration;
const DT: Duration = Duration::from_nanos(16_666_667);

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
        for _ in 0..120 * 60 {
            let o = s.recovery_task_observation(0, task.site_request());
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
            "{disruption:?}: {:?}",
            task.telemetry()
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
