//! Bounded attribution of guarded outlines. The benchmark enables this only in
//! a separate diagnostic pass, never in timed plain/profiled or live rendering.
//! "clip-candidate" is not a visibility claim: a bounding box can cross the
//! viewport even when none of its triangles cover a pixel.
use super::{
    encode_profile::DrawProfile,
    mesh::Vertex,
    plan::{Draw, Shape},
    presentation::Clip,
    stroke_culling,
};
use std::{
    io::{self, Write},
    time::Duration,
};

const VISIBILITY: [&str; 4] = ["empty", "unclipped", "outside", "clip-candidate"];
const POINTS: [&str; 6] = ["other", "3", "4", "5-8", "9-16", "17+"];
const EXTENT: [&str; 7] = ["unknown", "0-1", "1-2", "2-4", "4-8", "8-32", "32+"];
const BUCKETS: usize = VISIBILITY.len() * POINTS.len() * EXTENT.len();

#[derive(Default)]
struct Bucket {
    draws: u64,
    points: u64,
    vertices: u64,
    indices: u64,
    width_sum: f64,
    extent_sum: f64,
    short_edges: u64,
    collapsed_edges: u64,
    prepare: Duration,
    tessellate: Duration,
    finish: Duration,
}

pub(crate) struct Census {
    buckets: [Bucket; BUCKETS],
    pub capture: Option<capture::Capture>,
}

pub(crate) mod capture;

impl Default for Census {
    fn default() -> Self {
        Self {
            buckets: std::array::from_fn(|_| Bucket::default()),
            capture: None,
        }
    }
}

impl Census {
    pub fn totals(&self) -> [u64; 4] {
        self.buckets.iter().fold([0; 4], |mut sum, b| {
            for (sum, value) in sum
                .iter_mut()
                .zip([b.draws, b.points, b.vertices, b.indices])
            {
                *sum += value;
            }
            sum
        })
    }

