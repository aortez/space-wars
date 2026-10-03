use super::*;

pub(in crate::mission_pilot) fn fixture(
    enabled: bool,
) -> (MaterialMissionPilot, MissionObservationV1) {
    let (mut bot, mut o) = super::super::capture_escape::tests::armed();
    bot.configure_escape_travel(enabled);
    let c = &mut o.local.combat;
    let p = &mut c.recovery.flight.pilot;
    p.tick = 920;
    p.ship.position = Vec2::new(0.0, 300.0);
    p.ship.velocity = Vec2::ZERO;
    p.gravity = Vec2::ZERO;
    p.landing.phase = LandingPhase::Flying;
    p.landing.supported_feet = 0;
    c.target.as_mut().unwrap().motion.position = p.ship.position + Vec2::X * 150.0;
    c.weapons.last_hit_taken_tick = Some(p.tick);
    (bot, o)
}

fn started() -> (MaterialMissionPilot, MissionObservationV1) {
    let (mut bot, mut o) = fixture(true);
    bot.intent(&o);
    assert!(bot.committing_escape_travel());
    o.local.combat.recovery.flight.pilot.tick += 1;
    (bot, o)
}

#[test]
fn fresh_escape_gets_one_transfer_instead_of_an_immediate_incoming_fire_pursuit() {
    for enabled in [false, true] {
        let (mut bot, o) = fixture(enabled);
        let action = bot.intent(&o);
        assert_eq!(bot.committing_escape_travel(), enabled);
        assert_eq!(bot.telemetry.pursuit.is_none(), enabled);
        assert!(!action.flight.controls.interact_held);
        let escape = bot.telemetry.capture_escape.as_ref().unwrap().last.unwrap();
        assert_eq!(escape.finished_tick, Some(920));
        assert_eq!(escape.deadline_tick, 920);
        if enabled {
            let a = bot.telemetry.escape_travel.as_ref().unwrap().last.unwrap();
            assert_eq!(a.escape, escape);
            assert_eq!(a.destination, bot.telemetry.target);
            assert_eq!(a.selected_tick, Some(920));
            assert_eq!(a.deadline_tick, 4520);
            assert_eq!(a.deferred_pursuit_ticks, 1);
            assert_eq!(a.last_deferred_reason, Some("responding to incoming fire"));
        }
    }
}

#[test]
fn stale_failed_or_physically_unready_handoffs_cannot_arm() {
    for change in 0..10 {
        let (mut bot, mut o) = fixture(true);
        bot.update_capture_escape(&o);
        let p = &mut o.local.combat.recovery.flight.pilot;
        match change {
            0 => p.tick += 1,
            1 => {
                bot.telemetry
                    .capture_escape
                    .as_mut()
                    .unwrap()
                    .last
                    .as_mut()
                    .unwrap()
                    .reason = Some("recovery required")
            }
            2 => p.ship_form = ShipForm::EscapePod,
            3 => p.location = PilotLocation::OnFoot,
            4 => p.controls_armed = false,
            5 => p.queries_ready = false,
            6 => p.landing.supported_feet = 1,
            7 => o.local.combat.recovery.flight.flight.enabled = false,
            8 => bot.telemetry.target = Some(1),
            _ => o.match_rules = false,
        }
        bot.prepare_escape_travel(&o);
        assert!(!bot.committing_escape_travel(), "change {change}");
    }
}

#[test]
fn hits_progress_and_clone_do_not_renew_the_original_transfer_deadline() {
    let (mut bot, mut o) = started();
    let mut copy = bot.clone();
    let action = bot.intent(&o);
    assert_eq!(action, copy.intent(&o));
    assert_eq!(bot.telemetry, copy.telemetry);
    let t = bot.telemetry.clone();
    assert_eq!(action, bot.intent(&o));
    assert_eq!(bot.telemetry, t);
    for tick in [1000, 2000, 4519] {
        o.local.combat.recovery.flight.pilot.tick = tick;
        o.local.combat.weapons.last_hit_taken_tick = Some(tick);
        // Even continuous progress cannot move the absolute transfer limit.
        bot.progress_tick = tick;
        bot.intent(&o);
        assert!(bot.committing_escape_travel() && bot.telemetry.pursuit.is_none());
        assert_eq!(
            bot.telemetry
                .escape_travel
                .as_ref()
                .unwrap()
                .last
                .unwrap()
                .deadline_tick,
            4520
        );
    }
    o.local.combat.recovery.flight.pilot.tick = 4520;
    bot.intent(&o);
    assert!(!bot.committing_escape_travel());
    assert_eq!(
        bot.telemetry
            .escape_travel
            .as_ref()
            .unwrap()
            .last
            .unwrap()
            .reason,
        Some("transfer deadline")
    );
    assert!(bot.telemetry.pursuit.is_some());
    bot.reset(bot.context);
    assert_eq!(bot.telemetry.escape_travel, Some(EscapeTravel::default()));
    assert_eq!(bot.telemetry.capture_escape, Some(CaptureEscape::default()));
}

