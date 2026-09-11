use super::*;
use crate::input::{GamepadInput, GamepadSeatInput};
use crate::raster::{RasterOptions, RasterRenderer};
use std::{cell::RefCell, rc::Rc};

mod tools;

#[test]
fn mining_maps_aim_and_trigger_and_releases_after_clear_or_disconnect() {
    let pads = Rc::new(RefCell::new(GamepadInput::default()));
    let mut input = ClientInput::new(Rc::clone(&pads));
    input.press(GameKey::TerrainDrill);
    input.press(GameKey::P1Thrust);
    assert_eq!(
        TerrainLabAction::decode(&mining_controls(&input)),
        Some(TerrainLabAction::Mining(MiningControls {
            held: true,
            turn: 1.0,
            ..Default::default()
        }))
    );
    input.clear();
    assert_eq!(
        TerrainLabAction::decode(&mining_controls(&input)),
        Some(TerrainLabAction::Mining(MiningControls::default()))
    );
    pads.borrow_mut().set_seat(
        0,
        GamepadSeatInput {
            connected: true,
            right_stick_x: -1.0,
            right_stick_y: 0.8,
            right_trigger: 1.0,
            ..Default::default()
        },
    );
    let Some(TerrainLabAction::Mining(controls)) =
        TerrainLabAction::decode(&mining_controls(&input))
    else {
        panic!("mining controls");
    };
    assert!(controls.held);
    assert_eq!(controls.aim.x, -1.0);
    assert!(
        controls.aim.y > 0.5,
        "right-stick up aims upward in world coordinates"
    );
    pads.borrow_mut().disconnect_seat(0);
    assert_eq!(
        TerrainLabAction::decode(&mining_controls(&input)),
        Some(TerrainLabAction::Mining(MiningControls::default()))
    );
}

#[test]
fn retro_gamepad_can_rotate_aim_and_drill_without_a_right_stick() {
    let pads = Rc::new(RefCell::new(GamepadInput::default()));
    let mut input = ClientInput::new(Rc::clone(&pads));
    for (up, down, expected_turn) in [
        (true, false, 1.0),
        (false, true, -1.0),
        (true, true, 0.0),
        (false, false, 0.0),
    ] {
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                left_bumper: true,
                dpad_up: up,
                dpad_down: down,
                ..Default::default()
            },
        );
        assert_eq!(
            TerrainLabAction::decode(&mining_controls(&input)),
            Some(TerrainLabAction::Mining(MiningControls {
                held: true,
                turn: expected_turn,
                ..Default::default()
            }))
        );
    }
    input.press(GameKey::P1Brake);
    assert_eq!(
        TerrainLabAction::decode(&mining_controls(&input)),
        Some(TerrainLabAction::Mining(MiningControls {
            held: true,
            turn: -1.0,
            ..Default::default()
        }))
    );
    input.clear();
    pads.borrow_mut().disconnect_seat(0);
    assert_eq!(
        TerrainLabAction::decode(&mining_controls(&input)),
        Some(TerrainLabAction::Mining(MiningControls::default()))
    );
}

#[test]
fn mining_updates_the_beam_and_recovery_hud_in_both_renderers() {
    let viewport = Viewport::new(800.0, 480.0);
    let mut lab = TerrainLabClientScenario {
        state: TerrainLabScenario::init(
            TerrainLabConfig {
                angular_velocity: 0.0,
                orbit_radius: 0.0,
                ..Default::default()
            },
            42,
        ),
    };
    for _ in 0..120 {
        lab.step(&[], Duration::from_secs_f64(1.0 / 60.0));
    }
    let mut input = ClientInput::default();
    input.press(GameKey::TerrainDrill);
    for _ in 0..100 {
        let actions = lab.map_input(&mut input, false);
        lab.step(&actions, Duration::from_secs_f64(1.0 / 60.0));
    }
    assert!(lab.state.recovered.rock_cells + lab.state.recovered.ore_cells > 0);
    let frames = lab.render_frames(RenderBackend::Vector, viewport);
    assert_eq!(frames, lab.render_frames(RenderBackend::Raster, viewport));
    let overlay = crate::render::raster_text_overlay(&frames, viewport, lab.frame_layout());
    assert!(
        overlay
            .iter()
            .any(|primitive| primitive.text.starts_with("DRILLING"))
    );
    assert!(overlay.iter().any(|primitive| primitive.text
        == format!(
            "Recovered: Rock {:.2} u²   Ore {:.2} u²   |   Fragments: {}",
            lab.state.recovered_area(scenario_terrain_lab::ROCK),
            lab.state.recovered_area(scenario_terrain_lab::ORE),
            lab.state.fragments().len(),
        )));
    let scene = crate::render::scene_primitives_from_frames(&frames, viewport);
    assert!(
        scene
            .iter()
            .any(|primitive| primitive.text.starts_with("Recovered:"))
    );
    let mut renderer = RasterRenderer::new();
    let image = renderer.image_from_frames_with_layout(
        &frames,
        viewport,
        lab.frame_layout(),
        RasterOptions::default(),
    );
    let pixels = image.to_rgb8().unwrap();
    assert!(
        pixels
            .as_slice()
            .iter()
            .filter(|pixel| pixel.r > 200 && pixel.g > 90 && pixel.b < 100)
            .count()
            > 15,
        "character and active drill must be rasterized"
    );
    if let Some(directory) = std::env::var_os("SPACEWARS_TERRAIN_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let file = std::fs::File::create(directory.join("mining.png")).unwrap();
        let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(pixels.as_bytes())
            .unwrap();
    }
    input.release(GameKey::TerrainDrill);
    let actions = lab.map_input(&mut input, false);
    lab.step(&actions, Duration::from_secs_f64(1.0 / 60.0));
    assert!(!lab.state.mining_snapshot().active);
}

