//! Ballistic cells become water on contact with the actual panel top, not at
//! an existing water surface or an imaginary plane across the drain opening.
use super::*;

const SPLASH_DROPS: usize = 3;
const SPLASH_FRACTION: f64 = 0.3;
// Optional spray must leave capacity for ongoing pool outflow. Under pressure,
// more of the source goes straight into the pool rather than stalling impact.
// Moving lips release real uncovered strips as well as ordinary outfalls.
// Only the first 32 live parcels may include newly created optional spray.
const DRAIN_PARCEL_RESERVE: usize = MAX_SPILL_PARCELS - 32;

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

/// A contacted rigid block becomes a small liquid footprint, not a teleport to
/// a guessed pool. The ordinary swept water catches decide which ledge/panel
/// receives each part, including lower slabs and real gaps. Three area samples
/// for a digit square, one for a tiny AM/PM pixel; no secondary splash budget.
/// Capacity is checked for the WHOLE source before changing either material.
pub(super) fn liquefy(cell: &MeltCell, water: &mut WaterWorld, area: f64, layout: Layout) -> bool {
    let samples = if cell.meridiem { 1 } else { 3 };
    if water.parcels().len() + samples > MAX_SPILL_PARCELS {
        return false;
    }
    let extent = cell.extent(layout.pitch);
    let mut velocity = cell.velocity;
    if velocity.length() > 800.0 {
        velocity *= 800.0 / velocity.length();
    }
    for i in 0..samples {
        let offset = ((i as f32 + 0.5) / samples as f32 - 0.5) * extent.x * 1.5;
        let x = (cell.position.x + offset).clamp(layout.bounds_min.x, layout.bounds_max.x);
        water
            .add_falling(Parcel {
                position: Vec2::new(x, cell.position.y),
                velocity,
                volume: area / samples as f64,
                duration: 1.0 / 60.0,
                horizontal_bounds: Some([
                    f64::from(layout.bounds_min.x),
                    f64::from(layout.bounds_max.x),
                ]),
            })
            .expect("preflighted liquid footprint");
    }
    true
}

/// Vertical support height of a rotated square over the two finite panel tops.
/// Clip each edge to each panel's X interval, then evaluate its endpoints: the
/// separation of two straight edges is linear. Unlike an AABB test this cannot
/// hit empty space beside a rotated corner at the lip. Four edges, two panels,
/// no allocation or physics bodies; falling through the gap has no support.
fn contact_height(cell: &MeltCell, layout: Layout, floor: &ResponsiveFloor) -> Option<f64> {
    let outline = cell.outline(layout.pitch);
    let shape = floor.shape;
    let gap = floor.opening * shape.max_gap;
    let mut height: Option<f64> = None;
    for [left, right] in [[-shape.half_width, -gap], [gap, shape.half_width]] {
        for i in 0..4 {
            let a = outline[i];
            let b = outline[(i + 1) % 4];
            let x = f64::from(cell.position.x) + f64::from(a.x);
            let dx = f64::from(b.x) - f64::from(a.x);
            let (lo, hi) = if dx.abs() < 1e-12 {
                if x < left || x > right {
                    continue;
                }
                (0.0, 1.0)
            } else {
                let t0 = (left - x) / dx;
                let t1 = (right - x) / dx;
                (t0.min(t1).max(0.0), t0.max(t1).min(1.0))
            };
            if lo > hi {
                continue;
            }
            for t in [lo, hi] {
                let y = f64::from(a.y) + t * f64::from(b.y - a.y);
                let candidate = shape.surface_y(x + t * dx, floor.opening) - y;
                height = Some(height.map_or(candidate, |h| h.max(candidate)));
            }
        }
    }
    height
}

pub(super) fn merge(
    cell: &mut MeltCell,
    water: &mut WaterWorld,
    area: f64,
    layout: Layout,
    floor: &ResponsiveFloor,
) -> bool {
    let Some(contact_y) = contact_height(cell, layout, floor) else {
        return false;
    };
    if f64::from(cell.position.y) > contact_y + 1e-5 {
        return false;
    }
    // Resolve downward crossing at the current top rather than injecting water
    // from beneath it. Slow actuator motion is already limited by the engine.
    cell.position.y = contact_y as f32;
    let extent = cell.extent(layout.pitch);
    let left = (cell.position.x - extent.x).max(layout.bounds_min.x) as f64;
    let right = (cell.position.x + extent.x).min(layout.bounds_max.x) as f64;
    let lip = floor.opening * floor.shape.max_gap;
    let lip_y = floor.shape.floor_y - floor.opening * floor.shape.max_drop;
    let gap_left = left.max(-lip);
    let gap_right = right.min(lip);
    let gap = (gap_right - gap_left).max(0.0);
    if gap > 0.0 && water.parcels().len() == MAX_SPILL_PARCELS {
        // Do not partially convert a source if its gap portion cannot fit.
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
                position: Vec2::new(((gap_left + gap_right) * 0.5) as f32, lip_y as f32),
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
        let x = left + (right - left) * t as f64;
        // Gap portions originate at the inner edge's elevation; bank spray
        // follows the inclined top so it cannot start below the moving bed.
        let y = floor.shape.surface_y(x.abs().max(lip), floor.opening);
        water
            .add_falling(Parcel {
                position: Vec2::new(x as f32, y as f32 + 0.05),
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
