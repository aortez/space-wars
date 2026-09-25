//! Real solver regression: a settled foreign body is not a free-fall path to
//! the course underneath it. Do not rely on event cleanup to unstick the duck.
use super::*;
use engine_common::{ClockDuckCoursePattern, ClockDuckJumpProfile};

fn perched(
    profile: ClockDuckJumpProfile,
    direction: f32,
    half_width: f32,
) -> (DuckEvent, BodyId, f32) {
    let mut duck = DuckEvent::new_platforms(Layout::new(800.0 / 480.0), 42);
    duck.direction = direction;
    duck.controller.profile = profile;
    let width = duck.width;
    duck.course = Some(planner::Course {
        surfaces: [(0.0, 0.4), (0.48, 0.64), (0.72, 1.0)]
            .map(|(start, end)| planner::Surface {
                start: start * width,
                end: end * width,
                height: 0.0,
            })
            .to_vec(),
        pattern: ClockDuckCoursePattern::Platforms,
        attempts: 1,
        fallback: false,
    });
    // Supply already measured capabilities; the regression concerns recovery,
    // not whether a duck can calibrate while perched on a foreign body.
    for _ in 0..9 {
        duck.controller.heights.push(duck.movement.jump_height);
        duck.controller
            .flight_ticks
            .push(2.0 * (2.0 * duck.movement.jump_height / duck.movement.gravity).sqrt() / DT);
        duck.controller.speeds.push(duck.movement.run_speed);
        duck.controller
            .accelerations
            .push(duck.movement.run_speed * 6.0);
    }
    duck.spawn();
    duck.enter(EventPhase::Running);
    let radius = duck.radius;
    let x = width * 0.2;
    let block_center = duck.physics_position(Vec2::new(x, duck.layout.floor_y + radius));
    let entity = PhysicsId::new(3001);
    let body = BodyId::new(entity, BodyRole::PRIMARY);
    let mut collider = ColliderSpec::cuboid(
        ColliderId::new(entity, ColliderRole::PRIMARY, 0),
        radius * half_width,
        radius,
    );
    collider.restitution = 0.0;
    let world = duck.world.as_mut().unwrap();
    assert!(world.insert_body(
        body,
        BodySpec {
            position: block_center,
            ..BodySpec::default()
        },
        &[collider],
    ));
    world.set_pose(DUCK_BODY, block_center + Vec2::Y * radius * 2.0, 0.0, true);
    for _ in 0..30 {
        world.step(DT);
    }
    assert!(duck.grounded());
    assert_eq!(duck.supported_surface(), None);
    (duck, body, x)
}

#[test]
fn duck_leaves_settled_debris_and_recovers_without_removing_it() {
    for profile in [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing] {
        for direction in [-1.0, 1.0] {
            for half_width in [1.0, 3.0, 8.0] {
                let (mut duck, debris, start) = perched(profile, direction, half_width);
                for _ in 0..240 {
                    let grounded = duck.grounded();
                    let jumps = duck.jumps;
                    duck.step();
                    assert!(
                        duck.jumps == jumps || grounded,
                        "no airborne rescue impulse"
                    );
                    assert!(duck.outcome.is_none());
                    if duck.controller.recovery.stats.recoveries > 0 {
                        break;
                    }
                }
                assert_eq!(
                    duck.controller.recovery.stats.recoveries,
                    1,
                    "{profile:?} mirror={direction} width={half_width}: {:?} position={:?} debris={:?} contacts={:?}",
                    duck.controller.recovery,
                    duck.position(),
                    duck.world.as_ref().unwrap().motion(debris),
                    duck.world
                        .as_ref()
                        .unwrap()
                        .surface_contacts(DUCK_COLLIDER)
                        .collect::<Vec<_>>()
                );
                assert_eq!(duck.supported_surface(), Some(0));
                assert!(
                    (duck.position().unwrap().x - start).abs() > duck.radius * (half_width + 0.5)
                );
                assert!(duck.world.as_ref().unwrap().motion(debris).is_some());
                assert_eq!(duck.controller.profile, profile);
                assert!(duck.controller.recovery.stats.escape_jumps <= 1);
                let recovered_at = duck.phase_tick;
                for _ in 0..300 {
                    duck.step();
                    if duck.controller.navigator.confirmed > 0 {
                        break;
                    }
                }
                assert!(
                    duck.controller.navigator.confirmed > 0,
                    "resume planned jumps after debris"
                );
                eprintln!(
                    "{profile:?} mirror={direction} half-width={half_width}: recovered at {recovered_at} ticks, {} escape jumps",
                    duck.controller.recovery.stats.escape_jumps
                );
            }
        }
    }
}

#[test]
fn removed_debris_does_not_leave_the_duck_stuck_on_a_phantom_support() {
    let (mut duck, debris, _) = perched(ClockDuckJumpProfile::Careful, 1.0, 8.0);
    for _ in 0..20 {
        duck.step();
    }
    assert!(duck.controller.recovery.stats.active);
    assert!(duck.world.as_mut().unwrap().remove_entity(debris.entity));
    for _ in 0..120 {
        duck.step();
        if duck.controller.recovery.stats.recoveries > 0 {
            break;
        }
    }
    assert_eq!(duck.controller.recovery.stats.recoveries, 1);
    assert_eq!(duck.supported_surface(), Some(0));
}
