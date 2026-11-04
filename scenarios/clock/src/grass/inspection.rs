//! Read-only publication: no plant tick, resource credit or support mutation.
use super::*;
use engine_common::{ClockGrassInspection, ClockGrassTissue};
use engine_terrain::CellCoord;

pub(crate) fn snapshot(state: &crate::ClockState) -> Option<ClockGrassInspection> {
    if !state.config.grass_inspection {
        return None;
    }
    let o = state.plants.plants.observe();
    // Stable focus until the organism retires; never manufacture demo tissue.
    let organism = o.tissues.iter().find(|t| t.health > 0).map(|t| t.organism);
    let tissues: Vec<_> = o
        .tissues
        .iter()
        .filter(|t| Some(t.organism) == organism)
        .map(|t| ClockGrassTissue {
            id: t.id,
            cell: t.cell,
            role: match t.role {
                engine_core::plants::Role::Root => "root",
                engine_core::plants::Role::Runner => "runner",
                engine_core::plants::Role::Shoot => "shoot",
                engine_core::plants::Role::Moss => "moss",
            }
            .into(),
            size: t.size,
            health: t.health,
            development: t.development,
            invested: t.invested,
            support: t.support.map(|s| [s.body, s.generation]),
        })
        .collect();
    let connections = o
        .connections
        .iter()
        .filter(|(a, b)| tissues.iter().any(|t| t.id == *a) && tissues.iter().any(|t| t.id == *b))
        .map(|(a, b)| [*a, *b])
        .collect();
    let mut soil_cells = Vec::new();
    if let (Some(soil), Some(left), Some(right)) = (
        &state.soil,
        tissues.iter().map(|t| t.cell[0]).min(),
        tissues.iter().map(|t| t.cell[0]).max(),
    ) {
        for x in (left - 1).max(0)..=(right + 1).min(soil.terrain().width() as i32 - 1) {
            for y in 0..SOIL_DEPTH_CELLS as i32 {
                if soil
                    .terrain()
                    .cell(CellCoord::new(x, y))
                    .is_some_and(|c| c.material == CLOCK_DIRT)
                    && soil.world_point([x, y]).is_some()
                {
                    soil_cells.push([x, y]);
                }
            }
        }
    }
    debug_assert!(tissues.len() <= MAX_PLANT_TISSUES && soil_cells.len() <= MAX_SOIL_CELLS);
    Some(ClockGrassInspection {
        organism,
        tissues,
        connections,
        soil_cells,
        development_per_size: o.config.development_per_size,
    })
}
