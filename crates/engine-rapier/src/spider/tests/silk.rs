//! The silk line: shots and their failures, hanging and swinging, reeling,
//! cuts and replacements, and letting go when the anchor goes or something
//! solid comes between.
use std::f32::consts::TAU;

use super::*;

const SHELF: PhysicsId = PhysicsId::new(7);
const PANE: PhysicsId = PhysicsId::new(FOREIGN.value() + 1);

fn idle() -> SpiderIntent {
    SpiderIntent::default()
}

fn reel(amount: f32) -> SpiderIntent {
    SpiderIntent {
        reel: amount,
        ..idle()
    }
}

/// A spider hanging still from the ceiling over the middle of the room.
fn hanging(scale: Scale) -> Run {
    let mut run = Run::new(course(scale), scale, Vec2::new(0.0, 20.0));
    run.hang_from_ceiling();
    run
}

/// The counts that must always balance.
fn assert_balanced(state: &SpiderState) {
    let counts = state.counts;
    assert_eq!(
        counts.shots,
        counts.lines_tied + counts.shot_failures + u64::from(state.shot.is_some()),
        "{counts:?}"
    );
    assert_eq!(
        counts.lines_tied,
        counts.lines_retired + u64::from(state.line.is_some()),
        "{counts:?}"
    );
    assert_eq!(state.joints, usize::from(state.line.is_some()));
}

#[test]
fn a_shot_reaches_out_visibly_and_ties_a_line_as_long_as_the_span() {
    for scale in [CLOCK, METRES] {
        let r = scale.r;
        let mut run = Run::new(course(scale), scale, Vec2::new(0.0, 1.0));
        run.ticks(2, grip());
        let shot = run.tick(shoot(Vec2::Y)).shot.expect("on its way");
        assert_eq!(shot.collider.entity, CEILING);
        assert!(
            (shot.target / r - Vec2::new(0.0, 40.0)).length() < 0.01,
            "{:?}",
            shot.target / r
        );
        // The line's end travels out at the shot speed, along the aim.
        let mut flying = 0;
        let mut reached = 0.0;
        let tied = loop {
            let state = run.tick(grip());
            assert_balanced(&state);
            let Some(shot) = state.shot else {
                break state;
            };
            flying += 1;
            assert_eq!(state.line, None);
            let out = shot.tip - state.position;
            assert!(out.normalized().dot(Vec2::Y) > 0.9999);
            let step = out.length() / r - reached;
            assert!((step - 160.0 * DT).abs() < 1e-3, "{step}");
            reached = out.length() / r;
        };
        assert_eq!(flying, (39.0_f32 / (160.0 * DT)).ceil() as u32 - 1);
        let line = tied.line.expect("tied on arrival");
        assert_eq!(line.collider.entity, CEILING);
        assert!((line.anchor / r - Vec2::new(0.0, 40.0)).length() < 0.01);
        assert!((line.length / r - 39.0).abs() < 0.05, "{}", line.length / r);
        assert!(line.slack < 0.01 * r);
        assert_eq!(tied.counts.lines_tied, 1);
        assert_eq!(run.world.joint_count(), 1);
        // Standing on the floor, the spider still crawls and the line is
        // just tied to it.
        assert_eq!(tied.mode, SpiderMode::Crawling);
    }
}

