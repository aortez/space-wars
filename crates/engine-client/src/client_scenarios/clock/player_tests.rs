use super::*;
use crate::input::{GameKey, GamepadSeatInput};
use engine_common::ClockEventProfile;

mod falling;
mod join;
mod meltdown;

fn scenario(viewport: Viewport) -> ClockClientScenario {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 34, 56).unwrap(),
        )],
        Duration::ZERO,
    );
    ClockClientScenario {
        state,
        last_emitted_reading: Cell::new(None),
        benchmark: None,
    }
}

fn step(scenario: &mut ClockClientScenario, input: &mut ClientInput) {
    let actions = scenario.map_input(input, false);
    scenario.step(&actions, Duration::from_nanos(16_666_667));
}

#[test]
fn player_input_needs_neutral_after_spawn_clear_and_new_session_and_respects_seats() {
    let mut scenario = scenario(Viewport::new(800.0, 480.0));
    let (input, gamepads) = crate::input::new_shared_input();
    let pad = |axis, jump| GamepadSeatInput {
        connected: true,
        left_stick_x: axis,
        east: jump,
        ..Default::default()
    };
    gamepads.borrow_mut().set_seat(1, pad(1.0, true));
    input.borrow_mut().request_clock_player_duck(2);
    step(&mut scenario, &mut input.borrow_mut());
    for _ in 0..60 {
        step(&mut scenario, &mut input.borrow_mut());
    }
    let current = scenario.state.player_duck_state().unwrap();
    assert_eq!(current.player, 2);
    assert_eq!(current.move_milli, 0);
    assert_eq!(current.duck.jumps, 0);
    gamepads.borrow_mut().set_seat(1, pad(0.0, false));
    step(&mut scenario, &mut input.borrow_mut());
    input.borrow_mut().press(GameKey::NesRight);
    gamepads.borrow_mut().set_seat(0, pad(1.0, true));
    step(&mut scenario, &mut input.borrow_mut());
    assert_eq!(
        scenario.state.player_duck_state().unwrap().move_milli,
        0,
        "P1 does not steer P2"
    );
    gamepads.borrow_mut().set_seat(1, pad(-1.0, true));
    step(&mut scenario, &mut input.borrow_mut());
    let current = scenario.state.player_duck_state().unwrap();
    assert_eq!(current.move_milli, -1000);
    assert_eq!(current.duck.jumps, 1, "Picade East jumps");
    input.borrow_mut().clear();
    step(&mut scenario, &mut input.borrow_mut());
    assert_eq!(scenario.state.player_duck_state().unwrap().move_milli, 0);
    assert!(!scenario.state.player_duck_state().unwrap().jump_held);
    gamepads.borrow_mut().disconnect_seat(1);
    step(&mut scenario, &mut input.borrow_mut());
    assert_eq!(scenario.state.player_duck_state().unwrap().move_milli, 0);
    scenario.state.set_aspect_ratio(0.6);
    input.borrow_mut().request_clock_player_duck(2);
    gamepads.borrow_mut().set_seat(1, pad(1.0, true));
    step(&mut scenario, &mut input.borrow_mut());
    step(&mut scenario, &mut input.borrow_mut());
    let new_visit = scenario.state.player_duck_state().unwrap();
    assert_ne!(new_visit.session_id, current.session_id);
    assert_eq!(new_visit.move_milli, 0);
    assert!(!new_visit.jump_held);
}

#[test]
fn player_duck_is_observable_and_renders_through_both_production_adapters() {
    use crate::thruster_visual_tests::{raster, svg, write_png};
    let output = std::env::var_os("SPACEWARS_CLOCK_PLAYER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (name, viewport) in [
        ("hyperpixel", Viewport::new(800.0, 480.0)),
        ("picade", Viewport::new(1024.0, 768.0)),
        ("portrait", Viewport::new(480.0, 800.0)),
    ] {
        let mut scenario = scenario(viewport);
        scenario.step(&[ClockAction::toggle_player_duck(1)], Duration::ZERO);
        for _ in 0..90 {
            scenario.step(&[], Duration::from_nanos(16_666_667));
        }
        let current = scenario.clock_state().unwrap();
        assert_eq!(current.event_kind, None);
        assert!(!current.automatic_events_suspended);
        assert_eq!(current.player_duck.unwrap().player, 1);
        assert!(current.duck.is_none());
        assert!(current.can_trigger);
        assert_eq!(
            current
                .events
                .iter()
                .filter(|event| event.blocked_by_player)
                .count(),
            1
        );
        assert!(current.body_count <= 9 && current.collider_count <= 9);
        let frames = scenario.render_frames(RenderBackend::Raster, viewport);
        assert_eq!(
            frames,
            scenario.render_frames(RenderBackend::Vector, viewport)
        );
        let pixels = raster(&frames[0], viewport);
        let vector = svg(&frames[0], viewport);
        assert!(!vector.contains("NaN") && !vector.contains("inf"));
        // Text is composited by Slint over the raster image. The geometry-only
        // SVG exporter omits text, so inspect the production adapter directly.
        assert!(
            crate::render::scene_primitives_from_frames(&frames, viewport)
                .iter()
                .any(|item| item.text == "P1 DUCK")
        );
        assert_eq!(pixels.width(), viewport.width as u32);
        assert!(
            pixels
                .as_slice()
                .iter()
                .any(|p| p.r > 230 && p.g > 130 && p.b < 100),
            "visible duck/course"
        );
        if let Some(output) = &output {
            write_png(&output.join(format!("player-duck-{name}.png")), &pixels);
            std::fs::write(output.join(format!("player-duck-{name}.svg")), vector).unwrap();
        }
    }
}

