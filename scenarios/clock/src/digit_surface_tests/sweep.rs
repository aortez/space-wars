//! Every event against every font and layout, events cut short by each other,
//! repeated and paused, read the way a consumer would: one lifecycle per
//! surface, a publication that says what the changes say, bounded counts and
//! diagnostics that tell the truth about all of it.

use std::collections::BTreeMap;

use super::*;
use crate::digit_surfaces::{
    MAX_RECENT_CHANGES, MAX_RETAINED_CHANGES, MAX_SAMPLED_SURFACES, MAX_SURFACES,
};
use crate::events::EVENT_CATALOG;
use engine_common::{ClockSurfaceChangeKind, ClockSurfaceLimits, ClockSurfaceSourceCount};

/// The timed events, each of which ends by itself.
const TIMED: [ClockEventKind; 10] = [
    ClockEventKind::Falling,
    ClockEventKind::ColorCycle,
    ClockEventKind::Meltdown,
    ClockEventKind::Marquee,
    ClockEventKind::DigitSlide,
    ClockEventKind::Rain,
    ClockEventKind::Explosion,
    ClockEventKind::Aquarium,
    ClockEventKind::CellSwap,
    ClockEventKind::SlotMachine,
];

/// Reads every change step by step, as a consumer that never misses one, and
/// checks that each surface lives one lifecycle: admitted with the next serial,
/// handed off from the source it had, departing at most once and retired once,
/// and published exactly as the changes describe it.
struct Lifecycle {
    sequence: u64,
    next_id: u32,
    live: BTreeMap<SurfaceId, (SurfaceSource, bool)>,
    log: Vec<SurfaceChange>,
}

impl Lifecycle {
    fn new(state: &ClockState) -> Self {
        let mut lifecycle = Self {
            sequence: 0,
            next_id: 1,
            live: BTreeMap::new(),
            log: Vec::new(),
        };
        lifecycle.read(state, "the first reading");
        lifecycle
    }

    fn read(&mut self, state: &ClockState, context: &str) -> Vec<DigitSurface> {
        let changes = state.surface_changes();
        assert!(changes.len() <= MAX_RETAINED_CHANGES, "{context}");
        let seen = self.sequence;
        for change in changes.filter(|change| change.sequence > seen) {
            assert_eq!(change.sequence, self.sequence + 1, "{context}: a gap");
            self.sequence = change.sequence;
            let id = change.id;
            match change.kind {
                SurfaceChangeKind::Admitted { source, .. } => {
                    assert_eq!(id.get(), self.next_id, "{context}: the next serial");
                    self.next_id += 1;
                    self.live.insert(id, (source, false));
                }
                SurfaceChangeKind::HandedOff { from, to } => {
                    let record = self.live.get_mut(&id).expect("a live surface");
                    assert_eq!(record.0, from, "{context}: surface {}", id.get());
                    assert_ne!(from, to, "{context}");
                    record.0 = to;
                }
                SurfaceChangeKind::Departing => {
                    let record = self.live.get_mut(&id).expect("a live surface");
                    assert!(!record.1, "{context}: surface {} departs once", id.get());
                    record.1 = true;
                }
                SurfaceChangeKind::Retired(_) => {
                    assert!(
                        self.live.remove(&id).is_some(),
                        "{context}: surface {} is retired once",
                        id.get()
                    );
                }
            }
            self.log.push(*change);
        }
        let accounting = state.surface_accounting();
        assert_eq!(accounting.sequence, self.sequence, "{context}");
        let surfaces = state.digit_surfaces();
        let mut published: Vec<_> = surfaces
            .iter()
            .map(|s| (s.id, (s.source, s.departing)))
            .collect();
        published.sort_unstable_by_key(|(id, _)| *id);
        let live: Vec<_> = self
            .live
            .iter()
            .map(|(id, record)| (*id, *record))
            .collect();
        assert_eq!(published, live, "{context}: published as the changes say");
        assert_eq!(accounting.live, surfaces.len(), "{context}");
        assert!(accounting.live <= accounting.peak_live, "{context}");
        assert!(accounting.peak_live <= MAX_SURFACES, "{context}");
        assert_eq!(
            accounting.admitted - accounting.total_retired(),
            surfaces.len() as u64,
            "{context}"
        );
        surfaces
    }
}

