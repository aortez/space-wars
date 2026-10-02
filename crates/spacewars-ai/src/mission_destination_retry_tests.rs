use super::*;
use crate::{
    mission_evaluation::CaptureSelection,
    mission_policy::{MissionBot, MissionPolicy},
};
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{LandingPhase, SurfaceSortieScenario};
use std::time::Duration;

fn fixture() -> (MaterialMissionPilot, MissionObservationV1) {
    let mut state = SurfaceSortieScenario::init_material_arena(42);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let mut o = state.mission_observation(0, None);
    o.sun = None;
    o.opponent = None;
    o.local.combat.target = None;
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick = 10;
    p.controls_armed = true;
    p.location = PilotLocation::Aboard(p.vehicle);
    p.ship_available = true;
    p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 130.0);
    p.ship.velocity = p.planet.motion.velocity;
    p.landing.phase = LandingPhase::Flying;
    p.landing.supported_feet = 0;
    for planet in &mut o.planets {
        planet.claim.as_mut().unwrap().owner = if planet.index == 0 {
            Some(p.owner)
        } else {
            None
        };
        if planet.index != 0 {
            planet.motion.position = p.ship.position + Vec2::X * (planet.index as f32 * 300.0);
        }
    }
    p.planet = o.planets[0].clone();
    let mut bot = MaterialMissionPilot::with_policy(
        BrainReset {
            actor: p.owner,
            episode_seed: 42,
        },
        Default::default(),
        MissionPolicy::ValuePlanner,
    );
    bot.enable_destination_retry(true);
    (bot, o)
}

fn failure(
    bot: &mut MaterialMissionPilot,
    o: &MissionObservationV1,
    planet: usize,
    reason: &'static str,
) {
    let mut source = o.clone();
    source.local.combat.recovery.flight.pilot.planet = o.planets[planet].clone();
    bot.telemetry.target = Some(planet);
    bot.selected_tick = 1;
    let mut c = bot.new_capture_task(&source).telemetry().clone();
    c.sortie.started_tick = Some(2);
    c.sortie.failed_tick = Some(source.local.combat.recovery.flight.pilot.tick);
    c.sortie.failure = Some(reason);
    bot.remember_capture_failure(&source, &c);
    assert_eq!(
        bot.telemetry
            .destination_retry
            .as_ref()
            .unwrap()
            .failures
            .last()
            .unwrap()
            .context
            .planet,
        planet
    );
    bot.telemetry.target = None;
}

fn choice(
    bot: &MaterialMissionPilot,
    o: &MissionObservationV1,
    destination: usize,
) -> CaptureSelection {
    CaptureSelection {
        tick: o.local.combat.recovery.flight.pilot.tick,
        source_tick: o.local.combat.recovery.flight.pilot.tick - 1,
        current: bot.telemetry.target.unwrap(),
        selected_tick: bot.selected_tick,
        site: None,
        destination,
        current_seconds: 60.0,
        destination_seconds: 20.0,
        value: None,
    }
}

#[test]
fn both_selection_paths_prefer_an_alternative_over_the_unchanged_failed_planet() {
    let (mut bot, mut o) = fixture();
    failure(
        &mut bot,
        &o,
        1,
        "capture approach exhausted its time or retry budget",
    );
    bot.deferred.push((1, 1811));
    o.local.combat.recovery.flight.pilot.tick = 1900;
    let mut baseline = bot.clone();
    baseline.enable_destination_retry(false);
    baseline.intent(&o);
    bot.intent(&o);
    assert_eq!(baseline.telemetry.target, Some(1));
    assert_eq!(bot.telemetry.target, Some(2));
    assert_eq!(
        bot.telemetry
            .destination_retry
            .as_ref()
            .unwrap()
            .initial_changes,
        1
    );
    assert_eq!(
        bot.telemetry
            .destination_retry
            .as_ref()
            .unwrap()
            .first_effect_tick,
        Some(1900)
    );
    o.local.combat.recovery.flight.pilot.tick += 1;
    bot.apply_destination_selection(&o, choice(&bot, &o, 1));
    assert_eq!(bot.telemetry.target, Some(2));
    let memory = bot.telemetry.destination_retry.as_ref().unwrap();
    assert_eq!(memory.switch_rejections, 1);
    assert_eq!(
        memory.last_decision.as_ref().unwrap().path,
        DestinationSelectionPath::Switch
    );
    assert_eq!(
        memory.last_decision.as_ref().unwrap().failure.failed_tick,
        10
    );
    assert!(!bot.destination_switched);
}

