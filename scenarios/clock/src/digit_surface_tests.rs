//! The digit surface contract: who the lit cells are, where they are drawn and
//! what becomes of a marker attached to one. Every check reads the rendered
//! frame, so a published pose that the face does not actually draw fails here.

use super::*;
use crate::render::{
    ACTIVE_CELL_LAYER, digit_cell_polygons, filled_polygons_with_alpha, render_frame,
};
use engine_common::{ClockEventKind, ClockFont, ClockTimeFormat};
use engine_core::Vec2;

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

fn inside(polygon: &[Vec2], point: Vec2, tolerance: f32) -> bool {
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

/// The part of a surface's footprint inside its clip band, in its own frame.
fn visible(surface: &DigitSurface) -> Option<Footprint> {
    let mut footprint = surface.footprint;
    if let Some([lower, upper]) = surface.clip {
        assert!(
            surface.angle.abs() < 1e-6,
            "only an upright surface is clipped to a band"
        );
        footprint.min.y = footprint
            .min
            .y
            .max((lower - surface.origin.y) / surface.scale);
        footprint.max.y = footprint
            .max
            .y
            .min((upper - surface.origin.y) / surface.scale);
    }
    (!footprint.is_empty()).then_some(footprint)
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
/// lit digit cell drawn belongs to a surface unless `unowned` says the face
/// draws it without one.
pub(crate) fn assert_agreement(
    state: &ClockState,
    context: &str,
    unowned: impl Fn(Vec2) -> bool,
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
        let Some(footprint) = visible(surface) else {
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
        if unowned(centroid) {
            continue;
        }
        let owned = surfaces.iter().any(|surface| {
            let Some(footprint) = (surface.opacity > 0.0).then(|| visible(surface)).flatten()
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
    assert_agreement(state, context, |_| false)
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
