//! A tree's senses on a terrain support, for `engine_core::plants::tree`:
//! light at the empty cells the tree asked to probe, from the support's light
//! publication, and the supply a root would take at a dirt cell, from the
//! support's root uptake. Placement and physiology use `GrassEnvironment`, so
//! the tree lives by the rules grass does.
use crate::{plant_light::PlantLight, root_uptake::RootUptake};
use engine_core::plants::tree::Senses;

pub struct TreeSenses<'a, 'b, 'w> {
    /// This tick's publication, made with the tree's probes.
    pub light: &'a PlantLight<'b, 'w>,
    pub roots: &'a RootUptake<'w>,
}
impl Senses for TreeSenses<'_, '_, '_> {
    fn light(&self, cell: [i32; 2]) -> Option<u16> {
        let exposure = self.light.probe(cell, self.light.tick()).ok()?;
        Some(exposure.intensity.clamp(0.0, 1000.0).floor() as u16)
    }
    fn soil(&self, cell: [i32; 2]) -> Option<[u16; 2]> {
        self.roots.soil(cell)
    }
}

#[cfg(test)]
mod tree_growth_contract;
