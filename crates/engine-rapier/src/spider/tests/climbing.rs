//! Holding and crawling: the legs carry the spider against gravity on every
//! face of the course, round its corners, and let go for a reason.
use super::*;

/// The axis a normal is nearest, for naming faces.
fn face(normal: Vec2) -> (i8, i8) {
    if normal.x.abs() > normal.y.abs() {
        (normal.x.signum() as i8, 0)
    } else {
        (0, normal.y.signum() as i8)
    }
}

#[test]
fn crawls_the_room_against_gravity_around_inside_and_outside_corners() {
    for scale in [CLOCK, METRES] {
        // In the gap, on the floor, crawling with the floor on its left: the
        // west mesa's face is ahead, then its top, the left wall and the
        // ceiling, upside down.
        let mut run = Run::new(course(scale), scale, Vec2::new(-4.0, 1.0));
        let mut faces: Vec<(u64, (i8, i8))> = Vec::new();
        let mut steady: Vec<bool> = Vec::new();
        let mut height = f32::MIN;
        for tick in 0..2000 {
            let state = run.tick(crawl(-1.0));
            if tick == 0 {
                continue;
            }
            assert_eq!(state.mode, SpiderMode::Crawling, "tick {tick}");
            assert_eq!(state.last_release, None);
            let support = state.support.unwrap();
            let key = (support.collider.entity.value(), face(support.normal));
            if faces.last() != Some(&key) {
                faces.push(key);
                steady.push(false);
            }
            // Body on the surface, neither sunk into it nor drifting off.
            let gap = (state.position - support.point).dot(support.normal) / scale.r;
            assert!((gap - 1.0).abs() < 0.1, "tick {tick}: {gap}");
            // Never much faster than the crawl along the surface, even where
            // the surface turns under it.
            let along = Vec2::new(support.normal.y, -support.normal.x);
            let speed = -state.velocity.dot(along) / scale.r;
            assert!(speed < 5.0 * 1.05, "tick {tick}: {speed} on {key:?}");
            *steady.last_mut().unwrap() |= (speed - 5.0).abs() < 1e-3;
            // Climbing the walls without sliding back.
            if key.1 == (1, 0) {
                assert!(state.position.y >= height - 1e-3 * scale.r, "tick {tick}");
            }
            height = state.position.y;
        }
        let expected: Vec<(u64, (i8, i8))> = vec![
            (FLOOR.value(), (0, 1)),
            (WEST_MESA.value(), (1, 0)),
            (WEST_MESA.value(), (0, 1)),
            (LEFT_WALL.value(), (1, 0)),
            (CEILING.value(), (0, -1)),
        ];
        assert_eq!(faces, expected, "g {}", scale.g);
        // Each face crawled at the full speed somewhere along it.
        assert_eq!(steady, vec![true; faces.len()], "{faces:?}");
        // Along the ceiling, the spider went right, 21 radii from the corner.
        assert!(run.at().x > -40.0, "{:?}", run.at());
    }
}

#[test]
fn a_spider_too_weak_to_carry_its_weight_slips_down_a_wall() {
    let scale = CLOCK;
    let spec = SpiderSpec {
        drive: 0.8,
        ..SpiderSpec::new(scale.r, scale.g)
    };
    // Against the west mesa's face, trying to climb it.
    let mut run = Run::with_spec(course(scale), scale, Vec2::new(-11.0, 10.0), spec);
    run.ticks(3, crawl(-1.0));
    let before = run.state();
    let after = run.ticks(30, crawl(-1.0));
    assert_eq!(after.mode, SpiderMode::Crawling);
    assert_eq!(after.support.unwrap().collider.entity, WEST_MESA);
    // Gravity wins by what the legs cannot push: a fifth of a weight.
    let acceleration = (after.velocity.y - before.velocity.y) / (30.0 * DT);
    assert!(
        (acceleration / scale.g + 0.2).abs() < 0.01,
        "{}",
        acceleration / scale.g
    );
}

