#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn surface_comparison_launch_pause_restart_and_both_renderers() {
    for scenario in ["spacewars-surface-blocks", "spacewars-surface-contour"] {
        super::terrain_lab::run_terrain_lifecycle(scenario);
    }
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn spacewars_terrain_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_pilot_ai_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-ai");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_recovery_ai_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-recovery");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_combat_ai_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-combat");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_duel_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-duel");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_jetpack_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-jetpack");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_travel_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-travel");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_travel_duel_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-travel-duel");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_arena_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-arena");
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_arena_duel_launch_pause_restart_and_both_renderers() {
    super::terrain_lab::run_terrain_lifecycle("spacewars-terrain-arena-duel");
}
