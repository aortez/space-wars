//! Bounded overhead-only environment publication for one support-local tissue set.
use crate::{root_uptake::RootUptake, world::OpaqueFootprint};
use engine_core::{
    Color,
    lighting::{self, DirectionalSource, LightId, LightingError, LightingPublisher, Receiver},
    plants::{Environment, Exposure, Role, Support, Tissue},
};
use engine_terrain::CellCoord;
use std::collections::BTreeMap;

pub const MAX_TISSUES: usize = 256;
pub const DIM_ROOM: f32 = 50.0;

/// Direction is already in the simulation's world frame, independent of gravity.
pub fn full_sun(toward_source: engine_core::Vec2) -> DirectionalSource {
    DirectionalSource {
        toward_source,
        strength: lighting::FULL_SUN,
        color: Color::WHITE,
    }
}
pub fn dim_room(toward_source: engine_core::Vec2) -> DirectionalSource {
    DirectionalSource {
        strength: DIM_ROOM,
        ..full_sun(toward_source)
    }
}

/// World-space disks from all other relevant supports, transformed by their
/// simulation owners after the same completed canonical step. Rebuild after
/// motion, shape changes or removal; tags must match the receiving publication.
/// Identities share the local/physical namespace policy and must be unique.
pub struct WorldOccluders<'a> {
    pub tick: u64,
    pub geometry_generation: u64,
    pub footprints: &'a [OpaqueFootprint],
}

/// One publication per completed tick, shared by every plant on this support.
/// Borrowing root inputs prevents geometry/world mutation before physiology.
/// Use one publisher for the entire admitted set, never one per query batch.
pub struct PlantLight<'a, 'w> {
    roots: &'a RootUptake<'w>,
    samples: BTreeMap<u64, (Tissue, lighting::Exposure)>,
    tick: u64,
    geometry_generation: u64,
    pub work: crate::world::VisibilityCounters,
}
impl<'a, 'w> PlantLight<'a, 'w> {
    /// `namespace` must be reserved from physical IDs and other tissue sets;
    /// tissue IDs must never be reused within that namespace's lifetime.
    /// Call after the normal canonical step and before the matching plant tick.
    pub fn publish(
        publisher: &mut LightingPublisher,
        tick: u64,
        geometry_generation: u64,
        source: DirectionalSource,
        roots: &'a RootUptake<'w>,
        tissues: &[Tissue],
        namespace: u64,
    ) -> Result<Self, LightingError> {
        Self::publish_with_occluders(
            publisher,
            tick,
            geometry_generation,
            source,
            roots,
            tissues,
            namespace,
            WorldOccluders {
                tick,
                geometry_generation,
                footprints: &[],
            },
        )
    }