#[test]
fn overloaded_grip_lets_go_rounding_onto_an_underside_and_keeps_its_velocity() {
    let scale = CLOCK;
    let shelf = PhysicsId::new(7);
    for grip_weights in [1.1, 3.0] {
        let mut world = course(scale);
        block(
            &mut world,
            scale,
            shelf,
            Vec2::new(0.0, 26.0),
            Vec2::new(6.0, 2.0),
        );
        let spec = SpiderSpec {
            grip: grip_weights,
            ..SpiderSpec::new(scale.r, scale.g)
        };
        // On the shelf, crawling right: over its edge, down its side and
        // under it.
        let mut run = Run::with_spec(world, scale, Vec2::new(-2.0, 29.0), spec);
        let mut previous = run.tick(crawl(1.0));
        let mut let_go = None;
        let mut underneath = 0;
        for tick in 0..600 {
            let state = run.tick(crawl(1.0));
            if state
                .support
                .is_some_and(|s| s.collider.entity == shelf && s.normal.y < -0.999)
            {
                underneath += 1;
            }
            if let_go.is_none() && state.support.is_none() {
                let_go = Some(tick);
                assert_eq!(
                    state.last_release,
                    Some(SpiderRelease {
                        hold: SpiderHold::Surface,
                        reason: SpiderReleaseReason::Overloaded,
                    })
                );
                // It was rounding the corner onto the underside, its legs at
                // their limit against gravity and the turn.
                let normal = previous.support.unwrap().normal;
                assert!(normal.x > 0.0 && normal.y < 0.0, "{normal:?}");
                // It falls from where it was, as it was going.
                let next = run.tick(crawl(1.0));
                let change = (next.velocity - state.velocity) / DT;
                assert!((change - Vec2::new(0.0, -scale.g)).length() < 1e-3 * scale.g);
                assert!(state.position.distance_to(previous.position) < scale.r);
                break;
            }
            previous = state;
        }
        if grip_weights < 1.25 {
            assert!(let_go.is_some(), "a weak grip lets go");
            assert_eq!(underneath, 0);
        } else {
            // A full grip carries it upside down along the underside, and on
            // round the shelf.
            assert_eq!(let_go, None);
            assert!(underneath as f32 * DT > 10.0 / 5.0, "{underneath}");
            assert_eq!(run.state().support.unwrap().collider.entity, shelf);
        }
    }
}

#[test]
fn a_falling_spider_catches_what_it_reaches_and_nothing_through_a_solid() {
    let scale = CLOCK;
    // Dropped in the open, it lands and holds the floor.
    let mut run = Run::new(course(scale), scale, Vec2::new(0.0, 30.0));
    let landed = run.until(120, grip(), |s| s.support.is_some()).unwrap();
    let state = run.ticks(10, grip());
    assert!(landed > 40);
    assert_eq!(state.support.unwrap().collider.entity, FLOOR);
    assert!(
        state.velocity.length() < 0.05 * scale.r,
        "{:?}",
        state.velocity
    );
    assert_eq!(state.counts.catches, 1);
    // Falling beside the west mesa's face: a leg's reach catches it at 0.3
    // radii, not at 0.8, and never through a pane in between.
    let pane = PhysicsId::new(FOREIGN.value() + 1);
    for (gap, paned, catches_face) in [(0.3, false, true), (0.8, false, false), (0.3, true, false)]
    {
        let mut world = course(scale);
        if paned {
            block(
                &mut world,
                scale,
                pane,
                Vec2::new(-11.85, 8.0),
                Vec2::new(0.05, 8.0),
            );
        }
        let mut run = Run::new(world, scale, Vec2::new(-11.0 + gap, 14.0));
        run.until(120, grip(), |s| s.support.is_some()).unwrap();
        let held = run.state().support.unwrap();
        let context = format!("gap {gap}, pane {paned}");
        assert_eq!(held.collider.entity == WEST_MESA, catches_face, "{context}");
        assert_eq!(held.collider.entity == FLOOR, !catches_face, "{context}");
        if catches_face {
            assert!(run.at().y > 10.0, "{context}: {:?}", run.at());
            assert!(face(held.normal) == (1, 0));
        }
    }
}

#[test]
fn rides_and_crawls_on_a_moving_platform() {
    let scale = CLOCK;
    let r = scale.r;
    for vertical in [false, true] {
        let platform = PhysicsId::new(7);
        let mut world = world(scale);
        let velocity = if vertical {
            Vec2::new(0.0, 3.0)
        } else {
            Vec2::new(3.0, 0.0)
        } * r;
        assert!(world.insert_body(
            BodyId::new(platform, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::KinematicVelocity,
                position: Vec2::new(0.0, 10.0) * r,
                linear_velocity: velocity,
                ..BodySpec::default()
            },
            &[ColliderSpec::cuboid(
                ColliderId::new(platform, ColliderRole::PRIMARY, 0),
                40.0 * r,
                1.0 * r,
            )],
        ));
        world.step(DT);
        let mut run = Run::new(world, scale, Vec2::new(0.0, 12.5));
        run.ticks(30, grip());
        for direction in [0.0, 1.0, -1.0] {
            let state = run.ticks(30, crawl(direction));
            assert_eq!(state.support.unwrap().collider.entity, platform);
            let expected = velocity + Vec2::new(5.0 * direction * r, 0.0);
            assert!(
                (state.velocity - expected).length() < 0.02 * r,
                "vertical {vertical}, crawl {direction}: {:?}",
                state.velocity / r
            );
        }
    }
}

