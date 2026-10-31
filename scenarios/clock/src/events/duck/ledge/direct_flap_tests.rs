use super::flap_detour_tests::{ASPECTS, PROFILES, block, flood, ordinary, until};
use super::*;
use engine_common::{
    ClockDuckFlapDetourOutcome as Outcome, ClockDuckFlapDetourPhase as DetourPhase,
};

fn direct(aspect: f32, direction: f32) -> TestDuck {
    let mut duck = ordinary(aspect, direction, PROFILES[0]);
    duck.select_direct_flap_landing(true);
    duck
}

fn finished(duck: &TestDuck) -> bool {
    duck.ledge.detour.stats.phase == DetourPhase::Finished
}

#[test]
fn direct_flap_lands_perches_returns_and_resumes_from_normal_spawn() {
    for aspect in ASPECTS {
        for direction in [-1.0, 1.0] {
            for profile in PROFILES {
                let mut duck = direct(aspect, direction);
                duck.controller.profile = profile;
                let mut standing_ticks = 0;
                for _ in 0..350 {
                    duck.step();
                    let stats = duck.ledge.detour.stats;
                    assert!(stats.clearance_queries <= 64);
                    assert_eq!(duck.ledge.stats.catches, 0);
                    assert_eq!(duck.ledge.stats.climbs, 0);
                    assert!(!duck.ledge_attached());
                    assert!(duck.physics_counts().0 <= 8);
                    if stats.phase == DetourPhase::Perching {
                        assert!(duck.grounded());
                        assert!(duck.door_openness().0 < 0.2);
                        standing_ticks += 1;
                    }
                    if finished(&duck) {
                        break;
                    }
                }
                assert_eq!(
                    duck.ledge.detour.stats.outcome,
                    Some(Outcome::Returned),
                    "{aspect} {direction} {profile:?}: {:?}",
                    duck.diagnostics()
                );
                assert!(standing_ticks >= flap_detour::PERCH_TICKS);
                assert_eq!(duck.ledge.detour.stats.attempts, 1);
                assert_eq!(
                    duck.ledge.detour.stats.perch_ticks,
                    flap_detour::PERCH_TICKS
                );
                assert_eq!(duck.controller.heights.count, 0);
                assert_eq!(duck.jumps, 3); // ordinary launch, return, then warm-up
                while duck.phase != EventPhase::Resetting && duck.tick < DUCK_TICKS {
                    duck.step();
                }
                assert_eq!(duck.outcome, Some(ClockDuckOutcome::Exited));
                assert_eq!(duck.controller.heights.count, 2);
            }
        }
    }
}

#[test]
fn direct_flap_keeps_seeded_selection_and_course_geometry() {
    assert!(!crate::ClockConfig::default().duck_direct_flap_landing);
    for seed in 0..40 {
        let mut duck = TestDuck::new_course(Layout::new(ASPECTS[0]), seed, None);
        let surfaces = duck.terrain.course.as_ref().unwrap().surfaces.clone();
        let direction = duck.direction;
        duck.select_direct_flap_landing(true);
        until(&mut duck, finished);
        assert_eq!(
            duck.ledge.detour.stats.outcome,
            Some(if flap_detour::selected(seed) {
                Outcome::Returned
            } else {
                Outcome::NotSelected
            }),
            "seed={seed}: {:?}",
            duck.diagnostics()
        );
        assert_eq!(duck.terrain.course.as_ref().unwrap().surfaces, surfaces);
        assert_eq!(duck.direction, direction);
    }
}

#[test]
fn direct_flap_refuses_a_gap_inside_the_return_bodys_bounds() {
    let mut duck = direct(ASPECTS[0], 1.0);
    until(&mut duck, |d| d.position().is_some());
    let r = duck.radius;
    let entity = PhysicsId::new(1000);
    let mut pieces = Vec::new();
    for (part, center, half) in [(0, 3.5, 3.5), (1, 12.0, 2.0)] {
        let mut piece = ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, part),
            half * r,
            r,
        );
        piece.local_position = duck.render_position(Vec2::new(center * r, duck.layout.floor_y - r));
        pieces.push(piece);
    }
    let world = duck.world.as_mut().unwrap();
    world.remove_entity(entity);
    assert!(world.insert_body(
        BodyId::new(entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &pieces
    ));
    until(&mut duck, finished);
    assert_eq!(duck.ledge.detour.stats.outcome, Some(Outcome::Unsafe));
    assert_eq!(duck.ledge.detour.stats.attempts, 0);
}

