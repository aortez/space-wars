use super::*;
use crate::floor::responsive::FloorShape;

fn player(opening: f64, direction: f32) -> DuckEvent {
    let layout = Layout::new(4.0 / 3.0);
    let mut floor = ResponsiveFloor::new(FloorShape::clock(layout), 0.0);
    floor.opening = opening;
    DuckEvent::new_responsive_player(layout, 42, 1, 1, direction, floor, None)
}

#[test]
fn moving_panel_grounding_and_single_edge_jump_work_in_both_directions() {
    for direction in [-1.0, 1.0] {
        let mut duck = player(0.9, direction);
        for _ in 0..90 {
            duck.step();
        }
        assert!(duck.grounded());
        let before = duck.responsive_floor().unwrap().opening;
        let start_x = duck.position().unwrap().x;
        let mut grounded = 0;
        for _ in 0..180 {
            duck.step();
            grounded += usize::from(duck.grounded());
            assert_eq!(duck.physics_counts(), (4, 4));
            let p = duck.position().unwrap();
            let x = duck.render_position(p).x;
            let floor = duck.responsive_floor().unwrap();
            let bed = floor.shape.surface_y(f64::from(x), floor.opening) as f32;
            assert!((p.y - bed - duck.radius).abs() < duck.radius * 0.1);
        }
        assert!(grounded > 170);
        assert!(
            duck.position().unwrap().x > start_x + 1.0,
            "neutral rides the closing panel"
        );
        assert!(duck.responsive_floor().unwrap().opening < before);
        duck.set_player_input(0, true);
        duck.step();
        assert_eq!(duck.jumps, 1);
        assert!(
            duck.world
                .as_ref()
                .unwrap()
                .motion(DUCK_BODY)
                .unwrap()
                .linear_velocity
                .y
                > 40.0
        );
        for _ in 0..120 {
            duck.step();
        }
        assert!(duck.grounded());
        assert_eq!(duck.jumps, 1, "held button cannot auto-hop");
        duck.set_player_input(0, false);
        duck.step();
        duck.set_player_input(0, true);
        duck.step();
        assert_eq!(duck.jumps, 2);
    }
}

#[test]
fn a_player_standing_on_the_closed_seam_does_not_open_a_dry_floor() {
    let mut duck = player(0.0, 1.0);
    duck.spawn_motion = Some((
        Vec2::new(0.0, duck.layout.floor_y + duck.radius * 1.05),
        Vec2::ZERO,
    ));
    duck.spawn();
    duck.enter(EventPhase::Running);
    for _ in 0..600 {
        duck.step();
    }
    assert_eq!(duck.responsive_floor().unwrap().opening, 0.0);
    assert!(duck.grounded());
}

#[test]
fn dry_recovery_holds_an_occupied_drain_until_the_character_leaves_the_view() {
    let mut duck = player(0.75, 1.0);
    duck.spawn_motion = Some((
        Vec2::new(0.0, duck.layout.floor_y - duck.radius * 2.0),
        Vec2::ZERO,
    ));
    duck.spawn();
    duck.enter(EventPhase::Exiting);
    for _ in 0..180 {
        duck.step();
        assert!(duck.responsive_floor().unwrap().opening >= 0.75);
        if duck.outcome.is_some() {
            break;
        }
    }
    assert_eq!(duck.outcome, Some(ClockDuckOutcome::Fell));
    assert!(duck.responsive_floor().unwrap().clearance_holds > 0);
    assert_eq!(duck.physics_counts(), (0, 0));
}
