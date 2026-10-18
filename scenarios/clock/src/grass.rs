//! Bounded admission and retained material for the Clock's thin soil habitat.
//!
//! A plan is uncommitted work, not material or root contact. The habitat commits
//! actual dirt only after the host checks canonical occupancy at that boundary.
//! Retained cells occupy their original coordinates, so another introduction
//! fills holes rather than stacking another layer.

use engine_core::Vec2;

pub const INTRODUCTION_TICKS: u64 = 6 * 60;
pub const SEED_COOLDOWN_TICKS: u64 = 20 * 60 * 60;
pub const SEED_BATCH: usize = 3;
pub const SEED_SPACING_CELLS: i32 = 4;
pub const SEED_STARTING_RESERVES: [u64; 3] = [30; 3];

pub(crate) struct Introduction {
    pub tick: u64,
    seed: u64,
    committed: bool,
}
impl Introduction {
    pub fn new(seed: u64) -> Self {
        Self {
            tick: 0,
            seed,
            committed: false,
        }
    }
    pub fn take_seed(&mut self) -> Option<u64> {
        if self.committed {
            None
        } else {
            self.committed = true;
            Some(self.seed)
        }
    }
    pub fn step(&mut self) -> bool {
        self.tick += 1;
        self.tick >= INTRODUCTION_TICKS
    }
}

#[cfg(test)]
mod contracts;
mod habitat;
pub(crate) mod inspection;
mod residents;
pub(crate) mod surfaces;
pub use habitat::{
    CLOCK_DIRT, MAX_SOIL_BODIES, MAX_SOIL_CHUNK_COLLIDERS, MAX_SOIL_EXPORTS_PER_UPDATE,
    MAX_SOIL_EXTERNAL_INPUT, SOIL_PHYSICS_ID, SoilAccounting, SoilHabitat, SoilSupportState,
};
pub use residents::{
    BIOLOGY_TICK_MS, ClockPlants, MAX_BIOLOGY_STEPS_PER_UPDATE, MAX_PLANT_TISSUES, PlantPlayback,
};

/// World units; three rows leave finite underground runner/root space.
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
    /// Depth coordinate measured upward from the supporting floor.
    pub row: usize,
    pub min: Vec2,
    pub max: Vec2,
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
            let y = floor_y + row as f32 * SOIL_CELL_SIZE;
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
            assert!(site.min.y >= layout.floor_y);
            assert!(site.max.y <= layout.floor_y + 6.0);
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
