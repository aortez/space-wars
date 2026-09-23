use super::*;
use engine_water::{Boundary, PoolSpec, WaterConfig};

fn floating_player(direction: f32) -> (DuckEvent, WaterWorld) {
    let mut duck = DuckEvent::new_player(Layout::new(4.0 / 3.0), 42, None, 1, 1);
    duck.direction = direction;
    for _ in 0..90 {
        duck.step();
    }
    let water = deep_pool(&duck);
    for _ in 0..600 {
        duck.step_with_water(Some(&water));
    }
    (duck, water)
}

fn deep_pool(duck: &DuckEvent) -> WaterWorld {
    let half = duck.width * 0.5;
    let mut water = WaterWorld::new(
        WaterConfig::default(),
        vec![PoolSpec {
            left: f64::from(-half),
            column_width: f64::from(duck.width) / 128.0,
            bed: vec![f64::from(duck.layout.floor_y); 128],
            boundaries: [Boundary::Closed; 2],
        }],
    )
    .unwrap();
    for column in 0..128 {
        water
            .add_to_pool(
                0,
                f64::from(-half) + (column as f64 + 0.5) * f64::from(duck.width) / 128.0,
                f64::from(duck.radius * 4.0 * duck.width) / 128.0,
            )
            .unwrap();
    }
    water
}

#[test]
fn automatic_duck_paddles_without_learning_from_water_then_resumes_dry_jumps() {
    for direction in [-1.0, 1.0] {
        for profile in [
            engine_common::ClockDuckJumpProfile::Careful,
            engine_common::ClockDuckJumpProfile::Flowing,
        ] {
            let mut duck = DuckEvent::new_platforms(Layout::new(4.0 / 3.0), 42);
            duck.direction = direction;
            duck.select_jump_profile(Some(profile));
            // Interrupt the very first airborne warm-up rather than resetting
            // the body or inventing already-calibrated controller observations.
            while duck.jumps == 0 {
                assert!(duck.tick < 100);
                duck.step();
            }
            let water = deep_pool(&duck);
            let ledger = water.stats();
            let jumps = duck.jumps;
            for _ in 0..480 {
                duck.step_with_water(Some(&water));
                let stats = duck.diagnostics();
                let nav = stats.navigation.unwrap();
                assert_eq!(stats.outcome, None, "{stats:?}");
                assert_eq!(stats.jumps, jumps, "no water or buffered jump");
                assert_eq!(nav.calibrated_jumps, 0, "wet warm-up is discarded");
                assert_eq!(nav.speed_samples, 0);
                assert!(nav.planning.unwrap().plan.is_none());
            }
            let floating = duck.diagnostics();
            assert!(!floating.grounded);
            assert!(
                (420..=480).contains(&floating.visit.unwrap().submerged_milli),
                "{floating:?}"
            );
            assert_eq!(
                floating.navigation.unwrap().behavior,
                engine_common::ClockDuckBehavior::Paddling
            );
            assert_eq!(floating.navigation.unwrap().water.interruptions, 1);
            assert!(floating.navigation.unwrap().water.paddling_ticks > 400);
            assert_eq!(
                water.stats(),
                ledger,
                "the actor never creates/removes water"
            );

            let mut saw_recovery = false;
            let mut resumed = false;
            for _ in 0..600 {
                duck.step(); // the water event leaves; gravity/support are still real
                let stats = duck.diagnostics();
                let nav = stats.navigation.unwrap();
                assert_eq!(stats.outcome, None, "{stats:?}");
                saw_recovery |= nav.behavior == engine_common::ClockDuckBehavior::Recovering;
                if nav.water.recoveries == 1
                    && nav.calibrated_jumps == 2
                    && nav.speed_samples == 9
                    && nav.planning.unwrap().confirmed_landings > 0
                {
                    resumed = true;
                    break;
                }
            }
            assert!(
                saw_recovery && resumed,
                "profile={profile:?}, direction={direction}: {:?}",
                duck.diagnostics()
            );
        }
    }
}

#[test]
fn player_floats_paddles_in_screen_coordinates_and_cannot_jump_from_water() {
    for direction in [-1.0, 1.0] {
        let (mut duck, water) = floating_player(direction);
        let current = duck.player_diagnostics().unwrap();
        assert!(!current.duck.grounded);
        assert!(
            (420..=480).contains(&current.submerged_milli),
            "{current:?}"
        );
        assert!(current.velocity_milli.unwrap()[1].abs() < 100);
        let start = current.duck.position_milli.unwrap();
        let ledger = water.stats();
        duck.set_player_input(1000, true);
        for _ in 0..45 {
            duck.step_with_water(Some(&water));
        }
        let current = duck.player_diagnostics().unwrap();
        assert_eq!(current.duck.jumps, 0, "no air/swim jump or auto hop");
        assert!(
            current.duck.position_milli.unwrap()[0] > start[0] + 2000,
            "{current:?}"
        );
        duck.set_player_input(-1000, false);
        for _ in 0..90 {
            duck.step_with_water(Some(&water));
        }
        assert!(duck.player_diagnostics().unwrap().velocity_milli.unwrap()[0] < -1000);
        assert_eq!(water.stats(), ledger, "one-way forces never create water");
        assert!(duck.physics_counts().0 <= 9);
    }
}

#[test]
fn neutral_player_moves_with_real_water_flow_instead_of_braking_it_away() {
    let (mut duck, mut water) = floating_player(1.0);
    let y = duck
        .world
        .as_ref()
        .unwrap()
        .motion(DUCK_BODY)
        .unwrap()
        .position
        .y;
    duck.world
        .as_mut()
        .unwrap()
        .set_pose(DUCK_BODY, Vec2::new(-100.0, y), 0.0, true);
    // A water-height gradient creates the current through the real pool solver.
    for column in 0..64 {
        let c = water.pools()[0].columns().nth(column).unwrap();
        water
            .add_to_pool(0, c.left + c.width * 0.5, c.width * 10.0)
            .unwrap();
    }
    let start = duck
        .world
        .as_ref()
        .unwrap()
        .motion(DUCK_BODY)
        .unwrap()
        .position
        .x;
    let mut force = 0.0_f32;
    for _ in 0..120 {
        water.step(f64::from(DT)).unwrap();
        duck.step_with_water(Some(&water));
        force = force.max(duck.water_report.force.x);
    }
    let current = duck.player_diagnostics().unwrap();
    assert_eq!(current.move_milli, 0);
    assert!(force > 1.0);
    let end = current.duck.position_milli.unwrap()[0] as f32 / 1000.0;
    assert!(
        end > start + 2.0,
        "current should carry a neutral duck: {start} -> {end}"
    );
}

#[test]
fn leaving_water_restores_dry_controls_and_does_not_leave_a_stale_lift_force() {
    let (mut duck, water) = floating_player(1.0);
    let start = duck
        .player_diagnostics()
        .unwrap()
        .duck
        .position_milli
        .unwrap()[1];
    assert!(duck.water_report.submerged_fraction > 0.0);
    for _ in 0..60 {
        duck.step();
    }
    let current = duck.player_diagnostics().unwrap();
    assert_eq!(current.submerged_milli, 0);
    assert!(current.duck.grounded);
    assert!(current.duck.position_milli.unwrap()[1] < start - 10000);
    duck.set_player_input(0, true);
    duck.step();
    assert_eq!(duck.player_diagnostics().unwrap().duck.jumps, 1);
    assert!(water.stats().pooled > 0.0);
}
