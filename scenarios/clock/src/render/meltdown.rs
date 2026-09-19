//! Meltdown uses bounded convex primitives in both render paths.
use super::*;
use crate::events::meltdown::{MeltdownEvent, soften};
use engine_water::{Column, Parcel};

const WATER_COLOR: RenderColor = RenderColor::rgb(0.08, 0.55, 0.85);
const WATER_EDGE: RenderColor = RenderColor::rgb(0.36, 0.91, 1.0);

#[cfg(test)]
mod tests;

fn visible(column: Column) -> bool {
    column.volume > 0.0 && column.surface - column.bed >= 0.25
}

/// Shared face heights smooth the staircase without changing total area over
/// a contiguous wet, same-bed run. Endpoints keep their original heights:
/// trapezoidal integration then sums to the original column areas exactly.
/// Never bridge dry columns, bed steps, or the space between separate pools.
fn surface_edges(column: Column, previous: Option<Column>, next: Option<Column>) -> [f64; 2] {
    [previous, next].map(|neighbor| {
        neighbor
            .filter(|c| visible(*c) && c.bed == column.bed)
            .map_or(column.surface, |c| (c.surface + column.surface) * 0.5)
    })
}

fn ribbon(parcel: Parcel) -> [Vec2; 4] {
    let speed = parcel.velocity.length();
    let direction = if speed > 0.001 {
        parcel.velocity / speed
    } else {
        Vec2::new(0.0, -1.0)
    };
    let length = (speed * parcel.duration as f32).max(0.5);
    // A slightly tapered finite segment: the leading part falls faster and
    // narrows. Normalize the two widths so its area is still exactly volume.
    let acceleration = 400.0 * (-direction.y).max(0.0) * parcel.duration as f32 * 0.5;
    let back_speed = (speed - acceleration).max(1.0);
    let front_speed = (speed + acceleration).max(1.0);
    let width_sum = parcel.volume as f32 / length;
    let back_width = width_sum * front_speed / (front_speed + back_speed);
    let front_width = width_sum - back_width;
    let along = direction * (length * 0.5);
    let across = Vec2::new(-direction.y, direction.x);
    [
        parcel.position - along - across * back_width,
        parcel.position + along - across * front_width,
        parcel.position + along + across * front_width,
        parcel.position - along + across * back_width,
    ]
}

fn drop_outline(parcel: Parcel) -> [Vec2; 8] {
    let diagonal = std::f32::consts::FRAC_1_SQRT_2;
    let radius = (parcel.volume as f32 / (2.0 * std::f32::consts::SQRT_2)).sqrt();
    [
        Vec2::new(1.0, 0.0),
        Vec2::new(diagonal, diagonal),
        Vec2::new(0.0, 1.0),
        Vec2::new(-diagonal, diagonal),
        Vec2::new(-1.0, 0.0),
        Vec2::new(-diagonal, -diagonal),
        Vec2::new(0.0, -1.0),
        Vec2::new(diagonal, -diagonal),
    ]
    .map(|p| parcel.position + p * radius)
}

fn clip_channel<const N: usize>(points: [Vec2; N], bounds: Option<[f64; 2]>) -> Vec<RenderPoint> {
    assert!(N <= 8);
    let mut polygon = [Vec2::ZERO; 12];
    polygon[..N].copy_from_slice(&points);
    let mut len = N;
    if let Some([left, right]) = bounds {
        // Intersect with the channel, rather than independently moving
        // vertices onto its walls (which shears and can invert thin ribbons).
        for (sign, limit) in [(-1.0, -left as f32), (1.0, right as f32)] {
            if len == 0 {
                break;
            }
            let input = polygon;
            let count = len;
            len = 0;
            let mut previous = input[count - 1];
            for &current in &input[..count] {
                let old_inside = previous.x * sign <= limit;
                let new_inside = current.x * sign <= limit;
                if old_inside != new_inside {
                    let t = (limit * sign - previous.x) / (current.x - previous.x);
                    polygon[len] =
                        Vec2::new(limit * sign, previous.y + (current.y - previous.y) * t);
                    len += 1;
                }
                if new_inside {
                    polygon[len] = current;
                    len += 1;
                }
                previous = current;
            }
        }
    }
    if len > 0 && polygon[..len].iter().all(|p| p.x == polygon[0].x) {
        return Vec::new();
    }
    polygon[..len]
        .iter()
        .map(|p| RenderPoint::new(p.x, p.y))
        .collect()
}

