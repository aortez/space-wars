//! The reels held to "Returning digit cells to their places": cells travel
//! through their slot rather than being faded into it, the dim anchors stay put
//! behind them, and the whole spectacle stays inside a bounded primitive count
//! because it runs on a Pi.

use super::*;
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState,
    events::slot_machine::SLOT_MACHINE_TICKS,
};
use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
use std::time::Duration;

const ASPECT: f32 = 800.0 / 480.0;
/// Measured worst case is 1028 across wide, narrow and portrait, so this holds
/// the reels near where they are. Digit Slide's comparable transition documents
/// fewer than 300, and the difference is honest: it moves only the slots whose
/// digit changed, one step, while every reel here turns continuously with two
/// glyph frames in its window. The clock's own frame ceiling is 1800.
const PRIMITIVE_BUDGET: usize = 1100;

fn spinning(aspect: f32) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        7,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(ClockReading::new(10, 42, 0).expect("a valid reading")),
            ClockAction::preview_event(ClockEventKind::SlotMachine),
        ],
        Duration::ZERO,
    );
    state
}

fn advance(state: &mut ClockState) {
    ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
}

/// Filled squares about a cell wide on one layer, as (centre, alpha). The band
/// is generous because a travelling cell is drawn as an edge rect and a fill
/// rect at `half +/- stroke_half`, so neither is exactly a cell across, while an
/// anchor and a paid-out cell are. Vertical clipping does not affect the width
/// this measures, so a half-visible cell still counts.
fn cells(frame: &RenderFrame, z: i32, side: f32) -> Vec<(Vec2, f32)> {
    frame
        .layers
        .iter()
        .filter(|layer| layer.z == z)
        .flat_map(|layer| &layer.primitives)
        .filter_map(|primitive| {
            let RenderPrimitive::Polygon(polygon) = primitive else {
                return None;
            };
            if polygon.points.len() != 4 {
                return None;
            }
            let fill = polygon.fill.as_ref()?;
            let corners: Vec<Vec2> = polygon.points.iter().map(|p| Vec2::new(p.x, p.y)).collect();
            let measured = (corners[1] - corners[0]).length();
            if (measured - side).abs() > side * 0.12 {
                return None;
            }
            Some((
                corners.iter().fold(Vec2::ZERO, |sum, p| sum + *p) * 0.25,
                fill.color.a,
            ))
        })
        .collect()
}

/// A reel's cells are seen between rows, not only at rest. Travelling through a
/// slot is the whole point: a reading that only ever appeared on the grid would
/// be a dissolve dressed up as a spin.
#[test]
fn slot_render_contract_the_reels_are_seen_between_the_grid_rows() {
    let mut state = spinning(ASPECT);
    let layout = Layout::new(ASPECT);
    let side = layout.pitch * 0.8;
    let rows: Vec<f32> = (0..9)
        .map(|row| layout.face_origin.y + (row as f32 + 0.5) * layout.pitch)
        .collect();
    let mut travelling = 0;
    let mut resting = 0;
    for _ in 0..SLOT_MACHINE_TICKS {
        advance(&mut state);
        for (centre, _) in cells(&render_frame(&state), ACTIVE_CELL_LAYER, side) {
            let nearest = rows
                .iter()
                .map(|row| (row - centre.y).abs())
                .fold(f32::MAX, f32::min);
            if nearest > layout.pitch * 0.1 {
                travelling += 1;
            } else {
                resting += 1;
            }
        }
    }
    assert!(
        travelling > resting,
        "the reels spend their time in motion: {travelling} travelling against {resting} at rest"
    );
}

/// The dim anchors are the substrate. They are drawn for every slot and never
/// move, exactly as a fall or an explosion leaves them.
#[test]
fn slot_render_contract_the_anchors_never_move() {
    let mut state = spinning(ASPECT);
    let layout = Layout::new(ASPECT);
    let side = layout.pitch * 0.8;
    let first = cells(&render_frame(&state), INACTIVE_CELL_LAYER, side);
    assert!(
        first.len() >= 4 * 9,
        "every slot shows its anchors, not {} cells",
        first.len()
    );
    for tick in 0..SLOT_MACHINE_TICKS {
        advance(&mut state);
        if state.event_kind().is_none() {
            // The last tick completes the event and hands the slots back, from
            // which point the ordinary face owns its own anchors.
            break;
        }
        let anchors = cells(&render_frame(&state), INACTIVE_CELL_LAYER, side);
        assert_eq!(
            anchors.len(),
            first.len(),
            "the anchors are all still there at tick {tick}"
        );
        for (before, after) in first.iter().zip(&anchors) {
            assert!(
                (before.0 - after.0).length() < 1e-3,
                "anchor {:?} stayed put, but moved to {:?} at tick {tick}",
                before.0,
                after.0
            );
        }
    }
}

