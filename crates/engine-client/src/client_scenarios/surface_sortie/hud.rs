//! Compact logical-pixel overlay shared by raster and vector presentation.

use engine_common::{
    Camera2, Fill, RenderColor, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive,
    RenderText, TextAnchor,
};
use scenario_spacewars::surface_sortie::{SurfaceSortieState, hud::PlayerHud};

use crate::render::{Viewport, player_hud_layout};

const LIGHT: RenderColor = RenderColor::rgb(0.86, 0.93, 0.97);
const CYAN: RenderColor = RenderColor::rgb(0.25, 0.93, 0.8);
const AMBER: RenderColor = RenderColor::rgb(1.0, 0.82, 0.25);
const SHADOW: RenderColor = RenderColor::rgb(0.02, 0.025, 0.045);

pub(super) fn frame(state: &SurfaceSortieState, viewport: Viewport) -> RenderFrame {
    let players: Vec<_> = (0..state.player_count())
        .map(|seat| state.player_hud(seat))
        .collect();
    compose(&players, state.hud_title().as_deref(), viewport)
}

fn compose(players: &[PlayerHud], clock: Option<&str>, viewport: Viewport) -> RenderFrame {
    let mut frame = RenderFrame::new(Camera2::new(
        RenderPoint::new(viewport.width * 0.5, -viewport.height * 0.5),
        viewport.height,
    ));
    let panes = Viewport::new(viewport.width, viewport.height).split_horizontally(players.len());
    if players.len() == 2 {
        rect(
            &mut frame,
            Viewport::with_origin(
                viewport.width * 0.5 - 0.5,
                38.0,
                1.0,
                viewport.height - 38.0,
            ),
            RenderColor::rgba(0.35, 0.4, 0.5, 0.6),
        );
    }
    if let Some(clock) = clock {
        label(
            &mut frame,
            (viewport.width * 0.5, 17.0),
            clock,
            LIGHT,
            14.0,
            true,
            150.0,
        );
    }
    for (hud, pane) in players.iter().zip(panes) {
        let layout = player_hud_layout(pane, hud.player);
        let v = layout.vitals;
        // Deliberately no panel/backing here: just labels and meter tracks.
        label(
            &mut frame,
            (v.x, v.y),
            &format!("P{} · {}", hud.player + 1, hud.mode),
            hud.color,
            14.0,
            false,
            v.width,
        );
        meter(
            &mut frame,
            v.x,
            v.y + 25.0,
            v.width,
            hud.health.label,
            hud.health.fraction,
            hud.color,
        );
        if let Some(resource) = &hud.resource {
            meter(
                &mut frame,
                v.x,
                v.y + 52.0,
                v.width,
                resource.label,
                resource.fraction,
                CYAN,
            );
        }
        let note_width = v.width
            - if hud.rounds_loaded.is_some() {
                30.0
            } else {
                0.0
            };
        label(
            &mut frame,
            (v.x, v.y + 80.0),
            &hud.note,
            LIGHT,
            12.0,
            false,
            note_width,
        );
        if let Some(rounds) = hud.rounds_loaded {
            for index in 0..2 {
                rect(
                    &mut frame,
                    Viewport::with_origin(
                        v.x + v.width - 23.0 + index as f32 * 13.0,
                        v.y + 83.0,
                        8.0,
                        4.0,
                    ),
                    if index < rounds {
                        AMBER
                    } else {
                        RenderColor::rgb(0.2, 0.25, 0.3)
                    },
                );
            }
        }
        if let Some(prompt) = &hud.prompt {
            let p = layout.prompt;
            let details = wrap(&prompt.detail, 32);
            let height = 32.0
                + details.len() as f32 * 15.0
                + if prompt.progress.is_some() { 12.0 } else { 0.0 };
            rect(
                &mut frame,
                Viewport { height, ..p },
                RenderColor::rgba(0.025, 0.04, 0.07, 0.88),
            );
            let color = if prompt.warning { AMBER } else { CYAN };
            label(
                &mut frame,
                (p.x + p.width * 0.5, p.y + 13.0),
                &prompt.title,
                color,
                14.0,
                true,
                p.width - 18.0,
            );
            for (index, detail) in details.iter().enumerate() {
                label(
                    &mut frame,
                    (p.x + p.width * 0.5, p.y + 33.0 + index as f32 * 15.0),
                    detail,
                    LIGHT,
                    12.0,
                    true,
                    p.width - 18.0,
                );
            }
            if let Some(progress) = prompt.progress {
                bar(
                    &mut frame,
                    p.x + 10.0,
                    p.y + height - 9.0,
                    p.width - 62.0,
                    progress,
                    color,
                );
                label(
                    &mut frame,
                    (p.x + p.width - 43.0, p.y + height - 16.0),
                    &format!("{:.0}%", progress * 100.0),
                    color,
                    11.0,
                    false,
                    36.0,
                );
            }
        }
    }
    frame
}

