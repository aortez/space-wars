//! Material-owned point contact for hosts whose soil spans several moving
//! supports. Physics handles are derived bindings, not the organism's identity.
use super::*;

pub struct MaterialUptake<'a> {
    pub terrain: &'a Terrain,
    pub geometry: &'a TerrainGeometry,
    pub dirt: MaterialId,
    /// Stable material attachment, independent of a particular arena lifetime.
    pub support: Support,
    pub rates: [u16; 2],
    pub availability: [u16; 2],
    pub tick_ms: u16,
}
impl MaterialUptake<'_> {
    pub fn contact(&self, tissue: &Tissue) -> ContactReason {
        if tissue.health == 0 {
            return ContactReason::Dead;
        }
        if !matches!(tissue.role, Role::Root | Role::Moss) {
            return ContactReason::NotAbsorbing;
        }
        if tissue.support != Some(self.support) {
            return ContactReason::Support;
        }
        if !self.geometry.is_current(self.terrain) {
            return ContactReason::StaleGeometry;
        }
        if tissue.role == Role::Moss {
            return crate::moss::contact(self.terrain, self.geometry, tissue.cell);
        }
        material_contact(self.terrain, self.geometry, self.dirt, tissue.cell)
    }
}
impl Environment for MaterialUptake<'_> {
    fn exposure(&self, tissue: &Tissue) -> Exposure {
        if !(1..=1000).contains(&self.tick_ms)
            || self
                .rates
                .into_iter()
                .chain(self.availability)
                .any(|x| x > 1000)
        {
            return Exposure {
                nutrients: 1001,
                ..Exposure::default()
            };
        }
        let fractions = supply_fractions(self.rates, self.availability, self.tick_ms);
        let eligible = self.contact(tissue) == ContactReason::Eligible;
        Exposure {
            dirt_contact: eligible && tissue.role == Role::Root,
            surface_contact: eligible && tissue.role == Role::Moss,
            nutrients: fractions[0],
            water: fractions[1],
            light: 0,
        }
    }
    fn support_valid(&self, support: Option<Support>, cell: [i32; 2]) -> bool {
        support == Some(self.support)
            && self.geometry.is_current(self.terrain)
            && self
                .terrain
                .cell(CellCoord::new(cell[0], cell[1]))
                .is_some()
    }
    fn space_available(&self, cell: [i32; 2], role: Role, size: u16) -> bool {
        if size == 0 || !self.support_valid(Some(self.support), cell) {
            return false;
        }
        let c = CellCoord::new(cell[0], cell[1]);
        let material = self.terrain.cell(c).unwrap().material;
        let solid = self
            .geometry
            .source_cell(self.terrain, self.terrain.cell_center(c))
            .is_some();
        match role {
            Role::Root | Role::Runner => {
                solid && material == self.dirt && material != MaterialId::VOID
            }
            Role::Shoot => !solid,
            Role::Moss => crate::moss::site(self.terrain, self.geometry, cell),
        }
    }
}
