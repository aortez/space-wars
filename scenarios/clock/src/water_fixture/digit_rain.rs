//! Isolated digit/row, not live Clock integration. Exact cell widths and gaps;
//! collecting ledges, not general solid colliders or a sealed digit silhouette.
use super::*;
use crate::{
    digits::{self, GridCell, SegmentKind},
    rain::source::RainSource,
};
use engine_core::Vec2;
use engine_water::{DripConfig, Parcel, WaterError};
use rand::{SeedableRng, rngs::StdRng};

const PITCH: f32 = 12.0;
const HALF: f32 = PITCH * 0.4;
const DT: f64 = 1.0 / 60.0;
const PARCELS: usize = 512;

pub struct DigitRainFixture {
    pub water: WaterWorld,
    cells: Vec<(SegmentKind, GridCell)>,
    digits: Vec<u8>,
    source: RainSource,
    tick: u64,
    scheduled: f64,
    source_limited: u64,
    was_raining: bool,
    half_width: f32,
}

fn center(cell: GridCell) -> Vec2 {
    Vec2::new(
        (f32::from(cell.x) - 2.5) * PITCH,
        (f32::from(cell.y) - 4.0) * PITCH,
    )
}

impl DigitRainFixture {
    pub fn new(digit: u8, seed: u64) -> Self {
        Self::with_drips(digit, seed, true)
    }

    /// A/B control: same seed, scheduled rain, geometry and budgets. Capacity
    /// backpressure can still change which source attempts are admitted.
    pub fn with_drips(digit: u8, seed: u64, drips: bool) -> Self {
        Self::build(&[digit], seed, drips)
    }

    /// Four digits at the SAME rain intensity per digit and the same hard
    /// 512-parcel budget. Not the live Clock's layout/colon/meridiem policy.
    pub fn row(digits: [u8; 4], seed: u64, drips: bool) -> Self {
        Self::build(&digits, seed, drips)
    }

    fn build(digits: &[u8], seed: u64, drips: bool) -> Self {
        let half_width = 72.0 + (digits.len() - 1) as f32 * 42.0;
        let floor_columns = ((half_width - 6.0) / 3.0) as usize;
        let cells: Vec<_> = SegmentKind::ALL
            .into_iter()
            .flat_map(|kind| digits::cells(kind).iter().map(move |&cell| (kind, cell)))
            .collect();
        let mut specs = vec![
            PoolSpec {
                left: -f64::from(half_width),
                column_width: 3.0,
                bed: vec![-84.0; floor_columns],
                boundaries: [Boundary::Closed, Boundary::Spill { lip: -84.0 }],
            },
            PoolSpec {
                left: 6.0,
                column_width: 3.0,
                bed: vec![-84.0; floor_columns],
                boundaries: [Boundary::Spill { lip: -84.0 }, Boundary::Closed],
            },
        ];
        for slot in 0..digits.len() {
            for &(_, cell) in &cells {
                let p = center(cell) + Vec2::new(Self::slot_x(slot, digits.len()), 0.0);
                let top = f64::from(p.y + HALF);
                specs.push(PoolSpec {
                    left: f64::from(p.x - HALF),
                    column_width: f64::from(HALF),
                    bed: vec![top; 2],
                    boundaries: [Boundary::Spill { lip: top }; 2],
                });
            }
        }
        let cell_count = cells.len() * digits.len();
        let mut water = WaterWorld::new(
            WaterConfig {
                exit_y: -130.0,
                max_parcels: PARCELS,
                reserved_release_parcels: cell_count * 2,
                ..WaterConfig::default()
            },
            specs,
        )
        .unwrap();
        if drips {
            for pool in 2..cell_count + 2 {
                water
                    .set_drip_config(
                        pool,
                        Some(DripConfig {
                            target_volume: 3.0,
                            max_delay: 0.6,
                        }),
                    )
                    .unwrap();
            }
        }
        let mut fixture = Self {
            water,
            cells,
            digits: digits.to_vec(),
            source: RainSource::new(StdRng::seed_from_u64(seed)),
            tick: 0,
            scheduled: 0.0,
            source_limited: 0,
            was_raining: false,
            half_width,
        };
        fixture.set_digits(digits).unwrap();
        fixture
    }

