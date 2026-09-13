//! Presentation-only feedback from the surface flight controller's actuation.
//!
//! No bodies, collision queries or shared RNG consumption. The legacy renderer
//! retains its original exhaust; surface ships opt into this bounded effect.

use super::*;

pub mod fixture;
#[cfg(test)]
mod tests;

const MAX_TRAILS: usize = 24;
const TRAIL_SECONDS: f32 = 0.4;
const MIN_OUTPUT: f32 = 0.02;

/// Normalized controller acceleration, in ship-local axes (+Y is forward).
/// Angular output is positive counterclockwise. This excludes gravity and
/// collisions: being knocked sideways must not light an imaginary thruster.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ThrusterOutput {
    pub linear: Vec2,
    pub angular: f32,
    pub braking: bool,
}

impl ThrusterOutput {
    pub(crate) fn from_acceleration(
        linear: Vec2,
        angular: f32,
        angle: f32,
        linear_limit: f32,
        angular_limit: f32,
        braking: bool,
    ) -> Self {
        let local = linear.rotate_radians(-angle) / linear_limit;
        Self {
            linear: Vec2::new(local.x.clamp(-1.0, 1.0), local.y.clamp(-1.0, 1.0)),
            angular: (angular / angular_limit).clamp(-1.0, 1.0),
            braking,
        }
    }

