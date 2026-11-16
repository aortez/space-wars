//! The shell case, drawn to scale: the shell's path through the moss to the
//! armor plate and where it burst, with its source's ledger and what each
//! recipient absorbed.
use super::{AMBER, CYAN, INK, ROW, history, line_on, rect, text};
use crate::{
    MaterialLabState, Shell,
    shell::{PATH, PAYLOAD, PLATE_CENTER, PLATE_HALF, SHELL_RADIUS, TUFT_RADIUS},
};
use engine_common::{
    RenderCircle, RenderColor as C, RenderFrame, RenderLine, RenderPoint as P,
    RenderPrimitive as R, Stroke,
};
use engine_core::Vec2;

/// Screen position of the shell's start, and screen units per world unit.
const ORIGIN_X: f32 = -16.0;
const ORIGIN_Y: f32 = -1.0;
const SCALE: f32 = 1.9;
const MOSS: C = C::rgb(0.42, 0.74, 0.34);
const CUT: C = C::rgb(0.38, 0.44, 0.5);
const STEEL: C = C::rgb(0.42, 0.47, 0.53);

fn screen(at: Vec2) -> P {
    P::new(ORIGIN_X + SCALE * at.x, ORIGIN_Y + SCALE * at.y)
}

/// The unit vector `i` steps of `count` around a turn.
fn spoke(i: usize, count: usize) -> Vec2 {
    let angle = i as f32 * std::f32::consts::TAU / count as f32;
    Vec2::new(angle.cos(), angle.sin())
}

/// A filled circle of `radius` world units around `center`.
fn disc(f: &mut RenderFrame, center: Vec2, radius: f32, color: C) {
    f.push_primitive(
        3,
        R::Circle(RenderCircle::filled(screen(center), SCALE * radius, color)),
    );
}

/// A circle's outline, of `radius` world units around `center`.
fn ring(f: &mut RenderFrame, center: Vec2, radius: f32, color: C) {
    f.push_primitive(
        3,
        R::Circle(RenderCircle {
            center: screen(center),
            radius: SCALE * radius,
            fill: None,
            stroke: Some(Stroke::new(color, 1.5)),
        }),
    );
}

pub fn draw(f: &mut RenderFrame, s: &MaterialLabState, shell: &Shell) {
    let (info, color) = match shell.speed {
        Some(speed) => (
            format!("Shell speed: {speed:.1}  |  payload: {:.0}", PAYLOAD.energy),
            CYAN,
        ),
        None => ("Shell stopped  |  payload released".to_string(), AMBER),
    };
    text(f, -17.0, 5.5, info, 14.0, color);
    scene(f, shell);
    let last = table(f, shell, 5.8);
    history(f, s, 0.5, last - 2.0 * ROW - 0.2);
}

/// The shell's path, the moss, the plate, the shell and its burst.
fn scene(f: &mut RenderFrame, shell: &Shell) {
    // The path passes behind the plate's fill.
    if let Some(end) = shell.position.or(shell.struck_at) {
        let (a, b) = (screen(Vec2::ZERO), screen(end));
        line_on(f, 1, a.x, a.y, b.x, b.y, CUT);
    }
    let corner = screen(PLATE_CENTER - PLATE_HALF);
    rect(
        f,
        corner.x,
        corner.y,
        2.0 * SCALE * PLATE_HALF.x,
        2.0 * SCALE * PLATE_HALF.y,
        STEEL,
    );
    let top = corner.y + 2.0 * SCALE * PLATE_HALF.y;
    text(f, corner.x + 0.3, top - 0.2, "A", 14.0, C::WHITE);
    for (i, tuft) in shell.tufts.iter().enumerate() {
        if tuft.cut {
            ring(f, tuft.center, TUFT_RADIUS, CUT);
        } else {
            disc(f, tuft.center, TUFT_RADIUS, MOSS);
        }
        // Labels hang below their anchor, so under each tuft they stay clear
        // of the shell at 800x480.
        let at = screen(tuft.center);
        text(
            f,
            at.x - 0.35,
            at.y - 0.6,
            format!("M{}", i + 1),
            12.0,
            if tuft.cut { CUT } else { C::WHITE },
        );
    }
    if let Some(position) = shell.position {
        disc(f, position, SHELL_RADIUS, CYAN);
    }
    if let Some(center) = shell.burst {
        for i in 0..8 {
            let direction = spoke(i, 8);
            let (a, b) = (
                screen(center + direction * 0.15),
                screen(center + direction * 0.55),
            );
            f.push_primitive(4, R::Line(RenderLine::new(a, b, Stroke::new(AMBER, 2.0))));
        }
    }
    text(
        f,
        -17.0,
        -5.6,
        "A armor plate, fixed · M moss · to scale",
        12.0,
        INK,
    );
    text(
        f,
        -17.0,
        -6.4,
        format!(
            "Contact response: {} refused, the shell is unbound",
            shell.refused
        ),
        12.0,
        INK,
    );
}