pub(super) fn reform_progress(phase_tick: u64, row: i8) -> f32 {
    let delay = row.max(0) as u64 * 4;
    soften(phase_tick.saturating_sub(delay) as f32 / (REFORMING_TICKS - 1 - 32) as f32)
}

pub(super) fn render_reforming_face(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    let palette = state.palette();
    for segment in state.segments() {
        for cell in digits::cells(segment.id.kind) {
            let center = layout.cell_center(segment.id, *cell);
            render_square(frame, center, layout.pitch, 0.0, 0.0, palette);
            if !segment.lit {
                continue;
            }
            let progress = reform_progress(state.phase_tick(), cell.y);
            if progress == 0.0 {
                continue;
            }
            let half = layout.pitch * 0.4;
            frame.push_primitive(
                ACTIVE_CELL_LAYER,
                rectangle(
                    RenderPoint::new(center.x - half, center.y - half),
                    RenderPoint::new(center.x + half, center.y - half + 2.0 * half * progress),
                    RenderColor {
                        a: progress,
                        ..palette.fill
                    },
                    Some(Stroke::new(
                        RenderColor {
                            a: progress,
                            ..palette.edge
                        },
                        (layout.pitch * 0.045).max(0.8),
                    )),
                ),
            );
        }
    }
}

pub(super) fn render_water(
    frame: &mut RenderFrame,
    water: &engine_water::WaterWorld,
    layer: i32,
    opacity: f32,
) {
    let color = RenderColor {
        a: opacity,
        ..WATER_COLOR
    };
    let edge_color = RenderColor {
        a: opacity,
        ..WATER_EDGE
    };
    for pool in water.pools() {
        let mut columns = pool.columns().peekable();
        let mut previous = None;
        while let Some(column) = columns.next() {
            if visible(column) {
                let [left, right] = surface_edges(column, previous, columns.peek().copied());
                let points = [
                    RenderPoint::new(column.left as f32, column.bed as f32),
                    RenderPoint::new((column.left + column.width) as f32, column.bed as f32),
                    RenderPoint::new((column.left + column.width) as f32, right as f32),
                    RenderPoint::new(column.left as f32, left as f32),
                ];
                frame.push_primitive(
                    layer,
                    RenderPrimitive::Polygon(RenderPolygon::filled(points.to_vec(), color)),
                );
                let mut edge = points;
                edge[0].y = (left - ((left - column.bed) * 0.24).min(1.2)) as f32;
                edge[1].y = (right - ((right - column.bed) * 0.24).min(1.2)) as f32;
                frame.push_primitive(
                    layer,
                    RenderPrimitive::Polygon(RenderPolygon::filled(edge.to_vec(), edge_color)),
                );
            }
            previous = Some(column);
        }
    }
    // The ribbons represent water still in flight. Their area is the transported
    // volume: accelerating water stretches and thins rather than retaining a
    // pool-height rectangle down the entire cliff.
    for (index, parcel) in water.parcels().iter().enumerate() {
        if let Some(spill) = water.spill_ribbon(index) {
            for quad in spill.quads {
                let points = clip_channel(quad, parcel.horizontal_bounds);
                if points.len() >= 3 {
                    frame.push_primitive(
                        layer,
                        RenderPrimitive::Polygon(RenderPolygon::filled(points, color)),
                    );
                }
                // Continue the pool's bright surface down the same shared
                // faces; keep the highlight inside the represented water.
                let (a, b, c, d) = if spill.surface_side == 1 {
                    (quad[3], quad[2], quad[1], quad[0])
                } else {
                    (quad[0], quad[1], quad[2], quad[3])
                };
                let inset = |outer: Vec2, inner: Vec2| {
                    outer + (inner - outer) * (1.2 / (inner - outer).length().max(1.2)).min(0.24)
                };
                let points =
                    clip_channel([inset(a, d), inset(b, c), b, a], parcel.horizontal_bounds);
                if points.len() >= 3 {
                    frame.push_primitive(
                        layer,
                        RenderPrimitive::Polygon(RenderPolygon::filled(points, edge_color)),
                    );
                }
            }
            continue;
        }
        if (parcel.volume as f32)
            < 0.05 * (parcel.velocity.length() * parcel.duration as f32).max(0.5)
        {
            continue;
        }
        // Compact parcels remain drops at the apex instead of becoming very
        // wide ribbons as speed approaches zero. Both outlines preserve area.
        let points = if parcel.velocity.length() * (parcel.duration as f32)
            < (parcel.volume as f32).sqrt()
        {
            clip_channel(drop_outline(*parcel), parcel.horizontal_bounds)
        } else {
            clip_channel(ribbon(*parcel), parcel.horizontal_bounds)
        };
        if points.len() < 3 {
            continue;
        }
        frame.push_primitive(
            layer,
            RenderPrimitive::Polygon(RenderPolygon {
                points,
                fill: Some(Fill::new(color)),
                stroke: None,
            }),
        );
    }
}

