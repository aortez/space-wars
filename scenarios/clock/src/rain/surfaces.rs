//! Lit digit blocks are small collecting ledges, not solid/pressure colliders.
//! Colon and AM/PM stay presentation-only; seconds must not churn supports.
use crate::{
    DIGIT_SLOT_COUNT, DisplaySnapshot, SegmentId, SegmentKind, SegmentState, digits,
    floor::DrainGeometry,
};
use engine_water::{Boundary, DripConfig, PoolSpec, WaterConfig, WaterError, WaterWorld};

pub(super) const FLOOR_POOLS: usize = 2;
pub(super) const FLOOR_COLUMNS: usize = 128;
const CELL_POOLS: usize = 24 * DIGIT_SLOT_COUNT;
const POOLS: usize = FLOOR_POOLS + CELL_POOLS;
pub(super) const RELEASE_SLOTS: usize = CELL_POOLS * 2;
// Reuse the lab/engine ceiling, not a growing per-digit or per-event allocation.
pub(super) const PARCELS: usize = 512;

pub(super) struct DigitSurfaces {
    pub digits: [Option<u8>; DIGIT_SLOT_COUNT],
    pub pending: bool,
    pub deferrals: u64,
}

#[cfg(test)]
mod tests;

impl DigitSurfaces {
    pub fn new(drain: DrainGeometry, display: DisplaySnapshot) -> (Self, WaterWorld) {
        let layout = drain.layout();
        let half = layout.pitch * 0.4;
        let mut specs: Vec<_> = drain.water_pools(FLOOR_COLUMNS).into();
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
        assert_eq!(specs.len(), POOLS);
        let mut water = WaterWorld::new(
            WaterConfig {
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
        // Only the two floor outfalls enter the drain. Digit runoff stays at
        // its own x position; the old world-wide channel would teleport it.
        let lip = f64::from(drain.half_width());
        water.set_outlet_channel(0, 1, Some([-lip, lip])).unwrap();
        water.set_outlet_channel(1, 0, Some([-lip, lip])).unwrap();
        let unit = f64::from(layout.pitch * 0.8).powi(2);
        for pool in FLOOR_POOLS..POOLS {
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
        water.set_pool_supports(&Self::mask(display)).unwrap();
        (
            Self {
                digits: display.digits,
                pending: false,
                deferrals: 0,
            },
            water,
        )
    }

    fn mask(display: DisplaySnapshot) -> [bool; POOLS] {
        let mut mask = [true; POOLS];
        let mut pool = FLOOR_POOLS;
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
        match water.set_pool_supports(&Self::mask(display)) {
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
