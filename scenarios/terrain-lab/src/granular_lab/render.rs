use super::*;
use crate::ORE;
use engine_common::{
    Fill, RenderCircle, RenderColor, RenderLine, RenderPolygon, RenderPrimitive, RenderText,
    Stroke, TextAnchor,
};

const SOIL: RenderColor = RenderColor::rgb(0.69, 0.48, 0.26);
const ORE_COLOR: RenderColor = RenderColor::rgb(0.86, 0.67, 0.36);
const TEXT: RenderColor = RenderColor::rgb(0.92, 0.96, 0.95);
const ACCENT: RenderColor = RenderColor::rgb(0.38, 0.92, 0.81);

fn point(p: Vec2) -> RenderPoint {
    RenderPoint::new(p.x, p.y)
}
fn polygon(frame: &mut RenderFrame, points: Vec<Vec2>, color: RenderColor, layer: i32) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Polygon(RenderPolygon {
            points: points.into_iter().map(point).collect(),
            fill: Some(Fill::new(color)),
            stroke: Some(Stroke::new(color, 0.6)),
        }),
    );
}
fn text(frame: &mut RenderFrame, p: Vec2, value: String, size: f32, color: RenderColor) {
    frame.push_primitive(
        20,
        RenderPrimitive::Text(RenderText {
            position: point(p),
            text: value,
            size,
            color,
            anchor: TextAnchor::Center,
        }),
    );
}
fn outline(frame: &mut RenderFrame, p: Vec2, radius: f32, color: RenderColor) {
    frame.push_primitive(
        5,
        RenderPrimitive::Circle(RenderCircle {
            center: point(p),
            radius,
            fill: None,
            stroke: Some(Stroke::new(color, 1.5)),
        }),
    );
}

pub(super) fn frame(state: &GranularLabState) -> RenderFrame {
    let camera = state.camera();
    let h = camera.height;
    let center = Vec2::new(camera.center.x, camera.center.y);
    let mut frame = RenderFrame::new(camera);
    for (body, motion) in state.lab.bodies() {
        for chunk in body.geometry.chunks() {
            for rect in &chunk.rectangles {
                let local = rect.local_center(&body.terrain);
                let half = rect.half_extents(&body.terrain);
                let vertices = [
                    Vec2::new(-half.x, -half.y),
                    Vec2::new(half.x, -half.y),
                    Vec2::new(half.x, half.y),
                    Vec2::new(-half.x, half.y),
                ];
                polygon(
                    &mut frame,
                    vertices
                        .map(|p| motion.position + (local + p).rotate_radians(motion.angle))
                        .to_vec(),
                    if rect.material == ORE {
                        RenderColor::rgb(0.38, 0.35, 0.22)
                    } else {
                        RenderColor::rgb(0.22, 0.29, 0.27)
                    },
                    -5,
                );
            }
        }
    }
    for (grain, motion) in state.lab.grains() {
        let color = if grain.cell().material == ORE {
            ORE_COLOR
        } else {
            SOIL
        };
        if let Some(vertices) = grain.shape().vertices(grain.radius()) {
            polygon(
                &mut frame,
                vertices
                    .into_iter()
                    .map(|p| motion.position + p.rotate_radians(motion.angle))
                    .collect(),
                color,
                0,
            );
        } else {
            frame.push_primitive(
                0,
                RenderPrimitive::Circle(RenderCircle {
                    center: point(motion.position),
                    radius: grain.radius(),
                    fill: Some(Fill::new(color)),
                    stroke: None,
                }),
            );
        }
    }
    if let Some(probe) = state.lab.probe_snapshot() {
        let half = probe.half_extents;
        let vertices = [
            Vec2::new(-half.x, -half.y),
            Vec2::new(half.x, -half.y),
            Vec2::new(half.x, half.y),
            Vec2::new(-half.x, half.y),
        ];
        polygon(
            &mut frame,
            vertices
                .map(|p| probe.motion.position + p.rotate_radians(probe.motion.angle))
                .to_vec(),
            ACCENT,
            2,
        );
        text(
            &mut frame,
            probe.motion.position,
            "BOX".into(),
            11.0,
            RenderColor::rgb(0.04, 0.16, 0.14),
        );
    }
    let aim = state.aim();
    outline(&mut frame, aim, 3.0, RenderColor::rgb(0.73, 0.50, 0.30));
    for dir in [Vec2::X, Vec2::Y] {
        frame.push_primitive(
            6,
            RenderPrimitive::Line(RenderLine {
                start: point(aim - dir * 0.25),
                end: point(aim + dir * 0.25),
                stroke: Stroke::new(ACCENT, 1.5),
            }),
        );
    }
    if let Some((position, tick)) = state.flash
        && state.lab.tick.saturating_sub(tick) < 18
    {
        outline(
            &mut frame,
            position,
            3.0 + state.lab.tick.saturating_sub(tick) as f32 * 0.025,
            ORE_COLOR,
        );
    }
    // HUD rectangles also define the pointer hit targets, in camera coordinates.
    for command in Command::ALL {
        let p = state.button_center(command);
        let half = Vec2::new(h * 0.094, h * 0.028);
        polygon(
            &mut frame,
            vec![
                p - half,
                p + Vec2::new(half.x, -half.y),
                p + half,
                p + Vec2::new(-half.x, half.y),
            ],
            RenderColor::rgb(0.12, 0.20, 0.24),
            15,
        );
        text(&mut frame, p, state.button_label(command), 13.0, TEXT);
    }
    text(
        &mut frame,
        center + Vec2::Y * h * 0.475,
        "TERRAIN LAB · LOOSE DIRT".into(),
        18.0,
        TEXT,
    );
    let probe = state.lab.probe_snapshot().map_or("No box", |p| {
        if p.grain_contacts > 0 {
            "Box on loose dirt"
        } else if p.ground_contacts > 0 {
            "Box on solid terrain"
        } else {
            "Box unsupported"
        }
    });
    text(
        &mut frame,
        center - Vec2::Y * h * 0.34,
        format!(
            "{} / {} loose · {} resting · {} settled · {}",
            state.lab.loose_body_count(),
            state.config.max_loose_bodies,
            state.motion.supported_slow_cells,
            state.lab.deposited_cells(),
            probe
        ),
        14.0,
        TEXT,
    );
    let area = state.config.cell_size.powi(2);
    text(
        &mut frame,
        center - Vec2::Y * h * 0.385,
        format!(
            "Material area: solid {:.2} + loose {:.2} = {:.2} · t {:.2}s{}",
            state.balance.ground as f32 * area,
            state.balance.loose as f32 * area,
            state.balance.initial as f32 * area,
            state.lab.tick as f32 / FIXED_HZ as f32,
            if state.paused { " · paused" } else { "" }
        ),
        13.0,
        ACCENT,
    );
    text(
        &mut frame,
        center - Vec2::Y * h * 0.435,
        state.status.clone(),
        12.0,
        TEXT,
    );
    text(
        &mut frame,
        center - Vec2::Y * h * 0.48,
        "Click: blast · arrows/WASD: aim · T: grains · V: ground · X: limit · wheel: zoom".into(),
        11.0,
        TEXT,
    );
    frame
}
