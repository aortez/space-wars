//! Solid-looking ballistic cells become water at the floor/drain elevation,
//! not at an existing water surface. Splash and pool volumes share one source.
use super::*;

const SPLASH_DROPS: usize = 3;
const SPLASH_FRACTION: f64 = 0.3;
// Optional spray must leave capacity for ongoing pool outflow. Under pressure,
// more of the source goes straight into the pool rather than stalling impact.
const DRAIN_PARCEL_RESERVE: usize = 64;

pub(crate) fn soften(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MeltCell {
    pub position: Vec2,
    /// AM/PM pixels retain their smaller geometry and corresponding water volume.
    pub meridiem: bool,
    pub(super) velocity: Vec2,
    pub angle: f32,
    pub(super) spin: f32,
    pub(super) release_tick: u64,
}

impl MeltCell {
    pub(super) fn area_scale(&self) -> f64 {
        if self.meridiem {
            (0.16_f64 * 0.9 / 0.8).powi(2)
        } else {
            1.0
        }
    }

    pub(crate) fn outline(&self, pitch: f32) -> [Vec2; 4] {
        let half = if self.meridiem {
            pitch * crate::meridiem::PIXEL_SIZE * 0.5
        } else {
            pitch * 0.4
        };
        let (sin, cos) = self.angle.sin_cos();
        [
            Vec2::new(-half, -half),
            Vec2::new(half, -half),
            Vec2::new(half, half),
            Vec2::new(-half, half),
        ]
        .map(|p| Vec2::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos))
    }

    pub(super) fn extent(&self, pitch: f32) -> Vec2 {
        self.outline(pitch).iter().fold(Vec2::ZERO, |extent, p| {
            Vec2::new(extent.x.max(p.x.abs()), extent.y.max(p.y.abs()))
        })
    }
}

pub(super) fn merge(
    cell: &mut MeltCell,
    extent: Vec2,
    water: &mut WaterWorld,
    area: f64,
    drain: DrainGeometry,
) -> bool {
    let layout = drain.layout();
    let left = (cell.position.x - extent.x).max(layout.bounds_min.x) as f64;
    let right = (cell.position.x + extent.x).min(layout.bounds_max.x) as f64;
    if cell.position.y - extent.y > layout.floor_y {
        return false;
    }
    let lip = drain.half_width() as f64;
    let gap_left = left.max(-lip);
    let gap_right = right.min(lip);
    let gap = (gap_right - gap_left).max(0.0);
    if gap > 0.0 && water.parcels().len() == MAX_SPILL_PARCELS {
        // Do not partially convert a source if its gap portion cannot fit.
        cell.position.y = layout.floor_y + extent.y;
        cell.velocity.y = 0.0;
        return false;
    }
    let splash_count = MAX_SPILL_PARCELS
        .saturating_sub(water.parcels().len() + usize::from(gap > 0.0) + DRAIN_PARCEL_RESERVE)
        .min(SPLASH_DROPS);
    let splash_area = area * SPLASH_FRACTION * splash_count as f64 / SPLASH_DROPS as f64;
    // Partition the remainder by footprint, preserving bank/gap shares even at
    // the lip. The splash is real in-flight water, never credited twice.
    let density = (area - splash_area) / (right - left);
    for pool in 0..water.pools().len() {
        let spec = water.pools()[pool].spec();
        let (start, dx, count) = (spec.left, spec.column_width, spec.bed.len());
        let first = ((left - start) / dx).floor().clamp(0.0, count as f64) as usize;
        let end = ((right - start) / dx).ceil().clamp(0.0, count as f64) as usize;
        for i in first..end {
            let x = start + i as f64 * dx;
            let width = (right.min(x + dx) - left.max(x)).max(0.0);
            if width > 0.0 {
                water
                    .add_to_pool(pool, x + dx * 0.5, width * density)
                    .expect("bounded cell footprint inside bank");
            }
        }
    }
    if gap > 0.0 {
        water
            .add_falling(Parcel {
                position: Vec2::new(((gap_left + gap_right) * 0.5) as f32, layout.floor_y),
                velocity: Vec2::new(cell.velocity.x, cell.velocity.y.max(-800.0)),
                volume: gap * density,
                duration: 1.0 / 60.0,
                horizontal_bounds: Some([-lip, lip]),
            })
            .expect("reserved source spill capacity");
    }
    for i in 0..splash_count {
        let t = (i as f32 + 0.5) / splash_count as f32;
        let speed = (-cell.velocity.y * 0.35).clamp(90.0, 160.0);
        water
            .add_falling(Parcel {
                position: Vec2::new(
                    (left + (right - left) * t as f64) as f32,
                    layout.floor_y + 0.05,
                ),
                velocity: Vec2::new(
                    (t - 0.5) * speed * 0.85,
                    speed * (1.0 - (t - 0.5).abs() * 0.25),
                ),
                volume: splash_area / splash_count as f64,
                duration: 1.0 / 60.0,
                horizontal_bounds: Some([layout.bounds_min.x as f64, layout.bounds_max.x as f64]),
            })
            .expect("reserved bounded impact splash");
    }
    true
}
