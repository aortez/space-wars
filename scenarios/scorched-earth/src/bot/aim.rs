use super::*;
use engine_rapier::cannon::bracket::{Bracket, Outcome};

#[derive(Debug, Clone, Copy, Serialize)]
pub(super) struct Geometry {
    pub shooter: Vec2,
    pub target: Vec2,
    pub pitch: f32,
    #[serde(skip)]
    max_power: f32,
}

impl Geometry {
    pub fn read(state: &Duel, physics: &PhysicsWorld, player: usize) -> Self {
        let shooter = state.motion(physics, state.tanks[player].body).unwrap();
        Self {
            shooter: shooter.position,
            target: state
                .motion(physics, state.tanks[1 - player].body)
                .unwrap()
                .position,
            pitch: shooter.angle,
            max_power: state.placement.max_power(),
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
    /// The elevation and power, bracketed from real ranging shots. What those
    /// shots showed of the flat-ground estimate outlasts a move; the local
    /// bracket and confidence don't.
    #[serde(flatten)]
    pub bracket: Bracket,
    pub correction: &'static str,
    pub observations: u64,
}

impl Default for Aim {
    fn default() -> Self {
        Self {
            geometry: None,
            bracket: Bracket::new(50.0_f32.to_radians(), 32.0),
            correction: "initial estimate",
            observations: 0,
        }
    }
}

fn rough_power(geometry: Geometry, elevation: f32) -> f32 {
    // Deliberately coarse flat-ground estimate. Height, the actual muzzle,
    // intervening terrain and launch motion are resolved through observations.
    let distance = (geometry.target.x - geometry.shooter.x).abs().max(1.0);
    let estimate = (GRAVITY * distance / (2.0 * elevation).sin().max(0.1)).sqrt();
    ((estimate / 4.0).round() * 4.0).clamp(cannon::MIN_POWER, geometry.max_power)
}

impl Aim {
    pub(super) fn has_power_correction(&self) -> bool {
        !self.bracket.confident
            && matches!(self.correction, "short" | "long")
            && self.bracket.bounded()
    }

    pub fn refresh(&mut self, geometry: Geometry) {
        if self.geometry.is_none_or(|old| !old.matches(geometry)) {
            self.geometry = Some(geometry);
            let rough = |elevation| rough_power(geometry, elevation);
            self.bracket.restart(rough, geometry.max_power);
            self.correction = "new firing geometry";
        }
    }

    pub fn learn(&mut self, shot: &ShotObservation) {
        self.observations += 1;
        // A shot from a different pose remains useful evidence, but must not
        // install its old local bracket as the solution at the new location.
        let current = self.geometry.filter(|g| {
            shot.assessment != "target moved"
                && g.shooter.distance_to(shot.launch.shooter) <= 1.0
                && g.target.distance_to(shot.launch.target) <= 1.0
                && (g.pitch - shot.launch.pitch).abs() <= 8.0_f32.to_radians()
                && self.bracket.fired_at(shot.launch.elevation)
        });
        let Some(current) = current else {
            self.bracket.confident = false;
            self.correction = "observation geometry changed";
            return;
        };
        self.correction = shot.assessment;
        let outcome = match shot.assessment {
            "useful hit" => Outcome::Hit,
            "short" => Outcome::Short,
            "long" => Outcome::Long,
            "blocked arc" | "blocked barrel" | "self hit" => Outcome::Blocked,
            _ => Outcome::Unclear,
        };
        let rough = |elevation| rough_power(current, elevation);
        self.bracket
            .learn(outcome, shot.launch.power, rough, current.max_power);
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
