//! The digit surface contract: who the lit cells are, where they are drawn and
//! what becomes of a marker attached to one. Every check reads the rendered
//! frame, so a published pose that the face does not actually draw fails here.

use super::*;
use crate::render::{
    ACTIVE_CELL_LAYER, digit_cell_polygons, filled_polygons_with_alpha, render_frame,
};
use engine_common::{ClockEventKind, ClockFont, ClockTimeFormat};
use engine_core::Vec2;

mod cell_swap;
mod marquee;
mod slot_machine;
mod sweep;

pub(crate) const FONTS: [ClockFont; 4] = [
    ClockFont::Classic,
    ClockFont::Matrix,
    ClockFont::Sans,
    ClockFont::Serif,
];
/// A wide, a narrow and a portrait view.
pub(crate) const ASPECTS: [f32; 3] = [4.0, 4.0 / 3.0, 0.6];
const TICK: Duration = Duration::from_nanos(1_000_000_000 / 60);

pub(crate) fn reading(hour: u8, minute: u8, second: u8) -> ClockReading {
    ClockReading::new(hour, minute, second).expect("a valid reading")
}

pub(crate) fn config(aspect: f32, font: ClockFont, format: ClockTimeFormat) -> ClockConfig {
    ClockConfig {
        aspect_ratio: aspect,
        event_profile: ClockEventProfile::Off,
        time_format: format,
        fonts: ClockFontSettings {
            selected: font,
            ..ClockFontSettings::default()
        },
        ..ClockConfig::default()
    }
}

/// A synchronized face with no automatic events.
pub(crate) fn clock(
    aspect: f32,
    font: ClockFont,
    format: ClockTimeFormat,
    at: ClockReading,
) -> ClockState {
    let mut state = ClockScenario::init(config(aspect, font, format), 7);
    sync(&mut state, &[ClockAction::set_reading(at)]);
    state
}

/// Control without time passing, as a paused host delivers it.
pub(crate) fn sync(state: &mut ClockState, actions: &[Action]) {
    ClockScenario::step(state, actions, Duration::ZERO);
}

pub(crate) fn tick(state: &mut ClockState, actions: &[Action]) {
    ClockScenario::step(state, actions, TICK);
}

pub(crate) fn ids(surfaces: &[DigitSurface]) -> Vec<SurfaceId> {
    surfaces.iter().map(|surface| surface.id).collect()
}

/// The lit cells a reading draws, slot by slot.
pub(crate) fn lit_cells(state: &ClockState) -> [usize; DIGIT_SLOT_COUNT] {
    let display = state.display();
    std::array::from_fn(|slot| {
        crate::fonts::glyph(display.font, display.digits[slot])
            .cells()
            .count()
    })
}

pub(crate) fn inside(polygon: &[Vec2], point: Vec2, tolerance: f32) -> bool {
    let area: f32 = (0..polygon.len())
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    let winding = area.signum();
    (0..polygon.len()).all(|i| {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        let edge = b - a;
        let cross = edge.x * (point.y - a.y) - edge.y * (point.x - a.x);
        cross * winding >= -tolerance * edge.length()
    })
}

/// Surfaces sharing one drawn cell sum to its ink.
fn shared_opacity(surfaces: &[DigitSurface], surface: &DigitSurface) -> f32 {
    surfaces
        .iter()
        .filter(|other| {
            (other.origin - surface.origin).length() <= 1e-3 * surface.scale
                && (other.angle - surface.angle).abs() <= 1e-4
                && other.footprint == surface.footprint
                && other.clip == surface.clip
        })
        .map(|other| other.opacity)
        .sum::<f32>()
        .min(1.0)
}

/// What the published surfaces and the rendered frame have to agree on: every
/// visible surface is drawn where it says, with the ink it says, and every
/// lit digit cell drawn belongs to a surface unless `unowned`, given its
/// centroid and ink, says the face draws it without one.
pub(crate) fn assert_agreement(
    state: &ClockState,
    context: &str,
    unowned: impl Fn(Vec2, f32) -> bool,
) -> Vec<DigitSurface> {
    let surfaces = state.digit_surfaces();
    let mut seen = ids(&surfaces);
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), surfaces.len(), "{context}: one surface per id");
    let layout = Layout::new(state.aspect_ratio());
    let drawn = filled_polygons_with_alpha(&render_frame(state), ACTIVE_CELL_LAYER);
    for surface in &surfaces {
        assert!(surface.scale > 0.0 && surface.opacity >= 0.0);
        let ink = shared_opacity(&surfaces, surface);
        let Some(footprint) = surface.visible() else {
            continue;
        };
        if ink < 0.02 {
            continue;
        }
        let centre = (footprint.min + footprint.max) * 0.5;
        let samples = [
            centre,
            centre + (footprint.min - centre) * 0.75,
            centre + (footprint.max - centre) * 0.75,
            centre + (Vec2::new(footprint.min.x, footprint.max.y) - centre) * 0.75,
            centre + (Vec2::new(footprint.max.x, footprint.min.y) - centre) * 0.75,
        ];
        for sample in samples {
            let point = surface.world(sample);
            assert!(
                drawn
                    .iter()
                    .any(|(polygon, _)| inside(polygon, point, 1e-3 * layout.pitch)),
                "{context}: surface {} ({:?}) is not drawn at {point:?}",
                surface.id.get(),
                surface.source
            );
        }
        let point = surface.world(centre);
        assert!(
            drawn.iter().any(|(polygon, alpha)| {
                (alpha - ink).abs() <= 0.02 && inside(polygon, point, 1e-3 * layout.pitch)
            }),
            "{context}: surface {} is drawn without its ink {ink}",
            surface.id.get()
        );
    }
    let slack = (layout.pitch * 0.045).max(0.8) + 0.02 * layout.pitch;
    for (polygon, alpha) in digit_cell_polygons(state) {
        if alpha < 0.02 {
            continue;
        }
        let centroid = polygon.iter().fold(Vec2::ZERO, |sum, p| sum + *p) / polygon.len() as f32;
        if unowned(centroid, alpha) {
            continue;
        }
        let owned = surfaces.iter().any(|surface| {
            let Some(footprint) = (surface.opacity > 0.0).then(|| surface.visible()).flatten()
            else {
                return false;
            };
            let local = surface.local(centroid);
            let margin = slack / surface.scale;
            local.x >= footprint.min.x - margin
                && local.x <= footprint.max.x + margin
                && local.y >= footprint.min.y - margin
                && local.y <= footprint.max.y + margin
        });
        assert!(
            owned,
            "{context}: a lit cell drawn at {centroid:?} belongs to no surface"
        );
    }
    surfaces
}

pub(crate) fn assert_face_agreement(state: &ClockState, context: &str) -> Vec<DigitSurface> {
    assert_agreement(state, context, |_, _| false)
}

/// Why a marker came off its surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Loss {
    Retired(Retirement),
    /// Its surface no longer covers the point it was attached at.
    Unsupported,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Marker {
    pub surface: SurfaceId,
    pub local: Vec2,
    pub world: Vec2,
    /// How far it moved on the last step it followed.
    pub moved: f32,
    pub lost: Option<Loss>,
}

/// The attachment-marker fixture: a stand-in resident that keeps a local point
/// on one surface and follows only what the contract publishes. It reads the
/// lifecycle changes each step and never sees the event that moves its piece.
pub(crate) struct Markers {
    pub markers: Vec<Marker>,
    sequence: u64,
    /// Hand-offs seen for each marked surface, as (from, to).
    pub handoffs: Vec<(SurfaceId, SurfaceSource, SurfaceSource)>,
    pub departed: Vec<SurfaceId>,
}

impl Markers {
    /// One marker on each eligible surface `pick` chooses, at the local point
    /// it returns.
    pub fn attach(state: &ClockState, pick: impl Fn(&DigitSurface) -> Option<Vec2>) -> Self {
        let markers = state
            .digit_surfaces()
            .iter()
            .filter(|surface| surface.eligible())
            .filter_map(|surface| {
                let local = pick(surface)?;
                assert!(
                    surface.supports(local),
                    "a marker attaches on its footprint"
                );
                Some(Marker {
                    surface: surface.id,
                    local,
                    world: surface.world(local),
                    moved: 0.0,
                    lost: None,
                })
            })
            .collect();
        Self {
            markers,
            sequence: state.surface_accounting().sequence,
            handoffs: Vec::new(),
            departed: Vec::new(),
        }
    }

    pub fn attached(&self) -> impl Iterator<Item = &Marker> {
        self.markers.iter().filter(|marker| marker.lost.is_none())
    }

    pub fn on(&self, id: SurfaceId) -> bool {
        self.attached().any(|marker| marker.surface == id)
    }

