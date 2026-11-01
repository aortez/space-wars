//! The spider in its host's world: it never steps the world, a stopped clock
//! freezes it, what it owns stays bounded through many cycles, it leaves
//! cleanly, and a cloned world carries on identically.
use super::*;
use crate::world::physics_activity;

/// The counts that must always balance, and what the spider owns in the world
/// besides the course's own bodies.
fn assert_owned(run: &Run, state: &SpiderState, course_bodies: usize) {
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
    assert_eq!(
        counts.catches,
        counts.surface_releases + u64::from(state.support.is_some()),
        "{counts:?}"
    );
    assert!(state.joints <= 1);
    assert_eq!(run.world.joint_count(), state.joints);
    assert_eq!(run.world.body_count(), course_bodies + state.bodies);
}

/// One tick of a brain driving the spider.
fn think(run: &mut Run, brain: &mut GapCrossing) -> SpiderState {
    let state = run.state();
    let intent = brain.intent(&state, run.scale);
    run.tick(intent)
}

#[test]
fn a_zero_or_invalid_step_changes_nothing() {
    let scale = CLOCK;
    // Holding the west mesa, tied to the ceiling, another shot on its way.
    let mut run = Run::new(course(scale), scale, GapCrossing::START);
    run.ticks(5, grip());
    run.tick(shoot(GapCrossing::AIM));
    run.until(30, grip(), |s| s.line.is_some()).unwrap();
    let state = run.tick(shoot(Vec2::new(-1.0, 1.0)));
    assert!(state.support.is_some() && state.line.is_some() && state.shot.is_some());
    let spider = run.spider.clone();
    let activity = physics_activity();
    let everything = SpiderIntent {
        crawl: 1.0,
        grip: false,
        shoot: Some(Vec2::Y),
        cut: true,
        reel: 1.0,
    };
    for dt in [0.0, -DT, f32::NAN, f32::INFINITY] {
        assert!(run.spider.drive(&mut run.world, dt, everything, eligible));
    }
    assert_eq!(run.spider, spider);
    assert_eq!(run.state(), state);
    assert_eq!(physics_activity(), activity);
}

#[test]
fn the_spider_never_steps_the_world_itself() {
    let scale = CLOCK;
    let mut run = Run::new(course(scale), scale, GapCrossing::START);
    let mut brain = GapCrossing::default();
    let start = physics_activity();
    for tick in 0..400 {
        let state = run.state();
        let intent = brain.intent(&state, scale);
        assert!(run.spider.drive(&mut run.world, DT, intent, eligible));
        assert_eq!(physics_activity().steps, start.steps + tick);
        run.world.step(DT);
    }
    assert!(brain.stage >= GapStage::Swing, "{:?}", brain.stage);
    assert_eq!(physics_activity().worlds, start.worlds);
}

#[test]
fn many_cycles_keep_what_it_owns_bounded_and_its_counts_balanced() {
    let scale = CLOCK;
    let mut run = Run::new(course(scale), scale, Vec2::new(0.0, 1.0));
    let bodies = run.world.body_count() - 1;
    // A restless brain: shooting every way in turn, cutting, reeling both
    // ways, letting go and catching again.
    let aims = [
        Vec2::Y,
        Vec2::new(1.0, 1.0),
        Vec2::new(-1.0, 1.0),
        Vec2::X,
        Vec2::new(1.0, 2.0),
    ];
    for tick in 0..3000_u32 {
        let intent = SpiderIntent {
            crawl: if (tick / 120) % 2 == 0 { 1.0 } else { -1.0 },
            grip: tick % 400 < 300,
            shoot: (tick % 37 == 0).then(|| aims[(tick / 37) as usize % aims.len()]),
            cut: tick % 151 == 0,
            reel: [1.0, -1.0, 0.0][(tick / 90) as usize % 3],
        };
        let state = run.tick(intent);
        assert_owned(&run, &state, bodies);
    }
    let counts = run.state().counts;
    assert!(counts.lines_tied > 10, "{counts:?}");
    assert!(counts.shot_failures > 3, "{counts:?}");
    assert!(counts.catches > 3, "{counts:?}");
}