/// The surfaces as the control state reports them, checked against what the
/// ledger and the publication say.
fn assert_diagnostics(state: &ClockState, surfaces: &[DigitSurface], context: &str) {
    let diagnostics = state.digit_surface_state();
    let accounting = state.surface_accounting();
    let reported = diagnostics.accounting;
    assert_eq!(
        (
            reported.live,
            reported.peak_live,
            reported.admitted,
            reported.handed_off,
            reported.departed,
            reported.sequence
        ),
        (
            surfaces.len(),
            accounting.peak_live,
            accounting.admitted,
            accounting.handed_off,
            accounting.departed,
            accounting.sequence
        ),
        "{context}"
    );
    let departing = surfaces.iter().filter(|s| s.departing).count();
    assert_eq!(reported.departing, departing, "{context}");
    for reason in Retirement::ALL {
        assert_eq!(
            reported.retired.get(reason),
            accounting.retired(reason),
            "{context}"
        );
    }
    let sources: Vec<_> = SurfaceSource::ALL
        .into_iter()
        .filter_map(|source| {
            let live = surfaces.iter().filter(|s| s.source == source).count();
            (live > 0).then_some(ClockSurfaceSourceCount { source, live })
        })
        .collect();
    assert_eq!(diagnostics.sources, sources, "{context}");

    // One sample for each slot, source and departure: the lowest identity, by
    // slot, then staying before departing, then source.
    let group = |slot: u8, source: SurfaceSource, departing: bool| {
        let source = SurfaceSource::ALL.iter().position(|s| *s == source);
        (slot, departing, source)
    };
    let mut groups: Vec<_> = surfaces
        .iter()
        .map(|s| group(s.slot, s.source, s.departing))
        .collect();
    groups.sort_unstable();
    groups.dedup();
    assert_eq!(
        diagnostics.sample.len(),
        groups.len().min(MAX_SAMPLED_SURFACES),
        "{context}"
    );
    let milli = |value: f32| (value * 1000.0).round() as i32;
    for (sampled, expected) in diagnostics.sample.iter().zip(&groups) {
        assert_eq!(
            group(sampled.slot, sampled.source, sampled.departing),
            *expected,
            "{context}"
        );
        let lowest = surfaces
            .iter()
            .filter(|s| group(s.slot, s.source, s.departing) == *expected)
            .min_by_key(|s| s.id)
            .expect("a sampled group is published");
        assert_eq!(sampled.id, lowest.id.get(), "{context}");
        assert_eq!(sampled.cell, [lowest.cell.x, lowest.cell.y], "{context}");
        assert_eq!(
            sampled.origin_milli,
            [milli(lowest.origin.x), milli(lowest.origin.y)],
            "{context}"
        );
        assert_eq!(
            sampled.angle_millidegrees,
            milli(lowest.angle.to_degrees()),
            "{context}"
        );
        assert_eq!(sampled.scale_milli, milli(lowest.scale), "{context}");
        let Footprint { min, max } = lowest.footprint;
        assert_eq!(
            sampled.footprint_milli,
            [milli(min.x), milli(min.y), milli(max.x), milli(max.y)],
            "{context}"
        );
        assert_eq!(
            i32::from(sampled.opacity_milli),
            milli(lowest.opacity.clamp(0.0, 1.0)),
            "{context}"
        );
        assert_eq!(
            sampled.clip_milli,
            lowest.clip.map(|[low, high]| [milli(low), milli(high)]),
            "{context}"
        );
        assert_eq!(
            sampled.body,
            lowest.body.map(|body| body.entity.value()),
            "{context}"
        );
    }

    let retained: Vec<_> = state.surface_changes().copied().collect();
    let latest = &retained[retained.len().saturating_sub(MAX_RECENT_CHANGES)..];
    assert_eq!(diagnostics.recent.len(), latest.len(), "{context}");
    for (shown, change) in diagnostics.recent.iter().zip(latest) {
        assert_eq!(
            (shown.sequence, shown.tick, shown.id),
            (change.sequence, change.tick, change.id.get()),
            "{context}"
        );
        let kind = match change.kind {
            SurfaceChangeKind::Admitted { slot, cell, source } => {
                ClockSurfaceChangeKind::Admitted {
                    slot,
                    cell: [cell.x, cell.y],
                    source,
                }
            }
            SurfaceChangeKind::HandedOff { from, to } => {
                ClockSurfaceChangeKind::HandedOff { from, to }
            }
            SurfaceChangeKind::Departing => ClockSurfaceChangeKind::Departing,
            SurfaceChangeKind::Retired(reason) => ClockSurfaceChangeKind::Retired { reason },
        };
        assert_eq!(shown.change, kind, "{context}");
    }
    assert_eq!(
        diagnostics.limits,
        ClockSurfaceLimits {
            live: MAX_SURFACES,
            retained_changes: MAX_RETAINED_CHANGES,
            sample: MAX_SAMPLED_SURFACES,
            recent: MAX_RECENT_CHANGES,
        },
        "{context}"
    );
}