    pub fn record(
        &mut self,
        draw: &Draw,
        vertices: &[Vertex],
        clip: Option<Clip>,
        p: &DrawProfile,
    ) {
        let visibility = if p.indices == 0 {
            0
        } else if let Some(clip) = clip {
            if stroke_culling::vertices_outside(vertices, clip) {
                2
            } else {
                3
            }
        } else {
            1
        };
        let Shape::Polygon(points) = &draw.shape else {
            // Guarded reusable outlines are projected polygons today. Retain a
            // catch-all so new shape kinds cannot silently disappear from totals.
            self.add(visibility, 0, 0, draw, p, 0., 0, 0);
            return;
        };
        let point_class = match points.len() {
            3 => 1,
            4 => 2,
            5..=8 => 3,
            9..=16 => 4,
            17.. => 5,
            _ => 0,
        };
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for point in points {
            for (i, v) in [f64::from(point.x), f64::from(point.y)]
                .into_iter()
                .enumerate()
            {
                if !v.is_finite() {
                    self.add(visibility, point_class, 0, draw, p, 0., 0, 0);
                    return;
                }
                lo[i] = lo[i].min(v);
                hi[i] = hi[i].max(v);
            }
        }
        let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]);
        if !extent.is_finite() {
            self.add(visibility, point_class, 0, draw, p, 0., 0, 0);
            return;
        }
        let extent_class = if extent <= 1. {
            1
        } else if extent <= 2. {
            2
        } else if extent <= 4. {
            3
        } else if extent <= 8. {
            4
        } else if extent <= 32. {
            5
        } else {
            6
        };
        let width = f64::from(draw.stroke.map_or(0., |s| s.width));
        // Descriptive geometry only, not a replacement for Lyon's actual merge
        // or join rules. Include the closing edge; use f64 to avoid subtraction
        // overflow while inspecting finite projected coordinates.
        let merge_squared = (0.25_f64.powi(2) * 0.5).min(width * width * 0.05).max(1e-8);
        let (mut short, mut collapsed) = (0, 0);
        for (a, b) in points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
        {
            let d2 = (f64::from(a.x) - f64::from(b.x)).powi(2)
                + (f64::from(a.y) - f64::from(b.y)).powi(2);
            short += u64::from(d2 < width * width);
            collapsed += u64::from(d2 < merge_squared);
        }
        self.add(
            visibility,
            point_class,
            extent_class,
            draw,
            p,
            extent,
            short,
            collapsed,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        visibility: usize,
        points: usize,
        extent_class: usize,
        draw: &Draw,
        p: &DrawProfile,
        extent: f64,
        short: u64,
        collapsed: u64,
    ) {
        let b =
            &mut self.buckets[(visibility * POINTS.len() + points) * EXTENT.len() + extent_class];
        b.draws += 1;
        b.points += p.points;
        b.vertices += p.vertices;
        b.indices += p.indices;
        b.width_sum += f64::from(draw.stroke.map_or(0., |s| s.width));
        b.extent_sum += extent;
        b.short_edges += short;
        b.collapsed_edges += collapsed;
        b.prepare += p.prepare;
        b.tessellate += p.stroke;
        b.finish += p.finish;
    }

    pub fn write_csv(
        &self,
        writer: &mut impl Write,
        fixture: &str,
        mode: &str,
        frames: usize,
    ) -> io::Result<()> {
        for (i, b) in self.buckets.iter().enumerate().filter(|(_, b)| b.draws > 0) {
            let extent = EXTENT[i % EXTENT.len()];
            let points = POINTS[(i / EXTENT.len()) % POINTS.len()];
            let visibility = VISIBILITY[i / (POINTS.len() * EXTENT.len())];
            writeln!(
                writer,
                "{fixture},{mode},{frames},{visibility},{points},{extent},{},{},{},{},{:.6},{:.6},{},{},{:.6},{:.6},{:.6}",
                b.draws,
                b.points,
                b.vertices,
                b.indices,
                b.width_sum / b.draws as f64,
                b.extent_sum / b.draws as f64,
                b.short_edges,
                b.collapsed_edges,
                b.prepare.as_secs_f64() * 1000.,
                b.tessellate.as_secs_f64() * 1000.,
                b.finish.as_secs_f64() * 1000.
            )?;
        }
        Ok(())
    }

    pub const HEADER: &str = "fixture,mode,frames,visibility,points_class,extent_class_px,draws,points,vertices,indices,mean_width_px,mean_extent_px,edges_below_width,edges_below_merge_distance,prepare_ms,tessellate_ms,finish_ms";
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{RenderColor, RenderPoint, Stroke};

    #[test]
    fn bounded_buckets_account_for_closed_edges_visibility_and_nested_times() {
        let mut c = Census::default();
        let draw = Draw {
            shape: Shape::Polygon(vec![
                RenderPoint::new(0., 0.),
                RenderPoint::new(0.01, 0.),
                RenderPoint::new(0.5, 0.5),
            ]),
            fill: None,
            stroke: Some(Stroke::new(RenderColor::WHITE, 1.)),
        };
        let profile = DrawProfile {
            points: 3,
            vertices: 3,
            indices: 3,
            prepare: Duration::from_micros(2),
            stroke: Duration::from_micros(7),
            finish: Duration::from_micros(1),
            ..Default::default()
        };
        let clip = Clip {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        let inside = [[0.; 6]; 3];
        let outside = [[-10.; 6]; 3];
        c.record(&draw, &inside, Some(clip), &profile);
        c.record(&draw, &inside, None, &profile);
        c.record(&draw, &outside, Some(clip), &profile);
        c.record(
            &draw,
            &[],
            Some(clip),
            &DrawProfile {
                points: 3,
                ..Default::default()
            },
        );
        assert_eq!(c.buckets.iter().map(|b| b.draws).sum::<u64>(), 4);
        assert_eq!(c.totals(), [4, 12, 9, 9]);
        assert_eq!(Census::HEADER.split(',').count(), 17);
        assert_eq!(c.buckets.iter().map(|b| b.short_edges).sum::<u64>(), 12);
        assert_eq!(c.buckets.iter().map(|b| b.collapsed_edges).sum::<u64>(), 4);
        let mut csv = Vec::new();
        c.write_csv(&mut csv, "fixture", "reject", 1).unwrap();
        let csv = String::from_utf8(csv).unwrap();
        assert_eq!(csv.lines().count(), 4);
        for name in VISIBILITY {
            assert!(csv.contains(&format!(",{name},3,0-1,")));
        }
        assert!(csv.contains(",0.002000,0.007000,0.001000"));
    }

    #[test]
    fn nonpolygon_and_invalid_bounds_are_reported_not_dropped() {
        let mut c = Census::default();
        for shape in [
            Shape::Line {
                start: RenderPoint::ZERO,
                end: RenderPoint::ZERO,
            },
            Shape::Polygon(vec![]),
            Shape::Polygon(vec![RenderPoint::new(f32::NAN, 0.); 3]),
        ] {
            c.record(
                &Draw {
                    shape,
                    fill: None,
                    stroke: None,
                },
                &[],
                None,
                &DrawProfile::default(),
            );
        }
        assert_eq!(c.buckets.iter().map(|b| b.draws).sum::<u64>(), 3);
        assert_eq!(c.buckets.iter().filter(|b| b.draws > 0).count(), 2);
    }
}
