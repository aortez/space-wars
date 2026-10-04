use super::*;
use engine_common::{
    Fill, RenderCircle, RenderColor, RenderLine, RenderPolygon, RenderPrimitive, RenderText,
    Stroke, TextAnchor,
};

const INK: RenderColor = RenderColor::rgb(0.91, 0.94, 0.89);
const TEAMS: [RenderColor; 2] = [
    RenderColor::rgb(0.38, 0.81, 0.67),
    RenderColor::rgb(0.96, 0.53, 0.31),
];
fn point(p: Vec2) -> RenderPoint {
    RenderPoint::new(p.x, p.y)
}
fn polygon(
    frame: &mut RenderFrame,
    layer: i32,
    vertices: impl IntoIterator<Item = Vec2>,
    color: RenderColor,
) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Polygon(RenderPolygon {
            points: vertices.into_iter().map(point).collect(),
            fill: Some(Fill::new(color)),
            stroke: None,
        }),
    );
}
fn rect(frame: &mut RenderFrame, layer: i32, low: Vec2, high: Vec2, color: RenderColor) {
    polygon(
        frame,
        layer,
        [
            low,
            Vec2::new(high.x, low.y),
            high,
            Vec2::new(low.x, high.y),
        ],
        color,
    );
}
fn text(
    frame: &mut RenderFrame,
    at: Vec2,
    value: impl Into<String>,
    size: f32,
    color: RenderColor,
) {
    frame.push_primitive(
        20,
        RenderPrimitive::Text(RenderText {
            position: point(at),
            text: value.into(),
            size,
            color,
            anchor: TextAnchor::Center,
        }),
    );
}
fn line(frame: &mut RenderFrame, a: Vec2, b: Vec2, color: RenderColor, width: f32) {
    frame.push_primitive(
        5,
        RenderPrimitive::Line(RenderLine::new(
            point(a),
            point(b),
            Stroke::new(color, width),
        )),
    );
}
fn circle(frame: &mut RenderFrame, at: Vec2, radius: f32, color: RenderColor) {
    frame.push_primitive(
        6,
        RenderPrimitive::Circle(RenderCircle {
            center: point(at),
            radius,
            fill: Some(Fill::new(color)),
            stroke: None,
        }),
    );
}
fn button_center(index: usize) -> Vec2 {
    Vec2::new((index as f32 - 4.0) * 9.5, -26.0)
}
pub(super) fn button_at(at: Vec2) -> Option<Command> {
    Command::ALL.into_iter().enumerate().find_map(|(i, c)| {
        let d = at - button_center(i);
        (d.x.abs() <= 4.35 && d.y.abs() <= 1.8).then_some(c)
    })
}

