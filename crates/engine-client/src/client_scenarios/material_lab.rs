use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode,
};
use crate::{
    input::{ClientInput, GameKey},
    render::{FrameLayout, Viewport},
};
use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_material_lab::{Case, Load, MaterialLabScenario, MaterialLabState};
use std::{cell::Cell, path::PathBuf, sync::mpsc, time::Duration};
mod storage;
use storage::{Operation, Outcome};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "material-lab",
    launcher_category: super::LauncherCategory::Extra,
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
    controls_help: "Material Lab: impact, compression and rebound in slow motion. Starts paused. Up/down chooses a control; Space or gamepad bottom face activates. Run/Pause, Single step, Restart case and Next case control playback. Release load, Apply light load and Apply heavy load change the force without advancing time; then Run or Single step to observe the response. The Load, release & return case pauses after yielding and separation to guide these changes. Save pauses and replaces one persistent slot; Load restores paused, including load history. Saves survive restarting the application. Each run has a bounded observation window. Esc/Start opens host pause; R restarts the lab. Vertical displacement is magnified 20x; displayed time and work are physical values.",
    create,
};
const COMMAND_KIND: u32 = 1;
const CONTROLS: [&str; 9] = [
    "Run / Pause",
    "Single step",
    "Restart case",
    "Next case",
    "Save",
    "Load",
    "Release load (0)",
    "Apply light load (1000)",
    "Apply heavy load (8000)",
];
pub(crate) struct MaterialLabClientScenario {
    state: MaterialLabState,
    selected: usize,
    held: Cell<[bool; 3]>,
    path: Option<PathBuf>,
    pending: Option<mpsc::Receiver<Result<Outcome, String>>>,
    message: String,
}
fn create(
    _seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    let path = if let ScenarioAsset::MaterialLab(path) = asset {
        Some(path.clone())
    } else {
        None
    };
    Ok(Box::new(MaterialLabClientScenario {
        state: MaterialLabState::new(Case::default()),
        selected: 0,
        held: Cell::new([false; 3]),
        path,
        pending: None,
        message: "Choose Run to begin. Save and Load use one persistent slot.".into(),
    }))
}
impl MaterialLabClientScenario {
    fn poll(&mut self) {
        let Some(rx) = &self.pending else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("Storage worker disconnected".into()),
        };
        self.pending = None;
        match result {
            Ok(Outcome::Saved) => {
                self.message = format!(
                    "Saved at {:.5} s. Run resumes; Load returns here.",
                    self.state.summary().elapsed
                )
            }
            Ok(Outcome::Loaded(state)) => {
                self.state = *state;
                self.message = "Loaded. Paused at the saved physical state.".into();
            }
            Err(error) => self.message = error,
        }
    }
    fn activate(&mut self) {
        if self.pending.is_some() {
            return;
        }
        match self.selected {
            0 => {
                if self.state.stopped.is_none() {
                    self.state.paused = !self.state.paused;
                }
            }
            1 => {
                self.state.paused = true;
                self.state.advance();
            }
            2 => {
                self.state = MaterialLabState::new(self.state.case);
                self.message = "Restarted this case. Paused.".into();
            }
            3 => {
                self.state = MaterialLabState::new(self.state.case.next());
                self.message = "New case. Paused.".into();
            }
            4 | 5 => {
                self.state.paused = true;
                let Some(path) = self.path.clone() else {
                    self.message = "No save directory is available.".into();
                    return;
                };
                let operation = if self.selected == 4 {
                    match self.state.snapshot_bytes() {
                        Ok(bytes) => Operation::Save(bytes),
                        Err(e) => {
                            self.message = e;
                            return;
                        }
                    }
                } else {
                    Operation::Load
                };
                match storage::request(path, operation) {
                    Ok(rx) => {
                        self.pending = Some(rx);
                        self.message = "Working on the saved slot…".into();
                    }
                    Err(e) => self.message = e,
                }
            }
            6..=8 => {
                self.state.paused = true;
                let load = [Load::Released, Load::Light, Load::Heavy][self.selected - 6];
                self.message = match self.state.set_load(load) {
                    Ok(()) => format!(
                        "Load set to {:.0}. Use Run or Single step to observe the response.",
                        load.weight()
                    ),
                    Err(reason) => reason,
                };
            }
            _ => unreachable!(),
        }
    }
}
impl ClientScenario for MaterialLabClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }
    fn tick_model(&self) -> TickModel {
        MaterialLabScenario::tick_model()
    }
    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        self.poll();
        let was_stopped = self.state.stopped.is_some();
        let mut acted = false;
        for action in actions {
            if let Action::Scenario {
                kind: COMMAND_KIND,
                payload,
            } = action
            {
                match payload.as_slice() {
                    [0] => self.selected = (self.selected + CONTROLS.len() - 1) % CONTROLS.len(),
                    [1] => self.selected = (self.selected + 1) % CONTROLS.len(),
                    [2] => {
                        self.activate();
                        acted = true;
                    }
                    _ => {}
                }
            }
        }
        // Explicit step/load/run commands own this update; do not silently add a tick.
        if !acted && self.pending.is_none() {
            MaterialLabScenario::step(&mut self.state, &[], dt);
        }
        if !was_stopped && let Some(reason) = &self.state.stopped {
            self.message = reason.clone();
        }
        StepResult::default()
    }
    fn map_input(&self, input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        use engine_nes::ControllerButtons as B;
        let b = input.nes_controller_buttons(0);
        let now = [
            input.is_pressed(GameKey::NesUp) || b.contains(B::UP),
            input.is_pressed(GameKey::NesDown) || b.contains(B::DOWN),
            input.is_pressed(GameKey::P1Laser) || b.contains(B::B),
        ];
        let old = self.held.replace(now);
        (0..3)
            .filter(|i| now[*i] && !old[*i])
            .map(|i| Action::scenario(COMMAND_KIND, vec![i as u8]))
            .collect()
    }
    fn render_frames(&self, _renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame> {
        let mut f = MaterialLabScenario::render_frame(&self.state);
        let message: String = self.message.chars().take(110).collect();
        scenario_material_lab::render_controls(&mut f, CONTROLS[self.selected], &message);
        // Fit the complete schematic and controls when the host is resized.
        let width = viewport.width.max(1.0);
        f.camera.height = f.camera.height.max(36.0 * viewport.height.max(1.0) / width);
        for layer in &mut f.layers {
            for primitive in &mut layer.primitives {
                if let engine_common::RenderPrimitive::Text(text) = primitive {
                    text.size *= (width / 800.0).min(1.0);
                }
            }
        }
        vec![f]
    }
    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
    }
    fn runtime_diagnostics(&self) -> String {
        let mut v = self.state.diagnostics();
        v["control"] = serde_json::json!(self.selected);
        v["control_count"] = serde_json::json!(CONTROLS.len());
        v["storage_busy"] = serde_json::json!(self.pending.is_some());
        v["storage_status"] = serde_json::json!(self.message);
        format!("material_lab={v}")
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
    fn tick(s: &mut dyn ClientScenario, activate: bool) {
        let a = if activate {
            vec![Action::scenario(COMMAND_KIND, vec![2])]
        } else {
            vec![]
        };
        s.step(&a, Duration::from_secs_f32(1.0 / 60.0));
    }
    fn settle(s: &mut MaterialLabClientScenario) {
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while s.pending.is_some() {
            assert!(std::time::Instant::now() < until);
            tick(s, false);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn material_lab_controls_pause_step_save_restore_and_failed_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("material-lab.bin");
        let mut adapter = create(
            0,
            &Settings::default(),
            Viewport::new(800.0, 480.0),
            ScenarioStartMode::Normal,
            &ScenarioAsset::MaterialLab(path.clone()),
        )
        .unwrap();
        let s = adapter
            .as_any_mut()
            .downcast_mut::<MaterialLabClientScenario>()
            .unwrap();
        tick(s, false);
        assert_eq!(s.state.steps, 0);
        s.selected = 1;
        tick(s, true);
        assert_eq!(s.state.steps, 1);
        s.selected = 0;
        tick(s, true);
        assert_eq!(s.state.steps, 1);
        for _ in 0..250 {
            tick(s, false);
        }
        let summary = s.state.summary();
        let steps = s.state.steps;
        for (control, weight) in [(6, 0.0), (7, 1000.0), (8, 8000.0), (6, 0.0)] {
            s.selected = control;
            tick(s, true);
            assert!(s.state.paused);
            assert_eq!(s.state.steps, steps);
            assert_eq!(s.state.summary(), summary);
            assert_eq!(s.state.weight(), weight);
        }
        s.selected = 4;
        tick(s, true);
        settle(s);
        let bytes = s.state.snapshot_bytes().unwrap();
        assert!(s.state.paused);
        assert!(s.message.starts_with("Saved"));
        s.selected = 1;
        tick(s, true);
        let next = s.state.snapshot_bytes().unwrap();
        assert_ne!(next, bytes);
        s.selected = 5;
        tick(s, true);
        settle(s);
        assert_eq!(s.state.snapshot_bytes().unwrap(), bytes);
        s.selected = 1;
        tick(s, true);
        assert_eq!(s.state.snapshot_bytes().unwrap(), next);
        std::fs::write(path, b"broken").unwrap();
        s.selected = 5;
        tick(s, true);
        settle(s);
        assert!(s.message.starts_with("Load failed"));
        assert_eq!(s.state.snapshot_bytes().unwrap(), next);
        for single_step in [false, true] {
            s.state = MaterialLabState::new(Case::Elastic);
            s.state.paused = single_step;
            s.selected = 1;
            s.message = "Use Run or Single step to observe the response.".into();
            for _ in 0..1000 {
                tick(s, single_step);
                if s.state.stopped.is_some() {
                    break;
                }
            }
            assert_eq!(Some(s.message.as_str()), s.state.stopped.as_deref());
            s.selected = 5;
            tick(s, true);
            settle(s);
            tick(s, false);
            assert!(s.message.starts_with("Load failed"));
        }
    }
}
