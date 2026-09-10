use super::{content::*, effects::*, *};
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState, DisplaySnapshot,
    EventLifecycle, MARQUEE_TICKS, SegmentRepresentation, events::marquee::MarqueeEvent,
};
use engine_common::{
    ClockEventKind, ClockEventProfile, ClockMarqueePreset, ClockTimeFormat, RenderPrimitive,
    Scenario,
};
use std::time::Duration;

fn reading() -> ClockReading {
    ClockReading::new(8, 24, 0).unwrap()
}
fn step(state: &mut ClockState, count: u64) {
    for _ in 0..count {
        ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
    }
}
fn clock(preset: ClockMarqueePreset, aspect_ratio: f32) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            marquee_preset: preset,
            aspect_ratio,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        7,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading())],
        Duration::ZERO,
    );
    state
}

#[test]
fn bitmap_font_is_bounded_explicit_and_case_insensitive() {
    // The public validator and the private bitmap font must agree exhaustively.
    // Prefix a visible letter so a single space is not rejected as blank text.
    for byte in 0..=127 {
        let text = format!("A{}", char::from(byte));
        assert_eq!(
            text.parse::<engine_common::ClockMarqueeMessage>().is_ok(),
            font::glyph(byte).is_some(),
            "byte {byte}"
        );
    }
    assert_eq!(
        Content::text("space wars").unwrap().cells,
        Content::text("SPACE WARS").unwrap().cells
    );
    for text in ["ABCDEFGHIJKLMNOPQRSTUVWXYZ", "0123456789 .,:-!?/'"] {
        assert!(!Content::text(text).unwrap().cells.is_empty());
    }
    assert_eq!(Content::text("").err(), Some(TextError::Empty));
    assert_eq!(Content::text("  ").err(), Some(TextError::Empty));
    assert_eq!(
        Content::text("é").err(),
        Some(TextError::UnsupportedCharacter)
    );
    assert_eq!(
        Content::text("A\nB").err(),
        Some(TextError::UnsupportedCharacter)
    );
    assert_eq!(
        Content::text(&"A".repeat(MAX_TEXT_BYTES + 1)).err(),
        Some(TextError::TooLong)
    );
    let largest = Content::text(&"8".repeat(MAX_TEXT_BYTES)).unwrap();
    assert_eq!(largest.groups, MAX_TEXT_BYTES);
    assert!(largest.cells.len() <= MAX_CONTENT_CELLS);
    assert!(largest.cells.capacity() <= MAX_CONTENT_CELLS);
}

#[test]
fn custom_message_is_latched_and_shared_by_every_text_recipe() {
    for preset in [
        ClockMarqueePreset::TextScroll,
        ClockMarqueePreset::TextRibbon,
        ClockMarqueePreset::TextSpin,
    ] {
        let mut state = clock(preset, 800.0 / 480.0);
        let mut settings = state.settings();
        settings.marquee_message = "Hello, pi!".parse().unwrap();
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::configure(settings),
                ClockAction::preview_event(ClockEventKind::Marquee),
            ],
            Duration::ZERO,
        );
        step(&mut state, 180);
        let before = ClockScenario::render_frame(&state);
        let active = state.marquee_state().unwrap();
        assert_eq!(active.content, "HELLO, PI!");
        assert_eq!(active.group_count, 10);
        settings.marquee_message = "8".repeat(MAX_TEXT_BYTES).parse().unwrap();
        ClockScenario::step(
            &mut state,
            &[ClockAction::configure(settings)],
            Duration::ZERO,
        );
        assert_eq!(state.marquee_state(), Some(active));
        assert_eq!(ClockScenario::render_frame(&state), before);
        assert_eq!(state.settings().marquee_message, settings.marquee_message);
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Marquee)],
            Duration::ZERO,
        );
        assert_eq!(
            state.marquee_state().unwrap().content,
            "8".repeat(MAX_TEXT_BYTES)
        );
        let Some(crate::events::ActiveEvent::Marquee(event)) = &state.active_event else {
            panic!()
        };
        assert_eq!(
            event.content.cells,
            Content::text(settings.marquee_message.as_str())
                .unwrap()
                .cells
        );
        assert!(event.content.cells.len() <= MAX_CONTENT_CELLS);
        step(&mut state, MARQUEE_TICKS / 2);
        let frame = ClockScenario::render_frame(&state);
        let cells: Vec<_> = frame
            .layers
            .iter()
            .filter(|layer| layer.z == 3)
            .flat_map(|layer| &layer.primitives)
            .collect();
        assert!(!cells.is_empty() && cells.len() <= MAX_CONTENT_CELLS);
        let layout = crate::layout::Layout::new(state.aspect_ratio());
        for primitive in cells {
            let RenderPrimitive::Polygon(polygon) = primitive else {
                panic!()
            };
            for p in &polygon.points {
                assert!(p.x.is_finite() && p.y.is_finite());
                assert!(p.x >= layout.bounds_min.x + 7.9999 && p.x <= layout.bounds_max.x - 7.9999);
                assert!(p.y >= layout.floor_y + 7.9999 && p.y <= layout.bounds_max.y - 27.9999);
            }
        }
        step(&mut state, MARQUEE_TICKS / 2);
        assert!(state.marquee_state().is_none());
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    }
}

