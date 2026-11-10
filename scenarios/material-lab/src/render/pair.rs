//! Two free bodies: a schematic, each body's own readouts and the shared energy ledger.
use super::{AMBER, CYAN, INK, history, line, rect, text};
use crate::{MaterialLabState, Pair, PairBody};
use engine_common::{RenderColor as C, RenderFrame};

/// Screen position of the plane where the undeformed faces first meet.
const CONTACT_X: f32 = -8.0;
/// Normal displacement is magnified like the platen schematic.
const MAGNIFICATION: f32 = 20.0;
const LENGTH: f32 = 4.6;
const BOTTOM: f32 = 2.0;
const HEIGHT: f32 = 2.4;
/// Background between touching fills. The raster path covers every pixel a
/// fill touches, so this leaves a visible gap there too.
const GAP: f32 = 0.15;
/// Line pitch of the readouts, as in the platen layout.
const ROW: f32 = 0.75;
const TRACK: C = C::rgb(0.2, 0.23, 0.27);
// One hue per part of the energy split, in a fixed order whose neighbors stay
// distinct on this background for color-blind readers.
const MOTION: C = C::rgb(0.224, 0.529, 0.898);
const DAMAGE: C = C::rgb(0.851, 0.349, 0.149);
const ELASTIC: C = C::rgb(0.098, 0.62, 0.439);
const VISCOUS: C = C::rgb(0.788, 0.522, 0.0);
const COLLAPSE: C = C::rgb(0.68, 0.45, 0.72);

pub fn draw(f: &mut RenderFrame, s: &MaterialLabState, pair: &Pair) {
    // Arrows are scaled to the fastest body at the start of the case.
    let fastest = s
        .case
        .free_pair()
        .unwrap()
        .iter()
        .fold(0.0_f32, |v, b| v.max(b.velocity.abs()));
    for (i, body) in pair.bodies.iter().enumerate() {
        block(f, i, body, fastest);
    }
    text(
        f,
        -17.0,
        1.6,
        "Displacement x20 · grey: original face · amber: permanent set",
        12.0,
        INK,
    );
    table(f, pair);
    ledger(f, pair);
    history(
        f,
        s,
        4.5,
        if pair.collapse_loss > 0.0 { -2.7 } else { -2.0 },
    );
}

/// One body, shortened by its own layer's deformation at the contact face.
fn block(f: &mut RenderFrame, index: usize, body: &PairBody, fastest: f32) {
    // From the contact face into the body: the incoming body lies toward -x.
    let inward = if index == 0 { -1.0 } else { 1.0 };
    let original = CONTACT_X + MAGNIFICATION * (body.position - 0.5 * inward);
    let face = original + inward * MAGNIFICATION * body.deformation as f32;
    let permanent = original + inward * MAGNIFICATION * body.permanent_set as f32;
    let back = original + inward * LENGTH;
    let left = face.min(back);
    let width = (face - back).abs();
    let wood = body.material == "wood";
    rect(
        f,
        left,
        BOTTOM,
        width,
        HEIGHT,
        if body.failed {
            C::rgb(0.19, 0.17, 0.15)
        } else if wood {
            C::rgb(0.36, 0.24, 0.13)
        } else {
            C::rgb(0.27, 0.33, 0.41)
        },
    );
    if wood {
        for i in 0..3 {
            let y = BOTTOM + 0.2 + i as f32 * 0.2;
            line(
                f,
                back - inward * 0.2,
                y,
                face + inward * 0.1,
                y,
                if body.failed {
                    C::rgb(0.32, 0.26, 0.2)
                } else {
                    C::rgb(0.55, 0.38, 0.21)
                },
            );
        }
    }
    // Drawn first: the face shows through only while elastic compression
    // holds it away from both marks.
    line(
        f,
        face,
        BOTTOM,
        face,
        BOTTOM + HEIGHT,
        if body.failed { AMBER } else { CYAN },
    );
    line(
        f,
        original,
        BOTTOM - 0.3,
        original,
        BOTTOM + HEIGHT + 0.3,
        C::rgb(0.38, 0.44, 0.5),
    );
    if body.permanent_set > 0.0 {
        line(
            f,
            permanent,
            BOTTOM - 0.15,
            permanent,
            BOTTOM + HEIGHT + 0.15,
            AMBER,
        );
    }
    // Labels sit at the far end, clear of the marks around the contact face.
    let label = if index == 0 { back + 0.3 } else { back - 3.2 };
    text(
        f,
        label,
        BOTTOM + HEIGHT - 0.2,
        format!("{}  {}", ["A", "B"][index], body.material),
        14.0,
        C::WHITE,
    );
    text(
        f,
        label,
        BOTTOM + HEIGHT - 0.95,
        format!("mass {:.0}", body.mass),
        13.0,
        C::WHITE,
    );
    let reach = 2.5 * body.velocity / fastest;
    if reach.abs() > 0.05 {
        let (y, start) = (BOTTOM + HEIGHT + 0.75, 0.5 * (face + back) - 0.5 * reach);
        let tip = start + reach;
        let head = reach.signum() * reach.abs().min(0.3);
        line(f, start, y, tip, y, AMBER);
        line(f, tip - head, y + 0.2, tip, y, AMBER);
        line(f, tip - head, y - 0.2, tip, y, AMBER);
    }
}