    /// Follows one step. Returns the surfaces as published.
    pub fn follow(&mut self, state: &ClockState) -> Vec<DigitSurface> {
        let changes: Vec<_> = state.surface_changes().copied().collect();
        if let Some(first) = changes.first() {
            assert!(
                first.sequence <= self.sequence + 1,
                "the fixture reads every step, so it never misses a change"
            );
        }
        for change in changes.iter().filter(|c| c.sequence > self.sequence) {
            let marked = self
                .markers
                .iter()
                .any(|m| m.surface == change.id && m.lost.is_none());
            match change.kind {
                SurfaceChangeKind::Retired(reason) => {
                    for marker in &mut self.markers {
                        if marker.surface == change.id && marker.lost.is_none() {
                            marker.lost = Some(Loss::Retired(reason));
                        }
                    }
                }
                SurfaceChangeKind::HandedOff { from, to } if marked => {
                    self.handoffs.push((change.id, from, to));
                }
                SurfaceChangeKind::Departing if marked => self.departed.push(change.id),
                _ => (),
            }
        }
        self.sequence = state.surface_accounting().sequence;
        let surfaces = state.digit_surfaces();
        for marker in self.markers.iter_mut().filter(|m| m.lost.is_none()) {
            let surface = surfaces
                .iter()
                .find(|surface| surface.id == marker.surface)
                .expect("a surface that was not retired is published");
            let world = surface.world(marker.local);
            marker.moved = (world - marker.world).length();
            marker.world = world;
            if !surface.supports(marker.local) {
                marker.lost = Some(Loss::Unsupported);
            }
        }
        surfaces
    }
}

#[test]
fn surface_contract_the_face_publishes_its_drawn_lit_cells() {
    for font in FONTS {
        for aspect in ASPECTS {
            for format in [ClockTimeFormat::TwentyFourHour, ClockTimeFormat::TwelveHour] {
                let state = clock(aspect, font, format, reading(13, 48, 20));
                let context = format!("{font:?} at {aspect} in {format:?}");
                let surfaces = assert_face_agreement(&state, &context);
                let layout = Layout::new(aspect);
                assert_eq!(
                    surfaces.len(),
                    lit_cells(&state).iter().sum::<usize>(),
                    "{context}"
                );
                for surface in &surfaces {
                    assert!(surface.eligible() && !surface.departing, "{context}");
                    assert_eq!(surface.source, SurfaceSource::Anchored, "{context}");
                    assert_eq!(surface.footprint, Footprint::CELL, "{context}");
                    assert_eq!(surface.scale, layout.pitch, "{context}");
                    assert_eq!(
                        surface.origin,
                        layout.slot_cell_center(usize::from(surface.slot), surface.cell),
                        "{context}"
                    );
                    assert_eq!(surface.body, None, "{context}");
                }
                let accounting = state.surface_accounting();
                assert_eq!(accounting.admitted as usize, surfaces.len());
                assert_eq!(accounting.live, surfaces.len());
                assert_eq!(accounting.total_retired(), 0);
            }
        }
    }
}

#[test]
fn surface_contract_seconds_the_colon_and_colour_keep_every_identity() {
    let mut state = clock(
        800.0 / 480.0,
        ClockFont::Classic,
        ClockTimeFormat::TwentyFourHour,
        reading(12, 34, 50),
    );
    let mut markers = Markers::attach(&state, |_| Some(Vec2::new(0.1, -0.2)));
    let before = state.digit_surfaces();
    let sequence = state.surface_accounting().sequence;
    for second in 51..60 {
        tick(
            &mut state,
            &[ClockAction::set_reading(reading(12, 34, second))],
        );
        markers.follow(&state);
        assert_eq!(state.digit_surfaces(), before, "second {second}");
    }
    tick(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
    );
    assert_eq!(state.event_kind(), Some(ClockEventKind::ColorCycle));
    for _ in 0..COLOR_CYCLE_TICKS + 2 {
        tick(&mut state, &[]);
        markers.follow(&state);
        assert_face_agreement(&state, "colour cycle");
        assert_eq!(ids(&state.digit_surfaces()), ids(&before));
    }
    assert_eq!(state.event_kind(), None);
    assert_eq!(
        state.surface_accounting().sequence,
        sequence,
        "the colon, seconds and a palette record nothing"
    );
    assert_eq!(markers.attached().count(), before.len());
    assert!(markers.attached().all(|marker| marker.moved == 0.0));
}

#[test]
fn surface_contract_a_snapped_reading_replaces_only_the_changed_glyphs() {
    for font in FONTS {
        let mut state = clock(
            800.0 / 480.0,
            font,
            ClockTimeFormat::TwentyFourHour,
            reading(12, 34, 59),
        );
        let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
        let before = state.digit_surfaces();
        // Profile Off has no Digit Slide, so the new minute snaps in place.
        tick(&mut state, &[ClockAction::set_reading(reading(12, 35, 0))]);
        let after = markers.follow(&state);
        assert_face_agreement(&state, &format!("{font:?}"));
        for surface in &before {
            let kept = after.iter().any(|other| other.id == surface.id);
            assert_eq!(kept, surface.slot != 3, "{font:?}: slot {}", surface.slot);
            assert_eq!(markers.on(surface.id), kept, "{font:?}");
        }
        let replacements: Vec<_> = after.iter().filter(|surface| surface.slot == 3).collect();
        assert_eq!(replacements.len(), lit_cells(&state)[3], "{font:?}");
        assert!(
            replacements
                .iter()
                .all(|surface| !markers.markers.iter().any(|m| m.surface == surface.id)),
            "{font:?}: a replacement carries no marker"
        );
        let lost: Vec<_> = markers
            .markers
            .iter()
            .filter_map(|marker| marker.lost)
            .collect();
        assert!(
            !lost.is_empty()
                && lost
                    .iter()
                    .all(|loss| *loss == Loss::Retired(Retirement::Replaced))
        );
    }
}

#[test]
fn surface_contract_font_and_format_changes_replace_their_glyphs() {
    let mut state = clock(
        800.0 / 480.0,
        ClockFont::Classic,
        ClockTimeFormat::TwentyFourHour,
        reading(13, 5, 0),
    );
    let before = state.digit_surfaces();
    let mut settings = state.settings();
    settings.time_format = ClockTimeFormat::TwelveHour;
    sync(&mut state, &[ClockAction::configure(settings)]);
    let after = assert_face_agreement(&state, "twelve hour");
    // 13:05 becomes 1:05 PM, so the hour's two slots change and the minute's
    // two keep their surfaces.
    for surface in &before {
        let kept = after.iter().any(|other| other.id == surface.id);
        assert_eq!(kept, surface.slot >= 2, "slot {}", surface.slot);
    }
    let before = after;
    settings.fonts.selected = ClockFont::Serif;
    sync(&mut state, &[ClockAction::configure(settings)]);
    assert_eq!(state.active_font(), ClockFont::Serif);
    let after = assert_face_agreement(&state, "serif");
    assert!(
        after
            .iter()
            .all(|surface| !before.iter().any(|old| old.id == surface.id)),
        "a font change replaces every glyph"
    );
    assert_eq!(
        state.surface_accounting().retired(Retirement::Replaced) as usize,
        before.len()
            + lit_cells(&clock(
                800.0 / 480.0,
                ClockFont::Classic,
                ClockTimeFormat::TwentyFourHour,
                reading(13, 5, 0)
            ))[..2]
                .iter()
                .sum::<usize>()
    );
}

#[test]
fn surface_contract_a_resize_keeps_identities_and_local_points() {
    let mut state = clock(
        ASPECTS[0],
        ClockFont::Matrix,
        ClockTimeFormat::TwentyFourHour,
        reading(9, 41, 3),
    );
    let local = Vec2::new(-0.3, 0.25);
    let mut markers = Markers::attach(&state, |_| Some(local));
    let before = ids(&state.digit_surfaces());
    for aspect in [ASPECTS[1], ASPECTS[2], ASPECTS[0]] {
        state.set_aspect_ratio(aspect);
        sync(&mut state, &[]);
        let surfaces = assert_face_agreement(&state, &format!("{aspect}"));
        markers.follow(&state);
        assert_eq!(ids(&surfaces), before, "{aspect}");
        let layout = Layout::new(aspect);
        for marker in markers.attached() {
            let surface = surfaces.iter().find(|s| s.id == marker.surface).unwrap();
            let expected = layout.slot_cell_center(usize::from(surface.slot), surface.cell)
                + local * layout.pitch;
            assert!((marker.world - expected).length() < 1e-3, "{aspect}");
        }
    }
    assert_eq!(markers.attached().count(), before.len());
}

#[test]
fn surface_contract_a_paused_face_records_nothing() {
    let mut state = clock(
        800.0 / 480.0,
        ClockFont::Sans,
        ClockTimeFormat::TwelveHour,
        reading(22, 17, 30),
    );
    let before = state.digit_surfaces();
    let sequence = state.surface_accounting().sequence;
    for _ in 0..5 {
        sync(&mut state, &[]);
        sync(&mut state, &[ClockAction::set_reading(reading(22, 17, 31))]);
    }
    assert_eq!(state.digit_surfaces(), before);
    assert_eq!(state.surface_accounting().sequence, sequence);
}