/// The face at rest: anchored, one surface for each lit cell, drawn as it is
/// published.
fn assert_at_rest(state: &ClockState, context: &str) {
    let surfaces = assert_face_agreement(state, context);
    assert!(
        surfaces.iter().all(|s| s.source == SurfaceSource::Anchored
            && !s.departing
            && s.opacity == 1.0
            && s.clip.is_none()),
        "{context}"
    );
    assert_eq!(
        surfaces.len(),
        lit_cells(state).iter().sum::<usize>(),
        "{context}"
    );
    assert_one_per_cell(&surfaces, context);
}

#[test]
fn surface_contract_every_event_keeps_one_lifecycle_in_every_font_and_layout() {
    for (index, kind) in TIMED.into_iter().enumerate() {
        for (view, aspect) in ASPECTS.into_iter().enumerate() {
            let font = FONTS[(index + view) % FONTS.len()];
            let format = if (index + view) % 2 == 0 {
                ClockTimeFormat::TwelveHour
            } else {
                ClockTimeFormat::TwentyFourHour
            };
            let context = format!("{} in {font:?} at {aspect}", kind.as_str());
            let mut state = clock(aspect, font, format, reading(19, 59, 50));
            let mut lifecycle = Lifecycle::new(&state);
            sync(&mut state, &[ClockAction::preview_event(kind)]);
            assert_eq!(state.event_kind(), Some(kind), "{context}");
            let limit = 2 * EVENT_CATALOG[kind as usize].duration_ticks + 600;
            let mut ticks = 0;
            while state.event_kind() == Some(kind) {
                assert!(ticks < limit, "{context}: the event ends");
                let context = format!("{context}, tick {ticks}");
                // The reading moves on twice, once to a new hour.
                let actions = match ticks {
                    30 => vec![ClockAction::set_reading(reading(20, 0, 0))],
                    200 => vec![ClockAction::set_reading(reading(20, 1, 0))],
                    _ => Vec::new(),
                };
                tick(&mut state, &actions);
                ticks += 1;
                let surfaces = lifecycle.read(&state, &context);
                assert_diagnostics(&state, &surfaces, &context);
                if matches!(
                    kind,
                    ClockEventKind::ColorCycle | ClockEventKind::Rain | ClockEventKind::Aquarium
                ) {
                    assert!(
                        surfaces.iter().all(|s| s.source == SurfaceSource::Anchored),
                        "{context}: the face stays where it is"
                    );
                }
                if ticks % 7 == 0 {
                    assert_agreement(&state, &context, |_, _| {
                        // The texture of a reel and the ghost of a clock
                        // marquee are drawn without surfaces. Their own
                        // tests hold them.
                        matches!(kind, ClockEventKind::SlotMachine | ClockEventKind::Marquee)
                    });
                }
            }
            assert_at_rest(&state, &context);
            let surfaces = lifecycle.read(&state, &context);
            assert_diagnostics(&state, &surfaces, &context);
            // The face shows the last reading the event saw.
            let minute = match ticks {
                0..=30 => 9,
                31..=200 => 0,
                _ => 1,
            };
            assert_eq!(state.display().digits[3], Some(minute), "{context}");
        }
    }
}

