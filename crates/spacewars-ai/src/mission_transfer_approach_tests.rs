use super::*;

fn fixture(enabled: bool) -> (MaterialMissionPilot, MissionObservationV1) {
    let (mut bot, mut o) = super::super::escape_travel::tests::fixture(true);
    bot.configure_transfer_approach(enabled);
    o.planets[0].motion.position = Vec2::new(-120.0, 0.0);
    o.planets[0].radius = 40.0;
    o.planets[1].motion.position = Vec2::ZERO;
    o.planets[1].radius = 40.0;
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.planet = o.planets[0].clone();
    p.ship.position = Vec2::new(-220.0, 100.0);
    (bot, o)
}

#[test]
fn obstructed_entry_uses_a_clear_bearing_with_the_existing_transfer_clock() {
    let (mut bot, o) = fixture(true);
    let action = bot.intent(&o);
    let s = bot
        .telemetry
        .transfer_approach
        .as_ref()
        .unwrap()
        .last
        .unwrap();
    assert_eq!(s.reason, "selected clear approach bearing");
    assert!(!clear_entry(&o, 1, s.ordinary_entry));
    assert!(clear_entry(&o, 1, s.entry));
    assert!((s.entry.length() - 125.0).abs() < 0.001);
    assert_eq!(s.candidates_checked, 32);
    assert!(s.alternate && s.used_for_transfer);
    assert_eq!(
        (s.started_tick, s.selected_tick, s.deadline_tick),
        (920, 920, 4520)
    );
    assert!(!action.flight.controls.interact_held && bot.capture.is_none());
    assert_eq!(o.local.combat.recovery.flight.pilot.planet.index, 0);
}

#[test]
fn clear_original_entry_and_disabled_policy_preserve_the_existing_motor() {
    let (mut enabled, mut o) = fixture(true);
    o.planets[0].motion.position = Vec2::new(-1500.0, 0.0);
    o.local.combat.recovery.flight.pilot.planet = o.planets[0].clone();
    let mut disabled = enabled.clone();
    disabled.configure_transfer_approach(false);
    assert_eq!(enabled.intent(&o), disabled.intent(&o));
    let s = enabled.telemetry.transfer_approach.as_ref().unwrap();
    assert_eq!(
        (s.evaluated_ticks, s.alternate_ticks, s.guided_ticks),
        (1, 0, 0)
    );
    assert_eq!(s.last.unwrap().entry, s.last.unwrap().ordinary_entry);
    assert!(disabled.telemetry.transfer_approach.is_none());
}

#[test]
fn retained_bearing_tracks_translation_and_rechecks_moving_obstacles() {
    let (mut bot, mut o) = fixture(true);
    bot.intent(&o);
    let before = bot
        .telemetry
        .transfer_approach
        .as_ref()
        .unwrap()
        .last
        .unwrap();
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.planets[1].motion.position += Vec2::new(2.0, 1.0);
    bot.intent(&o);
    let after = bot
        .telemetry
        .transfer_approach
        .as_ref()
        .unwrap()
        .last
        .unwrap();
    assert_eq!(after.reason, "retaining clear approach bearing");
    assert_eq!(after.bearing, before.bearing);
    assert!((after.entry - before.entry - Vec2::new(2.0, 1.0)).length() < 0.001);
    o.local.combat.recovery.flight.pilot.tick += 1;
    let mut obstacle = o.planets[0].clone();
    obstacle.index = 2;
    obstacle.motion.position = after.entry;
    obstacle.radius = 2000.0;
    o.planets.push(obstacle);
    bot.intent(&o);
    let blocked = bot
        .telemetry
        .transfer_approach
        .as_ref()
        .unwrap()
        .last
        .unwrap();
    assert_eq!(blocked.reason, "no clear entry; ordinary guidance retained");
    assert!(!blocked.alternate);
    assert_eq!(blocked.entry, blocked.ordinary_entry);
    assert_eq!(blocked.candidates_checked, 33);
    assert_eq!(blocked.deadline_tick, before.deadline_tick);
}