#[test]
fn surface_contract_a_restart_admits_a_new_face_and_replays_identically() {
    let run = || {
        let mut state = clock(
            800.0 / 480.0,
            ClockFont::Classic,
            ClockTimeFormat::TwentyFourHour,
            reading(10, 59, 58),
        );
        for (minute, second) in [(59, 59), (0, 0), (0, 1), (1, 0)] {
            let hour = if minute == 59 { 10 } else { 11 };
            tick(
                &mut state,
                &[ClockAction::set_reading(reading(hour, minute, second))],
            );
        }
        let changes: Vec<_> = state.surface_changes().copied().collect();
        (state.digit_surfaces(), changes)
    };
    let (surfaces, changes) = run();
    assert_eq!(changes.first().map(|c| c.id.get()), Some(1));
    assert!(matches!(
        changes.first().map(|c| c.kind),
        Some(SurfaceChangeKind::Admitted { .. })
    ));
    let (again, replay) = run();
    assert_eq!(
        again, surfaces,
        "a restart is a new run with the same identities"
    );
    assert_eq!(
        replay, changes,
        "a fixed input sequence records the same lifecycle"
    );
}

/// No automatic event but the one a test is about.
pub(crate) fn only(kind: ClockEventKind) -> ClockEvents {
    ClockEvents {
        falling: kind == ClockEventKind::Falling,
        color_cycle: kind == ClockEventKind::ColorCycle,
        meltdown: kind == ClockEventKind::Meltdown,
        duck: kind == ClockEventKind::Duck,
        marquee: kind == ClockEventKind::Marquee,
        digit_slide: kind == ClockEventKind::DigitSlide,
        rain: kind == ClockEventKind::Rain,
        crow: kind == ClockEventKind::Crow,
        explosion: kind == ClockEventKind::Explosion,
        aquarium: kind == ClockEventKind::Aquarium,
        cell_swap: kind == ClockEventKind::CellSwap,
        grass: kind == ClockEventKind::Grass,
        slot_machine: kind == ClockEventKind::SlotMachine,
        spider: kind == ClockEventKind::Spider,
    }
}

/// A face whose minute changes slide, and nothing else happens on its own.
fn sliding(aspect: f32, font: ClockFont, at: ClockReading) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Calm,
            events: only(ClockEventKind::DigitSlide),
            ..config(aspect, font, ClockTimeFormat::TwentyFourHour)
        },
        7,
    );
    sync(&mut state, &[ClockAction::set_reading(at)]);
    state
}

#[test]
fn surface_contract_an_outgoing_digit_carries_its_markers_out_and_the_new_one_is_new() {
    for font in FONTS {
        for aspect in ASPECTS {
            let context = format!("{font:?} at {aspect}");
            let mut state = sliding(aspect, font, reading(12, 34, 59));
            let layout = Layout::new(aspect);
            let local = Vec2::new(0.2, -0.25);
            let mut markers = Markers::attach(&state, |_| Some(local));
            let before = state.digit_surfaces();
            tick(&mut state, &[ClockAction::set_reading(reading(12, 35, 0))]);
            assert_eq!(
                state.event_kind(),
                Some(ClockEventKind::DigitSlide),
                "{context}"
            );
            let opening = markers.follow(&state);
            let incoming: Vec<_> = opening
                .iter()
                .filter(|surface| surface.slot == 3 && !surface.departing)
                .map(|surface| surface.id)
                .collect();
            assert_eq!(incoming.len(), lit_cells(&state)[3], "{context}");
            assert!(
                incoming
                    .iter()
                    .all(|id| before.iter().all(|old| old.id != *id)),
                "{context}: the new glyph is new surfaces where it overlaps the old"
            );
            let mut lost = 0;
            for step in 1..=DIGIT_SLIDE_TICKS {
                let surfaces = assert_face_agreement(&state, &format!("{context}, tick {step}"));
                let [outgoing, _] = match &state.active_event {
                    Some(crate::events::ActiveEvent::DigitSlide(event)) => event.offsets(),
                    _ => [-9.0, 0.0],
                };
                for marker in &markers.markers {
                    let old = before.iter().find(|s| s.id == marker.surface).unwrap();
                    let home = layout.slot_cell_center(usize::from(old.slot), old.cell)
                        + local * layout.pitch;
                    if old.slot != 3 {
                        assert!(
                            marker.lost.is_none(),
                            "{context}: unchanged digits keep theirs"
                        );
                        assert_eq!(marker.world, home, "{context}");
                        continue;
                    }
                    match marker.lost {
                        None => {
                            let surface = surfaces.iter().find(|s| s.id == marker.surface).unwrap();
                            assert!(surface.departing, "{context}");
                            assert_eq!(surface.source, SurfaceSource::Slide, "{context}");
                            let expected = home + Vec2::new(0.0, outgoing * layout.pitch);
                            assert!(
                                (marker.world - expected).length() < 1e-3,
                                "{context}: the marker keeps its local point on the slide"
                            );
                            assert!(
                                surface.origin.y + 0.4 * layout.pitch > layout.digit_band()[0],
                                "{context}: a cell is retired as soon as it is out"
                            );
                        }
                        Some(loss) => {
                            assert_eq!(loss, Loss::Retired(Retirement::Replaced), "{context}");
                            let cell_top = layout.slot_cell_center(3, old.cell).y
                                + (0.4 + outgoing) * layout.pitch;
                            assert!(
                                cell_top <= layout.digit_band()[0] + 1e-3,
                                "{context}: a marker leaves only with its cell out of view"
                            );
                        }
                    }
                }
                lost = markers.markers.iter().filter(|m| m.lost.is_some()).count();
                tick(&mut state, &[]);
                markers.follow(&state);
            }
            assert_eq!(state.event_kind(), None, "{context}");
            let after = assert_face_agreement(&state, &context);
            assert_eq!(
                lost,
                before.iter().filter(|s| s.slot == 3).count(),
                "{context}: every outgoing marker left with its digit"
            );
            let mut settled: Vec<_> = after
                .iter()
                .filter(|surface| surface.slot == 3)
                .map(|surface| surface.id)
                .collect();
            settled.sort_unstable();
            let mut incoming = incoming.clone();
            incoming.sort_unstable();
            assert_eq!(
                settled, incoming,
                "{context}: the slid-in glyph is the face"
            );
            assert!(after.iter().all(|s| s.source == SurfaceSource::Anchored));
            assert!(
                after
                    .iter()
                    .filter(|s| s.slot != 3)
                    .all(|s| before.iter().any(|old| old.id == s.id)),
                "{context}"
            );
        }
    }
}

#[test]
fn surface_contract_a_slide_cut_short_retires_its_pieces_instead_of_snapping_them() {
    let mut state = sliding(800.0 / 480.0, ClockFont::Sans, reading(12, 34, 59));
    let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
    tick(&mut state, &[ClockAction::set_reading(reading(12, 35, 0))]);
    markers.follow(&state);
    let first = state.digit_surfaces();
    let first_incoming: Vec<_> = first
        .iter()
        .filter(|s| s.slot == 3 && !s.departing)
        .map(|s| s.id)
        .collect();
    for _ in 0..20 {
        tick(&mut state, &[]);
        markers.follow(&state);
    }
    // The next minute arrives mid-slide: the slide ends at once, and the
    // cooldown it leaves lets the newer reading snap in.
    tick(&mut state, &[ClockAction::set_reading(reading(12, 36, 0))]);
    let surfaces = markers.follow(&state);
    assert_eq!(state.event_kind(), None);
    assert_face_agreement(&state, "after the slide was cut short");
    let changes: Vec<_> = state.surface_changes().copied().collect();
    for id in &first_incoming {
        assert!(
            changes
                .iter()
                .any(|c| c.id == *id
                    && c.kind == SurfaceChangeKind::Retired(Retirement::Interrupted)),
            "a glyph cut off part way in is retired, not carried home"
        );
        assert!(surfaces.iter().all(|s| s.id != *id));
    }
    assert!(
        surfaces
            .iter()
            .filter(|s| s.slot == 3)
            .all(|s| first.iter().all(|old| old.id != s.id)),
        "the reading that snapped in is new surfaces"
    );
    let lost: Vec<_> = markers.markers.iter().filter_map(|m| m.lost).collect();
    assert_eq!(lost.len(), first.iter().filter(|s| s.departing).count());
    assert!(
        lost.iter()
            .all(|loss| *loss == Loss::Retired(Retirement::Replaced)),
        "the markers on the outgoing glyph left with it"
    );
    assert!(markers.attached().all(|m| m.moved == 0.0), "no piece jumps");
    let accounting = state.surface_accounting();
    assert_eq!(accounting.live, surfaces.len());
    assert!(accounting.peak_live <= 2 * crate::fonts::MAX_DIGIT_CELLS);
}

