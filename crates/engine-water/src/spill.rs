//! Finite slices of an attached outfall. Shared material cross-sections keep
//! neighboring slices connected; their interiors still represent their own volume.
use crate::{Parcel, WaterConfig};
use engine_core::Vec2;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy)]
pub struct SpillRibbon {
    /// Two convex pieces. Their combined area equals the parcel's volume before
    /// clipping against channel walls. No Clock or renderer types are involved.
    pub quads: [[Vec2; 4]; 2],
    /// Which side continues the upper pool surface (left spills wind oppositely).
    pub surface_side: usize,
}

/// Only continuous automatic outfalls participate in mixing; independent drips,
/// rain/splash sources remain separate. Outlet IDs are pool_index * 2 + edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpillSource {
    Outlet { pool: usize, edge: usize },
    Drip { pool: usize, edge: usize },
    Junction { outlets: [usize; 2] },
}

impl SpillSource {
    pub(crate) fn outlet_id(self) -> Option<usize> {
        match self {
            Self::Outlet { pool, edge } => Some(pool * 2 + edge),
            Self::Drip { .. } | Self::Junction { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Section {
    pub position: Vec2,
    pub velocity: Vec2,
    /// Unit-depth volume per second through this material cross-section.
    pub flow: f64,
}

impl Section {
    pub fn advance(&mut self, dt: f64, config: WaterConfig, bounds: Option<[f64; 2]>) {
        self.position +=
            self.velocity * dt as f32 + Vec2::new(0.0, (-0.5 * config.gravity * dt * dt) as f32);
        self.velocity.y =
            (self.velocity.y - (config.gravity * dt) as f32).max(-config.max_speed as f32);
        if let Some([left, right]) = bounds {
            let x = self.position.x.clamp(left as f32, right as f32);
            if x != self.position.x {
                self.position.x = x;
                self.velocity.x = 0.0;
            }
        }
    }

    fn offset(self) -> Vec2 {
        let speed = self.velocity.length();
        if speed <= 1e-6 {
            return Vec2::ZERO;
        }
        Vec2::new(-self.velocity.y, self.velocity.x) * (self.flow as f32 / (2.0 * speed * speed))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Spill {
    pub source: SpillSource,
    pub tick: u64,
    /// Last original emission from each parent of a contiguous merged slice.
    /// Collision scheduling can skip a tick even for continuous incoming water.
    pub upstream_end: Option<[u64; 2]>,
    pub tail: Section,
    pub head: Section,
}

impl Spill {
    pub fn advance(&mut self, dt: f64, config: WaterConfig, bounds: Option<[f64; 2]>) {
        self.tail.advance(dt, config, bounds);
        self.head.advance(dt, config, bounds);
    }

    pub fn ribbon(self, parcel: &Parcel) -> Option<SpillRibbon> {
        if matches!(self.source, SpillSource::Drip { .. }) {
            return None;
        }
        let along = self.head.position - self.tail.position;
        let length = along.length();
        if length <= 1e-6 {
            return None;
        }
        let center = (self.head.position + self.tail.position) * 0.5;
        let tail = self.tail.offset();
        let head = self.head.offset();
        // A rising/falling source moves the section centers vertically too.
        // Its center-to-center chord is NOT the material flow direction: using
        // that chord can fold the midpoint back through the lip. Interpolate
        // the material faces instead, then solve area along that direction.
        let average = tail.normalized() + head.normalized();
        let normal = if average.length_squared() > 1e-10 {
            average.normalized()
        } else {
            Vec2::new(-along.y, along.x) / length
        };
        let span = along.x as f64 * normal.y as f64 - along.y as f64 * normal.x as f64;
        if span <= 1e-9 {
            return None;
        }
        let pieces = |half_width: f32| {
            let mid = normal * half_width;
            [
                [
                    self.tail.position - tail,
                    center - mid,
                    center + mid,
                    self.tail.position + tail,
                ],
                [
                    center - mid,
                    self.head.position - head,
                    self.head.position + head,
                    center + mid,
                ],
            ]
        };
        // Shared faces are immutable, so normalizing every whole parcel would
        // reopen seams. Solve ONLY the interior width for the transported area.
        let base = pieces(0.0).iter().map(area).sum::<f64>();
        let half_width = ((parcel.volume - base) / span) as f32;
        if !half_width.is_finite() || half_width <= 0.0 {
            // Abrupt source changes or wall-compressed slices may not admit a
            // positive connected strip. Preserve volume via the detached shape.
            return None;
        }
        if matches!(self.source, SpillSource::Junction { .. })
            && (half_width > 2.0 * tail.length().max(head.length())
                || half_width.max(tail.length()).max(head.length())
                    > 2.0 * (parcel.volume as f32).sqrt())
        {
            // A migrating junction can squeeze two material faces together.
            // Both faces may themselves be wide after opposing flows mix, so
            // an endpoint-relative test alone misses the resulting thin spike.
            // Bound this merged slice against its own area scale as well. The
            // compact fallback still represents all its volume; steady outlet
            // faces and all transport/mixing state are untouched.
            return None;
        }
        let quads = pieces(half_width);
        if !quads.iter().all(convex) {
            return None;
        }
        Some(SpillRibbon {
            quads,
            surface_side: match self.source {
                SpillSource::Outlet { edge, .. } => edge,
                SpillSource::Drip { .. } => unreachable!(),
                SpillSource::Junction { .. } => 1,
            },
        })
    }
}

fn area(points: &[Vec2; 4]) -> f64 {
    let anchor = points[0];
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| {
            let a = *a - anchor;
            let b = *b - anchor;
            a.x as f64 * b.y as f64 - a.y as f64 * b.x as f64
        })
        .sum::<f64>()
        * 0.5
}

fn convex(points: &[Vec2; 4]) -> bool {
    (0..4).all(|i| {
        let a = points[(i + 1) % 4] - points[i];
        let b = points[(i + 2) % 4] - points[(i + 1) % 4];
        a.x * b.y - a.y * b.x >= -1e-6
    })
}
