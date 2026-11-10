//! Finite-mass compliant loading, elastic return and flight with measured work.
use super::*;
use compression::{CompressionEvent, CompressionLoad, CompressionRejection as R};
use engine_material::{CRUSH_TIME_CONSTANT, SustainedLoad, sustained};

mod loads;
mod model;
mod moving;
mod paired;
mod step;
use model::{History, MaterialCurve};
pub use moving::MovingSupportSummary;
pub use paired::MaterialCompressionSummary;

/// Largest number of internal midpoint reaction solves per public call.
pub const MAX_DYNAMIC_COMPRESSION_STEPS: usize = 1024;
/// Maximum accepted changes of normal acceleration in one physical history.
pub const MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES: usize = 32;

/// Physical totals and fixed numerical allowances, retained after invalidation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DynamicCompressionSummary {
    pub indentation: f64,
    pub velocity: f64,
    pub plastic_indentation: f64,
    /// Relative kinetic energy; reduced mass is used for a moving support.
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
    /// A moving pair admits two position allowances; each body is also checked.
    pub absolute_errors: [f64; 3],
    /// Y*depth, sqrt(2*effective_mass*Y*depth), depth. Frozen at enrollment.
    pub scales: [f64; 3],
    pub elapsed: f64,
    pub solves: u64,
}