#[test]
fn the_only_remaining_target_can_retry_after_the_native_cooldown() {
    let (mut bot, mut o) = fixture();
    failure(
        &mut bot,
        &o,
        1,
        "cover search probe budget exhausted with unmeasured candidates",
    );
    o.planets[2].claim.as_mut().unwrap().owner = Some(bot.context.actor);
    bot.deferred.push((1, 1811));
    o.local.combat.recovery.flight.pilot.tick = 1810;
    bot.intent(&o);
    assert_eq!(bot.telemetry.target, None);
    o.local.combat.recovery.flight.pilot.tick += 1;
    bot.intent(&o);
    assert_eq!(bot.telemetry.target, Some(1));
    let memory = bot.telemetry.destination_retry.as_ref().unwrap();
    assert_eq!(memory.first_effect_tick, None);
    assert_eq!(memory.retry_selections, 1);
    assert_eq!(memory.last_retry.as_ref().unwrap().tick, 1811);
    assert_eq!(
        memory.last_retry.as_ref().unwrap().failure.kind,
        DestinationFailureKind::IncompleteSearch
    );
}

#[test]
fn incomplete_evidence_precedes_observed_constraints_then_oldest_failure_retries() {
    let (mut bot, mut o) = fixture();
    failure(
        &mut bot,
        &o,
        1,
        "no complete flag round trip in fresh surveys",
    );
    o.local.combat.recovery.flight.pilot.tick = 11;
    failure(&mut bot, &o, 2, "cover route evidence deadline exhausted");
    assert!(!bot.destination_retry_admitted(&o, 1));
    assert!(bot.destination_retry_admitted(&o, 2));
    o.local.combat.recovery.flight.pilot.tick = 12;
    failure(
        &mut bot,
        &o,
        1,
        "capture approach exhausted its time or retry budget",
    );
    assert!(!bot.destination_retry_admitted(&o, 1));
    assert!(bot.destination_retry_admitted(&o, 2));
    o.local.combat.recovery.flight.pilot.tick = 13;
    failure(
        &mut bot,
        &o,
        2,
        "capture approach exhausted its time or retry budget",
    );
    assert!(bot.destination_retry_admitted(&o, 1));
    assert!(!bot.destination_retry_admitted(&o, 2));
    assert_eq!(
        bot.telemetry
            .destination_retry
            .as_ref()
            .unwrap()
            .failures
            .len(),
        2
    );
}

#[test]
fn owned_or_deferred_alternatives_do_not_prevent_exploration() {
    let (mut bot, mut o) = fixture();
    failure(
        &mut bot,
        &o,
        1,
        "cover search exhausted its observed candidates",
    );
    bot.deferred.push((2, 20));
    assert!(bot.destination_retry_admitted(&o, 1));
    o.local.combat.recovery.flight.pilot.tick = 20;
    assert!(!bot.destination_retry_admitted(&o, 1));
    o.planets[2].claim.as_mut().unwrap().owner = Some(bot.context.actor);
    assert!(bot.destination_retry_admitted(&o, 1));
}