#[test]
fn a_support_that_leaves_or_is_replaced_is_let_go_and_never_inherited() {
    let scale = CLOCK;
    let beam = PhysicsId::new(7);
    for replace in [false, true] {
        let mut world = world(scale);
        block(
            &mut world,
            scale,
            beam,
            Vec2::new(0.0, 20.0),
            Vec2::new(8.0, 1.0),
        );
        world.step(DT);
        let mut run = Run::new(world, scale, Vec2::new(0.0, 22.0));
        let held = run.ticks(20, grip());
        assert_eq!(held.support.unwrap().collider.entity, beam);
        assert!(run.world.remove_entity(beam));
        if replace {
            // The same identities, in the same place, are a new support.
            block(
                &mut run.world,
                scale,
                beam,
                Vec2::new(0.0, 20.0),
                Vec2::new(8.0, 1.0),
            );
        }
        let state = run.tick(grip());
        let context = format!("replaced {replace}");
        assert_eq!(state.support, None, "{context}");
        assert_eq!(
            state.last_release,
            Some(SpiderRelease {
                hold: SpiderHold::Surface,
                reason: SpiderReleaseReason::Removed,
            }),
            "{context}"
        );
        assert_eq!(state.counts.catches, 1, "{context}");
        if replace {
            // Only a fresh catch, counted and after the pause any lost hold
            // takes, holds the new beam.
            let ticks = run.until(30, grip(), |s| s.support.is_some()).unwrap();
            assert!(ticks as f32 * DT >= REARM_SECONDS, "{ticks}");
            assert_eq!(run.state().counts.catches, 2);
        } else {
            let next = run.tick(grip());
            let change = (next.velocity - state.velocity) / DT;
            assert!((change - Vec2::new(0.0, -scale.g)).length() < 1e-3 * scale.g);
        }
    }
}

#[test]
fn an_ineligible_or_unreachable_support_is_let_go() {
    let scale = CLOCK;
    let mut run = Run::new(course(scale), scale, Vec2::new(0.0, 1.0));
    run.ticks(10, grip());
    assert!(
        run.spider
            .drive(&mut run.world, DT, grip(), |c| c.entity != FLOOR)
    );
    run.world.step(DT);
    let state = run.state();
    assert_eq!(state.support, None);
    assert_eq!(
        state.last_release.map(|r| r.reason),
        Some(SpiderReleaseReason::Ineligible)
    );
    // A platform that drops away faster than the legs can follow.
    let platform = PhysicsId::new(7);
    let mut world = world(scale);
    assert!(world.insert_body(
        BodyId::new(platform, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::KinematicPosition,
            position: Vec2::new(0.0, 10.0) * scale.r,
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(platform, ColliderRole::PRIMARY, 0),
            20.0 * scale.r,
            scale.r,
        )],
    ));
    world.step(DT);
    let mut run = Run::new(world, scale, Vec2::new(0.0, 12.0));
    run.ticks(10, grip());
    let body = BodyId::new(platform, BodyRole::PRIMARY);
    assert!(
        run.world
            .set_next_kinematic_pose(body, Vec2::new(0.0, 7.0) * scale.r, 0.0)
    );
    run.tick(grip());
    let state = run.tick(grip());
    assert_eq!(state.support, None);
    assert_eq!(
        state.last_release.map(|r| r.reason),
        Some(SpiderReleaseReason::OutOfReach)
    );
}

#[test]
fn crawls_across_a_seam_between_two_colliders_without_letting_go() {
    let west = PhysicsId::new(7);
    let east = PhysicsId::new(8);
    for scale in [CLOCK, METRES] {
        for upside_down in [false, true] {
            // Two tiles meeting edge to edge at x = 0: a floor along y = 11,
            // or a ceiling along y = 29.
            let (height, side) = if upside_down {
                (30.0, -1.0)
            } else {
                (10.0, 1.0)
            };
            let mut world = world(scale);
            block(
                &mut world,
                scale,
                west,
                Vec2::new(-10.0, height),
                Vec2::new(10.0, 1.0),
            );
            block(
                &mut world,
                scale,
                east,
                Vec2::new(10.0, height),
                Vec2::new(10.0, 1.0),
            );
            world.step(DT);
            let mut run = Run::new(world, scale, Vec2::new(-5.0, height + side * 2.1));
            run.until(30, grip(), |s| s.support.is_some()).unwrap();
            let mut held = Vec::new();
            for tick in 0..240 {
                let state = run.tick(crawl(side));
                let support = state.support.expect("never lets go");
                if held.last() != Some(&support.collider.entity) {
                    held.push(support.collider.entity);
                }
                let gap = (state.position.y / scale.r - height) * side - 2.0;
                assert!(
                    gap.abs() < 0.1,
                    "upside down {upside_down}, tick {tick}: {gap}"
                );
            }
            let state = run.state();
            assert_eq!(held, vec![west, east], "upside down {upside_down}");
            assert_eq!(state.counts.catches, 1);
            assert_eq!(state.counts.surface_releases, 0);
            assert!(run.at().x > 14.0, "{:?}", run.at());
        }
    }
}
