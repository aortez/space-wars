//! A Slot Machine's reels carry the face's surfaces out of each slot and bring
//! new ones in with the glyph each reel locks on. The frames between are
//! texture and carry nothing.

use super::*;
use crate::events::slot_machine::{SLOT_MACHINE_TICKS, SlotMachineEvent};

fn event(state: &ClockState) -> &SlotMachineEvent {
    match &state.active_event {
        Some(crate::events::ActiveEvent::SlotMachine(event)) => event,
        _ => panic!("a slot machine is running"),
    }
}

fn locked(state: &ClockState) -> [bool; DIGIT_SLOT_COUNT] {
    state
        .slot_machine_state()
        .expect("a slot machine is running")
        .locked_slots
}

/// The agreement a reel keeps. The scramble and the near misses before the
/// lock are texture, drawn without a surface.
fn assert_reel_agreement(state: &ClockState, context: &str) -> Vec<DigitSurface> {
    let layout = Layout::new(state.aspect_ratio());
    let event = event(state);
    let mut texture = Vec::new();
    if !event.paying_out() {
        for slot in 0..DIGIT_SLOT_COUNT {
            let travel = event.travel(slot);
            for step in 0..2 {
                let index = travel.floor() as u32 + step;
                if !event.texture(slot, index) {
                    continue;
                }
                let offset = Vec2::new(0.0, (index as f32 - travel) * 9.0 * layout.pitch);
                texture.extend(
                    event
                        .glyph(slot, index)
                        .cells()
                        .map(|cell| layout.slot_cell_center(slot, cell) + offset),
                );
            }
        }
    }
    // Frames sit a whole cell apart at least, and a clipped cell's centroid
    // stays within half a cell of its centre.
    assert_agreement(state, context, |centroid, _| {
        texture.iter().any(|centre| {
            (centre.x - centroid.x).abs() < 0.05 * layout.pitch
                && (centre.y - centroid.y).abs() < 0.5 * layout.pitch
        })
    })
}