#[test]
fn removing_a_spider_takes_what_it_owns_and_nothing_else() {
    let scale = CLOCK;
    let mut world = course(scale);
    let (entities, bodies, colliders) = (
        world.entity_count(),
        world.body_count(),
        world.collider_count(),
    );
    let spec = SpiderSpec::new(scale.r, scale.g);
    let mut spider =
        Spider::insert(&mut world, SPIDER, GapCrossing::START * scale.r, spec).unwrap();
    let mut tick = |world: &mut PhysicsWorld, intent: SpiderIntent| {
        assert!(spider.drive(world, DT, intent, eligible));
        world.step(DT);
        spider.state(world).unwrap()
    };
    // Holding the mesa, tied to the ceiling, with another shot on its way.
    for _ in 0..5 {
        tick(&mut world, grip());
    }
    tick(&mut world, shoot(Vec2::Y));
    while tick(&mut world, grip()).line.is_none() {}
    let state = tick(&mut world, shoot(Vec2::new(1.0, 1.0)));
    assert!(state.support.is_some() && state.line.is_some() && state.shot.is_some());
    let counts = spider.remove(&mut world);
    assert_eq!(world.entity_count(), entities);
    assert_eq!(world.body_count(), bodies);
    assert_eq!(world.collider_count(), colliders);
    assert_eq!(world.joint_count(), 0);
    assert!(!world.contains_entity(SPIDER));
    for entity in [FLOOR, LEFT_WALL, RIGHT_WALL, CEILING, WEST_MESA, EAST_MESA] {
        assert!(world.contains_entity(entity));
    }
    // Every hold let go, every line retired and every shot ended, once.
    assert_eq!(counts.catches, counts.surface_releases);
    assert_eq!(counts.lines_tied, 1);
    assert_eq!(counts.lines_retired, 1);
    assert_eq!(counts.shots, counts.lines_tied + counts.shot_failures);
    world.step(DT);
}

#[test]
fn a_spider_is_refused_where_it_cannot_be() {
    let scale = CLOCK;
    let r = scale.r;
    let mut world = course(scale);
    // The world indexes its solids in its first step.
    world.step(DT);
    let spec = SpiderSpec::new(r, scale.g);
    let clear = Vec2::new(0.0, 5.0) * r;
    let entities = world.entity_count();
    let refused = [
        ("inside the floor", SPIDER, Vec2::new(0.0, 0.5) * r, spec),
        ("an entity already there", FLOOR, clear, spec),
        ("nowhere", SPIDER, Vec2::new(f32::NAN, 5.0 * r), spec),
        ("no body", SPIDER, clear, SpiderSpec::new(0.0, scale.g)),
        (
            "no gravity to weigh",
            SPIDER,
            clear,
            SpiderSpec::new(r, 0.0),
        ),
        ("no grip", SPIDER, clear, SpiderSpec { grip: 0.0, ..spec }),
        (
            "endless reach",
            SPIDER,
            clear,
            SpiderSpec {
                reach: f32::INFINITY,
                ..spec
            },
        ),
        (
            "a line into the body",
            SPIDER,
            clear,
            SpiderSpec {
                min_line: 0.5,
                ..spec
            },
        ),
        (
            "no line between",
            SPIDER,
            clear,
            SpiderSpec {
                min_line: 60.0,
                ..spec
            },
        ),
    ];
    for (case, entity, at, spec) in refused {
        assert!(
            Spider::insert(&mut world, entity, at, spec).is_none(),
            "{case}"
        );
        assert_eq!(world.entity_count(), entities, "{case}");
    }
    assert!(Spider::insert(&mut world, SPIDER, clear, spec).is_some());
}

#[test]
fn a_cloned_world_and_spider_carry_on_identically() {
    let scale = CLOCK;
    let mut run = Run::new(course(scale), scale, GapCrossing::START);
    let mut brain = GapCrossing::default();
    for _ in 0..90 {
        think(&mut run, &mut brain);
    }
    assert!(run.state().line.is_some());
    let mut copy = Run {
        world: run.world.clone(),
        spider: run.spider.clone(),
        scale,
        ticks: run.ticks,
    };
    let mut copied = brain.clone();
    for _ in 0..300 {
        assert_eq!(think(&mut run, &mut brain), think(&mut copy, &mut copied));
    }
}

#[test]
fn crosses_the_gap_by_swinging_on_a_line_at_both_scales() {
    for scale in [CLOCK, METRES] {
        let mut run = Run::new(course(scale), scale, GapCrossing::START);
        let mut brain = GapCrossing::default();
        let bodies = run.world.body_count() - 1;
        let mut hung = 0;
        let mut lowest = f32::MAX;
        for _ in 0..1500 {
            let state = think(&mut run, &mut brain);
            assert_owned(&run, &state, bodies);
            if brain.stage == GapStage::Swing {
                hung += u32::from(state.mode == SpiderMode::Hanging);
                lowest = lowest.min(state.position.y / scale.r);
            }
            if brain.stage == GapStage::Climb && state.line.is_none() {
                break;
            }
        }
        let state = run.state();
        let context = format!("g {}: {:?} at {:?}", scale.g, brain.stage, run.at());
        // On top of the east mesa, the line cut behind it.
        assert_eq!(brain.stage, GapStage::Climb, "{context}");
        assert_eq!(state.line, None, "{context}");
        let support = state.support.unwrap();
        assert_eq!(support.collider.entity, EAST_MESA, "{context}");
        assert!(support.normal.y > 0.999, "{context}");
        assert!(run.at().x > 13.0 && run.at().y > 16.5, "{context}");
        assert_eq!(
            state.last_release,
            Some(SpiderRelease {
                hold: SpiderHold::Line,
                reason: SpiderReleaseReason::Requested,
            })
        );
        assert_eq!(state.counts.lines_tied, 1);
        // It swung on the line, down into the gap and up the far side.
        assert!(hung > 10, "{context}: {hung}");
        assert!(lowest < 14.0, "{context}: {lowest}");
    }
}