#[test]
fn arrival_returns_to_the_existing_capture_task_with_a_neutral_handoff() {
    let (mut bot, mut o) = started();
    let target = bot.telemetry.target.unwrap();
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.planet = o
        .planets
        .iter()
        .find(|v| v.index == target)
        .unwrap()
        .clone();
    p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 90.0);
    p.ship.velocity = p.planet.motion.velocity;
    let action = bot.intent(&o);
    assert_eq!(action, CombatIntent::default());
    assert!(bot.capture.is_some());
    let s = bot.telemetry.escape_travel.as_ref().unwrap();
    assert_eq!(s.capture_handoffs, 1);
    assert_eq!(s.last.unwrap().reason, Some("capture task started"));
    assert_eq!(s.last.unwrap().finished_tick, Some(921));
    assert_eq!(s.last.unwrap().deadline_tick, 4520);
}

#[test]
fn changed_owned_missing_or_stalled_destinations_end_without_rearming() {
    for change in 0..4 {
        let (mut bot, mut o) = started();
        let target = bot.telemetry.target.unwrap();
        match change {
            0 => bot.selected_tick += 1,
            1 => {
                o.planets
                    .iter_mut()
                    .find(|v| v.index == target)
                    .unwrap()
                    .claim
                    .as_mut()
                    .unwrap()
                    .owner = Some(PlayerId::PLAYER_1)
            }
            2 => o.planets.retain(|v| v.index != target),
            _ => {
                o.local.combat.recovery.flight.pilot.tick = 2121;
                o.local.combat.weapons.last_hit_taken_tick = Some(2121);
            }
        }
        bot.intent(&o);
        let s = bot.telemetry.escape_travel.as_ref().unwrap();
        assert_eq!(s.attempts, 1);
        assert!(s.last.unwrap().finished_tick.is_some());
        o.local.combat.recovery.flight.pilot.tick += 1;
        bot.intent(&o);
        assert_eq!(bot.telemetry.escape_travel.as_ref().unwrap().attempts, 1);
    }
}

#[test]
fn safety_and_unavailable_controls_preserve_priority_and_the_clock() {
    for change in 0..5 {
        let (mut bot, mut o) = started();
        match change {
            0 => {
                o.sun = Some(
                    scenario_spacewars::surface_sortie::mission::MissionObstacle {
                        position: Vec2::new(0.0, 260.0),
                        radius: 30.0,
                    },
                )
            }
            1 => o.local.combat.recovery.flight.pilot.ship_available = false,
            2 => o.local.combat.recovery.flight.pilot.controls_armed = false,
            3 => o.local.combat.recovery.flight.pilot.queries_ready = false,
            _ => o.local.combat.recovery.flight.flight.enabled = false,
        }
        let before = bot
            .telemetry
            .escape_travel
            .as_ref()
            .unwrap()
            .last
            .unwrap()
            .travel_ticks;
        let action = bot.intent(&o);
        let a = bot.telemetry.escape_travel.as_ref().unwrap().last.unwrap();
        assert_eq!(a.deadline_tick, 4520);
        assert_eq!(a.travel_ticks, before);
        assert_eq!(action.weapons, Default::default());
        if change == 0 {
            assert_eq!(bot.telemetry.goal, MissionGoal::AvoidSun);
        }
        if change == 1 {
            assert_eq!(a.reason, Some("recovery required"));
        }
        if change == 2 {
            assert_eq!(action, CombatIntent::default());
        }
    }
}

#[test]
fn defensive_weapons_preserve_native_gates_and_the_existing_transfer_motor() {
    let (bot, mut o) = started();
    let c = &mut o.local.combat;
    let p = &c.recovery.flight.pilot;
    c.target.as_mut().unwrap().motion.position = p.ship.position + Vec2::Y * 100.0;
    c.laser_available = true;
    c.cannon_ready = true;
    let mut candidate = bot.clone();
    let fire = candidate.intent(&o);
    assert!(fire.weapons.laser && fire.weapons.cannon);
    let mut control = bot.clone();
    control.telemetry.escape_travel = None;
    // The pre-existing offline deferral isolates the unchanged flight motor.
    let ordinary = control.intent_with_inputs(&o, None, None, None, true);
    assert_eq!(fire.flight, ordinary.flight);
    for change in 0..4 {
        let mut branch = bot.clone();
        let mut observed = o.clone();
        let c = &mut observed.local.combat;
        match change {
            0 => {
                c.laser_available = false;
                c.cannon_ready = false;
            }
            1 => c.target.as_mut().unwrap().visible = false,
            2 => c.target.as_mut().unwrap().ground_occluded = true,
            _ => c.target.as_mut().unwrap().motion.position += Vec2::X * 100.0,
        }
        let action = branch.intent(&observed);
        assert_eq!(action.flight, ordinary.flight);
        assert_eq!(action.weapons, Default::default());
    }
}
