//! Fixed-step presentation history, independent of render cadence and gameplay.

use std::time::Duration;

use engine_common::{Camera2, RenderPoint};
use scenario_spacewars::surface_sortie::camera::{CameraFocus, CameraTarget};

const FOCUS_HALF_LIFE: f32 = 0.14;
const ZOOM_HALF_LIFE: f32 = 0.16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PlayerCamera {
    anchor: RenderPoint,
    offset: RenderPoint,
    height: f32,
    focus: CameraFocus,
    pub(super) framed_opponent: bool,
}

impl PlayerCamera {
    pub(super) fn new(target: CameraTarget) -> Self {
        Self {
            anchor: target.anchor,
            offset: RenderPoint::new(
                target.camera.center.x - target.anchor.x,
                target.camera.center.y - target.anchor.y,
            ),
            height: target.camera.height,
            focus: target.focus,
            framed_opponent: target.framed_opponent,
        }
    }

    pub(super) fn advance(&mut self, target: CameraTarget, dt: Duration) {
        if dt.is_zero() {
            return;
        }
        let movement = RenderPoint::new(
            target.anchor.x - self.anchor.x,
            target.anchor.y - self.anchor.y,
        );
        // Do not sweep across the universe after a discontinuous relocation.
        // Ordinary flight moves far less than this in one simulation tick.
        let teleport_distance = (self.height.max(target.camera.height) * 1.5).max(100.0);
        if movement.x.hypot(movement.y) > teleport_distance {
            *self = Self::new(target);
            return;
        }
        if self.focus != target.focus {
            // Boarding/disembarking changes the anchor. Preserve the previous
            // world-space composition before easing toward the new actor.
            self.offset.x -= movement.x;
            self.offset.y -= movement.y;
        }
        // With an unchanged actor, carry the composition along with its motion
        // exactly. Only the framing offset lags, not the ship's steering.
        let blend = 1.0 - (-std::f32::consts::LN_2 * dt.as_secs_f32() / FOCUS_HALF_LIFE).exp();
        self.offset.x += (target.camera.center.x - target.anchor.x - self.offset.x) * blend;
        self.offset.y += (target.camera.center.y - target.anchor.y - self.offset.y) * blend;
        if (self.height - target.camera.height).abs() < 0.001 {
            self.height = target.camera.height;
        } else {
            // Log-height easing makes equal zoom ratios feel alike in either
            // direction. Exponential decay is bounded and cannot overshoot.
            let blend = 1.0 - (-std::f32::consts::LN_2 * dt.as_secs_f32() / ZOOM_HALF_LIFE).exp();
            self.height =
                (self.height.ln() + (target.camera.height.ln() - self.height.ln()) * blend).exp();
        }
        self.anchor = target.anchor;
        self.focus = target.focus;
        self.framed_opponent = target.framed_opponent;
    }

    pub(super) fn displayed(&self, aspect: f32) -> Camera2 {
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect.max(0.001)
        } else {
            1.0
        };
        // During zoom-in, an old framing offset must not push the active actor
        // off-screen. Fit it with 10% margins, including narrow split panes.
        // This also keeps paired framing usable on portrait displays.
        let height = self
            .height
            .max(self.offset.x.abs() / (aspect * 0.4))
            .max(self.offset.y.abs() / 0.4);
        Camera2::new(
            RenderPoint::new(self.anchor.x + self.offset.x, self.anchor.y + self.offset.y),
            height,
        )
    }
}

#[cfg(test)]
mod client_tests;
#[cfg(test)]
mod tests;
