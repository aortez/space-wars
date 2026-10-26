use crate::MaterialLabState;
use engine_common::{
    Camera2, RenderColor as C, RenderFrame, RenderLine, RenderPoint as P, RenderPolygon,
    RenderPrimitive as R, RenderText, Stroke,
};
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
    text(
        &mut f,
        -17.0,
        10.3,
        "MATERIAL LAB  /  FALLING PLATEN",
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
    let platen = 1.0 - a.indentation as f32 * 20.0;
    let permanent = 1.0 - a.plastic_indentation as f32 * 20.0;
    let surface = permanent.min(platen);
    rect(
        &mut f,
        -14.0,
        -4.2,
        11.0,
        surface + 4.2,
        C::rgb(0.36, 0.24, 0.13),
    );
    for i in 0..8 {
        let x = -13.7 + i as f32 * 1.4;
        line(&mut f, x, -4.0, x, surface - 0.1, C::rgb(0.55, 0.38, 0.21));
    }
    line(&mut f, -15.0, 1.0, -2.0, 1.0, C::rgb(0.38, 0.44, 0.5));
    line(&mut f, -14.5, permanent, -2.5, permanent, AMBER);
    rect(&mut f, -14.0, platen, 11.0, 1.3, C::rgb(0.16, 0.55, 0.69));
    line(&mut f, -14.0, platen, -3.0, platen, CYAN);
    text(&mut f, -13.6, platen + 0.9, "20 mass units", 14.0, C::WHITE);
    text(&mut f, -14.0, -5.2, "Wood layer / fixed support", 14.0, INK);
    text(
        &mut f,
        -14.0,
        -6.1,
        "Displacement x20 · schematic",
        13.0,
        INK,
    );
    text(
        &mut f,
        -14.0,
        -6.9,
        "Grey: original surface   Amber: permanent set",
        12.0,
        AMBER,
    );
    let lines = [
        format!("Normal speed    {:.3}", a.velocity),
        format!("Compression     {:.5}", a.indentation.max(0.0)),
        format!("Permanent set   {:.5}", a.plastic_indentation),
        format!("Capacity left   {:.2} / 1200", s.remaining()),
        format!("Damage work     {:.3}", a.damage_work),
        format!("Elastic energy  {:.3}", a.elastic_energy),
        format!("Motion energy   {:.3}", a.kinetic_energy),
        format!("Viscous loss    {:.3}", a.viscous_loss),
    ];
    for (i, l) in lines.into_iter().enumerate() {
        text(&mut f, 0.5, 5.8 - i as f32 * 0.75, l, 14.0, INK);
    }
    rect(&mut f, 0.5, -0.9, 14.5, 0.25, C::rgb(0.2, 0.23, 0.27));
    rect(
        &mut f,
        0.5,
        -0.9,
        14.5 * (s.remaining() / 1200.0) as f32,
        0.25,
        CYAN,
    );
    text(&mut f, 0.5, -2.0, "CONTACT HISTORY", 13.0, CYAN);
    for (i, e) in s.events.iter().rev().take(4).enumerate() {
        text(
            &mut f,
            0.5,
            -3.0 - i as f32 * 0.8,
            format!("{:.4}s  {}  #{}", e.time, e.label, e.episode),
            13.0,
            INK,
        );
    }
    if let Some(reason) = &s.stopped {
        text(&mut f, -17.0, 6.5, reason, 13.0, AMBER);
    }
    f
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
