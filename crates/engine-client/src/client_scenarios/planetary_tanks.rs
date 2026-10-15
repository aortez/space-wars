//! Scripted acceptance fixture; commands come from the shared scenario fixture.
use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode,
};
use crate::{
    input::ClientInput,
    render::{FrameLayout, Viewport},
};
use engine_common::{Action, DirtMode, RenderFrame, Settings, StepResult, TickModel};
use scenario_spacewars::tank_fixture::{FixtureCase, FixtureSample, MEASURED_STEPS, TankFixture};
use std::time::Duration;

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "planetary-tank-fixture",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: false,
        headless_benchmark: false,
        pointer_input: false,
        player_zoom: false,
        game_over: false,
        native_video: false,
        captures_gamepad_start: false,
        captures_gamepad_select: false,
    },
    controls_help: "Planetary tank fixture: two independently scripted tanks drive, brake, approach a material gap and lose support after a cut. Start/Esc pauses; Restart repeats the seed. Commands come from the shared acceptance script.",
    create,
};
pub(super) const ANGULAR_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "planetary-tank-fixture-angular",
    create: create_angular,
    ..REGISTRATION
};
pub(super) const COMPARISON_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "planetary-tank-comparison",
    controls_help: "Frozen normal-scale comparison. Left: ordinary pilot. Center: tank. Right: ship. All three world cameras use the same scale; no actor geometry is changed. Start/Esc pauses.",
    create: create_comparison,
    ..REGISTRATION
};
pub(super) const ANGULAR_COMPARISON_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "planetary-tank-comparison-angular",
    create: create_comparison_angular,
    ..COMPARISON_REGISTRATION
};
fn create_comparison(
    seed: u64,
    _: &Settings,
    _: Viewport,
    _: ScenarioStartMode,
    _: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(make_comparison(seed, false))
}
fn create_comparison_angular(
    seed: u64,
    _: &Settings,
    _: Viewport,
    _: ScenarioStartMode,
    _: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(make_comparison(seed, true))
}
fn make_comparison(seed: u64, angular: bool) -> Box<dyn ClientScenario> {
    let mut client = make(seed, angular);
    client.comparison = true;
    Box::new(client)
}
struct PlanetaryTanks {
    comparison: bool,
    fixture: TankFixture,
    trace: Vec<FixtureSample>,
    angular: bool,
    complete: bool,
    passed: bool,
}
fn create(
    seed: u64,
    _: &Settings,
    _: Viewport,
    _: ScenarioStartMode,
    _: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(make(seed, false)))
}
fn create_angular(
    seed: u64,
    _: &Settings,
    _: Viewport,
    _: ScenarioStartMode,
    _: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(make(seed, true)))
}
fn make(seed: u64, angular: bool) -> PlanetaryTanks {
    PlanetaryTanks {
        comparison: false,
        fixture: TankFixture::new(
            seed,
            if angular {
                DirtMode::Angular
            } else {
                DirtMode::Round
            },
            FixtureCase::Cut,
        ),
        trace: Vec::new(),
        angular,
        complete: false,
        passed: false,
    }
}
impl ClientScenario for PlanetaryTanks {
    fn registration(&self) -> &'static ScenarioRegistration {
        if self.comparison {
            if self.angular {
                &ANGULAR_COMPARISON_REGISTRATION
            } else {
                &COMPARISON_REGISTRATION
            }
        } else if self.angular {
            &ANGULAR_REGISTRATION
        } else {
            &REGISTRATION
        }
    }
    fn tick_model(&self) -> TickModel {
        TickModel::FixedTimestep { hz: 60 }
    }
    fn step(&mut self, _: &[Action], dt: Duration) -> StepResult {
        if !self.comparison && !self.complete && !dt.is_zero() {
            self.fixture.prepare_step();
            self.fixture.step();
            let sample = self.fixture.sample();
            eprintln!(
                "tank_fixture_sample={}",
                serde_json::to_string(&sample).unwrap()
            );
            self.trace.push(sample);
            if self.trace.len() == MEASURED_STEPS {
                let mut checks = self.fixture.assertions(&self.trace);
                checks.insert("bounded_lifecycle", self.fixture.cleanup());
                self.passed = checks.values().all(|&v| v);
                eprintln!(
                    "tank_fixture_assertions={}",
                    serde_json::to_string(&checks).unwrap()
                );
                self.complete = true;
            }
        }
        StepResult::default()
    }
    fn map_input(&self, _: &mut ClientInput, _: bool) -> Vec<Action> {
        Vec::new()
    }
    fn render_frames(&self, _: RenderBackend, _: Viewport) -> Vec<RenderFrame> {
        if self.comparison {
            // Three equally scaled cameras, left to right: pilot, tank, ship.
            // The frozen initial fixture retains real positions and geometry.
            let centers = [
                self.fixture.pilot_snapshot().unwrap().motion.position,
                self.fixture
                    .state
                    .tank_snapshot(self.fixture.state.tank_ids().next().unwrap())
                    .unwrap()
                    .assembly
                    .chassis
                    .position,
                self.fixture.state.ships[0].position,
            ];
            centers
                .into_iter()
                .map(|center| {
                    let mut frame = self.fixture.frame();
                    frame.camera = engine_common::Camera2::new(
                        engine_common::RenderPoint::new(center.x, center.y),
                        32.0,
                    );
                    frame
                })
                .collect()
        } else {
            vec![self.fixture.frame()]
        }
    }
    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
    }
    fn runtime_diagnostics(&self) -> String {
        let latest = self.trace.last();
        format!(
            "tank_fixture_pilots={}\ntank_fixture_tick={}\ntank_fixture_shape={}\ntank_fixture_complete={}\ntank_fixture_passed={}\ntank_fixture_actors={}\ntank_fixture_latest={}",
            usize::from(self.fixture.pilot_snapshot().is_some()),
            self.trace.len(),
            if self.angular { "angular" } else { "round" },
            self.complete,
            self.passed,
            self.fixture.state.tank_ids().count(),
            latest.map_or_else(|| "null".into(), |s| serde_json::to_string(s).unwrap())
        )
    }
    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_cameras_preserve_frozen_world_and_equal_scale() {
        for angular in [false, true] {
            let mut client = make_comparison(42, angular);
            let before = client.runtime_diagnostics();
            let frames = client.render_frames(RenderBackend::Vector, Viewport::new(1024.0, 768.0));
            assert_eq!(frames.len(), 3);
            assert!(
                frames
                    .iter()
                    .all(|f| f.camera == engine_common::Camera2::new(f.camera.center, 32.0))
            );
            let raster = client.render_frames(RenderBackend::Raster, Viewport::new(1024.0, 768.0));
            assert_eq!(frames, raster);
            assert_ne!(frames[0].camera.center, frames[1].camera.center);
            assert_ne!(frames[1].camera.center, frames[2].camera.center);
            // Only the cameras differ: all panels retain the normal world draw list.
            for frame in &frames[1..] {
                assert_eq!(frame.layers, frames[0].layers);
            }
            client.step(&[], Duration::from_secs_f32(1.0 / 60.0));
            assert_eq!(before, client.runtime_diagnostics());
        }
    }
    #[test]
    fn client_script_pause_and_cleanup() {
        for angular in [false, true] {
            let mut client = make(42, angular);
            let before = client.runtime_diagnostics();
            client.step(&[], Duration::ZERO);
            assert_eq!(before, client.runtime_diagnostics());
            for _ in 0..MEASURED_STEPS {
                client.step(&[], Duration::from_secs_f32(1.0 / 60.0));
            }
            let done = client.runtime_diagnostics();
            assert!(done.contains("tank_fixture_passed=true"), "{done}");
            assert!(done.contains("tank_fixture_actors=0"));
            assert!(done.contains("tank_fixture_pilots=1"));
            client.step(&[], Duration::from_secs_f32(1.0 / 60.0));
            assert_eq!(done, client.runtime_diagnostics());
        }
    }
}
