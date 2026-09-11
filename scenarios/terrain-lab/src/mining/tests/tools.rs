use super::*;
use crate::{MiningTool, MiningToolProfile, TerrainView, ToolControls};

fn tools(cycle_tool: bool, cycle_view: bool) -> Action {
    TerrainLabAction::Tools(ToolControls {
        cycle_tool,
        cycle_view,
    })
    .encode()
}

#[test]
fn each_tool_has_a_distinct_cut_and_precision_leaves_adjacent_ore_intact() {
    for (tool, expected_cells, pulses) in [
        (MiningTool::Precision, 1, 9),
        (MiningTool::Drill, 5, 9),
        (MiningTool::Excavator, 11, 6),
    ] {
        let mut state = fixture(ORE);
        state.select_tool(tool);
        let initial = state.terrain.clone();
        let profile = state.tool_profile();
        let snapshot = state.mining_snapshot();
        let preview = state
            .terrain
            .brush_cells(snapshot.brush.unwrap())
            .unwrap()
            .map(|(coord, _)| coord)
            .collect::<Vec<_>>();
        assert_eq!(preview.len(), expected_cells);
        step(&mut state, &[drill(true)]);
        for _ in 0..(pulses - 1) * profile.interval_ticks {
            step(&mut state, &[]);
        }
        assert_eq!(state.recovered.ore_cells, expected_cells as u64);
        assert_eq!(state.recovered.rock_cells, 0);
        for y in 0..25 {
            for x in 0..25 {
                let coord = CellCoord::new(x, y);
                if preview.contains(&coord) {
                    assert_eq!(
                        state.terrain.cell(coord).unwrap().material,
                        MaterialId::VOID
                    );
                } else {
                    assert_eq!(state.terrain.cell(coord), initial.cell(coord));
                }
            }
        }
    }
}

#[test]
fn preview_matches_the_next_pulse_on_rotated_translated_terrain_for_every_tool() {
    for tool in MiningTool::ALL {
        for angle in [0.0, 0.37, 0.79, 2.1] {
            let mut state = fixture(ROCK);
            state.select_tool(tool);
            let translation = Vec2::new(4.0, -3.0);
            state
                .physics
                .set_pose(state.terrain_assembly.body(), translation, angle, true);
            state.physics.set_pose(
                state.spaceling.body(),
                translation + Vec2::new(0.0, 2.0).rotate_radians(angle),
                angle,
                true,
            );
            state.physics.step(1.0 / FIXED_HZ as f32);
            let before = state.terrain.clone();
            let brush = state.mining_snapshot().brush.unwrap();
            let predicted = state
                .terrain
                .brush_cells(brush)
                .unwrap()
                .map(|(p, _)| p)
                .collect::<Vec<_>>();
            step(&mut state, &[drill(true)]);
            let actual = before
                .cells()
                .iter()
                .zip(state.terrain.cells())
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, _)| CellCoord::new(i as i32 % 25, i as i32 / 25))
                .collect::<Vec<_>>();
            assert_eq!(actual, predicted, "{tool:?}, angle {angle}");
        }
    }
}

#[test]
fn switching_tools_retains_the_last_pulses_cooldown() {
    let mut state = fixture(ROCK);
    state.select_tool(MiningTool::Excavator);
    step(&mut state, &[drill(true)]);
    let target = CellCoord::new(12, 12);
    assert_eq!(state.terrain.cell(target).unwrap().durability, 70);
    for tick in 1..12 {
        state.select_tool(if tick % 2 == 0 {
            MiningTool::Precision
        } else {
            MiningTool::Drill
        });
        step(&mut state, &[]);
        assert_eq!(state.terrain.cell(target).unwrap().durability, 70);
    }
    state.select_tool(MiningTool::Precision);
    step(&mut state, &[]);
    assert_eq!(state.terrain.cell(target).unwrap().durability, 50);
}