#[test]
fn a_shot_that_cannot_tie_says_why_and_leaves_nothing_behind() {
    let scale = CLOCK;
    let shoot_up = || shoot(Vec2::Y);
    type Setup = fn(&mut PhysicsWorld, Scale);
    type During = fn(&mut Run);
    let none: Setup = |_, _| {};
    let nothing: During = |_| {};
    let cases: [(&str, Setup, SpiderIntent, During, SilkFailure); 8] = [
        // Over the east mesa, the ceiling is 44 radii away that way.
        (
            "nothing in range",
            none,
            shoot(Vec2::new(1.0, 2.0)),
            nothing,
            SilkFailure::NoTarget,
        ),
        (
            "a foreign pane in the way",
            |world, scale| {
                block(
                    world,
                    scale,
                    PANE,
                    Vec2::new(0.0, 10.0),
                    Vec2::new(3.0, 0.5),
                )
            },
            shoot_up(),
            nothing,
            SilkFailure::Blocked,
        ),
        (
            "a second shot while one extends",
            none,
            shoot_up(),
            |run| {
                run.tick(shoot(Vec2::Y));
            },
            SilkFailure::Busy,
        ),
        (
            "the target leaves",
            |world, scale| {
                block(
                    world,
                    scale,
                    SHELF,
                    Vec2::new(0.0, 20.0),
                    Vec2::new(3.0, 1.0),
                )
            },
            shoot_up(),
            |run| {
                assert!(run.world.remove_entity(SHELF));
            },
            SilkFailure::TargetLost,
        ),
        (
            "the target is replaced",
            |world, scale| {
                block(
                    world,
                    scale,
                    SHELF,
                    Vec2::new(0.0, 20.0),
                    Vec2::new(3.0, 1.0),
                )
            },
            shoot_up(),
            |run| {
                assert!(run.world.remove_entity(SHELF));
                block(
                    &mut run.world,
                    run.scale,
                    SHELF,
                    Vec2::new(0.0, 20.0),
                    Vec2::new(3.0, 1.0),
                );
            },
            SilkFailure::TargetLost,
        ),
        (
            "a pane comes between",
            none,
            shoot_up(),
            |run| {
                block(
                    &mut run.world,
                    run.scale,
                    PANE,
                    Vec2::new(0.0, 30.0),
                    Vec2::new(3.0, 0.5),
                )
            },
            SilkFailure::Obstructed,
        ),
        (
            "cut on its way",
            none,
            shoot_up(),
            |run| {
                run.tick(SpiderIntent {
                    cut: true,
                    ..grip()
                });
            },
            SilkFailure::Cut,
        ),
        (
            "the target draws away faster than the shot",
            |world, scale| {
                assert!(world.insert_body(
                    BodyId::new(SHELF, BodyRole::PRIMARY),
                    BodySpec {
                        kind: BodyKind::KinematicVelocity,
                        position: Vec2::new(0.0, 10.0) * scale.r,
                        linear_velocity: Vec2::new(0.0, 170.0) * scale.r,
                        ..BodySpec::default()
                    },
                    &[ColliderSpec::cuboid(
                        ColliderId::new(SHELF, ColliderRole::PRIMARY, 0),
                        3.0 * scale.r,
                        scale.r,
                    )],
                ));
            },
            shoot_up(),
            nothing,
            SilkFailure::OutOfRange,
        ),
    ];
    for (case, setup, aim, during, failure) in cases {
        let mut world = course(scale);
        setup(&mut world, scale);
        let mut run = Run::new(world, scale, Vec2::new(0.0, 1.0));
        run.ticks(2, grip());
        run.tick(aim);
        during(&mut run);
        let state = run.ticks(30, grip());
        assert_eq!(state.last_failure, Some(failure), "{case}");
        assert_balanced(&state);
        if failure == SilkFailure::Busy {
            // The first shot ties all the same.
            assert_eq!(state.counts.shots, 2, "{case}");
            assert!(state.line.is_some(), "{case}");
            continue;
        }
        assert_eq!(state.counts.shot_failures, 1, "{case}");
        assert_eq!((state.line, state.shot), (None, None), "{case}");
        assert_eq!(run.world.joint_count(), 0, "{case}");
    }
    // A line longer than the spider spins fails as soon as it is shot.
    let spec = SpiderSpec {
        max_line: 30.0,
        ..SpiderSpec::new(scale.r, scale.g)
    };
    let mut run = Run::with_spec(course(scale), scale, Vec2::new(0.0, 1.0), spec);
    run.ticks(2, grip());
    run.tick(shoot(Vec2::Y));
    let state = run.ticks(30, grip());
    assert_eq!(state.last_failure, Some(SilkFailure::OutOfRange));
    assert_eq!(run.world.joint_count(), 0);
    assert_balanced(&state);
}

