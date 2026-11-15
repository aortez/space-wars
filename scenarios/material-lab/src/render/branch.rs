//! The branch case, drawn to scale: the trunk, the branch and the weight on
//! it, with the weld's load against its safe limit and its capacity.
use super::{AMBER, CYAN, INK, ROW, history, line, rect, text};
use crate::{
    Branch, MaterialLabState, Placement,
    branch::{BRANCH_HALF, TRUNK_CENTER, TRUNK_HALF, WEIGHT_HALF},
};
use engine_common::{
    RenderColor as C, RenderFrame, RenderLine, RenderPoint as P, RenderPolygon,
    RenderPrimitive as R, Stroke,
};
use engine_core::Vec2;

/// Screen position of the weld's root, and screen units per world unit.
const ROOT_X: f32 = -14.0;
const ROOT_Y: f32 = -0.5;
const SCALE: f32 = 2.5;
const WOOD_FILL: C = C::rgb(0.36, 0.24, 0.13);
const GRAIN: C = C::rgb(0.55, 0.38, 0.21);
const TRACK: C = C::rgb(0.2, 0.23, 0.27);
/// The moment bar spans this multiple of the safe moment.
const MOMENT_RANGE: f32 = 1.5;

fn screen(x: f32, y: f32) -> P {
    P::new(ROOT_X + SCALE * x, ROOT_Y + SCALE * y)
}

/// A box at its body's pose.
fn body(f: &mut RenderFrame, at: &Placement, half: Vec2, color: C) {
    let (sin, cos) = at.angle.sin_cos();
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .map(|(sx, sy)| {
            let (dx, dy) = (sx * half.x, sy * half.y);
            screen(at.x + cos * dx - sin * dy, at.y + sin * dx + cos * dy)
        })
        .to_vec();
    f.push_primitive(1, R::Polygon(RenderPolygon::filled(corners, color)));
}

pub fn draw(f: &mut RenderFrame, s: &MaterialLabState, b: &Branch) {
    text(
        f,
        -17.0,
        5.5,
        format!(
            "Tip load: {:.0}  |  changes: {}/{}",
            b.tip_load,
            s.load_changes.len(),
            crate::MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES
        ),
        14.0,
        if b.tip_load == 0.0 { AMBER } else { CYAN },
    );
    scene(f, b);
    let last = table(f, b, 5.8);
    let y = last - 2.0 * ROW;
    bars(f, b, y);
    history(f, s, 0.5, y - 2.0 * ROW - 0.2);
}

/// The trunk, the weld, the branch and the weight with its load arrow.
fn scene(f: &mut RenderFrame, b: &Branch) {
    let trunk = screen(TRUNK_CENTER.x - TRUNK_HALF.x, TRUNK_CENTER.y - TRUNK_HALF.y);
    rect(
        f,
        trunk.x,
        trunk.y,
        2.0 * SCALE * TRUNK_HALF.x,
        2.0 * SCALE * TRUNK_HALF.y,
        WOOD_FILL,
    );
    for i in 0..3 {
        let x = trunk.x + 0.6 + i as f32 * 0.65;
        line(
            f,
            x,
            trunk.y + 0.1,
            x,
            trunk.y + 2.0 * SCALE * TRUNK_HALF.y - 0.1,
            GRAIN,
        );
    }
    let top = trunk.y + 2.0 * SCALE * TRUNK_HALF.y;
    text(f, trunk.x + 0.3, top - 0.2, "A", 14.0, C::WHITE);
    body(f, &b.branch, BRANCH_HALF, WOOD_FILL);
    body(f, &b.weight, WEIGHT_HALF, C::rgb(0.16, 0.55, 0.69));
    let branch = screen(b.branch.x, b.branch.y);
    let weight = screen(b.weight.x, b.weight.y);
    // Labels follow each body; neither turns noticeably while it falls.
    text(
        f,
        branch.x - SCALE * BRANCH_HALF.x + 0.3,
        branch.y + 0.35,
        "B  wood branch, mass 20",
        13.0,
        C::WHITE,
    );
    text(
        f,
        weight.x - SCALE * WEIGHT_HALF.x + 0.3,
        weight.y + SCALE * WEIGHT_HALF.y - 0.2,
        "C",
        14.0,
        C::WHITE,
    );
    if b.tip_load > 0.0 {
        let tip = weight.y + SCALE * WEIGHT_HALF.y + 0.25;
        line(
            f,
            weight.x,
            tip + 0.5 + b.tip_load / 8000.0,
            weight.x,
            tip,
            AMBER,
        );
        line(f, weight.x - 0.3, tip + 0.3, weight.x, tip, AMBER);
        line(f, weight.x + 0.3, tip + 0.3, weight.x, tip, AMBER);
    }
    // The weld across the root section: cyan while it holds, amber while an
    // overload drains it, and grey where it was once it has severed.
    let color = if b.severed_at.is_some() {
        C::rgb(0.38, 0.44, 0.5)
    } else if b.load.utilization > 1.0 {
        AMBER
    } else {
        CYAN
    };
    let half = SCALE * BRANCH_HALF.y;
    f.push_primitive(
        3,
        R::Line(RenderLine::new(
            P::new(ROOT_X, ROOT_Y - half),
            P::new(ROOT_X, ROOT_Y + half),
            Stroke::new(color, 4.0),
        )),
    );
    text(
        f,
        -17.0,
        trunk.y - 0.3,
        "A fixed trunk · C weight, mass 20 · to scale",
        12.0,
        INK,
    );
}

