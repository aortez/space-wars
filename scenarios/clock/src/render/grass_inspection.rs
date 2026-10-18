//! Magnified material-coordinate cutaway; ordinary world geometry is untouched.
use super::*;
use engine_common::{ClockGrassInspection, RenderLine};

pub(crate) const LAYER: i32 = 40;

pub(crate) fn bounds(layout: Layout) -> (Vec2, Vec2) {
    let width =
        (layout.bounds_max.x - layout.bounds_min.x - 2.0 * layout.frame_width - 12.0).min(220.0);
    let height = (layout.face_origin.y - layout.floor_y - 12.0).min(80.0);
    let bottom = layout.floor_y + 4.0;
    (
        Vec2::new(-width * 0.5, bottom),
        Vec2::new(width * 0.5, bottom + height),
    )
}

pub(super) fn render(frame: &mut RenderFrame, view: &ClockGrassInspection, layout: Layout) {
    let (min, max) = bounds(layout);
    let width = max.x - min.x;
    let height = max.y - min.y;
    let bottom = min.y;
    frame.push_primitive(
        LAYER,
        rectangle(
            RenderPoint::new(-width * 0.5, bottom),
            RenderPoint::new(width * 0.5, bottom + height),
            RenderColor::rgb(0.025, 0.035, 0.05),
            None,
        ),
    );
    let min_x = view
        .tissues
        .iter()
        .map(|t| t.cell[0])
        .chain(view.soil_cells.iter().map(|c| c[0]))
        .min()
        .unwrap_or(0);
    let max_x = view
        .tissues
        .iter()
        .map(|t| t.cell[0])
        .chain(view.soil_cells.iter().map(|c| c[0]))
        .max()
        .unwrap_or(1);
    let max_y = view
        .tissues
        .iter()
        .map(|t| t.cell[1])
        .max()
        .unwrap_or(3)
        .max(3);
    let scale = ((width - 8.0) / (max_x - min_x + 2) as f32)
        .min((height - 20.0) / (max_y + 2) as f32)
        .min(10.0);
    let center = |cell: [i32; 2]| {
        Vec2::new(
            (cell[0] as f32 - (min_x + max_x) as f32 * 0.5) * scale,
            bottom + 4.0 + (cell[1] as f32 + 0.5) * scale,
        )
    };
    for &cell in &view.soil_cells {
        let c = center(cell);
        let h = scale * 0.5;
        frame.push_primitive(
            LAYER,
            rectangle(
                RenderPoint::new(c.x - h, c.y - h),
                RenderPoint::new(c.x + h, c.y + h),
                RenderColor::rgb(0.25, 0.15, 0.075),
                None,
            ),
        );
    }
    for &[a, b] in &view.connections {
        let a = view.tissues.iter().find(|t| t.id == a).unwrap();
        let b = view.tissues.iter().find(|t| t.id == b).unwrap();
        let a = center(a.cell);
        let b = center(b.cell);
        frame.push_primitive(
            LAYER + 1,
            RenderPrimitive::Line(RenderLine::new(
                RenderPoint::new(a.x, a.y),
                RenderPoint::new(b.x, b.y),
                Stroke::new(RenderColor::rgb(0.55, 0.55, 0.55), 1.0),
            )),
        );
    }
    for t in &view.tissues {
        let c = center(t.cell);
        let progress = t.development[0] as f32 / view.development_per_size[0].max(1) as f32;
        let h = scale * (0.22 + 0.06 * (f32::from(t.size) + progress));
        let color = if t.health == 0 {
            RenderColor::rgb(0.4, 0.3, 0.2)
        } else {
            match t.role.as_str() {
                "root" => RenderColor::rgb(0.85, 0.9, 1.0),
                "runner" => RenderColor::rgb(1.0, 0.7, 0.12),
                _ => RenderColor::rgb(0.25, 0.85, 0.25),
            }
        };
        frame.push_primitive(
            LAYER + 2,
            rectangle(
                RenderPoint::new(c.x - h, c.y - h),
                RenderPoint::new(c.x + h, c.y + h),
                color,
                None,
            ),
        );
    }
    frame.push_primitive(
        LAYER + 3,
        RenderPrimitive::Text(RenderText {
            position: RenderPoint::new(0.0, bottom + height - 8.0),
            text: match view.organism {
                Some(id) => format!("Roots (white) / runners (gold) - {id}"),
                None => "Grass cutaway - no living patch".into(),
            },
            color: LABEL_COLOR,
            size: 10.0,
            anchor: TextAnchor::Center,
        }),
    );
}
