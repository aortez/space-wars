//! Scripted hardware poses through the real ship, rail and projectile renderers.
//! This is a visual fixture, not another launcher scenario or physics model.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Loaded,
    Cruise,
    Reloading,
    Launched,
    Rotated,
}

impl Case {
    pub const ALL: [Self; 5] = [
        Self::Loaded,
        Self::Cruise,
        Self::Reloading,
        Self::Launched,
        Self::Rotated,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::Cruise => "cruise",
            Self::Reloading => "reloading",
            Self::Launched => "launched",
            Self::Rotated => "rotated-flight",
        }
    }
}

pub struct Snapshot {
    pub frame: RenderFrame,
    /// The same ship/camera without rails or ammunition for pixel comparisons.
    pub bare: RenderFrame,
}

pub fn capture(case: Case, camera_height: f32) -> Snapshot {
    const DT: f32 = 1.0 / 60.0;
    let mut ship = ShipState::new(0, Vec2::ZERO, Color::RED, 100, DT);
    ship.enable_weapon_supply();
    if case == Case::Cruise {
        ship.wing_theta = MAX_WING_THETA;
    }
    if case == Case::Rotated {
        ship.rotation_radians = -0.65;
        ship.direction = direction_from_rotation(ship.rotation_radians);
    }
    let mut round = None;
    if matches!(case, Case::Reloading | Case::Launched | Case::Rotated) {
        ship.set_cannon(true);
        round = ship.update_cannon_with_recoil(DT, 0, 0.0);
        ship.set_cannon(false);
        if case == Case::Reloading {
            // Start a real reload, then show its half-complete presentation.
            let armament = ship.armament.as_mut().unwrap();
            armament.advance(DT);
            armament.advance(ROUND_RELOAD_SECONDS * 0.5);
            round = None;
        } else if let Some(round) = &mut round {
            // Prescribed separation makes the shared in-flight artwork easy
            // to compare against the identical round still on the other rail.
            round.position += ship.direction * 12.0;
        }
    }
    let camera = Camera2::new(
        render_point(physics::ship_pivot(ship.form) + ship.direction * 4.0),
        camera_height.clamp(24.0, 440.0),
    );
    let mut frame = RenderFrame::new(camera);
    render_ship(&mut frame, &ship);
    if let Some(round) = round {
        render_debris(&mut frame, &round);
    }
    ship.armament = None;
    let mut bare = RenderFrame::new(camera);
    render_ship(&mut bare, &ship);
    Snapshot { frame, bare }
}
