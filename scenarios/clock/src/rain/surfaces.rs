//! Lit digit blocks are small collecting ledges, not solid/pressure colliders.
//! Colon and AM/PM stay presentation-only; seconds must not churn supports.
use crate::{
    DIGIT_SLOT_COUNT, DisplaySnapshot, SegmentId, SegmentKind, SegmentState, digits,
    floor::responsive::FloorShape, layout::Layout,
};
use engine_water::{
    Boundary, DripConfig, PoolSpec, SplashConfig, WaterConfig, WaterError, WaterWorld,
};

#[cfg(test)]
pub(super) const FLOOR_POOLS: usize = 2;
#[cfg(test)]
pub(super) const RELEASE_SLOTS: usize = 24 * DIGIT_SLOT_COUNT * 2;
// Reuse the lab/engine ceiling, not a growing per-digit or per-event allocation.
pub(super) const PARCELS: usize = 512;

pub(super) struct DigitSurfaces {
    pub floor_pools: usize,
    pub cell_pools: usize,
    pub release_slots: usize,
    font: engine_common::ClockFont,
    pub digits: [Option<u8>; DIGIT_SLOT_COUNT],
    pub pending: bool,
    pub deferrals: u64,
}

#[cfg(test)]
mod tests;

impl DigitSurfaces {
    pub fn new(layout: Layout, display: DisplaySnapshot, seed: u64) -> (Self, WaterWorld) {
        let floor = FloorShape::clock(layout);
        let (surfaces, mut water) = Self::with_floor(
            layout,
            display,
            floor.pools().into(),
            Some(crate::floor::responsive::drain_splash(seed)),
        );
        floor.configure(&mut water);
        (surfaces, water)
    }

    pub fn on_responsive_floor(
        layout: Layout,
        display: DisplaySnapshot,
        floor: &crate::floor::responsive::ResponsiveFloor,
        seed: u64,
    ) -> (Self, WaterWorld) {
        let (surfaces, mut water) = Self::with_floor(
            layout,
            display,
            floor.shape.pools_at(floor.opening).into(),
            Some(crate::floor::responsive::drain_splash(seed)),
        );
        floor.shape.configure_at(&mut water, floor.opening);
        (surfaces, water)
    }

    pub fn with_floor(
        layout: Layout,
        display: DisplaySnapshot,
        mut specs: Vec<PoolSpec>,
        splash: Option<SplashConfig>,
    ) -> (Self, WaterWorld) {
        let floor_pools = specs.len();
        let cell_pools = crate::fonts::guides(display.font).cells().count() * DIGIT_SLOT_COUNT;
        // Only lit cells hold water. Reserve two columns per cell in the
        // densest glyph, rather than every cell any digit could ever light.
        let release_slots = (0..10)
            .map(|digit| {
                crate::fonts::glyph(display.font, Some(digit))
                    .0
                    .count_ones() as usize
            })
            .max()
            .unwrap()
            * DIGIT_SLOT_COUNT
            * 2;
        let half = layout.pitch * 0.4;
        for slot in 0..DIGIT_SLOT_COUNT {
            for kind in SegmentKind::ALL {
                let id = SegmentId {
                    digit_slot: slot as u8,
                    kind,
                };
                for cell in crate::fonts::CellMask(
                    crate::fonts::guides(display.font).0
                        & crate::fonts::region(display.font, kind).0,
                )
                .cells()
                {
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
        assert_eq!(specs.len(), floor_pools + cell_pools);
        let mut water = WaterWorld::new(
            WaterConfig {
                splash,
                // A gentle local response when a drop reaches an already-wet
                // surface. Shared engine defaults and Meltdown stay unchanged.
                impact_response: 0.12,
                max_parcels: PARCELS,
                reserved_release_parcels: release_slots,
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
        let drip_scale = (release_slots as f64 / (24 * DIGIT_SLOT_COUNT * 2) as f64).max(1.0);
        for pool in floor_pools..floor_pools + cell_pools {
            water
                .set_drip_config(
                    pool,
                    Some(DripConfig {
                        // Heavy live rain needs coarser batching than the
                        // low-throughput lab. A quarter-cell drop keeps a wet
                        // four-digit face within the fixed parcel budget. The
                        // longer wait also prevents light-rain residual films
                        // from flooding that same budget with tiny parcels.
                        // Denser faces batch proportionally more runoff while
                        // retaining the same fixed 512-parcel ceiling.
                        target_volume: unit / 4.0 * drip_scale,
                        max_delay: 1.2 * drip_scale,
                    }),
                )
                .unwrap();
        }
        water
            .set_pool_supports(&Self::mask(display, floor_pools)[..floor_pools + cell_pools])
            .unwrap();
        (
            Self {
                floor_pools,
                cell_pools,
                release_slots,
                font: display.font,
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
                let glyph = crate::fonts::glyph(display.font, digit);
                let guides = crate::fonts::CellMask(
                    crate::fonts::guides(display.font).0
                        & crate::fonts::region(display.font, kind).0,
                );
                for cell in guides.cells() {
                    mask[pool] = glyph.contains(cell);
                    pool += 1;
                }
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
        debug_assert_eq!(self.font, display.font);
        if self.digits == display.digits {
            self.pending = false;
            return;
        }
        match water.set_pool_supports(
            &Self::mask(display, self.floor_pools)[..self.floor_pools + self.cell_pools],
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
