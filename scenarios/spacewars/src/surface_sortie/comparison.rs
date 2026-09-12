//! Matched single-planet scenes preserve all three surfaces for comparisons.
use super::*;
pub use engine_terrain::TerrainSurface;
use engine_terrain::{Brush, CellCoord, EditMode, TerrainEdit};

impl SurfaceSortieScenario {
    pub fn init_surface_comparison(
        seed: u64,
        players: usize,
        surface: TerrainSurface,
    ) -> SurfaceSortieState {
        let mut state = Self::init_material_surface(seed, players, surface);
        state.surface_comparison = Some(surface);
        let planet = &mut state.world.planets[0];
        planet.wrapper_omega = 0.0;
        // The same initial ships now land on diagonal material in both scenes.
        planet.wrapper_angle = std::f32::consts::FRAC_PI_4;
        state.world.physics.world.set_pose(
            physics::primary_body(physics::planet_entity(0)),
            planet.position,
            planet.wrapper_angle,
            true,
        );
        state.world.physics.material_queries_dirty = true;
        // Put excavations away from the untouched initial landing. The left cap
        // remains joined by one cell at local (-45, 0); mining it frees the cap.
        let cell = |x, y| CellCoord::new(60 + x, 60 + y);
        for brush in [
            Brush::Circle {
                center: cell(0, -58),
                radius: 5,
            },
            Brush::Capsule {
                start: cell(-18, -44),
                end: cell(18, -44),
                radius: 2,
            },
            Brush::Capsule {
                start: cell(-45, -60),
                end: cell(-45, -1),
                radius: 0,
            },
            Brush::Capsule {
                start: cell(-45, 1),
                end: cell(-45, 60),
                radius: 0,
            },
        ] {
            state
                .world
                .queue_planet_edit(
                    0,
                    TerrainEdit {
                        brush,
                        mode: EditMode::Remove,
                    },
                )
                .unwrap();
        }
        // Commit through the ordinary lifecycle before any actor is advanced.
        terrain::commit(&mut state.world);
        for pilot in &mut state.pilots {
            pilot.jetpack_charge = Some(1.0);
        }
        state
    }
}

impl SurfaceSortieState {
    pub fn surface_comparison(&self) -> Option<TerrainSurface> {
        self.surface_comparison
    }
}

#[cfg(test)]
mod tests;
