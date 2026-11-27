use crate::{Case, MaterialLabState};
use engine_common::{
    Camera2, RenderColor as C, RenderFrame, RenderLine, RenderPoint as P, RenderPolygon,
    RenderPrimitive as R, RenderText, Stroke,
};
use engine_rapier::world::loading::response::dynamic_compression::DynamicCompressionSummary;
mod branch;
mod pair;
mod shell;
mod split;
const INK: C = C::rgb(0.78, 0.86, 0.92);
const CYAN: C = C::rgb(0.22, 0.82, 0.92);
const AMBER: C = C::rgb(1.0, 0.66, 0.24);
/// Line pitch of the readouts in every layout.
const ROW: f32 = 0.75;
pub fn text(f: &mut RenderFrame, x: f32, y: f32, value: impl Into<String>, size: f32, color: C) {
    let mut t = RenderText::new(P::new(x, y), value);
    t.size = size;
    t.color = color;
    f.push_primitive(10, R::Text(t));
}
fn rect(f: &mut RenderFrame, x: f32, y: f32, w: f32, h: f32, c: C) {
    f.push_primitive(
        1,
        R::Polygon(RenderPolygon::filled(
            vec![
                P::new(x, y),
                P::new(x + w, y),
                P::new(x + w, y + h),
                P::new(x, y + h),
            ],
            c,
        )),
    );
}
fn line(f: &mut RenderFrame, x: f32, y: f32, xx: f32, yy: f32, c: C) {
    line_on(f, 2, x, y, xx, yy, c);
}
/// A line on layer `z`. On the fills' layer it passes behind fills pushed after it.
fn line_on(f: &mut RenderFrame, z: i32, x: f32, y: f32, xx: f32, yy: f32, c: C) {
    f.push_primitive(
        z,
        R::Line(RenderLine::new(
            P::new(x, y),
            P::new(xx, yy),
            Stroke::new(c, 1.5),
        )),
    );
}
pub fn frame(s: &MaterialLabState) -> RenderFrame {
    let mut f = RenderFrame::new(Camera2::new(P::new(0.0, 0.0), 24.0));
    let a = s.summary();
    let pair = s.pair();
    text(
        &mut f,
        -17.0,
        10.3,
        if s.case == Case::ShellThroughMoss {
            "MATERIAL LAB  /  WEAPON SOURCE"
        } else if s.case == Case::BranchBends {
            "MATERIAL LAB  /  BENDING CONNECTION"
        } else if s.case == Case::BranchFailure {
            "MATERIAL LAB  /  CONNECTION FAILURE"
        } else if s.case == Case::RegionSplit {
            "MATERIAL LAB  /  REGION SPLIT"
        } else if s.case == Case::PairFailure {
            "MATERIAL LAB  /  PAIRED COLLAPSE"
        } else if pair.is_some() {
            "MATERIAL LAB  /  PAIRED COLLISION"
        } else if s.case == Case::SupportFailure {
            "MATERIAL LAB  /  SUPPORT FAILURE"
        } else {
            "MATERIAL LAB  /  LOAD & RECOVERY"
        },
        22.0,
        CYAN,
    );
    text(&mut f, -17.0, 8.8, s.case.name(), 17.0, INK);
    text(
        &mut f,
        -17.0,
        7.6,
        format!(
            "{}  |  {}  |  t = {:.5} s",
            if s.paused { "Paused" } else { "Slow motion" },
            s.phase().label(),
            s.elapsed()
        ),
        15.0,
        INK,
    );
    if let Some(shell) = &s.shell() {
        shell::draw(&mut f, s, shell);
    } else if let Some(branch) = &s.branch() {
        branch::draw(&mut f, s, branch);
    } else if let Some(split) = &s.split() {
        split::draw(&mut f, s, split);
    } else if let Some(pair) = &pair {
        pair::draw(&mut f, s, pair);
    } else {
        platen(&mut f, s, &a);
    }
    f
}
/// A weighted rigid platen over the fixed wood layer, with one material's readouts.
fn platen(f: &mut RenderFrame, s: &MaterialLabState, a: &DynamicCompressionSummary) {
    let platen = 1.0 - a.indentation as f32 * 20.0;
    let permanent = 1.0 - a.plastic_indentation as f32 * 20.0;
    let failure = s.failure();
    let surface = failure
        .as_ref()
        .map_or(permanent.min(platen), |e| 1.0 - e.indentation as f32 * 20.0);
    // High enough that the longest arrow, over the highest platen, stays clear
    // of this text at 800x480.
    text(
        f,
        -14.0,
        5.5,
        if s.case == Case::SupportFailure {
            format!("Fixed load: {:.0}", s.weight())
        } else {
            format!(
                "Load: {:.0}  |  changes: {}/{}",
                s.weight(),
                s.load_changes.len(),
                crate::MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES
            )
        },
        14.0,
        if s.weight() == 0.0 { AMBER } else { CYAN },
    );
    if s.weight() > 0.0 {
        let tip = platen + 1.5;
        line(f, -8.5, tip + 0.5 + s.weight() / 8000.0, -8.5, tip, AMBER);
        line(f, -8.8, tip + 0.3, -8.5, tip, AMBER);
        line(f, -8.2, tip + 0.3, -8.5, tip, AMBER);
    }
    rect(
        f,
        -14.0,
        -4.2,
        11.0,
        surface + 4.2,
        if failure.is_some() {
            C::rgb(0.19, 0.17, 0.15)
        } else {
            C::rgb(0.36, 0.24, 0.13)
        },
    );
    // The grain and both reference levels pass behind the platen, so none of
    // them crosses its label; the levels stay in view past its ends.
    for i in 0..8 {
        let x = -13.7 + i as f32 * 1.4;
        line_on(f, 1, x, -4.0, x, surface - 0.1, C::rgb(0.55, 0.38, 0.21));
    }
    line_on(f, 1, -15.0, 1.0, -2.0, 1.0, C::rgb(0.38, 0.44, 0.5));
    line_on(f, 1, -14.5, permanent, -2.5, permanent, AMBER);
    rect(f, -14.0, platen, 11.0, 1.3, C::rgb(0.16, 0.55, 0.69));
    line(f, -14.0, platen, -3.0, platen, CYAN);
    text(f, -13.6, platen + 1.1, "20 mass units", 14.0, C::WHITE);
    text(
        f,
        -14.0,
        -5.2,
        if failure.is_some() {
            "Collapsed wood / no support"
        } else {
            "Wood layer / fixed support"
        },
        14.0,
        INK,
    );
    text(f, -14.0, -6.1, "Displacement x20 · schematic", 13.0, INK);
    text(
        f,
        -14.0,
        -6.9,
        "Grey: original surface   Amber: permanent set",
        12.0,
        AMBER,
    );
    let mut rows = vec![
        ("Normal speed", format!("{:.3}", a.velocity)),
        ("Compression", format!("{:.5}", a.indentation.max(0.0))),
        ("Permanent set", format!("{:.5}", a.plastic_indentation)),
        (
            "Capacity left",
            format!("{:.2} / {:.0}", s.remaining(), s.capacity()),
        ),
        ("Damage work", format!("{:.3}", a.damage_work)),
        ("Elastic energy", format!("{:.3}", a.elastic_energy)),
        ("Motion energy", format!("{:.3}", a.kinetic_energy)),
        ("Viscous loss", format!("{:.3}", a.viscous_loss)),
        ("Gravity work", format!("{:.3}", a.gravity_work)),
        ("Gravity impulse", format!("{:.3}", s.gravity_impulse())),
    ];
    if let Some(e) = &failure {
        rows.push(("Collapse loss", format!("{:.3}", e.dissipated())));
    }
    // The value column clears the longest label at 800x480, where text is
    // largest against the scene.
    for (i, (label, value)) in rows.iter().enumerate() {
        let y = 5.8 - i as f32 * ROW;
        text(f, 0.5, y, *label, 14.0, INK);
        text(f, 6.5, y, value, 14.0, INK);
    }
    // The bar takes the next row, which keeps it clear of the labels'
    // descenders at 800x480.
    let bar_y = 5.8 - (rows.len() + 1) as f32 * ROW;
    rect(f, 0.5, bar_y, 14.5, 0.25, C::rgb(0.2, 0.23, 0.27));
    rect(
        f,
        0.5,
        bar_y,
        14.5 * (s.remaining() / s.capacity()) as f32,
        0.25,
        CYAN,
    );
    history(f, s, 0.5, bar_y - 0.7);
}
/// The latest contact observations, and why a finished run stopped. The
/// branch and shell cases have no contacts to number: the branch's joint
/// severs, and the shell's source reports what it reached.
fn history(f: &mut RenderFrame, s: &MaterialLabState, x: f32, y: f32) {
    let unnumbered = s.case.is_branch() || s.case == Case::ShellThroughMoss;
    text(
        f,
        x,
        y,
        if unnumbered {
            "HISTORY"
        } else {
            "CONTACT HISTORY"
        },
        13.0,
        CYAN,
    );
    for (i, e) in s.events.iter().rev().take(4).enumerate() {
        text(
            f,
            x,
            y - 0.8 - i as f32 * 0.7,
            if unnumbered {
                format!("{:.4}s  {}", e.time, e.label)
            } else {
                format!("{:.4}s  {}  #{}", e.time, e.label, e.episode)
            },
            13.0,
            INK,
        );
    }
    if let Some(reason) = &s.stopped {
        text(f, -17.0, 6.5, reason, 13.0, AMBER);
    } else if let Some(hint) = s.guidance() {
        text(f, -17.0, 6.5, hint, 13.0, AMBER);
    }
}
/// Client controls are presentation only and never enter a physics snapshot.
pub fn controls(f: &mut RenderFrame, selected: &str, status: &str) {
    text(
        f,
        -17.0,
        -8.3,
        format!("CONTROL: < {selected} >"),
        15.0,
        CYAN,
    );
    text(
        f,
        -17.0,
        -9.3,
        "Up/Down: choose · Space / gamepad bottom: activate · Esc/Start: host pause",
        12.0,
        INK,
    );
    text(f, -17.0, -10.4, status, 12.0, INK);
}
