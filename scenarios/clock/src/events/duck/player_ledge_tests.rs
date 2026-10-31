use super::*;
use engine_common::{
    ClockDuckFlapHold, ClockDuckLedgePhase as Phase, ClockDuckLedgeReason as Reason,
};

fn ready(aspect: f32, direction: f32) -> TestDuck {
    let mut duck = TestDuck::new_player(Layout::new(aspect), 42, None, 1, 1);
    duck.face(direction);
    for _ in 0..OPENING_TICKS {
        duck.step();
    }
    duck
}

// Ordinary spawn and controls only: no actuator commands or body placement.
fn jump_to_flap(duck: &mut TestDuck, hold: bool) {
    for _ in 0..65 {
        let steer = if duck.position().unwrap().x > duck.radius * 5.6 {
            (-duck.direction * 300.0) as i16
        } else {
            0
        };
        let jump = if duck.jumps == 0 {
            duck.grounded()
        } else {
            hold
        };
        duck.set_player_input(steer, jump, false, false);
        duck.step();
        if duck.ledge_state().phase == Phase::Hanging {
            return;
        }
    }
}

#[test]
fn player_hold_catches_release_hangs_and_fresh_press_climbs_without_auto_hop() {
    for aspect in [5.0 / 3.0, 4.0 / 3.0, 0.6] {
        for direction in [-1.0, 1.0] {
            let mut duck = ready(aspect, direction);
            jump_to_flap(&mut duck, true);
            assert_eq!(
                duck.ledge_state().phase,
                Phase::Hanging,
                "{:?}",
                duck.diagnostics()
            );
            assert_eq!(duck.jumps, 1);
            let anchor = duck.ledge_anchor();
            for _ in 0..120 {
                duck.step();
                assert_eq!(duck.ledge_state().phase, Phase::Hanging);
                assert_eq!(duck.ledge_state().climbs, 0, "launch hold cannot climb");
            }
            let counts = (
                duck.world.as_ref().unwrap().body_count(),
                duck.world.as_ref().unwrap().collider_count(),
            );
            duck.set_player_input(0, false, false, false);
            for _ in 0..3_600 {
                duck.step();
                assert_eq!(duck.ledge_state().phase, Phase::Hanging);
                assert_eq!(duck.ledge_anchor(), anchor);
                assert_eq!(
                    duck.diagnostics().flaps[0].hold,
                    Some(ClockDuckFlapHold::Attachment)
                );
                assert!(duck.ledge_state().clearance_queries <= 64);
                assert_eq!(
                    (
                        duck.world.as_ref().unwrap().body_count(),
                        duck.world.as_ref().unwrap().collider_count()
                    ),
                    counts
                );
            }
            duck.set_player_input(0, true, false, false);
            for _ in 0..150 {
                duck.step();
                if duck.ledge_state().climbs == 1 {
                    break;
                }
            }
            assert_eq!(duck.ledge_state().climbs, 1);
            assert!(duck.grounded());
            for _ in 0..30 {
                duck.step();
            }
            assert_eq!(duck.jumps, 1, "climb hold cannot become a landing jump");
            duck.set_player_input(0, false, false, false);
            duck.step();
            duck.set_player_input((direction * 1000.0) as i16, true, false, false);
            duck.step();
            assert_eq!(duck.jumps, 2, "a fresh grounded press still jumps");
        }
    }
}

#[test]
fn player_releasing_jump_before_the_edge_does_not_catch() {
    for direction in [-1.0, 1.0] {
        let mut duck = ready(5.0 / 3.0, direction);
        jump_to_flap(&mut duck, false);
        assert_eq!(duck.jumps, 1);
        assert_eq!(duck.ledge_state().catches, 0);
    }
}

#[test]
fn player_down_beats_a_fresh_climb_and_held_jump_cannot_cancel_the_drop() {
    for release_first in [false, true] {
        let mut duck = ready(5.0 / 3.0, 1.0);
        jump_to_flap(&mut duck, true);
        assert_eq!(duck.ledge_state().phase, Phase::Hanging);
        if release_first {
            duck.set_player_input(0, false, false, false);
            duck.step();
        }
        let before = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
        duck.set_player_input(0, true, false, true);
        duck.step();
        let after = duck.world.as_ref().unwrap().motion(DUCK_BODY).unwrap();
        assert_eq!(duck.ledge_state().reason, Some(Reason::Released));
        assert_eq!(duck.ledge_state().climbs, 0);
        assert_eq!(duck.ledge_state().releases, 1);
        assert!(
            (after.linear_velocity.y - before.linear_velocity.y + duck.movement.gravity * DT).abs()
                < 0.001
        );
        for _ in 0..40 {
            duck.set_player_input(0, true, false, false);
            duck.step();
            assert_eq!(duck.ledge_state().catches, 1);
            assert!(duck.ledge_state().rearm_required);
            assert_eq!(duck.ledge_state().releases, 1);
            assert_eq!(duck.jumps, 1);
        }
        duck.set_player_input(0, false, false, false);
        duck.step();
        assert!(!duck.ledge_state().rearm_required);
    }
}