#[test]
fn tools_map_from_keyboard_and_gamepad_and_release_on_disconnect() {
    let pads = Rc::new(RefCell::new(GamepadInput::default()));
    let mut input = ClientInput::new(Rc::clone(&pads));
    input.press(GameKey::P1Cannon);
    input.press(GameKey::P1Wing);
    let decoded = TerrainLabAction::decode(&controls(&input)).unwrap();
    assert_eq!(
        decoded,
        TerrainLabAction::Controls(scenario_terrain_lab::LabControls {
            tunnel: true,
            debug: true,
            ..Default::default()
        })
    );
    input.release(GameKey::P1Cannon);
    input.release(GameKey::P1Wing);
    pads.borrow_mut().set_seat(
        0,
        GamepadSeatInput {
            connected: true,
            south: true,
            east: true,
            west: true,
            left_trigger: 1.0,
            right_bumper: true,
            ..Default::default()
        },
    );
    assert_eq!(
        TerrainLabAction::decode(&controls(&input)),
        Some(TerrainLabAction::Controls(
            scenario_terrain_lab::LabControls {
                jump: true,
                crater: true,
                tunnel: true,
                debug: true,
                ..Default::default()
            }
        ))
    );
    pads.borrow_mut().set_seat(
        0,
        GamepadSeatInput {
            connected: false,
            south: true,
            east: true,
            west: true,
            right_bumper: true,
            ..Default::default()
        },
    );
    assert_eq!(
        TerrainLabAction::decode(&controls(&input)),
        Some(TerrainLabAction::Controls(Default::default()))
    );
}

#[test]
fn launcher_factory_and_both_renderers_show_a_real_hole() {
    let viewport = Viewport::new(1280.0, 720.0);
    let factory = REGISTRATION
        .create(
            17,
            &Settings::default(),
            viewport,
            ScenarioStartMode::Normal,
        )
        .unwrap();
    assert_eq!(factory.registration().id, "terrain-lab");
    assert!(factory.registration().capabilities.pointer_input);
    let mut lab = TerrainLabClientScenario {
        state: TerrainLabScenario::init(
            TerrainLabConfig {
                angular_velocity: 0.0,
                orbit_radius: 0.0,
                ..Default::default()
            },
            17,
        ),
    };
    let mut renderer = RasterRenderer::new();
    let mut samples = Vec::new();
    for name in ["intact", "tunnel", "crater", "overlay"] {
        let action = TerrainLabAction::controls(
            0.0,
            false,
            name == "crater",
            name == "tunnel",
            name != "intact",
        );
        lab.step(&[action], Duration::from_secs_f64(1.0 / 60.0));
        let frames = lab.render_frames(RenderBackend::Vector, viewport);
        assert_eq!(frames, lab.render_frames(RenderBackend::Raster, viewport));
        let scene = crate::render::scene_primitives_from_frames(&frames, viewport);
        assert!(scene.len() > 40);
        let overlay = crate::render::raster_text_overlay(&frames, viewport, lab.frame_layout());
        assert!(
            overlay
                .iter()
                .any(|primitive| primitive.text == "TERRAIN LAB")
        );
        let image = renderer.image_from_frames_with_layout(
            &frames,
            viewport,
            lab.frame_layout(),
            RasterOptions::default(),
        );
        let pixels = image.to_rgb8().unwrap();
        let pixel = pixels.as_slice()[370 * 1280 + 640];
        samples.push((pixel.r, pixel.g, pixel.b));
        if let Some(directory) = std::env::var_os("SPACEWARS_TERRAIN_ARTIFACTS") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let file = std::fs::File::create(directory.join(format!("{name}.png"))).unwrap();
            let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(pixels.as_bytes())
                .unwrap();
        }
    }
    assert_ne!(
        samples[0], samples[1],
        "tunnel must remove rasterized material"
    );
    assert!(
        samples[1].0 < 20 && samples[1].1 < 20 && samples[1].2 < 30,
        "tunnel interior must show background: {:?}",
        samples[1]
    );
}
