use engine_common::{
    Fill, RenderCircle, RenderColor, RenderFrame, RenderLine, RenderPoint, RenderPolygon,
    RenderPrimitive, RenderText, Stroke, TextAnchor,
};
use engine_core::Vec2;
use engine_rapier::spaceling::{SpacelingBalance, SpacelingGetUpResult};
use engine_terrain::CHUNK_SIZE;

use crate::{ORE, ROCK, TerrainLabState};

const ROCK_COLOR: RenderColor = RenderColor::rgb(0.14, 0.23, 0.30);
const ORE_COLOR: RenderColor = RenderColor::rgb(0.39, 0.46, 0.25);
const LIGHT: RenderColor = RenderColor::rgb(0.87, 0.94, 0.96);
const CYAN: RenderColor = RenderColor::rgb(0.24, 0.90, 0.83);
const ORANGE: RenderColor = RenderColor::rgb(1.0, 0.58, 0.23);

pub(super) fn frame(state: &TerrainLabState) -> RenderFrame {
    let camera = state.camera();
    let hud = |y| Vec2::new(camera.center.x, camera.center.y + camera.height * y);
    let mut frame = RenderFrame::new(camera);
    // Material scans and greedy meshing happen on edits. Each view only transforms
    // the cached local rectangles into the existing renderer's polygon contract.
    for (terrain, geometry, motion, edited_chunks) in state.terrain_bodies() {
        let local_to_world = |local: Vec2| motion.position + local.rotate_radians(motion.angle);
        for chunk in geometry.chunks() {
            for rect in &chunk.rectangles {
                let color = if rect.material == ORE {
                    ORE_COLOR
                } else {
                    ROCK_COLOR
                };
                let local = rect.local_center(terrain);
                let half = rect.half_extents(terrain);
                let points = [
                    Vec2::new(-half.x, -half.y),
                    Vec2::new(half.x, -half.y),
                    Vec2::new(half.x, half.y),
                    Vec2::new(-half.x, half.y),
                ]
                .map(|offset| point(local_to_world(local + offset)))
                .to_vec();
                frame.push_primitive(
                    -10,
                    RenderPrimitive::Polygon(RenderPolygon {
                        points,
                        fill: Some(Fill::new(color)),
                        // Cover antialiasing seams between adjacent filled paths.
                        // The optional contrasting outline exposes the actual cover.
                        stroke: Some(Stroke::new(
                            if state.overlay {
                                RenderColor::rgb(0.31, 0.43, 0.48)
                            } else {
                                color
                            },
                            if state.overlay { 0.6 } else { 0.75 },
                        )),
                    }),
                );
            }
        }
        if state.overlay {
            for chunk in geometry.chunks() {
                let bounds = terrain.chunk_bounds(chunk.id).unwrap();
                let min =
                    terrain.cell_center(bounds.min) - Vec2::new(0.5, 0.5) * terrain.cell_size();
                let max =
                    terrain.cell_center(bounds.max) + Vec2::new(0.5, 0.5) * terrain.cell_size();
                let color = if edited_chunks.contains(&chunk.id) {
                    ORANGE
                } else {
                    CYAN
                };
                let corners = [
                    min,
                    Vec2::new(max.x, min.y),
                    max,
                    Vec2::new(min.x, max.y),
                    min,
                ];
                for edge in corners.windows(2) {
                    line(
                        &mut frame,
                        0,
                        local_to_world(edge[0]),
                        local_to_world(edge[1]),
                        color,
                        1.0,
                    );
                }
            }
        }
    }
    let (start, end, hit) = state.probe();
    if state.overlay {
        line(
            &mut frame,
            1,
            start,
            hit.map_or(end, |hit| hit.point),
            if hit.is_some() { ORANGE } else { CYAN },
            1.5,
        );
        if let Some(hit) = hit {
            circle(&mut frame, hit.point, 0.2, ORANGE);
        }
    }

    let character = state.spaceling_snapshot();
    let (balance, suit) = match character.balance {
        SpacelingBalance::Balanced => ("BALANCED", ORANGE),
        SpacelingBalance::KnockedDown => ("KNOCKED DOWN", RenderColor::rgb(1.0, 0.28, 0.24)),
        SpacelingBalance::Recovering => ("RECOVERING", RenderColor::rgb(1.0, 0.85, 0.25)),
    };
    let local =
        |x, y| character.motion.position + Vec2::new(x, y).rotate_radians(character.motion.angle);
    line(
        &mut frame,
        5,
        local(-0.18, -0.2),
        local(-0.28, -0.75),
        suit,
        3.0,
    );
    line(
        &mut frame,
        5,
        local(0.18, -0.2),
        local(0.28, -0.75),
        suit,
        3.0,
    );
    line(
        &mut frame,
        5,
        local(-0.2, 0.2),
        local(-0.43, -0.1),
        LIGHT,
        2.0,
    );
    line(
        &mut frame,
        5,
        local(0.2, 0.2),
        local(0.43, -0.1),
        LIGHT,
        2.0,
    );
    line(
        &mut frame,
        5,
        local(0.0, -0.25),
        local(0.0, 0.35),
        suit,
        5.0,
    );
    circle(&mut frame, local(0.0, 0.6), 0.29, LIGHT);
    circle(&mut frame, local(0.12, 0.63), 0.16, ROCK_COLOR);
    if state.overlay
        && let Some(support) = character.support
    {
        line(
            &mut frame,
            6,
            support.position,
            support.position + support.normal * 1.5,
            CYAN,
            2.0,
        );
    }
    let mining = state.mining_snapshot();
    let drill_color = if mining.active { ORANGE } else { CYAN };
    let profile = state.tool_profile();
    let mut preview_count = 0;
    if let (Some(brush), Some(target)) = (mining.brush, mining.target) {
        let (terrain, motion) = state
            .terrain_body(target.body)
            .expect("retained preview target");
        for (coord, cell) in terrain.brush_cells(brush).expect("bounded tool brush") {
            preview_count += 1;
            let center = terrain.cell_center(coord);
            let half = terrain.cell_size() * 0.5;
            let points = [(-half, -half), (half, -half), (half, half), (-half, half)]
                .map(|(x, y)| {
                    point(motion.position + (center + Vec2::new(x, y)).rotate_radians(motion.angle))
                })
                .to_vec();
            frame.push_primitive(
                2,
                RenderPrimitive::Polygon(RenderPolygon {
                    points,
                    fill: Some(Fill::new(RenderColor::rgba(0.24, 0.90, 0.83, 0.18))),
                    stroke: Some(Stroke::new(
                        if cell.durability <= profile.damage {
                            LIGHT
                        } else {
                            drill_color
                        },
                        1.2,
                    )),
                }),
            );
        }
    }
    line(
        &mut frame,
        7,
        mining.origin,
        mining.end,
        drill_color,
        if mining.active { 3.0 } else { 1.0 },
    );
    if mining.target.is_some() {
        circle(&mut frame, mining.end, 0.18, drill_color);
    }
    // Keep text readable over rock and ore in the close views. The renderer
    // clips these wide camera-relative panels to the viewport.
    for (bottom, top) in [
        (0.295, 0.5),
        (-0.5, if state.overlay { -0.18 } else { -0.365 }),
    ] {
        let half_width = camera.height * 4.0;
        frame.push_primitive(
            9,
            RenderPrimitive::Polygon(RenderPolygon {
                points: [
                    (-half_width, bottom),
                    (half_width, bottom),
                    (half_width, top),
                    (-half_width, top),
                ]
                .map(|(x, y)| point(hud(y) + Vec2::new(x, 0.0)))
                .to_vec(),
                fill: Some(Fill::new(RenderColor::rgba(0.02, 0.025, 0.05, 0.94))),
                stroke: None,
            }),
        );
    }
    text(&mut frame, hud(0.46), "TERRAIN LAB".into(), 23.0, LIGHT);
    text(
        &mut frame,
        hud(0.407),
        format!(
            "{}   |   {}   |   Reach {:.1}",
            state.selected_tool().name(),
            state.view.name(),
            profile.range
        ),
        16.0,
        CYAN,
    );
    text(
        &mut frame,
        hud(0.36),
        "T / top face: tool   V / right shoulder: view   J / left trigger: hold for debug".into(),
        12.0,
        LIGHT,
    );
    text(
        &mut frame,
        hud(0.32),
        if state.overlay {
            "DEBUG   X / right face: crater   K / left face: tunnel"
        } else if character.needs_get_up() {
            match character.get_up_result {
                SpacelingGetUpResult::Started => "Getting up...",
                SpacelingGetUpResult::Blocked => {
                    "No room to stand. Move sideways or clear space, then tap jump to retry."
                }
                SpacelingGetUpResult::NoSupport => {
                    "Get up needs footing. Tap Space / bottom face again once supported."
                }
                SpacelingGetUpResult::NoGravity => "Get up needs gravity and footing.",
                SpacelingGetUpResult::Unsettled => {
                    "Still tumbling. Tap Space / bottom face again once settled."
                }
                _ => "Space / bottom face: get up   Left stick / A,D: move",
            }
        } else {
            "Aim: right stick / W,S / pointer   Mine: L,LB / ZR,RT / E / hold pointer"
        }
        .into(),
        12.0,
        if state.overlay || character.needs_get_up() {
            suit
        } else {
            LIGHT
        },
    );
    text(
        &mut frame,
        hud(-0.405),
        format!(
            "{}   {}   |   Cut: {} cell{}",
            if mining.active {
                "DRILLING"
            } else {
                "DRILL READY"
            },
            mining.target.map_or_else(
                || "No material in reach".into(),
                |target| format!(
                    "{}: {} / {} durability",
                    if target.material == ORE {
                        "ORE"
                    } else {
                        "ROCK"
                    },
                    target.durability,
                    target.hardness
                ),
            ),
            preview_count,
            if preview_count == 1 { "" } else { "s" },
        ),
        16.0,
        drill_color,
    );
    text(
        &mut frame,
        hud(-0.455),
        format!(
            "Recovered: Rock {:.2} u²   Ore {:.2} u²   |   Fragments: {}",
            state.recovered_area(ROCK),
            state.recovered_area(ORE),
            state.fragments().len(),
        ),
        16.0,
        CYAN,
    );
    if state.overlay {
        text(
            &mut frame,
            hud(-0.225),
            format!(
                "{balance}   Recovery {:.0}%   Settle {:.2}s   Get up: {:?} #{}",
                character.recovery_progress * 100.0,
                character.settled_seconds,
                character.get_up_result,
                character.get_up_attempts,
            ),
            13.0,
            suit,
        );
        text(
            &mut frame,
            hud(-0.27),
            format!(
                "{}   Probe: {}   Removed: {} cells   Revision: {}",
                if character.grounded() {
                    "SUPPORTED"
                } else {
                    "AIRBORNE"
                },
                if hit.is_some() { "SOLID" } else { "CLEAR" },
                state.removed_cells,
                state.terrain.revision()
            ),
            13.0,
            LIGHT,
        );
        text(
            &mut frame,
            hud(-0.315),
            format!(
                "Cell {:.2}   Chunks {} x {}   Quads {}   Colliders {}   Cells {:.1} KiB",
                state.config.cell_size,
                state.terrain.width().div_ceil(CHUNK_SIZE),
                state.terrain.height().div_ceil(CHUNK_SIZE),
                state.rectangle_count(),
                state.collider_count(),
                state.terrain_cell_bytes() as f64 / 1024.0
            ),
            13.0,
            LIGHT,
        );
        text(
            &mut frame,
            hud(-0.36),
            format!(
                "Edit {} cells / {} chunks   Cut {:.2} ms   Split {:.2} ms   Rebuild {:.2} ms",
                state.last_edit.changed_cells,
                state.last_edit.rebuilt_chunks,
                state.last_edit.edit_time.as_secs_f64() * 1000.0,
                state.last_edit.connectivity_time.as_secs_f64() * 1000.0,
                state.last_edit.rebuild_time.as_secs_f64() * 1000.0
            ),
            13.0,
            LIGHT,
        );
    }
    frame
}

fn point(p: Vec2) -> RenderPoint {
    RenderPoint::new(p.x, p.y)
}
fn line(frame: &mut RenderFrame, layer: i32, a: Vec2, b: Vec2, color: RenderColor, width: f32) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Line(RenderLine {
            start: point(a),
            end: point(b),
            stroke: Stroke::new(color, width),
        }),
    );
}
fn circle(frame: &mut RenderFrame, position: Vec2, radius: f32, color: RenderColor) {
    frame.push_primitive(
        6,
        RenderPrimitive::Circle(RenderCircle::filled(point(position), radius, color)),
    );
}
fn text(frame: &mut RenderFrame, position: Vec2, text: String, size: f32, color: RenderColor) {
    frame.push_primitive(
        10,
        RenderPrimitive::Text(RenderText {
            position: point(position),
            text,
            color,
            size,
            anchor: TextAnchor::Center,
        }),
    );
}
