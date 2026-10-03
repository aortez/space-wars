use super::*;
use crate::mission_policy::{MissionBot, MissionPolicy};
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{LandingPhase, SurfaceSortieScenario};
use std::time::Duration;

fn fixture(enabled: bool) -> (MissionBot, MissionObservationV1) {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let mut o = state.mission_observation(0, None);
    o.match_rules = true;
    o.sun = None;
    o.boundary.radius = 5000.0;
    for (planet, x) in o.planets.iter_mut().zip([0.0, 1000.0]) {
        planet.motion.position = Vec2::new(x, 0.0);
        planet.motion.velocity = Vec2::ZERO;
        planet.radius = 60.0;
        planet.claim.as_mut().unwrap().owner = None;
    }
    o.planets[0].claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_1);
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick = 200;
    p.planet = o.planets[0].clone();
    p.ship.position = Vec2::new(500.0, 500.0);
    p.ship.velocity = Vec2::ZERO;
    p.ship.angle = 0.0;
    p.ship.spin = 0.0;
    p.ship_health = 54.0;
    p.gravity = Vec2::ZERO;
    p.controls_armed = true;
    p.landing.altitude = 400.0;
    p.landing.supported_feet = 0;
    p.landing.phase = LandingPhase::Flying;
    let target = o.local.combat.target.as_mut().unwrap();
    target.motion.position = Vec2::new(650.0, 500.0);
    target.motion.velocity = Vec2::ZERO;
    target.ship_form = Some(ShipForm::Ship);
    target.health = 94.0;
    target.health_fraction = 0.94;
    target.visible = true;
    target.ground_occluded = false;
    o.opponent.as_mut().unwrap().motion = target.motion;
    o.local.combat.weapons.last_hit_taken_tick = None;
    let bot = MissionBot::new(
        MissionPolicy::ValuePlanner,
        BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        },
        Default::default(),
    )
    .with_pursuit_health(enabled);
    (bot, o)
}

#[test]
fn disadvantaged_discretionary_pursuit_is_opt_in_and_continues_surface_selection() {
    let (mut baseline, o) = fixture(false);
    let before = o.clone();
    baseline.intent(&o);
    assert_eq!(baseline.telemetry().goal, MissionGoal::Hunt);
    assert!(baseline.telemetry().pursuit_health.is_none());
    assert!(
        serde_json::to_value(baseline.telemetry())
            .unwrap()
            .get("pursuit_health")
            .is_none()
    );
    let (mut guarded, _) = fixture(true);
    guarded.intent(&o);
    assert!(guarded.telemetry().pursuit.is_none());
    assert_eq!(guarded.telemetry().target, Some(1));
    assert!(matches!(
        guarded.telemetry().goal,
        MissionGoal::Launch | MissionGoal::Transfer
    ));
    let gate = guarded.telemetry().pursuit_health.as_ref().unwrap();
    assert_eq!((gate.checks, gate.admitted, gate.deferred), (1, 0, 1));
    assert_eq!(
        gate.last.unwrap().decision,
        PursuitHealthDecision::WeakerHull
    );
    assert_eq!(o, before);
}

#[test]
fn hull_parity_accepts_equal_or_stronger_ships_and_invalid_hull_stays_unknown() {
    for own in [94.0, 100.0] {
        let (mut bot, mut o) = fixture(true);
        o.local.combat.recovery.flight.pilot.ship_health = own;
        assert!(bot.pursuit_opportunity(&o, false));
        assert_eq!(
            bot.telemetry()
                .pursuit_health
                .as_ref()
                .unwrap()
                .last
                .unwrap()
                .decision,
            PursuitHealthDecision::Admitted
        );
    }
    for (own, other) in [
        (f32::NAN, 94.0),
        (54.0, f32::INFINITY),
        (0.0, 94.0),
        (54.0, -1.0),
    ] {
        let (mut bot, mut o) = fixture(true);
        o.local.combat.recovery.flight.pilot.ship_health = own;
        o.local.combat.target.as_mut().unwrap().health = other;
        assert!(!bot.pursuit_opportunity(&o, false));
        let check = bot
            .telemetry()
            .pursuit_health
            .as_ref()
            .unwrap()
            .last
            .unwrap();
        assert_eq!(check.decision, PursuitHealthDecision::InvalidHull);
        assert_eq!(check.own_hull, own.is_finite().then_some(own));
        assert_eq!(check.opponent_hull, other.is_finite().then_some(other));
    }
}

