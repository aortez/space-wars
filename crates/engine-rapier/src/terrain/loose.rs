//! Shared material-release lifecycle. Owns loose samples and their body handles;
//! borrows the caller's terrain and canonical world, and never steps a solver.
use super::*;
use engine_terrain::{DetachedCell, MaterialId, TerrainEdit, TerrainError};
use std::collections::BTreeMap;

mod deposition;
pub use deposition::DepositCommit;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LooseTerrainConfig {
    pub max_grains: usize,
    pub shape: GrainShape,
    pub friction: f32,
    pub restitution: f32,
}

impl Default for LooseTerrainConfig {
    fn default() -> Self {
        Self {
            max_grains: 192,
            shape: GrainShape::Round,
            friction: 0.6,
            restitution: 0.0,
        }
    }
}

/// An unpublished field edit. Preparing cannot change the source or its world.
#[derive(Debug, Clone)]
pub struct PreparedRelease {
    source: Terrain,
    terrain: Terrain,
    grains: Vec<DetachedCell>,
    detached: Vec<DetachedTerrain>,
    changed_cells: u64,
    discarded_cells: u64,
}

/// Whether departing cells become physical material or leave the simulation
/// (for example, mining). Ordering is retained until the field is split.
#[derive(Debug, Clone, Copy)]
pub enum MaterialEdit {
    Release(TerrainEdit),
    Discard(TerrainEdit),
}

impl PreparedRelease {
    pub fn new(source: &Terrain, edits: &[TerrainEdit]) -> Result<Self, TerrainError> {
        Self::from_edits(source, edits.iter().copied().map(MaterialEdit::Release))
    }

    pub fn from_edits(
        source: &Terrain,
        edits: impl IntoIterator<Item = MaterialEdit>,
    ) -> Result<Self, TerrainError> {
        let mut terrain = source.clone();
        let mut grains = Vec::new();
        let mut changed_cells = 0;
        let mut discarded_cells = 0;
        for edit in edits {
            let result = match edit {
                MaterialEdit::Release(edit) => {
                    let result = terrain.apply_releasing(edit)?;
                    grains.extend(result.cells);
                    result.edit
                }
                MaterialEdit::Discard(edit) => {
                    let result = terrain.apply(edit)?;
                    discarded_cells += result
                        .removed
                        .iter()
                        .map(|m| u64::from(m.cells))
                        .sum::<u64>();
                    result
                }
            };
            changed_cells += u64::from(result.changed_cells);
        }
        let detached = if grains.is_empty() && discarded_cells == 0 {
            Vec::new()
        } else {
            terrain.detach_disconnected()?
        };
        Ok(Self {
            source: source.clone(),
            terrain,
            grains,
            detached,
            changed_cells,
            discarded_cells,
        })
    }
    pub fn grain_count(&self) -> usize {
        self.grains.len()
    }
    pub fn fragment_count(&self) -> usize {
        self.detached.len()
    }
    pub fn changed_cells(&self) -> u64 {
        self.changed_cells
    }
    pub fn discarded_cells(&self) -> u64 {
        self.discarded_cells
    }
    pub fn source_empty(&self) -> bool {
        self.terrain
            .cells()
            .iter()
            .all(|c| c.material == MaterialId::VOID)
    }
}

pub struct TerrainBodyMut<'a> {
    pub terrain: &'a mut Terrain,
    pub geometry: &'a mut TerrainGeometry,
    pub assembly: &'a mut TerrainAssembly,
}

