use super::*;
use crate::mission_policy::{MissionBot, MissionPolicy, MissionRecoveryProfile};
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{SurfaceSortieScenario, sensor_profile};
use std::time::Duration;

const DT: Duration = Duration::from_nanos(16_666_667);

fn context() -> BrainReset {
    BrainReset {
        actor: PlayerId::PLAYER_1,
        episode_seed: 42,
    }
}

#[test]
fn disabled_driver_preserves_each_policy_and_its_observation_cadence() {
    for policy in MissionPolicy::ALL {
        let mut original = SurfaceSortieScenario::init_material_match(42);
        let mut candidate = original.clone();
        let mut control = MissionBot::new(policy, context(), Default::default());
        let mut paired = control.clone().with_terrain_recovery(false);
        let evaluation = MissionEvaluator::new(2);
        for _ in 0..32 {
            let observation = original.mission_observation_with_cadence(
                0,
                control.sensor_request(),
                Default::default(),
            );
            let intent = control.intent_with_evaluation(&observation, &evaluation);
            let (other, action) = paired.decide_with_terrain_recovery(
                &mut candidate,
                Default::default(),
                &evaluation,
            );
            assert_eq!(observation, other);
            assert_eq!(intent, action);
            assert_eq!(control.telemetry(), paired.telemetry());
            SurfaceSortieScenario::step(&mut original, &intent.encode(PlayerId::PLAYER_1), DT);
            SurfaceSortieScenario::step(&mut candidate, &action.encode(PlayerId::PLAYER_1), DT);
            assert_eq!(
                SurfaceSortieScenario::observe(&original).payload,
                SurfaceSortieScenario::observe(&candidate).payload
            );
        }
    }
}

#[test]
fn mission_losses_create_configured_tasks_without_renewing_an_existing_task() {
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    for profile in MissionRecoveryProfile::ALL {
        let enabled = profile != MissionRecoveryProfile::Standard;
        let hatch = profile == MissionRecoveryProfile::SupportedHatchV1;
        let mut bot = MissionBot::new(MissionPolicy::ValuePlanner, context(), Default::default())
            .with_recovery_profile(profile);
        for generation in 1..=2 {
            // Observation fixture isolates task ownership; physical recoveries
            // and mission continuation are exercised by the retained matrix.
            let mut observation = state.mission_observation(0, None);
            let p = &mut observation.local.combat.recovery.flight.pilot;
            p.tick = generation * 10;
            p.ship_form = ShipForm::EscapePod;
            p.recovery.as_mut().unwrap().ships_lost = generation;
            bot.intent(&observation);
            let task = bot.recovery.as_ref().unwrap();
            assert_eq!(task.telemetry().started_tick, Some(generation * 10));
            assert_eq!(task.rebuild_search_request().is_some(), enabled);
            if enabled {
                assert!(task.rebuild_search_request().unwrap().include_staging_map);
            }
            observation.local.combat.recovery.flight.pilot.tick += 1;
            bot.intent(&observation);
            assert_eq!(
                bot.recovery.as_ref().unwrap().telemetry().started_tick,
                Some(generation * 10)
            );
        }
        let copy = bot.clone();
        assert_eq!(copy.terrain_recovery, enabled);
        assert_eq!(copy.hatch_supported_arrival, hatch);
        bot.reset(context());
        assert_eq!(bot.terrain_recovery, enabled);
        assert_eq!(bot.hatch_supported_arrival, hatch);
        assert!(bot.recovery.is_none());
    }
}

#[test]
fn completion_clears_native_refinement_before_step_without_touching_the_other_seat() {
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut bot = MissionBot::new(MissionPolicy::ValuePlanner, context(), Default::default())
        .with_terrain_recovery(true);
    bot.recovery = Some(RecoverShipTask::new(context()));
    bot.configure_recovery_task();
    assert!(state.set_rebuild_refinement(1, true));
    let before = SurfaceSortieScenario::observe(&state).payload;
    let ((_, intent), profile) = sensor_profile::measure(|| {
        bot.decide_with_terrain_recovery(&mut state, Default::default(), &MissionEvaluator::new(2))
    });
    assert_eq!(profile.stages["recovery_task_observation"].calls, 1);
    assert_eq!(profile.stages["tactical_sortie_observation"].calls, 1);
    assert_eq!(bot.telemetry().completed_recoveries, 1);
    assert!(bot.recovery.is_none());
    assert_eq!(state.rebuild_refinement_enabled(0), Some(false));
    assert_eq!(state.rebuild_refinement_enabled(1), Some(true));
    assert_eq!(before, SurfaceSortieScenario::observe(&state).payload);
    let (_, repeated) =
        bot.decide_with_terrain_recovery(&mut state, Default::default(), &MissionEvaluator::new(2));
    assert_eq!(intent, repeated);
    assert_eq!(bot.telemetry().completed_recoveries, 1);
}