#[test]
fn refusal_does_not_delay_a_new_defensive_response_or_refresh_retry_clock() {
    let (mut bot, mut o) = fixture(true);
    assert!(!bot.pursuit_opportunity(&o, false));
    assert_eq!(bot.next_pursuit_tick, 0);
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick += 1;
    o.local.combat.weapons.last_hit_taken_tick = Some(p.tick);
    assert!(bot.pursuit_opportunity(&o, false));
    assert_eq!(
        bot.telemetry().pursuit.unwrap().reason,
        "responding to incoming fire"
    );
    assert_eq!(bot.telemetry().pursuit_health.as_ref().unwrap().checks, 1);
}

#[test]
fn vulnerable_targets_and_active_pursuits_keep_their_existing_rules() {
    for form in [None, Some(ShipForm::EscapePod), Some(ShipForm::Ship)] {
        let (mut bot, mut o) = fixture(true);
        o.local.combat.recovery.flight.pilot.ship_health = 1.0;
        let target = o.local.combat.target.as_mut().unwrap();
        target.ship_form = form;
        if form == Some(ShipForm::Ship) {
            target.health = 49.0;
            target.health_fraction = 0.49;
        }
        assert!(bot.pursuit_opportunity(&o, false));
        assert_eq!(
            bot.telemetry().pursuit.unwrap().reason,
            "nearby vulnerable opponent"
        );
        assert_eq!(bot.telemetry().pursuit_health.as_ref().unwrap().checks, 0);
        let start = bot.telemetry().pursuit.unwrap().started_tick;
        o.local.combat.recovery.flight.pilot.tick += 1;
        let target = o.local.combat.target.as_mut().unwrap();
        target.ship_form = Some(ShipForm::Ship);
        target.health = 100.0;
        target.health_fraction = 1.0;
        assert!(bot.pursuit_opportunity(&o, false));
        assert_eq!(bot.telemetry().pursuit.unwrap().started_tick, start);
        assert_eq!(bot.telemetry().pursuit_health.as_ref().unwrap().checks, 0);
        o.local.combat.recovery.flight.pilot.tick = start + PURSUIT_BUDGET_TICKS;
        assert!(!bot.pursuit_opportunity(&o, false));
        assert_eq!(
            bot.next_pursuit_tick,
            start + PURSUIT_BUDGET_TICKS + PURSUIT_RETRY_TICKS
        );
    }
}

#[test]
fn capture_recovery_eligibility_and_external_deferral_precede_the_hull_gate() {
    let (baseline, o) = fixture(true);
    for exclusion in 0..7 {
        let mut bot = baseline.clone();
        let mut observation = o.clone();
        match exclusion {
            0 => bot.capture = Some(bot.new_capture_task(&observation)),
            1 => observation.local.combat.recovery.flight.pilot.location = PilotLocation::OnFoot,
            2 => observation.local.combat.recovery.flight.pilot.ship_form = ShipForm::EscapePod,
            3 => {
                observation
                    .local
                    .combat
                    .recovery
                    .flight
                    .pilot
                    .ship_available = false
            }
            4 => observation.match_rules = false,
            5 => observation.local.combat.target.as_mut().unwrap().visible = false,
            _ => {}
        }
        assert!(!bot.pursuit_opportunity(&observation, exclusion == 6));
        assert_eq!(bot.telemetry().pursuit_health.as_ref().unwrap().checks, 0);
    }
}

#[test]
fn clone_repeat_tick_and_reset_preserve_configuration_without_stale_decisions() {
    let (mut bot, o) = fixture(true);
    let mut copy = bot.clone();
    let intent = bot.intent(&o);
    assert_eq!(copy.intent(&o), intent);
    assert_eq!(copy.telemetry(), bot.telemetry());
    let telemetry = bot.telemetry().clone();
    assert_eq!(bot.intent(&o), intent);
    assert_eq!(bot.telemetry(), &telemetry);
    let context = bot.context;
    bot.reset(context);
    assert_eq!(
        bot.telemetry().pursuit_health,
        Some(PursuitHealthTelemetry::default())
    );
    assert_eq!(bot.intent(&o), intent);
    assert_eq!(bot.telemetry(), &telemetry);
}