#[test]
fn surface_contract_a_slot_machine_carries_the_face_out_and_locks_new_surfaces_in() {
    for font in FONTS {
        for aspect in ASPECTS {
            let context = format!("{font:?} at {aspect}");
            let mut state = clock(
                aspect,
                font,
                ClockTimeFormat::TwentyFourHour,
                reading(10, 28, 0),
            );
            let layout = Layout::new(aspect);
            let mut markers = Markers::attach(&state, |_| Some(Vec2::new(0.2, 0.25)));
            let face = state.digit_surfaces();
            let accounting = state.surface_accounting();
            sync(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::SlotMachine)],
            );
            let mut steps = [face.clone(), face.clone()];
            // The frame of its reel's strip each surface rides.
            let mut frames: Vec<(SurfaceId, f32)> = Vec::new();
            let mut ticks = 0;
            while state.event_kind() == Some(ClockEventKind::SlotMachine) {
                let context = format!("{context}, tick {ticks}");
                let surfaces = markers.follow(&state);
                let band = layout.digit_band();
                for surface in &surfaces {
                    assert_eq!(surface.source, SurfaceSource::Reel, "{context}");
                    assert_eq!(surface.clip, Some(band), "{context}");
                    assert_eq!((surface.angle, surface.scale), (0.0, layout.pitch));
                    let slot = usize::from(surface.slot);
                    let home = layout.slot_cell_center(slot, surface.cell);
                    assert_eq!(surface.origin.x, home.x, "{context}");
                    // A surface rides one whole frame of its reel all its
                    // life: the opening glyph the first, the new one the
                    // frame the reel locks on.
                    let travel = event(&state).travel(slot);
                    let frame = (surface.origin.y - home.y) / (9.0 * layout.pitch) + travel;
                    assert!((frame - frame.round()).abs() < 1e-3, "{context}: {frame}");
                    // A reel carries surfaces only on the two frames it draws.
                    assert!(
                        (travel.floor()..=travel.floor() + 1.0).contains(&frame.round()),
                        "{context}: frame {frame} at travel {travel}"
                    );
                    match frames.iter().find(|(id, _)| *id == surface.id) {
                        Some((_, first)) => assert_eq!(frame.round(), *first, "{context}"),
                        None => frames.push((surface.id, frame.round())),
                    }
                    let opening = face.iter().any(|old| old.id == surface.id);
                    assert_eq!(surface.departing, opening, "{context}");
                    assert_eq!(frame.round() == 0.0, opening, "{context}");
                    // An opening cell is retired as it leaves the slot.
                    assert!(!opening || visible(surface).is_some(), "{context}");
                }
                if ticks % 4 == 0 || ticks + 3 > SLOT_MACHINE_TICKS {
                    assert_reel_agreement(&state, &context);
                }
                steps = [std::mem::take(&mut steps[1]), surfaces];
                tick(&mut state, &[]);
                ticks += 1;
            }
            assert_eq!(ticks, SLOT_MACHINE_TICKS, "{context}");
            let after = markers.follow(&state);
            assert_no_jump([&steps[0], &steps[1], &after], layout.pitch, &context);
            assert_face_agreement(&state, &context);
            // The reels landed on the same reading, as new surfaces.
            let mut landed = ids(&after);
            landed.sort_unstable();
            let mut incoming: Vec<_> = frames
                .iter()
                .filter(|(_, frame)| *frame > 0.0)
                .map(|(id, _)| *id)
                .collect();
            incoming.sort_unstable();
            assert_eq!(landed, incoming, "{context}");
            assert_eq!(after.len(), face.len(), "{context}");
            assert!(
                after
                    .iter()
                    .all(|s| s.source == SurfaceSource::Anchored && s.opacity == 1.0)
            );
            let now = state.surface_accounting();
            assert_eq!(
                now.admitted - accounting.admitted,
                face.len() as u64,
                "{context}"
            );
            assert_eq!(
                now.retired(Retirement::Replaced),
                face.len() as u64,
                "{context}"
            );
            assert_eq!(now.total_retired(), face.len() as u64, "{context}");
            for marker in &markers.markers {
                assert_eq!(
                    marker.lost,
                    Some(Loss::Retired(Retirement::Replaced)),
                    "{context}"
                );
            }
            assert_eq!(markers.departed.len(), face.len(), "{context}");
        }
    }
}

