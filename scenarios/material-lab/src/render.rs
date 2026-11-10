use crate::{Case, MaterialLabState};
use engine_common::{
    Camera2, RenderColor as C, RenderFrame, RenderLine, RenderPoint as P, RenderPolygon,
    RenderPrimitive as R, RenderText, Stroke,
};
use engine_rapier::world::loading::response::dynamic_compression::DynamicCompressionSummary;
mod pair;
const INK: C = C::rgb(0.78, 0.86, 0.92);
const CYAN: C = C::rgb(0.22, 0.82, 0.92);
const AMBER: C = C::rgb(1.0, 0.66, 0.24);
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
    f.push_primitive(
        2,
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
        if pair.is_some() {
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
            a.elapsed
        ),
        15.0,
        INK,
    );
    if let Some(pair) = &pair {
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
    text(
        f,
        -14.0,
        5.2,
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
    for i in 0..8 {
        let x = -13.7 + i as f32 * 1.4;
        line(f, x, -4.0, x, surface - 0.1, C::rgb(0.55, 0.38, 0.21));
    }
    line(f, -15.0, 1.0, -2.0, 1.0, C::rgb(0.38, 0.44, 0.5));
    line(f, -14.5, permanent, -2.5, permanent, AMBER);
    rect(f, -14.0, platen, 11.0, 1.3, C::rgb(0.16, 0.55, 0.69));
    line(f, -14.0, platen, -3.0, platen, CYAN);
    text(f, -13.6, platen + 0.9, "20 mass units", 14.0, C::WHITE);
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
    let mut lines = vec![
        format!("Normal speed    {:.3}", a.velocity),
        format!("Compression     {:.5}", a.indentation.max(0.0)),
        format!("Permanent set   {:.5}", a.plastic_indentation),
        format!("Capacity left   {:.2} / {:.0}", s.remaining(), s.capacity()),
        format!("Damage work     {:.3}", a.damage_work),
        format!("Elastic energy  {:.3}", a.elastic_energy),
        format!("Motion energy   {:.3}", a.kinetic_energy),
        format!("Viscous loss    {:.3}", a.viscous_loss),
        format!("Gravity work    {:.3}", a.gravity_work),
        format!("Gravity impulse  {:.3}", s.gravity_impulse()),
    ];
    if let Some(e) = &failure {
        lines.push(format!("Collapse loss   {:.3}", e.dissipated()));
    }
    for (i, l) in lines.into_iter().enumerate() {
        text(f, 0.5, 5.8 - i as f32 * 0.75, l, 14.0, INK);
    }
    let bar_y = if failure.is_some() { -2.9 } else { -2.1 };
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
/// The latest contact observations, and why a finished run stopped.
fn history(f: &mut RenderFrame, s: &MaterialLabState, x: f32, y: f32) {
    text(f, x, y, "CONTACT HISTORY", 13.0, CYAN);
    for (i, e) in s.events.iter().rev().take(4).enumerate() {
        text(
            f,
            x,
            y - 0.8 - i as f32 * 0.7,
            format!("{:.4}s  {}  #{}", e.time, e.label, e.episode),
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