#[test]
fn a_hanging_spider_swings_with_the_period_of_a_pendulum_its_length() {
    for scale in [CLOCK, METRES] {
        let mut run = hanging(scale);
        let state = run.state();
        let line = state.line.unwrap();
        // Hanging still, the line carries the weight straight up.
        assert_eq!(state.mode, SpiderMode::Hanging);
        assert!(
            state.velocity.length() < 0.01 * scale.r,
            "{:?}",
            state.velocity
        );
        assert!(
            (line.tension / run.weight() - 1.0).abs() < 0.02,
            "{}",
            line.tension / run.weight()
        );
        assert!((state.up - Vec2::Y).length() < 1e-3);
        // Its weight draws the line out by the stretch one weight gives.
        assert!(line.slack < 1e-3 * scale.r);
        let stretch = SpiderSpec::new(scale.r, scale.g).silk_stretch;
        assert!(
            (line.stretch / scale.r - stretch).abs() < 1e-3,
            "{}",
            line.stretch / scale.r
        );
        // Pushed aside a tenth of a radian, it swings back and forth, as long
        // as it hangs.
        let length = line.distance;
        let push = 0.1 * (scale.g * length).sqrt();
        assert!(
            run.world
                .set_velocity(run.spider.body(), Vec2::new(push, 0.0), 0.0, true)
        );
        let mut crossings = Vec::new();
        let mut was = 0.0;
        for tick in 0..2000 {
            let state = run.tick(idle());
            let x = state.position.x - line.anchor.x;
            if tick > 0 && (x > 0.0) != (was > 0.0) {
                crossings.push(tick as f32 - x / (x - was));
            }
            was = x;
        }
        let swings = (crossings.len() - 1) as f32 / 2.0;
        let period = (crossings.last().unwrap() - crossings[0]) * DT / swings;
        let expected = TAU * (length / scale.g).sqrt();
        assert!(
            (period / expected - 1.0).abs() < 0.005,
            "{period} vs {expected}"
        );
    }
}

#[test]
fn reeling_climbs_the_line_at_its_speed_until_it_is_as_short_as_it_gets() {
    for scale in [CLOCK, METRES] {
        let mut run = hanging(scale);
        // Half the reel climbs at half the speed.
        run.ticks(30, reel(0.5));
        for _ in 0..30 {
            let state = run.tick(reel(0.5));
            let climbing = state.velocity.dot(state.up) / scale.r;
            assert!((climbing - 3.0).abs() < 0.03, "{climbing}");
        }
        run.ticks(30, reel(1.0));
        for _ in 0..30 {
            let state = run.tick(reel(1.0));
            let climbing = state.velocity.dot(state.up) / scale.r;
            assert!((climbing - 6.0).abs() < 0.06, "{climbing}");
            // Climbing steadily, the line carries the spider's weight.
            let line = state.line.unwrap();
            assert_eq!(state.mode, SpiderMode::Hanging);
            assert!(line.taut && line.slack < (6.0 * DT + TAUT) * scale.r);
            assert!(
                (line.tension / run.weight() - 1.0).abs() < 0.02,
                "{}",
                line.tension / run.weight()
            );
        }
        let state = run.ticks(600, reel(1.0));
        let line = state.line.unwrap();
        assert!(
            (line.length / scale.r - 1.25).abs() < 1e-3,
            "{}",
            line.length / scale.r
        );
        assert!(state.velocity.length() < 0.05 * scale.r);
    }
}

#[test]
fn crawling_and_reeling_at_once_climb_no_faster_than_the_faster_of_them() {
    for scale in [CLOCK, METRES] {
        // On the west mesa's face, tied to the ceiling straight above.
        let mut run = Run::new(course(scale), scale, Vec2::new(-11.0, 5.0));
        run.ticks(5, grip());
        run.tick(shoot(Vec2::Y));
        run.until(30, grip(), |s| s.line.is_some()).unwrap();
        let both = SpiderIntent {
            crawl: -1.0,
            reel: 1.0,
            ..grip()
        };
        let mut fastest: f32 = 0.0;
        let mut state = run.state();
        for _ in 0..40 {
            state = run.tick(both);
            assert_eq!(state.support.unwrap().collider.entity, WEST_MESA);
            fastest = fastest.max(state.velocity.y / scale.r);
        }
        // The reel, the faster, sets the pace; the legs add nothing to it.
        assert!(fastest < 6.0 * 1.01, "g {}: {fastest}", scale.g);
        assert!(
            (state.velocity.y / scale.r - 6.0).abs() < 0.03,
            "g {}",
            scale.g
        );
    }
}

