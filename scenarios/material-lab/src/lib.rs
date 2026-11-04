//! A bounded, slow-motion application of the native falling-platen material path.
use bincode::Options;
use engine_common::{Action, Observation, RenderFrame, Scenario, StepResult, TickModel};
use engine_rapier::world::{
    PhysicsWorld,
    loading::response::dynamic_compression::DynamicCompressionSummary,
    loading::{EpisodeTransition, RegionToken},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
mod render;
mod scene;
pub use render::controls as render_controls;
#[cfg(test)]
mod tests;

/// One displayed update advances this much physical time: about 68x slow motion.
pub const PHYSICS_DT: f32 = 1.0 / 4096.0;
pub const MAX_SAVE_BYTES: u64 = 1_048_576;
const SAVE_MAGIC: &[u8; 8] = b"MATLAB01";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Case {
    Elastic,
    #[default]
    PlasticReturn,
    Support,
    Overload,
}
impl Case {
    pub const ALL: [Self; 4] = [
        Self::Elastic,
        Self::PlasticReturn,
        Self::Support,
        Self::Overload,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Elastic => "Elastic bounce",
            Self::PlasticReturn => "Yield, rebound & return",
            Self::Support => "Yield & continuing support",
            Self::Overload => "Overloaded support",
        }
    }
    fn parameters(self) -> (f32, f32, u32) {
        match self {
            Self::Elastic => (1000.0, 1.0 / 64.0, 512),
            Self::PlasticReturn => (1000.0, 1.0 / 16.0, 716),
            Self::Support => (4000.0, 1.0 / 64.0, 512),
            Self::Overload => (8000.0, 1.0 / 64.0, 272),
        }
    }
    pub fn next(self) -> Self {
        Self::ALL[(Self::ALL.iter().position(|c| *c == self).unwrap() + 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Phase {
    Falling,
    Impact,
    Support,
    Rebound,
    Separated,
}
impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Falling => "Falling",
            Self::Impact => "Impact",
            Self::Support => "Support",
            Self::Rebound => "Rebound",
            Self::Separated => "Separated",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub time: f64,
    pub label: String,
    pub episode: u32,
}

pub struct MaterialLabState {
    world: PhysicsWorld,
    material: RegionToken,
    pub case: Case,
    pub paused: bool,
    pub steps: u32,
    pub impacts: u32,
    pub separations: u32,
    pub events: Vec<Event>,
    pub stopped: Option<String>,
}
impl MaterialLabState {
    pub fn new(case: Case) -> Self {
        let (world, material) = scene::build(case);
        Self {
            world,
            material,
            case,
            paused: true,
            steps: 0,
            impacts: 0,
            separations: 0,
            events: Vec::new(),
            stopped: None,
        }
    }
    pub fn summary(&self) -> DynamicCompressionSummary {
        self.world
            .material_dynamic_compression_summary()
            .unwrap_or_else(|| DynamicCompressionSummary {
                indentation: -f64::from(self.case.parameters().1),
                ..Default::default()
            })
    }
    pub fn remaining(&self) -> f64 {
        self.world
            .loading_material_remaining(self.material)
            .unwrap()
    }
    pub fn phase(&self) -> Phase {
        let s = self.summary();
        if s.indentation < s.plastic_indentation {
            if self.impacts == 0 {
                Phase::Falling
            } else {
                Phase::Separated
            }
        } else if self.events.last().is_some_and(|e| e.label == "Impact") {
            Phase::Impact
        } else if s.velocity < 0.0 {
            Phase::Rebound
        } else {
            Phase::Support
        }
    }
    pub fn advance(&mut self) {
        if self.stopped.is_some() {
            return;
        }
        let at = self.summary().elapsed;
        match self.world.step_material_dynamic_compression(PHYSICS_DT) {
            Ok(result) => {
                self.steps += 1;
                for e in result.events {
                    let label = match e.event.transition {
                        EpisodeTransition::Impact => {
                            self.impacts += 1;
                            "Impact"
                        }
                        EpisodeTransition::Support => "Support",
                        EpisodeTransition::End => {
                            self.separations += 1;
                            "Separation"
                        }
                    };
                    if self.events.len() == 8 {
                        self.events.remove(0);
                    }
                    self.events.push(Event {
                        time: at + e.at,
                        label: label.into(),
                        episode: self.impacts,
                    });
                }
                if self.steps == self.case.parameters().2 {
                    self.stopped = Some("Run complete — restart or choose another case".into());
                    self.paused = true;
                }
            }
            Err(reason) => {
                self.stopped = Some(format!(
                    "Run stopped: {reason:?}. Last valid state retained."
                ));
                self.paused = true;
            }
        }
    }
    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({ "case": self.case.name(), "paused": self.paused, "steps": self.steps,
            "impacts": self.impacts, "separations": self.separations, "phase": self.phase(),
            "summary": self.summary(), "remaining": self.remaining(), "events": self.events,
            "stopped": self.stopped })
    }
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, String> {
        let save = Save {
            case: self.case,
            steps: self.steps,
            impacts: self.impacts,
            separations: self.separations,
            events: self.events.clone(),
            stopped: self.stopped.clone(),
            world: self.world.snapshot_bytes().map_err(|e| e.to_string())?,
        };
        let mut bytes = SAVE_MAGIC.to_vec();
        bytes.extend(codec().serialize(&save).map_err(|e| e.to_string())?);
        Ok(bytes)
    }
    /// Restore paused. The native snapshot owns physical history and numerical allowances.
    /// The bounded wrapper additionally verifies all saved display metadata by replay.
    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_SAVE_BYTES || !bytes.starts_with(SAVE_MAGIC) {
            return Err("Not a supported Material Lab save".into());
        }
        let save: Save = codec()
            .deserialize(&bytes[8..])
            .map_err(|e| e.to_string())?;
        if save.steps > save.case.parameters().2 || save.events.len() > 8 {
            return Err("Invalid run history".into());
        }
        let world = PhysicsWorld::from_snapshot_bytes(&save.world).map_err(|e| e.to_string())?;
        let mut expected = Self::new(save.case);
        for _ in 0..save.steps {
            expected.advance();
        }
        if save.stopped.is_some() && expected.stopped.is_none() {
            // A saved numerical refusal retains the last successful tick. Verify
            // that the next tick actually refuses, without accepting a time reset.
            expected.advance();
        }
        if expected.steps != save.steps
            || expected.impacts != save.impacts
            || expected.separations != save.separations
            || expected.events != save.events
            || expected.stopped != save.stopped
            || expected.world.snapshot_bytes().map_err(|e| e.to_string())? != save.world
        {
            return Err("Save history does not match this supported run".into());
        }
        expected.world = world;
        expected.paused = true;
        Ok(expected)
    }
}
fn codec() -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(MAX_SAVE_BYTES)
        .reject_trailing_bytes()
}
#[derive(Serialize, Deserialize)]
struct Save {
    case: Case,
    steps: u32,
    impacts: u32,
    separations: u32,
    events: Vec<Event>,
    stopped: Option<String>,
    world: Vec<u8>,
}

pub struct MaterialLabScenario;
impl Scenario for MaterialLabScenario {
    type State = MaterialLabState;
    type Config = Case;
    fn init(config: Case, _seed: u64) -> Self::State {
        MaterialLabState::new(config)
    }
    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: 60 }
    }
    fn step(s: &mut Self::State, _actions: &[Action], dt: Duration) -> StepResult {
        if !dt.is_zero() && !s.paused {
            s.advance();
        }
        StepResult::default()
    }
    fn observe(s: &Self::State) -> Observation {
        Observation {
            payload: serde_json::to_vec(&s.diagnostics()).unwrap(),
        }
    }
    fn render_frame(s: &Self::State) -> RenderFrame {
        render::frame(s)
    }
}
