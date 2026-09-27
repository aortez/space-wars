//! Deterministic land, pool, and low-bank fixtures using the production actor.
use super::*;
use engine_water::{Boundary, PoolSpec, WaterConfig};
use planner::Surface;

fn flat_player(aspect: f32, direction: f32) -> DuckEvent {
    let mut duck = DuckEvent::new_player(Layout::new(aspect), 42, None, 1, 1);
    duck.direction = direction;
    duck.course.as_mut().unwrap().surfaces = vec![Surface {
        start: 0.0,
        end: duck.width,
        height: 0.0,
    }];
    duck.spawn_motion = Some((
        duck.physics_position(Vec2::new(
            duck.width * 0.25,
            duck.layout.floor_y + duck.radius * 1.05,
        )),
        Vec2::ZERO,
    ));
    for _ in 0..90 {
        duck.step();
    }
    assert!(duck.grounded());
    duck
}

fn pool(duck: &DuckEvent, depth: f32) -> WaterWorld {
    let half = duck.width * 0.5;
    let columns = 64;
    let mut water = WaterWorld::new(
        WaterConfig::default(),
        vec![PoolSpec {
            left: f64::from(-half),
            column_width: f64::from(duck.width) / columns as f64,
            bed: vec![f64::from(duck.layout.floor_y); columns],
            boundaries: [Boundary::Closed; 2],
        }],
    )
    .unwrap();
    for column in 0..columns {
        water
            .add_to_pool(
                0,
                f64::from(-half) + (column as f64 + 0.5) * f64::from(duck.width) / columns as f64,
                f64::from(depth * duck.width) / columns as f64,
            )
            .unwrap();
    }
    water
}

fn floating() -> (DuckEvent, WaterWorld) {
    let mut duck = flat_player(4.0 / 3.0, 1.0);
    let water = pool(&duck, duck.radius * 4.0);
    for _ in 0..480 {
        duck.step_with_water(Some(&water));
    }
    assert!(!duck.grounded());
    assert!((0.42..0.48).contains(&duck.water_report.submerged_fraction));
    (duck, water)
}

#[test]
fn run_is_faster_but_does_not_raise_jump_height_on_any_target_layout() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        for direction in [-1.0, 1.0] {
            let mut jumps = Vec::new();
            for run in [false, true] {
                let mut duck = flat_player(aspect, direction);
                duck.set_player_input((direction * 1000.0) as i16, false, run, false);
                for _ in 0..30 {
                    duck.step();
                }
                let speed = duck
                    .world
                    .as_ref()
                    .unwrap()
                    .motion(DUCK_BODY)
                    .unwrap()
                    .linear_velocity
                    .x
                    * direction;
                let expected = duck.movement.run_speed * if run { 1.5 } else { 1.0 };
                assert!((speed - expected).abs() < 0.01, "{speed} vs {expected}");
                let start = duck.position().unwrap();
                let mut peak = start.y;
                duck.set_player_input((direction * 1000.0) as i16, true, run, false);
                let mut landed = false;
                for tick in 0..120 {
                    duck.step();
                    peak = peak.max(duck.position().unwrap().y);
                    if tick > 3 && duck.grounded() {
                        landed = true;
                        break;
                    }
                }
                assert!(landed);
                assert_eq!(duck.jumps, 1);
                jumps.push((duck.position().unwrap().x - start.x, peak - start.y));
                duck.set_player_input(0, true, run, false);
                for _ in 0..60 {
                    duck.step();
                }
                assert_eq!(
                    duck.jumps, 1,
                    "holding Jump must not auto-hop after landing"
                );
            }
            assert!(jumps[1].0 > jumps[0].0 * 1.45, "{jumps:?}");
            assert!((jumps[1].1 - jumps[0].1).abs() < 0.01, "{jumps:?}");
        }
    }
}

