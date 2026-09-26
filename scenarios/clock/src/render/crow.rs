use super::*;
use crate::crow::{CrowVisit, Phase};

/// Code-native pixel silhouette, shared by raster and vector adapters.
pub(super) fn render(frame: &mut RenderFrame, crow: &CrowVisit, layout: Layout) {
    let pixel = layout.pitch * 0.105;
    let mirror = if crow.facing_right { 1.0 } else { -1.0 };
    let rows = [
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
    let transform = |x: f32, y: f32| {
        RenderPoint::new(
            crow.position.x + mirror * x * pixel,
            crow.position.y + y * pixel,
        )
    };
    let outline = RenderColor::rgb(0.35, 0.40, 0.51);
    if matches!(crow.phase, Phase::Entering | Phase::Flying | Phase::Leaving) {
        let flap = (crow.phase_tick as f32 * 0.34).sin();
        frame.push_primitive(
            8,
            RenderPrimitive::Polygon(RenderPolygon {
                points: vec![
                    transform(-3.0, 5.0),
                    transform(-7.0, 7.0 + flap * 6.0),
                    transform(-1.0, 6.0),
                    transform(1.0, 4.0),
                ],
                fill: Some(Fill::new(outline)),
                stroke: None,
            }),
        );
    }
    for (row, pattern) in rows.into_iter().enumerate() {
        for (column, ink) in pattern.bytes().enumerate() {
            let color = match ink {
                b'O' => outline,
                b'B' => RenderColor::rgb(0.08, 0.105, 0.18),
                b'W' => RenderColor::rgb(0.9, 0.94, 1.0),
                b'K' => RenderColor::rgb(0.48, 0.51, 0.57),
                _ => continue,
            };
            let a = transform(column as f32 - 7.0, (rows.len() - row - 1) as f32);
            let b = transform(column as f32 - 6.0, (rows.len() - row) as f32);
            frame.push_primitive(
                8,
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
