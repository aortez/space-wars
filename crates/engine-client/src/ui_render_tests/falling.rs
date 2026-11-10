//! Reading changes during Falling, captured through the production renderer.
use crate::{
    render::Viewport,
    thruster_visual_tests::{raster, write_png},
};
use engine_common::{ClockEventKind, ClockEventProfile, ClockFont, ClockFontSettings, Scenario};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario};
use std::time::Duration;

#[test]
fn falling_reading_changes_return_to_the_complete_face_at_target_sizes() {
    let output = std::env::var_os("SPACEWARS_FALLING_ARTIFACTS").map(std::path::PathBuf::from);
    for (name, (width, height), shared, font, from, to, change_at) in [
        (
            "rollover",
            (800, 480),
            false,
            ClockFont::Classic,
            (11, 11),
            (11, 12),
            180,
        ),
        (
            "shared-rollover",
            (1024, 768),
            true,
            ClockFont::Classic,
            (11, 11),
            (11, 12),
            180,
        ),
        (
            "correction",
            (480, 800),
            false,
            ClockFont::Matrix,
            (11, 11),
            (8, 8),
            180,
        ),
        (
            "during-return",
            (800, 480),
            false,
            ClockFont::Classic,
            (11, 11),
            (11, 12),
            240,
        ),
        (
            "sparser",
            (800, 480),
            false,
            ClockFont::Classic,
            (11, 12),
            (11, 11),
            240,
        ),
    ] {
        let viewport = Viewport::new(width as f32, height as f32);
        let config = ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            fonts: ClockFontSettings {
                selected: font,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut state = ClockScenario::init(config, 7);
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(from.0, from.1, 0).unwrap(),
            )],
            Duration::ZERO,
        );
        let dt = Duration::from_nanos(16_666_667);
        if shared {
            ClockScenario::step(
                &mut state,
                &[ClockAction::toggle_player_duck(1)],
                Duration::ZERO,
            );
            for _ in 0..90 {
                ClockScenario::step(&mut state, &[], dt);
            }
        }
        let base = (state.body_count(), state.collider_count());
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Falling)],
            Duration::ZERO,
        );
        let path = output.as_ref().map(|root| root.join(name));
        if let Some(path) = &path {
            std::fs::create_dir_all(path.join("film")).unwrap();
        }
        let latest = ClockReading::new(to.0, to.1, 0).unwrap();
        for tick in 0..=360 {
            if tick == change_at {
                ClockScenario::step(
                    &mut state,
                    &[ClockAction::set_reading(latest)],
                    Duration::ZERO,
                );
            }
            let sample =
                [0, change_at, change_at + 1, 209, 210, 225, 255, 285, 360].contains(&tick);
            if sample || (path.is_some() && tick % 2 == 0) {
                let pixels = raster(&ClockScenario::render_frame(&state), viewport);
                if let Some(path) = &path {
                    if sample {
                        write_png(&path.join(format!("tick-{tick:03}.png")), &pixels);
                    }
                    if tick % 2 == 0 {
                        write_png(
                            &path.join(format!("film/frame-{:04}.png", tick / 2)),
                            &pixels,
                        );
                    }
                }
                if tick == 360 {
                    let mut reference = ClockScenario::init(config, 7);
                    ClockScenario::step(
                        &mut reference,
                        &[ClockAction::set_reading(latest)],
                        Duration::ZERO,
                    );
                    let face = raster(&ClockScenario::render_frame(&reference), viewport);
                    let start = if shared {
                        (width * (height / 8) * 3) as usize
                    } else {
                        0
                    };
                    let end = if shared {
                        (width * (height * 3 / 4) * 3) as usize
                    } else {
                        face.as_bytes().len()
                    };
                    assert!(
                        pixels.as_bytes()[start..end] == face.as_bytes()[start..end],
                        "{name}: the latest face is restored"
                    );
                }
            }
            if tick < 360 {
                ClockScenario::step(&mut state, &[], dt);
            }
        }
        assert!(state.event_kind().is_none(), "{name}");
        assert_eq!((state.body_count(), state.collider_count()), base, "{name}");
        assert_eq!(state.player_duck_state().is_some(), shared, "{name}");
    }
}