/// The source's ledger by part, then what each recipient absorbed, under a
/// heading from `top`. Returns the last row's height.
fn table(f: &mut RenderFrame, shell: &Shell, top: f32) -> f32 {
    text(f, 0.5, top, "SOURCE LEDGER", 13.0, CYAN);
    text(f, 6.5, top, "Kinetic", 13.0, CYAN);
    text(f, 11.0, top, "Payload", 13.0, CYAN);
    let (kinetic, payload) = (shell.ledger.kinetic, shell.ledger.payload);
    let parts = [
        ("Budget", kinetic.budget, payload.budget),
        ("Delivered", kinetic.delivered, payload.delivered),
        ("Absorbed", kinetic.absorbed, payload.absorbed),
        ("Excess", kinetic.excess, payload.excess),
    ];
    let row = |i: usize| top - (i + 1) as f32 * ROW;
    for (i, (label, k, p)) in parts.iter().enumerate() {
        text(f, 0.5, row(i), *label, 14.0, INK);
        text(f, 6.5, row(i), format!("{k:.2}"), 14.0, INK);
        text(f, 11.0, row(i), format!("{p:.2}"), 14.0, INK);
    }
    let heading = row(parts.len()) - 0.25;
    text(f, 0.5, heading, "WORK ABSORBED", 13.0, CYAN);
    let path = &shell.tufts[..PATH];
    // An empty `f32` sum is -0, which would print as "-0.000".
    let passes = path
        .iter()
        .filter_map(|tuft| tuft.pass)
        .fold(0.0, |sum, work| sum + work);
    let cut = path.iter().filter(|tuft| tuft.cut).count();
    let open = shell.tufts[PATH];
    let behind = shell.tufts[PATH + 1];
    let dash = || "—".to_string();
    let burst = shell.burst.is_some();
    let rows = [
        ("Path moss M1-3", format!("{passes:.3}  ·  {cut} cut")),
        (
            "Plate, hit",
            shell
                .hit
                .map_or_else(dash, |hit| format!("{:.2}", hit.consumed[1])),
        ),
        (
            "Plate, blast",
            shell
                .plate_blast
                .map_or_else(dash, |work| format!("{work:.2}")),
        ),
        (
            "Open moss M4",
            open.blast.map_or_else(dash, |work| {
                format!("{work:.3}{}", if open.cut { "  ·  cut" } else { "" })
            }),
        ),
        (
            "Moss M5",
            match behind.blast {
                Some(work) => format!("{work:.3}"),
                None if burst => "none  ·  shadowed".into(),
                None => dash(),
            },
        ),
        (
            "Plate left",
            format!("{:.1} / {:.0}", shell.plate_remaining, shell.plate_capacity),
        ),
    ];
    let row = |i: usize| heading - (i + 1) as f32 * ROW;
    for (i, (label, value)) in rows.iter().enumerate() {
        text(f, 0.5, row(i), *label, 14.0, INK);
        text(f, 6.5, row(i), value, 14.0, INK);
    }
    row(rows.len() - 1)
}
