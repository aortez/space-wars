//! Lit digit blocks are small collecting ledges, not solid/pressure colliders.
//! Colon and AM/PM stay presentation-only; seconds must not churn supports.
use crate::{
    DIGIT_SLOT_COUNT, DisplaySnapshot, SegmentId, SegmentKind, SegmentState, digits,
    floor::responsive::FloorShape, layout::Layout,
};
use engine_water::{Boundary, DripConfig, PoolSpec, WaterConfig, WaterError, WaterWorld};

#[cfg(test)]
pub(super) const FLOOR_POOLS: usize = 2;
const CELL_POOLS: usize = 24 * DIGIT_SLOT_COUNT;
pub(super) const RELEASE_SLOTS: usize = CELL_POOLS * 2;
// Reuse the lab/engine ceiling, not a growing per-digit or per-event allocation.
pub(super) const PARCELS: usize = 512;

pub(super) struct DigitSurfaces {
    pub floor_pools: usize,
    pub digits: [Option<u8>; DIGIT_SLOT_COUNT],
    pub pending: bool,
    pub deferrals: u64,
}

#[cfg(test)]
mod tests;

impl DigitSurfaces {
    pub fn new(layout: Layout, display: DisplaySnapshot) -> (Self, WaterWorld) {
        let floor = FloorShape::clock(layout);
        let (surfaces, mut water) = Self::with_floor(layout, display, floor.pools().into());
        floor.configure(&mut water);
        (surfaces, water)
    }

    pub fn on_responsive_floor(
        layout: Layout,
        display: DisplaySnapshot,
        floor: &crate::floor::responsive::ResponsiveFloor,
    ) -> (Self, WaterWorld) {
        let (surfaces, mut water) =
            Self::with_floor(layout, display, floor.shape.pools_at(floor.opening).into());
        floor.shape.configure_at(&mut water, floor.opening);
        (surfaces, water)
    }

    pub fn with_floor(
        layout: Layout,
        display: DisplaySnapshot,
        mut specs: Vec<PoolSpec>,
    ) -> (Self, WaterWorld) {
        let floor_pools = specs.len();
        let half = layout.pitch * 0.4;
        for slot in 0..DIGIT_SLOT_COUNT {
            for kind in SegmentKind::ALL {
                let id = SegmentId {
                    digit_slot: slot as u8,
                    kind,
                };
                for &cell in digits::cells(kind) {
                    let center = layout.cell_center(id, cell);
                    let top = f64::from(center.y + half);
                    specs.push(PoolSpec {
                        left: f64::from(center.x - half),
                        column_width: f64::from(half),
                        bed: vec![top; 2],
                        boundaries: [Boundary::Spill { lip: top }; 2],
                    });
                }
            }
        }
        assert_eq!(specs.len(), floor_pools + CELL_POOLS);
        let mut water = WaterWorld::new(
            WaterConfig {
                // A gentle local response when a drop reaches an already-wet
                // surface. Shared engine defaults and Meltdown stay unchanged.
                impact_response: 0.12,
                max_parcels: PARCELS,
                reserved_release_parcels: RELEASE_SLOTS,
                exit_y: f64::from(layout.bounds_min.y),
                spill_channel: Some([
                    f64::from(layout.bounds_min.x),
                    f64::from(layout.bounds_max.x),
                ]),
                ..WaterConfig::default()
            },
            specs,
        )
        .expect("bounded digit and floor geometry");
        let unit = f64::from(layout.pitch * 0.8).powi(2);
        for pool in floor_pools..floor_pools + CELL_POOLS {
            water
                .set_drip_config(
                    pool,
                    Some(DripConfig {
                        // Heavy live rain needs coarser batching than the
                        // low-throughput lab. A quarter-cell drop keeps a wet
                        // four-digit face within the fixed parcel budget. The
                        // longer wait also prevents light-rain residual films
                        // from flooding that same budget with tiny parcels.
                        target_volume: unit / 4.0,
                        max_delay: 1.2,
                    }),
                )
                .unwrap();
        }
        water
            .set_pool_supports(&Self::mask(display, floor_pools)[..floor_pools + CELL_POOLS])
            .unwrap();
        (
            Self {
                floor_pools,
                digits: display.digits,
                pending: false,
                deferrals: 0,
            },
            water,
        )
    }

    fn mask(display: DisplaySnapshot, floor_pools: usize) -> [bool; engine_water::MAX_POOLS] {
        let mut mask = [true; engine_water::MAX_POOLS];
        let mut pool = floor_pools;
        for digit in display.digits {
            for kind in SegmentKind::ALL {
                let lit = digit.is_some_and(|d| digits::digit_mask(d) & (1 << kind as u8) != 0);
                let end = pool + digits::cells(kind).len();
                mask[pool..end].fill(lit);
                pool = end;
            }
        }
        mask
    }

    pub fn synchronize(
        &mut self,
        water: &mut WaterWorld,
        display: DisplaySnapshot,
        segments: &mut [SegmentState],
    ) {
        if self.digits == display.digits {
            self.pending = false;
            return;
        }
        match water.set_pool_supports(
            &Self::mask(display, self.floor_pools)[..self.floor_pools + CELL_POOLS],
        ) {
            Ok(()) => {
                self.digits = display.digits;
                self.pending = false;
                digits::apply_snapshot(segments, display);
            }
            Err(WaterError::Capacity) => {
                // Keep visible and physical supports together, then retry the
                // latest reading on the next simulation tick. No lost water,
                // stale queued readings, or invisible catching ledges.
                self.pending = true;
                self.deferrals += 1;
            }
            Err(other) => panic!("fixed digit support mask: {other}"),
        }
    }
}