    /// Publish with world-space opaque tissue from other relevant supports.
    /// Local tissues plus external disks share MAX_TISSUES admission, including
    /// roots/dead elements. Never partition this input to bypass the work cap.
    pub fn publish_with_occluders(
        publisher: &mut LightingPublisher,
        tick: u64,
        geometry_generation: u64,
        source: DirectionalSource,
        roots: &'a RootUptake<'w>,
        tissues: &[Tissue],
        namespace: u64,
        occluders: WorldOccluders<'_>,
    ) -> Result<Self, LightingError> {
        if occluders.tick != tick {
            return Err(LightingError::Stale);
        }
        if occluders.geometry_generation != geometry_generation {
            return Err(LightingError::GeometryGeneration);
        }
        if publisher.snapshot().is_some_and(|previous| {
            tick <= previous.tick() || geometry_generation < previous.geometry_generation()
        }) {
            return Err(LightingError::Stale);
        }
        if tissues.len() > MAX_TISSUES || occluders.footprints.len() > MAX_TISSUES - tissues.len() {
            return Err(LightingError::Capacity);
        }
        if !roots.valid_controls()
            || !roots.support_valid(Some(roots.assembly.plant_support()), [0, 0])
        {
            return Err(LightingError::InvalidInput);
        }
        // Validate source before using its direction to place samples.
        let snapshot = lighting::LightingSnapshot::new(tick, geometry_generation, source, &[])?;
        let motion = roots
            .world
            .motion(roots.assembly.body())
            .ok_or(LightingError::InvalidInput)?;
        let mut identities = std::collections::BTreeSet::new();
        let mut footprints = Vec::new();
        let mut receivers = Vec::new();
        let mut shoots = Vec::new();
        for tissue in tissues {
            if !identities.insert(tissue.id) {
                return Err(LightingError::ConflictingIdentity);
            }
            if tissue.size == 0 || tissue.support != Some(roots.assembly.plant_support()) {
                return Err(LightingError::InvalidInput);
            }
            if tissue.role == Role::Root {
                continue;
            }
            let center = motion.position
                + roots
                    .terrain
                    .cell_center(CellCoord::new(tissue.cell[0], tissue.cell[1]))
                    .rotate_radians(motion.angle);
            // A declared opaque disk, not a collider or a physiological size rule.
            let radius = roots.terrain.cell_size() * 0.25 * f32::from(tissue.size).sqrt();
            let id = LightId(namespace, tissue.id);
            footprints.push(OpaqueFootprint { id, center, radius });
            if tissue.role == Role::Shoot && tissue.health > 0 {
                receivers.push(Receiver {
                    position: center + source.toward_source * radius,
                    surface: Some(id),
                });
                shoots.push(tissue);
            }
        }
        footprints.extend_from_slice(occluders.footprints);
        let geometry = roots.world.query_snapshot();
        let visibility = geometry.lighting_visibility(&footprints, &[])?;
        let mut samples = BTreeMap::new();
        for (batch, tissue_batch) in receivers
            .chunks(lighting::MAX_RECEIVERS)
            .zip(shoots.chunks(lighting::MAX_RECEIVERS))
        {
            for (tissue, exposure) in tissue_batch.iter().zip(snapshot.query(
                tick,
                geometry_generation,
                batch,
                &visibility,
            )?) {
                samples.insert(tissue.id, ((*tissue).clone(), exposure));
            }
        }
        // Commit only after all admission and query validation succeeds.
        publisher.publish(tick, geometry_generation, source, &[])?;
        Ok(Self {
            roots,
            samples,
            tick,
            geometry_generation,
            work: visibility.counters(),
        })
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn geometry_generation(&self) -> u64 {
        self.geometry_generation
    }

    pub fn diagnostic(
        &self,
        tissue: &Tissue,
        tick: u64,
    ) -> Result<&lighting::Exposure, LightingError> {
        if tick != self.tick {
            return Err(LightingError::Stale);
        }
        let (published, exposure) = self
            .samples
            .get(&tissue.id)
            .ok_or(LightingError::InvalidInput)?;
        if published != tissue {
            return Err(LightingError::GeometryGeneration);
        }
        Ok(exposure)
    }

    /// Enforce zero exposure age at the physiology boundary.
    pub fn environment(&self, tick: u64) -> Result<impl Environment + '_, LightingError> {
        if tick != self.tick {
            return Err(LightingError::Stale);
        }
        Ok(LitEnvironment(self))
    }
}
struct LitEnvironment<'a, 'b, 'w>(&'a PlantLight<'b, 'w>);
impl Environment for LitEnvironment<'_, '_, '_> {
    fn exposure(&self, tissue: &Tissue) -> Exposure {
        let mut exposure = self.0.roots.exposure(tissue);
        if tissue.role == Role::Shoot && tissue.health > 0 {
            // Incident intensity also drives shade classification. Energy uses
            // the runtime's configured per-logical-tick coefficient separately.
            exposure.light = self
                .0
                .diagnostic(tissue, self.0.tick)
                .map(|e| e.intensity.floor() as u16)
                .unwrap_or(1001);
        }
        exposure
    }
    fn space_available(&self, cell: [i32; 2], role: Role, size: u16) -> bool {
        self.0.roots.space_available(cell, role, size)
    }
    fn support_valid(&self, support: Option<Support>, cell: [i32; 2]) -> bool {
        self.0.roots.support_valid(support, cell)
    }
}

#[cfg(test)]
mod plant_light_contract;
