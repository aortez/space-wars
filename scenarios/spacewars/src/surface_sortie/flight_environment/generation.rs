//! Versioned candidate sampling and independent geometric admission.
//! Witnesses and deadline estimates are evaluator data, never policy inputs.
use super::*;
use engine_rapier::world::{CollisionGroups, PhysicsWorld, polar::GeometryOutline};
use std::f32::consts::{FRAC_PI_2, PI};

mod admission;
mod recipe;
mod route;
#[cfg(test)]
mod tests;
pub mod transfer;

pub const GENERATOR_VERSION: u32 = 1;
pub const GOAL_CLEARANCE: f32 = 12.0;
pub const ROUTE_MARGIN: f32 = 0.25;
pub const GRID_SPACING: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlightFamily {
    Clear,
    Moving,
    Obstructed,
    Altered,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightCandidate {
    pub generator_version: u32,
    pub seed: u64,
    pub index: u64,
    pub family: FlightFamily,
    pub recipe: String,
    pub task: FlightTask,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GravitySample {
    pub position: Vec2,
    pub acceleration: Vec2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdmissionEvidence {
    pub start_com: Vec2,
    pub goal_start: Vec2,
    pub goal_end: Vec2,
    pub world_center: Vec2,
    pub world_radius: f32,
    pub hull_com_radius: f32,
    pub goal_margin_after_hull_and_tolerance: f32,
    pub direct_clear: bool,
    pub full_hull_start_clear: bool,
    pub goal_trajectory_clear: bool,
    pub route: Vec<Vec2>,
    pub route_length: f32,
    pub first_leg_moves_away: bool,
    pub rotation_envelopes: Vec<(Vec2, f32)>,
    pub gravity_samples: Vec<GravitySample>,
    pub reference_time_seconds: Option<f32>,
    pub deadline_seconds: f32,
    pub object_count: usize,
}

#[derive(Debug, Serialize)]
pub struct FlightAdmission {
    pub candidate: FlightCandidate,
    /// Admitted to this conservative diagnostic recipe, not proven flyable.
    pub admitted: bool,
    /// First rejection only, in deterministic check order. No silent retries.
    pub rejection: Option<String>,
    pub evidence: Option<AdmissionEvidence>,
    /// Renderer-only outlines. Admission uses native geometry, not these lines.
    pub geometry: Vec<GeometryOutline>,
}

pub fn recipe_manifest() -> serde_json::Value {
    serde_json::json!({
        "generator_version":GENERATOR_VERSION,"rng":"SplitMix64; candidate keyed by seed and index; top 24 bits for uniform f32",
        "family_schedule":"index modulo 4: clear, moving, obstructed, altered; round-robin candidate schedule",
        "batch_sampling":"choose equal accepted quotas per family; skip filled families, retain every attempted rejection; stop at explicit max_rounds and report incomplete quota",
        "obstructed_schedule":"(index / 4) modulo 3: wall, open U, curved corridor",
        "altered_schedule":"(index / 4) modulo 2: carved passage, built protrusion",
        "heading":[-PI,PI],"spin":[-1.8,1.8],"clear_moving_speed":[0,70],"obstructed_speed":[0,15],"altered_speed":[0,12],
        "target_speed":[5,20],"distance":[30,200],"ordinary_altitude":[100,150],"carved_altitude":[15,25],
        "gravity":"seeded equal zero/production practice gravity; one stationary-center practice planet; no mass or radius variants",
        "planet_spin":"seeded choice -0.015, 0, +0.015; carved passage stationary only",
        "goal_clearance":GOAL_CLEARANCE,"route_margin":ROUTE_MARGIN,"grid_spacing":GRID_SPACING,
        "rotation":"full-rotation shape bound around stationary planet origin; excludes potential routes through rotating envelopes",
        "route":"orientation-independent COM enclosing disk, continuous native capsule queries; A* then visible shortcuts; grid failure means no conservative witness, not proven disconnection",
        "deadline":"reference screen: (5 seconds + 1.9 seconds per route segment + length / 35) / (1 - target speed / 35) + dwell; engineering estimate, not dynamic feasibility proof",
        "goal_motion":"full deadline segment; static native shapes plus full rotation envelopes; inward world-edge clearance",
        "gravity_measurement":"production spherical gravity solver at route/goal-path samples spaced at most 8 units; not flown trajectories or global extrema",
        "exclusions":["generated match worlds and sun","moving body origins or debris","planet-attached targets","mid-episode geometry changes as a navigation task","orientation-dependent narrow passages","dynamic feasibility guarantee"],
        "training":false,"deadline_ticks":1800,"decision_hz":60
    })
}

#[derive(Debug, Serialize)]
pub struct FlightTaskBatch {
    pub seed: u64,
    pub per_family: usize,
    pub max_rounds: u64,
    pub complete: bool,
    /// Clear, moving, obstructed, altered; successful quota completion is equal.
    pub admitted_by_family: [usize; 4],
    /// Every attempted candidate, including every rejection. Filled families
    /// skip later indices; remaining indices retain their stable seed identity.
    pub attempts: Vec<FlightAdmission>,
}

impl FlightTaskBatch {
    pub fn generate(seed: u64, per_family: usize, max_rounds: u64) -> Result<Self, &'static str> {
        if per_family == 0 || per_family > 1024 || max_rounds == 0 || max_rounds > 4096 {
            return Err("invalid task batch budget");
        }
        let mut batch = Self {
            seed,
            per_family,
            max_rounds,
            complete: false,
            admitted_by_family: [0; 4],
            attempts: Vec::new(),
        };
        for round in 0..max_rounds {
            for family in 0..4 {
                if batch.admitted_by_family[family] == per_family {
                    continue;
                }
                let candidate = FlightCandidate::generate(seed, round * 4 + family as u64)?;
                let report = candidate.inspect();
                if report.admitted {
                    batch.admitted_by_family[family] += 1;
                }
                batch.attempts.push(report);
            }
            if batch.admitted_by_family.iter().all(|&n| n == per_family) {
                batch.complete = true;
                break;
            }
        }
        Ok(batch)
    }
}
