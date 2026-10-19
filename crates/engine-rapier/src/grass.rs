//! Terrain eligibility for the shared grass brain, plus the shared moss surface
//! rule so mixed patches on one support validate together. No duplicate physiology.
use crate::root_uptake::RootUptake;
use engine_core::{
    Vec2,
    plants::{Environment, Exposure, Plants, Rejection, Role, Support, Tissue},
};
use engine_terrain::{CellCoord, MaterialId};

pub struct GrassEnvironment<'a, E> {
    pub roots: &'a RootUptake<'a>,
    pub physiology: E,
}
impl<E: Environment> Environment for GrassEnvironment<'_, E> {
    fn exposure(&self, tissue: &Tissue) -> Exposure {
        self.physiology.exposure(tissue)
    }
    fn support_valid(&self, support: Option<Support>, cell: [i32; 2]) -> bool {
        self.roots.support_valid(support, cell)
    }
    fn space_available(&self, cell: [i32; 2], role: Role, size: u16) -> bool {
        if size == 0 {
            return false;
        }
        let coord = CellCoord::new(cell[0], cell[1]);
        // Keep all tissue inside the finite host terrain domain.
        let Some(material) = self.roots.terrain.cell(coord).map(|c| c.material) else {
            return false;
        };
        let solid = self
            .roots
            .geometry
            .source_cell(self.roots.terrain, self.roots.terrain.cell_center(coord))
            .is_some();
        match role {
            Role::Root | Role::Runner => {
                solid && material == self.roots.dirt && material != MaterialId::VOID
            }
            Role::Shoot => !solid,
            Role::Moss => crate::moss::site(self.roots.terrain, self.roots.geometry, cell),
        }
    }
}
/// Quantize biological up after transforming actual world gravity into support space.
pub fn local_up(roots: &RootUptake<'_>, gravity: Vec2) -> Result<[i32; 2], Rejection> {
    if !gravity.x.is_finite() || !gravity.y.is_finite() || gravity.length_squared() == 0.0 {
        return Err(Rejection::Invalid);
    }
    let motion = roots
        .world
        .motion(roots.assembly.body())
        .ok_or(Rejection::Support)?;
    let up = (-gravity).rotate_radians(-motion.angle);
    Ok(if up.x.abs() > up.y.abs() {
        [if up.x > 0.0 { 1 } else { -1 }, 0]
    } else {
        [0, if up.y > 0.0 { 1 } else { -1 }]
    })
}
/// Apply attachment loss between ticks. Removed cells lose their existing share;
/// the caller may then publish the remaining complete tissue set once.
pub fn detach_invalid<E: Environment>(plants: &mut Plants, env: &E) -> Result<usize, Rejection> {
    let invalid: Vec<_> = plants
        .observe()
        .tissues
        .iter()
        .filter(|t| {
            !env.support_valid(t.support, t.cell) || !env.space_available(t.cell, t.role, t.size)
        })
        .map(|t| t.id)
        .collect();
    for id in &invalid {
        plants.remove(*id)?;
    }
    Ok(invalid.len())
}

#[cfg(test)]
mod grass_growth_contract;
