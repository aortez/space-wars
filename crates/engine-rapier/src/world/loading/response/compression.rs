//! Opt-in driven compression of a fixed material layer by an ideal rigid platen.
//! The compliant layer's envelope is represented by the fixed rectangle; rigid
//! penetration correction is disabled. Actual prescribed motion supplies work.
use super::*;
use engine_material::{CRUSH_TIME_CONSTANT, SustainedLoad, sustained};

mod model;
mod scene;
mod step;
use model::History;
use scene::Scene;

/// Refusal discards all provisional motion, time, capacity and output changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionRejection {
    InvalidCommand,
    ResponseDisabled,
    UnsupportedScene,
    StaleContinuation,
    FailedRegion,
    Limit,
    Numerical,
    Snapshot,
}
use CompressionRejection as R;

/// Persistent physical and numerical accounting, including after invalidation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CompressionSummary {
    /// Indentation relative to the initially touching platen; negative is a gap.
    pub indentation: f64,
    pub plastic_indentation: f64,
    pub elastic_energy: f64,
    /// Signed supplied work. Elastic unloading returns work to the driver.
    pub driver_work: f64,
    /// Actual decrease of the material region's precise remaining capacity.
    pub damage_work: f64,
    /// Physical viscous overstress loss, separate from damage and numerical error.
    pub viscous_loss: f64,
    /// Physical plastic work minus actual capacity charge.
    pub charge_error: f64,
    /// Sum of absolute charge errors, bounded by an initial fixed energy scale.
    pub absolute_charge_error: f64,
    /// Sum of absolute actual-minus-commanded displacement errors.
    pub absolute_motion_error: f64,
    pub elapsed: f64,
    pub steps: u64,
}

/// Canonical episode event at a time within the committed interval.
#[derive(Debug, Clone, PartialEq)]
pub struct CompressionEvent {
    pub at: f64,
    pub event: EpisodeEvent,
}

/// Complete physical load on one material patch. Geometry duplicates do not
/// multiply this record. Force is the end reaction; impulse integrates the
/// reaction across the entire interval, including any detached portion.
#[derive(Debug, Clone, PartialEq)]
pub struct CompressionLoad {
    pub law: &'static str,
    pub material_id: u64,
    /// Owner during the contacting portion; None for an entirely detached step.
    pub episode: Option<EpisodeId>,
    /// Platen then material support, independent of canonical source ordering.
    pub sources: [ContactSource; 2],
    pub normal: Vec2,
    pub duration: f32,
    pub width: f32,
    pub section: Section,
    pub displacement: f64,
    pub plastic_displacement: f64,
    pub force: f64,
    pub impulse: f64,
    pub elastic_before: f64,
    pub elastic_after: f64,
    pub driver_work: f64,
    pub damage_work: f64,
    pub viscous_loss: f64,
    pub charge_error: f64,
}

/// Successful transaction. Pause has no load/events and preserves history.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompressionStep {
    pub events: Vec<CompressionEvent>,
    pub load: Option<CompressionLoad>,
    pub summary: Option<CompressionSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(in crate::world) struct Continuation {
    revision: u32,
    history: History,
    status: Status,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Status {
    Active(Box<Active>),
    Invalidated(gravity::Invalidation),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Active {
    scene: Scene,
    owner: Option<EpisodeId>,
}
impl Continuation {
    pub(in crate::world) fn invalidate(&mut self, reason: gravity::Invalidation) {
        if matches!(self.status, Status::Active(_)) {
            self.status = Status::Invalidated(reason);
        }
    }
    pub(in crate::world) fn valid_for(&self, world: &PhysicsWorld) -> bool {
        if self.revision != 1 || !self.history.valid() {
            return false;
        }
        match &self.status {
            Status::Invalidated(_) => true,
            Status::Active(active) => {
                let Ok(scene) = Scene::read(world) else {
                    return false;
                };
                let loading = world.loading.as_ref().unwrap();
                let mut pair = scene.sources;
                pair.sort();
                scene == active.scene
                    && scene.x == self.history.summary.indentation
                    && scene.material == self.history.current_material()
                    && active.owner.is_some()
                        == (scene.x >= self.history.summary.plastic_indentation)
                    && loading.episodes.len() == usize::from(active.owner.is_some())
                    && match active.owner {
                        Some(id) => loading
                            .episodes
                            .get(&pair)
                            .is_some_and(|e| e.id == id && e.absent_seconds == 0.0),
                        None => loading.episodes.is_empty(),
                    }
            }
        }
    }
}

impl PhysicsWorld {
    /// Prescribe a platen's signed normal speed for one interval against a fixed
    /// compliant material layer. Positive speed compresses, negative unloads.
    ///
    /// Requires exactly two aligned cardinal rectangles of half-extents (0.5,1),
    /// a velocity-kinematic platen and a fixed material support. Each rectangle
    /// may have 1/2/4 uniform tangent slices and 1/2 coincident copies. Both need
    /// live loading sources; only the support has a registered material. Initial
    /// enrollment requires exact touching and zero speed. No gravity, joints,
    /// other forces, friction, damping, sensors, CCD or angular/tangent motion.
    /// The fixed center defines a touching origin bounded to [-16,16] per axis.
    /// Material thickness is [0.125,8], depth [0.125,1].
    ///
    /// A unilateral elastic-perfectly-plastic layer has stiffness E*A/depth and
    /// yield force strength*A. Plastic flow adds viscosity Y*tau/depth, using
    /// the material law's crush time constant. Measured motion supplies elastic
    /// storage, plastic damage and viscous loss. Holds do no work; unloading
    /// returns elastic energy. This is a bounded host coupling, not a dynamic
    /// gravity-support solve. The platen is an ideal prescribed rigid boundary.
    ///
    /// Positive dt must be <=1, |speed|<=0.25, |indentation|<=depth/4. History
    /// permits <=3600 seconds and <=1,000,000 steps. Fixed cumulative numerical
    /// limits are depth*1e-5 for motion and Y*depth*1e-5 for charge rounding.
    /// Failed/exhausted materials, unrepresentable work and all unsupported
    /// scenes refuse atomically. Failure topology is left to its host.
    ///
    /// dt=0 clears ephemeral outputs without applying speed or enrolling.
    /// External edits and other positive stepping invalidate continuation
    /// permanently. Saved history uses snapshot v14; v1-v13 remain unchanged.
    pub fn step_material_compression(
        &mut self,
        dt: f32,
        speed: f32,
    ) -> Result<CompressionStep, CompressionRejection> {
        step::advance(self, dt, speed)
    }

    /// Read retained work/deformation/error totals, even after invalidation.
    pub fn material_compression_summary(&self) -> Option<CompressionSummary> {
        Some(
            self.loading
                .as_ref()?
                .compression
                .as_ref()?
                .history
                .summary
                .clone(),
        )
    }
}

#[cfg(test)]
mod tests;