#[test]
fn a_slack_line_lets_the_spider_fly_free_until_it_goes_taut_and_catches_it() {
    for scale in [CLOCK, METRES] {
        let mut run = hanging(scale);
        let hung = run.state();
        // Tossed up its line, high enough to rise a radius: the line goes
        // slack, and gravity alone moves the spider until the line catches it.
        let toss = (2.0 * scale.g * scale.r).sqrt();
        let body = run.spider.body();
        assert!(
            run.world
                .set_velocity(body, Vec2::new(0.0, toss), 0.0, true)
        );
        let mut previous = run.state();
        let mut flying = 0;
        let mut caught = false;
        for tick in 0..120 {
            let state = run.tick(idle());
            let line = state.line.expect("the line holds");
            if !line.taut {
                flying += 1;
                assert_eq!(state.mode, SpiderMode::Free, "tick {tick}");
                assert_eq!(line.tension, 0.0, "tick {tick}");
                let change = (state.velocity - previous.velocity) / DT;
                assert!(
                    (change - Vec2::new(0.0, -scale.g)).length() < 1e-3 * scale.g,
                    "tick {tick}: {change:?}"
                );
            } else if flying > 0 && !caught {
                caught = true;
                assert_eq!(state.mode, SpiderMode::Hanging, "tick {tick}");
                assert!(line.distance <= line.length * (1.0 + 1e-3), "{line:?}");
            }
            previous = state;
        }
        assert!(flying > 8 && caught, "g {}: {flying}", scale.g);
        // Caught along the line, it hangs where it was before.
        assert!(run.state().position.distance_to(hung.position) < 0.01 * scale.r);
    }
}

#[test]
fn a_line_tied_across_a_fall_swings_the_spider_on_at_its_speed() {
    let scale = CLOCK;
    // Dropped in the open over the west mesa, it shoots at the left wall.
    let mut run = Run::new(course(scale), scale, Vec2::new(-40.0, 34.0));
    run.tick(idle());
    let mut previous = run.tick(SpiderIntent {
        shoot: Some(Vec2::new(-1.0, 0.0)),
        ..idle()
    });
    let tied = loop {
        let state = run.tick(idle());
        if state.line.is_some() {
            break state;
        }
        assert!(run.ticks < 30);
        previous = state;
    };
    // The line runs across the fall, so tying it takes none of the speed:
    // the spider swings on toward the wall as fast as gravity made it.
    let line = tied.line.unwrap();
    assert_eq!(line.collider.entity, LEFT_WALL);
    let falling = previous.velocity + Vec2::new(0.0, -scale.g) * DT;
    assert!(falling.length() > 5.0 * scale.r, "{falling:?}");
    assert!(
        (tied.velocity.length() / falling.length() - 1.0).abs() < 0.01,
        "{:?} after {:?}",
        tied.velocity,
        falling
    );
    let state = run.ticks(30, idle());
    assert_eq!(state.mode, SpiderMode::Hanging);
    assert!(state.velocity.x < -2.0 * scale.r, "{:?}", state.velocity);
}

/// Fall until falling faster than `speed` radii per second, then shoot
/// straight up at the ceiling. The states on the tick before the line ties
/// and on the tick it ties.
fn tie_while_falling(run: &mut Run, speed: f32) -> (SpiderState, SpiderState) {
    let r = run.scale.r;
    run.until(120, idle(), |s| s.velocity.y < -speed * r)
        .unwrap();
    let mut previous = run.tick(shoot(Vec2::Y));
    loop {
        let state = run.tick(idle());
        if state.line.is_some() {
            return (previous, state);
        }
        assert!(run.ticks < 240);
        previous = state;
    }
}

#[test]
fn a_line_tied_across_a_fall_away_from_its_anchor_gives_to_catch_the_spider() {
    for scale in [CLOCK, METRES] {
        let r = scale.r;
        for damping in [0.7, 0.3] {
            let spec = SpiderSpec {
                silk_damping: damping,
                ..SpiderSpec::new(r, scale.g)
            };
            let mut run = Run::with_spec(course(scale), scale, Vec2::new(0.0, 36.0), spec);
            let (previous, tied) = tie_while_falling(&mut run, 30.0);
            // Tying takes nothing off the fall: through the tick it ties on,
            // only gravity changes the spider.
            let falling = previous.velocity + Vec2::new(0.0, -scale.g) * DT;
            assert!(
                (tied.velocity - falling).length() < 1e-3 * r,
                "{:?} after {:?}",
                tied.velocity,
                falling
            );
            // The line gives, so the next tick slows the spider without
            // stopping it.
            let next = run.tick(idle());
            assert!(
                next.velocity.y < 0.0 && next.velocity.y > tied.velocity.y,
                "{:?} after {:?}",
                next.velocity,
                tied.velocity
            );
            let mut peak = next.line.unwrap().stretch;
            let mut stopped = None;
            let mut highest = f32::MIN;
            let mut state = next;
            for _ in 0..240 {
                state = run.tick(idle());
                peak = peak.max(state.line.unwrap().stretch);
                if state.velocity.y >= 0.0 {
                    stopped.get_or_insert(run.ticks);
                }
                if stopped.is_some() {
                    highest = highest.max(state.position.y);
                }
            }
            // It drew the line out well past its weight's stretch, and no
            // further than the line gives.
            assert!(
                peak > 2.0 * spec.silk_stretch * r && peak < spec.silk_give * r,
                "{}",
                peak / r
            );
            // Then it hangs on the line, stretched by its weight alone.
            let line = state.line.unwrap();
            assert_eq!(state.mode, SpiderMode::Hanging);
            assert!(state.velocity.length() < 1e-2 * r, "{:?}", state.velocity);
            assert!(
                (line.stretch / r - spec.silk_stretch).abs() < 1e-3,
                "{}",
                line.stretch / r
            );
            assert!(
                (line.tension / run.weight() - 1.0).abs() < 0.02,
                "{}",
                line.tension / run.weight()
            );
            // At the default damping it springs back barely past where it
            // comes to hang. Damped less, it bounces well past that.
            let rise = (highest - state.position.y) / r;
            if damping == 0.7 {
                assert!(rise < 0.05, "{rise}");
            } else {
                assert!(rise > 0.5, "{rise}");
            }
        }
    }
}