#[test]
fn holding_or_mashing_swim_is_cadence_limited_and_never_becomes_air_jumps() {
    for mash in [false, true] {
        let (mut duck, water) = floating();
        let ledger = water.stats();
        let mut last_stroke = None;
        let mut strokes = 0;
        let mut peak = f32::NEG_INFINITY;
        for tick in 0..600 {
            duck.set_player_input(0, !mash || tick % 2 == 0, false, false);
            duck.step_with_water(Some(&water));
            let current = duck.player_diagnostics().unwrap();
            if current.swim_strokes != strokes {
                if let Some(previous) = last_stroke {
                    assert!(duck.tick - previous >= player_control::SWIM_COOLDOWN_TICKS);
                }
                last_stroke = Some(duck.tick);
                strokes = current.swim_strokes;
            }
            peak = peak.max(duck.position().unwrap().y);
            assert_eq!(duck.jumps, 0);
        }
        assert!((4..=34).contains(&strokes), "strokes={strokes}");
        let surface = duck.layout.floor_y + duck.radius * 4.0;
        assert!(
            peak > surface + duck.radius * 0.6,
            "a visible bob above the surface"
        );
        assert!(
            peak < surface + duck.radius * 3.5,
            "no repeated-flight jetpack"
        );
        assert_eq!(water.stats(), ledger);

        // Enter the wet hold, consume its edge, then remove water mid-stroke.
        duck.set_player_input(0, true, false, false);
        duck.step_with_water(Some(&water));
        let strokes = duck.player.as_ref().unwrap().swim_strokes;
        for _ in 0..180 {
            duck.step();
        }
        assert!(duck.grounded());
        assert_eq!(
            duck.jumps, 0,
            "no buffered jump when held swim reaches shore"
        );
        assert_eq!(duck.player.as_ref().unwrap().swim_strokes, strokes);
        duck.set_player_input(0, false, false, false);
        duck.step();
        duck.set_player_input(0, true, false, false);
        duck.step();
        assert_eq!(duck.jumps, 1, "a fresh ground press still jumps");
    }
}

#[test]
fn dive_is_shallow_and_release_restores_float_while_jump_overrides_down() {
    let (mut duck, water) = floating();
    let float_y = duck.position().unwrap().y;
    duck.set_player_input(0, false, false, true);
    let mut lowest = float_y;
    for _ in 0..600 {
        duck.step_with_water(Some(&water));
        lowest = lowest.min(duck.position().unwrap().y);
    }
    let diving = duck.player_diagnostics().unwrap();
    assert!((730..=840).contains(&diving.submerged_milli), "{diving:?}");
    assert!(
        lowest > float_y - duck.radius * 1.25,
        "no unlimited descent"
    );
    assert!(duck.position().unwrap().y < float_y - duck.radius * 0.3);
    assert_eq!(diving.swim_strokes, 0);
    duck.set_player_input(0, true, false, true);
    duck.step_with_water(Some(&water));
    assert_eq!(duck.player.as_ref().unwrap().swim_strokes, 1);
    assert!(
        duck.world
            .as_ref()
            .unwrap()
            .motion(DUCK_BODY)
            .unwrap()
            .linear_velocity
            .y
            > 0.0
    );
    duck.set_player_input(0, false, false, false);
    for _ in 0..480 {
        duck.step_with_water(Some(&water));
    }
    assert!((duck.position().unwrap().y - float_y).abs() < duck.radius * 0.03);
    assert!((420..=480).contains(&duck.player_diagnostics().unwrap().submerged_milli));
}

#[test]
fn shallow_water_uses_ground_jumps_and_down_does_not_push_through_the_floor() {
    let mut duck = flat_player(4.0 / 3.0, 1.0);
    let water = pool(&duck, duck.radius * 0.8);
    let y = duck.position().unwrap().y;
    duck.set_player_input(0, false, false, true);
    for _ in 0..120 {
        duck.step_with_water(Some(&water));
    }
    assert!(duck.grounded());
    assert!((duck.position().unwrap().y - y).abs() < duck.radius * 0.03);
    duck.set_player_input(0, true, false, true);
    duck.step_with_water(Some(&water));
    assert_eq!(duck.jumps, 1);
    assert_eq!(duck.player.as_ref().unwrap().swim_strokes, 0);
}