pub(super) fn frame(state: &ScorchedState) -> RenderFrame {
    let mut frame = RenderFrame::new(state.camera());
    rect(
        &mut frame,
        -30,
        Vec2::new(-1000.0, -1000.0),
        Vec2::new(1000.0, 1000.0),
        RenderColor::rgb(0.055, 0.085, 0.12),
    );
    // Distant hills are presentation only. Foreground material below is drawn
    // from the same interpolated geometry that supplies collision shapes.
    // The raster backend requires convex polygons: emit one trapezoid per ridge segment.
    let ridge = |x: f32| Vec2::new(x, 5.0 + 3.0 * (x * 0.16).sin() + 2.0 * (x * 0.37).cos());
    for i in 0..26 {
        let x = -65.0 + i as f32 * 5.0;
        polygon(
            &mut frame,
            -25,
            [
                Vec2::new(x, -32.0),
                Vec2::new(x + 5.0, -32.0),
                ridge(x + 5.0),
                ridge(x),
            ],
            RenderColor::rgb(0.10, 0.17, 0.21),
        );
    }
    for body in &state.terrain {
        let motion = state.physics.motion(body.assembly.body()).unwrap();
        for chunk in body.geometry.chunks() {
            for r in &chunk.rectangles {
                let center = r.local_center(&body.terrain);
                let half = r.half_extents(&body.terrain);
                polygon(
                    &mut frame,
                    -5,
                    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(x, y)| {
                        motion.position
                            + (center + Vec2::new(x * half.x, y * half.y))
                                .rotate_radians(motion.angle)
                    }),
                    RenderColor::rgb(0.38, 0.30, 0.20),
                );
            }
            for poly in &chunk.polygons {
                polygon(
                    &mut frame,
                    -5,
                    poly.vertices
                        .iter()
                        .map(|p| motion.position + p.rotate_radians(motion.angle)),
                    RenderColor::rgb(0.38, 0.30, 0.20),
                );
            }
        }
    }
    for grain in state.loose.iter() {
        let m = state.physics.motion(grain.body()).unwrap();
        let color = RenderColor::rgb(0.77, 0.59, 0.34);
        if let Some(vertices) = grain.shape().vertices(grain.radius()) {
            polygon(
                &mut frame,
                2,
                vertices
                    .into_iter()
                    .map(|p| m.position + p.rotate_radians(m.angle)),
                color,
            );
        } else {
            circle(&mut frame, m.position, grain.radius(), color);
        }
    }
    for (i, tank) in state.tanks.iter().enumerate() {
        let m = state.physics.motion(tank.body).unwrap();
        let local = |x, y| m.position + Vec2::new(x, y).rotate_radians(m.angle);
        let color = if tank.health > 0.0 {
            TEAMS[i]
        } else {
            RenderColor::rgb(0.24, 0.25, 0.26)
        };
        polygon(
            &mut frame,
            4,
            [
                local(-1.7, -0.65),
                local(1.7, -0.65),
                local(1.9, 0.0),
                local(1.4, 0.6),
                local(-1.4, 0.6),
                local(-1.9, 0.0),
            ],
            color,
        );
        for x in [-1.2, -0.4, 0.4, 1.2] {
            circle(
                &mut frame,
                local(x, -0.5),
                0.30,
                RenderColor::rgb(0.08, 0.11, 0.12),
            );
        }
        let (pivot, muzzle) = state.muzzle(i);
        circle(&mut frame, pivot, 0.7, color);
        line(&mut frame, pivot, muzzle, color, 5.0);
        text(
            &mut frame,
            m.position + Vec2::Y * 4.5,
            format!(
                "{} · {:.0}",
                if i == state.selected && !state.config.demo {
                    "YOU"
                } else {
                    "CPU"
                },
                tank.health
            ),
            12.0,
            color,
        );
    }
    for shell in &state.shells {
        line(
            &mut frame,
            shell.position - shell.velocity.normalized() * 1.4,
            shell.position,
            TEAMS[shell.owner],
            1.6,
        );
        circle(&mut frame, shell.position, 0.19, INK);
    }
    for (position, tick) in &state.flashes {
        let t = (state.tick - tick) as f32 / 22.0;
        frame.push_primitive(
            8,
            RenderPrimitive::Circle(RenderCircle {
                center: point(*position),
                radius: 0.5 + t * BLAST_RADIUS,
                fill: None,
                stroke: Some(Stroke::new(
                    RenderColor::rgba(1.0, 0.78, 0.35, 1.0 - t),
                    2.0,
                )),
            }),
        );
    }
    text(
        &mut frame,
        Vec2::new(0.0, 28.0),
        "SCORCHED EARTH",
        24.0,
        INK,
    );
    let tank = state.tanks[state.selected];
    let shape = if state.config.shape == GrainShape::Round {
        "Round"
    } else {
        "Angular"
    };
    text(
        &mut frame,
        Vec2::new(0.0, 24.0),
        format!(
            "Tank {}   ·   angle {:.0}°   ·   power {:.0}   ·   {} dirt",
            state.selected + 1,
            tank.elevation.to_degrees(),
            tank.speed,
            shape
        ),
        15.0,
        TEAMS[state.selected],
    );
    let result = if let Some(winner) = state.winner() {
        format!("Tank {} wins — Reset to replay the same hills", winner + 1)
    } else if !state.fighting() {
        "Both tanks destroyed — Reset to replay".into()
    } else {
        state.status.into()
    };
    rect(
        &mut frame,
        15,
        Vec2::new(-1000.0, -1000.0),
        Vec2::new(100.0, -18.0),
        RenderColor::rgb(0.055, 0.085, 0.12),
    );
    text(&mut frame, Vec2::new(0.0, -19.6), result, 13.0, INK);
    text(
        &mut frame,
        Vec2::new(0.0, -22.3),
        format!(
            "{} / {} loose  ·  {} returned  ·  {} impacts  ·  {} capacity rejections",
            state.loose_cells(),
            state.config.max_grains,
            state.deposited_cells(),
            state.impacts,
            state.rejected_blasts
        ),
        12.0,
        INK,
    );
    for (i, label) in [
        "Angle −",
        "Angle +",
        "Power −",
        "Power +",
        "FIRE",
        "Tank",
        if state.config.demo { "Demo ON" } else { "Demo" },
        "Dirt",
        "Reset",
    ]
    .into_iter()
    .enumerate()
    {
        let center = button_center(i);
        rect(
            &mut frame,
            18,
            center - Vec2::new(4.35, 1.8),
            center + Vec2::new(4.35, 1.8),
            if i == 4 {
                RenderColor::rgb(0.28, 0.36, 0.27)
            } else {
                RenderColor::rgb(0.14, 0.21, 0.25)
            },
        );
        text(&mut frame, center - Vec2::Y * 0.4, label, 12.0, INK);
    }
    text(
        &mut frame,
        Vec2::new(0.0, -30.0),
        "←/→ angle · ↑/↓ power · Space fire · Tab tank · V demo · T dirt/reset · R restart",
        11.0,
        INK,
    );
    frame
}