#[test]
fn context_changes_invalidate_memory_but_elapsed_time_does_not() {
    for mutation in 0..5 {
        let (mut bot, mut o) = fixture();
        failure(
            &mut bot,
            &o,
            1,
            "no complete flag round trip in fresh surveys",
        );
        o.local.combat.recovery.flight.pilot.tick += 100_000;
        bot.refresh_destination_failures(&o);
        assert!(!bot.destination_retry_admitted(&o, 1));
        match mutation {
            0 => o.planets[1].revision += 1,
            1 => o.planets[1].claim.as_mut().unwrap().owner = Some(bot.context.actor),
            2 => o.planets.retain(|p| p.index != 1),
            3 => {
                o.planets[1].claim.as_mut().unwrap().flag =
                    Some(scenario_spacewars::surface_sortie::PlanetFlagObservation {
                        player: PlayerId::PLAYER_2,
                        position: Vec2::new(13.0, 17.0),
                        normal: Vec2::Y,
                        raised_fraction: 1.0,
                    })
            }
            4 => o.planets[1].claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_2),
            _ => unreachable!(),
        }
        bot.refresh_destination_failures(&o);
        assert!(
            bot.telemetry
                .destination_retry
                .as_ref()
                .unwrap()
                .failures
                .is_empty(),
            "mutation {mutation}"
        );
        assert_eq!(
            bot.telemetry
                .destination_retry
                .as_ref()
                .unwrap()
                .context_invalidations,
            1
        );
    }
}

#[test]
fn flag_motion_uses_planet_coordinates_and_new_local_identity_releases_memory() {
    let (mut bot, mut o) = fixture();
    let planet = &mut o.planets[1];
    let point = Vec2::new(20.0, 30.0);
    planet.claim.as_mut().unwrap().flag =
        Some(scenario_spacewars::surface_sortie::PlanetFlagObservation {
            player: PlayerId::PLAYER_2,
            position: planet.motion.position + point.rotate_radians(planet.motion.angle),
            normal: Vec2::Y,
            raised_fraction: 1.0,
        });
    failure(
        &mut bot,
        &o,
        1,
        "no complete flag round trip in fresh surveys",
    );
    let planet = &mut o.planets[1];
    planet.motion.position += Vec2::new(10.0, 5.0);
    planet.motion.angle += 0.7;
    planet
        .claim
        .as_mut()
        .unwrap()
        .flag
        .as_mut()
        .unwrap()
        .position = planet.motion.position + point.rotate_radians(planet.motion.angle);
    bot.refresh_destination_failures(&o);
    assert!(!bot.destination_retry_admitted(&o, 1));
    o.planets[1]
        .claim
        .as_mut()
        .unwrap()
        .flag
        .as_mut()
        .unwrap()
        .position += Vec2::X;
    bot.refresh_destination_failures(&o);
    assert!(
        bot.telemetry
            .destination_retry
            .as_ref()
            .unwrap()
            .failures
            .is_empty()
    );
}

#[test]
fn stale_foreign_or_post_claim_failures_cannot_gain_current_context() {
    for mutation in 0..5 {
        let (mut bot, o) = fixture();
        bot.telemetry.target = Some(0);
        bot.selected_tick = 2;
        let mut c = bot.new_capture_task(&o).telemetry().clone();
        c.sortie.started_tick = Some(3);
        c.sortie.failed_tick = Some(10);
        c.sortie.failure = Some("failure");
        match mutation {
            0 => c.sortie.failed_tick = Some(9),
            1 => c.sortie.started_tick = Some(1),
            2 => bot.telemetry.target = Some(1),
            3 => c.sortie.landing.claimed_tick = Some(9),
            4 => c.sortie.failure = None,
            _ => unreachable!(),
        }
        bot.remember_capture_failure(&o, &c);
        assert_eq!(
            bot.telemetry
                .destination_retry
                .as_ref()
                .unwrap()
                .recorded_failures,
            0
        );
    }
}