#[test]
fn swim_can_clear_a_low_bank_that_paddling_alone_cannot() {
    for direction in [-1.0, 1.0] {
        for swim in [false, true] {
            let mut duck = DuckEvent::new_player(Layout::new(4.0 / 3.0), 42, None, 1, 1);
            duck.direction = direction;
            let bank = duck.width * 0.6;
            duck.course.as_mut().unwrap().surfaces = vec![
                Surface {
                    start: 0.0,
                    end: bank,
                    height: 0.0,
                },
                Surface {
                    start: bank,
                    end: duck.width,
                    height: duck.radius * 3.0,
                },
            ];
            duck.spawn_motion = Some((
                duck.physics_position(Vec2::new(
                    bank - duck.radius * 6.0,
                    duck.layout.floor_y + duck.radius * 2.6,
                )),
                Vec2::ZERO,
            ));
            let geometry = arena::CourseGeometry::from_duck(&duck);
            let mut water =
                WaterWorld::new(WaterConfig::default(), geometry.water_pools()).unwrap();
            let wet_pool = usize::from(direction < 0.0);
            let columns = water.pools()[wet_pool]
                .columns()
                .map(|c| (c.left + c.width * 0.5, c.width))
                .collect::<Vec<_>>();
            for (x, width) in columns {
                water
                    .add_to_pool(wet_pool, x, width * f64::from(duck.radius * 2.5))
                    .unwrap();
            }
            for _ in 0..120 {
                duck.step_with_water(Some(&water));
            }
            duck.set_player_input((direction * 1000.0) as i16, swim, false, false);
            let mut ashore = false;
            for _ in 0..360 {
                duck.step_with_water(Some(&water));
                if duck.grounded() && duck.position().unwrap().x > bank + duck.radius * 2.0 {
                    ashore = true;
                    break;
                }
            }
            assert_eq!(
                ashore,
                swim,
                "direction={direction} swim={swim}: {:?}",
                duck.player_diagnostics()
            );
        }
    }
}

#[test]
fn run_boosts_paddling_without_removing_a_real_opposing_current() {
    let mut speeds = Vec::new();
    for run in [false, true] {
        let (mut duck, mut water) = floating();
        let y = duck
            .world
            .as_ref()
            .unwrap()
            .motion(DUCK_BODY)
            .unwrap()
            .position
            .y;
        // A left-high surface makes a right-going current through the real
        // water solver. The player paddles left against it; no synthetic flow.
        duck.world.as_mut().unwrap().set_pose(
            DUCK_BODY,
            Vec2::new(-duck.radius * 3.0, y),
            0.0,
            true,
        );
        let left = water.pools()[0]
            .columns()
            .take(32)
            .map(|c| (c.left + c.width * 0.5, c.width))
            .collect::<Vec<_>>();
        for (x, width) in left {
            water
                .add_to_pool(0, x, width * f64::from(duck.radius * 0.5))
                .unwrap();
        }
        let ledger = water.stats().pooled;
        duck.set_player_input(-1000, false, run, false);
        for _ in 0..90 {
            water.step(f64::from(DT)).unwrap();
            duck.step_with_water(Some(&water));
        }
        assert!((water.stats().pooled - ledger).abs() < 1e-6);
        assert!(!duck.grounded());
        speeds.push(
            duck.world
                .as_ref()
                .unwrap()
                .motion(DUCK_BODY)
                .unwrap()
                .linear_velocity
                .x,
        );
        // Releasing the stick stops propulsion, not buoyancy/current coupling.
        duck.set_player_input(0, false, run, false);
        let before = duck
            .world
            .as_ref()
            .unwrap()
            .motion(DUCK_BODY)
            .unwrap()
            .linear_velocity
            .x;
        duck.step_with_water(Some(&water));
        let after = duck
            .world
            .as_ref()
            .unwrap()
            .motion(DUCK_BODY)
            .unwrap()
            .linear_velocity
            .x;
        assert!(
            after.abs() > before.abs() * 0.7,
            "neutral must not reset drift"
        );
    }
    assert!(
        speeds[0] < -1.0 && speeds[1] < speeds[0] * 1.25,
        "{speeds:?}"
    );
}

#[test]
fn held_down_and_jump_add_no_vertical_motion_in_dry_air() {
    let mut baseline = flat_player(4.0 / 3.0, 1.0);
    let mut held = flat_player(4.0 / 3.0, 1.0);
    for duck in [&mut baseline, &mut held] {
        duck.set_player_input(0, true, false, false);
        duck.step();
    }
    baseline.set_player_input(0, false, false, false);
    held.set_player_input(0, true, true, true);
    for _ in 0..120 {
        baseline.step();
        held.step();
        assert_eq!(held.position(), baseline.position());
        assert_eq!(held.jumps, 1);
    }
    assert_eq!(held.player.as_ref().unwrap().swim_strokes, 0);
}