#[derive(Debug)]
pub struct ReleaseCommit {
    /// The caller registers these as ordinary terrain fields, so subsequent
    /// mining/impacts and scenario support rules use its existing field registry.
    pub fragments: Vec<TerrainFragment>,
    pub dirty_chunks: Vec<ChunkId>,
    pub rebuilt_chunks: usize,
    pub new_grains: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseError {
    Capacity,
    Invalid(TerrainError),
}
impl std::fmt::Display for ReleaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Capacity => f.write_str("loose material limit"),
            Self::Invalid(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for ReleaseError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialQuantity {
    pub material: MaterialId,
    pub cells: u64,
    /// Nominal unit-depth square-cell quantity; contact proxies have pore space.
    pub area: f64,
}

#[derive(Debug, Clone)]
pub struct LooseTerrain {
    config: LooseTerrainConfig,
    grains: Vec<TerrainGrain>,
    settling: BTreeMap<PhysicsId, deposition::SettlingState>,
    deposited_cells: u64,
}

impl LooseTerrain {
    pub fn new(config: LooseTerrainConfig) -> Result<Self, TerrainError> {
        if !(1..=4096).contains(&config.max_grains)
            || !config.friction.is_finite()
            || config.friction < 0.0
            || !config.restitution.is_finite()
            || !(0.0..=1.0).contains(&config.restitution)
        {
            return Err(TerrainError("invalid loose material configuration"));
        }
        Ok(Self {
            config,
            grains: Vec::new(),
            settling: BTreeMap::new(),
            deposited_cells: 0,
        })
    }
    pub fn config(&self) -> LooseTerrainConfig {
        self.config
    }
    pub fn len(&self) -> usize {
        self.grains.len()
    }
    pub fn is_empty(&self) -> bool {
        self.grains.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &TerrainGrain> {
        self.grains.iter()
    }
    pub fn can_admit(&self, additional: usize) -> bool {
        additional <= self.config.max_grains.saturating_sub(self.len())
    }
    pub fn quantities(&self) -> Vec<MaterialQuantity> {
        let mut amounts = BTreeMap::<MaterialId, (u64, f64)>::new();
        for grain in self.iter() {
            let quantity = amounts.entry(grain.cell().material).or_default();
            quantity.0 += 1;
            quantity.1 += f64::from(grain.cell_size()).powi(2);
        }
        amounts
            .into_iter()
            .map(|(material, (cells, area))| MaterialQuantity {
                material,
                cells,
                area,
            })
            .collect()
    }

    /// Commit between solver steps. Capacity/staleness/identity failures leave
    /// field, existing bodies, pool and allocator unchanged. New destinations
    /// are inserted before publishing the field edit; failed insertion removes
    /// only those unpublished destinations. No mining/destruction is credited.
    ///
    /// For an event spanning several fields, prepare all plans and admit their
    /// aggregate grain/fragment count before committing any plan. The caller
    /// supplies its allocator to share identity ordering with solid fragments.
    pub fn commit(
        &mut self,
        world: &mut PhysicsWorld,
        source: TerrainBodyMut<'_>,
        plan: PreparedRelease,
        next_id: &mut u64,
    ) -> Result<ReleaseCommit, ReleaseError> {
        let invalid = |s| ReleaseError::Invalid(TerrainError(s));
        if *source.terrain != plan.source
            || !source.geometry.is_current(source.terrain)
            || source.geometry.surface() != source.assembly.spec.surface
        {
            return Err(invalid("stale material release source"));
        }
        if !self.can_admit(plan.grain_count()) {
            return Err(ReleaseError::Capacity);
        }
        let parent = world
            .motion(source.assembly.body())
            .ok_or(invalid("missing material body"))?;
        let center = world
            .center_of_mass(source.assembly.body())
            .ok_or(invalid("missing material center"))?;
        let count = plan.grain_count() + plan.fragment_count();
        let end = next_id
            .checked_add(count as u64)
            .ok_or(invalid("material identity exhausted"))?;
        if (*next_id..end).any(|id| world.contains_entity(PhysicsId::new(id))) {
            return Err(invalid("material identity already exists"));
        }
        let new_grains = plan.grain_count();
        let mut fragments = Vec::new();
        let mut grains = Vec::new();
        let mut id = *next_id;
        let inserted = (|| {
            for piece in plan.detached {
                fragments.push(TerrainFragment::insert(
                    world,
                    PhysicsId::new(id),
                    piece,
                    parent,
                    center,
                    source.assembly.spec,
                )?);
                id += 1;
            }
            let grain_spec = TerrainSpec {
                friction: self.config.friction,
                restitution: self.config.restitution,
                ..source.assembly.spec
            };
            for seed in plan.grains {
                grains.push(TerrainGrain::insert_with_shape(
                    world,
                    PhysicsId::new(id),
                    seed,
                    parent,
                    center,
                    grain_spec,
                    self.config.shape,
                )?);
                id += 1;
            }
            Some(())
        })();
        if inserted.is_none() {
            for id in *next_id..id {
                world.remove_entity(PhysicsId::new(id));
            }
            return Err(invalid("material destination insertion failed"));
        }
        // Existing valid assembly + a valid edit of its same field dimensions.
        // This is the same synchronous geometry boundary used by terrain edits.
        *source.terrain = plan.terrain;
        let dirty_chunks = source.geometry.refresh(source.terrain);
        let rebuilt_chunks = source
            .assembly
            .synchronize(world, source.terrain, source.geometry)
            .expect("validated material source and geometry");
        self.grains.extend(grains);
        *next_id = end;
        Ok(ReleaseCommit {
            fragments,
            dirty_chunks,
            rebuilt_chunks,
            new_grains,
        })
    }

    pub fn audit(&self, world: &PhysicsWorld) -> Result<(), TerrainError> {
        if self.len() > self.config.max_grains {
            return Err(TerrainError("loose material over capacity"));
        }
        for grain in self.iter() {
            let m = world
                .motion(grain.body())
                .ok_or(TerrainError("missing loose body"))?;
            let mass = world
                .body_mass(grain.body())
                .ok_or(TerrainError("missing loose mass"))?;
            let expected = grain.cell_size().powi(2);
            if ![
                m.position.x,
                m.position.y,
                m.angle,
                m.linear_velocity.x,
                m.linear_velocity.y,
                m.angular_velocity,
                mass,
            ]
            .into_iter()
            .all(f32::is_finite)
                || (mass - expected).abs() > expected * 0.0001
            {
                return Err(TerrainError("invalid loose material motion or mass"));
            }
        }
        Ok(())
    }
}

/// A bounded radial velocity change, not a calibrated pressure/energy model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadialImpulse {
    pub center: Vec2,
    pub radius: f32,
    pub speed: f32,
}
impl RadialImpulse {
    pub fn apply(
        self,
        world: &mut PhysicsWorld,
        bodies: impl IntoIterator<Item = BodyId>,
    ) -> usize {
        if ![self.center.x, self.center.y, self.radius, self.speed]
            .into_iter()
            .all(f32::is_finite)
            || self.radius <= 0.0
            || self.speed < 0.0
        {
            return 0;
        }
        let mut moved = 0;
        for body in bodies {
            let Some(center) = world.center_of_mass(body) else {
                continue;
            };
            let offset = center - self.center;
            let distance = offset.length();
            if distance > 0.0001 && distance < self.radius {
                let delta = offset / distance * (self.speed * (1.0 - distance / self.radius));
                if delta.length_squared() > 0.0
                    && delta.x.is_finite()
                    && delta.y.is_finite()
                    && world.apply_velocity_delta(body, delta, true)
                {
                    moved += 1;
                }
            }
        }
        moved
    }
}

#[cfg(test)]
mod tests;