/// Motion, deformation, damage and capacity, one column per body.
fn table(f: &mut RenderFrame, pair: &Pair) {
    const COLUMNS: [f32; 2] = [-9.8, -4.4];
    type Readout = fn(&PairBody) -> String;
    let rows: [(&str, Readout); 9] = [
        ("Velocity", |b| format!("{:+.3}", b.velocity)),
        ("Momentum", |b| format!("{:+.2}", b.momentum)),
        ("Motion energy", |b| format!("{:.3}", b.kinetic_energy)),
        ("Deformation", |b| format!("{:.5}", b.deformation)),
        ("Permanent set", |b| format!("{:.5}", b.permanent_set)),
        ("Elastic energy", |b| format!("{:.3}", b.elastic_energy)),
        ("Damage work", |b| format!("{:.3}", b.damage_work)),
        ("Viscous loss", |b| format!("{:.3}", b.viscous_loss)),
        ("Capacity left", |b| {
            format!("{:.1} / {:.0}", b.remaining, b.capacity)
        }),
    ];
    for (i, body) in pair.bodies.iter().enumerate() {
        text(
            f,
            COLUMNS[i],
            ROW,
            format!(
                "{}  {}{}",
                ["A", "B"][i],
                body.material,
                if body.failed { " / failed" } else { "" }
            ),
            13.0,
            if body.failed { AMBER } else { CYAN },
        );
        // Each body's remaining share of its own capacity, under its figure.
        let y = -(rows.len() as f32) * ROW - 0.45;
        rect(f, COLUMNS[i], y, 4.4, 0.25, TRACK);
        rect(
            f,
            COLUMNS[i],
            y,
            4.4 * (body.remaining / body.capacity) as f32,
            0.25,
            CYAN,
        );
    }
    for (row, (label, value)) in rows.into_iter().enumerate() {
        let y = -(row as f32) * ROW;
        text(f, -17.0, y, label, 14.0, INK);
        for (i, body) in pair.bodies.iter().enumerate() {
            text(f, COLUMNS[i], y, value(body), 14.0, INK);
        }
    }
}

/// Where the pair's initial motion energy is now: one bar and its figures.
fn ledger(f: &mut RenderFrame, pair: &Pair) {
    const X: f32 = 4.5;
    const WIDTH: f32 = 12.5;
    let mut parts = vec![
        ("Motion", MOTION, pair.total(|b| b.kinetic_energy)),
        ("Damage work", DAMAGE, pair.total(|b| b.damage_work)),
        ("Elastic energy", ELASTIC, pair.total(|b| b.elastic_energy)),
        ("Viscous loss", VISCOUS, pair.total(|b| b.viscous_loss)),
    ];
    if pair.collapse_loss > 0.0 {
        parts.push(("Collapse loss", COLLAPSE, pair.collapse_loss));
    }
    text(f, X, 5.8, "SHARED ENERGY", 13.0, CYAN);
    let mut left = X;
    for (i, &(label, color, energy)) in parts.iter().enumerate() {
        let width = WIDTH * (energy / pair.initial_energy) as f32;
        if width > GAP {
            rect(f, left + 0.5 * GAP, 4.65, width - GAP, 0.3, color);
        }
        left += width;
        let y = 4.2 - i as f32 * ROW;
        rect(f, X, y - 0.45, 0.3, 0.3, color);
        text(f, X + 0.6, y, label, 14.0, INK);
        text(f, X + 7.3, y, format!("{energy:.3}"), 14.0, INK);
    }
    let closing = [
        ("Numerical defect", format!("{:.1e}", pair.numerical_defect)),
        ("Initial motion", format!("{:.3}", pair.initial_energy)),
        (
            "Momentum A + B",
            format!("{:+.2}", pair.total(|b| b.momentum)),
        ),
    ];
    for (i, (label, value)) in closing.into_iter().enumerate() {
        let y = 4.2 - (parts.len() + i) as f32 * ROW;
        text(f, X + 0.6, y, label, 14.0, INK);
        text(f, X + 7.3, y, value, 14.0, INK);
    }
}