#[test]
fn direct_flap_refuses_obstructions_before_launch_and_before_return() {
    for stage in [
        DetourPhase::Pending,
        DetourPhase::Approaching,
        DetourPhase::Perching,
    ] {
        let mut duck = direct(ASPECTS[0], 1.0);
        until(&mut duck, |d| {
            d.position().is_some() && d.ledge.detour.stats.phase == stage
        });
        let r = duck.radius;
        let point = duck.render_position(Vec2::new(
            if stage == DetourPhase::Perching {
                4.0 * r
            } else {
                6.0 * r
            },
            duck.layout.floor_y + 5.0 * r,
        ));
        block(
            &mut duck,
            point,
            Vec2::new(
                r * 0.3,
                r * if stage == DetourPhase::Perching {
                    6.0
                } else {
                    2.0
                },
            ),
        );
        until(&mut duck, finished);
        assert_eq!(
            duck.ledge.detour.stats.outcome,
            Some(if stage == DetourPhase::Pending {
                Outcome::Unsafe
            } else {
                Outcome::Blocked
            }),
            "{stage:?}: {:?}",
            duck.diagnostics()
        );
        assert_eq!(duck.ledge.stats.catches, 0);
        assert!(!duck.ledge_attached());
    }
}

#[test]
fn direct_flap_late_or_disrupted_landing_ends_without_retry() {
    for late in [false, true] {
        let mut duck = direct(ASPECTS[0], 1.0);
        until(&mut duck, |d| d.jumps == 1 && d.phase_tick >= 25);
        if late {
            duck.ledge.detour.age = 100;
        } else {
            let delta = Vec2::new(duck.radius * 50.0, 0.0);
            duck.world
                .as_mut()
                .unwrap()
                .apply_velocity_delta(DUCK_BODY, delta, true);
        }
        until(&mut duck, finished);
        assert_eq!(duck.ledge.detour.stats.outcome, Some(Outcome::Expired));
        assert_eq!(duck.ledge.detour.stats.attempts, 1);
        assert_eq!(duck.ledge.stats.catches, 0);
        assert_eq!(duck.controller.heights.count, 0);
    }
}

#[test]
fn direct_flap_handles_water_timeout_deadline_and_lost_support() {
    for case in 0..4 {
        let mut duck = direct(ASPECTS[0], 1.0);
        until(&mut duck, |d| {
            d.ledge.detour.stats.phase == DetourPhase::Perching
        });
        let mut water = None;
        match case {
            0 => water = Some(flood(&duck)),
            1 => duck.ledge.detour.stats.ticks = flap_detour::BUDGET_TICKS - 1,
            2 => duck.tick = DUCK_TICKS - RESET_TICKS - u64::from(flap_detour::BUDGET_TICKS),
            _ => {
                duck.world
                    .as_mut()
                    .unwrap()
                    .remove_entity(PhysicsId::new(1000));
            }
        }
        for _ in 0..3 {
            duck.step_with_water(water.as_ref());
        }
        assert_eq!(
            duck.ledge.detour.stats.outcome,
            Some(
                [
                    Outcome::Wet,
                    Outcome::TimedOut,
                    Outcome::Deadline,
                    Outcome::Blocked
                ][case]
            )
        );
        assert!(!duck.ledge_attached());
        assert_eq!(duck.controller.heights.count, 0);
    }
}

#[test]
fn direct_flap_takeover_preserves_motion_at_each_stage() {
    for stage in [
        DetourPhase::Approaching,
        DetourPhase::Perching,
        DetourPhase::Returning,
    ] {
        let mut duck = direct(ASPECTS[0], -1.0);
        until(&mut duck, |d| d.ledge.detour.stats.phase == stage);
        let before = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
        assert!(duck.take_control(9, 1));
        let after = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
        assert_eq!(before.position, after.position);
        assert_eq!(before.linear_velocity, after.linear_velocity);
        assert_eq!(duck.ledge.detour.stats.outcome, Some(Outcome::TakenOver));
        let stats = duck.ledge.detour.stats;
        for _ in 0..5 {
            duck.step();
        }
        assert_eq!(duck.ledge.detour.stats, stats);
    }
}

#[test]
fn direct_flap_accepts_closed_responsive_floor_and_refuses_moving_support() {
    use crate::floor::responsive::FloorShape;
    for aspect in ASPECTS {
        for direction in [-1.0, 1.0] {
            for moving in [false, true] {
                let mut duck = direct(aspect, direction);
                let mut floor = ResponsiveFloor::new(FloorShape::clock(duck.layout), 0.0);
                if moving {
                    floor.opening = 0.9;
                }
                duck.terrain.responsive_floor = Some(floor);
                until(&mut duck, finished);
                if moving {
                    assert!(matches!(
                        duck.ledge.detour.stats.outcome,
                        Some(Outcome::Unsafe | Outcome::Expired)
                    ));
                    assert_eq!(duck.ledge.detour.stats.attempts, 0);
                } else {
                    assert_eq!(
                        duck.ledge.detour.stats.outcome,
                        Some(Outcome::Returned),
                        "{aspect} {direction}: {:?}",
                        duck.diagnostics()
                    );
                }
            }
        }
    }
}