#[test]
fn surface_contract_a_previewed_slide_carries_the_face_out_and_brings_a_new_one_in() {
    let mut state = clock(
        800.0 / 480.0,
        ClockFont::Serif,
        ClockTimeFormat::TwelveHour,
        reading(9, 41, 0),
    );
    let mut markers = Markers::attach(&state, |_| Some(Vec2::new(-0.1, 0.1)));
    let before = state.digit_surfaces();
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::DigitSlide)],
    );
    let opening = markers.follow(&state);
    assert_face_agreement(&state, "preview");
    // A blank hour slot has nothing to slide; every other glyph leaves as
    // itself and comes back as new surfaces.
    assert_eq!(opening.iter().filter(|s| s.departing).count(), before.len());
    assert!(
        before
            .iter()
            .all(|old| opening.iter().any(|s| s.id == old.id && s.departing))
    );
    for _ in 0..DIGIT_SLIDE_TICKS {
        tick(&mut state, &[]);
        markers.follow(&state);
    }
    let after = assert_face_agreement(&state, "after the preview");
    assert_eq!(after.len(), before.len());
    assert!(
        after
            .iter()
            .all(|s| before.iter().all(|old| old.id != s.id))
    );
    assert_eq!(markers.attached().count(), 0);
}

/// Where the canonical world has a body this step.
pub(crate) fn body_pose(
    state: &ClockState,
    body: engine_rapier::world::BodyId,
) -> Option<(Vec2, f32)> {
    let world = state
        .active_event
        .as_ref()
        .and_then(|event| event.own_world())
        .or_else(|| state.arena.world())?;
    let motion = world.motion(body)?;
    Some((motion.position, motion.angle))
}

/// Ink by drawn cell: surfaces at one pose share one cell.
fn shared_ink(surfaces: &[DigitSurface]) -> Vec<(SurfaceId, f32)> {
    surfaces
        .iter()
        .map(|surface| (surface.id, shared_opacity(surfaces, surface)))
        .collect()
}

/// Checks every drawn cell's ink carries on from the last step, changing by
/// at most `limit`, whichever surfaces carry it now. Returns this step's ink.
fn assert_ink_continues(
    ink: &[(SurfaceId, f32)],
    surfaces: &[DigitSurface],
    limit: f32,
    context: &str,
) -> Vec<(SurfaceId, f32)> {
    let next = shared_ink(surfaces);
    for (id, value) in &next {
        let previous = ink.iter().find(|(old, _)| old == id).map(|(_, v)| *v);
        let co_located = surfaces
            .iter()
            .find(|s| s.id == *id)
            .map(|surface| {
                surfaces
                    .iter()
                    .filter(|other| (other.origin - surface.origin).length() < 1e-3)
                    .filter_map(|other| ink.iter().find(|(old, _)| *old == other.id))
                    .map(|(_, v)| *v)
                    .fold(0.0, f32::max)
            })
            .unwrap_or(0.0);
        let before = previous.unwrap_or(co_located);
        assert!(
            (value - before).abs() <= limit,
            "{context}: a cell's ink moves from {before} to {value}"
        );
    }
    next
}

/// Checks a rigid piece stays where its body puts it: the same offset in the
/// body's frame on every step, and the body's own turn.
fn assert_on_bodies(
    state: &ClockState,
    surfaces: &[DigitSurface],
    offsets: &mut Vec<(SurfaceId, Vec2)>,
    context: &str,
) {
    for surface in surfaces.iter().filter(|s| s.source == SurfaceSource::Rigid) {
        let body = surface.body.expect("a rigid surface names its body");
        let (position, angle) = body_pose(state, body).expect("its body is in the world");
        assert!((surface.angle - angle).abs() < 1e-5, "{context}");
        let offset = (surface.origin - position).rotate_radians(-angle);
        match offsets.iter().find(|(id, _)| *id == surface.id) {
            Some((_, first)) => assert!(
                (offset - *first).length() < 1e-3,
                "{context}: surface {} moves with its body",
                surface.id.get()
            ),
            None => offsets.push((surface.id, offset)),
        }
    }
}

#[test]
fn surface_contract_falling_pieces_ride_their_bodies_and_come_home_as_themselves() {
    for font in FONTS {
        for aspect in ASPECTS {
            let context = format!("{font:?} at {aspect}");
            let at = reading(12, 34, 10);
            let mut state = clock(aspect, font, ClockTimeFormat::TwentyFourHour, at);
            let local = Vec2::new(0.25, 0.1);
            let mut markers = Markers::attach(&state, |_| Some(local));
            let before = state.digit_surfaces();
            sync(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Falling)],
            );
            markers.follow(&state);
            let mut offsets = Vec::new();
            let mut fastest = 0.0_f32;
            let mut ticks = 0;
            while state.event_kind() == Some(ClockEventKind::Falling) {
                let surfaces = if ticks % 3 == 0 {
                    assert_face_agreement(&state, &format!("{context}, tick {ticks}"))
                } else {
                    state.digit_surfaces()
                };
                assert_on_bodies(&state, &surfaces, &mut offsets, &context);
                assert_eq!(
                    surfaces.len(),
                    before.len(),
                    "{context}: nothing added or lost"
                );
                tick(&mut state, &[]);
                markers.follow(&state);
                ticks += 1;
                let moved = markers.attached().map(|m| m.moved).fold(0.0, f32::max);
                if state.event_phase() == Some(EventPhase::Reforming) && state.phase_tick() == 0 {
                    assert!(
                        moved <= fastest + 0.01 * Layout::new(aspect).pitch,
                        "{context}: the hand-off to the return path does not jump"
                    );
                }
                fastest = moved;
            }
            assert_eq!(
                ticks as u64,
                FALLING_TICKS + REFORMING_TICKS,
                "{context}: the event ran to its end"
            );
            let after = assert_face_agreement(&state, &context);
            assert_eq!(
                ids(&after),
                ids(&before),
                "{context}: the same pieces came home"
            );
            assert_eq!(markers.attached().count(), before.len(), "{context}");
            let layout = Layout::new(aspect);
            for marker in markers.attached() {
                let surface = after.iter().find(|s| s.id == marker.surface).unwrap();
                let home = layout.slot_cell_center(usize::from(surface.slot), surface.cell);
                assert!((marker.world - (home + local * layout.pitch)).length() < 1e-3);
            }
            for surface in &before {
                let handoffs: Vec<_> = markers
                    .handoffs
                    .iter()
                    .filter(|(id, _, _)| *id == surface.id)
                    .map(|(_, from, to)| (*from, *to))
                    .collect();
                assert_eq!(
                    handoffs,
                    [
                        (SurfaceSource::Anchored, SurfaceSource::Rigid),
                        (SurfaceSource::Rigid, SurfaceSource::Return),
                        (SurfaceSource::Return, SurfaceSource::Anchored),
                    ],
                    "{context}: each hand-off is recorded exactly once"
                );
            }
        }
    }
}

#[test]
fn surface_contract_a_reading_changed_on_the_way_home_keeps_survivors_and_fades_the_rest() {
    for font in FONTS {
        let context = format!("{font:?}");
        let mut state = clock(
            800.0 / 480.0,
            font,
            ClockTimeFormat::TwentyFourHour,
            reading(12, 34, 10),
        );
        let mut markers = Markers::attach(&state, |_| Some(Vec2::new(-0.2, 0.3)));
        let fallen = state.digit_surfaces();
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Falling)],
        );
        markers.follow(&state);
        while state.event_phase() != Some(EventPhase::Reforming) || state.phase_tick() < 30 {
            tick(&mut state, &[]);
            markers.follow(&state);
        }
        let mut ink = shared_ink(&state.digit_surfaces());
        let mut steps = 0;
        let mut actions = vec![ClockAction::set_reading(reading(12, 35, 0))];
        while state.event_kind() == Some(ClockEventKind::Falling) {
            tick(&mut state, &std::mem::take(&mut actions));
            let surfaces = markers.follow(&state);
            let context = format!("{context}, step {steps}");
            assert_face_agreement(&state, &context);
            ink = assert_ink_continues(&ink, &surfaces, 0.06, &context);
            assert_one_per_cell(&surfaces, &context);
            steps += 1;
        }
        let after = assert_face_agreement(&state, &context);
        let fresh = clock(
            800.0 / 480.0,
            font,
            ClockTimeFormat::TwentyFourHour,
            reading(12, 35, 0),
        );
        assert_eq!(after.len(), fresh.digit_surfaces().len(), "{context}");
        let glyphs = |minute: u8| crate::fonts::glyph(font, Some(minute));
        let survives =
            |surface: &DigitSurface| surface.slot != 3 || glyphs(5).contains(surface.cell);
        for surface in &fallen {
            let kept = after.iter().any(|s| s.id == surface.id);
            assert_eq!(
                kept,
                survives(surface),
                "{context}: slot {} {:?}",
                surface.slot,
                surface.cell
            );
            let marker = markers
                .markers
                .iter()
                .find(|m| m.surface == surface.id)
                .unwrap();
            assert_eq!(
                marker.lost,
                (!kept).then_some(Loss::Retired(Retirement::Replaced)),
                "{context}"
            );
        }
        let arrived: Vec<_> = after
            .iter()
            .filter(|s| fallen.iter().all(|old| old.id != s.id))
            .collect();
        assert_eq!(
            arrived.len(),
            (glyphs(5).0 & !glyphs(4).0).count_ones() as usize,
            "{context}: exactly the cells the new reading adds are new"
        );
        assert!(arrived.iter().all(|s| s.slot == 3
            && glyphs(5).contains(s.cell)
            && !markers.markers.iter().any(|m| m.surface == s.id)));
    }
}

