//! Renewable point-contact supply in a terrain assembly's local frame.
use crate::{terrain::TerrainAssembly, world::PhysicsWorld};
use engine_core::plants::{Environment, Exposure, Role, Support, Tissue};
use engine_terrain::{CellCoord, MaterialId, Terrain, TerrainGeometry};

mod material;
pub use material::MaterialUptake;

fn material_contact(
    terrain: &Terrain,
    geometry: &TerrainGeometry,
    dirt: MaterialId,
    cell: [i32; 2],
) -> ContactReason {
    let p = terrain.cell_center(CellCoord::new(cell[0], cell[1]));
    let Some(cell) = geometry.source_cell(terrain, p) else {
        return ContactReason::Air;
    };
    if dirt == MaterialId::VOID || terrain.cell(cell).map(|c| c.material) != Some(dirt) {
        return ContactReason::Ineligible;
    }
    ContactReason::Eligible
}

fn supply_fractions(rates: [u16; 2], availability: [u16; 2], tick_ms: u16) -> [u16; 2] {
    std::array::from_fn(|i| {
        (u64::from(rates[i]) * u64::from(availability[i]) * u64::from(tick_ms) / 1_000_000) as u16
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactReason {
    Eligible,
    Dead,
    /// Shoots and runners have no passive contact supply.
    NotAbsorbing,
    Support,
    StaleGeometry,
    /// A root point outside solid geometry, or moss with no solid neighbor.
    Air,
    Ineligible,
    /// Moss whose own cell lies inside solid geometry rather than on it.
    Enclosed,
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
    /// Roots need dirt at their point; moss needs a solid surface of any material.
    pub fn contact(&self, t: &Tissue) -> ContactReason {
        if t.health == 0 {
            return ContactReason::Dead;
        }
        if !matches!(t.role, Role::Root | Role::Moss) {
            return ContactReason::NotAbsorbing;
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
        if t.role == Role::Moss {
            return crate::moss::contact(self.terrain, self.geometry, t.cell);
        }
        // Roots are lightweight point absorbers at local cell centers. No
        // proximity tolerance or world-position cache admits air samples.
        material_contact(self.terrain, self.geometry, self.dirt, t.cell)
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
        let fractions = supply_fractions(self.rates, self.availability, self.tick_ms);
        let eligible = self.contact(t) == ContactReason::Eligible;
        Exposure {
            dirt_contact: eligible && t.role == Role::Root,
            surface_contact: eligible && t.role == Role::Moss,
            nutrients: fractions[0],
            water: fractions[1],
            light: 0,
        }
    }
    fn space_available(&self, cell: [i32; 2], role: Role, _: u16) -> bool {
        if role == Role::Moss {
            return crate::moss::site(self.terrain, self.geometry, cell);
        }
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