/// One internal interval, in execution order. The reaction is the mean
/// constitutive force in the work-preserving discrete solve, not a rigid
/// penetration-correction impulse. `load.force` is its endpoint reaction.
/// For paired materials, the two forces are individual endpoint law diagnostics;
/// both load records carry the same common reaction impulse, counted once.
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicCompressionInterval {
    pub at: f64,
    pub load: CompressionLoad,
    /// The incoming body's material load, when both bodies carry material.
    /// `load` remains the support load; their work sums to the pair's work.
    pub other_load: Option<CompressionLoad>,
    pub gravity_work: f64,
    /// Relative kinetic-energy change (reduced mass for a moving support).
    pub kinetic_change: f64,
    /// Relative endpoint speed, rounded to f32 for a moving pair.
    pub incoming: f32,
    /// Relative endpoint speed, rounded to f32 for a moving pair.
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
    Falling(Box<FallingActive>),
    ChangingLoad(Box<FallingActive>),
    MovingSupport(Box<FallingActive>),
    PairedMaterials(Box<FallingActive>),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FallingActive {
    scene: Scene,
    owner: Option<EpisodeId>,
    /// The first impact interval has committed; continuation must observe Support
    /// with the same owner on the next interval, including after a save.
    pending_support: bool,
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
        let moving = world.raw.bodies.iter().all(|(_, b)| b.is_dynamic());
        let plates = if moving {
            compression::scene::Scene::read_moving(world)?
        } else {
            compression::scene::Scene::read_dynamic(world)?
        };
        let n = Vector::new(plates.normal[0], plates.normal[1]);
        let acceleration = world.raw.gravity.dot(n);
        let mass = Self::mass(world, plates.bodies[0])?;
        if moving {
            Self::mass(world, plates.bodies[1])?;
        }
        if !acceleration.is_finite()
            || !(0.0..=1e6).contains(&acceleration)
            || world.raw.gravity != n * acceleration
        {
            return Err(R::UnsupportedScene);
        }
        Ok(Self {
            plates,
            mass,
            acceleration,
        })
    }

    fn mass(world: &PhysicsWorld, id: BodyId) -> Result<f32, R> {
        let body = &world.raw.bodies[world.body_handle(id).ok_or(R::UnsupportedScene)?];
        let mut refreshed = body.clone();
        refreshed.recompute_mass_properties_from_colliders(&world.raw.colliders);
        if !(5.0..=80.0).contains(&body.mass())
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
        Ok(body.mass())
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
        } else if self.revision == 2 {
            REBOUND_SNAPSHOT_VERSION
        } else if self.revision == 3 {
            FALLING_SUPPORT_SNAPSHOT_VERSION
        } else if self.revision == 4 {
            CHANGING_LOAD_SNAPSHOT_VERSION
        } else if self.revision == 5 {
            MOVING_SUPPORT_SNAPSHOT_VERSION
        } else {
            PAIRED_COMPRESSION_SNAPSHOT_VERSION
        }
    }
    pub(in crate::world) fn snapshot_tag(&self) -> u64 {
        if self.revision == 1 {
            DYNAMIC_COMPRESSION_SNAPSHOT_TAG
        } else if self.revision == 2 {
            REBOUND_SNAPSHOT_TAG
        } else if self.revision == 3 {
            FALLING_SUPPORT_SNAPSHOT_TAG
        } else if self.revision == 4 {
            CHANGING_LOAD_SNAPSHOT_TAG
        } else if self.revision == 5 {
            MOVING_SUPPORT_SNAPSHOT_TAG
        } else {
            PAIRED_COMPRESSION_SNAPSHOT_TAG
        }
    }
    pub(in crate::world) fn serialize_extensions(
        &self,
        bytes: &mut Vec<u8>,
    ) -> Result<(), PhysicsWorldError> {
        if self.revision == 3 {
            let incoming = self
                .history
                .incoming
                .as_ref()
                .ok_or_else(|| PhysicsWorldError("missing falling initial motion".into()))?;
            bincode::serialize_into(&mut *bytes, incoming)
                .map_err(|e| PhysicsWorldError(e.to_string()))?;
        } else if self.revision == 4 {
            self.history.serialize_loads(bytes)?;
        } else if self.revision >= 5 {
            let moving = self
                .history
                .moving
                .as_ref()
                .ok_or_else(|| PhysicsWorldError("missing moving-support history".into()))?;
            bincode::serialize_into(&mut *bytes, moving)
                .map_err(|e| PhysicsWorldError(e.to_string()))?;
        }
        if self.revision == 6 {
            let paired = self
                .history
                .paired
                .as_ref()
                .ok_or_else(|| PhysicsWorldError("missing paired material history".into()))?;
            bincode::serialize_into(bytes, paired).map_err(|e| PhysicsWorldError(e.to_string()))?;
        }
        Ok(())
    }
    pub(in crate::world) fn deserialize_extensions(
        &mut self,
        reader: &mut impl std::io::Read,
    ) -> Result<(), PhysicsWorldError> {
        if self.revision == 3 {
            self.history.incoming = Some(
                bincode::deserialize_from(&mut *reader)
                    .map_err(|e| PhysicsWorldError(e.to_string()))?,
            );
        } else if self.revision == 4 {
            self.history.deserialize_loads(reader)?;
        } else if self.revision >= 5 {
            self.history.moving = Some(
                bincode::deserialize_from(&mut *reader)
                    .map_err(|e| PhysicsWorldError(e.to_string()))?,
            );
        }
        if self.revision == 6 {
            self.history.paired = Some(
                bincode::deserialize_from(reader).map_err(|e| PhysicsWorldError(e.to_string()))?,
            );
        }
        Ok(())
    }
    fn active(&self) -> Option<(&Scene, Option<EpisodeId>)> {
        match &self.status {
            Status::Active(a) if self.revision == 1 => Some((&a.scene, Some(a.owner))),
            Status::Rebound(a) if self.revision == 2 => Some((&a.scene, a.owner)),
            Status::Falling(a)
                if self.revision == 3 && (!a.pending_support || a.owner.is_some()) =>
            {
                Some((&a.scene, a.owner))
            }
            Status::ChangingLoad(a)
                if self.revision == 4
                    && (!a.pending_support
                        || (a.owner.is_some() && self.history.incoming.is_some())) =>
            {
                Some((&a.scene, a.owner))
            }
            Status::MovingSupport(a)
                if self.revision == 5 && (!a.pending_support || a.owner.is_some()) =>
            {
                Some((&a.scene, a.owner))
            }
            Status::PairedMaterials(a)
                if self.revision == 6 && (!a.pending_support || a.owner.is_some()) =>
            {
                Some((&a.scene, a.owner))
            }
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
            && scene.acceleration == self.history.current_acceleration()
            && scene.plates.x == self.history.summary.indentation
            && if let Some(moving) = &self.history.moving {
                moving.live(world, &scene)
            } else {
                world.body_kind(scene.plates.bodies[1]) == Some(BodyKind::Fixed)
                    && f64::from(scene.plates.speed) == self.history.summary.velocity
            }
            && scene.plates.material == self.history.current_material()
            && self.history.paired.as_ref().is_none_or(|p| {
                world
                    .loading
                    .as_ref()
                    .unwrap()
                    .response
                    .as_ref()
                    .unwrap()
                    .materials
                    .get(&scene.plates.sources[0].region)
                    == Some(&p.material(0))
            })
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
    /// Advance finite-mass normal compression against unilateral material layers.
    ///
    /// Uses the driven-compression rectangle/source/section/frame restrictions,
    /// with one dynamic, rotation-locked, nonsleeping body of refreshed mass
    /// `[5,80]` and gravity scale 1. The support alone has a material. Gravity is
    /// along local +x, including zero for release or incoming motion. Once
    /// enrolled, `set_material_dynamic_compression_acceleration` can change it
    /// while retaining the same physical history and numerical allowances.
    /// Resting touching/preloaded starts retain their v15/v16 contracts.
    /// Revision 3 also admits -depth/4 <= x0 <= 0 with normal speed `[0,64]`
    /// and weight <= 2Y. A detached start needs positive speed or gravity;
    /// touching arrival needs positive speed. Initial kinetic energy/momentum
    /// are retained explicitly. Imported histories and plastic preload refuse.
    /// Revision 5 also admits a dynamic support of refreshed mass `[5,80]`,
    /// with both bodies rotation-locked and moving only along the normal.
    /// External gravity is zero, initial relative speed is `(0,64]`, and each
    /// body's speed stays in `[-64,64]`. Only the support carries material.
    /// The same law uses reduced mass and equal/opposite impulses; the companion
    /// `material_moving_support_summary` retains both laboratory-frame balances.
    /// Revision 6 admits finite material on both free bodies, with separate
    /// deformation, capacity and charge accounting under a common work reaction.
    /// Both materials use the same thickness/depth and elastic-limit bounds.
    /// The smaller depth bounds total indentation; the smaller yield force and
    /// depth set the fixed pair scales. `material_paired_compression_summary`
    /// returns the two histories, in incoming-body/support geometric order.
    ///
    /// Signed material work covers loading, unloading, separation and return.
    /// Falling contact emits Impact, then Support with the same owner on the
    /// next interval. Support observes continued contact; it does not imply rest.
    /// All phases use one compliant work calculation, with no separate impact
    /// charge. Actual drift across the deformed surface ends/restarts ownership;
    /// permanent set, capacity and fixed numerical budgets persist.
    ///
    /// Positive dt <= 1/30; at most 1024 power-of-two subdivisions, bounded by
    /// elastic/viscous timescales. Actual intervals sum exactly to dt. Physical
    /// and numerical limits refuse atomically. Pause clears ephemeral output
    /// without enrollment. Public edits/other stepping permanently invalidate
    /// retained history, except the explicit acceleration API for fixed supports.
    /// Snapshot v20 retains paired materials and both motion histories;
    /// earlier modes keep their v1-v19 layouts and stepping behavior.
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