#[test]
fn surface_contract_a_reading_changed_before_a_reel_locks_replaces_its_incoming_glyph() {
    let mut state = clock(
        800.0 / 480.0,
        ClockFont::Classic,
        ClockTimeFormat::TwentyFourHour,
        reading(10, 28, 0),
    );
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::SlotMachine)],
    );
    // Run until the last reel's locking glyph is in its window but turning.
    let in_window = |state: &ClockState| {
        state
            .digit_surfaces()
            .iter()
            .any(|s| s.slot == 3 && !s.departing)
    };
    while !in_window(&state) {
        assert!(
            !locked(&state)[3],
            "the last reel's glyph comes in before it locks"
        );
        tick(&mut state, &[]);
    }
    tick(&mut state, &[]);
    assert_eq!(locked(&state), [true, true, true, false]);
    let low = Vec2::new(0.0, -0.3);
    let mut markers = Markers::attach(&state, |s| (s.slot >= 2).then_some(low));
    let before = state.digit_surfaces();
    let sequence = state.surface_accounting().sequence;
    tick(&mut state, &[ClockAction::set_reading(reading(10, 29, 0))]);
    let after = markers.follow(&state);
    assert_reel_agreement(&state, "retargeted");
    for surface in before.iter().filter(|s| !s.departing) {
        let kept = after.iter().any(|other| other.id == surface.id);
        assert_eq!(kept, surface.slot != 3, "slot {}", surface.slot);
        if surface.slot >= 2 {
            assert_eq!(markers.on(surface.id), kept);
        }
    }
    let fresh: Vec<_> = after
        .iter()
        .filter(|s| before.iter().all(|old| old.id != s.id))
        .collect();
    let nine = crate::fonts::glyph(ClockFont::Classic, Some(9))
        .cells()
        .count();
    assert_eq!(fresh.len(), nine);
    assert!(fresh.iter().all(|s| s.slot == 3 && !s.departing));
    let changes: Vec<_> = state
        .surface_changes()
        .filter(|change| change.sequence > sequence)
        .map(|change| change.kind)
        .collect();
    let eight = crate::fonts::glyph(ClockFont::Classic, Some(8))
        .cells()
        .count();
    assert_eq!(changes.len(), eight + nine, "{changes:?}");
    // A reading that changes after every reel has locked moves nothing until
    // the face takes over and replaces what changed.
    while !locked(&state)[3] {
        tick(&mut state, &[]);
        markers.follow(&state);
    }
    let settled = markers.follow(&state);
    let sequence = state.surface_accounting().sequence;
    tick(&mut state, &[ClockAction::set_reading(reading(10, 30, 0))]);
    assert_eq!(ids(&markers.follow(&state)), ids(&settled));
    assert_eq!(state.surface_accounting().sequence, sequence);
    assert_reel_agreement(&state, "locked");
    while state.event_kind() == Some(ClockEventKind::SlotMachine) {
        tick(&mut state, &[]);
        markers.follow(&state);
    }
    let face = markers.follow(&state);
    assert_face_agreement(&state, "after");
    for surface in &settled {
        let kept = face.iter().any(|other| other.id == surface.id);
        assert_eq!(kept, surface.slot < 2, "slot {}", surface.slot);
    }
    assert!(markers.attached().next().is_none());
}

#[test]
fn surface_contract_a_slot_machine_cut_short_keeps_only_what_locked() {
    for stop in [0, 30, 260] {
        let context = format!("stopped at {stop}");
        let mut state = clock(
            0.6,
            ClockFont::Matrix,
            ClockTimeFormat::TwentyFourHour,
            reading(10, 28, 0),
        );
        let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
        let face = state.digit_surfaces();
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::SlotMachine)],
        );
        for _ in 0..stop {
            tick(&mut state, &[]);
            markers.follow(&state);
        }
        let lock = locked(&state);
        let held = markers.follow(&state);
        let late = Markers::attach(&state, |_| Some(Vec2::ZERO));
        // Paused, the reels neither move nor record anything.
        let sequence = state.surface_accounting().sequence;
        for _ in 0..3 {
            sync(&mut state, &[]);
            assert_eq!(state.digit_surfaces(), held, "{context}");
        }
        assert_eq!(state.surface_accounting().sequence, sequence, "{context}");
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
        );
        let after = markers.follow(&state);
        let mut late = late;
        late.follow(&state);
        assert_face_agreement(&state, &context);
        assert_eq!(after.len(), face.len(), "{context}");
        for surface in &held {
            let kept = after.iter().any(|other| other.id == surface.id);
            // The opening glyph goes either way; a locked glyph is home.
            let expected = !surface.departing && lock[usize::from(surface.slot)];
            assert_eq!(kept, expected, "{context}: slot {}", surface.slot);
        }
        for marker in &markers.markers {
            assert_eq!(
                marker.lost,
                Some(Loss::Retired(Retirement::Replaced)),
                "{context}"
            );
        }
        for marker in &late.markers {
            let slot = held
                .iter()
                .find(|s| s.id == marker.surface)
                .map(|s| usize::from(s.slot))
                .expect("a marker sits on a published surface");
            let expected = (!lock[slot]).then_some(Loss::Retired(Retirement::Interrupted));
            assert_eq!(marker.lost, expected, "{context}: slot {slot}");
        }
        if stop == 260 {
            assert_eq!(lock, [true, true, true, false], "{context}");
            assert!(!late.markers.is_empty(), "{context}");
        }
    }
}
