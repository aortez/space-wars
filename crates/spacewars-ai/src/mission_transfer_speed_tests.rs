use super::*;

fn bound(normal: Vec2, minimum: f32) -> TransferSpeedLimit {
    TransferSpeedLimit {
        planet: 0,
        normal,
        clearance: 0.0,
        closing_speed: 0.0,
        turn_seconds: 0.0,
        deceleration: 20.0,
        maximum_closing_speed: 0.0,
        minimum_world_normal_speed: minimum,
    }
}

fn fixture(enabled: bool) -> (MaterialMissionPilot, MissionObservationV1) {
    let (mut bot, mut o) = super::super::escape_travel::tests::fixture(true);
    bot.configure_transfer_approach(true);
    bot.configure_transfer_speed(enabled);
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
fn one_constraint_removes_only_the_excess_inward_component() {
    let (v, checked) = project(Vec2::new(-40.0, 7.0), &[bound(Vec2::X, 10.0)]);
    assert_eq!(v, Some(Vec2::new(10.0, 7.0)));
    assert_eq!(checked, 2);
    assert_eq!(
        project(Vec2::new(20.0, 7.0), &[bound(Vec2::X, 10.0)]),
        (Some(Vec2::new(20.0, 7.0)), 1)
    );
}

#[test]
fn intersecting_constraints_are_solved_together_and_not_by_ordered_clamps() {
    let a = bound(Vec2::X, 5.0);
    let b = bound(Vec2::Y, 8.0);
    for constraints in [[a, b], [b, a]] {
        let (v, checked) = project(Vec2::new(-20.0, -20.0), &constraints);
        assert_eq!(v, Some(Vec2::new(5.0, 8.0)));
        assert_eq!(checked, 4);
    }
}

#[test]
fn infeasible_or_excessive_changes_cannot_emit_an_unbounded_velocity() {
    for constraints in [
        vec![bound(Vec2::X, 20.0), bound(-Vec2::X, 20.0)],
        vec![bound(Vec2::X, 100.0)],
        vec![bound(Vec2::ZERO, 0.0)],
    ] {
        assert!(project(Vec2::ZERO, &constraints).0.is_none());
    }
}

#[test]
fn speed_reserve_uses_relative_motion_turn_time_and_inward_gravity() {
    let (_, mut o) = fixture(true);
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.ship.velocity = Vec2::new(60.0, -10.0);
    p.gravity = -Vec2::Y * 5.0;
    let first = limits(&o, 1)[0];
    assert_eq!(first.planet, 0);
    let room = (first.clearance - 70.0).max(0.0);
    let speed = first.maximum_closing_speed;
    assert!(
        (speed * first.turn_seconds + speed * speed / (2.0 * first.deceleration) - room).abs()
            < 0.001
    );
    let boost = Vec2::new(40.0, -20.0);
    o.local.combat.recovery.flight.pilot.ship.velocity += boost;
    for planet in &mut o.planets {
        planet.motion.velocity += boost;
    }
    let moving = limits(&o, 1)[0];
    assert!((moving.closing_speed - first.closing_speed).abs() < 0.001);
    assert_eq!(moving.maximum_closing_speed, first.maximum_closing_speed);
    assert!(
        (moving.minimum_world_normal_speed
            - first.minimum_world_normal_speed
            - boost.dot(first.normal))
        .abs()
            < 0.001
    );
}

#[test]
fn a_nonlocal_neighbor_can_request_braking_before_the_frame_changes() {
    let (mut bot, mut o) = fixture(true);
    bot.intent(&o);
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick += 1;
    p.planet = o.planets[1].clone();
    p.ship.velocity = Vec2::new(60.0, -10.0);
    let action = bot.intent(&o);
    let s = bot
        .telemetry
        .transfer_speed
        .as_ref()
        .unwrap()
        .last
        .as_ref()
        .unwrap();
    assert_eq!(bot.telemetry.goal, MissionGoal::Transfer);
    assert_eq!(s.destination, 1);
    assert_eq!(s.limits.len(), 1);
    assert_eq!(s.limits[0].planet, 0);
    assert!(s.force_brake && action.flight.controls.brake_held);
    assert!(!action.flight.controls.interact_held && !action.flight.wings.closed);
    assert_eq!(
        (s.started_tick, s.selected_tick, s.deadline_tick),
        (920, 920, 4520)
    );
}

#[test]
fn local_climb_and_neutral_capture_handoff_keep_priority() {
    let (mut bot, mut o) = fixture(true);
    let mut disabled = bot.clone();
    disabled.configure_transfer_speed(false);
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.ship.position = p.planet.motion.position + Vec2::Y * 75.0;
    assert_eq!(bot.intent(&o), disabled.intent(&o));
    assert_eq!(bot.telemetry.goal, MissionGoal::Launch);
    assert_eq!(
        bot.telemetry
            .transfer_speed
            .as_ref()
            .unwrap()
            .evaluated_ticks,
        0
    );
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick += 1;
    p.planet = o.planets[1].clone();
    p.ship.position = Vec2::Y * 130.0;
    p.ship.velocity = Vec2::ZERO;
    assert_eq!(bot.intent(&o), CombatIntent::default());
    assert!(bot.capture.is_some());
    assert_eq!(
        bot.telemetry
            .transfer_speed
            .as_ref()
            .unwrap()
            .evaluated_ticks,
        0
    );
}

#[test]
fn unconstrained_transfer_keeps_the_existing_controls_exact() {
    let (mut bot, mut o) = fixture(true);
    o.planets[0].motion.position = Vec2::new(-1500.0, -1500.0);
    o.local.combat.recovery.flight.pilot.planet = o.planets[0].clone();
    let mut baseline = bot.clone();
    baseline.configure_transfer_speed(false);
    assert_eq!(bot.intent(&o), baseline.intent(&o));
    let s = bot.telemetry.transfer_speed.as_ref().unwrap();
    assert_eq!(
        (s.evaluated_ticks, s.limited_ticks, s.braking_ticks),
        (1, 0, 0)
    );
}

#[test]
fn duplicate_ticks_clone_and_reset_preserve_bounded_state() {
    let (mut bot, mut o) = fixture(true);
    bot.intent(&o);
    let mut copy = bot.clone();
    o.local.combat.recovery.flight.pilot.tick += 1;
    let action = bot.intent(&o);
    assert_eq!(copy.intent(&o), action);
    assert_eq!(copy.telemetry, bot.telemetry);
    let state = bot.telemetry.clone();
    assert_eq!(bot.intent(&o), action);
    assert_eq!(bot.telemetry, state);
    bot.reset(bot.context);
    assert_eq!(bot.telemetry.transfer_speed, Some(TransferSpeed::default()));
}

#[test]
fn expired_or_unready_transfers_cannot_refresh_speed_guidance() {
    for change in 0..5 {
        let (mut bot, mut o) = fixture(true);
        bot.intent(&o);
        let state = bot.telemetry.transfer_speed.clone();
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick += 1;
        match change {
            0 => p.tick = 4520,
            1 => p.queries_ready = false,
            2 => p.controls_armed = false,
            3 => p.ship_form = ShipForm::EscapePod,
            _ => o.local.combat.recovery.flight.flight.enabled = false,
        }
        bot.intent(&o);
        assert_eq!(bot.telemetry.transfer_speed, state, "change {change}");
    }
}