#[test]
fn surface_contract_a_reading_that_keeps_changing_on_the_way_home_never_jumps_or_duplicates() {
    // The minute's cell that a 4 has and a 5 lacks fades out, back in as a
    // new piece and out again, so two fading pieces share it and merge.
    for font in FONTS {
        let context = format!("{font:?}");
        let mut state = clock(
            800.0 / 480.0,
            font,
            ClockTimeFormat::TwentyFourHour,
            reading(12, 34, 10),
        );
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Falling)],
        );
        while state.event_phase() != Some(EventPhase::Reforming) || state.phase_tick() < 20 {
            tick(&mut state, &[]);
        }
        let progress = |state: &ClockState| match &state.active_event {
            Some(crate::events::ActiveEvent::Falling(event)) => event.progress(),
            _ => 1.0,
        };
        let mut ink = shared_ink(&state.digit_surfaces());
        let peak = state.surface_accounting().live;
        let mut window = 0.0;
        let mut steps = 0;
        while state.event_kind() == Some(ClockEventKind::Falling) {
            let minute = 34 + (state.phase_tick() / 10) % 2;
            let before = progress(&state);
            if state.display().digits[3] != Some(minute as u8 % 10) {
                window = before;
            }
            tick(
                &mut state,
                &[ClockAction::set_reading(reading(12, minute as u8, 0))],
            );
            let surfaces = state.digit_surfaces();
            let context = format!("{context}, step {steps}");
            assert_face_agreement(&state, &context);
            // No cell fades faster than the time left for its fade allows.
            let limit = 0.01 + (progress(&state) - before) / (1.0 - window);
            ink = assert_ink_continues(&ink, &surfaces, limit, &context);
            assert_one_per_cell(&surfaces, &context);
            assert!(
                surfaces.len() <= 2 * peak,
                "{context}: fading pieces do not pile up"
            );
            steps += 1;
        }
        let after = assert_face_agreement(&state, &context);
        let fresh = clock(
            800.0 / 480.0,
            font,
            ClockTimeFormat::TwentyFourHour,
            state.reading().unwrap(),
        );
        assert_eq!(after.len(), fresh.digit_surfaces().len(), "{context}");
    }
}

#[test]
fn surface_contract_falling_cut_short_or_paused_neither_snaps_nor_moves_its_pieces() {
    let mut state = clock(
        800.0 / 480.0,
        ClockFont::Classic,
        ClockTimeFormat::TwentyFourHour,
        reading(7, 15, 0),
    );
    let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Falling)],
    );
    for _ in 0..50 {
        tick(&mut state, &[]);
        markers.follow(&state);
    }
    let falling = state.digit_surfaces();
    for _ in 0..3 {
        sync(&mut state, &[]);
        assert_eq!(state.digit_surfaces(), falling, "a pause moves nothing");
    }
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
    );
    let after = markers.follow(&state);
    assert_face_agreement(&state, "after the fall was cut short");
    assert!(
        after
            .iter()
            .all(|s| falling.iter().all(|old| old.id != s.id))
    );
    assert!(
        markers
            .markers
            .iter()
            .all(|m| m.lost == Some(Loss::Retired(Retirement::Interrupted))),
        "a piece in mid-air is not carried home with what it holds"
    );
}

#[test]
fn surface_contract_falling_with_a_player_follows_the_arenas_bodies() {
    let mut state = clock(
        800.0 / 480.0,
        ClockFont::Sans,
        ClockTimeFormat::TwentyFourHour,
        reading(16, 20, 0),
    );
    sync(&mut state, &[ClockAction::toggle_player_duck(1)]);
    assert!(state.player_duck_session().is_some());
    let mut markers = Markers::attach(&state, |_| Some(Vec2::new(0.3, 0.3)));
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Falling)],
    );
    assert!(
        state
            .active_event
            .as_ref()
            .is_some_and(|event| event.shares_visit_arena())
    );
    let mut offsets = Vec::new();
    while state.event_kind() == Some(ClockEventKind::Falling) {
        let surfaces = state.digit_surfaces();
        assert_on_bodies(&state, &surfaces, &mut offsets, "shared");
        tick(&mut state, &[]);
        markers.follow(&state);
    }
    assert!(!offsets.is_empty());
    assert_face_agreement(&state, "after a shared fall");
    assert_eq!(markers.attached().count(), markers.markers.len());
}

/// Lit digit cells a reading draws in all four slots.
fn glyph_cells(font: ClockFont, digits: [Option<u8>; DIGIT_SLOT_COUNT]) -> usize {
    digits
        .iter()
        .map(|digit| crate::fonts::glyph(font, *digit).cells().count())
        .sum()
}

/// Runs an Explosion until its return has run `ticks`, following `fixtures`.
fn explode_until_return(state: &mut ClockState, ticks: u64, fixtures: &mut [&mut Markers]) {
    sync(
        state,
        &[ClockAction::preview_event(ClockEventKind::Explosion)],
    );
    for fixture in fixtures.iter_mut() {
        fixture.follow(state);
    }
    while state.event_phase() != Some(EventPhase::Reforming) || state.phase_tick() < ticks {
        tick(state, &[]);
        for fixture in fixtures.iter_mut() {
            fixture.follow(state);
        }
    }
}

/// Checks that no surface moves much further on one step than it did on the
/// step before, which a piece put somewhere else would.
fn assert_no_jump(steps: [&[DigitSurface]; 3], pitch: f32, context: &str) {
    let [older, before, after] = steps;
    let origin = |surfaces: &[DigitSurface], id| {
        surfaces
            .iter()
            .find(|surface| surface.id == id)
            .map(|surface| surface.origin)
    };
    for surface in after {
        let Some(previous) = origin(before, surface.id) else {
            continue;
        };
        let speed = origin(older, surface.id).map_or(0.0, |o| (previous - o).length());
        assert!(
            (surface.origin - previous).length() <= speed * 1.25 + 0.25 * pitch,
            "{context}: surface {} jumps from {previous:?} to {:?}",
            surface.id.get(),
            surface.origin
        );
    }
}

/// Follows one step of `fixtures` and returns the publication.
fn follow_all(state: &ClockState, fixtures: &mut [&mut Markers]) -> Vec<DigitSurface> {
    for fixture in fixtures.iter_mut() {
        fixture.follow(state);
    }
    state.digit_surfaces()
}

/// Checks that no two surfaces coming home are going to the same place, so
/// the face never ends up with a cell twice.
fn assert_one_per_cell(surfaces: &[DigitSurface], context: &str) {
    let homing: Vec<_> = surfaces.iter().filter(|s| !s.departing).collect();
    for (index, surface) in homing.iter().enumerate() {
        assert!(
            homing[index + 1..]
                .iter()
                .all(|other| (other.origin - surface.origin).length() > 1e-3),
            "{context}: two surfaces share a cell at {:?}",
            surface.origin
        );
    }
}

