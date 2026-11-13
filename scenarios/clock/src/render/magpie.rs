//! The magpie: a code-native pixel silhouette like the crow's, black with a
//! white belly, a long tail with a green sheen, and a coin at its beak when
//! it is carrying one. Shared by the raster and vector adapters.
use super::*;
use crate::events::explosion::magpie::Magpie;
use engine_common::ClockMagpiePhase as Phase;

/// How long the head is down during a take, around the tick the coin
/// leaves the pile.
const HEAD_DOWN: std::ops::Range<u64> = 12..34;

const OUTLINE: RenderColor = RenderColor::rgb(0.16, 0.17, 0.22);
const BLACK: RenderColor = RenderColor::rgb(0.07, 0.07, 0.10);
const SHEEN: RenderColor = RenderColor::rgb(0.13, 0.32, 0.40);
const WHITE: RenderColor = RenderColor::rgb(0.93, 0.94, 0.96);
const HORN: RenderColor = RenderColor::rgb(0.36, 0.37, 0.42);
const GLINT: RenderColor = RenderColor::rgb(0.55, 0.58, 0.66);

/// Standing, facing right: the tail back and a little down, the head up.
/// Column 9 is over the feet, and the beak's tip is at the right edge.
const STANDING: [&str; 12] = [
    "            OOOO  ",
    "           OBBBBO ",
    "           OBEBBKK",
    "    OO      OBBBOK",
    "   OGGO     OBBO  ",
    "  OGGGGO  OOBBBBO ",
    " OGGGGGGOOBBBWWBO ",
    "OGGGGGBBBBBBWWWWO ",
    " OOOOBBGGBBBWWWWO ",
    "      OBGGBBWWWO  ",
    "       OOOOOOOO   ",
    "         K   K    ",
];

/// The head down at the coin in front of the feet.
const PECKING: [&str; 12] = [
    "                  ",
    "                  ",
    "     OO           ",
    "    OGGO          ",
    "   OGGGGOO        ",
    "  OGGGGGGOOOOO    ",
    " OGGGGGBBBBBBBO   ",
    "OGGGGBBBGGBBBWBOO ",
    " OOOOBBGGBBWWWBBBO",
    "      OBBBWWWOBEBK",
    "       OOOOOOOOBKK",
    "         K   K OK ",
];

/// Where the beak's tip is, in pixels from the feet, for each pose.
const BEAK_UP: (f32, f32) = (9.0, 9.5);
const BEAK_DOWN: (f32, f32) = (9.0, 1.0);

pub(super) fn render(frame: &mut RenderFrame, magpie: &Magpie, layout: Layout) {
    let pixel = layout.pitch * 0.105;
    let mirror = if magpie.facing_right { 1.0 } else { -1.0 };
    let head_down = magpie.phase == Phase::Taking && HEAD_DOWN.contains(&magpie.phase_tick);
    let rows = if head_down { PECKING } else { STANDING };
    let transform = |x: f32, y: f32| {
        RenderPoint::new(
            magpie.position.x + mirror * x * pixel,
            magpie.position.y + y * pixel,
        )
    };
    if matches!(
        magpie.phase,
        Phase::Entering | Phase::Flying | Phase::Leaving
    ) {
        let flap = magpie.flight.wing;
        frame.push_primitive(
            CREATURE_LAYER,
            RenderPrimitive::Polygon(RenderPolygon {
                points: vec![
                    transform(-1.0, 6.0),
                    transform(-7.5, 8.0 + flap * 7.0),
                    transform(1.0, 7.5),
                    transform(3.0, 5.5),
                ],
                fill: Some(Fill::new(OUTLINE)),
                stroke: None,
            }),
        );
        // The white of the primaries along the wing's trailing edge.
        frame.push_primitive(
            CREATURE_LAYER,
            RenderPrimitive::Polygon(RenderPolygon {
                points: vec![
                    transform(-4.0, 7.0 + flap * 3.5),
                    transform(-7.5, 8.0 + flap * 7.0),
                    transform(-6.3, 8.8 + flap * 7.0),
                    transform(-3.0, 7.9 + flap * 3.5),
                ],
                fill: Some(Fill::new(WHITE)),
                stroke: None,
            }),
        );
    }
    for (row, pattern) in rows.into_iter().enumerate() {
        for (column, ink) in pattern.bytes().enumerate() {
            let color = match ink {
                b'O' => OUTLINE,
                b'B' => BLACK,
                b'G' => SHEEN,
                b'W' => WHITE,
                b'K' => HORN,
                b'E' => GLINT,
                _ => continue,
            };
            let a = transform(column as f32 - 9.0, (rows.len() - row - 1) as f32);
            let b = transform(column as f32 - 8.0, (rows.len() - row) as f32);
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
    if let Some(side) = magpie.carrying {
        let radius = side * 0.5;
        let (x, y) = if head_down { BEAK_DOWN } else { BEAK_UP };
        let tip = transform(x, y);
        frame.push_primitive(
            CREATURE_LAYER,
            RenderPrimitive::Circle(RenderCircle {
                center: RenderPoint::new(tip.x + mirror * radius * 0.55, tip.y - radius * 0.2),
                radius,
                fill: Some(Fill::new(coins::COIN_FILL)),
                stroke: Some(Stroke::new(
                    coins::COIN_EDGE,
                    (layout.pitch * 0.045).max(0.8),
                )),
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_poses_are_rectangular_and_keep_the_beak_at_the_right_edge() {
        for rows in [STANDING, PECKING] {
            assert!(rows.iter().all(|row| row.len() == 18));
            assert!(rows.iter().any(|row| row.ends_with('K')));
        }
    }
}