#[test]
fn visual_events_leave_the_player_and_course_visible_in_all_layouts() {
    use crate::thruster_visual_tests::{raster, write_png};
    use engine_common::ClockEventKind;
    let output = std::env::var_os("SPACEWARS_CLOCK_PLAYER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (name, viewport) in [
        ("hyperpixel", Viewport::new(800.0, 480.0)),
        ("picade", Viewport::new(1024.0, 768.0)),
        ("portrait", Viewport::new(480.0, 800.0)),
    ] {
        for event in [
            ClockEventKind::ColorCycle,
            ClockEventKind::Marquee,
            ClockEventKind::DigitSlide,
        ] {
            let mut baseline = scenario(viewport);
            let mut composed = scenario(viewport);
            for scene in [&mut baseline, &mut composed] {
                scene.step(&[ClockAction::toggle_player_duck(1)], Duration::ZERO);
                for _ in 0..90 {
                    scene.step(&[], Duration::from_nanos(16_666_667));
                }
            }
            composed.step(&[ClockAction::preview_event(event)], Duration::ZERO);
            for _ in 0..if event == ClockEventKind::DigitSlide {
                24
            } else {
                120
            } {
                for scene in [&mut baseline, &mut composed] {
                    scene.step(&[], Duration::from_nanos(16_666_667));
                }
            }
            assert_eq!(composed.state.event_kind(), Some(event));
            assert_eq!(
                composed.state.player_duck_state(),
                baseline.state.player_duck_state()
            );
            let frames = composed.render_frames(RenderBackend::Raster, viewport);
            assert_eq!(
                frames,
                composed.render_frames(RenderBackend::Vector, viewport)
            );
            assert!(
                crate::render::scene_primitives_from_frames(&frames, viewport)
                    .iter()
                    .any(|item| item.text == "P1 DUCK")
            );
            let pixels = raster(&frames[0], viewport);
            let reference = raster(
                &baseline.render_frames(RenderBackend::Raster, viewport)[0],
                viewport,
            );
            let lower = (pixels.width() * (pixels.height() * 3 / 4)) as usize;
            assert_eq!(
                &pixels.as_slice()[lower..],
                &reference.as_slice()[lower..],
                "player/course changed under {event:?} on {name}"
            );
            if let Some(output) = &output {
                write_png(
                    &output.join(format!("player-duck-{name}-{event:?}.png")),
                    &pixels,
                );
            }
        }
    }
}

#[test]
fn rain_and_player_share_a_visible_course_in_all_layouts() {
    use crate::thruster_visual_tests::{raster, svg, write_png};
    use engine_common::{ClockEventKind, ClockRainAmount};
    let output = std::env::var_os("SPACEWARS_CLOCK_PLAYER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (name, viewport) in [
        ("hyperpixel", Viewport::new(800.0, 480.0)),
        ("picade", Viewport::new(1024.0, 768.0)),
        ("portrait", Viewport::new(480.0, 800.0)),
    ] {
        let mut scene = scenario(viewport);
        let mut settings = scene.state.settings();
        settings.rain_amount = ClockRainAmount::Heavy;
        scene.step(
            &[
                ClockAction::configure(settings),
                ClockAction::toggle_player_duck(1),
            ],
            Duration::ZERO,
        );
        for _ in 0..90 {
            scene.step(&[], Duration::from_nanos(16_666_667));
        }
        let current = scene.state.player_duck_state().unwrap();
        let target = current.duck.position_milli.unwrap()[0] as f32 / 1000.0;
        scene.step(
            &[ClockAction::preview_event(ClockEventKind::Rain)],
            Duration::ZERO,
        );
        for tick in 1..=900 {
            let p = scene.state.player_duck_state().unwrap();
            let x = p.duck.position_milli.unwrap()[0] as f32 / 1000.0;
            let vx = p.velocity_milli.unwrap()[0] as f32 / 1000.0;
            let axis = (((target - x) * 0.05 - vx * 0.03).clamp(-1.0, 1.0) * 1000.0) as i16;
            scene.step(
                &[ClockAction::player_duck_input(
                    scenario_clock::ClockDuckInput {
                        session_id: current.session_id,
                        player: 1,
                        move_milli: axis,
                        jump: false,
                    },
                )],
                Duration::from_nanos(16_666_667),
            );
            if tick != 300 && tick != 900 {
                continue;
            }
            let state = scene.clock_state().unwrap();
            assert_eq!(state.player_duck.unwrap().session_id, current.session_id);
            assert!(state.rain.unwrap().player_course);
            assert!(state.body_count <= 9);
            let frames = scene.render_frames(RenderBackend::Raster, viewport);
            assert_eq!(frames, scene.render_frames(RenderBackend::Vector, viewport));
            let pixels = raster(&frames[0], viewport);
            assert!(
                pixels
                    .as_slice()
                    .iter()
                    .any(|p| p.r > 230 && p.g > 180 && p.b < 50)
            );
            if let Some(output) = &output {
                write_png(
                    &output.join(format!("player-rain-{name}-{tick}.png")),
                    &pixels,
                );
                std::fs::write(
                    output.join(format!("player-rain-{name}-{tick}.svg")),
                    svg(&frames[0], viewport),
                )
                .unwrap();
            }
        }
        scene.step(&[ClockAction::toggle_player_duck(1)], Duration::ZERO);
        for _ in 0..30 {
            scene.step(&[], Duration::from_nanos(16_666_667));
        }
        assert!(scene.state.player_duck_state().is_none());
        assert!(scene.state.rain_state().unwrap().player_course);
        let frames = scene.render_frames(RenderBackend::Raster, viewport);
        if let Some(output) = &output {
            write_png(
                &output.join(format!("rain-course-{name}-after-exit.png")),
                &raster(&frames[0], viewport),
            );
        }
    }
}