#[test]
fn surface_contract_a_resize_ends_an_event_before_anything_reads_it() {
    let events = [
        (ClockEventKind::DigitSlide, 10),
        (ClockEventKind::Falling, 100),
        (ClockEventKind::Explosion, 100),
        (ClockEventKind::Meltdown, 100),
        (ClockEventKind::Marquee, 100),
        (ClockEventKind::CellSwap, 20),
        (ClockEventKind::SlotMachine, 100),
    ];
    for (kind, run) in events {
        for (index, aspect) in ASPECTS.into_iter().enumerate() {
            let resized = ASPECTS[(index + 1) % ASPECTS.len()];
            let context = format!("{} from {aspect} to {resized}", kind.as_str());
            let mut state = clock(
                aspect,
                FONTS[index],
                ClockTimeFormat::TwentyFourHour,
                reading(4, 39, 0),
            );
            let mut lifecycle = Lifecycle::new(&state);
            sync(&mut state, &[ClockAction::preview_event(kind)]);
            for _ in 0..run {
                tick(&mut state, &[]);
                lifecycle.read(&state, &context);
            }
            let before = state.digit_surfaces();
            let sequence = state.surface_accounting().sequence;
            state.set_aspect_ratio(resized);
            assert_eq!(state.event_kind(), None, "{context}");
            // Read at once, before another step.
            let surfaces = lifecycle.read(&state, &context);
            assert_diagnostics(&state, &surfaces, &context);
            assert_at_rest(&state, &context);
            assert!(
                state.surface_accounting().sequence > sequence,
                "{context}: the resize gave the face back"
            );
            for surface in before.iter().filter(|s| s.departing) {
                assert!(surfaces.iter().all(|s| s.id != surface.id), "{context}");
            }
            let sequence = state.surface_accounting().sequence;
            sync(&mut state, &[]);
            assert_eq!(state.digit_surfaces(), surfaces, "{context}");
            assert_eq!(state.surface_accounting().sequence, sequence, "{context}");
        }
    }
}

#[test]
fn surface_contract_visitors_leave_the_face_where_it_is() {
    for kind in [ClockEventKind::Duck, ClockEventKind::Crow] {
        for aspect in ASPECTS {
            let context = format!("{} at {aspect}", kind.as_str());
            let mut state = clock(
                aspect,
                ClockFont::Classic,
                ClockTimeFormat::TwentyFourHour,
                reading(7, 45, 0),
            );
            let face = state.digit_surfaces();
            let mut lifecycle = Lifecycle::new(&state);
            sync(&mut state, &[ClockAction::preview_event(kind)]);
            for ticks in 0..360 {
                let context = format!("{context}, tick {ticks}");
                tick(&mut state, &[]);
                let surfaces = lifecycle.read(&state, &context);
                assert_eq!(surfaces, face, "{context}");
                if ticks % 30 == 0 {
                    assert_diagnostics(&state, &surfaces, &context);
                    assert_at_rest(&state, &context);
                }
            }
        }
    }
}