#[test]
fn safety_and_stale_selection_gates_remain_priorities_without_false_effects() {
    for mutation in 0..4 {
        let (mut bot, mut o) = fixture();
        failure(&mut bot, &o, 1, "failure");
        bot.telemetry.target = Some(2);
        let mut selection = choice(&bot, &o, 1);
        match mutation {
            0 => selection.tick -= 1,
            1 => selection.selected_tick += 1,
            2 => o.local.combat.recovery.flight.pilot.landing.supported_feet = 1,
            3 => bot.deferred.push((1, 100)),
            _ => unreachable!(),
        }
        bot.apply_destination_selection(&o, selection);
        assert_eq!(bot.telemetry.target, Some(2));
        assert_eq!(
            bot.telemetry
                .destination_retry
                .as_ref()
                .unwrap()
                .first_effect_tick,
            None
        );
    }
}

#[test]
fn read_only_probe_keeps_memory_and_duplicate_intent_does_not_record_again() {
    let (mut bot, mut o) = fixture();
    failure(&mut bot, &o, 1, "failure");
    bot.telemetry.target = Some(2);
    let before = bot.telemetry.destination_retry.clone();
    assert_eq!(bot.transfer_probe_gate(&o, 1), Err(RETRY_PREFERENCE));
    assert_eq!(bot.telemetry.destination_retry, before);
    bot.telemetry.target = None;
    o.local.combat.recovery.flight.pilot.tick += 1;
    let intent = bot.intent(&o);
    let after = bot.telemetry.destination_retry.clone();
    assert_eq!(bot.intent(&o), intent);
    assert_eq!(bot.telemetry.destination_retry, after);
}

#[test]
fn recovery_keeps_failures_but_episode_reset_keeps_only_the_option() {
    let (mut bot, o) = fixture();
    failure(&mut bot, &o, 1, "failure");
    let before = bot.telemetry.destination_retry.clone();
    bot.reconsider(11, "ship or surface recovery required", false);
    assert_eq!(bot.telemetry.destination_retry, before);
    bot.reset(bot.context);
    assert_eq!(
        bot.telemetry.destination_retry,
        Some(DestinationRetryTelemetry::default())
    );
    bot.enable_destination_retry(false);
    assert!(
        serde_json::to_value(bot.telemetry())
            .unwrap()
            .get("destination_retry")
            .is_none()
    );
    bot.reset(bot.context);
    assert!(bot.telemetry.destination_retry.is_none());
}

#[test]
fn native_failure_is_recorded_before_reconsideration_drops_the_capture() {
    let (mut bot, mut o) = fixture();
    o.local.combat.recovery.flight.pilot.planet = o.planets[1].clone();
    bot.telemetry.target = Some(1);
    bot.selected_tick = 1;
    bot.capture = Some(bot.new_capture_task(&o));
    bot.intent(&o);
    let started = bot
        .capture
        .as_ref()
        .unwrap()
        .telemetry()
        .started_tick
        .unwrap();
    o.local.combat.recovery.flight.pilot.tick = started + 150 * 60 + 1;
    bot.intent(&o);
    let failed = o.local.combat.recovery.flight.pilot.tick;
    let memory = bot.telemetry.destination_retry.as_ref().unwrap();
    assert_eq!(memory.recorded_failures, 1);
    assert_eq!(memory.failures[0].failed_tick, failed);
    assert_eq!(memory.failures[0].context.planet, 1);
    assert_eq!(
        memory.failures[0].kind,
        DestinationFailureKind::ExecutionLimit
    );
    assert!(bot.capture.is_some());
    o.local.combat.recovery.flight.pilot.tick += 1;
    bot.intent(&o);
    assert!(bot.capture.is_none());
    assert_eq!(bot.deferred, vec![(1, failed + 1 + 1800)]);
    assert_eq!(
        bot.telemetry
            .destination_retry
            .as_ref()
            .unwrap()
            .recorded_failures,
        1
    );
}

#[test]
#[should_panic]
fn experiment_requires_the_value_policy() {
    let _ = MissionBot::new(
        MissionPolicy::Legacy,
        BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 1,
        },
        Default::default(),
    )
    .with_destination_retry(true);
}
