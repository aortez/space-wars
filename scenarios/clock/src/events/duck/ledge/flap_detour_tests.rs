use super::*;
use engine_common::{
    ClockDuckFlapDetourOutcome as Outcome, ClockDuckFlapDetourPhase as DetourPhase,
    ClockDuckJumpProfile,
};

pub(super) const ASPECTS: [f32; 3] = [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0];
pub(super) const PROFILES: [ClockDuckJumpProfile; 2] =
    [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing];
const SEED: u64 = 3;

pub(super) fn ordinary(aspect: f32, direction: f32, profile: ClockDuckJumpProfile) -> DuckEvent {
    assert!(flap_detour::selected(SEED));
    let mut duck = DuckEvent::new_platforms(Layout::new(aspect), SEED);
    duck.direction = direction;
    duck.controller.profile = profile;
    duck
}

pub(super) fn until(duck: &mut DuckEvent, predicate: impl Fn(&DuckEvent) -> bool) {
    for _ in 0..400 {
        if predicate(duck) {
            return;
        }
        duck.step();
    }
    panic!("stage not reached: {:?}", duck.diagnostics());
}

pub(super) fn block(duck: &mut DuckEvent, position: Vec2, half: Vec2) {
    let entity = PhysicsId::new(9000);
    assert!(duck.world.as_mut().unwrap().insert_body(
        BodyId::new(entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position,
            ..Default::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            half.x,
            half.y
        )],
    ));
}

fn anchor(duck: &DuckEvent) -> Vec2 {
    let panel = duck.door_panels()[0].unwrap();
    panel.point(Vec2::new(
        panel.hinge.x + panel.inward * panel.width,
        panel.bottom,
    ))
}

#[test]
fn flap_detour_runs_from_ordinary_spawn() {
    for aspect in ASPECTS {
        for direction in [-1.0, 1.0] {
            for profile in PROFILES {
                let mut duck = ordinary(aspect, direction, profile);
                let mut previous = DetourPhase::Pending;
                let mut trace = Vec::new();
                for _ in 0..400 {
                    duck.step();
                    assert!(duck.ledge.detour.stats.clearance_queries <= 64);
                    assert!(duck.ledge.stats.clearance_queries <= 64);
                    assert!(duck.physics_counts().0 <= 8);
                    let phase = duck.ledge.detour.stats.phase;
                    if phase != previous {
                        trace.push((duck.tick, duck.diagnostics()));
                        previous = phase;
                    }
                    if phase == DetourPhase::Finished {
                        break;
                    }
                }
                assert_eq!(
                    duck.ledge.detour.stats.outcome,
                    Some(Outcome::Returned),
                    "{aspect} {direction} {profile:?}: {trace:#?}"
                );
                assert_eq!(duck.ledge.stats.climbs, 1);
                assert_eq!(duck.jumps, 3); // approach, return, then ordinary warm-up
                assert_eq!(duck.controller.heights.count, 0);
                assert_eq!(duck.ledge.detour.stats.attempts, 1);
                assert_eq!(
                    duck.ledge.detour.stats.perch_ticks,
                    flap_detour::PERCH_TICKS
                );
                assert!(duck.ledge.detour.stats.ticks < flap_detour::BUDGET_TICKS);
                assert_eq!(duck.ledge.automatic.stats.hang_ticks, automatic::HANG_TICKS);
                assert_eq!(
                    duck.world.as_ref().unwrap().body_kind(DUCK_BODY),
                    Some(BodyKind::Dynamic)
                );
                while duck.phase != EventPhase::Resetting && duck.tick < DUCK_TICKS {
                    duck.step();
                }
                assert_eq!(
                    duck.outcome,
                    Some(ClockDuckOutcome::Exited),
                    "{aspect} {direction} {profile:?}: {:?}",
                    duck.diagnostics()
                );
                assert_eq!(
                    duck.controller.heights.count, 2,
                    "ordinary calibration resumes after the stunt"
                );
            }
        }
    }
}

