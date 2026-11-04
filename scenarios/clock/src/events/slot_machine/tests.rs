use super::*;
use engine_common::ClockSlotMachinePhase;

fn display(digits: [Option<u8>; DIGIT_SLOT_COUNT]) -> DisplaySnapshot {
    DisplaySnapshot {
        font: ClockFont::Classic,
        digits,
        colon_lit: true,
        meridiem: None,
    }
}

fn at(reading: [u8; 4]) -> DisplaySnapshot {
    display(reading.map(Some))
}

fn run(event: &mut SlotMachineEvent, ticks: u64) -> bool {
    let mut finished = false;
    for _ in 0..ticks {
        finished = event.step();
    }
    finished
}

/// The four phases partition the event exactly, so the catalog's duration and
/// the reels agree about when it is over.
#[test]
fn slot_contract_the_phases_partition_the_whole_event() {
    let mut event = SlotMachineEvent::new(at([1, 0, 4, 2]), 7, false);
    let mut seen = Vec::new();
    for tick in 0..SLOT_MACHINE_TICKS {
        let phase = event.phase();
        if seen.last() != Some(&phase) {
            seen.push(phase);
        }
        assert!(
            !event.step() || tick + 1 == SLOT_MACHINE_TICKS,
            "the event finishes exactly on its catalog duration"
        );
    }
    assert_eq!(
        seen,
        vec![
            ClockSlotMachinePhase::SpinUp,
            ClockSlotMachinePhase::Spinning,
            ClockSlotMachinePhase::Settling,
            ClockSlotMachinePhase::Payout,
        ]
    );
}

/// Every reel starts on the reading the face was showing, travels forwards only,
/// and comes to rest exactly on a whole glyph: its own stop. Landing between
/// glyphs would leave two half-drawn digits in the slot.
#[test]
fn slot_contract_every_reel_opens_on_the_face_and_lands_on_a_whole_glyph() {
    let opening = at([1, 0, 4, 2]);
    let mut event = SlotMachineEvent::new(opening, 7, false);
    for slot in 0..DIGIT_SLOT_COUNT {
        assert_eq!(event.travel(slot), 0.0, "slot {slot} starts at rest");
        assert_eq!(
            event.glyph(slot, 0).0,
            crate::fonts::glyph(opening.font, opening.digits[slot]).0,
            "slot {slot} opens on the face's own digit"
        );
    }
    let mut previous = [0.0f32; DIGIT_SLOT_COUNT];
    for tick in 0..SLOT_MACHINE_TICKS {
        event.step();
        for (slot, previous) in previous.iter_mut().enumerate() {
            let travel = event.travel(slot);
            assert!(
                travel >= *previous - 1e-4,
                "slot {slot} only ever turns forwards, but went {travel} after {previous} at \
                 tick {tick}"
            );
            *previous = travel;
        }
    }
    for slot in 0..DIGIT_SLOT_COUNT {
        let travel = event.travel(slot);
        assert!(
            (travel - travel.round()).abs() < 1e-3,
            "slot {slot} rests on a whole glyph, not {travel}"
        );
        assert_eq!(
            event.glyph(slot, travel.round() as u32).0,
            crate::fonts::glyph(ClockFont::Classic, Some([1, 0, 4, 2][slot])).0,
            "slot {slot} rests on its target"
        );
    }
}

/// The reels stop left to right on the published stagger. That order is the part
/// that actually reads as a slot machine.
#[test]
fn slot_contract_the_reels_lock_left_to_right_on_the_stagger() {
    let mut event = SlotMachineEvent::new(at([1, 0, 4, 2]), 7, false);
    let mut locked = [0u64; DIGIT_SLOT_COUNT];
    for tick in 1..=SLOT_MACHINE_TICKS {
        event.step();
        let state = event.diagnostics();
        for (slot, locked) in locked.iter_mut().enumerate() {
            if state.locked_slots[slot] && *locked == 0 {
                *locked = tick;
            }
        }
    }
    for (slot, locked) in locked.iter().enumerate() {
        assert_eq!(
            *locked,
            locked_at(slot),
            "slot {slot} locks on its published tick"
        );
    }
    for slot in 1..DIGIT_SLOT_COUNT {
        assert_eq!(
            locked[slot] - locked[slot - 1],
            LOCK_STAGGER_TICKS,
            "slot {slot} follows the one on its left by the stagger"
        );
    }
}

