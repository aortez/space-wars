//! Renewable point-contact supply in a terrain assembly's local frame.
use crate::{terrain::TerrainAssembly, world::PhysicsWorld};
use engine_core::plants::{Environment, Exposure, Role, Support, Tissue};
use engine_terrain::{CellCoord, MaterialId, Terrain, TerrainGeometry};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactReason {
    Eligible,
    Dead,
    NotRoot,
    Support,
    StaleGeometry,
    Air,
    Ineligible,
}

/// Rebuild this borrowed view at each physiology boundary. It owns no cache,
/// resources or physics world. One view serves one support-local plant frame.
pub struct RootUptake<'a> {
    pub world: &'a PhysicsWorld,
    pub assembly: &'a TerrainAssembly,
    pub terrain: &'a Terrain,
    pub geometry: &'a TerrainGeometry,
    /// Explicit material eligibility; VOID can never qualify.
    pub dirt: MaterialId,
    /// Independent fractions 0..=1000 of the configured root intake per second.
    pub rates: [u16; 2],
    pub availability: [u16; 2],
    /// Fixed simulation milliseconds per tick, 1..=1000. Maintenance uses the
    /// same fixed tick; changing duration requires retuning physiology config.
    pub tick_ms: u16,
}
impl RootUptake<'_> {
    pub fn contact(&self, t: &Tissue) -> ContactReason {
        if t.health == 0 {
            return ContactReason::Dead;
        }
        if t.role != Role::Root {
            return ContactReason::NotRoot;
        }
        if t.support != Some(self.assembly.plant_support())
            || !self.world.contains_body(self.assembly.body())
        {
            return ContactReason::Support;
        }
        if !self
            .assembly
            .plant_current(self.world, self.terrain, self.geometry)
        {
            return ContactReason::StaleGeometry;
        }
        // Roots are lightweight point absorbers at local cell centers. No
        // proximity tolerance or world-position cache admits air samples.
        let p = self
            .terrain
            .cell_center(CellCoord::new(t.cell[0], t.cell[1]));
        let Some(cell) = self.geometry.source_cell(self.terrain, p) else {
            return ContactReason::Air;
        };
        if self.dirt == MaterialId::VOID
            || self.terrain.cell(cell).map(|c| c.material) != Some(self.dirt)
        {
            return ContactReason::Ineligible;
        }
        ContactReason::Eligible
    }
    pub fn valid_controls(&self) -> bool {
        (1..=1000).contains(&self.tick_ms)
            && self
                .rates
                .into_iter()
                .chain(self.availability)
                .all(|x| x <= 1000)
    }
}
impl Environment for RootUptake<'_> {
    fn exposure(&self, t: &Tissue) -> Exposure {
        if !self.valid_controls() {
            return Exposure {
                nutrients: 1001,
                ..Exposure::default()
            };
        }
        let fractions: [u16; 2] = std::array::from_fn(|i| {
            (u64::from(self.rates[i]) * u64::from(self.availability[i]) * u64::from(self.tick_ms)
                / 1_000_000) as u16
        });
        Exposure {
            dirt_contact: self.contact(t) == ContactReason::Eligible,
            nutrients: fractions[0],
            water: fractions[1],
            light: 0,
        }
    }
    fn space_available(&self, cell: [i32; 2], role: Role, _: u16) -> bool {
        let p = self.terrain.cell_center(CellCoord::new(cell[0], cell[1]));
        let solid = self.geometry.source_cell(self.terrain, p).is_some();
        role == Role::Root || !solid
    }
    fn support_valid(&self, support: Option<Support>, _: [i32; 2]) -> bool {
        support == Some(self.assembly.plant_support())
            && self
                .assembly
                .plant_current(self.world, self.terrain, self.geometry)
    }
}

#[cfg(test)]
mod root_uptake_contract;
