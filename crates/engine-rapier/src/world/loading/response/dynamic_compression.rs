//! Finite-mass compliant loading, elastic return and flight with measured work.
use super::*;
use compression::{CompressionEvent, CompressionLoad, CompressionRejection as R};
use engine_material::{CRUSH_TIME_CONSTANT, SustainedLoad, sustained};

mod model;
mod step;
use model::{History, MaterialCurve};

/// Largest number of internal midpoint reaction solves per public call.
pub const MAX_DYNAMIC_COMPRESSION_STEPS: usize = 1024;

/// Physical totals and fixed numerical allowances, retained after invalidation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DynamicCompressionSummary {
    pub indentation: f64,
    pub velocity: f64,
    pub plastic_indentation: f64,
    pub kinetic_energy: f64,
    pub elastic_energy: f64,
    pub gravity_work: f64,
    pub damage_work: f64,
    pub viscous_loss: f64,
    pub charge_error: f64,
    pub absolute_charge_error: f64,
    pub reaction_impulse: f64,
    /// Integral of endpoint-average velocity; supports the position-defect check.
    pub integrated_velocity: f64,
    /// Energy, impulse and position defects, respectively. Never material damage.
    pub errors: [f64; 3],
    /// Sums of absolute defects, bounded by 1e-5 times the initial scales.
    pub absolute_errors: [f64; 3],
    /// Y*depth, sqrt(2*mass*Y*depth), depth. Frozen at enrollment.
    pub scales: [f64; 3],
    pub elapsed: f64,
    pub solves: u64,
}