#[test]
fn player_joins_live_rain_and_keeps_visible_panels_through_cleanup_in_all_layouts() {
    use crate::thruster_visual_tests::{raster, svg, write_png};
    use engine_common::{ClockEventKind, ClockRainAmount, ClockRainDuckPhase};
    let output = std::env::var_os("SPACEWARS_CLOCK_PLAYER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (name, viewport) in [
        ("hyperpixel", Viewport::new(800.0, 480.0)),
        ("picade", Viewport::new(1024.0, 768.0)),
        ("portrait", Viewport::new(480.0, 800.0)),
    ] {
        let mut scene = scenario(viewport);
        let mut settings = scene.state.settings();
        settings.rain_amount = ClockRainAmount::Heavy;
        scene.step(
            &[
                ClockAction::configure(settings),
                ClockAction::preview_event(ClockEventKind::Rain),
            ],
            Duration::ZERO,
        );
        for _ in 0..1200 {
            scene.step(&[], Duration::from_nanos(16_666_667));
            if scene.state.rain_state().unwrap().duck_phase == ClockRainDuckPhase::Floating {
                break;
            }
        }
        assert_eq!(
            scene.state.rain_state().unwrap().duck_phase,
            ClockRainDuckPhase::Floating
        );
        let id = scene.state.event_id();
        scene.step(&[ClockAction::toggle_player_duck(1)], Duration::ZERO);
        assert_eq!(scene.state.event_id(), id);
        let current = scene.state.player_duck_state().unwrap();
        let target = current.duck.position_milli.unwrap()[0] as f32 / 1000.0;
        for tick in 0..=960 {
            let p = scene.state.player_duck_state().unwrap();
            let x = p.duck.position_milli.unwrap()[0] as f32 / 1000.0;
            let vx = p.velocity_milli.unwrap()[0] as f32 / 1000.0;
            let axis = (((target - x) * 0.05 - vx * 0.03).clamp(-1.0, 1.0) * 1000.0) as i16;
            scene.step(
                &[ClockAction::player_duck_input(
                    scenario_clock::ClockDuckInput {
                        session_id: current.session_id,
                        player: 1,
                        move_milli: axis,
                        jump: false,
                    },
                )],
                Duration::from_nanos(16_666_667),
            );
            if tick == 720 {
                scene.step(
                    &[ClockAction::preview_event(ClockEventKind::Marquee)],
                    Duration::ZERO,
                );
            }
            if ![0, 300, 719, 960].contains(&tick) {
                continue;
            }
            let state = scene.clock_state().unwrap();
            let player = state.player_duck.unwrap();
            assert_eq!(player.session_id, current.session_id);
            assert!(player.floor_open_milli.is_some());
            assert_eq!((state.body_count, state.collider_count), (4, 4));
            if let Some(rain) = state.rain {
                assert!(rain.player_joined && !rain.player_course);
                assert_eq!(rain.duck_phase, ClockRainDuckPhase::HandedOff);
            }
            let frames = scene.render_frames(RenderBackend::Raster, viewport);
            assert_eq!(frames, scene.render_frames(RenderBackend::Vector, viewport));
            let pixels = raster(&frames[0], viewport);
            assert!(
                pixels
                    .as_slice()
                    .iter()
                    .any(|p| p.r > 230 && p.g > 180 && p.b < 50)
            );
            if let Some(output) = &output {
                write_png(
                    &output.join(format!("player-panels-{name}-{tick}.png")),
                    &pixels,
                );
                std::fs::write(
                    output.join(format!("player-panels-{name}-{tick}.svg")),
                    svg(&frames[0], viewport),
                )
                .unwrap();
            }
        }
    }
}
