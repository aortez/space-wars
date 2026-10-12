use super::*;
use crate::mission_policy::{MissionBot, MissionPolicy};
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{LandingPhase, SurfaceSortieScenario};
use std::time::Duration;

fn fixture(enabled: bool) -> (MaterialMissionPilot, MissionObservationV1) {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let mut o = state.mission_observation(0, None);
    o.match_rules = true;
    o.sun = None;
    o.boundary.radius = 5000.0;
    for (index, planet) in o.planets.iter_mut().enumerate() {
        planet.motion.position = Vec2::new(index as f32 * 2000.0, 0.0);
        planet.motion.velocity = Vec2::ZERO;
        planet.motion.spin = 0.0;
        planet.radius = 60.0;
        planet.claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_1);
    }
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick = 2000;
    p.planet = o.planets[0].clone();
    p.ship.position = Vec2::new(0.0, 160.0);
    p.ship.velocity = Vec2::ZERO;
    p.ship.angle = 0.0;
    p.ship.spin = 0.0;
    p.gravity = Vec2::ZERO;
    p.controls_armed = true;
    p.queries_ready = true;
    p.landing.altitude = 100.0;
    p.landing.supported_feet = 0;
    p.landing.phase = LandingPhase::Flying;
    let target = o.local.combat.target.as_mut().unwrap();
    target.motion.position = p.ship.position + Vec2::Y * 100.0;
    target.motion.velocity = Vec2::ZERO;
    target.visible = true;
    target.ground_occluded = false;
    o.opponent.as_mut().unwrap().motion = target.motion;
    let mut bot = MaterialMissionPilot::with_policy(
        BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        },
        CombatBreakSettings {
            interval_seconds: 0,
            ..Default::default()
        },
        MissionPolicy::ValuePlanner,
    );
    bot.configure_hunt_clearance_handoff(enabled);
    bot.telemetry.goal = MissionGoal::Hunt;
    bot.pursuit_climb = Some(0);
    (bot, o)
}

fn expire(bot: &mut MaterialMissionPilot, o: &MissionObservationV1) {
    let tick = o.local.combat.recovery.flight.pilot.tick;
    bot.telemetry.pursuit = Some(MissionPursuit {
        started_tick: tick - PURSUIT_BUDGET_TICKS,
        last_visible_tick: tick,
        reason: "nearby opponent after securing ground",
    });
}

#[test]
fn pursuit_expiry_and_restart_keep_an_unfinished_hunt_climb() {
    for restarting in [false, true] {
        let (mut control, o) = fixture(false);
        let (mut candidate, _) = fixture(true);
        if !restarting {
            expire(&mut control, &o);
            expire(&mut candidate, &o);
        }
        let before = o.clone();
        control.intent(&o);
        let mut copy = candidate.clone();
        let result = candidate.intent(&o);
        assert_eq!(result, copy.intent(&o));
        assert_eq!(o, before);
        assert_eq!(control.pursuit_climb, None);
        assert_eq!(candidate.pursuit_climb, Some(0));
        assert_eq!(
            candidate.telemetry.reason,
            Some("climbing for a firing pass")
        );
        assert_eq!(result.weapons, Default::default());
        assert_eq!(candidate.telemetry.pursuit, control.telemetry.pursuit);
        assert_eq!(candidate.telemetry.events, control.telemetry.events);
        assert_eq!(candidate.next_pursuit_tick, control.next_pursuit_tick);
        let cached = candidate.telemetry.clone();
        assert_eq!(candidate.intent(&o), result);
        assert_eq!(candidate.telemetry, cached);
    }
}

#[test]
fn hunt_handoff_retains_normal_clearance_and_range_release() {
    for release_by_range in [false, true] {
        let (mut bot, mut o) = fixture(true);
        expire(&mut bot, &o);
        if release_by_range {
            let position = o.local.combat.recovery.flight.pilot.ship.position + Vec2::Y * 350.0;
            o.opponent.as_mut().unwrap().motion.position = position;
            o.local.combat.target.as_mut().unwrap().motion.position = position;
        } else {
            o.local.combat.recovery.flight.pilot.ship.position.y = 201.0;
        }
        bot.intent(&o);
        assert_eq!(bot.pursuit_climb, None);
        assert_ne!(bot.telemetry.reason, Some("climbing for a firing pass"));
    }
}

#[test]
fn a_real_destination_change_does_not_restore_the_hunt_climb() {
    let (mut control, mut o) = fixture(false);
    let (mut candidate, _) = fixture(true);
    o.planets[1].claim.as_mut().unwrap().owner = None;
    expire(&mut control, &o);
    expire(&mut candidate, &o);
    assert_eq!(candidate.intent(&o), control.intent(&o));
    assert!(matches!(
        candidate.telemetry.goal,
        MissionGoal::Launch | MissionGoal::Transfer
    ));
    assert_eq!(candidate.pursuit_climb, None);
    assert_eq!(candidate.telemetry.target, Some(1));
}

#[test]
fn hunt_handoff_selection_survives_reset_without_preserving_a_stale_climb() {
    let context = BrainReset {
        actor: PlayerId::PLAYER_1,
        episode_seed: 42,
    };
    let mut bot = MissionBot::new(MissionPolicy::ValuePlanner, context, Default::default());
    assert!(bot.telemetry.hunt_clearance_handoff.is_none());
    assert!(
        serde_json::to_value(bot.telemetry())
            .unwrap()
            .get("hunt_clearance_handoff")
            .is_none()
    );
    bot = bot.with_hunt_clearance_handoff(true);
    bot.pursuit_climb = Some(0);
    bot.reset(context);
    assert_eq!(
        bot.telemetry.hunt_clearance_handoff,
        Some(HUNT_CLEARANCE_HANDOFF_PROFILE)
    );
    assert_eq!(bot.pursuit_climb, None);
    assert_eq!(
        bot.clone().telemetry.hunt_clearance_handoff,
        bot.telemetry.hunt_clearance_handoff
    );
}