#[test]
fn detours_are_seeded_occasional_and_do_not_reroll_the_course() {
    let selected = (0..1000)
        .filter(|seed| flap_detour::selected(*seed))
        .count();
    assert!((160..=240).contains(&selected), "{selected}");
    assert!(!flap_detour::selected(42));
    for seed in 0..40 {
        let mut duck = DuckEvent::new_course(Layout::new(ASPECTS[0]), seed, None);
        let surfaces = duck.course.as_ref().unwrap().surfaces.clone();
        let direction = duck.direction;
        until(&mut duck, |d| {
            d.ledge.detour.stats.phase == DetourPhase::Finished
        });
        assert_eq!(duck.course.as_ref().unwrap().surfaces, surfaces);
        assert_eq!(duck.direction, direction);
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
    }
}

#[test]
fn blocked_perch_and_return_are_refused_without_a_door_claim() {
    for return_block in [false, true] {
        let mut duck = ordinary(ASPECTS[0], 1.0, PROFILES[0]);
        until(&mut duck, |d| d.world.is_some() && d.position().is_some());
        let r = duck.radius;
        let tip = anchor(&duck);
        let point = if return_block {
            tip + Vec2::new(r * 3.8, r * 2.0)
        } else {
            tip + Vec2::new(-r * 1.5, r * 1.5)
        };
        block(&mut duck, point, Vec2::new(r * 0.5, r));
        until(&mut duck, |d| {
            d.ledge.detour.stats.phase == DetourPhase::Finished
        });
        assert_eq!(duck.ledge.detour.stats.outcome, Some(Outcome::Unsafe));
        assert_eq!(duck.ledge.detour.stats.attempts, 0);
        assert_eq!(duck.ledge.stats.catches, 0);
        assert!(!duck.ledge_attached());
        for _ in 0..100 {
            duck.step();
        }
        assert_eq!(duck.door_openness().0, 0.0);
    }
}

#[test]
fn changed_flap_failed_catch_and_late_return_block_end_the_single_attempt() {
    for case in 0..3 {
        let mut duck = ordinary(ASPECTS[0], 1.0, PROFILES[0]);
        until(&mut duck, |d| {
            d.ledge.detour.stats.phase
                == if case == 2 {
                    DetourPhase::Perching
                } else {
                    DetourPhase::Approaching
                }
        });
        let r = duck.radius;
        let tip = anchor(&duck);
        match case {
            0 => duck.phase_tick = 20,
            1 => block(
                &mut duck,
                tip + Vec2::new(-r * 1.5, r * 1.5),
                Vec2::splat(r * 0.5),
            ),
            _ => block(
                &mut duck,
                tip + Vec2::new(r * 2.8, r * 3.0),
                Vec2::new(r * 0.4, r * 6.0),
            ),
        }
        until(&mut duck, |d| {
            d.ledge.detour.stats.phase == DetourPhase::Finished
        });
        assert_eq!(duck.ledge.detour.stats.attempts, 1);
        assert_eq!(
            duck.ledge.detour.stats.outcome,
            Some([Outcome::Expired, Outcome::Interrupted, Outcome::Blocked][case])
        );
        assert!(!duck.ledge_attached());
        assert!(duck.ledge.automatic.stats.attempts <= 1);
    }
}

pub(super) fn flood(duck: &DuckEvent) -> WaterWorld {
    use engine_water::{Boundary, PoolSpec, WaterConfig};
    let mut water = WaterWorld::new(
        WaterConfig::default(),
        vec![PoolSpec {
            left: f64::from(-duck.width * 0.5),
            column_width: f64::from(duck.width) / 64.0,
            bed: vec![f64::from(duck.layout.floor_y); 64],
            boundaries: [Boundary::Closed; 2],
        }],
    )
    .unwrap();
    for i in 0..64 {
        water
            .add_to_pool(
                0,
                f64::from(-duck.width * 0.5) + (i as f64 + 0.5) * f64::from(duck.width) / 64.0,
                f64::from(duck.radius * 12.0 * duck.width) / 64.0,
            )
            .unwrap();
    }
    water
}