    fn slot_x(slot: usize, slots: usize) -> f32 {
        (slot as f32 - (slots - 1) as f32 * 0.5) * 84.0
    }

    pub fn digit(&self) -> u8 {
        self.digits[0]
    }
    pub fn digits(&self) -> &[u8] {
        &self.digits
    }
    pub fn source_limited_ticks(&self) -> u64 {
        self.source_limited
    }
    pub fn scheduled_volume(&self) -> f64 {
        self.scheduled
    }

    /// Keep old visual and physical digit if the bounded release cannot fit.
    pub fn set_digit(&mut self, digit: u8) -> Result<(), WaterError> {
        self.set_digits(&[digit])
    }

    pub fn set_digits(&mut self, digits: &[u8]) -> Result<(), WaterError> {
        if digits.len() != self.digits.len() || digits.iter().any(|d| *d > 9) {
            return Err(WaterError::InvalidInput);
        }
        let mut enabled = [true; engine_water::MAX_POOLS];
        for (slot, &digit) in digits.iter().enumerate() {
            for (i, (kind, _)) in self.cells.iter().enumerate() {
                enabled[slot * self.cells.len() + i + 2] =
                    digits::digit_mask(digit) & (1 << *kind as u8) != 0;
            }
        }
        self.water
            .set_pool_supports(&enabled[..self.cells.len() * digits.len() + 2])?;
        self.digits.copy_from_slice(digits);
        Ok(())
    }

    pub fn step(&mut self, raining: bool) {
        self.tick += 1;
        if raining {
            self.scheduled += 2.0 * self.digits.len() as f64;
        }
        // Attempt the final scheduled remainder when the source is stopped,
        // even when that falls between randomized batches (as in live Rain).
        let count = if raining {
            self.source.emission_count(self.tick) * self.digits.len()
        } else {
            usize::from(self.was_raining)
        };
        self.was_raining = raining;
        let pending = (self.scheduled - self.water.stats().injected).max(0.0);
        if count > 0 && pending > 1e-9 {
            // Leave headroom for support retirement and both floor outlets.
            if self.water.parcels().len() + count
                <= PARCELS - self.cells.len() * self.digits.len() * 2 - 2
            {
                let volume = pending / count as f64;
                for _ in 0..count {
                    self.water
                        .add_falling(Parcel {
                            position: Vec2::new(
                                (self.source.next_fraction() * 2.0 - 1.0)
                                    * (self.half_width - 24.0),
                                82.0,
                            ),
                            velocity: Vec2::new(0.0, -70.0),
                            volume,
                            duration: 0.04,
                            horizontal_bounds: Some([
                                -f64::from(self.half_width),
                                f64::from(self.half_width),
                            ]),
                        })
                        .unwrap();
                }
            } else {
                self.source_limited += 1;
            }
        }
        self.water.step(DT).unwrap();
    }

    pub fn frame(&self) -> RenderFrame {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::new(0.0, -15.0), 210.0));
        for (pool_index, pool) in self.water.pools().iter().take(2).enumerate() {
            let spec = pool.spec();
            let right = spec.left + spec.column_width * spec.bed.len() as f64;
            frame.push_primitive(
                0,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    vec![
                        RenderPoint::new(spec.left as f32, -120.0),
                        RenderPoint::new(right as f32, -120.0),
                        RenderPoint::new(right as f32, -84.0),
                        RenderPoint::new(spec.left as f32, -84.0),
                    ],
                    RenderColor::rgb(0.075, 0.105, 0.145),
                )),
            );
            debug_assert!(self.water.pools()[pool_index].enabled());
        }
        for slot in 0..self.digits.len() {
            for (i, &(_, cell)) in self.cells.iter().enumerate() {
                crate::render::water_fixture_cell(
                    &mut frame,
                    center(cell) + Vec2::new(Self::slot_x(slot, self.digits.len()), 0.0),
                    PITCH,
                    self.water.pools()[slot * self.cells.len() + i + 2].enabled(),
                );
            }
        }
        crate::render::water_fixture(&mut frame, &self.water);
        frame
    }
}

#[cfg(test)]
mod tests;
