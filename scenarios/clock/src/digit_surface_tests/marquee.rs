//! A takeover carries the face's own surfaces without holding them: a clock
//! preset on the copy of the reading its recipe animates, a text takeover with
//! the reading it sweeps clear and back.

use super::*;
use crate::{digit_surfaces::Glyph, events::marquee::MarqueeEvent};
use engine_common::ClockMarqueePreset;

const CLOCK_PRESETS: [ClockMarqueePreset; 4] = [
    ClockMarqueePreset::ClockChase,
    ClockMarqueePreset::ClockWave,
    ClockMarqueePreset::ClockSpin,
    ClockMarqueePreset::DigitSpin,
];
const TEXT_PRESETS: [ClockMarqueePreset; 3] = [
    ClockMarqueePreset::TextScroll,
    ClockMarqueePreset::TextRibbon,
    ClockMarqueePreset::TextSpin,
];

fn showing(aspect: f32, font: ClockFont, preset: ClockMarqueePreset) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            marquee_preset: preset,
            ..config(aspect, font, ClockTimeFormat::TwentyFourHour)
        },
        7,
    );
    sync(&mut state, &[ClockAction::set_reading(reading(10, 28, 0))]);
    state
}

fn event(state: &ClockState) -> &MarqueeEvent {
    match &state.active_event {
        Some(crate::events::ActiveEvent::Marquee(event)) => event,
        _ => panic!("a marquee is showing"),
    }
}

/// The agreement every takeover keeps. The copy of the reading a clock preset
/// dissolves where it stands is presentation and owns no surface.
fn assert_marquee_agreement(state: &ClockState, context: &str) -> Vec<DigitSurface> {
    let layout = Layout::new(state.aspect_ratio());
    let event = event(state);
    let ink = event.face(layout).ink;
    let places: Vec<Vec2> = (0..DIGIT_SLOT_COUNT)
        .flat_map(|slot| {
            Glyph::anchored(state.segments(), slot)
                .mask
                .cells()
                .map(move |cell| layout.slot_cell_center(slot, cell))
        })
        .collect();
    assert_agreement(state, context, |centroid, alpha| {
        event.uses_clock
            && (alpha - ink).abs() <= 0.02
            && places
                .iter()
                .any(|place| (*place - centroid).length() <= 0.05 * layout.pitch)
    })
}

/// Whether a preset moves the face's cells rather than only relighting them.
fn moves(preset: ClockMarqueePreset) -> bool {
    preset != ClockMarqueePreset::ClockChase
}

#[test]
fn surface_contract_a_clock_marquee_carries_the_face_on_the_copy_it_animates() {
    for (index, preset) in CLOCK_PRESETS.into_iter().enumerate() {
        for (place, aspect) in ASPECTS.into_iter().enumerate() {
            let font = FONTS[(index + place) % FONTS.len()];
            let context = format!("{} in {font:?} at {aspect}", preset.as_str());
            let mut state = showing(aspect, font, preset);
            let layout = Layout::new(aspect);
            let mut markers = Markers::attach(&state, |_| Some(Vec2::new(-0.25, 0.3)));
            let face = state.digit_surfaces();
            let accounting = state.surface_accounting();
            sync(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Marquee)],
            );
            let mut steps = [face.clone(), face.clone()];
            let mut travelled = 0.0f32;
            let mut turned = 0.0f32;
            let mut ticks = 0;
            while state.event_kind() == Some(ClockEventKind::Marquee) {
                let context = format!("{context}, tick {ticks}");
                let surfaces = markers.follow(&state);
                assert_eq!(ids(&surfaces), ids(&face), "{context}");
                assert!(
                    surfaces
                        .iter()
                        .all(|s| s.source == SurfaceSource::Marquee && s.eligible()),
                    "{context}"
                );
                assert_no_jump([&steps[0], &steps[1], &surfaces], layout.pitch, &context);
                if ticks % 9 == 0 || ticks < 3 || ticks + 4 > MARQUEE_TICKS {
                    assert_marquee_agreement(&state, &context);
                }
                for (surface, home) in surfaces.iter().zip(&face) {
                    travelled = travelled.max((surface.origin - home.origin).length());
                    turned = turned.max(surface.angle.abs());
                }
                steps = [std::mem::take(&mut steps[1]), surfaces];
                tick(&mut state, &[]);
                ticks += 1;
            }
            assert_eq!(ticks, MARQUEE_TICKS, "{context}");
            let after = markers.follow(&state);
            assert_no_jump([&steps[0], &steps[1], &after], layout.pitch, &context);
            assert_face_agreement(&state, &context);
            assert_eq!(after, face, "{context}: the face comes home as itself");
            let now = state.surface_accounting();
            assert_eq!(now.admitted, accounting.admitted, "{context}");
            assert_eq!(now.total_retired(), 0, "{context}");
            assert_eq!(markers.attached().count(), face.len(), "{context}");
            for marker in &markers.markers {
                let handoffs: Vec<_> = markers
                    .handoffs
                    .iter()
                    .filter(|(id, ..)| *id == marker.surface)
                    .map(|(_, from, to)| (*from, *to))
                    .collect();
                assert_eq!(
                    handoffs,
                    [
                        (SurfaceSource::Anchored, SurfaceSource::Marquee),
                        (SurfaceSource::Marquee, SurfaceSource::Anchored)
                    ],
                    "{context}"
                );
            }
            if moves(preset) {
                assert!(travelled > layout.pitch, "{context}: {travelled}");
            } else {
                assert!(travelled < 1e-3 * layout.pitch, "{context}: {travelled}");
            }
            let spins = matches!(
                preset,
                ClockMarqueePreset::ClockSpin | ClockMarqueePreset::DigitSpin
            );
            assert_eq!(turned > 3.0, spins, "{context}: {turned}");
        }
    }
}