#[test]
fn surface_contract_an_event_cut_short_before_it_moves_gives_back_what_rests_at_home() {
    for kind in TIMED {
        for (index, aspect) in ASPECTS.into_iter().enumerate() {
            let font = FONTS[(kind as usize + index) % FONTS.len()];
            let resized = ASPECTS[(index + 1) % ASPECTS.len()];
            let context = format!("{} in {font:?} at {aspect}", kind.as_str());
            let mut state = clock(
                aspect,
                font,
                ClockTimeFormat::TwentyFourHour,
                reading(12, 34, 0),
            );
            let mut lifecycle = Lifecycle::new(&state);
            sync(&mut state, &[ClockAction::preview_event(kind)]);
            assert_eq!(state.event_kind(), Some(kind), "{context}");
            let held = lifecycle.read(&state, &context);
            // A host that resizes before the event's first tick ends it there.
            state.set_aspect_ratio(resized);
            assert_eq!(state.event_kind(), None, "{context}");
            let surfaces = lifecycle.read(&state, &context);
            assert_at_rest(&state, &context);
            let retired: BTreeMap<_, _> = lifecycle
                .log
                .iter()
                .filter_map(|change| match change.kind {
                    SurfaceChangeKind::Retired(reason) => Some((change.id, reason)),
                    _ => None,
                })
                .collect();
            // A fresh face, so each cell rests where it was admitted.
            let layout = Layout::new(aspect);
            for surface in &held {
                let context = format!("{context}: surface {}", surface.id.get());
                let kept = surfaces.iter().any(|s| s.id == surface.id);
                let home = layout.slot_cell_center(usize::from(surface.slot), surface.cell);
                if surface.departing {
                    // It keeps its own reason wherever it stands.
                    assert!(!kept, "{context}");
                    assert!(
                        retired
                            .get(&surface.id)
                            .is_some_and(|reason| *reason != Retirement::Interrupted),
                        "{context}: {:?}",
                        retired.get(&surface.id)
                    );
                } else if (surface.origin - home).length() < 1e-3 * layout.pitch
                    && surface.angle.abs() < 1e-4
                    && (surface.scale - layout.pitch).abs() < 1e-3 * layout.pitch
                {
                    assert!(kept, "{context}: it rests at home and is handed back");
                } else {
                    assert_eq!(
                        retired.get(&surface.id),
                        Some(&Retirement::Interrupted),
                        "{context}: it was away from home"
                    );
                }
            }
        }
    }
}

#[test]
fn surface_contract_a_reading_or_setting_changed_while_paused_is_published_at_once() {
    // Readings that change every slot, and between them the hour's format and
    // the font, each applied by a step that does not tick, as a paused host
    // applies them.
    let readings = [
        reading(13, 46, 0),
        reading(20, 57, 0),
        reading(9, 8, 0),
        reading(17, 29, 0),
    ];
    for (index, kind) in TIMED.into_iter().enumerate() {
        for (view, font) in FONTS.into_iter().enumerate() {
            let aspect = ASPECTS[(index + view) % ASPECTS.len()];
            let context = format!("{} in {font:?} at {aspect}", kind.as_str());
            let mut state = clock(
                aspect,
                font,
                ClockTimeFormat::TwentyFourHour,
                reading(12, 34, 0),
            );
            let mut lifecycle = Lifecycle::new(&state);
            sync(&mut state, &[ClockAction::preview_event(kind)]);
            let limit = 2 * EVENT_CATALOG[kind as usize].duration_ticks + 600;
            let mut ticks = 0;
            let mut pauses = 0;
            while state.event_kind() == Some(kind) {
                assert!(ticks < limit, "{context}: the event ends");
                if ticks % 23 == 11 {
                    let actions = if pauses % 3 == 2 {
                        let mut settings = state.settings();
                        settings.time_format = match settings.time_format {
                            ClockTimeFormat::TwelveHour => ClockTimeFormat::TwentyFourHour,
                            ClockTimeFormat::TwentyFourHour => ClockTimeFormat::TwelveHour,
                        };
                        settings.fonts.selected = FONTS[(view + pauses) % FONTS.len()];
                        ClockAction::configure(settings)
                    } else {
                        ClockAction::set_reading(readings[pauses % readings.len()])
                    };
                    pauses += 1;
                    sync(&mut state, &[actions]);
                    let context = format!("{context}, paused at tick {ticks}");
                    let surfaces = lifecycle.read(&state, &context);
                    assert_diagnostics(&state, &surfaces, &context);
                    assert_agreement(&state, &context, |_, _| {
                        matches!(kind, ClockEventKind::SlotMachine | ClockEventKind::Marquee)
                    });
                }
                tick(&mut state, &[]);
                ticks += 1;
                lifecycle.read(&state, &format!("{context}, tick {ticks}"));
            }
            assert!(pauses > 0, "{context}");
            assert_at_rest(&state, &context);
        }
    }
}