#[test]
fn clock_geometry_has_stable_routes_and_reuses_its_buffer() {
    let display = crate::digits::snapshot(reading(), ClockTimeFormat::TwentyFourHour);
    let mut content = Content::clock(display);
    let capacity = content.cells.capacity();
    let initial = content.cells.clone();
    // An appearance-only recipe must not re-fit or shift normal clock anchors,
    // even at the widest layout. It shares the authoritative digit origins.
    for aspect in [0.25, 0.75, 800.0 / 480.0, 4.0] {
        let layout = crate::layout::Layout::new(aspect);
        let placement = Placement {
            content: content.bounds,
            viewport: Bounds {
                min: Vec2::new(layout.bounds_min.x + 8.0, layout.floor_y + 8.0),
                max: Vec2::new(layout.bounds_max.x - 8.0, layout.bounds_max.y - 28.0),
            },
            center: Vec2::new(0.0, layout.face_origin.y + 4.5 * layout.pitch),
            pitch: layout.pitch,
        };
        let recipe = Recipe {
            lighting: Lighting::Chase,
            ..Recipe::default()
        };
        for cell in &content.cells {
            let points = recipe.quad(
                *cell,
                placement,
                Playback {
                    seconds: 6.0,
                    progress: 0.5,
                    strength: 1.0,
                },
            );
            let actual = points.into_iter().fold(Vec2::ZERO, |sum, p| sum + p) * 0.25;
            let expected =
                Vec2::new(layout.face_origin.x, layout.face_origin.y) + cell.center * layout.pitch;
            assert!((actual.x - expected.x).abs() < 0.0001);
            assert!((actual.y - expected.y).abs() < 0.0001);
        }
    }
    for hour in 0..24 {
        for format in [ClockTimeFormat::TwelveHour, ClockTimeFormat::TwentyFourHour] {
            content.update_clock(crate::digits::snapshot(
                ClockReading::new(hour, 58, 0).unwrap(),
                format,
            ));
            assert!(content.cells.len() <= 128);
            assert_eq!(content.cells.capacity(), capacity);
            for cell in &content.cells {
                assert!((0.0..1.0).contains(&cell.path));
                assert!(cell.group < 6);
            }
        }
    }
    content.update_clock(display);
    assert_eq!(content.cells, initial);
    // The perimeter is a deliberate clockwise walk, independent of segment
    // storage ordering. All twenty positions occur exactly once on an eight.
    content.update_clock(DisplaySnapshot {
        digits: [Some(8), None, None, None],
        colon_lit: false,
        meridiem: None,
    });
    let mut outside: Vec<_> = content
        .cells
        .iter()
        .filter(|c| c.center.y != 4.5)
        .map(|c| (c.path * 20.0).round() as u8)
        .collect();
    outside.sort_unstable();
    assert_eq!(outside, (0..20).collect::<Vec<_>>());
}

