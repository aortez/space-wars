use super::*;

pub(crate) fn step(state: &mut TerrainLabState, actions: &[Action]) {
    TerrainLabScenario::step(
        state,
        actions,
        Duration::from_secs_f64(1.0 / f64::from(FIXED_HZ)),
    );
}

#[test]
fn crater_removes_support_and_character_falls_into_new_ground() {
    let mut state = TerrainLabScenario::init(
        TerrainLabConfig {
            angular_velocity: 0.0,
            orbit_radius: 0.0,
            ..TerrainLabConfig::default()
        },
        42,
    );
    for _ in 0..120 {
        step(&mut state, &[]);
    }
    let before = state.spaceling_snapshot();
    assert!(before.grounded());
    step(
        &mut state,
        &[TerrainLabAction::controls(0.0, false, true, false, true)],
    );
    assert!(!state.spaceling_snapshot().grounded());
    assert!(state.last_edit.removed_cells > 0);
    let removed = state.removed_cells;
    for _ in 0..180 {
        step(&mut state, &[]);
    }
    let after = state.spaceling_snapshot();
    assert!(after.motion.position.y < before.motion.position.y - 2.0);
    assert!(
        after.grounded(),
        "character should land on the crater floor: {after:?}"
    );
    assert_eq!(
        removed, state.removed_cells,
        "held crater button must not repeat"
    );
}

#[test]
fn tunnel_separates_the_planet_under_translation_rotation_and_checkpoint_resume() {
    let mut first = TerrainLabScenario::init(TerrainLabConfig::default(), 17);
    assert!(
        first.probe().2.is_some(),
        "initial frame must query the solid terrain"
    );
    for _ in 0..20 {
        step(&mut first, &[]);
    }
    assert!(first.probe().2.is_some());
    step(
        &mut first,
        &[TerrainLabAction::controls(0.0, false, false, true, true)],
    );
    assert!(first.probe().2.is_none());
    assert_eq!(first.fragments().len(), 1);
    let mut restored = first.clone();
    // A field serialization round trip also rebuilds presentation geometry.
    restored.terrain = bincode::deserialize(&bincode::serialize(first.terrain()).unwrap()).unwrap();
    restored.geometry = TerrainGeometry::new(restored.terrain());
    for tick in 0..360 {
        let actions = [TerrainLabAction::controls(
            if tick < 90 { 1.0 } else { 0.0 },
            tick == 50,
            tick == 120,
            false,
            true,
        )];
        step(&mut first, &actions);
        step(&mut restored, &actions);
        assert_eq!(
            TerrainLabScenario::observe(&first).payload,
            TerrainLabScenario::observe(&restored).payload
        );
        // The released half can now fall back across the original tunnel.
    }
    assert!(first.planet_motion().position.length() > 1.0);
    assert!(first.planet_motion().angle > 0.1);
}

#[test]
fn seeded_generation_and_ordered_brush_actions_replay() {
    let mut first = TerrainLabScenario::init(TerrainLabConfig::default(), 18);
    let mut second = TerrainLabScenario::init(TerrainLabConfig::default(), 18);
    let other = TerrainLabScenario::init(TerrainLabConfig::default(), 19);
    assert_ne!(first.terrain_hash(), other.terrain_hash());
    for tick in 0..200 {
        let actions = if tick % 25 == 0 {
            vec![
                TerrainLabAction::Edit(TerrainEdit {
                    brush: Brush::Circle {
                        center: CellCoord::new(24 + tick / 10, 65),
                        radius: 5,
                    },
                    mode: EditMode::Damage(60),
                })
                .encode(),
            ]
        } else {
            Vec::new()
        };
        step(&mut first, &actions);
        step(&mut second, &actions);
        assert_eq!(first.terrain_hash(), second.terrain_hash());
        assert_eq!(
            TerrainLabScenario::observe(&first).payload,
            TerrainLabScenario::observe(&second).payload
        );
    }
    // Timing diagnostics are intentionally excluded from deterministic observations.
    assert_eq!(first.terrain(), second.terrain());
}

#[test]
fn invalid_controls_are_ignored_and_zero_duration_does_not_step() {
    let mut state = TerrainLabScenario::init(TerrainLabConfig::default(), 1);
    for _ in 0..120 {
        step(&mut state, &[]);
    }
    for invalid in [
        Action::scenario(1, vec![]),
        TerrainLabAction::controls(f32::NAN, true, true, true, true),
        Action::scenario(2, vec![255; 23]),
    ] {
        assert!(TerrainLabAction::decode(&invalid).is_none());
    }
    let before = TerrainLabScenario::observe(&state).payload;
    TerrainLabScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(before, TerrainLabScenario::observe(&state).payload);
}
