//! Optional immutable polygon artwork. Ordinary world primitives remain the
//! authoritative frame and wire format; renderers may ignore these hints.
use super::{RenderColor, RenderPoint, RenderPolygon};
use std::sync::Arc;

#[derive(Debug)]
pub struct PolygonGeometry {
    polygons: Vec<RenderPolygon>,
    bytes: usize,
    extent: f64,
}

impl PolygonGeometry {
    /// Bounded, finite artwork with fixed styles and logical-pixel strokes.
    /// Replace the Arc after geometry OR style changes; never mutate a definition.
    pub fn new(polygons: Vec<RenderPolygon>) -> Result<Self, String> {
        if polygons.is_empty() || polygons.len() > 4096 {
            return Err("invalid retained polygon group size".into());
        }
        let mut bytes = polygons.capacity() * std::mem::size_of::<RenderPolygon>();
        let mut extent = 0.0_f64;
        for p in &polygons {
            if !(3..=256).contains(&p.points.len())
                || p.fill.is_none() && p.stroke.is_none()
                || p.fill
                    .is_some_and(|f| !valid_color(f.color) || f.color.a == 0.0)
                || p.stroke.is_some_and(|s| {
                    !valid_color(s.color)
                        || s.color.a == 0.0
                        || !s.width.is_finite()
                        || s.width <= 0.0
                })
            {
                return Err("invalid retained polygon style/shape".into());
            }
            bytes += p.points.capacity() * std::mem::size_of::<RenderPoint>();
            for p in &p.points {
                if !p.x.is_finite() || !p.y.is_finite() {
                    return Err("non-finite retained source point".into());
                }
                extent = extent.max(f64::from(p.x).abs() + f64::from(p.y).abs());
            }
        }
        Ok(Self {
            polygons,
            bytes,
            extent,
        })
    }
    pub fn polygons(&self) -> &[RenderPolygon] {
        &self.polygons
    }
    pub fn len(&self) -> usize {
        self.polygons.len()
    }
    pub fn is_empty(&self) -> bool {
        self.polygons.is_empty()
    }
    pub fn payload_bytes(&self) -> usize {
        self.bytes
    }
    pub fn extent(&self) -> f64 {
        self.extent
    }
}

fn valid_color(c: RenderColor) -> bool {
    [c.r, c.g, c.b, c.a]
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}

/// A hint for a contiguous range of ordinary primitives in one layer. Layer
/// indices, not Z values, disambiguate layers with equal Z. Producers retain the
/// definition across frames; pointer identity is local, never persisted.
#[derive(Debug, Clone)]
pub struct PolygonGeometryHint {
    pub layer: usize,
    pub first: usize,
    pub geometry: Arc<PolygonGeometry>,
    pub position: RenderPoint,
    pub angle: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenderFrame;
    #[test]
    fn hints_are_not_wire_or_equality_state() {
        let ordinary = RenderFrame::default();
        let mut hinted = ordinary.clone();
        hinted.geometry_hints.push(PolygonGeometryHint {
            layer: 0,
            first: 0,
            position: RenderPoint::ZERO,
            angle: 0.0,
            geometry: Arc::new(
                PolygonGeometry::new(vec![RenderPolygon::filled(
                    vec![
                        RenderPoint::ZERO,
                        RenderPoint::new(1., 0.),
                        RenderPoint::new(0., 1.),
                    ],
                    RenderColor::RED,
                )])
                .unwrap(),
            ),
        });
        let serialized = toml::to_string(&hinted).unwrap();
        assert_eq!(serialized, toml::to_string(&ordinary).unwrap());
        let restored: RenderFrame = toml::from_str(&serialized).unwrap();
        assert_eq!(hinted, restored);
        assert!(restored.geometry_hints.is_empty());
    }
}