/// Nothing is faded into place. A cell sitting on the grid is either fully lit
/// or part of the payout's chase, which never drops a cell below half.
#[test]
fn slot_render_contract_no_cell_is_faded_onto_the_grid() {
    let mut state = spinning(ASPECT);
    let layout = Layout::new(ASPECT);
    let side = layout.pitch * 0.8;
    for tick in 0..SLOT_MACHINE_TICKS {
        advance(&mut state);
        let drawn = cells(&render_frame(&state), ACTIVE_CELL_LAYER, side);
        let brightest = drawn.iter().map(|(_, alpha)| *alpha).fold(0.0, f32::max);
        if !drawn.is_empty() {
            assert!(
                brightest > 0.99,
                "the reading keeps its brightness at tick {tick}, but peaked at {brightest:.3}"
            );
            let dimmest = drawn
                .iter()
                .map(|(_, alpha)| *alpha)
                .fold(f32::MAX, f32::min);
            assert!(
                dimmest >= 0.5 - 1e-3,
                "no cell is faded out at tick {tick}: one was {dimmest:.3}"
            );
        }
    }
}

/// A spectacle on a Pi stays bounded, and it creates no physics at all.
#[test]
fn slot_render_contract_the_whole_spin_stays_within_its_budget() {
    for aspect in [0.6, ASPECT, 1024.0 / 768.0] {
        let mut state = spinning(aspect);
        let mut worst = 0;
        for tick in 0..SLOT_MACHINE_TICKS {
            advance(&mut state);
            let frame = render_frame(&state);
            let primitives: usize = frame.layers.iter().map(|l| l.primitives.len()).sum();
            worst = worst.max(primitives);
            assert!(
                primitives <= PRIMITIVE_BUDGET,
                "aspect {aspect} drew {primitives} primitives at tick {tick}"
            );
            assert_eq!(
                (state.body_count(), state.collider_count()),
                (0, 0),
                "the reels own no physics"
            );
        }
        assert!(worst > 0, "aspect {aspect} drew the reels at all");
    }
}

/// The event hands the ordinary face back exactly as an untouched clock draws
/// it, so nothing of the reels is left behind in the slots.
#[test]
fn slot_render_contract_the_face_returns_when_the_reels_are_done() {
    let mut untouched = ClockScenario::init(
        ClockConfig {
            aspect_ratio: ASPECT,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        7,
    );
    ClockScenario::step(
        &mut untouched,
        &[ClockAction::set_reading(
            ClockReading::new(10, 42, 0).expect("a valid reading"),
        )],
        Duration::ZERO,
    );
    let mut state = spinning(ASPECT);
    for _ in 0..SLOT_MACHINE_TICKS {
        advance(&mut state);
    }
    assert_eq!(
        state.event_kind(),
        None,
        "the event has released the digit slots"
    );
    assert_eq!(
        render_frame(&state),
        render_frame(&untouched),
        "the face comes back exactly as an untouched clock draws it"
    );
}

/// The payout sweeps the whole reading. The chase head has to travel a glyph
/// outline plus the per-slot offsets inside the payout's ticks, so every cell is
/// lit once rather than the sweep stalling partway across the face.
#[test]
fn slot_render_contract_the_payout_sweeps_every_cell() {
    let mut state = spinning(ASPECT);
    let layout = Layout::new(ASPECT);
    let side = layout.pitch * 0.8;
    let mut brightest: Vec<(Vec2, f32)> = Vec::new();
    for _ in 0..SLOT_MACHINE_TICKS {
        advance(&mut state);
        if state.event_kind().is_none() {
            break;
        }
        let Some(slot_machine) = state.slot_machine_state() else {
            continue;
        };
        if slot_machine.phase != engine_common::ClockSlotMachinePhase::Payout {
            continue;
        }
        for (centre, alpha) in cells(&render_frame(&state), ACTIVE_CELL_LAYER, side) {
            match brightest
                .iter_mut()
                .find(|(at, _)| (*at - centre).length() < side * 0.1)
            {
                Some(seen) => seen.1 = seen.1.max(alpha),
                None => brightest.push((centre, alpha)),
            }
        }
    }
    assert!(
        brightest.len() >= 20,
        "the payout shows the settled reading, not {} cells",
        brightest.len()
    );
    // The chase is sampled at 30 ticks, so a cell peaks wherever the head
    // happened to be on a tick rather than exactly on it: the measured minimum
    // is 0.86. A cell the head never reached sits at the pattern's 0.5 floor, so
    // this separates swept from unswept with margin either side.
    let dimmest = brightest
        .iter()
        .map(|(_, alpha)| *alpha)
        .fold(f32::MAX, f32::min);
    assert!(
        dimmest > 0.75,
        "the chase reaches every cell, but one peaked at only {dimmest:.3} across \
         {} cells",
        brightest.len()
    );
}