#[test]
fn wet_interruption_timeout_departure_and_lost_support_release_the_detour() {
    for case in 0..5 {
        let mut duck = ordinary(ASPECTS[0], 1.0, PROFILES[1]);
        until(&mut duck, |d| d.ledge.stats.phase == Phase::Hanging);
        let mass = duck.world.as_ref().unwrap().body_mass(DUCK_BODY);
        let mut water = None;
        match case {
            0 => water = Some(flood(&duck)),
            1 => duck.ledge.detour.stats.ticks = flap_detour::BUDGET_TICKS - 1,
            2 => duck.tick = OPENING_TICKS + EXIT_DELAY_TICKS,
            3 => {
                duck.world
                    .as_mut()
                    .unwrap()
                    .remove_entity(PhysicsId::new(1000));
            }
            _ => {
                duck.world.as_mut().unwrap().apply_velocity_delta(
                    DUCK_BODY,
                    Vec2::new(duck.radius * 40.0, 0.0),
                    true,
                );
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
                    Outcome::Blocked,
                    Outcome::Interrupted
                ][case]
            ),
            "{:?}",
            duck.diagnostics()
        );
        assert!(!duck.ledge_attached());
        assert!(!duck.ledge.automatic.stats.active);
        assert_eq!(duck.world.as_ref().unwrap().body_mass(DUCK_BODY), mass);
        assert_eq!(duck.controller.heights.count, 0);
    }
}

#[test]
fn takeover_and_arena_retirement_keep_body_ownership_and_clear_autonomous_intent() {
    for stage in [
        DetourPhase::Approaching,
        DetourPhase::Recovering,
        DetourPhase::Perching,
        DetourPhase::Returning,
    ] {
        let mut duck = ordinary(ASPECTS[0], 1.0, PROFILES[0]);
        until(&mut duck, |d| d.ledge.detour.stats.phase == stage);
        let before = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
        let ledge = duck.ledge_state();
        assert!(duck.take_control(9, 1));
        let after = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
        assert_eq!(before.position, after.position);
        assert_eq!(before.linear_velocity, after.linear_velocity);
        assert_eq!(duck.ledge_state(), ledge);
        assert_eq!(duck.ledge.detour.stats.outcome, Some(Outcome::TakenOver));
        let stats = duck.ledge.detour.stats;
        for _ in 0..5 {
            duck.step();
        }
        assert_eq!(duck.ledge.detour.stats, stats);
    }
    let mut duck = ordinary(ASPECTS[0], 1.0, PROFILES[0]);
    until(&mut duck, |d| d.ledge.stats.phase == Phase::Hanging);
    let mut arena = VisitArena::claim(&mut duck);
    arena.retain(Box::new(duck));
    let vacant = arena.vacant().unwrap();
    assert!(!vacant.ledge_attached());
    assert!(vacant.world.as_ref().unwrap().motion(DUCK_BODY).is_none());
    assert_eq!(vacant.door_openness(), (0.0, 0.0));
    let mut rejoined = arena.rejoin(2, 1).unwrap();
    for _ in 0..100 {
        rejoined.step();
    }
    assert_eq!(rejoined.ledge.detour.stats.attempts, 0);
}

#[test]
fn stationary_responsive_support_can_return_but_moving_slopes_are_skipped() {
    use crate::floor::responsive::FloorShape;
    for aspect in ASPECTS {
        for direction in [-1.0, 1.0] {
            let mut duck = ordinary(aspect, direction, PROFILES[0]);
            duck.responsive_floor = Some(ResponsiveFloor::new(FloorShape::clock(duck.layout), 0.0));
            until(&mut duck, |d| {
                d.ledge.detour.stats.phase == DetourPhase::Finished
            });
            assert_eq!(
                duck.ledge.detour.stats.outcome,
                Some(Outcome::Returned),
                "{:?}",
                duck.diagnostics()
            );
            let mut duck = ordinary(aspect, direction, PROFILES[1]);
            let mut floor = ResponsiveFloor::new(FloorShape::clock(duck.layout), 0.0);
            floor.opening = 0.9;
            duck.responsive_floor = Some(floor);
            until(&mut duck, |d| {
                d.ledge.detour.stats.phase == DetourPhase::Finished
            });
            assert!(matches!(
                duck.ledge.detour.stats.outcome,
                Some(Outcome::Unsafe | Outcome::Expired)
            ));
            assert_eq!(duck.ledge.stats.catches, 0);
        }
    }
}