pub(super) fn render(frame: &mut RenderFrame, event: &MeltdownEvent, layout: Layout) {
    if let Some(lab) = &event.floats {
        for (min, max) in &lab.supports {
            frame.push_primitive(
                ARENA_LAYER,
                rectangle(
                    RenderPoint::new(min.x, min.y),
                    RenderPoint::new(max.x, max.y),
                    FLOOR_EDGE_COLOR,
                    None,
                ),
            );
        }
    }
    render_water(frame, &event.water, ACTIVE_CELL_LAYER, 1.0);
    // Keep solid blocks legible as they pass through the pool to the floor.
    for cell in &event.cells {
        if cell.meridiem {
            super::meridiem::pixel(
                frame,
                cell.position,
                layout.pitch * crate::meridiem::PIXEL_SIZE,
                cell.angle,
                1.0,
            );
            continue;
        }
        render_square(
            frame,
            cell.position,
            layout.pitch,
            cell.angle,
            1.0,
            DigitPalette::default(),
        );
    }
    if let Some(lab) = &event.floats {
        if let Some(y) = lab.reference_y {
            let spec = event.water.pools()[0].spec();
            let width = spec.column_width * spec.bed.len() as f64;
            for i in 0..24 {
                let x = (spec.left + width * i as f64 / 24.0) as f32;
                frame.push_primitive(
                    ACTIVE_CELL_LAYER,
                    rectangle(
                        RenderPoint::new(x, y - 0.35),
                        RenderPoint::new(x + width as f32 / 48.0, y + 0.35),
                        RenderColor::rgb(0.58, 0.65, 0.8),
                        None,
                    ),
                );
            }
        }
        let bodies = lab
            .bodies
            .iter()
            .map(|b| (b.body.body(), b.body.shape(), b.palette))
            .chain(lab.piston.iter().map(|p| {
                (
                    p.body,
                    engine_water::immersion::HullShape::Box {
                        half_width: p.half_extents.x,
                        half_height: p.half_extents.y,
                    },
                    0,
                )
            }));
        for (body, shape, index) in bodies {
            let motion = lab.world.motion(body).expect("live water-lab body");
            let color = match index {
                0 => RenderColor::rgb(1.0, 0.62, 0.18),
                1 => RenderColor::rgb(1.0, 0.91, 0.28),
                _ => RenderColor::rgb(0.85, 0.32, 0.4),
            };
            let (sides, radius, half_width, half_height) = match shape {
                engine_water::immersion::HullShape::Circle { radius } => (32, radius, 0.0, 0.0),
                engine_water::immersion::HullShape::Box {
                    half_width,
                    half_height,
                } => (4, 0.0, half_width, half_height),
            };
            let (sin, cos) = motion.angle.sin_cos();
            let points = (0..sides)
                .map(|i| {
                    let p = if sides == 4 {
                        [
                            Vec2::new(-half_width, -half_height),
                            Vec2::new(half_width, -half_height),
                            Vec2::new(half_width, half_height),
                            Vec2::new(-half_width, half_height),
                        ][i]
                    } else {
                        let a = std::f32::consts::TAU * i as f32 / sides as f32;
                        Vec2::new(a.cos() * radius, a.sin() * radius)
                    };
                    RenderPoint::new(
                        motion.position.x + p.x * cos - p.y * sin,
                        motion.position.y + p.x * sin + p.y * cos,
                    )
                })
                .collect();
            frame.push_primitive(
                ACTIVE_CELL_LAYER,
                RenderPrimitive::Polygon(RenderPolygon {
                    points,
                    fill: Some(Fill::new(color)),
                    stroke: Some(Stroke::new(RenderColor::rgb(1.0, 0.95, 0.8), 0.6)),
                }),
            );
        }
    }
}
