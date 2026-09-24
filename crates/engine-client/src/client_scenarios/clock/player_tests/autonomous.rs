use super::*;
use crate::thruster_visual_tests::{raster, svg, write_png};
use engine_common::{ClockDuckBehavior, ClockEventKind, ClockRainAmount};

#[test]
fn automatic_duck_rain_and_recovery_render_in_all_layouts() {
    let output = std::env::var_os("SPACEWARS_CLOCK_PLAYER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (device, viewport) in [
        ("picade", Viewport::new(1024.0, 768.0)),
        ("hyperpixel", Viewport::new(800.0, 480.0)),
        ("portrait", Viewport::new(480.0, 800.0)),
    ] {
        let mut scene = scenario(viewport);
        let mut settings = scene.state.settings();
        settings.rain_amount = ClockRainAmount::Heavy;
        scene.step(
            &[
                ClockAction::configure(settings),
                ClockAction::preview_event(ClockEventKind::Duck),
            ],
            Duration::ZERO,
        );
        for _ in 0..100 {
            scene.step(&[], Duration::from_nanos(16_666_667));
        }
        scene.step(
            &[ClockAction::preview_event(ClockEventKind::Rain)],
            Duration::ZERO,
        );
        let mut wet_capture = false;
        let mut recovery_capture = false;
        let mut landings_before_dry = 0;
        for tick in 1..=1600 {
            if tick == 901 {
                let nav = scene.state.duck_state().unwrap().navigation.unwrap();
                landings_before_dry = nav.planning.unwrap().confirmed_landings;
                scene.step(
                    &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
                    Duration::ZERO,
                );
            }
            scene.step(&[], Duration::from_nanos(16_666_667));
            let state = scene.clock_state().unwrap();
            assert!(state.player_duck.is_none());
            let duck = state.duck.expect("one resident automatic duck");
            assert_eq!(duck.outcome, None, "{device}, {tick}: {duck:?}");
            let nav = duck.navigation.unwrap();
            let capture = if !wet_capture
                && nav.behavior == ClockDuckBehavior::Paddling
                && nav.water.paddling_ticks >= 180
            {
                wet_capture = true;
                Some("paddling")
            } else if !recovery_capture
                && tick > 901
                // Some layouts already regain support between showers. Require
                // a new post-Rain landing, not an extra artificial wet cycle.
                && nav.water.recoveries > 0
                && duck.visit.unwrap().submerged_milli == 0
                && !matches!(nav.behavior, ClockDuckBehavior::Paddling | ClockDuckBehavior::Recovering)
                && nav.planning.unwrap().confirmed_landings > landings_before_dry
            {
                recovery_capture = true;
                Some("dry-recovery")
            } else if tick == 900 {
                Some("heavy-rain")
            } else {
                None
            };
            let Some(stage) = capture else { continue };
            let frames = scene.render_frames(RenderBackend::Raster, viewport);
            assert_eq!(frames, scene.render_frames(RenderBackend::Vector, viewport));
            let pixels = raster(&frames[0], viewport);
            let vector = svg(&frames[0], viewport);
            assert!(!vector.contains("NaN") && !vector.contains("inf"));
            assert!(
                pixels
                    .as_slice()
                    .iter()
                    .filter(|p| p.r > 230 && p.g > 180 && p.b < 50)
                    .count()
                    > 10,
                "visible automatic duck"
            );
            let frozen = scene.state.duck_state();
            scene.step(&[], Duration::ZERO);
            assert_eq!(scene.state.duck_state(), frozen);
            assert_eq!(scene.render_frames(RenderBackend::Raster, viewport), frames);
            if let Some(output) = &output {
                let file = format!("automatic-rain-{device}-{stage}");
                write_png(&output.join(format!("{file}.png")), &pixels);
                std::fs::write(output.join(format!("{file}.svg")), vector).unwrap();
            }
            if recovery_capture {
                break;
            }
        }
        assert!(
            wet_capture && recovery_capture,
            "{device}: {:?}",
            scene.state.duck_state()
        );
    }
}