#[test]
fn surface_contract_explosion_blocks_ride_their_bodies_and_come_home_as_themselves() {
    for font in FONTS {
        for aspect in ASPECTS {
            let context = format!("{font:?} at {aspect}");
            let layout = Layout::new(aspect);
            // The AM/PM pixels explode too, and are not surfaces.
            let mut state = clock(
                aspect,
                font,
                ClockTimeFormat::TwelveHour,
                reading(12, 34, 10),
            );
            let local = Vec2::new(-0.25, 0.2);
            let mut markers = Markers::attach(&state, |_| Some(local));
            let before = state.digit_surfaces();
            let mut sorted = ids(&before);
            sorted.sort_unstable();
            sync(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Explosion)],
            );
            markers.follow(&state);
            let mut offsets = Vec::new();
            let mut fastest = 0.0_f32;
            let mut ticks = 0;
            while state.event_kind() == Some(ClockEventKind::Explosion) {
                let surfaces = if ticks % 3 == 0 {
                    assert_face_agreement(&state, &format!("{context}, tick {ticks}"))
                } else {
                    state.digit_surfaces()
                };
                assert_on_bodies(&state, &surfaces, &mut offsets, &context);
                let mut current = ids(&surfaces);
                current.sort_unstable();
                assert_eq!(current, sorted, "{context}: the same blocks throughout");
                tick(&mut state, &[]);
                markers.follow(&state);
                ticks += 1;
                let moved = markers.attached().map(|m| m.moved).fold(0.0, f32::max);
                if state.event_phase() == Some(EventPhase::Reforming) && state.phase_tick() == 0 {
                    assert!(
                        moved <= fastest + 0.01 * layout.pitch,
                        "{context}: the hand-off to the return path does not jump"
                    );
                }
                fastest = moved;
            }
            assert_eq!(
                ticks as u64,
                crate::events::explosion::EXPLOSION_TICKS,
                "{context}: the event ran to its end"
            );
            assert!(!offsets.is_empty(), "{context}: the blocks rode bodies");
            let after = assert_face_agreement(&state, &context);
            assert_eq!(
                ids(&after),
                ids(&before),
                "{context}: the same blocks came home"
            );
            assert_eq!(markers.attached().count(), before.len(), "{context}");
            for marker in markers.attached() {
                let surface = after.iter().find(|s| s.id == marker.surface).unwrap();
                let home = layout.slot_cell_center(usize::from(surface.slot), surface.cell);
                assert!((marker.world - (home + local * layout.pitch)).length() < 1e-3);
            }
            for surface in &before {
                let handoffs: Vec<_> = markers
                    .handoffs
                    .iter()
                    .filter(|(id, _, _)| *id == surface.id)
                    .map(|(_, from, to)| (*from, *to))
                    .collect();
                assert_eq!(
                    handoffs,
                    [
                        (SurfaceSource::Anchored, SurfaceSource::Rigid),
                        (SurfaceSource::Rigid, SurfaceSource::Return),
                        (SurfaceSource::Return, SurfaceSource::Anchored),
                    ],
                    "{context}: each hand-off is recorded exactly once"
                );
            }
        }
    }
}

#[test]
fn surface_contract_a_sparser_reading_on_the_way_home_merges_surplus_blocks_away() {
    for font in FONTS {
        let context = format!("{font:?}");
        let pitch = Layout::new(800.0 / 480.0).pitch;
        let mut state = clock(
            800.0 / 480.0,
            font,
            ClockTimeFormat::TwentyFourHour,
            reading(20, 8, 10),
        );
        let mut centred = Markers::attach(&state, |_| Some(Vec2::ZERO));
        let mut cornered = Markers::attach(&state, |_| Some(Vec2::new(0.3, 0.3)));
        let exploded = state.digit_surfaces();
        let admitted = state.surface_accounting().admitted;
        explode_until_return(&mut state, 30, &mut [&mut centred, &mut cornered]);
        let mut older = state.digit_surfaces();
        tick(&mut state, &[]);
        let mut previous = follow_all(&state, &mut [&mut centred, &mut cornered]);
        let mut actions = vec![ClockAction::set_reading(reading(20, 1, 0))];
        let mut steps = 0;
        while state.event_kind() == Some(ClockEventKind::Explosion) {
            tick(&mut state, &std::mem::take(&mut actions));
            let surfaces = follow_all(&state, &mut [&mut centred, &mut cornered]);
            let context = format!("{context}, step {steps}");
            assert_face_agreement(&state, &context);
            assert_no_jump([&older, &previous, &surfaces], pitch, &context);
            older = std::mem::replace(&mut previous, surfaces);
            steps += 1;
        }
        let after = assert_face_agreement(&state, &context);
        assert_one_per_cell(&after, &context);
        let merged = glyph_cells(font, [Some(2), Some(0), Some(0), Some(8)])
            - glyph_cells(font, [Some(2), Some(0), Some(0), Some(1)]);
        assert!(merged > 0, "{context}");
        assert_eq!(
            state.surface_accounting().admitted,
            admitted,
            "{context}: a sparser reading needs no new surfaces"
        );
        assert_eq!(
            state.surface_accounting().retired(Retirement::MergedAway) as usize,
            merged,
            "{context}"
        );
        assert!(
            after
                .iter()
                .all(|s| exploded.iter().any(|old| old.id == s.id)),
            "{context}: every block that came home is the surface it was, even one sent to \
             another cell"
        );
        for surface in &exploded {
            let kept = after.iter().any(|s| s.id == surface.id);
            let centre = centred.markers.iter().find(|m| m.surface == surface.id);
            let corner = cornered.markers.iter().find(|m| m.surface == surface.id);
            if kept {
                assert!(centre.unwrap().lost.is_none() && corner.unwrap().lost.is_none());
            } else {
                assert_eq!(surface.slot, 3, "{context}: only the changed digit merges");
                assert!(cornered.departed.contains(&surface.id), "{context}");
                // A shrinking block stops holding its corners long before it
                // is gone, and is lost with what it still holds at the end.
                assert_eq!(corner.unwrap().lost, Some(Loss::Unsupported), "{context}");
                assert_eq!(
                    centre.unwrap().lost,
                    Some(Loss::Retired(Retirement::MergedAway)),
                    "{context}"
                );
            }
        }
    }
}

#[test]
fn surface_contract_a_denser_reading_on_the_way_home_splits_new_blocks_off_the_debris() {
    for font in FONTS {
        let context = format!("{font:?}");
        let pitch = Layout::new(800.0 / 480.0).pitch;
        let mut state = clock(
            800.0 / 480.0,
            font,
            ClockTimeFormat::TwentyFourHour,
            reading(11, 11, 10),
        );
        let mut markers = Markers::attach(&state, |_| Some(Vec2::new(0.1, -0.2)));
        let exploded = state.digit_surfaces();
        let admitted = state.surface_accounting().admitted;
        explode_until_return(&mut state, 30, &mut [&mut markers]);
        let mut older = state.digit_surfaces();
        tick(&mut state, &[]);
        let mut previous = markers.follow(&state);
        let mut actions = vec![ClockAction::set_reading(reading(20, 8, 0))];
        let mut steps = 0;
        while state.event_kind() == Some(ClockEventKind::Explosion) {
            tick(&mut state, &std::mem::take(&mut actions));
            let surfaces = markers.follow(&state);
            assert_face_agreement(&state, &format!("{context}, step {steps}"));
            assert_no_jump([&older, &previous, &surfaces], pitch, &context);
            for surface in surfaces.iter().filter(|s| {
                state.event_kind().is_some() && exploded.iter().all(|old| old.id != s.id)
            }) {
                assert_eq!(surface.source, SurfaceSource::Return, "{context}");
                assert!(!surface.departing, "{context}");
                let half = surface.footprint.max.x;
                match previous.iter().find(|old| old.id == surface.id) {
                    // A split block grows from nothing, never appearing whole.
                    None => assert!(half < 0.01, "{context}: a new block starts at {half}"),
                    Some(old) => assert!(half >= old.footprint.max.x, "{context}"),
                }
            }
            older = std::mem::replace(&mut previous, surfaces);
            steps += 1;
        }
        let after = assert_face_agreement(&state, &context);
        assert_one_per_cell(&after, &context);
        let split = glyph_cells(font, [Some(2), Some(0), Some(0), Some(8)])
            - glyph_cells(font, [Some(1), Some(1), Some(1), Some(1)]);
        assert_eq!(
            state.surface_accounting().admitted - admitted,
            split as u64,
            "{context}: exactly the blocks the reading lacks are new"
        );
        assert_eq!(state.surface_accounting().total_retired(), 0, "{context}");
        assert!(
            exploded
                .iter()
                .all(|old| after.iter().any(|s| s.id == old.id)),
            "{context}: every exploded block came home as itself"
        );
        assert_eq!(
            markers.attached().count(),
            exploded.len(),
            "{context}: no marker was lost, and none was copied onto a new block"
        );
        assert!(
            after
                .iter()
                .filter(|s| exploded.iter().all(|old| old.id != s.id))
                .all(|s| !markers.markers.iter().any(|m| m.surface == s.id))
        );
    }
}

