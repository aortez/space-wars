use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{SurfaceSortieScenario, jetpack::CrossingDirection},
};
use spacewars_ai::{
    BrainReset,
    jetpack_crossing::{CrossingGoal, JetpackCrossingPilot},
};
use std::time::Duration;

#[test]
fn crossing_replay_clone_reset_and_identity_are_bounded() {
    let mut state = SurfaceSortieScenario::init_material_jetpack(42, 2);
    let context = BrainReset {
        actor: PlayerId::PLAYER_1,
        episode_seed: 42,
    };
    let mut bot = JetpackCrossingPilot::new(context);
    for _ in 0..160 {
        let o = state.jetpack_crossing_observation(0, bot.direction());
        let a = bot.step(&o);
        let telemetry = bot.telemetry().clone();
        assert_eq!(bot.step(&o), a);
        assert_eq!(bot.telemetry(), &telemetry);
        SurfaceSortieScenario::step(
            &mut state,
            &[a.encode(context.actor)],
            Duration::from_nanos(16_666_667),
        );
    }
    assert!(matches!(
        bot.telemetry().goal,
        CrossingGoal::Lift | CrossingGoal::Cross
    ));
    let mut copy_state = state.clone();
    let mut copy = bot.clone();
    for _ in 0..90 {
        let o = state.jetpack_crossing_observation(0, bot.direction());
        let copy_o = copy_state.jetpack_crossing_observation(0, copy.direction());
        assert_eq!(o, copy_o);
        let a = bot.step(&o);
        assert_eq!(a, copy.step(&copy_o));
        assert_eq!(bot.telemetry(), copy.telemetry());
        for s in [&mut state, &mut copy_state] {
            SurfaceSortieScenario::step(
                s,
                &[a.encode(context.actor)],
                Duration::from_nanos(16_666_667),
            );
        }
    }
    let mut changed = state.jetpack_crossing_observation(0, bot.direction());
    changed.surveyed = true;
    changed.plan = bot.telemetry().plan.clone();
    changed.plan.as_mut().unwrap().ship_angle += std::f32::consts::TAU;
    bot.step(&changed);
    assert_ne!(
        bot.telemetry().goal,
        CrossingGoal::Blocked,
        "equivalent wrapped headings do not move the ship"
    );
    changed.pilot.tick += 1;
    changed.pilot.planet.revision += 1;
    assert!(!bot.step(&changed).primary_held);
    assert_eq!(bot.telemetry().goal, CrossingGoal::Blocked);
    bot.reset(context);
    assert_eq!(
        bot.telemetry(),
        JetpackCrossingPilot::new(context).telemetry()
    );
    changed.pilot.owner = PlayerId::PLAYER_2;
    bot.step(&changed);
    assert_eq!(
        bot.telemetry().reason,
        Some("jetpack observation identity/version mismatch")
    );
    bot.reset(context);
    let mut missing = state.jetpack_crossing_observation(0, CrossingDirection::Left);
    missing.charge = None;
    bot.step(&missing);
    assert_eq!(bot.telemetry().goal, CrossingGoal::Blocked);
}

#[test]
fn physical_trial_crosses_both_directions_and_preserves_remaining_charge_on_boarding() {
    for seat in 0..2 {
        let mut state = SurfaceSortieScenario::init_material_jetpack(42, 2);
        let owner = PlayerId::from_index(seat).unwrap();
        let mut bot = JetpackCrossingPilot::new(BrainReset {
            actor: owner,
            episode_seed: 42,
        });
        let mut before_boarding = 1.0;
        for _ in 0..2400 {
            let o = state.jetpack_crossing_observation(seat, bot.direction());
            if o.pilot.location == scenario_spacewars::surface_sortie::PilotLocation::OnFoot {
                before_boarding = o.charge.unwrap();
            }
            let a = bot.step(&o);
            SurfaceSortieScenario::step(
                &mut state,
                &[a.encode(owner)],
                Duration::from_nanos(16_666_667),
            );
            if matches!(
                bot.telemetry().goal,
                CrossingGoal::Complete | CrossingGoal::Blocked
            ) {
                break;
            }
        }
        assert_eq!(
            bot.telemetry().goal,
            CrossingGoal::Complete,
            "{:?}",
            bot.telemetry()
        );
        assert_eq!(bot.telemetry().crossings, 2);
        assert!(bot.telemetry().claimed);
        let aboard = state.jetpack_crossing_observation(seat, bot.direction());
        assert!(aboard.charge.unwrap() > 0.01 && aboard.charge.unwrap() < 0.8);
        assert!((aboard.charge.unwrap() - before_boarding).abs() <= 1.0 / 240.0 + 1e-6);
    }
}