#[test]
fn surface_contract_a_text_marquee_carries_the_reading_clear_and_back_as_itself() {
    for (index, preset) in TEXT_PRESETS.into_iter().enumerate() {
        for (place, aspect) in ASPECTS.into_iter().enumerate() {
            let font = FONTS[(index + place) % FONTS.len()];
            let context = format!("{} in {font:?} at {aspect}", preset.as_str());
            let mut state = showing(aspect, font, preset);
            let layout = Layout::new(aspect);
            let mut markers = Markers::attach(&state, |_| Some(Vec2::new(0.3, -0.1)));
            let face = state.digit_surfaces();
            sync(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Marquee)],
            );
            let mut steps = [face.clone(), face.clone()];
            let mut cleared = false;
            let mut ticks = 0;
            while state.event_kind() == Some(ClockEventKind::Marquee) {
                let context = format!("{context}, tick {ticks}");
                let surfaces = markers.follow(&state);
                assert_eq!(ids(&surfaces), ids(&face), "{context}");
                let carried = event(&state).face(layout);
                for (surface, home) in surfaces.iter().zip(&face) {
                    // The text's own cells are not digit surfaces, and the
                    // reading travels whole, upright and at its own scale.
                    assert_eq!(surface.source, SurfaceSource::Marquee, "{context}");
                    assert_eq!(
                        surface.origin,
                        home.origin + Vec2::new(carried.offset, 0.0),
                        "{context}"
                    );
                    assert_eq!(
                        (surface.angle, surface.scale),
                        (0.0, layout.pitch),
                        "{context}"
                    );
                    assert_eq!(surface.opacity, carried.ink, "{context}");
                }
                cleared |= carried.ink == 0.0;
                // The reading leaves to the left and comes back from the
                // right, so it moves once out of view to turn around.
                if steps.iter().all(|step| step[0].opacity > 0.0) && surfaces[0].opacity > 0.0 {
                    assert_no_jump([&steps[0], &steps[1], &surfaces], layout.pitch, &context);
                }
                if ticks % 9 == 0 || ticks < 3 || ticks + 4 > MARQUEE_TICKS {
                    assert_marquee_agreement(&state, &context);
                }
                steps = [std::mem::take(&mut steps[1]), surfaces];
                tick(&mut state, &[]);
                ticks += 1;
            }
            assert!(cleared, "{context}: the reading left the view");
            let after = markers.follow(&state);
            assert_no_jump([&steps[0], &steps[1], &after], layout.pitch, &context);
            assert_face_agreement(&state, &context);
            assert_eq!(after, face, "{context}: the face comes home as itself");
            assert_eq!(state.surface_accounting().total_retired(), 0);
            assert_eq!(markers.attached().count(), face.len(), "{context}");
        }
    }
}