#[test]
fn a_line_drawn_out_to_its_give_holds_fast() {
    let scale = CLOCK;
    let r = scale.r;
    // A line that gives no more than half a radius, caught on a fall that
    // would draw it out further.
    let spec = SpiderSpec {
        silk_give: 0.5,
        ..SpiderSpec::new(r, scale.g)
    };
    let mut run = Run::with_spec(course(scale), scale, Vec2::new(0.0, 36.0), spec);
    let (_, tied) = tie_while_falling(&mut run, 30.0);
    let length = tied.line.unwrap().length;
    let mut peak = 0.0_f32;
    for _ in 0..60 {
        let line = run.tick(idle()).line.unwrap();
        assert!(line.distance < length + 0.501 * r, "{}", line.stretch / r);
        peak = peak.max(line.stretch);
    }
    assert!(peak > 0.45 * r, "{}", peak / r);
    assert!(run.state().velocity.length() < 1e-2 * r);
}

#[test]
fn a_stretched_line_pulls_back_but_never_pushes() {
    let scale = CLOCK;
    let r = scale.r;
    let mut run = hanging(scale);
    assert!(run.state().line.unwrap().stretch > 0.2 * r);
    // Flung toward its anchor faster than the line springs back, the spider
    // feels no push from it, only gravity.
    let body = run.spider.body();
    assert!(
        run.world
            .set_velocity(body, Vec2::new(0.0, 4.0 * r), 0.0, true)
    );
    let state = run.tick(idle());
    let line = state.line.unwrap();
    assert!(line.stretch > 0.0, "{}", line.stretch / r);
    assert_eq!(line.tension, 0.0);
    assert!(
        (state.velocity.y - (4.0 * r - scale.g * DT)).abs() < 1e-3 * r,
        "{:?}",
        state.velocity
    );
}

#[test]
fn a_shot_at_a_support_moved_since_the_last_step_ties_to_it_where_it_is() {
    let scale = CLOCK;
    let r = scale.r;
    let mut world = course(scale);
    block(
        &mut world,
        scale,
        SHELF,
        Vec2::new(0.0, 20.0),
        Vec2::new(3.0, 1.0),
    );
    let mut run = Run::new(world, scale, Vec2::new(0.0, 1.0));
    run.ticks(5, grip());
    // The host sets the shelf aside; rays still find it where the last step
    // left it, straight overhead.
    let shelf = BodyId::new(SHELF, BodyRole::PRIMARY);
    assert!(
        run.world
            .set_pose(shelf, Vec2::new(10.0, 20.0) * r, 0.0, true)
    );
    run.tick(shoot(Vec2::Y));
    run.until(30, grip(), |s| s.line.is_some()).unwrap();
    // Tied to the middle of its underside, where the shelf now is.
    let line = run.state().line.unwrap();
    assert_eq!(line.collider.entity, SHELF);
    assert!(
        (line.local_anchor - Vec2::new(0.0, -r)).length() < 1e-3 * r,
        "{:?}",
        line.local_anchor / r
    );
    assert!((line.anchor - Vec2::new(10.0, 19.0) * r).length() < 1e-3 * r);
}

