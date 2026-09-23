use super::*;
use engine_common::ClockDuckJumpProfile;

fn at_moment(aspect: f32, direction: f32, profile: ClockDuckJumpProfile, moment: u8) -> DuckEvent {
    let mut duck = DuckEvent::new_platforms(Layout::new(aspect), 42);
    duck.direction = direction;
    duck.select_jump_profile(Some(profile));
    loop {
        let motion = duck.world.as_ref().and_then(|w| w.motion(DUCK_BODY));
        let ready = match moment {
            0 => duck.tick == 15,
            1 => duck.grounded() && motion.is_some_and(|m| m.linear_velocity.x.abs() > 1.0),
            2 => {
                duck.jumps >= 3
                    && !duck.grounded()
                    && motion.is_some_and(|m| m.linear_velocity.y > 1.0)
            }
            3 => {
                duck.jumps >= 3
                    && !duck.grounded()
                    && motion.is_some_and(|m| m.linear_velocity.y < -1.0)
            }
            4 => duck.phase == EventPhase::Exiting,
            _ => unreachable!(),
        };
        if ready {
            return duck;
        }
        assert!(
            duck.tick < DUCK_TICKS && duck.outcome.is_none(),
            "moment {moment}"
        );
        duck.step();
    }
}

#[test]
fn takeover_keeps_every_existing_body_mass_contact_and_clock_in_both_directions() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        for direction in [-1.0, 1.0] {
            for profile in [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing] {
                for moment in 0..5 {
                    let mut duck = at_moment(aspect, direction, profile, moment);
                    let motions = duck.world.as_ref().map(|w| w.motions().collect::<Vec<_>>());
                    let mass = duck.world.as_ref().and_then(|w| w.body_mass(DUCK_BODY));
                    let contacts = duck
                        .world
                        .as_ref()
                        .map(|w| w.surface_contacts(DUCK_COLLIDER).collect::<Vec<_>>());
                    let phase = (duck.phase, duck.phase_tick, duck.tick);
                    let facing = duck.facing();
                    let doors = (
                        duck.entrance_visible(),
                        duck.exit_visible(),
                        duck.door_openness(),
                    );
                    assert!(duck.take_control(7, 2));
                    assert_eq!(duck.player_session(), Some((7, 2)));
                    assert_eq!((duck.phase, duck.phase_tick, duck.tick), phase);
                    assert_eq!(duck.facing(), facing);
                    assert_eq!(
                        (
                            duck.entrance_visible(),
                            duck.exit_visible(),
                            duck.door_openness()
                        ),
                        doors
                    );
                    assert_eq!(
                        duck.world.as_ref().and_then(|w| w.body_mass(DUCK_BODY)),
                        mass
                    );
                    assert_eq!(
                        duck.world
                            .as_ref()
                            .map(|w| w.surface_contacts(DUCK_COLLIDER).collect::<Vec<_>>()),
                        contacts
                    );
                    if let Some(motions) = motions {
                        let world = duck.world.as_ref().unwrap();
                        assert_eq!(
                            world.body_count(),
                            motions.len() + 1,
                            "only add entrance wall"
                        );
                        for record in motions {
                            assert_eq!(world.motion(record.id), Some(record.motion));
                        }
                        assert!(duck.buoyant.is_some(), "same hull is already water-ready");
                    } else {
                        assert!(duck.world.is_none(), "opening does not spawn early");
                    }
                    assert!(!duck.take_control(8, 1), "cannot steal an owned actor");
                    assert!(duck.debug_arc().is_none(), "no stale AI plan overlay");
                }
            }
        }
    }
}

#[test]
fn airborne_takeover_has_one_gravity_step_bounded_braking_and_no_extra_jump() {
    for direction in [-1.0, 1.0] {
        for moment in [2, 3] {
            let mut duck = at_moment(4.0 / 3.0, direction, ClockDuckJumpProfile::Flowing, moment);
            let motion = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
            let jumps = duck.jumps;
            let tick = duck.tick;
            assert!(duck.take_control(1, 1));
            duck.set_player_input(0, true);
            duck.step();
            let after = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
            assert_eq!(duck.tick, tick + 1);
            assert_eq!(duck.jumps, jumps, "an in-air press does not add a jump");
            let acceleration = duck.movement.run_speed * DT * 6.0;
            let expected_x = motion.linear_velocity.x
                + (-motion.linear_velocity.x).clamp(-acceleration, acceleration);
            assert!((after.linear_velocity.x - expected_x).abs() < 0.001);
            assert!(
                (after.linear_velocity.y - (motion.linear_velocity.y - duck.movement.gravity * DT))
                    .abs()
                    < 0.001
            );
            assert!((after.position - motion.position).length() < duck.radius);
        }
    }
}

#[test]
fn grounded_takeover_can_jump_immediately_and_player_does_not_inherit_the_timeout() {
    for direction in [-1.0, 1.0] {
        let mut duck = at_moment(4.0 / 3.0, direction, ClockDuckJumpProfile::Careful, 1);
        let start = duck.position().unwrap();
        let jumps = duck.jumps;
        assert!(duck.take_control(1, 1));
        duck.set_player_input(0, true);
        duck.step();
        assert_eq!(
            duck.jumps,
            jumps + 1,
            "preserved solver support permits a jump"
        );
        assert!(duck.position().unwrap().y > start.y);
        for _ in 0..DUCK_TICKS * 2 {
            duck.step();
        }
        assert_eq!(duck.jumps, jumps + 1, "holding never hops after landing");
        assert!(duck.grounded());
        assert_eq!(duck.outcome, None, "automatic timeout no longer applies");
        duck.dismiss_player();
        assert!(!duck.take_control(2, 2), "do not revive a removed actor");
        for _ in 0..RESET_TICKS {
            duck.step();
        }
        assert!(duck.player_finished());
        assert_eq!(duck.physics_counts(), (0, 0));
    }
}