/// The weld's load and capacity under a heading from `top`. Once the weld
/// has severed, the load is the one it severed under. Returns the last row's
/// height.
fn table(f: &mut RenderFrame, b: &Branch, top: f32) -> f32 {
    text(
        f,
        0.5,
        top,
        if b.severed_at.is_some() {
            "SEVERED WELD  /  LOAD AT SEVERING"
        } else {
            "WELD AT THE ROOT"
        },
        13.0,
        CYAN,
    );
    let rows = [
        ("Root moment", format!("{:.1}", b.load.moment.abs())),
        ("Safe moment", format!("{:.0}", b.safe_moment)),
        ("Utilization", format!("{:.3}", b.load.utilization)),
        ("Shear", format!("{:.1}", b.load.shear.abs())),
        (
            "Capacity left",
            format!("{:.2} / {:.0}", b.remaining, b.capacity),
        ),
        ("Work charged", format!("{:.2}", b.spent)),
        ("Charge per step", format!("{:.3}", b.load.charged)),
        (
            "Time to failure",
            match (b.severed_at, b.time_to_failure) {
                (Some(_), _) => "severed".into(),
                (None, Some(t)) => format!("{t:.5} s"),
                (None, None) => "never at this load".into(),
            },
        ),
        ("Branch drop", format!("{:.4}", b.drop.max(0.0))),
    ];
    let row = |i: usize| top - (i + 1) as f32 * ROW;
    for (i, (label, value)) in rows.iter().enumerate() {
        text(f, 0.5, row(i), *label, 14.0, INK);
        text(f, 6.5, row(i), value, 14.0, INK);
    }
    row(rows.len() - 1)
}

/// The root moment against the safe moment, and the capacity left.
fn bars(f: &mut RenderFrame, b: &Branch, y: f32) {
    const X: f32 = 6.5;
    const WIDTH: f32 = 8.5;
    let share = (b.load.moment.abs() / (MOMENT_RANGE * b.safe_moment)).min(1.0);
    text(f, 0.5, y + 0.45, "Moment / safe", 12.0, INK);
    rect(f, X, y, WIDTH, 0.25, TRACK);
    rect(
        f,
        X,
        y,
        WIDTH * share,
        0.25,
        if b.load.utilization > 1.0 {
            AMBER
        } else {
            CYAN
        },
    );
    let limit = X + WIDTH / MOMENT_RANGE;
    line(f, limit, y - 0.15, limit, y + 0.4, C::WHITE);
    let y = y - ROW;
    text(f, 0.5, y + 0.45, "Capacity left", 12.0, INK);
    rect(f, X, y, WIDTH, 0.25, TRACK);
    rect(
        f,
        X,
        y,
        WIDTH * (b.remaining / b.capacity) as f32,
        0.25,
        CYAN,
    );
}