#[test]
fn tool_and_view_buttons_need_release_and_resume_from_a_checkpoint() {
    let mut first = fixture(ROCK);
    step(&mut first, &[tools(true, true)]);
    assert_eq!(first.selected_tool(), MiningTool::Excavator);
    assert_eq!(first.view, TerrainView::Mining);
    let mut restored = first.clone();
    for _ in 0..10 {
        for state in [&mut first, &mut restored] {
            step(state, &[tools(true, true)]);
        }
    }
    assert_eq!(first.selected_tool(), MiningTool::Excavator);
    assert_eq!(first.view, TerrainView::Mining);
    for state in [&mut first, &mut restored] {
        step(state, &[tools(false, false)]);
        step(state, &[tools(true, true), drill(true)]);
    }
    assert_eq!(first.selected_tool(), MiningTool::Precision);
    assert_eq!(first.view, TerrainView::Detail);
    assert_eq!(first.terrain(), restored.terrain());
    assert_eq!(
        TerrainLabScenario::observe(&first).payload,
        TerrainLabScenario::observe(&restored).payload
    );
    assert_eq!(first.mining.cooldown, restored.mining.cooldown);
    for invalid in [
        vec![],
        vec![2, 0, 0],
        vec![1, 2, 0],
        vec![1, 0, 3],
        vec![1, 0, 0, 0],
    ] {
        assert!(TerrainLabAction::decode(&Action::scenario(4, invalid)).is_none());
    }
}

#[test]
fn debug_modifier_is_required_and_does_not_activate_an_already_held_cut() {
    let mut state = fixture(ROCK);
    let hash = state.terrain_hash();
    step(
        &mut state,
        &[TerrainLabAction::controls(0.0, false, true, true, false)],
    );
    assert_eq!(state.terrain_hash(), hash);
    step(
        &mut state,
        &[TerrainLabAction::controls(0.0, false, true, true, true)],
    );
    assert_eq!(state.terrain_hash(), hash);
    step(
        &mut state,
        &[TerrainLabAction::controls(0.0, false, false, false, true)],
    );
    step(
        &mut state,
        &[TerrainLabAction::controls(0.0, false, false, true, true)],
    );
    assert_ne!(state.terrain_hash(), hash);
    assert_eq!(state.recovered, MiningInventory::default());
    step(
        &mut state,
        &[TerrainLabAction::controls(0.0, false, false, false, false)],
    );
    assert!(!state.overlay);
}

#[test]
fn zoom_follows_the_character_and_strike_face_and_cancels_old_pointer_aim() {
    let mut state = fixture(ORE);
    step(
        &mut state,
        &[pointer(Vec2::new(0.0, 0.0), PointerPhase::Press)],
    );
    let hash = state.terrain_hash();
    for view in [TerrainView::Mining, TerrainView::Detail] {
        state.set_view(view);
        assert!(!state.mining_snapshot().active);
        let camera = state.camera();
        let snapshot = state.mining_snapshot();
        for point in [snapshot.origin, snapshot.end] {
            let position =
                camera.world_to_viewport(RenderPoint::new(point.x, point.y), 800.0 / 480.0);
            assert!((0.15..0.85).contains(&position.x));
            assert!((0.2..0.8).contains(&position.y));
        }
        for _ in 0..20 {
            step(&mut state, &[pointer(Vec2::ZERO, PointerPhase::Drag)]);
        }
        assert_eq!(state.terrain_hash(), hash);
    }
    state.zoom_in();
    assert_eq!(state.view, TerrainView::Detail);
    state.zoom_out();
    assert_eq!(state.view, TerrainView::Mining);
    state.zoom_out();
    state.zoom_out();
    assert_eq!(state.view, TerrainView::Overview);
}

#[test]
fn tool_profiles_are_configurable_and_bounded_without_changing_cell_size() {
    let mut config = TerrainLabConfig::default();
    config.mining_tools[0] = MiningToolProfile {
        range: f32::NAN,
        cut_half_width: -1.0,
        cut_radius: 100.0,
        damage: 0,
        interval_ticks: 0,
    };
    let mut state = TerrainLabScenario::init(config, 42);
    state.select_tool(MiningTool::Precision);
    let profile = state.tool_profile();
    assert_eq!(profile.range, 5.0);
    assert_eq!(profile.cut_half_width, 0.0);
    assert_eq!(profile.cut_radius, 0.0);
    assert_eq!(profile.damage, 1);
    assert_eq!(profile.interval_ticks, 1);
    assert_eq!(state.terrain.cell_size(), 0.5);
    let mut state = fixture(ROCK);
    state.config.mining_tools[MiningTool::Precision as usize] = MiningToolProfile {
        range: 1.0,
        ..MiningToolProfile::DEFAULTS[0]
    };
    state.select_tool(MiningTool::Precision);
    step(&mut state, &[drill(true)]);
    assert!(state.mining_snapshot().target.is_none());
    assert_eq!(state.removed_cells, 0);
    state.config.mining_tools[0].range = 3.0;
    state.config.mining_tools[0].damage = 100;
    step(&mut state, &[]);
    assert_eq!(state.recovered.rock_cells, 1);
}
