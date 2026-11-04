//! Bounded admission and retained material for the Clock's thin soil habitat.
//!
//! A plan is uncommitted work, not material or root contact. The habitat commits
//! actual dirt only after the host checks canonical occupancy at that boundary.
//! Retained cells occupy their original coordinates, so another introduction
//! fills holes rather than stacking another layer.
//!
//! The layer is inset into the top of the supporting floor rather than stacked
//! on it, so the walking surface stays at the floor line for every actor and
//! event. Roots occupy the rows below that line and shoots grow above it.

use engine_core::Vec2;

pub const INTRODUCTION_TICKS: u64 = 6 * 60;
pub const SEED_COOLDOWN_TICKS: u64 = 20 * 60 * 60;
pub const SEED_BATCH: usize = 3;
pub const SEED_SPACING_CELLS: i32 = 4;
pub const SEED_STARTING_RESERVES: [u64; 3] = [30; 3];

/// Moss succession. One bounded mat per opening is admitted when the ceiling
/// finishes closing, so a wider break leaves more cover behind as the lit grass
/// withers.
pub const MAX_MOSS_PATCHES: usize = crate::ceiling::MAX_OPENINGS;
/// Mat cells across every patch together, the way grass's trait bounds all of
/// its seeds at once: the moss policy recognises its own tissue by policy and
/// support, so each mat counts the others. Six cells a patch at the widest
/// break. The total stays small because every mat cell is also a light
/// receiver, and shoots and mats together have to fit `MAX_RECEIVERS`.
pub const MAX_MOSS_TISSUES: usize = 18;
/// A mat waits longer between growth opportunities than grass does. The dim
/// room saturates moss, but it pays only two energy a tick against upkeep, so
/// a cell takes many ticks to fund the twenty a neighbour costs.
pub const MOSS_GROWTH_INTERVAL_TICKS: u16 = 4;
pub const MOSS_STARTING_RESERVES: [u64; 3] = [24; 3];
/// Moss lies in the first air row above the floor line: its own cell has to be
/// outside solid geometry, with the soil surface row beneath it as its contact.
/// That is also the row a grass shoot rises into, so the two organisms compete
/// by ordinary occupancy rather than through any moss-specific exemption.
pub const MOSS_ROW: i32 = SOIL_DEPTH_CELLS as i32;

#[derive(Default)]
pub(crate) struct Introduction {
    pub tick: u64,
}
impl Introduction {
    pub fn step(&mut self) -> bool {
        self.tick += 1;
        self.tick >= INTRODUCTION_TICKS
    }
}

#[cfg(test)]
mod contracts;
mod habitat;
pub(crate) mod inspection;
pub(crate) mod residents;
#[cfg(test)]
mod succession_contracts;
pub(crate) mod surfaces;
pub use habitat::{
    CLOCK_DIRT, MAX_SOIL_BODIES, MAX_SOIL_CHUNK_COLLIDERS, MAX_SOIL_EXPORTS_PER_UPDATE,
    MAX_SOIL_EXTERNAL_INPUT, SOIL_PHYSICS_ID, SoilAccounting, SoilHabitat, SoilSupportState,
};
pub use residents::{
    BIOLOGY_TICK_MS, ClockPlants, MAX_BIOLOGY_STEPS_PER_UPDATE, MAX_GRASS_TISSUES,
    MAX_PLANT_TISSUES, PlantPlayback,
};

/// World units; three rows leave finite underground runner/root space, cut into
/// the supporting floor so the surface row's top rests on the floor line.
pub const SOIL_CELL_SIZE: f32 = 2.0;
pub const SOIL_DEPTH_CELLS: usize = 3;
/// Covers the widest supported Clock layout (1920 units) at the target depth.
pub const MAX_SOIL_CELLS: usize = 2880;
pub const MAX_SOIL_ADMISSIONS: usize = 2880;
pub const MAX_SOIL_SITE_QUERIES: usize = 2880;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoilSite {
    /// Stable horizontal grid coordinate relative to the arena's left edge.
    pub column: usize,
    /// Depth coordinate measured upward from the layer's deepest row. The
    /// surface row is `SOIL_DEPTH_CELLS - 1`; its top is the floor line.
    pub row: usize,
    pub min: Vec2,
    pub max: Vec2,
}