#[test]
fn group_rotation_and_glyph_rotation_are_distinct_and_do_not_mutate_content() {
    let content = Content::text("AB").unwrap();
    let before = content.cells.clone();
    let cell = content.cells[0];
    let placement = Placement {
        content: content.bounds,
        viewport: Bounds {
            min: Vec2::new(-100.0, -100.0),
            max: Vec2::new(100.0, 100.0),
        },
        center: Vec2::ZERO,
        pitch: 1.0,
    };
    let playback = Playback {
        seconds: 1.0,
        progress: 0.5,
        strength: 1.0,
    };
    let sample = |target| {
        Recipe {
            rotation: Some(Rotation { target, turns: 0.5 }),
            ..Recipe::default()
        }
        .quad(cell, placement, playback)
    };
    assert_ne!(sample(Target::Content), sample(Target::Glyph));
    assert_ne!(sample(Target::Glyph), sample(Target::Cell));
    assert_eq!(sample(Target::Content), sample(Target::Content));
    assert_eq!(before, content.cells);
}

#[test]
fn glyph_wave_translates_a_letter_while_cell_wave_deforms_it() {
    let content = Content::text("A").unwrap();
    let placement = Placement {
        content: content.bounds,
        viewport: Bounds {
            min: Vec2::new(-100.0, -100.0),
            max: Vec2::new(100.0, 100.0),
        },
        center: Vec2::ZERO,
        pitch: 1.0,
    };
    let playback = Playback {
        seconds: 0.3,
        progress: 0.2,
        strength: 1.0,
    };
    let sample = |target| {
        Recipe {
            wave: Some(Wave {
                target,
                amplitude: 1.0,
                wavelength: 18.0,
                cycles_per_second: 0.3,
            }),
            ..Recipe::default()
        }
        .quad(content.cells[0], placement, playback)
    };
    let glyph = sample(Target::Glyph);
    let ribbon = sample(Target::Cell);
    assert_eq!(glyph[0].y, glyph[1].y);
    assert!((ribbon[0].y - ribbon[1].y).abs() > 0.001);
}

#[test]
fn rectangular_clip_handles_rotated_partial_and_fully_hidden_cells() {
    let bounds = Bounds {
        min: Vec2::new(-1.0, -1.0),
        max: Vec2::new(1.0, 1.0),
    };
    for angle in 0..90 {
        for x in -3_i32..=3 {
            let quad = [
                Vec2::new(-0.7, -0.7),
                Vec2::new(0.7, -0.7),
                Vec2::new(0.7, 0.7),
                Vec2::new(-0.7, 0.7),
            ]
            .map(|p| p.rotate_radians(angle as f32 * 0.1) + Vec2::new(x as f32, 0.0));
            let (points, count) = clip_quad(quad, bounds);
            assert!(count <= 8);
            for p in &points[..count] {
                assert!((-1.00001..=1.00001).contains(&p.x) && (-1.00001..=1.00001).contains(&p.y));
            }
            if x.abs() == 3 {
                assert_eq!(count, 0);
            }
        }
    }
}

