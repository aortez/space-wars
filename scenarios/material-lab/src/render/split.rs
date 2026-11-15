//! The split case: the striker, the wood and its fragment along the strike,
//! one column of readouts per block and the wood's balance from strike to strike.
use super::{AMBER, CYAN, INK, ROW, history, line, rect, text};
use crate::{MaterialLabState, Split, SplitBlock, Stage, split::STRIKER_SPEED};
use engine_common::{RenderColor as C, RenderFrame};

/// Screen position of the wood's struck face.
const FACE_X: f32 = -8.0;
/// The striker's approach and rebound are magnified like the pair schematic.
const MAGNIFICATION: f32 = 20.0;
/// The wood and the fragment are drawn to scale, this long per world unit.
const SCALE: f32 = 4.6;
/// The striker's drawn length, which is not to scale but holds its labels at 800x480.
const STRIKER_LENGTH: f32 = 5.2;
const BOTTOM: f32 = 2.0;
const HEIGHT: f32 = 2.4;
const TRACK: C = C::rgb(0.2, 0.23, 0.27);

pub fn draw(f: &mut RenderFrame, s: &MaterialLabState, split: &Split) {
    striker(f, &split.striker);
    let note = if split.stage > Stage::Split {
        "rebuilt"
    } else {
        "fixed"
    };
    wood(f, &split.wood, "B  wood", Some(note));
    // The fragment is too narrow for more than its letter; the table names it.
    if let Some(fragment) = &split.fragment {
        wood(f, fragment, "C", None);
    }
    text(
        f,
        -17.0,
        1.6,
        "Striker motion x20 · wood and fragment to scale",
        12.0,
        INK,
    );
    table(f, split);
    let rows = ledger(f, split);
    history(f, s, 4.5, 5.8 - (rows as f32 + 1.5) * ROW);
}

/// The striker, at its magnified distance from the wood's struck face.
fn striker(f: &mut RenderFrame, b: &SplitBlock) {
    let face = FACE_X + MAGNIFICATION * b.right;
    let back = face - STRIKER_LENGTH;
    rect(
        f,
        back,
        BOTTOM,
        STRIKER_LENGTH,
        HEIGHT,
        C::rgb(0.27, 0.33, 0.41),
    );
    line(f, face, BOTTOM, face, BOTTOM + HEIGHT, CYAN);
    text(
        f,
        back + 0.3,
        BOTTOM + HEIGHT - 0.2,
        "A  armor",
        14.0,
        C::WHITE,
    );
    text(
        f,
        back + 0.3,
        BOTTOM + HEIGHT - 0.95,
        "striker, mass 2",
        13.0,
        C::WHITE,
    );
    let reach = 2.5 * b.velocity / STRIKER_SPEED;
    if reach.abs() > 0.05 {
        let (y, start) = (
            BOTTOM + HEIGHT + 0.75,
            back + 0.5 * STRIKER_LENGTH - 0.5 * reach,
        );
        let tip = start + reach;
        let head = reach.signum() * reach.abs().min(0.3);
        line(f, start, y, tip, y, AMBER);
        line(f, tip - head, y + 0.2, tip, y, AMBER);
        line(f, tip - head, y - 0.2, tip, y, AMBER);
    }
}

/// A wood block from its collider's extent, with its grain and labels.
fn wood(f: &mut RenderFrame, b: &SplitBlock, label: &str, note: Option<&str>) {
    let (left, right) = (FACE_X + SCALE * b.left, FACE_X + SCALE * b.right);
    rect(
        f,
        left,
        BOTTOM,
        right - left,
        HEIGHT,
        C::rgb(0.36, 0.24, 0.13),
    );
    for i in 0..3 {
        let y = BOTTOM + 0.2 + i as f32 * 0.2;
        line(f, left + 0.1, y, right - 0.1, y, C::rgb(0.55, 0.38, 0.21));
    }
    text(f, left + 0.3, BOTTOM + HEIGHT - 0.2, label, 14.0, C::WHITE);
    if let Some(note) = note {
        text(f, left + 0.3, BOTTOM + HEIGHT - 0.95, note, 13.0, C::WHITE);
    }
}

/// Body, motion, volume and capacity, one column per block.
fn table(f: &mut RenderFrame, split: &Split) {
    const COLUMNS: [f32; 3] = [-11.6, -7.2, -2.8];
    type Readout = fn(&SplitBlock) -> String;
    // Capacity and its remainder take separate rows, so the armor's figures fit
    // a column at 800x480.
    let rows: [(&str, Readout); 5] = [
        ("Body", |b| format!("{}", b.body)),
        ("Velocity", |b| format!("{:+.3}", b.velocity)),
        ("Volume", |b| format!("{:.1}", b.volume)),
        ("Capacity", |b| format!("{:.0}", b.capacity)),
        ("Capacity left", |b| format!("{:.1}", b.remaining)),
    ];
    let blocks: Vec<_> = [
        Some(&split.striker),
        Some(&split.wood),
        split.fragment.as_ref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    for (i, b) in blocks.iter().enumerate() {
        text(
            f,
            COLUMNS[i],
            ROW,
            format!(
                "{}  {}",
                ["A", "B", "C"][i],
                ["armor", "wood", "fragment"][i]
            ),
            13.0,
            CYAN,
        );
        // Each block's remaining share of its own capacity, under its figures
        // with room for text that keeps its pixel size on a small screen.
        let y = -(rows.len() as f32) * ROW - 0.6;
        rect(f, COLUMNS[i], y, 4.0, 0.25, TRACK);
        rect(
            f,
            COLUMNS[i],
            y,
            4.0 * (b.remaining / b.capacity) as f32,
            0.25,
            CYAN,
        );
    }
    for (row, (label, value)) in rows.into_iter().enumerate() {
        let y = -(row as f32) * ROW;
        text(f, -17.0, y, label, 14.0, INK);
        for (i, b) in blocks.iter().enumerate() {
            text(f, COLUMNS[i], y, value(b), 14.0, INK);
        }
    }
}

/// Where the wood's balance went: each strike, the split and the move, as
/// they happen. Returns the number of rows drawn.
fn ledger(f: &mut RenderFrame, split: &Split) -> usize {
    const X: f32 = 4.5;
    let mut rows = vec![(
        "Undamaged".to_string(),
        format!("{:.2}", crate::split::whole().capacity()),
    )];
    if let Some(charge) = split.charges.first() {
        rows.push(("First strike".into(), format!("-{charge:.2}")));
    }
    if let (Some(before), Some([kept, piece])) = (split.before_split, split.parts) {
        rows.push(("At the split".into(), format!("{before:.2}")));
        rows.push(("  kept".into(), format!("{kept:.2}")));
        rows.push(("  fragment".into(), format!("{piece:.2}")));
        rows.push(("  kept + fragment".into(), format!("{:.2}", kept + piece)));
    }
    if let Some(moved) = split.moved {
        rows.push((
            format!("Moved to body {}", split.wood.body),
            format!("{moved:.2}"),
        ));
    }
    if let Some(charge) = split.charges.get(1) {
        rows.push(("Second strike".into(), format!("-{charge:.2}")));
    }
    rows.push(("Wood now".into(), format!("{:.2}", split.wood.remaining)));
    text(f, X, 5.8, "WOOD BALANCE", 13.0, CYAN);
    for (i, (label, value)) in rows.iter().enumerate() {
        let y = 5.0 - i as f32 * ROW;
        text(f, X, y, label, 14.0, INK);
        text(f, X + 8.0, y, value, 14.0, INK);
    }
    rows.len()
}
