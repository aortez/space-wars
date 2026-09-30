use super::*;

fn ready(font: ClockFont) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            fonts: ClockFontSettings {
                selected: font,
                ..Default::default()
            },
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        42,
    );
    state.apply_reading(ClockReading::new(23, 58, 59).unwrap(), false);
    state
}

#[test]
fn catalog_has_distinct_bounded_digits_and_exact_cell_partitions() {
    for font in ClockFont::ALL {
        let mut glyphs = Vec::new();
        for digit in 0..10 {
            let glyph = fonts::glyph(font, Some(digit));
            assert!(glyph.0 != 0 && glyph.0 >> 54 == 0);
            assert!(!glyphs.contains(&glyph));
            glyphs.push(glyph);
            let mut combined = 0;
            for kind in SegmentKind::ALL {
                let piece = glyph.0 & fonts::region(font, kind).0;
                assert_eq!(piece & combined, 0);
                combined |= piece;
            }
            assert_eq!(combined, glyph.0);
        }
        assert!(fonts::guides(font).cells().count() <= 54);
    }
    for font in ClockFont::ALL.into_iter().skip(1) {
        assert_ne!(
            fonts::glyph(font, Some(8)),
            fonts::glyph(ClockFont::Classic, Some(8))
        );
    }
}

#[test]
fn every_font_survives_physical_events_time_changes_and_recovery() {
    for font in ClockFont::ALL {
        for kind in [
            ClockEventKind::Falling,
            ClockEventKind::Meltdown,
            ClockEventKind::Explosion,
            ClockEventKind::Rain,
            ClockEventKind::Marquee,
            ClockEventKind::DigitSlide,
        ] {
            let mut state = ready(font);
            state.preview_event(kind);
            for tick in 0..2600 {
                if tick == 90 {
                    state.apply_reading(ClockReading::new(0, 1, 0).unwrap(), false);
                }
                assert_eq!(state.active_font(), font);
                assert!(state.body_count() <= fonts::MAX_DIGIT_CELLS + 40);
                if tick % 120 == 0 {
                    let frame = ClockScenario::render_frame(&state);
                    assert!(!frame.layers.is_empty());
                }
                if state.event_kind().is_none() {
                    break;
                }
                state.advance_tick();
            }
            assert!(state.event_kind().is_none(), "{font:?} {kind:?}");
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            for slot in 0..4 {
                let actual = state
                    .segments()
                    .iter()
                    .filter(|s| s.id.digit_slot == slot && s.lit)
                    .fold(0, |mask, s| mask | s.shape.0);
                assert_eq!(
                    actual,
                    fonts::glyph(font, state.display.digits[slot as usize]).0
                );
            }
        }
    }
}

#[test]
fn font_changes_wait_for_effects_and_do_not_advance_paused_physics() {
    for kind in [
        ClockEventKind::Falling,
        ClockEventKind::Meltdown,
        ClockEventKind::Explosion,
        ClockEventKind::Rain,
        ClockEventKind::ColorCycle,
        ClockEventKind::Marquee,
        ClockEventKind::DigitSlide,
    ] {
        let mut state = ready(ClockFont::Classic);
        state.preview_event(kind);
        for _ in 0..45 {
            state.advance_tick();
        }
        let before = state.segments().to_vec();
        let counts = (state.body_count(), state.collider_count());
        let tick = state.simulation_tick();
        let mut settings = state.settings();
        settings.fonts.selected = ClockFont::Serif;
        state.configure(settings);
        assert_eq!(state.active_font(), ClockFont::Classic);
        assert_eq!(state.segments(), before);
        assert_eq!((state.body_count(), state.collider_count()), counts);
        assert_eq!(state.simulation_tick(), tick);
        for _ in 0..2600 {
            if state.event_kind().is_none() {
                break;
            }
            state.advance_tick();
        }
        assert_eq!(state.active_font(), ClockFont::Serif);
    }
}

#[test]
fn rotation_uses_only_the_selected_pool_without_repeats_or_event_rng_changes() {
    let mut a = ready(ClockFont::Sans);
    let mut b = ready(ClockFont::Sans);
    for state in [&mut a, &mut b] {
        let mut settings = state.settings();
        settings.fonts.rotate = true;
        settings.fonts.pool =
            ClockFontPool::try_from(vec![ClockFont::Sans, ClockFont::Serif]).unwrap();
        settings.events.digit_slide = false;
        state.configure(settings);
    }
    for minute in 0..20 {
        let previous = a.active_font();
        for state in [&mut a, &mut b] {
            state.apply_reading(ClockReading::new(1, minute, 0).unwrap(), true);
        }
        assert_ne!(a.active_font(), previous);
        assert_eq!(a.active_font(), b.active_font());
        assert!(a.config.fonts.pool.contains(a.active_font()));
    }
    let mut original = ready(ClockFont::Classic);
    a.start_event(ClockEventKind::Duck);
    original.start_event(ClockEventKind::Duck);
    assert_eq!(a.duck_state(), original.duck_state());
}

#[test]
fn font_settings_round_trip_and_reject_empty_or_unknown_pools() {
    for selected in ClockFont::ALL {
        for bits in 1..16 {
            let settings = ClockSettings {
                fonts: ClockFontSettings {
                    selected,
                    rotate: true,
                    pool: ClockFontPool::from_bits(bits).unwrap(),
                },
                ..Default::default()
            };
            assert_eq!(
                ClockAction::decode(&ClockAction::configure(settings)),
                Some(ClockAction::Configure(settings))
            );
        }
    }
    let Action::Scenario { kind, payload } = ClockAction::configure(ClockSettings::default())
    else {
        panic!()
    };
    for (index, value) in [(9, 4), (10, 2), (11, 0), (11, 16)] {
        let mut invalid = payload.clone();
        invalid[index] = value;
        assert_eq!(ClockAction::decode(&Action::scenario(kind, invalid)), None);
    }
}
