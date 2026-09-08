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
