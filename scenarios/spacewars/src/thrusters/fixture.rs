//! Deterministic visual inspection, not a launcher entry or a physics model.
//! Prescribed actuation/poses exercise the production effect and ship renderer.
//! Separate surface-controller tests verify that real inputs publish actuation.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Idle,
    Forward,
    Left,
    Right,
    Reverse,
    Brake,
    RotatedBrake,
    Cruise,
    Pod,
}

impl Case {
    pub const ALL: [Self; 9] = [
        Self::Idle,
        Self::Forward,
        Self::Left,
        Self::Right,
        Self::Reverse,
        Self::Brake,
        Self::RotatedBrake,
        Self::Cruise,
        Self::Pod,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Forward => "low-speed-forward",
            Self::Left => "turn-left",
            Self::Right => "turn-right",
            Self::Reverse => "reverse-model",
            Self::Brake => "full-braking",
            Self::RotatedBrake => "rotated-braking",
            Self::Cruise => "cruise",
            Self::Pod => "pod-forward",
        }
    }
}

pub struct Snapshot {
    pub frame: RenderFrame,
    /// The identical ship/camera without effects, for pixel-difference checks.
    pub unlit: RenderFrame,
    pub output: ThrusterOutput,
    pub trail_count: usize,
}

pub fn capture(case: Case, ticks: u32, camera_height: f32) -> Snapshot {
    let mut ship = ShipState::new(0, Vec2::ZERO, Color::RED, 100, 1.0 / 60.0);
    let mut output = ThrusterOutput::default();
    let mut local_velocity = Vec2::ZERO;
    match case {
        Case::Idle => {}
        Case::Forward | Case::Pod | Case::Cruise => {
            output.linear.y = 1.0;
            local_velocity.y = if case == Case::Cruise { 140.0 } else { 18.0 };
            if case == Case::Cruise {
                ship.wing_theta = MAX_WING_THETA;
            }
            if case == Case::Pod {
                ship.form = ShipForm::EscapePod;
            }
        }
        Case::Left => output.angular = 1.0,
        Case::Right => output.angular = -1.0,
        Case::Reverse => {
            output.linear.y = -1.0;
            local_velocity.y = -18.0;
        }
        Case::Brake | Case::RotatedBrake => {
            output.linear = Vec2::new(-0.6, -0.8);
            output.angular = -0.65;
            output.braking = true;
            local_velocity = Vec2::new(24.0, 32.0);
            if case == Case::RotatedBrake {
                ship.rotation_radians = 0.9;
            }
        }
    }
    ship.velocity = local_velocity.rotate_radians(ship.rotation_radians);
    for _ in 0..ticks.min(600) {
        ship.position += ship.velocity / 60.0;
        advance(&mut ship, output, 1.0 / 60.0);
    }
    let camera = Camera2::new(
        render_point(ship.position + physics::ship_pivot(ship.form)),
        camera_height.clamp(12.0, 440.0),
    );
    let mut unlit = RenderFrame::new(camera);
    render_ship(&mut unlit, &ship);
    let mut frame = unlit.clone();
    render(&mut frame, &ship);
    Snapshot {
        frame,
        unlit,
        output,
        trail_count: ship
            .thrusters
            .as_ref()
            .map_or(0, ThrusterVisuals::trail_count),
    }
}