#[test]
fn a_reel_weaker_than_the_weight_holds_on_but_cannot_climb() {
    let scale = CLOCK;
    let spec = SpiderSpec {
        reel_pull: 0.8,
        ..SpiderSpec::new(scale.r, scale.g)
    };
    let mut run = Run::with_spec(course(scale), scale, Vec2::new(0.0, 20.0), spec);
    let hung = run.hang_from_ceiling();
    let state = run.ticks(120, reel(1.0));
    assert!(state.position.distance_to(hung.position) < 0.01 * scale.r);
    let (line, was) = (state.line.unwrap(), hung.line.unwrap());
    assert!((line.length - was.length).abs() < 0.01 * scale.r);
    // The line carries the weight the reel cannot lift.
    assert!(
        (line.tension / run.weight() - 1.0).abs() < 0.02,
        "{}",
        line.tension / run.weight()
    );
    assert!(
        state.velocity.length() < 1e-3 * scale.r,
        "{:?}",
        state.velocity
    );
}

#[test]
fn paying_out_lengthens_the_line_at_its_speed_up_to_the_longest() {
    let scale = CLOCK;
    let spec = SpiderSpec {
        max_line: 25.0,
        ..SpiderSpec::new(scale.r, scale.g)
    };
    let mut run = Run::with_spec(course(scale), scale, Vec2::new(0.0, 20.0), spec);
    let start = run.hang_from_ceiling().line.unwrap().length;
    let state = run.ticks(12, reel(-1.0));
    let paid = (state.line.unwrap().length - start) / scale.r;
    assert!((paid - 12.0 * 10.0 * DT).abs() < 1e-3, "{paid}");
    // Gravity takes the spider down after it.
    assert!(state.velocity.y < -scale.r);
    // At the longest it lets out no more, and the spider hangs from it.
    let state = run.ticks(120, reel(-1.0));
    let line = state.line.unwrap();
    assert_eq!(line.length, 25.0 * scale.r);
    assert!(
        (line.stretch / scale.r - spec.silk_stretch).abs() < 1e-3,
        "{}",
        line.stretch / scale.r
    );
}

#[test]
fn reeling_in_a_slack_line_gathers_it_without_a_pull() {
    let scale = CLOCK;
    let r = scale.r;
    // Standing on the floor, tied to the ceiling, with five radii let out.
    let mut run = Run::new(course(scale), scale, Vec2::new(0.0, 1.0));
    run.ticks(2, grip());
    run.tick(shoot(Vec2::Y));
    run.until(30, grip(), |s| s.line.is_some()).unwrap();
    let state = run.ticks(
        30,
        SpiderIntent {
            reel: -1.0,
            ..grip()
        },
    );
    assert!((state.line.unwrap().slack / r - 5.0).abs() < 0.01);
    for _ in 0..30 {
        let before = run.state().line.unwrap();
        let state = run.tick(SpiderIntent {
            reel: 1.0,
            ..grip()
        });
        let gathered = (before.length - state.line.unwrap().length) / r;
        assert!((gathered - 6.0 * DT).abs() < 1e-4, "{gathered}");
        assert!(state.velocity.length() < 1e-3 * r, "{:?}", state.velocity);
    }
}

#[test]
fn a_cut_line_lets_the_spider_fly_on_as_it_was_going() {
    for scale in [CLOCK, METRES] {
        let mut run = hanging(scale);
        let body = run.spider.body();
        assert!(
            run.world
                .set_velocity(body, Vec2::new(3.0 * scale.r, 0.0), 0.0, true)
        );
        let before = run.ticks(5, idle());
        let state = run.tick(SpiderIntent {
            cut: true,
            ..idle()
        });
        assert_eq!(state.line, None);
        assert_eq!(
            state.last_release,
            Some(SpiderRelease {
                hold: SpiderHold::Line,
                reason: SpiderReleaseReason::Requested,
            })
        );
        assert_eq!(run.world.joint_count(), 0);
        // Only gravity changed its velocity on the tick it let go.
        let change = (state.velocity - before.velocity) / DT;
        assert!(
            (change - Vec2::new(0.0, -scale.g)).length() < 1e-3 * scale.g,
            "{change:?}"
        );
        assert_balanced(&state);
    }
}

