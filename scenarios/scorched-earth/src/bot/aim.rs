use super::*;

#[derive(Debug, Clone, Copy, Serialize)]
pub(super) struct Geometry {
    pub shooter: Vec2,
    pub target: Vec2,
    pub pitch: f32,
}

impl Geometry {
    pub fn read(state: &ScorchedState, player: usize) -> Self {
        let shooter = state.physics.motion(state.tanks[player].body).unwrap();
        Self {
            shooter: shooter.position,
            target: state
                .physics
                .motion(state.tanks[1 - player].body)
                .unwrap()
                .position,
            pitch: shooter.angle,
        }
    }

    pub(super) fn matches(self, other: Self) -> bool {
        self.shooter.distance_to(other.shooter) <= 1.0
            && self.target.distance_to(other.target) <= 1.0
            && (self.pitch - other.pitch).abs() <= 8.0_f32.to_radians()
    }
}

#[derive(Clone, Serialize)]
pub(super) struct Aim {
    geometry: Option<Geometry>,
    pub elevation: f32,
    pub power: f32,
    pub confident: bool,
    lower_power: Option<f32>,
    upper_power: Option<f32>,
    /// Power/range relationship learned from real ranging shots, retained when
    /// positions change; local bracketing and confidence are cleared instead.
    range_scale: f32,
    pub correction: &'static str,
    pub observations: u64,
    blocked_arcs: u8,
}

impl Default for Aim {
    fn default() -> Self {
        Self {
            geometry: None,
            elevation: 50.0_f32.to_radians(),
            power: 32.0,
            confident: false,
            lower_power: None,
            upper_power: None,
            range_scale: 1.0,
            correction: "initial estimate",
            observations: 0,
            blocked_arcs: 0,
        }
    }
}

fn rough_power(geometry: Geometry, elevation: f32) -> f32 {
    // Deliberately coarse flat-ground estimate. Height, the actual muzzle,
    // intervening terrain and launch motion are resolved through observations.
    let distance = (geometry.target.x - geometry.shooter.x).abs().max(1.0);
    let estimate = (GRAVITY * distance / (2.0 * elevation).sin().max(0.1)).sqrt();
    ((estimate / 4.0).round() * 4.0).clamp(12.0, 48.0)
}

impl Aim {
    pub(super) fn has_power_correction(&self) -> bool {
        !self.confident
            && matches!(self.correction, "short" | "long")
            && (self.lower_power.is_some() || self.upper_power.is_some())
    }

    pub fn refresh(&mut self, geometry: Geometry) {
        if self.geometry.is_none_or(|old| !old.matches(geometry)) {
            self.geometry = Some(geometry);
            self.lower_power = None;
            self.upper_power = None;
            self.confident = false;
            self.power =
                (rough_power(geometry, self.elevation) * self.range_scale).clamp(12.0, 48.0);
            self.correction = "new firing geometry";
        }
    }

    pub fn learn(&mut self, shot: &ShotObservation) {
        self.observations += 1;
        // A shot from a different pose remains useful evidence, but must not
        // install its old local bracket as the solution at the new location.
        let current = self.geometry;
        let applies = current.is_some_and(|g| {
            g.shooter.distance_to(shot.launch.shooter) <= 1.0
                && g.target.distance_to(shot.launch.target) <= 1.0
                && (g.pitch - shot.launch.pitch).abs() <= 8.0_f32.to_radians()
                && (self.elevation - shot.launch.elevation).abs() < 0.01
        });
        if shot.assessment == "target moved" || !applies {
            self.confident = false;
            self.correction = "observation geometry changed";
            return;
        }
        self.confident = shot.assessment == "useful hit";
        self.correction = shot.assessment;
        match shot.assessment {
            "useful hit" => {
                self.power = shot.launch.power;
                self.lower_power = None;
                self.upper_power = None;
            }
            "short" => self.lower_power = Some(shot.launch.power),
            "long" => self.upper_power = Some(shot.launch.power),
            "blocked arc" | "blocked barrel" | "self hit" => {
                self.lower_power = None;
                self.upper_power = None;
                self.blocked_arcs = self.blocked_arcs.saturating_add(1);
                self.elevation =
                    (self.elevation + 10.0_f32.to_radians()).min(80.0_f32.to_radians());
                self.power = (rough_power(current.unwrap(), self.elevation) * self.range_scale)
                    .clamp(12.0, 48.0);
                return;
            }
            _ => return,
        }
        if !self.confident {
            self.power = match (self.lower_power, self.upper_power) {
                (Some(low), Some(high)) if low < high => (low + high) * 0.5,
                _ => {
                    // Clear an inconsistent bracket after terrain changed the
                    // result; never assume global monotonicity across impacts.
                    if self
                        .lower_power
                        .zip(self.upper_power)
                        .is_some_and(|(l, h)| l >= h)
                    {
                        self.lower_power = None;
                        self.upper_power = None;
                    }
                    shot.launch.power
                        + if shot.assessment == "short" {
                            2.5
                        } else {
                            -2.5
                        }
                }
            }
            .clamp(12.0, 48.0);
        }
        self.range_scale =
            (self.power / rough_power(current.unwrap(), self.elevation)).clamp(0.65, 1.4);
    }
}

pub(super) fn assess(shot: &ShotObservation, final_velocity: Vec2) -> &'static str {
    if shot.launch.blocked_barrel {
        return "blocked barrel";
    }
    if shot.target_moved {
        return "target moved";
    }
    if shot.damage[shot.launch.owner] > shot.damage[1 - shot.launch.owner] {
        return "self hit";
    }
    // A weak splash is evidence for another correction, not solved aim.
    // Use geometry as well as actual damage so a lethal low-health hit is
    // not incorrectly recorded as a miss by the health clamp.
    if shot.damage[1 - shot.launch.owner] > 0.0 && shot.closest <= 2.5 {
        return "useful hit";
    }
    if let Some(height) = shot.crossing_height {
        return if height > 1.5 {
            "long"
        } else if height < -1.5 {
            "short"
        } else {
            "near target"
        };
    }
    let direction = (shot.launch.target.x - shot.launch.origin.x).signum();
    let offset = (shot.end.x - shot.launch.target.x) * direction;
    if shot.termination == "terrain"
        && offset < -2.0
        && (final_velocity.y > 0.0 || shot.end.y > shot.launch.target.y + 1.5)
    {
        return "blocked arc";
    }
    if offset < -1.5 {
        "short"
    } else if offset > 1.5 {
        "long"
    } else {
        "near target"
    }
}