#[test]
fn surface_contract_a_reading_changed_back_on_the_way_home_reenlists_merging_blocks_as_new() {
    for font in FONTS {
        let context = format!("{font:?}");
        let mut state = clock(
            800.0 / 480.0,
            font,
            ClockTimeFormat::TwentyFourHour,
            reading(20, 8, 10),
        );
        let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
        let exploded = state.digit_surfaces();
        let admitted = state.surface_accounting().admitted;
        explode_until_return(&mut state, 20, &mut [&mut markers]);
        let mut actions = vec![ClockAction::set_reading(reading(20, 1, 0))];
        while state.phase_tick() < 40 {
            tick(&mut state, &std::mem::take(&mut actions));
            markers.follow(&state);
        }
        let merging: Vec<_> = state
            .digit_surfaces()
            .into_iter()
            .filter(|s| s.departing)
            .map(|s| s.id)
            .collect();
        let mut actions = vec![ClockAction::set_reading(reading(20, 8, 0))];
        let mut steps = 0;
        while state.event_kind() == Some(ClockEventKind::Explosion) {
            tick(&mut state, &std::mem::take(&mut actions));
            markers.follow(&state);
            assert_face_agreement(&state, &format!("{context}, step {steps}"));
            steps += 1;
        }
        let after = assert_face_agreement(&state, &context);
        assert_one_per_cell(&after, &context);
        assert_eq!(after.len(), exploded.len(), "{context}");
        assert_eq!(
            merging.len(),
            glyph_cells(font, [Some(2), Some(0), Some(0), Some(8)])
                - glyph_cells(font, [Some(2), Some(0), Some(0), Some(1)]),
            "{context}"
        );
        let accounting = state.surface_accounting();
        assert_eq!(
            accounting.admitted - admitted,
            merging.len() as u64,
            "{context}: a departing block is never brought back as itself"
        );
        assert_eq!(
            accounting.retired(Retirement::Replaced) as usize,
            merging.len(),
            "{context}"
        );
        assert_eq!(accounting.retired(Retirement::MergedAway), 0, "{context}");
        for marker in &markers.markers {
            assert_eq!(
                marker.lost,
                merging
                    .contains(&marker.surface)
                    .then_some(Loss::Retired(Retirement::Replaced)),
                "{context}"
            );
        }
    }
}

#[test]
fn surface_contract_am_pm_pixels_on_the_way_home_are_never_surfaces() {
    let at = reading(13, 45, 10);
    for (from, to) in [
        (ClockTimeFormat::TwentyFourHour, ClockTimeFormat::TwelveHour),
        (ClockTimeFormat::TwelveHour, ClockTimeFormat::TwentyFourHour),
    ] {
        for font in FONTS {
            let context = format!("{font:?} from {from:?}");
            let mut state = clock(800.0 / 480.0, font, from, at);
            let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
            let exploded = state.digit_surfaces();
            let admitted = state.surface_accounting().admitted;
            let before = state.display().digits;
            explode_until_return(&mut state, 30, &mut [&mut markers]);
            let mut settings = state.settings();
            settings.time_format = to;
            let mut actions = vec![ClockAction::configure(settings)];
            let mut steps = 0;
            while state.event_kind() == Some(ClockEventKind::Explosion) {
                tick(&mut state, &std::mem::take(&mut actions));
                markers.follow(&state);
                assert_face_agreement(&state, &format!("{context}, step {steps}"));
                steps += 1;
            }
            let after = assert_face_agreement(&state, &context);
            assert_one_per_cell(&after, &context);
            let fresh = clock(800.0 / 480.0, font, to, at);
            assert_eq!(after.len(), fresh.digit_surfaces().len(), "{context}");
            let accounting = state.surface_accounting();
            let old = glyph_cells(font, before);
            let new = glyph_cells(font, state.display().digits);
            // Digit blocks first come home to digit cells. Spare ones become
            // pixels or merge away, and a digit cell short of a digit block
            // gets a new surface, whether its block was a pixel or split off.
            assert_eq!(
                (accounting.admitted - admitted) as usize,
                new.saturating_sub(old),
                "{context}"
            );
            assert_eq!(
                accounting.total_retired() as usize,
                old.saturating_sub(new),
                "{context}"
            );
            for marker in &markers.markers {
                let kept = after.iter().any(|s| s.id == marker.surface);
                assert_eq!(marker.lost.is_none(), kept, "{context}");
                assert!(
                    matches!(
                        marker.lost,
                        None | Some(Loss::Retired(Retirement::Replaced | Retirement::MergedAway))
                    ),
                    "{context}: {:?}",
                    marker.lost
                );
            }
            assert!(exploded.len() == old);
        }
    }
}

#[test]
fn surface_contract_an_explosion_cut_short_or_paused_neither_snaps_nor_moves_its_blocks() {
    // Before the burst every block still rests at its cell, so the face keeps
    // them; after it, none is carried home with what it holds.
    for (stop, phase) in [
        (10, EventPhase::Warning),
        (80, EventPhase::Exploding),
        (
            crate::events::explosion::WARNING_TICKS + crate::events::explosion::BURST_TICKS + 30,
            EventPhase::Reforming,
        ),
    ] {
        let context = format!("{phase:?}");
        let mut state = clock(
            800.0 / 480.0,
            ClockFont::Matrix,
            ClockTimeFormat::TwentyFourHour,
            reading(7, 15, 0),
        );
        let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
        let face = state.digit_surfaces();
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Explosion)],
        );
        for _ in 0..stop {
            tick(&mut state, &[]);
            markers.follow(&state);
        }
        assert_eq!(state.event_phase(), Some(phase));
        let held = state.digit_surfaces();
        for _ in 0..3 {
            sync(&mut state, &[]);
            assert_eq!(
                state.digit_surfaces(),
                held,
                "{context}: a pause moves nothing"
            );
        }
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
        );
        let after = markers.follow(&state);
        assert_face_agreement(&state, &context);
        if phase == EventPhase::Warning {
            assert_eq!(ids(&after), ids(&face), "{context}");
            assert_eq!(markers.attached().count(), face.len(), "{context}");
            continue;
        }
        assert!(after.iter().all(|s| face.iter().all(|old| old.id != s.id)));
        assert!(
            markers
                .markers
                .iter()
                .all(|m| m.lost == Some(Loss::Retired(Retirement::Interrupted))),
            "{context}: a block away from home is not carried there with what it holds"
        );
    }
}

#[test]
fn surface_contract_an_explosion_with_a_player_follows_the_arenas_bodies() {
    let mut state = clock(
        800.0 / 480.0,
        ClockFont::Serif,
        ClockTimeFormat::TwentyFourHour,
        reading(16, 20, 0),
    );
    sync(&mut state, &[ClockAction::toggle_player_duck(1)]);
    assert!(state.player_duck_session().is_some());
    let mut markers = Markers::attach(&state, |_| Some(Vec2::new(0.3, -0.3)));
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Explosion)],
    );
    assert!(
        state
            .active_event
            .as_ref()
            .is_some_and(|event| event.shares_visit_arena())
    );
    let mut offsets = Vec::new();
    let mut ticks = 0;
    while state.event_kind() == Some(ClockEventKind::Explosion) {
        let surfaces = if ticks % 5 == 0 {
            assert_face_agreement(&state, &format!("shared, tick {ticks}"))
        } else {
            state.digit_surfaces()
        };
        assert_on_bodies(&state, &surfaces, &mut offsets, "shared");
        tick(&mut state, &[]);
        markers.follow(&state);
        ticks += 1;
    }
    assert!(!offsets.is_empty());
    assert_face_agreement(&state, "after a shared explosion");
    assert_eq!(markers.attached().count(), markers.markers.len());
}

/// The solid digit blocks a Meltdown still has.
fn solids(state: &ClockState) -> usize {
    match &state.active_event {
        Some(crate::events::ActiveEvent::Meltdown(event)) => {
            event.cells.iter().filter(|cell| !cell.meridiem).count()
        }
        _ => 0,
    }
}

