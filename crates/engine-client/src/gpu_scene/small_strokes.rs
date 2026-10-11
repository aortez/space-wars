//! Explicit render-lab approximation, not a production LOD policy. Only tiny
//! opaque polygons whose outline matches their fill are eligible. Lyon still
//! handles merging, folding, caps and output order; only round joins become bevels.
use super::plan::{Draw, Shape};

/// Return projected source bounds for the restricted one-pixel experiment.
/// Cached templates deliberately remain the reference; cold/warm and zoom
/// transitions therefore belong to the image-quality test, not just throughput.
pub(crate) fn bounds(draw: &Draw) -> Option<[f32; 4]> {
    let stroke = draw.stroke?;
    if draw.fill != Some(stroke.color)
        || stroke.color.a != 1.
        || [stroke.color.r, stroke.color.g, stroke.color.b]
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        || !stroke.width.is_finite()
        || !(0.001..=1.).contains(&stroke.width)
    {
        return None;
    }
    let Shape::Polygon(points) = &draw.shape else {
        return None;
    };
    if !(3..=64).contains(&points.len()) {
        return None;
    }
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for point in points {
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x.abs() > 16_384.
            || point.y.abs() > 16_384.
        {
            return None;
        }
        bounds[0] = bounds[0].min(point.x);
        bounds[1] = bounds[1].min(point.y);
        bounds[2] = bounds[2].max(point.x);
        bounds[3] = bounds[3].max(point.y);
    }
    ((bounds[2] - bounds[0]).max(bounds[3] - bounds[1]) <= 1.).then_some(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_scene::mesh::MeshBuilder;
    use engine_common::{RenderColor, RenderPoint, Stroke};

    fn draw(extent: f32) -> Draw {
        let color = RenderColor::rgb(0.2, 0.6, 0.4);
        Draw {
            shape: Shape::Polygon(vec![
                RenderPoint::ZERO,
                RenderPoint::new(extent, 0.),
                RenderPoint::new(0.4 * extent, extent),
            ]),
            fill: Some(color),
            stroke: Some(Stroke::new(color, 0.75)),
        }
    }

    #[test]
    fn selection_is_bounded_and_preserves_distinct_outlines_and_transparency() {
        assert!(bounds(&draw(1.)).is_some());
        assert!(bounds(&draw(1.0001)).is_none());
        let mut d = draw(0.8);
        d.fill = None;
        assert!(bounds(&d).is_none());
        d.fill = Some(RenderColor::RED);
        assert!(bounds(&d).is_none());
        for width in [0., 1.001, f32::NAN, f32::INFINITY] {
            let mut d = draw(0.8);
            d.stroke.as_mut().unwrap().width = width;
            assert!(bounds(&d).is_none());
        }
        for color in [
            RenderColor::rgba(0.2, 0.6, 0.4, 0.5),
            RenderColor::rgb(f32::NAN, 0., 0.),
        ] {
            let mut d = draw(0.8);
            d.fill = Some(color);
            d.stroke.as_mut().unwrap().color = color;
            assert!(bounds(&d).is_none());
        }
        for points in [
            vec![],
            vec![RenderPoint::ZERO; 65],
            vec![RenderPoint::new(f32::NAN, 0.); 3],
            vec![RenderPoint::new(16_385., 0.); 3],
        ] {
            let mut d = draw(0.8);
            d.shape = Shape::Polygon(points);
            assert!(bounds(&d).is_none());
        }
        let mut d = draw(0.8);
        d.shape = Shape::Circle {
            center: RenderPoint::ZERO,
            radius: 0.1,
        };
        assert!(bounds(&d).is_none());
    }

    #[test]
    fn control_is_explicit_and_large_or_fill_only_draws_are_exact() {
        let mut a = MeshBuilder::default();
        let mut b = MeshBuilder::default();
        b.small_strokes = true;
        for size in [2., 40.] {
            let d = draw(size);
            a.append_draw(&d).unwrap();
            b.append_draw(&d).unwrap();
        }
        let d = draw(0.8);
        a.append_draw_with_styles(&d, d.fill, None).unwrap();
        b.append_draw_with_styles(&d, d.fill, None).unwrap();
        assert_eq!(a.buffers.vertices, b.buffers.vertices);
        assert_eq!(a.buffers.indices, b.buffers.indices);
        assert_eq!(b.small_strokes_used, 0);
        // Style overrides used by cached fill/stroke splitting must not apply
        // eligibility calculated from a different stroke width or color.
        for stroke in [
            Stroke::new(d.fill.unwrap(), 2.),
            Stroke::new(RenderColor::RED, 0.75),
        ] {
            a.append_draw_with_styles(&d, d.fill, Some(stroke)).unwrap();
            b.append_draw_with_styles(&d, d.fill, Some(stroke)).unwrap();
        }
        assert_eq!(a.buffers.vertices, b.buffers.vertices);
        assert_eq!(a.buffers.indices, b.buffers.indices);
        assert_eq!(b.small_strokes_used, 0);
        a.append_draw(&d).unwrap();
        b.append_draw(&d).unwrap();
        assert_eq!(a.small_strokes_used, 0);
        assert_eq!(b.small_strokes_used, 1);
        a.validate().unwrap();
        b.validate().unwrap();
        b.clear();
        b.small_strokes = false;
        a.clear();
        a.append_draw(&d).unwrap();
        b.append_draw(&d).unwrap();
        assert_eq!(a.buffers.vertices, b.buffers.vertices);
        assert_eq!(a.buffers.indices, b.buffers.indices);
    }
}
