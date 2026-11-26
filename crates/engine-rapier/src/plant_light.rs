//! Bounded overhead-only environment publication for one support-local tissue set.
use crate::{
    root_uptake::RootUptake,
    world::{BodyMotion, OpaqueFootprint, TranslucentFootprint},
};
use engine_core::{
    Color,
    lighting::{self, DirectionalSource, LightId, LightingError, LightingPublisher, Receiver},
    plants::{Environment, Exposure, Role, Support, Tissue},
};
use engine_terrain::CellCoord;
use std::collections::BTreeMap;

pub const MAX_TISSUES: usize = 256;
/// Empty cells measured per publication, one query batch.
pub const MAX_PROBES: usize = lighting::MAX_RECEIVERS;
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
    probes: BTreeMap<[i32; 2], lighting::Exposure>,
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
        Self::publish_with_probes(
            publisher,
            tick,
            geometry_generation,
            source,
            roots,
            tissues,
            namespace,
            occluders,
            &[],
            &|_| 0.0,
        )
    }

    /// Also measure light at up to MAX_PROBES distinct cells of the terrain,
    /// each sampled where a size-one blade there would sample; a cell inside
    /// solid geometry reads dark. Probes shade nothing; a tree reads them to
    /// choose where to grow. `transmits` gives the share of the light each
    /// tissue's disk lets through, below 1; 0 keeps it opaque, as the other
    /// publications do.
    #[allow(clippy::too_many_arguments)]
    pub fn publish_with_probes(
        publisher: &mut LightingPublisher,
        tick: u64,
        geometry_generation: u64,
        source: DirectionalSource,
        roots: &'a RootUptake<'w>,
        tissues: &[Tissue],
        namespace: u64,
        occluders: WorldOccluders<'_>,
        probes: &[[i32; 2]],
        transmits: &dyn Fn(&Tissue) -> f32,
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
        if tissues.len() > MAX_TISSUES
            || occluders.footprints.len() > MAX_TISSUES - tissues.len()
            || probes.len() > MAX_PROBES
        {
            return Err(LightingError::Capacity);
        }
        let distinct: std::collections::BTreeSet<_> = probes.iter().collect();
        if distinct.len() != probes.len()
            || probes.iter().any(|cell| {
                roots
                    .terrain
                    .cell(CellCoord::new(cell[0], cell[1]))
                    .is_none()
            })
        {
            return Err(LightingError::InvalidInput);
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
        let mut translucent = Vec::new();
        let mut receivers = Vec::new();
        let mut shoots = Vec::new();
        for tissue in tissues {
            if !identities.insert(tissue.id) {
                return Err(LightingError::ConflictingIdentity);
            }
            if tissue.size == 0 || tissue.support != Some(roots.assembly.plant_support()) {
                return Err(LightingError::InvalidInput);
            }
            let (center, footprint) = disk(roots, &motion, tissue, namespace);
            let share = transmits(tissue);
            if !(0.0..1.0).contains(&share) {
                return Err(LightingError::InvalidInput);
            }
            match footprint {
                Some(d) if share > 0.0 => translucent.push(TranslucentFootprint {
                    id: d.id,
                    center: d.center,
                    radius: d.radius,
                    transmits: share,
                }),
                Some(d) => footprints.push(d),
                None => {}
            }
            if tissue.health == 0 {
                continue;
            }
            match (tissue.role, footprint) {
                // A thin moss mat samples at its own surface point and shades nothing.
                (Role::Moss, _) => receivers.push(Receiver {
                    position: center,
                    surface: None,
                }),
                (Role::Shoot, Some(disk)) => receivers.push(Receiver {
                    position: center + source.toward_source * disk.radius,
                    surface: Some(disk.id),
                }),
                _ => continue,
            }
            shoots.push(tissue);
        }
        footprints.extend_from_slice(occluders.footprints);
        let blade = roots.terrain.cell_size() * 0.25;
        let probe_receivers: Vec<Receiver> = probes
            .iter()
            .map(|&cell| Receiver {
                position: center(roots, &motion, cell) + source.toward_source * blade,
                surface: None,
            })
            .collect();
        let geometry = roots.world.query_snapshot();
        let visibility = geometry
            .lighting_visibility(&footprints, &[])?
            .with_translucent(&translucent)?;
        let measured = if probes.is_empty() {
            Vec::new()
        } else {
            snapshot.query(tick, geometry_generation, &probe_receivers, &visibility)?
        };
        let probes = probes.iter().copied().zip(measured).collect();
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
            probes,
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

    /// Light this tick at a cell probed in this publication.
    pub fn probe(&self, cell: [i32; 2], tick: u64) -> Result<&lighting::Exposure, LightingError> {
        if tick != self.tick {
            return Err(LightingError::Stale);
        }
        self.probes.get(&cell).ok_or(LightingError::InvalidInput)
    }

    /// Enforce zero exposure age at the physiology boundary.
    pub fn environment(&self, tick: u64) -> Result<impl Environment + '_, LightingError> {
        if tick != self.tick {
            return Err(LightingError::Stale);
        }
        Ok(LitEnvironment(self))
    }
}
/// World center of a support-local cell.
fn center(roots: &RootUptake<'_>, motion: &BodyMotion, cell: [i32; 2]) -> engine_core::Vec2 {
    motion.position
        + roots
            .terrain
            .cell_center(CellCoord::new(cell[0], cell[1]))
            .rotate_radians(motion.angle)
}

/// World center of a tissue's cell and its declared opaque disk, if any.
fn disk(
    roots: &RootUptake<'_>,
    motion: &BodyMotion,
    tissue: &Tissue,
    namespace: u64,
) -> (engine_core::Vec2, Option<OpaqueFootprint>) {
    let center = center(roots, motion, tissue.cell);
    // A declared opaque disk, not a collider or a physiological size rule.
    // Roots are underground and thin moss is flat against its surface. Wood
    // shades as a blade does but samples no light of its own.
    let footprint =
        matches!(tissue.role, Role::Runner | Role::Shoot | Role::Wood).then(|| OpaqueFootprint {
            id: LightId(namespace, tissue.id),
            center,
            radius: roots.terrain.cell_size() * 0.25 * f32::from(tissue.size).sqrt(),
        });
    (center, footprint)
}

/// The world-space disks a publication declares for this support's tissues,
/// for a host to pass to other supports' publications as `WorldOccluders`.
/// Recompute after any motion, growth or removal; roots and moss declare none.
pub fn footprints(
    roots: &RootUptake<'_>,
    tissues: &[Tissue],
    namespace: u64,
) -> Result<Vec<OpaqueFootprint>, LightingError> {
    let motion = roots
        .world
        .motion(roots.assembly.body())
        .ok_or(LightingError::InvalidInput)?;
    tissues
        .iter()
        .map(|tissue| {
            if tissue.size == 0 || tissue.support != Some(roots.assembly.plant_support()) {
                return Err(LightingError::InvalidInput);
            }
            Ok(disk(roots, &motion, tissue, namespace).1)
        })
        .filter_map(Result::transpose)
        .collect()
}

struct LitEnvironment<'a, 'b, 'w>(&'a PlantLight<'b, 'w>);
impl Environment for LitEnvironment<'_, '_, '_> {
    fn exposure(&self, tissue: &Tissue) -> Exposure {
        let mut exposure = self.0.roots.exposure(tissue);
        if matches!(tissue.role, Role::Shoot | Role::Moss) && tissue.health > 0 {
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