#[test]
fn enhanced_sensor_keeps_mission_context_and_defers_the_same_landing_survey() {
    let mut state = SurfaceSortieScenario::init_material_match(42);
    for _ in 0..17 {
        SurfaceSortieScenario::step(&mut state, &[], DT);
    }
    let bot = MissionBot::new(MissionPolicy::ValuePlanner, context(), Default::default());
    let pilot = state.pilot_observation(0, None);
    let request = MissionSensorRequest {
        site: None,
        last_survey: Some(LandingSurveyStamp {
            tick: 1,
            planet: pilot.planet.index,
            form: pilot.ship_form,
        }),
        ..bot.sensor_request()
    };
    let ordinary = state.mission_observation_with_cadence(0, request, Default::default());
    let (enhanced, profile) = sensor_profile::measure(|| {
        state.mission_observation_with_rebuild_search(
            0,
            request,
            Default::default(),
            &Default::default(),
        )
    });
    assert!(matches!(
        ordinary.local.combat.recovery.flight.pilot.site_query,
        LandingSiteQuery::Deferred { .. }
    ));
    assert_eq!(ordinary, enhanced);
    assert_eq!(profile.stages["recovery_task_observation"].calls, 1);
}

#[test]
#[should_panic(expected = "configure before the first intent")]
fn selecting_the_profile_mid_task_is_rejected() {
    let state = SurfaceSortieScenario::init_material_combat(42);
    let mut bot = MissionBot::new(MissionPolicy::ValuePlanner, context(), Default::default());
    bot.intent(&state.mission_observation(0, None));
    bot.configure_terrain_recovery(true);
}

#[test]
fn hatch_arrival_selection_preserves_mission_memory_and_survives_reset() {
    let state = SurfaceSortieScenario::init_material_combat(42);
    let mut bot = MissionBot::new(MissionPolicy::ValuePlanner, context(), Default::default());
    let o = state.mission_observation(0, None);
    let action = bot.intent(&o);
    let original = format!("{bot:?}");
    bot.set_hatch_supported_arrival(true);
    assert_eq!(bot.intent(&o), action);
    assert!(bot.clone().hatch_supported_arrival);
    bot.set_hatch_supported_arrival(false);
    assert_eq!(format!("{bot:?}"), original);
    bot.set_hatch_supported_arrival(true);
    bot.reset(context());
    assert!(bot.hatch_supported_arrival);
    assert!(!bot.terrain_recovery, "the options remain independent");
}

#[test]
fn named_profiles_preserve_the_complete_qualified_initial_controllers() {
    for seat in 0..2 {
        let context = BrainReset {
            actor: PlayerId::from_index(seat).unwrap(),
            episode_seed: 20202,
        };
        for profile in MissionRecoveryProfile::ALL {
            let mut explicit =
                MissionBot::new(MissionPolicy::ValuePlanner, context, Default::default())
                    .with_terrain_recovery(profile != MissionRecoveryProfile::Standard);
            explicit
                .set_hatch_supported_arrival(profile == MissionRecoveryProfile::SupportedHatchV1);
            let selected =
                MissionBot::new(MissionPolicy::ValuePlanner, context, Default::default())
                    .with_recovery_profile(profile);
            assert_eq!(format!("{selected:?}"), format!("{explicit:?}"));
            let standard = selected.with_recovery_profile(MissionRecoveryProfile::Standard);
            let ordinary =
                MissionBot::new(MissionPolicy::ValuePlanner, context, Default::default());
            assert_eq!(format!("{standard:?}"), format!("{ordinary:?}"));
        }
    }
}

#[test]
#[should_panic(expected = "configure before the first intent")]
fn named_profile_cannot_replace_configuration_after_a_decision() {
    let state = SurfaceSortieScenario::init_material_combat(42);
    let mut bot = MissionBot::new(MissionPolicy::ValuePlanner, context(), Default::default());
    bot.intent(&state.mission_observation(0, None));
    let _ = bot.with_recovery_profile(MissionRecoveryProfile::SupportedHatchV1);
}