#[test]
fn surface_contract_meltdown_blocks_keep_their_surfaces_until_they_melt_leave_or_are_reclaimed() {
    for (index, font) in FONTS.into_iter().enumerate() {
        for aspect in ASPECTS {
            let context = format!("{font:?} at {aspect}");
            // The AM/PM pixels melt too, and are not surfaces.
            let format = if index % 2 == 0 {
                ClockTimeFormat::TwelveHour
            } else {
                ClockTimeFormat::TwentyFourHour
            };
            let mut state = clock(aspect, font, format, reading(20, 8, 10));
            let mut markers = Markers::attach(&state, |_| Some(Vec2::new(0.2, 0.2)));
            let face = state.digit_surfaces();
            let admitted = state.surface_accounting().admitted;
            sync(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Meltdown)],
            );
            let mut ticks = 0;
            let mut refill: Vec<DigitSurface> = Vec::new();
            let mut exited = 0;
            while state.event_kind() == Some(ClockEventKind::Meltdown) {
                let surfaces = markers.follow(&state);
                if state.event_phase() != Some(EventPhase::Reforming) {
                    exited = state.meltdown_state().unwrap().exited_solid_microunits;
                }
                let context = format!("{context}, tick {ticks}");
                if ticks % 5 == 0 {
                    assert_face_agreement(&state, &context);
                }
                if state.event_phase() == Some(EventPhase::Reforming) {
                    // The refill is new surfaces, growing as the face is drawn.
                    assert!(surfaces.iter().all(|s| s.source == SurfaceSource::Refill
                        && face.iter().all(|old| old.id != s.id)));
                    for surface in &surfaces {
                        if let Some(old) = refill.iter().find(|old| old.id == surface.id) {
                            assert!(surface.footprint.max.y >= old.footprint.max.y, "{context}");
                            assert_eq!(surface.origin, old.origin, "{context}");
                        }
                    }
                    refill = surfaces;
                } else {
                    // Water and the AM/PM pixels are not surfaces.
                    assert_eq!(surfaces.len(), solids(&state), "{context}");
                    assert!(surfaces.iter().all(|s| s.body.is_none()
                        && matches!(s.source, SurfaceSource::Anchored | SurfaceSource::Ballistic)));
                }
                tick(&mut state, &[]);
                ticks += 1;
            }
            assert_eq!(
                ticks as u64,
                MELTING_TICKS + DRAINING_TICKS + REFORMING_TICKS,
                "{context}"
            );
            let after = markers.follow(&state);
            assert_face_agreement(&state, &context);
            assert_eq!(
                ids(&after),
                ids(&refill),
                "{context}: the refill is the face"
            );
            let accounting = state.surface_accounting();
            assert_eq!(
                accounting.admitted - admitted,
                after.len() as u64,
                "{context}"
            );
            let gone = [
                Retirement::Melted,
                Retirement::Exited,
                Retirement::Reclaimed,
            ]
            .map(|reason| accounting.retired(reason))
            .iter()
            .sum::<u64>();
            assert_eq!(gone as usize, face.len(), "{context}");
            assert!(accounting.retired(Retirement::Melted) > 0, "{context}");
            if format == ClockTimeFormat::TwentyFourHour {
                // Every solid that left the view was a digit block.
                assert_eq!(
                    accounting.retired(Retirement::Exited) * 1_000_000,
                    exited,
                    "{context}"
                );
            }
            for marker in &markers.markers {
                assert!(
                    matches!(
                        marker.lost,
                        Some(Loss::Retired(
                            Retirement::Melted | Retirement::Exited | Retirement::Reclaimed
                        ))
                    ),
                    "{context}: {:?}",
                    marker.lost
                );
            }
            for surface in &face {
                let handoffs: Vec<_> = markers
                    .handoffs
                    .iter()
                    .filter(|(id, _, _)| *id == surface.id)
                    .map(|(_, from, to)| (*from, *to))
                    .collect();
                assert!(
                    handoffs.is_empty()
                        || handoffs == [(SurfaceSource::Anchored, SurfaceSource::Ballistic)],
                    "{context}: {handoffs:?}"
                );
            }
        }
    }
}

#[test]
fn surface_contract_meltdown_with_a_player_rides_the_arenas_bodies() {
    let mut state = clock(
        800.0 / 480.0,
        ClockFont::Classic,
        ClockTimeFormat::TwentyFourHour,
        reading(18, 48, 0),
    );
    sync(&mut state, &[ClockAction::toggle_player_duck(1)]);
    assert!(state.player_duck_session().is_some());
    let mut markers = Markers::attach(&state, |_| Some(Vec2::new(-0.3, 0.1)));
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Meltdown)],
    );
    let mut offsets = Vec::new();
    let mut ticks = 0;
    let mut exited = 0;
    while state.event_kind() == Some(ClockEventKind::Meltdown) {
        let surfaces = markers.follow(&state);
        if ticks % 5 == 0 {
            assert_face_agreement(&state, &format!("shared, tick {ticks}"));
        }
        assert_on_bodies(&state, &surfaces, &mut offsets, "shared");
        if state.event_phase() != Some(EventPhase::Reforming) {
            assert_eq!(surfaces.len(), solids(&state));
            exited = state.meltdown_state().unwrap().exited_solid_microunits;
        }
        tick(&mut state, &[]);
        ticks += 1;
    }
    assert!(!offsets.is_empty(), "the blocks fell on the arena's bodies");
    markers.follow(&state);
    assert_face_agreement(&state, "after a shared meltdown");
    // The arena reports a block that melts and one that leaves the same way
    // the event does on its own.
    let accounting = state.surface_accounting();
    assert!(accounting.retired(Retirement::Melted) > 0);
    assert_eq!(accounting.retired(Retirement::Exited) * 1_000_000, exited);
    assert!(markers.markers.iter().all(|m| matches!(
        m.lost,
        Some(Loss::Retired(
            Retirement::Melted | Retirement::Exited | Retirement::Reclaimed
        ))
    )));
    assert!(
        markers
            .handoffs
            .iter()
            .all(|(_, from, to)| (*from, *to) == (SurfaceSource::Anchored, SurfaceSource::Rigid))
    );
}

#[test]
fn surface_contract_a_meltdown_cut_short_gives_back_what_never_left_and_the_refill() {
    let start = MELTING_TICKS + DRAINING_TICKS;
    for stop in [30, start + 40] {
        let context = format!("stopped at {stop}");
        let mut state = clock(
            800.0 / 480.0,
            ClockFont::Sans,
            ClockTimeFormat::TwentyFourHour,
            reading(10, 10, 0),
        );
        let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Meltdown)],
        );
        for _ in 0..stop {
            tick(&mut state, &[]);
            markers.follow(&state);
        }
        let held = state.digit_surfaces();
        let waiting: Vec<_> = held
            .iter()
            .filter(|s| matches!(s.source, SurfaceSource::Anchored | SurfaceSource::Refill))
            .map(|s| s.id)
            .collect();
        let low = Vec2::new(0.0, -0.3);
        let mut late = Markers::attach(&state, |s| s.supports(low).then_some(low));
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
        );
        let after = markers.follow(&state);
        late.follow(&state);
        assert_face_agreement(&state, &context);
        assert!(!waiting.is_empty(), "{context}");
        for id in &waiting {
            assert!(after.iter().any(|s| s.id == *id), "{context}: {}", id.get());
        }
        if stop < start {
            assert!(waiting.len() < held.len(), "{context}: some blocks fell");
            for marker in &markers.markers {
                assert_eq!(
                    marker.lost,
                    (!waiting.contains(&marker.surface))
                        .then_some(Loss::Retired(Retirement::Interrupted)),
                    "{context}"
                );
            }
        } else {
            assert_eq!(waiting.len(), held.len(), "{context}");
            assert!(
                !late.markers.is_empty(),
                "{context}: the refill is eligible"
            );
            assert_eq!(late.attached().count(), late.markers.len(), "{context}");
        }
    }
}

#[test]
fn surface_contract_a_reading_changed_during_the_refill_replaces_only_the_changed_cells() {
    for font in FONTS {
        let context = format!("{font:?}");
        let mut state = clock(
            800.0 / 480.0,
            font,
            ClockTimeFormat::TwentyFourHour,
            reading(12, 34, 0),
        );
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Meltdown)],
        );
        while state.event_phase() != Some(EventPhase::Reforming) || state.phase_tick() < 45 {
            tick(&mut state, &[]);
        }
        let before = state.digit_surfaces();
        let mut markers = Markers::attach(&state, |_| Some(Vec2::new(0.0, -0.35)));
        tick(&mut state, &[ClockAction::set_reading(reading(12, 35, 0))]);
        let after = markers.follow(&state);
        assert_face_agreement(&state, &context);
        let glyph = |digit: u8| crate::fonts::glyph(font, Some(digit));
        for surface in &before {
            let kept = after.iter().any(|s| s.id == surface.id);
            assert_eq!(
                kept,
                surface.slot != 3 || glyph(5).contains(surface.cell),
                "{context}"
            );
        }
        let new: Vec<_> = after
            .iter()
            .filter(|s| before.iter().all(|old| old.id != s.id))
            .collect();
        assert_eq!(new.len(), (glyph(5).0 & !glyph(4).0).count_ones() as usize);
        assert!(
            new.iter()
                .all(|s| s.slot == 3 && s.source == SurfaceSource::Refill)
        );
        for marker in &markers.markers {
            let kept = after.iter().any(|s| s.id == marker.surface);
            assert_eq!(
                marker.lost,
                (!kept).then_some(Loss::Retired(Retirement::Replaced)),
                "{context}"
            );
        }
    }
}

#[test]
fn surface_contract_the_water_lab_leaves_the_face_alone() {
    let mut state = ClockScenario::init(
        ClockConfig {
            water_lab: ClockWaterLab::Floating,
            ..config(
                800.0 / 480.0,
                ClockFont::Classic,
                ClockTimeFormat::TwentyFourHour,
            )
        },
        7,
    );
    sync(&mut state, &[ClockAction::set_reading(reading(9, 41, 0))]);
    let face = state.digit_surfaces();
    let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Meltdown)],
    );
    let mut ticks = 0;
    while state.event_kind() == Some(ClockEventKind::Meltdown) {
        tick(&mut state, &[]);
        let surfaces = markers.follow(&state);
        if ticks % 30 == 0 {
            assert_face_agreement(&state, &format!("lab, tick {ticks}"));
        }
        assert_eq!(surfaces, face);
        ticks += 1;
    }
    assert!(ticks > 0);
    assert_eq!(markers.attached().count(), face.len());
}
