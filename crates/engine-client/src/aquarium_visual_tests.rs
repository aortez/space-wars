//! Repeatable aquarium captures through both production presentation adapters.
use crate::{
    render::Viewport,
    thruster_visual_tests::{raster, svg, write_png},
};
use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
use scenario_clock::{ClockAction, ClockConfig, ClockDuckInput, ClockReading, ClockScenario};
use std::{path::PathBuf, time::Duration};

#[test]
fn aquarium_duck_dives_and_recovers_through_public_controls_in_both_adapters() {
    let output = std::env::var_os("SPACEWARS_AQUARIUM_ARTIFACTS").map(PathBuf::from);
    if let Some(directory) = &output {
        std::fs::create_dir_all(directory).unwrap();
    }
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        let viewport = Viewport::new(width as f32, height as f32);
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: viewport.aspect_ratio(),
                event_profile: ClockEventProfile::Off,
                ..Default::default()
            },
            177,
        );
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(ClockReading::new(12, 38, 0).unwrap()),
                ClockAction::preview_event(ClockEventKind::Aquarium),
            ],
            Duration::ZERO,
        );
        for _ in 0..600 {
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        ClockScenario::step(
            &mut state,
            &[ClockAction::toggle_player_duck(1)],
            Duration::ZERO,
        );
        let (session_id, player) = state.player_duck_session().unwrap();
        let mut submerged_ticks = 0;
        let mut surface_again = false;
        for tick in 0..=780 {
            let input = ClockDuckInput {
                session_id,
                player,
                move_milli: if (120..360).contains(&tick) { 1000 } else { 0 },
                jump: false,
                run: false,
                dive: (120..420).contains(&tick),
            };
            ClockScenario::step(
                &mut state,
                &[ClockAction::player_duck_input(input)],
                Duration::from_nanos(16_666_667),
            );
            let duck = state.player_duck_state().expect("same live visit");
            assert_eq!(duck.session_id, session_id);
            submerged_ticks += usize::from(duck.submerged_milli > 990);
            surface_again |= tick > 480 && (400..550).contains(&duck.submerged_milli);
            if [90, 150, 210, 270, 330, 390, 480, 540, 660, 780].contains(&tick) {
                let frame = ClockScenario::render_frame(&state);
                let pixels = raster(&frame, viewport);
                let vector = svg(&frame, viewport);
                assert!(!vector.contains("NaN") && !vector.contains("inf"));
                if let Some(dir) = &output {
                    let name = format!("duck-{width}x{height}-{tick:04}");
                    write_png(&dir.join(format!("{name}.png")), &pixels);
                    std::fs::write(dir.join(format!("{name}.svg")), vector).unwrap();
                    std::fs::write(
                        dir.join(format!("{name}.json")),
                        serde_json::to_string_pretty(
                            &serde_json::json!({"duck":duck,"aquarium":state.aquarium_state()}),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                }
            }
        }
        assert!(
            submerged_ticks >= 90,
            "sustained dive at {width}x{height}: {submerged_ticks}"
        );
        assert!(
            surface_again,
            "release/exhaustion restores flotation at {width}x{height}"
        );
        assert_eq!(state.player_duck_state().unwrap().dive_ticks_left, 240);
    }
}

#[test]
fn aquarium_isolated_drain_loads_render_in_both_adapters() {
    use scenario_clock::water_fixture::{RETAINED_DEPTHS, RetainedDrainFixture};
    let output = std::env::var_os("SPACEWARS_AQUARIUM_ARTIFACTS").map(PathBuf::from);
    if let Some(directory) = &output {
        std::fs::create_dir_all(directory).unwrap();
    }
    let viewport = Viewport::new(800.0, 600.0);
    for (name, depth) in RETAINED_DEPTHS {
        let mut fixture = RetainedDrainFixture::new(depth);
        for tick in 0..=480 {
            if [0, 30, 60, 90, 120, 180, 300, 480].contains(&tick) {
                let frame = fixture.frame();
                let pixels = raster(&frame, viewport);
                let vector = svg(&frame, viewport);
                assert!(!vector.contains("NaN") && !vector.contains("inf"));
                if let Some(directory) = &output {
                    let name = format!("drain-{name}-{tick:04}");
                    write_png(&directory.join(format!("{name}.png")), &pixels);
                    std::fs::write(directory.join(format!("{name}.svg")), vector).unwrap();
                    let s = fixture.water.stats();
                    std::fs::write(
                        directory.join(format!("{name}.json")),
                        serde_json::json!({
                            "tick": tick, "initial_depth": depth,
                            "tilt_degrees": fixture.angle_degrees(),
                            "injected": s.injected, "pooled": s.pooled,
                            "in_flight": s.in_flight, "drained": s.drained,
                            "reclaimed": s.reclaimed, "parcels": s.parcels,
                            "capacity_limited_ticks": s.capacity_limited_ticks
                        })
                        .to_string(),
                    )
                    .unwrap();
                }
            }
            fixture.step(1.0 / 60.0);
        }
    }
}

#[test]
fn aquarium_full_cycle_renders_in_both_adapters() {
    let output = std::env::var_os("SPACEWARS_AQUARIUM_ARTIFACTS").map(PathBuf::from);
    if let Some(directory) = &output {
        std::fs::create_dir_all(directory).unwrap();
    }
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        let viewport = Viewport::new(width as f32, height as f32);
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: viewport.aspect_ratio(),
                event_profile: ClockEventProfile::Off,
                ..Default::default()
            },
            177,
        );
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(ClockReading::new(12, 38, 0).unwrap()),
                ClockAction::preview_event(ClockEventKind::Aquarium),
            ],
            Duration::ZERO,
        );
        for tick in 0..=3000 {
            if [
                0, 180, 360, 720, 1200, 1740, 1860, 2040, 2099, 2100, 2130, 2160, 2190, 2220, 2280,
                2400, 2580, 2700, 2820, 2910, 2999, 3000,
            ]
            .contains(&tick)
            {
                let frame = ClockScenario::render_frame(&state);
                let pixels = raster(&frame, viewport);
                let vector = svg(&frame, viewport);
                assert!(!vector.contains("NaN") && !vector.contains("inf"));
                if let Some(directory) = &output {
                    let name = format!("{width}x{height}-{tick:04}");
                    write_png(&directory.join(format!("{name}.png")), &pixels);
                    std::fs::write(directory.join(format!("{name}.svg")), vector).unwrap();
                    std::fs::write(
                        directory.join(format!("{name}.json")),
                        serde_json::to_string_pretty(&state.aquarium_state()).unwrap(),
                    )
                    .unwrap();
                }
            }
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        assert!(state.aquarium_state().is_none());
    }
}