#[test]
fn all_recipes_are_bounded_deterministic_clipped_and_restore_the_exact_face() {
    for aspect in [0.25, 0.75, 800.0 / 480.0, 4.0] {
        for preset in ClockMarqueePreset::ALL {
            let mut state = clock(preset, aspect);
            let original = ClockScenario::render_frame(&state);
            let segments = state.segments().to_vec();
            ClockScenario::step(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Marquee)],
                Duration::ZERO,
            );
            assert_eq!(ClockScenario::render_frame(&state), original);
            let mut previous = 0;
            for tick in [1, 45, 180, 360, 540, 675, 719] {
                step(&mut state, tick - previous);
                previous = tick;
                let frame = ClockScenario::render_frame(&state);
                assert_eq!(frame, ClockScenario::render_frame(&state));
                assert_eq!(state.segments(), segments);
                assert_eq!((state.body_count(), state.collider_count()), (0, 0));
                assert!(
                    frame
                        .layers
                        .iter()
                        .map(|l| l.primitives.len())
                        .sum::<usize>()
                        < 400
                );
                let layout = crate::layout::Layout::new(aspect);
                let bounds = Bounds {
                    min: Vec2::new(layout.bounds_min.x + 8.0, layout.floor_y + 8.0),
                    max: Vec2::new(layout.bounds_max.x - 8.0, layout.bounds_max.y - 28.0),
                };
                for layer in &frame.layers {
                    for primitive in &layer.primitives {
                        let RenderPrimitive::Polygon(polygon) = primitive else {
                            panic!("expected code-native geometry")
                        };
                        for p in &polygon.points {
                            assert!(p.x.is_finite() && p.y.is_finite());
                            if layer.z == 3 && (45..=675).contains(&tick) {
                                assert!(
                                    p.x >= bounds.min.x - 0.0001 && p.x <= bounds.max.x + 0.0001
                                );
                                assert!(
                                    p.y >= bounds.min.y - 0.0001 && p.y <= bounds.max.y + 0.0001
                                );
                            }
                        }
                    }
                }
            }
            step(&mut state, MARQUEE_TICKS - previous);
            assert_eq!(state.lifecycle(), EventLifecycle::Cooldown);
            assert_eq!(state.marquee_state(), None);
            assert_eq!(ClockScenario::render_frame(&state), original);
        }
    }
}

#[test]
fn reading_changes_stay_live_and_settings_do_not_replace_the_active_recipe() {
    let mut state = clock(ClockMarqueePreset::ClockWave, 1.5);
    ClockScenario::step(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Marquee)],
        Duration::ZERO,
    );
    step(&mut state, 150);
    let mut settings = state.settings();
    settings.marquee_preset = ClockMarqueePreset::TextRibbon;
    settings.time_format = ClockTimeFormat::TwelveHour;
    let next = ClockReading::new(23, 59, 1).unwrap();
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::configure(settings),
            ClockAction::set_reading(next),
        ],
        Duration::ZERO,
    );
    assert_eq!(state.phase_tick(), 150);
    assert_eq!(
        state.marquee_state().unwrap().preset,
        ClockMarqueePreset::ClockWave
    );
    let Some(crate::events::ActiveEvent::Marquee(event)) = &state.active_event else {
        panic!()
    };
    assert_eq!(event.content.cells, Content::clock(state.display()).cells);
    let frame = ClockScenario::render_frame(&state);
    for _ in 0..10 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(frame, ClockScenario::render_frame(&state));
    ClockScenario::step(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Marquee)],
        Duration::ZERO,
    );
    assert_eq!(state.event_id(), 2);
    assert_eq!(
        state.marquee_state().unwrap().preset,
        ClockMarqueePreset::TextRibbon
    );
    assert_eq!(state.phase_tick(), 0);
}

#[test]
fn resize_and_preview_replacement_release_content_and_restore_latest_time() {
    for kind in ClockEventKind::ALL {
        let mut state = clock(ClockMarqueePreset::TextRibbon, 1.5);
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Marquee)],
            Duration::ZERO,
        );
        step(&mut state, 360);
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(kind)],
            Duration::ZERO,
        );
        assert_eq!(
            state.marquee_state().is_some(),
            kind == ClockEventKind::Marquee
        );
        state.set_aspect_ratio(0.75);
        assert_eq!(state.marquee_state(), None);
        assert_eq!(state.event_kind(), None);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert!(
            state
                .segments()
                .iter()
                .all(|s| s.representation == SegmentRepresentation::Anchored)
        );
    }
}

#[test]
fn combined_recipe_samples_all_three_independent_effects() {
    let event = MarqueeEvent::new(
        ClockMarqueePreset::TextRibbon,
        engine_common::ClockMarqueeMessage::default(),
        DisplaySnapshot::unsynchronized(),
    );
    assert!(event.recipe.scroll);
    assert!(event.recipe.wave.is_some());
    assert_eq!(event.recipe.lighting, Lighting::Sweep);
    let cell = event.content.cells[0];
    let sample = event
        .recipe
        .lighting
        .sample(cell, event.content.bounds, 1.0, 0.3);
    let later = event
        .recipe
        .lighting
        .sample(cell, event.content.bounds, 2.5, 0.5);
    assert_ne!(sample, later);
}