/// One run of events, each cut short by the next or run out, with pauses and
/// readings between. Returns every change it recorded and the face it left.
fn chain(aspect: f32, font: ClockFont) -> (Vec<SurfaceChange>, Vec<DigitSurface>) {
    // Each event and the ticks it runs before the next is previewed over it.
    // Repeats start from whatever the one before left.
    let script = [
        (ClockEventKind::DigitSlide, 5),
        (ClockEventKind::Explosion, 70),
        (ClockEventKind::Explosion, 260),
        (ClockEventKind::CellSwap, 30),
        (ClockEventKind::CellSwap, 75),
        (ClockEventKind::Falling, 90),
        (ClockEventKind::SlotMachine, 100),
        (ClockEventKind::Meltdown, 150),
        (ClockEventKind::Marquee, 40),
        (ClockEventKind::Falling, 450),
        (ClockEventKind::Meltdown, 500),
        (ClockEventKind::SlotMachine, 0),
        (ClockEventKind::Marquee, 1),
        (ClockEventKind::DigitSlide, u64::MAX),
    ];
    let mut state = clock(
        aspect,
        font,
        ClockTimeFormat::TwelveHour,
        reading(11, 58, 0),
    );
    let mut lifecycle = Lifecycle::new(&state);
    let mut minute = 58;
    for (step, (kind, run)) in script.into_iter().enumerate() {
        let context = format!("{font:?} at {aspect}, {} (step {step})", kind.as_str());
        sync(&mut state, &[ClockAction::preview_event(kind)]);
        assert_eq!(state.event_kind(), Some(kind), "{context}");
        lifecycle.read(&state, &context);
        let mut ticks = 0;
        while ticks < run && state.event_kind().is_some() {
            let context = format!("{context}, tick {ticks}");
            let actions = if ticks % 61 == 17 {
                minute = (minute + 1) % 60;
                let hour = if minute < 58 { 12 } else { 11 };
                vec![ClockAction::set_reading(reading(hour, minute, 0))]
            } else {
                Vec::new()
            };
            tick(&mut state, &actions);
            ticks += 1;
            let surfaces = lifecycle.read(&state, &context);
            if ticks % 13 == 0 {
                assert_diagnostics(&state, &surfaces, &context);
            }
        }
        // Paused, nothing moves and nothing is recorded.
        let held = state.digit_surfaces();
        let sequence = state.surface_accounting().sequence;
        for _ in 0..3 {
            sync(&mut state, &[]);
        }
        assert_eq!(state.digit_surfaces(), held, "{context}");
        assert_eq!(state.surface_accounting().sequence, sequence, "{context}");
    }
    assert_eq!(state.event_kind(), None);
    let context = format!("{font:?} at {aspect}, after the run");
    assert_at_rest(&state, &context);
    let surfaces = lifecycle.read(&state, &context);
    assert_diagnostics(&state, &surfaces, &context);
    let accounting = state.surface_accounting();
    for reason in [
        Retirement::Replaced,
        Retirement::Interrupted,
        Retirement::Melted,
    ] {
        assert!(accounting.retired(reason) > 0, "{context}: {reason:?}");
    }
    (lifecycle.log, surfaces)
}

#[test]
fn surface_contract_events_cut_short_by_each_other_keep_one_lifecycle_and_replay() {
    for (index, aspect) in ASPECTS.into_iter().enumerate() {
        let font = FONTS[index];
        let (log, face) = chain(aspect, font);
        let (replay, again) = chain(aspect, font);
        assert_eq!(replay, log, "{font:?} at {aspect}: the same lifecycle");
        assert_eq!(again, face, "{font:?} at {aspect}: the same face");
    }
}