/// A reel still turning adopts a new reading without any change to its motion,
/// so a minute rolling over mid-spin is invisible and the event lands on live
/// time. This is the opposite of Digit Slide, which cancels outright.
#[test]
fn slot_contract_a_reading_change_mid_spin_retargets_without_disturbing_the_motion() {
    let mut undisturbed = SlotMachineEvent::new(at([1, 0, 4, 2]), 7, false);
    let mut retargeted = SlotMachineEvent::new(at([1, 0, 4, 2]), 7, false);
    run(&mut undisturbed, 100);
    run(&mut retargeted, 100);
    retargeted.synchronize(at([1, 0, 4, 3]));
    assert_eq!(retargeted.diagnostics().retargets, 1);
    for _ in 0..50 {
        undisturbed.step();
        retargeted.step();
        for slot in 0..DIGIT_SLOT_COUNT {
            assert_eq!(
                undisturbed.travel(slot),
                retargeted.travel(slot),
                "slot {slot} keeps its trajectory through a retarget"
            );
        }
    }
    run(&mut retargeted, SLOT_MACHINE_TICKS);
    assert_eq!(
        retargeted.settled().digits,
        [Some(1), Some(0), Some(4), Some(3)],
        "the reels land on the reading they were given, not the one they opened on"
    );
}

/// A slot that has already stopped keeps what it landed on. The ordinary face
/// takes the newer reading once the event completes.
#[test]
fn slot_contract_a_locked_reel_is_not_moved_by_a_later_reading() {
    let mut event = SlotMachineEvent::new(at([1, 0, 4, 2]), 7, false);
    run(&mut event, locked_at(0));
    assert!(
        event.diagnostics().locked_slots[0],
        "the first reel has stopped"
    );
    assert!(
        !event.diagnostics().locked_slots[1],
        "the second reel is still turning"
    );
    event.synchronize(at([9, 9, 9, 9]));
    run(&mut event, SLOT_MACHINE_TICKS);
    let settled = event.settled().digits;
    assert_eq!(settled[0], Some(1), "the stopped reel keeps its own result");
    assert_eq!(
        &settled[1..],
        &[Some(9), Some(9), Some(9)],
        "the reels still turning take the newer reading"
    );
}

/// A blank leading digit is an ordinary outcome in 12-hour mode, so that reel
/// still turns and still stops rather than being skipped.
#[test]
fn slot_contract_a_blank_leading_digit_is_a_real_stop() {
    let blank = display([None, Some(9), Some(0), Some(5)]);
    let mut event = SlotMachineEvent::new(blank, 7, false);
    run(&mut event, SLOT_MACHINE_TICKS);
    assert_eq!(event.settled().digits[0], None);
    assert!(
        event.travel(0) > 1.0,
        "the blank reel turns like any other, not {}",
        event.travel(0)
    );
    assert_eq!(
        event.glyph(0, event.travel(0).round() as u32).0,
        0,
        "it comes to rest on an empty slot"
    );
}

/// The last two stops before each lock are digits in the face's own font, so the
/// reel resolves out of the scramble into legitimate readings and can never come
/// to rest showing a letter.
#[test]
fn slot_contract_the_scramble_resolves_into_digits_before_the_lock() {
    let event = SlotMachineEvent::new(at([1, 0, 4, 2]), 7, false);
    let digits: Vec<u64> = (0..10)
        .map(|digit| crate::fonts::glyph(ClockFont::Classic, Some(digit)).0)
        .collect();
    for slot in 0..DIGIT_SLOT_COUNT {
        let stop = stop_index(slot);
        for index in stop.saturating_sub(2)..=stop {
            assert!(
                digits.contains(&event.glyph(slot, index).0),
                "slot {slot} shows a digit at stop {index} of {stop}"
            );
        }
        let scrambled = (1..stop.saturating_sub(2))
            .filter(|index| !digits.contains(&event.glyph(slot, *index).0))
            .count();
        assert!(
            scrambled > 0,
            "slot {slot} scrambles somewhere before it resolves"
        );
    }
}

/// Same seed, same reels. Every position is addressed from the tick rather than
/// accumulated, so a replay is identical and a paused event cannot drift.
#[test]
fn slot_contract_the_same_seed_replays_exactly() {
    let frames = |seed: u64| {
        let mut event = SlotMachineEvent::new(at([1, 0, 4, 2]), seed, false);
        let mut seen = Vec::new();
        for _ in 0..SLOT_MACHINE_TICKS {
            event.step();
            for slot in 0..DIGIT_SLOT_COUNT {
                let travel = event.travel(slot);
                seen.push((travel.to_bits(), event.glyph(slot, travel.floor() as u32).0));
            }
        }
        seen
    };
    assert_eq!(frames(7), frames(7));
    assert_ne!(
        frames(7),
        frames(8),
        "a different seed scrambles differently"
    );
}