fn meter(
    frame: &mut RenderFrame,
    x: f32,
    y: f32,
    width: f32,
    name: &str,
    fraction: f32,
    color: RenderColor,
) {
    label(
        frame,
        (x, y),
        &format!("{name} {:.0}%", fraction.clamp(0.0, 1.0) * 100.0),
        LIGHT,
        12.0,
        false,
        width,
    );
    bar(frame, x, y + 17.0, width, fraction, color);
}

fn bar(frame: &mut RenderFrame, x: f32, y: f32, width: f32, fraction: f32, color: RenderColor) {
    rect(
        frame,
        Viewport::with_origin(x - 1.0, y - 1.0, width + 2.0, 6.0),
        SHADOW,
    );
    rect(
        frame,
        Viewport::with_origin(x, y, width, 4.0),
        RenderColor::rgb(0.18, 0.22, 0.27),
    );
    if fraction > 0.0 {
        rect(
            frame,
            Viewport::with_origin(x, y, width * fraction.clamp(0.0, 1.0), 4.0),
            color,
        );
    }
}

fn rect(frame: &mut RenderFrame, r: Viewport, color: RenderColor) {
    frame.push_primitive(
        15,
        RenderPrimitive::Polygon(RenderPolygon {
            points: [
                (r.x, r.y),
                (r.x + r.width, r.y),
                (r.x + r.width, r.y + r.height),
                (r.x, r.y + r.height),
            ]
            .map(|(x, y)| RenderPoint::new(x, -y))
            .to_vec(),
            fill: Some(Fill::new(color)),
            stroke: None,
        }),
    );
}

fn label(
    frame: &mut RenderFrame,
    (x, y): (f32, f32),
    text: &str,
    color: RenderColor,
    size: f32,
    centered: bool,
    width: f32,
) {
    if text.is_empty() {
        return;
    }
    let size = size.min(width / (text.chars().count() as f32 * 0.63).max(1.0));
    // Two tiny glyph draws, no blurred panels or full-screen shadow pass.
    for (offset, color) in [(1.0, SHADOW), (0.0, color)] {
        frame.push_primitive(
            20,
            RenderPrimitive::Text(RenderText {
                position: RenderPoint::new(x + offset, -y - offset),
                text: text.into(),
                color,
                size,
                anchor: if centered {
                    TextAnchor::Center
                } else {
                    TextAnchor::TopLeft
                },
            }),
        );
    }
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let line = lines.last_mut().unwrap();
        if !line.is_empty() && line.chars().count() + word.chars().count() + 1 > width {
            lines.push(word.to_owned());
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    lines
}

pub(super) fn bot_diagnostics_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("SPACEWARS_BOT_HUD").is_ok_and(|v| v == "1"))
}

pub(super) fn append_bot_diagnostic(frames: &mut [RenderFrame], player: usize, text: &str) {
    let players = frames.len() / 2;
    let Some(overlay) = frames.last_mut() else {
        return;
    };
    let width = overlay.camera.center.x * 2.0 / players as f32;
    // Separate, opt-in row below any gameplay prompt. Limit long planner labels
    // to this pane; full diagnostics remain in the status/trace endpoint.
    let mut text: String = format!("AI: {text}").chars().take(55).collect();
    if text.chars().count() == 55 {
        text.push('…');
    }
    label(
        overlay,
        (width * (player as f32 + 0.5), 144.0),
        &text,
        AMBER,
        11.0,
        true,
        width - 24.0,
    );
}

#[cfg(test)]
pub(super) fn has_bot_diagnostic(frames: &[RenderFrame], player: usize) -> bool {
    let overlay = frames.last().unwrap();
    let pane_width = overlay.camera.center.x * 2.0 / (frames.len() / 2) as f32;
    overlay.layers.iter().flat_map(|l| &l.primitives).any(|p| matches!(p, RenderPrimitive::Text(t)
        if t.text.starts_with("AI: ") && t.position.x >= player as f32 * pane_width && t.position.x < (player + 1) as f32 * pane_width))
}

#[cfg(test)]
mod tests;
