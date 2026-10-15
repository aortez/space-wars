//! Shared convex primitives keep the software and GPU aquarium identical.
use super::*;
use crate::events::aquarium::{AquariumEvent, Fish, Species, fish_radius};

const WATER: RenderColor = RenderColor::rgb(0.022, 0.13, 0.20);
const SURFACE: RenderColor = RenderColor::rgb(0.20, 0.65, 0.73);

pub(super) fn water(frame: &mut RenderFrame, event: &AquariumEvent) {
    // Use the same sloped-bed surfaces and conserved spill silhouettes throughout
    // the event. Drawing the pools again when the hatch opens changes their tint.
    meltdown::render_water_colors(frame, &event.water, BACKGROUND_LAYER, [WATER, SURFACE]);
}

pub(super) fn residents(frame: &mut RenderFrame, event: &AquariumEvent) {
    let layout = event.layout;
    let radius = fish_radius(layout);
    let wet_port = event
        .water
        .sample(event.port)
        .is_some_and(|s| s.surface_y as f32 > event.port.y + radius);
    if wet_port {
        // A visible water-filled side passage explains both arrival and exit.
        let x = layout.bounds_min.x;
        frame.push_primitive(
            13,
            rectangle(
                RenderPoint::new(x, event.port.y - radius * 1.45),
                RenderPoint::new(x + layout.frame_width * 1.8, event.port.y + radius * 1.45),
                RenderColor::rgb(0.06, 0.29, 0.34),
                None,
            ),
        );
        frame.push_primitive(
            13,
            rectangle(
                RenderPoint::new(x, event.port.y - radius * 1.12),
                RenderPoint::new(x + layout.frame_width * 1.8, event.port.y + radius * 1.12),
                WATER,
                None,
            ),
        );
    }
    for fish in event.fish().iter().filter(|f| f.visible()) {
        render_fish(frame, fish, layout);
    }
}

fn render_fish(frame: &mut RenderFrame, fish: &Fish, layout: Layout) {
    let scale = layout.pitch * 0.08;
    let fin = fish.tail_phase.sin() * 0.7;
    let (body, dark, light) = match fish.species {
        Species::Tetra => (
            RenderColor::rgb(0.13, 0.39, 0.51),
            RenderColor::rgb(0.88, 0.10, 0.19),
            RenderColor::rgb(0.22, 0.92, 1.0),
        ),
        Species::Danio => (
            RenderColor::rgb(0.76, 0.75, 0.48),
            RenderColor::rgb(0.08, 0.23, 0.39),
            RenderColor::rgb(0.92, 0.93, 0.72),
        ),
        Species::Cory => (
            RenderColor::rgb(0.64, 0.55, 0.36),
            RenderColor::rgb(0.23, 0.27, 0.24),
            RenderColor::rgb(0.91, 0.81, 0.57),
        ),
    };
    let mut polygon = |points: &[(f32, f32)], color| {
        let mut points: Vec<Vec2> = points
            .iter()
            .map(|&(x, y)| {
                fish.position
                    + (Vec2::new(x, y * if fish.heading.cos() < 0.0 { -1.0 } else { 1.0 }) * scale)
                        .rotate_radians(fish.heading)
            })
            .collect();
        // Only the side passage is allowed outside the frame; clip at the
        // visible screen boundary while the complete actor crosses offscreen.
        let left = layout.bounds_min.x;
        let input = std::mem::take(&mut points);
        if let Some(mut previous) = input.last().copied() {
            for current in input {
                if (previous.x >= left) != (current.x >= left) {
                    let t = (left - previous.x) / (current.x - previous.x);
                    points.push(Vec2::new(left, previous.y + t * (current.y - previous.y)));
                }
                if current.x >= left {
                    points.push(current);
                }
                previous = current;
            }
        }
        if points.len() >= 3 {
            frame.push_primitive(
                14,
                RenderPrimitive::Polygon(RenderPolygon {
                    points: points.iter().map(|p| RenderPoint::new(p.x, p.y)).collect(),
                    fill: Some(Fill { color }),
                    stroke: None,
                }),
            );
        }
    };
    polygon(
        &[
            (-5.2, -2.0 + fin),
            (-2.5, -0.4),
            (-2.5, 0.4),
            (-5.2, 2.0 + fin),
        ],
        body,
    );
    polygon(&[(-2.0, 1.2), (0.0, 2.8), (1.2, 1.2)], body);
    polygon(
        &[
            (-3.2, -0.6),
            (-2.2, -1.5),
            (2.5, -1.5),
            (4.5, -0.5),
            (4.5, 0.5),
            (2.3, 1.5),
            (-2.2, 1.5),
            (-3.2, 0.6),
        ],
        body,
    );
    match fish.species {
        Species::Tetra => {
            polygon(&[(-3.0, 0.0), (3.6, 0.0), (3.6, 0.8), (-3.0, 0.8)], light);
            polygon(
                &[(-2.9, -0.9), (0.8, -1.1), (0.8, -0.05), (-2.9, -0.05)],
                dark,
            );
            polygon(
                &[(0.8, -1.1), (3.0, -0.9), (3.0, -0.1), (0.8, -0.1)],
                RenderColor::rgb(0.64, 0.81, 0.81),
            );
        }
        Species::Danio => {
            for y in [-0.9, 0.0, 0.9] {
                polygon(
                    &[
                        (-2.8, y - 0.2),
                        (3.2, y - 0.2),
                        (3.2, y + 0.2),
                        (-2.8, y + 0.2),
                    ],
                    dark,
                );
            }
            polygon(
                &[
                    (-4.9, -1.2 + fin),
                    (-2.8, -0.2),
                    (-2.8, 0.2),
                    (-4.9, 0.5 + fin),
                ],
                light,
            );
        }
        Species::Cory => {
            polygon(
                &[(-2.0, -1.35), (3.2, -1.35), (3.2, -0.7), (-2.0, -0.7)],
                light,
            );
            for x in [-1.8, 0.2, 2.0] {
                polygon(
                    &[(x, -0.4), (x + 0.8, -0.4), (x + 0.8, 0.6), (x, 0.6)],
                    dark,
                );
            }
            polygon(&[(3.6, -0.6), (5.2, -1.7), (5.4, -1.4), (3.9, -0.3)], light);
            polygon(&[(-0.5, -0.7), (0.8, -2.5), (1.5, -2.2), (1.0, -0.6)], body);
        }
    }
    polygon(&[(2.7, 0.1), (3.7, 0.1), (3.7, 1.05), (2.7, 1.05)], light);
    polygon(
        &[(3.1, 0.25), (3.7, 0.25), (3.7, 0.85), (3.1, 0.85)],
        RenderColor::rgb(0.01, 0.035, 0.045),
    );
}