#[test]
fn native_local_climb_has_priority_over_the_selected_entry() {
    let (mut bot, mut o) = fixture(true);
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.ship.position = p.planet.motion.position + Vec2::Y * 75.0;
    let mut baseline = bot.clone();
    baseline.configure_transfer_approach(false);
    assert_eq!(bot.intent(&o), baseline.intent(&o));
    assert_eq!(bot.telemetry.goal, MissionGoal::Launch);
    let s = bot.telemetry.transfer_approach.as_ref().unwrap();
    assert!(!s.last.unwrap().used_for_transfer);
    assert_eq!(s.guided_ticks, 0);
}

#[test]
fn native_arrival_still_requires_the_destination_frame_and_neutral_handoff() {
    let (mut bot, mut o) = fixture(true);
    bot.intent(&o);
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick += 1;
    p.ship.position = Vec2::new(-120.0, 0.0);
    p.ship.velocity = Vec2::ZERO;
    bot.intent(&o);
    assert!(bot.capture.is_none());
    let last = bot
        .telemetry
        .transfer_approach
        .as_ref()
        .unwrap()
        .last
        .unwrap();
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick += 1;
    p.planet = o.planets[1].clone();
    p.ship.position = Vec2::Y * 130.0;
    assert_eq!(bot.intent(&o), CombatIntent::default());
    assert!(bot.capture.is_some());
    assert_eq!(
        bot.telemetry.transfer_approach.as_ref().unwrap().last,
        Some(last)
    );
    assert_eq!(
        bot.telemetry
            .escape_travel
            .as_ref()
            .unwrap()
            .capture_handoffs,
        1
    );
}

#[test]
fn unavailable_controls_and_finished_travel_cannot_select_an_entry() {
    for change in 0..5 {
        let (mut bot, mut o) = fixture(true);
        bot.intent(&o);
        let before = bot.telemetry.transfer_approach.clone();
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick += 1;
        match change {
            0 => p.controls_armed = false,
            1 => p.queries_ready = false,
            2 => o.local.combat.recovery.flight.flight.enabled = false,
            3 => p.ship_form = ShipForm::EscapePod,
            _ => p.tick = 4520,
        }
        bot.intent(&o);
        assert_eq!(bot.telemetry.transfer_approach, before, "change {change}");
    }
}

#[test]
fn duplicate_ticks_clone_and_reset_keep_selection_deterministic() {
    let (mut bot, mut o) = fixture(true);
    bot.intent(&o);
    let mut copy = bot.clone();
    o.local.combat.recovery.flight.pilot.tick += 1;
    let action = bot.intent(&o);
    assert_eq!(copy.intent(&o), action);
    assert_eq!(copy.telemetry, bot.telemetry);
    let telemetry = bot.telemetry.clone();
    assert_eq!(bot.intent(&o), action);
    assert_eq!(bot.telemetry, telemetry);
    bot.reset(bot.context);
    assert_eq!(
        bot.telemetry.transfer_approach,
        Some(TransferApproach::default())
    );
}

#[test]
fn geometric_screen_keeps_sun_and_arena_margins() {
    let (_, mut o) = fixture(true);
    o.planets.truncate(2);
    let entry = Vec2::new(125.0, 0.0);
    assert!(clear_entry(&o, 1, entry));
    o.boundary.radius = 140.0;
    assert!(!clear_entry(&o, 1, entry));
    o.boundary.radius = 5000.0;
    o.sun = Some(
        scenario_spacewars::surface_sortie::mission::MissionObstacle {
            position: entry + Vec2::X * 120.0,
            radius: 20.0,
        },
    );
    assert!(!clear_entry(&o, 1, entry));
}

#[test]
fn changed_selection_cannot_reuse_a_previous_approach() {
    for change_destination in [false, true] {
        let (mut bot, mut o) = fixture(true);
        bot.intent(&o);
        let before = bot.telemetry.transfer_approach.clone();
        o.local.combat.recovery.flight.pilot.tick += 1;
        let target = if change_destination {
            &o.planets[0]
        } else {
            bot.selected_tick += 1;
            &o.planets[1]
        };
        bot.transfer_entry(&o, target);
        assert_eq!(bot.telemetry.transfer_approach, before);
    }
}
