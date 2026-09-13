use super::*;

#[test]
fn camera_combat_frame_has_separate_enter_and_exit_distances() {
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    for player in 0..2 {
        state.pilots[player].landing.phase = LandingPhase::Flying;
    }
    let p1 = state.pilots[0].vehicle.0;
    let p2 = state.pilots[1].vehicle.0;
    state.world.ships[p1].position = Vec2::new(1000.0, 1000.0);
    let mut held = false;
    for (distance, expected) in [
        (310.0, false),
        (265.0, false),
        (259.0, true),
        (265.0, true),
        (259.0, true),
        (299.0, true),
        (301.0, false),
        (275.0, false),
        (250.0, true),
    ] {
        state.world.ships[p2].position = state.world.ships[p1].position + Vec2::X * distance;
        let intent = SurfaceSortieScenario::camera_target(&state, 0, held);
        assert_eq!(
            intent.framed_opponent, expected,
            "distance={distance}, held={held}"
        );
        held = intent.framed_opponent;
    }
    state.world.ships[p2].dead = true;
    assert!(!SurfaceSortieScenario::camera_target(&state, 0, true).framed_opponent);
}

#[test]
fn camera_landing_and_disembarking_intents_are_read_only() {
    let mut state = parked();
    let aboard = SurfaceSortieScenario::camera_target(&state, 0, false);
    assert_eq!(aboard.focus, camera::CameraFocus::Vehicle);
    assert_eq!(aboard.camera.height, 44.0);
    tick(
        &mut state,
        SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        },
    );
    assert_eq!(state.location(0), PilotLocation::OnFoot);
    let before = SurfaceSortieScenario::observe(&state).payload;
    let foot = SurfaceSortieScenario::camera_target(&state, 0, true);
    assert_eq!(foot.focus, camera::CameraFocus::Spaceling);
    assert!(!foot.framed_opponent);
    assert_eq!(
        foot.anchor,
        render_point(state.spaceling_snapshot(0).unwrap().motion.position)
    );
    assert_eq!(before, SurfaceSortieScenario::observe(&state).payload);
}