    // Main engine, paired reverse jets, then fore/aft jets on either side.
    // A turn uses an opposing force couple, not a fictitious sideways push.
    fn nozzles(self) -> [f32; 7] {
        let right = self.linear.x.max(0.0);
        let left = (-self.linear.x).max(0.0);
        let ccw = self.angular.max(0.0);
        let cw = (-self.angular).max(0.0);
        [
            self.linear.y.max(0.0),
            (-self.linear.y).max(0.0),
            (-self.linear.y).max(0.0),
            (right + cw).min(1.0),
            (right + ccw).min(1.0),
            (left + ccw).min(1.0),
            (left + cw).min(1.0),
        ]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThrusterVisuals {
    pub output: ThrusterOutput,
    form: ShipForm,
    phase: f32,
    trails: Vec<IonTrail>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct IonTrail {
    start: Vec2,
    end: Vec2,
    velocity: Vec2,
    remaining: f32,
    strength: f32,
}

impl ThrusterVisuals {
    fn new(form: ShipForm) -> Self {
        Self {
            output: ThrusterOutput::default(),
            form,
            phase: 0.0,
            trails: Vec::with_capacity(MAX_TRAILS),
        }
    }

    pub fn trail_count(&self) -> usize {
        self.trails.len()
    }
}

/// Called after the same physics step that produced `output`, so attached jets
/// use the rendered ship pose. Pause/zero-dt never advance the visual clock.
pub(crate) fn advance(ship: &mut ShipState, output: ThrusterOutput, dt: f32) {
    if dt <= 0.0 || !dt.is_finite() {
        return;
    }
    if ship.dead {
        ship.thrusters = None;
        return;
    }
    let nozzle = ship_transform(ship).transform_point(nozzle_layout(ship.form)[0].0);
    let direction = -Vec2::Y.rotate_radians(ship.rotation_radians);
    let effects = ship
        .thrusters
        .get_or_insert_with(|| ThrusterVisuals::new(ship.form));
    if effects.form != ship.form {
        *effects = ThrusterVisuals::new(ship.form);
    }
    effects.output = output;
    effects.phase = (effects.phase + dt * 4.0).fract();
    for trail in &mut effects.trails {
        trail.start += trail.velocity * dt;
        trail.end += trail.velocity * dt;
        trail.remaining -= dt;
    }
    effects.trails.retain(|trail| trail.remaining > 0.0);
    let strength = output.linear.y.clamp(0.0, 1.0);
    if strength > MIN_OUTPUT {
        if effects.trails.len() == MAX_TRAILS {
            effects.trails.remove(0);
        }
        let size = form_scale(ship.form);
        effects.trails.push(IonTrail {
            start: nozzle,
            end: nozzle + direction * (1.0 + strength) * size,
            // Partial inheritance leaves a readable wake even at low speed,
            // without stretching it into a world-spanning line at cruise.
            velocity: ship.velocity * 0.6 + direction * (14.0 + strength * 12.0) * size,
            remaining: TRAIL_SECONDS,
            strength,
        });
    }
}

fn form_scale(form: ShipForm) -> f32 {
    match form {
        ShipForm::Ship => 1.0,
        ShipForm::EscapePod => 0.45,
    }
}

fn nozzle_layout(form: ShipForm) -> [(Vec2, Vec2); 7] {
    let ship = [
        (Vec2::new(2.5, -1.25), -Vec2::Y),
        (Vec2::new(1.35, 5.6), Vec2::Y),
        (Vec2::new(3.65, 5.6), Vec2::Y),
        (Vec2::new(1.35, 5.0), -Vec2::X),
        (Vec2::new(-0.8, -0.35), -Vec2::X),
        (Vec2::new(3.65, 5.0), Vec2::X),
        (Vec2::new(5.8, -0.35), Vec2::X),
    ];
    if form == ShipForm::Ship {
        ship
    } else {
        [
            (Vec2::new(0.0, -0.2), -Vec2::Y),
            (Vec2::new(-0.6, 0.9), Vec2::Y),
            (Vec2::new(0.6, 0.9), Vec2::Y),
            (Vec2::new(-0.65, 0.65), -Vec2::X),
            (Vec2::new(-1.15, -0.05), -Vec2::X),
            (Vec2::new(0.65, 0.65), Vec2::X),
            (Vec2::new(1.15, -0.05), Vec2::X),
        ]
    }
}

pub(crate) fn render(frame: &mut RenderFrame, ship: &ShipState) {
    let Some(effects) = &ship.thrusters else {
        return;
    };
    if ship.dead || effects.form != ship.form {
        return;
    }
    for trail in &effects.trails {
        let fade = (trail.remaining / TRAIL_SECONDS).clamp(0.0, 1.0);
        frame.push_primitive(
            EXHAUST_LAYER,
            RenderPrimitive::Line(engine_common::RenderLine::new(
                render_point(trail.start),
                render_point(trail.end),
                Stroke::new(
                    RenderColor::rgba(0.12, 0.64, 1.0, fade * fade * (0.35 + 0.4 * trail.strength)),
                    1.8,
                ),
            )),
        );
    }
    let transform = ship_transform(ship);
    for (index, ((position, direction), strength)) in nozzle_layout(ship.form)
        .into_iter()
        .zip(effects.output.nozzles())
        .enumerate()
    {
        if strength <= MIN_OUTPUT {
            continue;
        }
        let phase = (effects.phase + index as f32 * 0.17 + ship.owner_id as f32 * 0.23).fract();
        let pulse = 0.85 + 0.15 * (phase * std::f32::consts::TAU).sin();
        let size = form_scale(ship.form);
        let length = if index == 0 { 5.5 } else { 2.7 } * (0.45 + 0.55 * strength) * pulse * size;
        let width = if index == 0 { 0.72 } else { 0.35 } * size;
        let color = if effects.output.braking {
            RenderColor::rgba(0.66, 0.42, 1.0, 0.8)
        } else {
            RenderColor::rgba(0.1, 0.7, 1.0, 0.8)
        };
        let side = Vec2::new(-direction.y, direction.x);
        polygon(
            frame,
            transform,
            &[
                position - side * width,
                position + side * width,
                position + direction * length,
            ],
            color,
        );
        polygon(
            frame,
            transform,
            &[
                position - side * width * 0.4,
                position + side * width * 0.4,
                position + direction * length * 0.72,
            ],
            RenderColor::rgba(0.8, 0.97, 1.0, 0.95),
        );
        // Two detached packets move outward. A steady core remains visible
        // through every pulse phase, including at a Pi's small render scale.
        for packet in [phase, (phase + 0.5).fract()] {
            let center = position + direction * length * (0.85 + packet * 0.9);
            let half = width * (1.0 - packet * 0.5);
            polygon(
                frame,
                transform,
                &[
                    center - side * half,
                    center + direction * width * 0.7,
                    center + side * half,
                    center - direction * width * 0.7,
                ],
                RenderColor::rgba(color.r, color.g, color.b, (1.0 - packet) * 0.75),
            );
        }
    }
}

fn polygon(frame: &mut RenderFrame, transform: Transform2, points: &[Vec2], color: RenderColor) {
    frame.push_primitive(
        EXHAUST_LAYER,
        RenderPrimitive::Polygon(RenderPolygon {
            points: points
                .iter()
                .map(|&point| render_point(transform.transform_point(point)))
                .collect(),
            fill: Some(Fill::new(color)),
            stroke: None,
        }),
    );
}
