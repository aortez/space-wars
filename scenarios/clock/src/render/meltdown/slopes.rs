//! Area-preserving reconstruction over an inclined bed. Shared wet face heights
//! plus a cell-local midpoint avoid a staircase without bridging real cliffs.
use super::*;

fn full(c: Column) -> bool {
    c.volume > 0.0 && c.surface >= c.bed_edges[0].max(c.bed_edges[1])
}

fn pieces(c: Column, previous: Option<Column>, next: Option<Column>) -> [[Vec2; 4]; 2] {
    let [left, right] = c.wet_interval();
    let middle = (left + right) * 0.5;
    let mut surface = [c.surface; 3];
    if full(c) {
        for (edge, neighbor) in [previous, next].into_iter().enumerate() {
            if let Some(n) = neighbor
                && full(n)
                && c.bed_edges[edge] == n.bed_edges[1 - edge]
            {
                let depth = ((c.surface + n.surface) * 0.5 - c.bed_edges[edge])
                    .max(0.0)
                    .min(2.0 * c.volume / c.width)
                    .min(2.0 * n.volume / n.width);
                surface[edge * 2] = c.bed_edges[edge] + depth;
            }
        }
        let edge_depth = surface[0] - c.bed_edges[0] + surface[2] - c.bed_edges[1];
        surface[1] = c.bed_at(middle) + (2.0 * c.volume / c.width - edge_depth * 0.5).max(0.0);
    }
    let x = [left, middle, right];
    std::array::from_fn(|i| {
        [
            Vec2::new(x[i] as f32, c.bed_at(x[i]) as f32),
            Vec2::new(x[i + 1] as f32, c.bed_at(x[i + 1]) as f32),
            Vec2::new(x[i + 1] as f32, surface[i + 1] as f32),
            Vec2::new(x[i] as f32, surface[i] as f32),
        ]
    })
}

pub(super) fn render(
    frame: &mut RenderFrame,
    c: Column,
    previous: Option<Column>,
    next: Option<Column>,
    layer: i32,
    colors: [RenderColor; 2],
) {
    for quad in pieces(c, previous, next) {
        let points: Vec<_> = quad.map(|p| RenderPoint::new(p.x, p.y)).into();
        frame.push_primitive(
            layer,
            RenderPrimitive::Polygon(RenderPolygon::filled(points.clone(), colors[0])),
        );
        let mut highlight = points;
        highlight[0].y = quad[3].y - ((quad[3].y - quad[0].y) * 0.24).min(1.2);
        highlight[1].y = quad[2].y - ((quad[2].y - quad[1].y) * 0.24).min(1.2);
        frame.push_primitive(
            layer,
            RenderPrimitive::Polygon(RenderPolygon::filled(highlight, colors[1])),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::water_fixture::{Profile, WaterFixture};

    #[test]
    fn slope_presentation_conserves_area_and_shares_wet_faces() {
        for mirror in [false, true] {
            for depth in [0.03, 1.0, 5.0, 15.0] {
                let mut fixture = WaterFixture::new(depth, mirror, Profile::Ramp);
                for _ in 0..120 {
                    fixture.step(1.0 / 60.0, true);
                    let columns: Vec<_> = fixture.water.pools()[0].columns().collect();
                    for (i, &c) in columns.iter().enumerate() {
                        if c.volume == 0.0 {
                            continue;
                        }
                        let previous = i.checked_sub(1).map(|j| columns[j]);
                        let next = columns.get(i + 1).copied();
                        let q = pieces(c, previous, next);
                        let total: f64 = q.iter().map(|quad| super::super::tests::area(quad)).sum();
                        assert!(
                            (total - c.volume).abs() < 2e-4,
                            "{total} {} {c:?}",
                            c.volume
                        );
                        for p in q.iter().flatten() {
                            assert!(p.y as f64 >= c.bed_at(p.x as f64) - 1e-5);
                        }
                        if let Some(n) = next
                            && full(c)
                            && full(n)
                            && c.bed_edges[1] == n.bed_edges[0]
                        {
                            assert_eq!(
                                q[1][2],
                                pieces(n, Some(c), columns.get(i + 2).copied())[0][3]
                            );
                        }
                    }
                }
            }
        }
    }
}