impl SoilSite {
    /// An inset cell at or below the floor line is inside solid support
    /// geometry, which every floor type supplies as a thick slab, and
    /// `plan_soil_admission` already refuses a column without full support.
    /// No canonical occupancy query can observe an actor inside it.
    pub fn inside_support(self, floor_y: f32) -> bool {
        self.max.y <= floor_y + 1e-3
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoilOccupancy {
    Empty,
    /// Actual retained dirt, not ordinary floor or a nearby material.
    RetainedDirt,
    /// Includes residents, protected space and non-dirt material.
    Protected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoilRefusal {
    InvalidGeometry,
    NoAdmissibleSpace,
    Capacity,
    AwaitingSupportAdapter,
}

#[derive(Debug, Default)]
pub struct SoilAdmissionPlan {
    pub sites: Vec<SoilSite>,
    pub retained: usize,
    pub queries: usize,
    pub refusal: Option<SoilRefusal>,
}

/// Plan only over supplied *actual* horizontal support intervals. Event-owned
/// floors must supply their geometry explicitly; an event name is not support.
/// The callback observes the whole candidate cell, including collision space.
/// `owned_cells` includes retained material outside these intervals as well.
pub fn plan_soil_admission(
    left: f32,
    right: f32,
    floor_y: f32,
    supports: &[(f32, f32)],
    owned_cells: usize,
    mut occupancy: impl FnMut(SoilSite) -> SoilOccupancy,
) -> SoilAdmissionPlan {
    let mut plan = SoilAdmissionPlan::default();
    if !left.is_finite()
        || !right.is_finite()
        || !floor_y.is_finite()
        || right <= left
        || right - left > MAX_SOIL_CELLS as f32 * SOIL_CELL_SIZE / SOIL_DEPTH_CELLS as f32
        || supports.len() > MAX_SOIL_SITE_QUERIES
        || supports
            .iter()
            .any(|&(a, b)| !a.is_finite() || !b.is_finite() || a > b)
    {
        plan.refusal = Some(SoilRefusal::InvalidGeometry);
        return plan;
    }
    let capacity = MAX_SOIL_CELLS.saturating_sub(owned_cells);
    let columns = ((right - left) / SOIL_CELL_SIZE).floor() as usize;
    for column in 0..columns {
        let x = left + column as f32 * SOIL_CELL_SIZE;
        // Require the entire cell to have support: no bridge across a drain.
        if !supports
            .iter()
            .any(|&(a, b)| a <= x && x + SOIL_CELL_SIZE <= b)
        {
            continue;
        }
        for row in 0..SOIL_DEPTH_CELLS {
            if plan.queries == MAX_SOIL_SITE_QUERIES {
                break;
            }
            // Inset: the surface row's top is the floor line, not its bottom.
            let y = floor_y - (SOIL_DEPTH_CELLS - row) as f32 * SOIL_CELL_SIZE;
            let site = SoilSite {
                column,
                row,
                min: Vec2::new(x, y),
                max: Vec2::new(x + SOIL_CELL_SIZE, y + SOIL_CELL_SIZE),
            };
            plan.queries += 1;
            match occupancy(site) {
                SoilOccupancy::RetainedDirt => plan.retained += 1,
                SoilOccupancy::Protected => break,
                SoilOccupancy::Empty => {
                    if plan.sites.len() == capacity.min(MAX_SOIL_ADMISSIONS) {
                        // Without a committed lower cell, do not admit upper rows.
                        break;
                    }
                    plan.sites.push(site);
                }
            }
        }
    }
    if plan.sites.is_empty() {
        plan.refusal = Some(if capacity == 0 {
            SoilRefusal::Capacity
        } else {
            SoilRefusal::NoAdmissibleSpace
        });
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{floor, layout::Layout};

    #[test]
    fn clock_grass_contract_soil_plan_follows_actual_drain_banks() {
        let layout = Layout::new(800.0 / 480.0);
        let supports: Vec<_> = floor::test_drain(layout)
            .slabs()
            .map(|(min, max)| (min.x, max.x))
            .collect();
        let plan = plan_soil_admission(
            layout.bounds_min.x,
            layout.bounds_max.x,
            layout.floor_y,
            &supports,
            0,
            |_| SoilOccupancy::Empty,
        );
        assert!(!plan.sites.is_empty());
        assert_eq!(plan.refusal, None);
        for site in plan.sites {
            assert!(
                supports
                    .iter()
                    .any(|&(a, b)| a <= site.min.x && site.max.x <= b)
            );
            // Inset: the layer fills the floor's top rows, not the space above.
            assert!(site.min.y >= layout.floor_y - 6.0);
            assert!(site.max.y <= layout.floor_y);
        }
    }

    #[test]
    fn clock_grass_contract_soil_plan_reuses_cells_and_protects_residents() {
        let first = plan_soil_admission(0.0, 8.0, 0.0, &[(0.0, 8.0)], 0, |s| {
            if s.column == 1 {
                SoilOccupancy::Protected
            } else {
                SoilOccupancy::Empty
            }
        });
        assert_eq!(first.sites.len(), 9);
        let second = plan_soil_admission(0.0, 8.0, 0.0, &[(0.0, 8.0)], 9, |s| {
            if s.column == 1 {
                SoilOccupancy::Protected
            } else {
                SoilOccupancy::RetainedDirt
            }
        });
        assert!(second.sites.is_empty());
        assert_eq!(second.retained, 9);
        assert_eq!(second.refusal, Some(SoilRefusal::NoAdmissibleSpace));
        let full = plan_soil_admission(0.0, 8.0, 0.0, &[(0.0, 8.0)], MAX_SOIL_CELLS, |_| {
            SoilOccupancy::Empty
        });
        assert_eq!(full.refusal, Some(SoilRefusal::Capacity));
        assert!(full.sites.is_empty());
    }

    #[test]
    fn clock_grass_contract_soil_plan_bounds_work_and_rejects_bad_geometry() {
        let plan = plan_soil_admission(-960.0, 960.0, 0.0, &[(-960.0, 960.0)], 0, |_| {
            SoilOccupancy::Empty
        });
        assert_eq!(plan.sites.len(), MAX_SOIL_CELLS);
        assert_eq!(plan.queries, MAX_SOIL_SITE_QUERIES);
        for (left, right) in [
            (f32::NAN, 1.0),
            (0.0, f32::INFINITY),
            (2.0, 1.0),
            (0.0, 1922.0),
        ] {
            let invalid = plan_soil_admission(left, right, 0.0, &[], 0, |_| {
                panic!("invalid geometry queried")
            });
            assert_eq!(invalid.refusal, Some(SoilRefusal::InvalidGeometry));
        }
        let missing =
            plan_soil_admission(0.0, 8.0, 0.0, &[], 0, |_| panic!("missing support queried"));
        assert_eq!(missing.queries, 0);
        assert_eq!(missing.refusal, Some(SoilRefusal::NoAdmissibleSpace));
    }
}