#[test]
fn surface_contract_a_reading_changed_under_a_marquee_replaces_only_its_glyph() {
    for preset in [
        ClockMarqueePreset::ClockSpin,
        ClockMarqueePreset::ClockWave,
        ClockMarqueePreset::TextScroll,
    ] {
        for at in [30, 400] {
            let context = format!("{} at tick {at}", preset.as_str());
            let mut state = showing(800.0 / 480.0, ClockFont::Classic, preset);
            let mut markers = Markers::attach(&state, |_| Some(Vec2::new(0.1, 0.2)));
            sync(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Marquee)],
            );
            for _ in 0..at {
                tick(&mut state, &[]);
                markers.follow(&state);
            }
            let before = state.digit_surfaces();
            let accounting = state.surface_accounting();
            tick(&mut state, &[ClockAction::set_reading(reading(10, 29, 0))]);
            let after = markers.follow(&state);
            assert_marquee_agreement(&state, &context);
            for surface in &before {
                let kept = after.iter().any(|other| other.id == surface.id);
                assert_eq!(kept, surface.slot != 3, "{context}: slot {}", surface.slot);
                assert_eq!(markers.on(surface.id), kept, "{context}");
            }
            let fresh: Vec<_> = after
                .iter()
                .filter(|surface| before.iter().all(|other| other.id != surface.id))
                .collect();
            let nine = crate::fonts::glyph(ClockFont::Classic, Some(9))
                .cells()
                .count();
            let eight = crate::fonts::glyph(ClockFont::Classic, Some(8))
                .cells()
                .count();
            assert_eq!(fresh.len(), nine, "{context}");
            assert!(
                fresh
                    .iter()
                    .all(|s| s.slot == 3 && s.source == SurfaceSource::Marquee),
                "{context}"
            );
            // One update per logical change, however many copies are drawn.
            let changes: Vec<_> = state
                .surface_changes()
                .filter(|change| change.sequence > accounting.sequence)
                .map(|change| change.kind)
                .collect();
            assert_eq!(changes.len(), eight + nine, "{context}: {changes:?}");
            assert!(changes.iter().all(|kind| matches!(
                kind,
                SurfaceChangeKind::Retired(Retirement::Replaced)
                    | SurfaceChangeKind::Admitted {
                        slot: 3,
                        source: SurfaceSource::Marquee,
                        ..
                    }
            )));
            for marker in &markers.markers {
                let slot = before
                    .iter()
                    .find(|surface| surface.id == marker.surface)
                    .map(|surface| surface.slot);
                if slot == Some(3) {
                    assert_eq!(
                        marker.lost,
                        Some(Loss::Retired(Retirement::Replaced)),
                        "{context}"
                    );
                }
            }
            while state.event_kind() == Some(ClockEventKind::Marquee) {
                tick(&mut state, &[]);
                markers.follow(&state);
            }
            let home = markers.follow(&state);
            assert_face_agreement(&state, &context);
            assert_eq!(ids(&home), ids(&after), "{context}");
            assert_eq!(markers.attached().count(), before.len() - eight);
        }
    }
}

#[test]
fn surface_contract_a_marquee_cut_short_retires_a_face_it_carried_off() {
    for preset in [
        ClockMarqueePreset::ClockChase,
        ClockMarqueePreset::DigitSpin,
        ClockMarqueePreset::TextRibbon,
    ] {
        for stop in [0, 30, 400] {
            let context = format!("{} stopped at {stop}", preset.as_str());
            let mut state = showing(4.0 / 3.0, ClockFont::Serif, preset);
            let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
            let face = state.digit_surfaces();
            sync(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Marquee)],
            );
            for _ in 0..stop {
                tick(&mut state, &[]);
                markers.follow(&state);
            }
            // Paused, the takeover neither moves nor records anything.
            let held = state.digit_surfaces();
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
            assert_face_agreement(&state, &context);
            let carried = stop > 0 && moves(preset);
            assert_eq!(after.len(), face.len(), "{context}");
            for surface in &after {
                assert_eq!(surface.source, SurfaceSource::Anchored, "{context}");
                assert_eq!(
                    face.iter().any(|old| old.id == surface.id),
                    !carried,
                    "{context}"
                );
            }
            for marker in &markers.markers {
                let expected = carried.then_some(Loss::Retired(Retirement::Interrupted));
                assert_eq!(marker.lost, expected, "{context}");
            }
        }
    }
}

#[test]
fn surface_contract_a_repeated_marquee_carries_whatever_face_the_last_one_left() {
    let mut state = showing(0.6, ClockFont::Matrix, ClockMarqueePreset::ClockSpin);
    let face = state.digit_surfaces();
    let mut markers = Markers::attach(&state, |_| Some(Vec2::new(0.2, 0.2)));
    let mut previous = 0;
    for (round, stop) in [200, 0, 300].into_iter().enumerate() {
        let context = format!("round {round}");
        let before = state.digit_surfaces();
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Marquee)],
        );
        let surfaces = markers.follow(&state);
        // A takeover that replaces one mid-motion starts from a new face at
        // home; one that replaces one that never moved keeps it.
        assert_eq!(ids(&surfaces) == ids(&before), previous == 0, "{context}");
        previous = stop;
        assert!(
            surfaces
                .iter()
                .all(|surface| surface.source == SurfaceSource::Marquee),
            "{context}"
        );
        assert_marquee_agreement(&state, &context);
        for _ in 0..stop {
            tick(&mut state, &[]);
            markers.follow(&state);
        }
    }
    while state.event_kind() == Some(ClockEventKind::Marquee) {
        tick(&mut state, &[]);
        markers.follow(&state);
    }
    let after = markers.follow(&state);
    assert_face_agreement(&state, "after");
    assert!(
        after
            .iter()
            .all(|surface| face.iter().all(|old| old.id != surface.id))
    );
    let accounting = state.surface_accounting();
    assert_eq!(
        accounting.retired(Retirement::Interrupted),
        face.len() as u64
    );
    assert_eq!(accounting.live, face.len());
    assert!(markers.attached().next().is_none());
}
