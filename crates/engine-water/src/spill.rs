//! Finite slices of an attached outfall. Shared material cross-sections keep
//! neighboring slices connected; their interiors still represent their own volume.
use crate::{Parcel, WaterConfig};
use engine_core::Vec2;

mod sheet;
pub(crate) use sheet::Sheet;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy)]
pub struct SpillRibbon {
    quads: [[Vec2; 4]; 12],
    quad_count: usize,
    surface_mask: u16,
    /// Which side continues the upper pool surface (left spills wind oppositely).
    pub surface_side: usize,
}

impl SpillRibbon {
    fn new(surface_side: usize) -> Self {
        Self {
            quads: [[Vec2::ZERO; 4]; 12],
            quad_count: 0,
            surface_mask: 0,
            surface_side,
        }
    }

    /// At most twelve convex pieces, with no allocation. A triangle repeats one
    /// corner. Combined area equals volume before clipping against walls.
    pub fn quads(&self) -> &[[Vec2; 4]] {
        &self.quads[..self.quad_count]
    }

    /// Only pieces bordering the exposed surface receive its highlight.
    pub fn has_surface(&self, index: usize) -> bool {
        index < self.quad_count && self.surface_mask & (1 << index) != 0
    }

    pub(crate) fn translated(mut self, offset: Vec2) -> Self {
        for quad in &mut self.quads[..self.quad_count] {
            for point in quad {
                *point += offset;
            }
        }
        self
    }

    fn push(&mut self, quad: [Vec2; 4], surface: bool) {
        self.quads[self.quad_count] = quad;
        self.surface_mask |= u16::from(surface) << self.quad_count;
        self.quad_count += 1;
    }

    fn push_piece(&mut self, [a, b, c, d]: [Vec2; 4]) -> Option<()> {
        let quad = [a, b, c, d];
        if convex(&quad) {
            self.push(quad, true);
            return Some(());
        }
        // Concave undersides are valid water, but the renderer clips convex
        // pieces against panels. Pick an interior diagonal and keep the surface
        // highlight only on the triangle touching that original outer edge.
        for pair in [[[a, b, c, c], [a, c, c, d]], [[a, b, d, d], [b, b, c, d]]] {
            if pair.iter().all(|q| convex(q) && area(q) >= 0.0) {
                for (i, triangle) in pair.into_iter().enumerate() {
                    self.push(triangle, i == self.surface_side);
                }
                return Some(());
            }
        }
        None
    }
}

/// Only continuous automatic outfalls participate in mixing; independent drips,
/// rain/splash sources remain separate. Outlet IDs are pool_index * 2 + edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpillSource {
    Outlet {
        pool: usize,
        edge: usize,
    },
    Drip {
        pool: usize,
        edge: usize,
    },
    Junction {
        outlets: [usize; 2],
    },
    /// A wet strip uncovered by a moving support, without an imposed outflow.
    Release {
        pool: usize,
    },
}

