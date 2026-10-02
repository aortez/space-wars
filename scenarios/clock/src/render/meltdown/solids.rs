//! Subtract the same finite panels used by falling-water collision. The solver
//! transports parcel centers; ribbons have width and can straddle a corner.
//! Split into convex pieces so neither render adapter draws through a wall.
use super::*;
use engine_water::SolidBox;

pub(super) fn render(
    frame: &mut RenderFrame,
    points: Vec<RenderPoint>,
    boxes: &[SolidBox],
    layer: i32,
    color: RenderColor,
) {
    if points.len() < 3 {
        return;
    }
    let Some((solid, rest)) = boxes.split_first() else {
        frame.push_primitive(
            layer,
            RenderPrimitive::Polygon(RenderPolygon::filled(points, color)),
        );
        return;
    };
    let planes = solid.planes();
    if planes
        .iter()
        .any(|&(n, offset)| points.iter().all(|p| distance(*p, n, offset) >= 0.0))
    {
        // The usual case: the entire ribbon misses this panel. No splitting
        // or scratch allocation for open-air flight or the central drain jet.
        render(frame, points, rest, layer, color);
        return;
    }
    let mut remaining = points;
    for (normal, offset) in planes {
        if remaining.len() < 3 {
            break;
        }
        let outside = clip(&remaining, normal, offset, true);
        render(frame, outside, rest, layer, color);
        remaining = clip(&remaining, normal, offset, false);
    }
    // What remains is inside all four planes, hence inside the solid.
}

fn distance(p: RenderPoint, normal: Vec2, offset: f32) -> f32 {
    p.x * normal.x + p.y * normal.y - offset
}

fn clip(points: &[RenderPoint], normal: Vec2, offset: f32, outside: bool) -> Vec<RenderPoint> {
    let sign = if outside { 1.0 } else { -1.0 };
    let mut result = Vec::with_capacity(points.len() + 1);
    let mut previous = points[points.len() - 1];
    let mut old_distance = distance(previous, normal, offset) * sign;
    for &current in points {
        let new_distance = distance(current, normal, offset) * sign;
        if (old_distance >= 0.0) != (new_distance >= 0.0) {
            let t = old_distance / (old_distance - new_distance);
            result.push(RenderPoint::new(
                previous.x + (current.x - previous.x) * t,
                previous.y + (current.y - previous.y) * t,
            ));
        }
        if new_distance >= 0.0 {
            result.push(current);
        }
        previous = current;
        old_distance = new_distance;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains(points: &[RenderPoint], point: Vec2) -> bool {
        let mut inside = false;
        for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
            if (a.y > point.y) != (b.y > point.y)
                && point.x < a.x + (b.x - a.x) * (point.y - a.y) / (b.y - a.y)
            {
                inside = !inside;
            }
        }
        inside
    }

    #[test]
    fn subtraction_keeps_only_visible_water_and_preserves_corners_in_convex_pieces() {
        for angle in [0.0, 0.3, -0.7] {
            let center = Vec2::new(0.3, -0.1);
            let half = Vec2::new(1.0, 0.5);
            let solid = SolidBox::new(center, half, angle).unwrap();
            let mut frame = RenderFrame::new(Camera2::new(RenderPoint::new(0.0, 0.0), 10.0));
            render(
                &mut frame,
                [(-3.0, -3.0), (3.0, -3.0), (3.0, 3.0), (-3.0, 3.0)]
                    .map(|(x, y)| RenderPoint::new(x, y))
                    .to_vec(),
                &[solid],
                0,
                WATER_COLOR,
            );
            let polygons: Vec<_> = frame
                .layers
                .iter()
                .flat_map(|layer| &layer.primitives)
                .map(|p| match p {
                    RenderPrimitive::Polygon(p) => p.points.as_slice(),
                    _ => panic!("expected convex pieces"),
                })
                .collect();
            assert!(polygons.len() <= 4);
            let area: f64 = polygons
                .iter()
                .map(|p| {
                    super::super::tests::area(
                        &p.iter().map(|p| Vec2::new(p.x, p.y)).collect::<Vec<_>>(),
                    )
                })
                .sum();
            assert!((area - 34.0).abs() < 1e-5, "{angle}: {area}");
            // Independent point-in-polygon oracle, including points on all
            // sides of a panel embedded in the middle of a wide ribbon.
            for y in -29..30 {
                for x in -29..30 {
                    let point = Vec2::new(x as f32 * 0.1 + 0.013, y as f32 * 0.1 + 0.021);
                    let local = (point - center).rotate_radians(-angle);
                    let in_solid = local.x.abs() < half.x && local.y.abs() < half.y;
                    assert_eq!(polygons.iter().any(|p| contains(p, point)), !in_solid);
                }
            }
        }
    }
}
