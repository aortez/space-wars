//! Read-only water sensing and two clear approaches beside the digit face.
use super::WaterTolerance;
use crate::{floor::responsive::ResponsiveFloor, layout::Layout};
use engine_core::Vec2;
use engine_water::WaterWorld;

#[derive(Clone, Copy, Default)]
pub(crate) struct Environment<'a> {
    pub water: Option<&'a WaterWorld>,
    /// Only the ordinary floor or fully closed rain panels support this slice.
    pub ground_available: bool,
    /// Even an ineligible moving panel remains a real foot-contact surface.
    pub panels: Option<&'a ResponsiveFloor>,
}

impl Environment<'_> {
    pub fn floor_support(self, x: f32, layout: Layout) -> Option<f32> {
        if x < layout.bounds_min.x || x > layout.bounds_max.x {
            return None;
        }
        if let Some(panels) = self.panels {
            (f64::from(x.abs()) > panels.opening * panels.shape.max_gap)
                .then(|| panels.shape.surface_y(f64::from(x), panels.opening) as f32)
        } else {
            self.ground_available.then_some(layout.floor_y)
        }
    }

    /// Detect pooled water above this visitor's tolerated depth at either foot.
    pub fn wet_feet(self, feet: Vec2, layout: Layout, tolerance: WaterTolerance) -> bool {
        self.water.is_some_and(|water| {
            [-0.25, 0.0, 0.25].into_iter().any(|x| {
                water
                    .sample(feet + Vec2::new(x, tolerance.foot_depth) * layout.pitch)
                    .is_some()
            })
        })
    }

    /// Exposure from real moving parcels, not the fact that Rain is active.
    /// A short sweep catches fast drops between ticks. This is a behavioral
    /// sensor, not a collider: it neither consumes nor redirects water.
    pub fn spray(self, feet: Vec2, layout: Layout) -> f32 {
        let Some(water) = self.water else { return 0.0 };
        let scale = layout.pitch;
        let mut exposure = 0.0;
        for parcel in water.parcels() {
            let radius = (parcel.volume as f32 / std::f32::consts::PI)
                .sqrt()
                .min(scale * 0.3);
            let min = feet + Vec2::new(-scale * 0.55 - radius, -radius);
            let max = feet + Vec2::new(scale * 0.55 + radius, scale * 1.1 + radius);
            let end = parcel.position;
            let start = end - parcel.velocity / 60.0;
            if crosses(start, end, min, max) {
                exposure += (parcel.volume as f32 / (scale * scale * 0.02)).min(1.0);
            }
        }
        exposure.min(1.0)
    }

    pub fn ground_spots(self, layout: Layout, tolerance: WaterTolerance) -> [Option<Vec2>; 2] {
        if !self.ground_available {
            return [None; 2];
        }
        // Keep the complete silhouette inside the frame and outside even the
        // widest digit. The same vertical lane is used to descend and leave.
        let outer = layout.bounds_min.x + layout.frame_width + layout.pitch * 0.76;
        let inner = layout.face_origin.x - layout.pitch * 0.68;
        if outer > inner {
            return [None; 2];
        }
        [-1.0, 1.0].map(|side| {
            let feet = Vec2::new(side * (outer + inner).abs() * 0.5, layout.floor_y);
            (!self.wet_feet(feet, layout, tolerance)
                && (!tolerance.avoids_spray || self.spray(feet, layout) == 0.0))
                .then_some(feet)
        })
    }
}

fn crosses(start: Vec2, end: Vec2, min: Vec2, max: Vec2) -> bool {
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for (a, b, low, high) in [
        (start.x, end.x, min.x, max.x),
        (start.y, end.y, min.y, max.y),
    ] {
        let delta = b - a;
        if delta.abs() < 1e-6 {
            if a < low || a > high {
                return false;
            }
        } else {
            let u = (low - a) / delta;
            let v = (high - a) / delta;
            enter = enter.max(u.min(v));
            exit = exit.min(u.max(v));
        }
    }
    enter <= exit
}