impl SpillSource {
    pub(crate) fn outlet_id(self) -> Option<usize> {
        match self {
            Self::Outlet { pool, edge } => Some(pool * 2 + edge),
            Self::Drip { .. } | Self::Junction { .. } | Self::Release { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Section {
    pub position: Vec2,
    pub velocity: Vec2,
    /// Unit-depth volume per second through this material cross-section.
    pub flow: f64,
    /// An advected material face, independent of the direction of travel.
    /// Retained deep tanks emit their actual vertical pool boundary.
    pub material_offset: Option<Vec2>,
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
        if let Some(offset) = self.material_offset {
            return offset;
        }
        let speed = self.velocity.length();
        if speed <= 1e-6 {
            return Vec2::ZERO;
        }
        Vec2::new(-self.velocity.y, self.velocity.x) * (self.flow as f32 / (2.0 * speed * speed))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Spill {
    pub sheet: Option<Sheet>,
    pub source: SpillSource,
    pub tick: u64,
    /// Last original emission from each parent of a contiguous merged slice.
    /// Collision scheduling can skip a tick even for continuous incoming water.
    pub upstream_end: Option<[u64; 2]>,
    pub tail: Section,
    pub head: Section,
}

impl Spill {
    pub fn released(parcel: Parcel, sheet: Sheet, pool: usize, tick: u64) -> Self {
        let section = Section {
            position: parcel.position,
            velocity: parcel.velocity,
            flow: 0.0,
            material_offset: None,
        };
        Self {
            sheet: Some(sheet),
            source: SpillSource::Release { pool },
            tick,
            upstream_end: None,
            tail: section,
            head: section,
        }
    }

    pub fn advance(&mut self, dt: f64, config: WaterConfig, bounds: Option<[f64; 2]>) {
        self.tail.advance(dt, config, bounds);
        self.head.advance(dt, config, bounds);
    }

    pub fn ribbon(self, parcel: &Parcel) -> Option<SpillRibbon> {
        if let Some(sheet) = self.sheet {
            return Some(sheet.ribbon(parcel));
        }
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
        if let SpillSource::Outlet { edge, .. } = self.source
            && let Some(ribbon) = self.smooth_outlet(parcel.volume, tail, head, edge)
        {
            return Some(ribbon);
        }
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
        // reopen seams. Solve only the interior width for the transported area.
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
        let mut ribbon = SpillRibbon::new(match self.source {
            SpillSource::Outlet { edge, .. } => edge,
            SpillSource::Drip { .. } | SpillSource::Release { .. } => unreachable!(),
            SpillSource::Junction { .. } => 1,
        });
        for quad in quads {
            ribbon.push(quad, true);
        }
        Some(ribbon)
    }

    fn smooth_outlet(
        self,
        volume: f64,
        tail: Vec2,
        head: Vec2,
        edge: usize,
    ) -> Option<SpillRibbon> {
        let [a, b, c, d] = [
            self.tail.position - tail,
            self.head.position - head,
            self.head.position + head,
            self.tail.position + tail,
        ];
        let envelope = [a, b, c, d];
        let full_area = area(&envelope);
        if full_area <= 1e-9 {
            return None;
        }
        if volume > full_area && convex(&envelope) {
            // A closing lip can compress a slice below its transported area.
            // Extending both material faces would eventually make them cross.
            // Keep the entire envelope and add that area to its underside.
            let (from, to) = if edge == 1 { (a, b) } else { (c, d) };
            let chord = to - from;
            let length_squared = f64::from(chord.length_squared());
            if length_squared <= 1e-10 {
                return None;
            }
            let extension = 2.0 * (volume - full_area) / length_squared.sqrt();
            let scale = f64::from((d - a).length().max((c - b).length())).max(volume.sqrt());
            if extension > scale * 4.0 {
                // Nearly coincident underside endpoints cannot hold arbitrary
                // area without producing an extremely long, thin spike.
                return None;
            }
            let midpoint = (from + to) * 0.5
                + Vec2::new(chord.y, -chord.x)
                    * (2.0 * (volume - full_area) / length_squared) as f32;
            let extra = [from, midpoint, to, to];
            if !midpoint.x.is_finite()
                || !midpoint.y.is_finite()
                || (full_area + area(&extra) - volume).abs() > volume * 1e-4
            {
                return None;
            }
            let mut ribbon = SpillRibbon::new(edge);
            ribbon.push(envelope, true);
            ribbon.push(extra, false);
            return Some(ribbon);
        }
        // A retracting lip separates consecutive material faces. Pinching both
        // sides to correct their area makes teeth in the exposed pool surface.
        // Keep that surface on its shared endpoint chord; adjust the underside.
        // Two short end transitions permit small volumes without collapsing the
        // middle or changing either shared face. Concave undersides are split
        // into triangles below; a self-crossing shape uses the older fallback.
        let transition = (volume / full_area * 0.5).min(0.25) as f32;
        let pieces = |scale: f32| {
            let face = |t: f32| {
                let mut low = a + (b - a) * t;
                let mut high = d + (c - d) * t;
                if edge == 1 {
                    low = high + (low - high) * scale;
                } else {
                    high = low + (high - low) * scale;
                }
                [low, high]
            };
            let [p, q] = face(transition);
            let [r, s] = face(1.0 - transition);
            [[a, p, q, d], [p, r, s, q], [r, b, c, s]]
        };
        let base = pieces(0.0).iter().map(area).sum::<f64>();
        // The underside follows two differently oriented material faces, so
        // its area is quadratic in scale. Solve the positive root without
        // subtracting nearly equal terms when the faces are almost parallel.
        let one = pieces(1.0).iter().map(area).sum::<f64>();
        let two = pieces(2.0).iter().map(area).sum::<f64>();
        let quadratic = (two - 2.0 * one + base) * 0.5;
        let linear = one - base - quadratic;
        let remaining = volume - base;
        let discriminant = linear * linear + 4.0 * quadratic * remaining;
        let scale = (2.0 * remaining / (linear + discriminant.sqrt())) as f32;
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let quads = pieces(scale);
        if (quads.iter().map(area).sum::<f64>() - volume).abs() > volume * 1e-4 {
            return None;
        }
        let mut ribbon = SpillRibbon::new(edge);
        for quad in quads {
            ribbon.push_piece(quad)?;
        }
        Some(ribbon)
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
