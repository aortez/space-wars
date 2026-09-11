use super::*;
use engine_common::RenderPrimitive;
use scenario_terrain_lab::{MiningTool, TerrainView};

#[test]
fn controller_tool_and_view_buttons_are_independent_of_debug_cuts() {
    let pads = Rc::new(RefCell::new(GamepadInput::default()));
    let mut input = ClientInput::new(Rc::clone(&pads));
    let mut lab = TerrainLabClientScenario {
        state: TerrainLabScenario::init(TerrainLabConfig::default(), 42),
    };
    pads.borrow_mut().set_seat(
        0,
        GamepadSeatInput {
            connected: true,
            north: true,
            right_bumper: true,
            east: true,
            west: true,
            ..Default::default()
        },
    );
    let hash = lab.state.terrain_hash();
    for _ in 0..15 {
        let actions = lab.map_input(&mut input, false);
        lab.step(&actions, Duration::from_secs_f64(1.0 / 60.0));
    }
    assert_eq!(lab.state.selected_tool(), MiningTool::Excavator);
    assert_eq!(lab.state.view, TerrainView::Mining);
    assert_eq!(
        lab.state.terrain_hash(),
        hash,
        "face buttons alone cannot make debug cuts"
    );
    assert!(!lab.state.overlay);
    pads.borrow_mut().disconnect_seat(0);
    let actions = lab.map_input(&mut input, false);
    lab.step(&actions, Duration::from_secs_f64(1.0 / 60.0));
    assert_eq!(
        TerrainLabAction::decode(&tool_controls(&input)),
        Some(TerrainLabAction::Tools(ToolControls::default()))
    );
    input.press(GameKey::TerrainTool);
    input.press(GameKey::TerrainView);
    let actions = lab.map_input(&mut input, false);
    lab.step(&actions, Duration::from_secs_f64(1.0 / 60.0));
    assert_eq!(lab.state.selected_tool(), MiningTool::Precision);
    assert_eq!(lab.state.view, TerrainView::Detail);
    input.clear();
    assert_eq!(
        TerrainLabAction::decode(&tool_controls(&input)),
        Some(TerrainLabAction::Tools(ToolControls::default()))
    );
}

#[test]
fn every_tool_and_zoom_level_keeps_the_hud_visible_in_both_renderers() {
    let viewport = Viewport::new(800.0, 480.0);
    let mut lab = TerrainLabClientScenario {
        state: TerrainLabScenario::init(TerrainLabConfig::default(), 42),
    };
    let mut raster = RasterRenderer::new();
    for tool in MiningTool::ALL {
        lab.state.select_tool(tool);
        for view in [
            TerrainView::Overview,
            TerrainView::Mining,
            TerrainView::Detail,
        ] {
            lab.state.set_view(view);
            let frames = lab.render_frames(RenderBackend::Vector, viewport);
            assert_eq!(frames, lab.render_frames(RenderBackend::Raster, viewport));
            let frame = &frames[0];
            let preview_count = lab
                .state
                .terrain()
                .brush_cells(lab.state.mining_snapshot().brush.unwrap())
                .unwrap()
                .count();
            assert!(preview_count > 0);
            assert_eq!(
                frame
                    .layers
                    .iter()
                    .find(|layer| layer.z == 2)
                    .unwrap()
                    .primitives
                    .len(),
                preview_count
            );
            for layer in &frame.layers {
                for primitive in &layer.primitives {
                    if let RenderPrimitive::Text(text) = primitive {
                        let position = frame.camera.world_to_viewport(text.position, 800.0 / 480.0);
                        assert!(
                            (0.025..0.975).contains(&position.y),
                            "offscreen HUD: {}",
                            text.text
                        );
                    }
                }
            }
            let overlay = crate::render::raster_text_overlay(&frames, viewport, lab.frame_layout());
            assert!(
                overlay
                    .iter()
                    .any(|p| p.text.starts_with(tool.name()) && p.text.contains(view.name()))
            );
            let image = raster.image_from_frames_with_layout(
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
                    .filter(|p| p.r > 200 && p.g > 90 && p.b < 100)
                    .count()
                    > 10
            );
        }
    }
    lab.zoom_player_out(0);
    assert_eq!(lab.state.view, TerrainView::Mining);
    lab.zoom_player_in(0);
    assert_eq!(lab.state.view, TerrainView::Detail);
    lab.zoom_player_out(1);
    assert_eq!(lab.state.view, TerrainView::Detail);
}
