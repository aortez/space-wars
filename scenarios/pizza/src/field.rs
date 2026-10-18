//! The event adds acceleration to the existing Rapier world, never a new solver.

use super::*;
use engine_common::{Fill, RenderPolygon};

/// World units / physics-second². The longest viewport dimension is one unit.
/// A two-second ramp and 0.9 physics-time scale keep the pull usable with flings.
const ACCELERATION: f32 = 0.30;
const RAMP_TICKS: u32 = 2 * FIXED_HZ;
const FADE_START_TICKS: u32 = DIRECTIONAL_GRAVITY_TICKS - RAMP_TICKS;
const CUE_LAYER: i32 = 5;

fn envelope(elapsed: u32) -> f32 {
    let t = if elapsed < RAMP_TICKS {
        elapsed as f32 / RAMP_TICKS as f32
    } else if elapsed <= FADE_START_TICKS {
        1.0
    } else {
        DIRECTIONAL_GRAVITY_TICKS.saturating_sub(elapsed) as f32 / RAMP_TICKS as f32
    }
    .clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn field(event: PizzaEventState) -> (Vec2, f32) {
    let PizzaEventState::Active {
        kind: PizzaEventKind::DirectionalGravity,
        direction,
        elapsed_ticks,
        ..
    } = event
    else {
        return (Vec2::ZERO, 0.0);
    };
    // World coordinates are y-up; Top is the top of every screen orientation.
    let direction = match direction {
        PizzaEventDirection::Left => Vec2::new(-1.0, 0.0),
        PizzaEventDirection::Right => Vec2::new(1.0, 0.0),
        PizzaEventDirection::Top => Vec2::new(0.0, 1.0),
        PizzaEventDirection::Bottom => Vec2::new(0.0, -1.0),
    };
    (direction, envelope(elapsed_ticks))
}

pub(super) fn apply(state: &mut PizzaState) -> bool {
    let (direction, strength) = field(state.event_state());
    if strength == 0.0 {
        return false;
    }
    let acceleration = direction * (ACCELERATION * strength);
    let physics = state.physics.as_mut().expect("normal Pizza owns Rapier");
    for ball in &state.balls {
        if ball.moving && state.held_ball_id != Some(ball.id) {
            // Rapier integrates acceleration over its scaled step and substeps,
            // including the first step after a kinematic held body is released.
            // The normal step clears this force immediately after integration.
            assert!(physics.apply_acceleration(ball_body_id(ball.id), acceleration, true));
        }
    }
    true
}

pub(super) fn render_cue(state: &PizzaState, frame: &mut RenderFrame) {
    let (direction, strength) = field(state.event_state());
    if strength == 0.0 {
        return;
    }
    let bounds = state.config.bounds;
    // A compact badge at the selected edge: 7% of the shorter screen dimension.
    let size = bounds.width.min(bounds.height) * 0.035;
    let center = bounds.center()
        + Vec2::new(
            direction.x * (bounds.width * 0.5 - size * 1.5),
            direction.y * (bounds.height * 0.5 - size * 1.5),
        );
    frame.push_primitive(
        CUE_LAYER,
        RenderPrimitive::Circle(RenderCircle {
            center: RenderPoint::new(center.x, center.y),
            radius: size,
            fill: Some(Fill::new(RenderColor::rgba(
                0.04,
                0.06,
                0.10,
                0.85 * strength,
            ))),
            stroke: Some(Stroke::new(
                RenderColor::rgba(0.65, 0.88, 1.0, strength),
                1.0,
            )),
        }),
    );
    let side = Vec2::new(-direction.y, direction.x);
    let points = [
        (-0.60, -0.18),
        (0.05, -0.18),
        (0.05, -0.48),
        (0.65, 0.0),
        (0.05, 0.48),
        (0.05, 0.18),
        (-0.60, 0.18),
    ]
    .into_iter()
    .map(|(along, across)| {
        let point = center + (direction * along + side * across) * size;
        RenderPoint::new(point.x, point.y)
    })
    .collect();
    frame.push_primitive(
        CUE_LAYER,
        RenderPrimitive::Polygon(RenderPolygon::filled(
            points,
            RenderColor::rgba(0.70, 0.92, 1.0, strength),
        )),
    );
}

#[cfg(test)]
mod tests;