#[test]
fn a_step_without_a_drive_finds_the_spider_unheld_under_its_own_gravity() {
    let scale = CLOCK;
    let r = scale.r;
    // Upside down on the ceiling, its body twice as heavy as the world's
    // gravity makes it, as a host may scale it.
    let mut run = Run::new(course(scale), scale, Vec2::new(0.0, 38.9));
    let body = run.spider.body();
    assert!(run.world.set_gravity_scale(body, 2.0, true));
    run.until(30, grip(), |s| s.support.is_some()).unwrap();
    let held = run.ticks(30, grip());
    assert_eq!(held.support.unwrap().collider.entity, CEILING);
    assert!(held.velocity.length() < 1e-3 * r, "{:?}", held.velocity);
    // Carrying its weight never touched the scale the host gave it.
    assert_eq!(run.world.gravity_scale(body), Some(2.0));
    // The host steps on without driving it: nothing holds it up.
    run.world.step(DT);
    let after = run.state();
    let change = (after.velocity - held.velocity) / DT;
    assert!(
        (change - Vec2::new(0.0, -2.0 * scale.g)).length() < 1e-3 * scale.g,
        "{change:?}"
    );
    for _ in 0..30 {
        run.world.step(DT);
    }
    assert!(run.state().position.y < held.position.y - 10.0 * r);
}

#[test]
fn what_leaves_the_world_reads_as_let_go_before_the_next_drive() {
    let scale = CLOCK;
    let beam = PhysicsId::new(7);
    let beam_at = Vec2::new(0.0, 30.0);
    let half = Vec2::new(3.0, 1.0);
    // Holding the beam from above, hanging from it, and shooting at it.
    for case in ["support", "anchor", "target"] {
        let mut world = course(scale);
        block(&mut world, scale, beam, beam_at, half);
        world.step(DT);
        let mut run = match case {
            "support" => {
                let mut run = Run::new(world, scale, beam_at + Vec2::new(0.0, 2.0));
                run.ticks(20, grip());
                run
            }
            "anchor" => {
                let mut run = Run::new(world, scale, Vec2::new(0.0, 20.0));
                run.hang_from_ceiling();
                run
            }
            _ => {
                let mut run = Run::new(world, scale, Vec2::new(0.0, 1.0));
                run.ticks(2, grip());
                run.tick(shoot(Vec2::Y));
                run
            }
        };
        let before = run.state();
        let held = before.support.map(|s| s.collider.entity);
        let tied = before.line.map(|l| l.collider.entity);
        let aimed = before.shot.map(|s| s.collider.entity);
        let expected = match case {
            "support" => held,
            "anchor" => tied,
            _ => aimed,
        };
        assert_eq!(expected, Some(beam), "{case}");
        // Replaced under the same identities, far away.
        assert!(run.world.remove_entity(beam));
        block(&mut run.world, scale, beam, Vec2::new(40.0, 30.0), half);
        let view = run.state();
        assert_ne!(
            view.support.map(|s| s.collider.entity),
            Some(beam),
            "{case}"
        );
        assert_eq!(view.line, None, "{case}");
        assert_eq!(view.shot, None, "{case}");
        assert_eq!(view.joints, 0, "{case}");
        assert_owned(&run, &view, run.world.body_count() - 1);
        match case {
            "target" => assert_eq!(view.last_failure, Some(SilkFailure::TargetLost)),
            _ => assert_eq!(
                view.last_release.map(|r| r.reason),
                Some(SpiderReleaseReason::Removed),
                "{case}"
            ),
        }
        // The next drive lets go of the same things, once.
        let next = run.tick(grip());
        assert_eq!(next.counts, view.counts, "{case}");
        assert_eq!(next.last_release, view.last_release, "{case}");
        assert_eq!(next.last_failure, view.last_failure, "{case}");
    }
}