/// One internal interval, in execution order. The reaction is the mean
/// constitutive force in the work-preserving discrete solve, not a rigid
/// penetration-correction impulse. `load.force` is its endpoint reaction.
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicCompressionInterval {
    pub at: f64,
    pub load: CompressionLoad,
    pub gravity_work: f64,
    pub kinetic_change: f64,
    pub incoming: f32,
    pub outgoing: f32,
    pub errors: [f64; 3],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DynamicCompressionStep {
    pub events: Vec<CompressionEvent>,
    pub intervals: Vec<DynamicCompressionInterval>,
    pub summary: Option<DynamicCompressionSummary>,
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
    // Appended variant: preserve the v15 Active/Invalidated tags and layout.
    Rebound(Box<ReboundActive>),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ReboundActive {
    scene: Scene,
    owner: Option<EpisodeId>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Active {
    scene: Scene,
    owner: EpisodeId,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Scene {
    plates: compression::scene::Scene,
    mass: f32,
    acceleration: f32,
}
impl Scene {
    fn read(world: &PhysicsWorld) -> Result<Self, R> {
        let plates = compression::scene::Scene::read_dynamic(world)?;
        let n = Vector::new(plates.normal[0], plates.normal[1]);
        let acceleration = world.raw.gravity.dot(n);
        let body = &world.raw.bodies[world
            .body_handle(plates.bodies[0])
            .ok_or(R::UnsupportedScene)?];
        let mut refreshed = body.clone();
        refreshed.recompute_mass_properties_from_colliders(&world.raw.colliders);
        if !acceleration.is_finite()
            || !(0.0..=1e6).contains(&acceleration)
            || world.raw.gravity != n * acceleration
            || !(5.0..=80.0).contains(&body.mass())
            || body.mass() != refreshed.mass()
            || body.local_center_of_mass() != refreshed.local_center_of_mass()
            || !body.local_center_of_mass().is_finite()
            || !body.mass_properties().effective_inv_mass.is_finite()
            || body.mass_properties().effective_inv_mass
                != refreshed.mass_properties().effective_inv_mass
            || body.mass_properties().effective_inv_mass.min_element() <= 0.0
            || body.mass_properties().effective_world_inv_inertia != 0.0
        {
            return Err(R::UnsupportedScene);
        }
        Ok(Self {
            plates,
            mass: body.mass(),
            acceleration,
        })
    }
}
impl Continuation {
    pub(in crate::world) fn invalidate(&mut self, reason: gravity::Invalidation) {
        if !matches!(self.status, Status::Invalidated(_)) {
            self.status = Status::Invalidated(reason);
        }
    }
    pub(in crate::world) fn snapshot_version(&self) -> u32 {
        if self.revision == 1 {
            DYNAMIC_COMPRESSION_SNAPSHOT_VERSION
        } else {
            REBOUND_SNAPSHOT_VERSION
        }
    }
    pub(in crate::world) fn snapshot_tag(&self) -> u64 {
        if self.revision == 1 {
            DYNAMIC_COMPRESSION_SNAPSHOT_TAG
        } else {
            REBOUND_SNAPSHOT_TAG
        }
    }
    fn active(&self) -> Option<(&Scene, Option<EpisodeId>)> {
        match &self.status {
            Status::Active(a) if self.revision == 1 => Some((&a.scene, Some(a.owner))),
            Status::Rebound(a) if self.revision == 2 => Some((&a.scene, a.owner)),
            _ => None,
        }
    }
    pub(in crate::world) fn valid_for(&self, world: &PhysicsWorld) -> bool {
        if self.revision != self.history.revision() || !self.history.valid() {
            return false;
        }
        if matches!(self.status, Status::Invalidated(_)) {
            return true;
        }
        let Some((saved, owner)) = self.active() else {
            return false;
        };
        let Ok(scene) = Scene::read(world) else {
            return false;
        };
        let loading = world.loading.as_ref().unwrap();
        let mut pair = scene.plates.sources;
        pair.sort();
        scene == *saved
            && scene.mass == self.history.mass
            && scene.acceleration == self.history.acceleration
            && scene.plates.x == self.history.summary.indentation
            && f64::from(scene.plates.speed) == self.history.summary.velocity
            && scene.plates.material == self.history.current_material()
            && owner.is_some() == (scene.plates.x >= self.history.summary.plastic_indentation)
            && loading.episodes.len() == usize::from(owner.is_some())
            && match owner {
                Some(id) => loading
                    .episodes
                    .get(&pair)
                    .is_some_and(|e| e.id == id && e.absent_seconds == 0.0),
                None => loading.episodes.is_empty(),
            }
    }
}

impl PhysicsWorld {
    /// Advance a finite-mass ideal rigid platen against a unilateral material layer.
    ///
    /// Uses the driven-compression rectangle/source/section/frame restrictions,
    /// with one dynamic, rotation-locked, nonsleeping body of refreshed mass
    /// `[5,80]` and gravity scale 1. The support alone has a material. Gravity is
    /// constant along local +x (zero is also allowed for a preloaded release).
    /// Enroll at rest: exactly touching with weight in (0,2Y], or elastically
    /// preloaded at 0 < x <= Y/k with weight <= Y. Initial elastic energy is
    /// explicit. Initial free flight and transfer from other histories refuse.
    ///
    /// The v15 contract retains monotone overload and exact safe equilibrium.
    /// Other admitted starts use the v16 rebound contract: signed material work,
    /// unloading, free flight and compliant recontact. Permanent indentation and
    /// fixed error budgets persist. Each contact episode has one Support owner;
    /// actual drift across the deformed surface ends/restarts ownership. No
    /// separate transient impact budget or rigid correction supplies energy.
    ///
    /// Positive dt <= 1/30; at most 1024 power-of-two subdivisions, bounded by
    /// elastic/viscous timescales. Actual intervals sum exactly to dt. Physical
    /// and numerical limits refuse atomically. Pause clears ephemeral output
    /// without enrollment. Public edits/other stepping permanently invalidate
    /// retained history. Snapshot v16 preserves v1-v15 selection and layouts.
    pub fn step_material_dynamic_compression(
        &mut self,
        dt: f32,
    ) -> Result<DynamicCompressionStep, compression::CompressionRejection> {
        step::advance(self, dt)
    }
    pub fn material_dynamic_compression_summary(&self) -> Option<DynamicCompressionSummary> {
        Some(
            self.loading
                .as_ref()?
                .dynamic_compression
                .as_ref()?
                .history
                .summary
                .clone(),
        )
    }
}

#[cfg(test)]
mod tests;
