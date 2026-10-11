//! Small arcade flight controller: bounded lateral steering, gravity/drag and
//! lift on each wing downstroke. It owns no collision world or random stream.
use crate::layout::Layout;
use engine_core::Vec2;

#[cfg(test)]
mod tests;

const DT: f32 = 1.0 / 60.0;
const RAISE_TICKS: u32 = 3;
const DOWN_TICKS: u32 = 6;
const RECOVER_TICKS: u32 = 9;
const STROKE_TICKS: u32 = RAISE_TICKS + DOWN_TICKS + RECOVER_TICKS;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Stroke {
    tick: u32,
    /// Total upward velocity supplied by this downstroke, before gravity/drag.
    impulse: f32,
    amplitude: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum Leg {
    #[default]
    Climb,
    Cross,
    Land,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Flight {
    pub velocity: Vec2,
    /// Normalized wing pose: raised (+1), level (0), lowered (-1).
    pub wing: f32,
    leg: Leg,
    launch_x: f32,
    stroke: Option<Stroke>,
}

impl Flight {
    pub fn retarget(&mut self, position: Vec2, grounded: bool, layout: Layout) {
        self.leg = Leg::Climb;
        self.launch_x = position.x;
        if grounded {
            self.velocity.x = 0.0;
            self.push_off(layout);
        }
    }

    fn push_off(&mut self, layout: Layout) {
        // A real foot contact can interrupt an obsolete landing stroke.
        // Start the next stroke above that support with a small leg push.
        self.velocity.y = layout.pitch * 2.0;
        self.stroke = None;
        self.wing = 0.0;
    }

    /// True only on a slow, downward crossing of the intended foot support.
    pub fn step(
        &mut self,
        position: &mut Vec2,
        target: Vec2,
        cruise: f32,
        layout: Layout,
        leaving: bool,
    ) -> bool {
        let scale = layout.pitch;
        let gravity = scale * 10.0;
        let width = layout.bounds_max.x - layout.bounds_min.x;
        let speed = (width * 0.55).max(scale * 9.0);
        if self.leg == Leg::Climb && position.y >= cruise {
            self.leg = Leg::Cross;
        }
        if !leaving
            && self.leg == Leg::Cross
            && (position.x - target.x).abs() < layout.pitch * 0.25
            && self.velocity.x.abs() < scale * 0.6
        {
            self.leg = Leg::Land;
        }
        let aim = match self.leg {
            Leg::Climb => Vec2::new(self.launch_x, cruise),
            Leg::Cross => Vec2::new(target.x, cruise),
            // Aim just through the support, so the wingbeat's small hover
            // oscillation cannot leave the feet indefinitely above the perch.
            Leg::Land => target - Vec2::new(0.0, scale * 0.6),
        };
        // Critically damped near the target: do not overshoot and turn around
        // repeatedly during the final approach.
        let desired_x = ((aim.x - position.x) * 2.5).clamp(-speed, speed);
        let acceleration_x =
            ((desired_x - self.velocity.x) * 10.0).clamp(-speed * 3.0, speed * 3.0);
        // Forward motion supports some weight while gliding, but never all of it.
        let glide = gravity * 0.55 * (self.velocity.x.abs() / speed).min(1.0);
        let drag = self.velocity.y * 0.9;
        if self.stroke.is_none() {
            // Start a deliberate stroke only when the glide can no longer
            // carry the desired motion. Leave room for the resulting rise.
            let error = aim.y - position.y - self.velocity.y * 0.15;
            let desired_y = (error * 2.5).clamp(-scale * 6.0, scale * 7.0);
            if self.velocity.y < desired_y - scale * 0.4 {
                let landing = self.leg == Leg::Land;
                let extra = scale * if landing { 0.3 } else { 1.2 };
                let minimum = scale * if landing { 1.0 } else { 2.8 };
                let impulse = (desired_y - self.velocity.y + (gravity - glide) * 0.15 + extra)
                    .clamp(minimum, scale * 8.0);
                // The body's remaining headroom bounds a stroke even during
                // a strong climb or an interrupted hop near the upper frame.
                let headroom =
                    (layout.bounds_max.y - layout.frame_width - scale * 1.6 - position.y).max(0.0);
                let safe_speed = (2.0 * gravity * 0.45 * headroom).sqrt();
                let impulse = impulse.min((safe_speed - self.velocity.y).max(0.0));
                if impulse > scale * 0.1 {
                    self.stroke = Some(Stroke {
                        tick: 0,
                        impulse,
                        amplitude: (impulse / (scale * 6.0)).sqrt().min(1.0),
                    });
                }
            }
        }
        let kick = self.advance_wings();
        self.velocity.x += acceleration_x * DT;
        self.velocity.y += kick + (glide - gravity - drag) * DT;
        let previous = *position;
        *position += self.velocity * DT;
        let contact = self.leg == Leg::Land
            && previous.y >= target.y
            && position.y <= target.y
            && (position.x - target.x).abs() <= layout.pitch * 0.35;
        if !contact {
            return false;
        }
        position.y = target.y;
        if self.velocity.y >= -scale * 2.5 && self.velocity.x.abs() <= scale * 0.6 {
            return true;
        }
        // A hard touchdown cannot pass through its intended digit. Absorb most
        // of the impact, then let the controller settle before declaring a perch.
        self.velocity.y *= -0.15;
        false
    }

    fn advance_wings(&mut self) -> f32 {
        let Some(stroke) = &mut self.stroke else {
            self.wing = 0.0;
            return 0.0;
        };
        let mut kick = 0.0;
        self.wing = if stroke.tick < RAISE_TICKS {
            let t = (stroke.tick + 1) as f32 / RAISE_TICKS as f32;
            stroke.amplitude * (1.0 - (t * std::f32::consts::PI).cos()) * 0.5
        } else if stroke.tick < RAISE_TICKS + DOWN_TICKS {
            let tick = stroke.tick - RAISE_TICKS;
            let t = (tick as f32 + 0.5) / DOWN_TICKS as f32;
            // The sampled weights sum to one, making impulse independent of
            // the visual stroke shape. The matching wing travels downward.
            kick = stroke.impulse
                * (t * std::f32::consts::PI).sin()
                * (std::f32::consts::PI / (2 * DOWN_TICKS) as f32).sin();
            stroke.amplitude
                * (((tick + 1) as f32 / DOWN_TICKS as f32) * std::f32::consts::PI).cos()
        } else {
            let t = (stroke.tick - RAISE_TICKS - DOWN_TICKS + 1) as f32 / RECOVER_TICKS as f32;
            -stroke.amplitude * (1.0 + (t * std::f32::consts::PI).cos()) * 0.5
        };
        stroke.tick += 1;
        if stroke.tick == STROKE_TICKS {
            self.stroke = None;
        }
        kick
    }
}