#[test]
fn each_new_line_replaces_the_last_and_every_one_is_retired_once() {
    let scale = CLOCK;
    let mut run = hanging(scale);
    let mut joints = vec![run.state().line.unwrap().joint];
    for i in 0..6_u64 {
        let side = if i % 2 == 0 { 0.4 } else { -0.4 };
        run.tick(SpiderIntent {
            shoot: Some(Vec2::new(side, 1.0)),
            ..idle()
        });
        run.until(30, idle(), |s| s.shot.is_none()).unwrap();
        let state = run.state();
        let line = state.line.unwrap();
        assert!(!joints.contains(&line.joint), "{:?} reused", line.joint);
        assert!(joints.iter().all(|old| !run.world.contains_joint(*old)));
        joints.push(line.joint);
        assert_eq!(run.world.joint_count(), 1);
        assert_eq!(state.counts.lines_tied, i + 2);
        assert_eq!(state.counts.lines_retired, i + 1);
        assert_eq!(
            state.last_release,
            Some(SpiderRelease {
                hold: SpiderHold::Line,
                reason: SpiderReleaseReason::Replaced,
            })
        );
        assert_balanced(&state);
    }
}

#[test]
fn a_solid_coming_between_the_spider_and_its_anchor_lets_the_line_go() {
    let scale = CLOCK;
    let mut run = hanging(scale);
    let hung = run.state();
    let middle = (hung.position + hung.line.unwrap().anchor) / 2.0 / scale.r;
    block(&mut run.world, scale, PANE, middle, Vec2::new(3.0, 0.5));
    // The world indexes a new solid in its next step, and the span is
    // checked against it from the tick after.
    run.tick(idle());
    let state = run.tick(idle());
    assert_eq!(state.line, None);
    assert_eq!(
        state.last_release,
        Some(SpiderRelease {
            hold: SpiderHold::Line,
            reason: SpiderReleaseReason::Obstructed,
        })
    );
    assert_eq!(run.world.joint_count(), 0);
    assert_balanced(&state);
}

#[test]
fn a_line_whose_anchor_leaves_lets_go_and_never_ties_to_what_replaces_it() {
    let scale = CLOCK;
    for replace in [false, true] {
        let mut world = course(scale);
        block(
            &mut world,
            scale,
            SHELF,
            Vec2::new(0.0, 30.0),
            Vec2::new(6.0, 1.0),
        );
        let mut run = Run::new(world, scale, Vec2::new(0.0, 20.0));
        let hung = run.hang_from_ceiling();
        assert_eq!(hung.line.unwrap().collider.entity, SHELF);
        assert!(run.world.remove_entity(SHELF));
        if replace {
            block(
                &mut run.world,
                scale,
                SHELF,
                Vec2::new(0.0, 30.0),
                Vec2::new(6.0, 1.0),
            );
        }
        let state = run.tick(idle());
        let context = format!("replaced {replace}");
        assert_eq!(state.line, None, "{context}");
        assert_eq!(
            state.last_release,
            Some(SpiderRelease {
                hold: SpiderHold::Line,
                reason: SpiderReleaseReason::Removed,
            }),
            "{context}"
        );
        // Nothing pins it where the line was: it falls.
        let change = (state.velocity - hung.velocity) / DT;
        assert!(
            (change - Vec2::new(0.0, -scale.g)).length() < 1e-3 * scale.g,
            "{context}"
        );
        let state = run.ticks(30, idle());
        assert_eq!(state.counts.lines_tied, 1, "{context}");
        assert_eq!(run.world.joint_count(), 0, "{context}");
        assert_balanced(&state);
    }
}

fn weightless(scale: Scale) -> PhysicsWorld {
    PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        length_unit: scale.unit,
        ..PhysicsWorldConfig::default()
    })
}

/// The spider's and another body's momentum together.
fn momentum(run: &Run, other: BodyId) -> Vec2 {
    let body =
        run.world.motion(other).unwrap().linear_velocity * run.world.body_mass(other).unwrap();
    run.state().velocity * run.mass() + body
}

#[test]
fn reeling_in_a_free_block_draws_both_together_and_conserves_momentum() {
    let scale = CLOCK;
    let block_body = BodyId::new(SHELF, BodyRole::PRIMARY);
    let mut world = weightless(scale);
    block_of(
        &mut world,
        scale,
        SHELF,
        Vec2::new(0.0, 20.0),
        Vec2::new(4.0, 1.0),
        BodyKind::Dynamic,
    );
    let mut run = Run::new(world, scale, Vec2::ZERO);
    run.tick(idle());
    run.tick(SpiderIntent {
        shoot: Some(Vec2::Y),
        ..idle()
    });
    run.until(30, idle(), |s| s.line.is_some()).unwrap();
    let start = run.state().line.unwrap().distance;
    let scale_of_momentum = run.mass() * scale.r;
    for _ in 0..60 {
        run.tick(reel(1.0));
        assert!(momentum(&run, block_body).length() < 1e-3 * scale_of_momentum);
    }
    let gap = run.state().line.unwrap().distance;
    assert!(
        gap < start - 4.0 * scale.r,
        "{} of {}",
        gap / scale.r,
        start / scale.r
    );
    assert!(run.world.motion(block_body).unwrap().linear_velocity.y < 0.0);
}

