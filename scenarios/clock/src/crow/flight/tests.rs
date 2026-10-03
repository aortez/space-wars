use super::*;

#[test]
fn downstrokes_accelerate_upward_and_recovery_loses_lift() {
    let layout = Layout::new(4.0 / 3.0);
    let mut position = Vec2::ZERO;
    let mut flight = Flight::default();
    flight.retarget(position, true, layout);
    let initial = flight.velocity.y;
    for _ in 0..RAISE_TICKS + DOWN_TICKS {
        flight.step(&mut position, Vec2::new(0.0, 100.0), 100.0, layout, false);
    }
    let downstroke = flight.velocity.y;
    assert!(downstroke > initial + layout.pitch * 2.0);
    assert!(flight.wing < -0.9);
    for _ in 0..RECOVER_TICKS {
        flight.step(&mut position, Vec2::new(0.0, 100.0), 100.0, layout, false);
    }
    assert!(flight.velocity.y < downstroke - layout.pitch);
    assert_eq!(flight.wing, 0.0);
}

#[test]
fn hard_touchdown_cannot_tunnel_through_the_intended_perch() {
    let layout = Layout::new(4.0 / 3.0);
    let mut position = Vec2::new(0.0, 0.01);
    let mut flight = Flight {
        velocity: Vec2::new(0.0, -layout.pitch * 8.0),
        leg: Leg::Land,
        stroke: Some(Stroke {
            tick: STROKE_TICKS - 1,
            impulse: 0.0,
            amplitude: 0.0,
        }),
        ..Default::default()
    };
    assert!(!flight.step(&mut position, Vec2::ZERO, 100.0, layout, false));
    assert_eq!(position.y, 0.0);
    assert!(flight.velocity.y >= 0.0);
    let mut landed = false;
    for _ in 0..120 {
        landed = flight.step(&mut position, Vec2::ZERO, 100.0, layout, false);
        assert!(position.y >= 0.0);
        if landed {
            break;
        }
    }
    assert!(landed);
}

#[test]
fn airborne_retarget_preserves_momentum_and_stroke_phase() {
    let layout = Layout::new(4.0 / 3.0);
    let mut flight = Flight::default();
    let mut position = Vec2::new(-100.0, 20.0);
    flight.retarget(position, true, layout);
    for _ in 0..43 {
        flight.step(&mut position, Vec2::new(100.0, 0.0), 80.0, layout, false);
    }
    let before = flight;
    flight.retarget(position, false, layout);
    assert_eq!(flight.velocity, before.velocity);
    assert_eq!(flight.stroke, before.stroke);
    assert_eq!(flight.wing, before.wing);
}

#[test]
fn cruising_has_visible_rises_and_unpowered_gliding_gaps() {
    let layout = Layout::new(4.0 / 3.0);
    let cruise = layout.pitch * 5.2;
    let mut position = Vec2::new(0.0, cruise);
    let mut flight = Flight::default();
    flight.retarget(position, false, layout);
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    let mut starts = 0;
    let mut gap = 0;
    let mut longest_gap = 0;
    let mut powered_ticks = 0;
    for tick in 0..600 {
        let was_gliding = flight.stroke.is_none();
        flight.step(
            &mut position,
            Vec2::new(10_000.0, cruise),
            cruise,
            layout,
            true,
        );
        if tick < 120 {
            continue;
        }
        low = low.min(position.y);
        high = high.max(position.y);
        if flight.stroke.is_some() {
            starts += usize::from(was_gliding);
            powered_ticks += 1;
            gap = 0;
        } else {
            assert_eq!(flight.wing, 0.0);
            gap += 1;
            longest_gap = longest_gap.max(gap);
        }
    }
    eprintln!(
        "cruise: {} strokes in 8 s, {:.3} cells rise/fall, longest glide {:.3}s, active ticks {} / 480, height {:.3}..{:.3} cells",
        starts,
        (high - low) / layout.pitch,
        longest_gap as f32 / 60.0,
        powered_ticks,
        low / layout.pitch,
        high / layout.pitch
    );
    assert!(starts > 0 && starts <= 20);
    // Keep a readable pulse between the gentle correction and the stronger
    // experiment's 0.404-cell excursion, without returning to constant flapping.
    assert!(high - low > layout.pitch * 0.15);
    assert!(high - low < layout.pitch * 0.30);
    assert!(longest_gap >= 12);
    assert!(low > layout.pitch * 4.4);
}

#[test]
fn landing_brakes_with_a_smaller_visible_stroke_than_a_climb() {
    let layout = Layout::new(4.0 / 3.0);
    let mut landing_position = Vec2::new(0.0, layout.pitch * 0.5);
    let mut climbing_position = landing_position;
    let mut landing = Flight {
        velocity: Vec2::new(0.0, -layout.pitch * 2.6),
        leg: Leg::Land,
        ..Default::default()
    };
    let mut climbing = Flight {
        leg: Leg::Climb,
        ..landing
    };
    for _ in 0..RAISE_TICKS {
        landing.step(
            &mut landing_position,
            Vec2::ZERO,
            layout.pitch * 5.2,
            layout,
            false,
        );
        climbing.step(
            &mut climbing_position,
            Vec2::ZERO,
            layout.pitch * 5.2,
            layout,
            false,
        );
    }
    assert!(landing.wing > 0.0 && landing.wing < climbing.wing * 0.8);
    assert!(landing.stroke.unwrap().impulse < climbing.stroke.unwrap().impulse);
}
