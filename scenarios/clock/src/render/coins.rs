//! Coins: the face's lit cells turn into gold pieces where they stand, drop
//! and pile in the arena, then fly home and square up as they arrive. The
//! batch is Explosion's with `Debris::Coins`; only the drawing is this
//! module's.
use super::*;
use crate::events::explosion::ExplosionEvent;

/// A coin's face and its rim.
const COIN_FILL: RenderColor = RenderColor::rgb(0.96, 0.78, 0.26);
const COIN_EDGE: RenderColor = RenderColor::rgb(0.62, 0.44, 0.08);
/// Points on a piece's outline while it is between a square and a disc. A
/// regular 24-gon's radius errs from its circle by under one percent, and the
/// square's corners fall on its vertices.
const OUTLINE_POINTS: usize = 24;

/// How a piece looks: the digit cell or label pixel it was, or the coin.
#[derive(Clone, Copy)]
struct Look {
    fill: RenderColor,
    edge: Option<RenderColor>,
}

impl Look {
    fn cell(cell: &crate::events::explosion::Cell, palette: DigitPalette) -> Self {
        if cell.label {
            Self {
                fill: LABEL_COLOR,
                edge: None,
            }
        } else {
            Self {
                fill: palette.fill,
                edge: Some(palette.edge),
            }
        }
    }

    const COIN: Self = Self {
        fill: COIN_FILL,
        edge: Some(COIN_EDGE),
    };

    fn blend(self, other: Self, t: f32) -> Self {
        Self {
            fill: blend(self.fill, other.fill, t),
            edge: match (self.edge, other.edge) {
                (Some(a), Some(b)) => Some(blend(a, b, t)),
                (a, b) => {
                    if t < 0.5 {
                        a
                    } else {
                        b
                    }
                }
            },
        }
    }
}

fn blend(a: RenderColor, b: RenderColor, t: f32) -> RenderColor {
    RenderColor::rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

pub(super) fn render(
    frame: &mut RenderFrame,
    state: &ClockState,
    event: &ExplosionEvent,
    layout: Layout,
) {
    for segment in state.segments() {
        for cell in segment.guides() {
            render_square(
                frame,
                layout.cell_center(segment.id, cell),
                layout.pitch,
                0.0,
                0.0,
                DigitPalette::default(),
            );
        }
    }
    let palette = DigitPalette::default();
    let stroke_width = (layout.pitch * 0.045).max(0.8);
    // The shape is the event's, so the footprints it publishes agree with
    // what is drawn: the cell's face until the minting turn is edge-on, the
    // coin from then on, and a piece squaring up as it flies home.
    let (width, roundness) = event.piece_shape();
    for cell in &event.cells {
        if cell.side <= 0.0 {
            continue;
        }
        let look = if roundness <= 0.0 {
            Look::cell(cell, palette)
        } else if roundness >= 1.0 {
            Look::COIN
        } else {
            Look::COIN.blend(Look::cell(cell, palette), 1.0 - roundness)
        };
        let layer = if cell.label {
            LABEL_LAYER
        } else {
            ACTIVE_CELL_LAYER
        };
        piece(
            frame,
            layer,
            cell.position,
            cell.side * 0.5,
            cell.angle,
            width,
            roundness,
            look,
            stroke_width,
        );
    }
}

/// One piece of the batch: a square `half` a side from its centre, the disc
/// inscribed in it, or any shape between, `roundness` of the way from the
/// square to the disc, and `width` of its own width across, since a turning
/// piece is foreshortened. A whole disc is drawn as a circle, which costs the
/// renderer far less than a polygon while the pile is largest.
#[allow(clippy::too_many_arguments)]
fn piece(
    frame: &mut RenderFrame,
    layer: i32,
    center: Vec2,
    half: f32,
    angle: f32,
    width: f32,
    roundness: f32,
    look: Look,
    stroke_width: f32,
) {
    let stroke = look.edge.map(|edge| Stroke::new(edge, stroke_width));
    if roundness >= 1.0 && width >= 1.0 {
        frame.push_primitive(
            layer,
            RenderPrimitive::Circle(RenderCircle {
                center: RenderPoint::new(center.x, center.y),
                radius: half,
                fill: Some(Fill::new(look.fill)),
                stroke,
            }),
        );
        return;
    }
    if roundness <= 0.0 {
        // Still the cell: the same four corners the face draws, foreshortened
        // by the turn, with the face's glow, so the first frame is the face.
        let points = [
            Vec2::new(-half * width, -half),
            Vec2::new(half * width, -half),
            Vec2::new(half * width, half),
            Vec2::new(-half * width, half),
        ]
        .map(|offset| {
            let point = center + offset.rotate_radians(angle);
            RenderPoint::new(point.x, point.y)
        });
        glow::quad(frame, points, look.fill, None);
        frame.push_primitive(
            layer,
            RenderPrimitive::Polygon(RenderPolygon {
                points: points.to_vec(),
                fill: Some(Fill::new(look.fill)),
                stroke,
            }),
        );
        return;
    }
    let points = (0..OUTLINE_POINTS)
        .map(|k| {
            let theta = std::f32::consts::TAU * k as f32 / OUTLINE_POINTS as f32;
            let (sin, cos) = theta.sin_cos();
            // The square's radius in this direction, then the disc's.
            let square = half / cos.abs().max(sin.abs());
            let radius = square + (half - square) * roundness;
            let point =
                center + Vec2::new(cos * radius * width, sin * radius).rotate_radians(angle);
            RenderPoint::new(point.x, point.y)
        })
        .collect();
    frame.push_primitive(
        layer,
        RenderPrimitive::Polygon(RenderPolygon {
            points,
            fill: Some(Fill::new(look.fill)),
            stroke,
        }),
    );
}