#[test]
fn a_line_giving_to_a_free_block_draws_it_along_and_conserves_momentum() {
    let scale = CLOCK;
    let block_body = BodyId::new(SHELF, BodyRole::PRIMARY);
    let mut world = weightless(scale);
    block_of(
        &mut world,
        scale,
        SHELF,
        Vec2::new(0.0, 20.0),
        Vec2::new(4.0, 1.0),
        BodyKind::Dynamic,
    );
    let mut run = Run::new(world, scale, Vec2::ZERO);
    run.tick(idle());
    run.tick(SpiderIntent {
        shoot: Some(Vec2::Y),
        ..idle()
    });
    run.until(30, idle(), |s| s.line.is_some()).unwrap();
    // Flung away from the block, the spider draws the line out, and the line
    // pulls the two together until they move as one.
    let body = run.spider.body();
    assert!(
        run.world
            .set_velocity(body, Vec2::new(0.0, -20.0 * scale.r), 0.0, true)
    );
    let start = momentum(&run, block_body);
    let scale_of_momentum = run.mass() * scale.r;
    let mut peak = 0.0_f32;
    for _ in 0..60 {
        let state = run.tick(idle());
        peak = peak.max(state.line.unwrap().stretch);
        assert!((momentum(&run, block_body) - start).length() < 1e-3 * scale_of_momentum);
    }
    assert!(peak > 0.25 * scale.r, "{}", peak / scale.r);
    assert!(run.world.motion(block_body).unwrap().linear_velocity.y < 0.0);
}

#[test]
fn crawling_on_a_free_plank_pushes_it_back_and_conserves_momentum() {
    let scale = CLOCK;
    let plank = BodyId::new(SHELF, BodyRole::PRIMARY);
    let mut world = weightless(scale);
    block_of(
        &mut world,
        scale,
        SHELF,
        Vec2::ZERO,
        Vec2::new(10.0, 0.5),
        BodyKind::Dynamic,
    );
    world.step(DT);
    let mut run = Run::new(world, scale, Vec2::new(0.0, 1.6));
    run.until(10, grip(), |s| s.support.is_some()).unwrap();
    let scale_of_momentum = run.mass() * scale.r;
    for _ in 0..60 {
        let state = run.tick(crawl(1.0));
        assert_eq!(state.support.unwrap().collider.entity, SHELF);
        assert!(momentum(&run, plank).length() < 1e-3 * scale_of_momentum);
    }
    assert!(run.state().velocity.x > 0.0);
    assert!(run.world.motion(plank).unwrap().linear_velocity.x < 0.0);
}

#[test]
fn a_line_tied_to_a_moving_beam_goes_along_with_it() {
    let scale = CLOCK;
    let r = scale.r;
    let mut world = world(scale);
    let velocity = Vec2::new(2.0 * r, 0.0);
    assert!(world.insert_body(
        BodyId::new(SHELF, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::KinematicVelocity,
            position: Vec2::new(0.0, 30.0) * r,
            linear_velocity: velocity,
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(SHELF, ColliderRole::PRIMARY, 0),
            40.0 * r,
            r,
        )],
    ));
    let mut run = Run::new(world, scale, Vec2::new(0.0, 20.0));
    let tied = run.hang_from_ceiling().line.unwrap();
    assert_eq!(tied.collider.entity, SHELF);
    let state = run.ticks(120, idle());
    let line = state.line.unwrap();
    // The anchor rides the beam and the line keeps its length.
    assert_eq!(line.local_anchor, tied.local_anchor);
    assert!((line.anchor - tied.anchor - velocity * (120.0 * DT)).length() < 1e-3 * r);
    assert!((line.length - tied.length).abs() < 1e-4 * r);
    // The spider trails along beneath it, swinging.
    assert!((state.position.x - line.anchor.x).abs() < 2.0 * r);
}
