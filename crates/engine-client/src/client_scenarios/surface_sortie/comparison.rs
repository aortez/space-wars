use super::*;
use scenario_spacewars::surface_sortie::comparison::TerrainSurface;

pub(crate) const SURFACE_BLOCKS_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-surface-blocks",
    controls_help: "Surface comparison: stepped ground. Compare with spacewars-surface-contour using the same seed. One stationary planet, diagonal landing, a crater, a tunnel and a cap attached by one cell. A/Space thrusts, jumps or gets up; hold in air for jetpack. Left/right turns or walks. B/X exits or boards. Right stick/arrows aim; RT/LB/E mines; Y/T changes size. Dark lines show collision shapes. Land, exit, claim, mine, recover and rebuild with ordinary Expedition rules. Start/Esc pauses; restart repeats the scene. Choose 1 or 2 human players in Settings.",
    create: create_blocks,
    ..TERRAIN_REGISTRATION
};

pub(crate) const SURFACE_CONTOUR_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-surface-contour",
    controls_help: "Surface comparison: sloped ground. Compare with spacewars-surface-blocks using the same seed. Material and mass are unchanged; exposed cell corners form slopes shared by drawing, collision and mining. One stationary planet, diagonal landing, a crater, a tunnel and a cap attached by one cell. A/Space thrusts, jumps or gets up; hold in air for jetpack. Left/right turns or walks. B/X exits or boards. Right stick/arrows aim; RT/LB/E mines; Y/T changes size. Dark lines show collision shapes. Start/Esc pauses; restart repeats the scene. Choose 1 or 2 human players in Settings.",
    create: create_contour,
    ..TERRAIN_REGISTRATION
};

fn create_blocks(
    seed: u64,
    settings: &Settings,
    _: Viewport,
    _: ScenarioStartMode,
    _: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(create(seed, settings, TerrainSurface::Blocks))
}
fn create_contour(
    seed: u64,
    settings: &Settings,
    _: Viewport,
    _: ScenarioStartMode,
    _: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(create(seed, settings, TerrainSurface::Contour))
}
fn create(seed: u64, settings: &Settings, surface: TerrainSurface) -> Box<dyn ClientScenario> {
    Box::new(SurfaceSortieClientScenario {
        state: SurfaceSortieScenario::init_surface_comparison(
            seed,
            settings.surface_expedition.players.count(),
            surface,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::RenderPrimitive;

    #[test]
    fn comparison_presets_label_the_surface_and_show_collision_shapes() {
        for (registration, label) in [
            (&SURFACE_BLOCKS_REGISTRATION, "STEPS"),
            (&SURFACE_CONTOUR_REGISTRATION, "SLOPES"),
        ] {
            let viewport = Viewport::new(800.0, 480.0);
            let mut scenario = registration
                .create(
                    42,
                    &Settings::default(),
                    viewport,
                    ScenarioStartMode::Normal,
                )
                .unwrap();
            assert_eq!(scenario.registration().id, registration.id);
            scenario.step(&[], Duration::from_nanos(16_666_667));
            for renderer in [RenderBackend::Vector, RenderBackend::Raster] {
                let frames = scenario.render_frames(renderer, viewport);
                let primitives: Vec<_> = frames[0]
                    .layers
                    .iter()
                    .flat_map(|l| &l.primitives)
                    .collect();
                assert!(
                    primitives
                        .iter()
                        .any(|p| matches!(p,RenderPrimitive::Text(t) if t.text.contains(label)))
                );
                assert!(primitives.iter().any(|p| matches!(p,RenderPrimitive::Polygon(p) if p.fill.is_none() && p.stroke.is_some())));
            }
        }
    }
}
