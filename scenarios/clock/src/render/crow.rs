use super::*;
use crate::crow::{Crow, Phase};

#[cfg(test)]
mod tests;

/// The side of a pixel of the crow's art, in pitches.
pub(super) const PIXEL: f32 = 0.105;
/// The column the crow's feet are at the left of.
const FEET: f32 = 7.0;

/// The crow's art standing, a row of pixels to a line from its head down:
/// its outline, its black, the white of its eye, and the grey of its bill
/// and legs. It faces right, and its feet stand on the foot of the last row.
pub(super) const STANDING: [&str; 11] = [
    "        OOOO  ",
    "       OBBBBK ",
    "       OBWBBKK",
    "      OBBBBKK ",
    "     OBBBBBO  ",
    " OOOBBBBBBO   ",
    "OBBBBBBBBBO   ",
    " OOBBBBBBO    ",
    "   OOOOOO     ",
    "     K K      ",
    "    KK KK     ",
];
/// The crow's art with its head down at the ground before its feet.
pub(super) const PECKING: [&str; 11] = [
    "              ",
    "              ",
    "              ",
    "      OOOOO   ",
    " OOO OBBBBBOO ",
    "OBBBBBBBBBBBBO",
    " OOBBBBBBBBWBO",
    "   OBBBBBOBBBO",
    "    OOOOO KBBO",
    "     K K  KKK ",
    "    KK KK  K  ",
];
/// How far each row of the standing art goes the way the crow faces when it
/// leans all the way, in pixels, from its head down: its head and neck go,
/// and its body stays.
pub(super) const LEAN: [f32; 11] = [2.0, 2.0, 2.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
/// How far the rows of the crow's head come down when it leans all the way,
/// in pixels.
const DIP: [f32; 11] = [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
/// The wing's shoulder, tip, and trailing and leading edges, in pixels from
/// the feet, with the tip level. Raised all the way the tip is [`WING_BEAT`]
/// higher, and lowered all the way as much lower.
const WING: [(f32, f32); 4] = [(-3.0, 5.0), (-7.0, 7.0), (-1.0, 6.0), (1.0, 4.0)];
pub(super) const WING_BEAT: f32 = 6.0;

const OUTLINE: RenderColor = RenderColor::rgb(0.35, 0.40, 0.51);
const BLACK: RenderColor = RenderColor::rgb(0.08, 0.105, 0.18);
const EYE: RenderColor = RenderColor::rgb(0.9, 0.94, 1.0);
const HORN: RenderColor = RenderColor::rgb(0.48, 0.51, 0.57);

/// How a crow stands or flies, for its silhouette: a crow of a visit to the
/// face, or the crow at the chess board.
pub(super) struct Pose {
    /// The crow's feet.
    pub position: Vec2,
    /// The pitch the crow is drawn at: a pixel of its art is [`PIXEL`] of
    /// it.
    pub pitch: f32,
    pub facing_right: bool,
    /// Its head is down at the ground before its feet.
    pub pecking: bool,
    /// Its wing while it is spread, from raised, 1, to lowered, -1.
    pub wing: Option<f32>,
    /// How far its head leans the way it faces, from 0, upright, to 1.
    pub lean: f32,
    /// How far off the ground it is drawn, in pixels of the art.
    pub hop: f32,
}

/// A crow of a visit to the face.
pub(super) fn render(frame: &mut RenderFrame, crow: &Crow, layout: Layout) {
    silhouette(
        frame,
        &Pose {
            position: crow.position,
            pitch: layout.pitch,
            facing_right: crow.facing_right,
            pecking: crow.phase == Phase::Pecking && (12..20).contains(&(crow.phase_tick % 28)),
            wing: matches!(crow.phase, Phase::Entering | Phase::Flying | Phase::Leaving)
                .then_some(crow.flight.wing),
            lean: 0.0,
            hop: 0.0,
        },
    );
}

/// Draws a crow as `pose` has it. Code-native pixel silhouette, shared by
/// raster and vector adapters.
pub(super) fn silhouette(frame: &mut RenderFrame, pose: &Pose) {
    let pixel = pose.pitch * PIXEL;
    let mirror = if pose.facing_right { 1.0 } else { -1.0 };
    let rows = if pose.pecking { PECKING } else { STANDING };
    // A crow with its head down at the ground leans no further.
    let lean = if pose.pecking { 0.0 } else { pose.lean };
    let transform = |x: f32, y: f32| {
        RenderPoint::new(
            pose.position.x + mirror * x * pixel,
            pose.position.y + (y + pose.hop) * pixel,
        )
    };
    if let Some(flap) = pose.wing {
        let [shoulder, tip, trailing, leading] = WING;
        frame.push_primitive(
            CREATURE_LAYER,
            RenderPrimitive::Polygon(RenderPolygon {
                points: vec![
                    transform(shoulder.0, shoulder.1),
                    transform(tip.0, tip.1 + flap * WING_BEAT),
                    transform(trailing.0, trailing.1),
                    transform(leading.0, leading.1),
                ],
                fill: Some(Fill::new(OUTLINE)),
                stroke: None,
            }),
        );
    }
    for (row, pattern) in rows.into_iter().enumerate() {
        let across = lean * LEAN[row];
        let down = lean * DIP[row];
        for (column, ink) in pattern.bytes().enumerate() {
            let color = match ink {
                b'O' => OUTLINE,
                b'B' => BLACK,
                b'W' => EYE,
                b'K' => HORN,
                _ => continue,
            };
            let a = transform(
                column as f32 - FEET + across,
                (rows.len() - row - 1) as f32 - down,
            );
            let b = transform(
                column as f32 - (FEET - 1.0) + across,
                (rows.len() - row) as f32 - down,
            );
            frame.push_primitive(
                CREATURE_LAYER,
                rectangle(
                    RenderPoint::new(a.x.min(b.x), a.y),
                    RenderPoint::new(a.x.max(b.x), b.y),
                    color,
                    None,
                ),
            );
        }
    }
}
