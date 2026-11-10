//! A bounded, slow-motion application of native material loading, unloading and
//! paired collisions and support collapse.
use bincode::Options;
use engine_common::{Action, Observation, RenderFrame, Scenario, StepResult, TickModel};
use engine_material::{ARMOR, Material, WOOD};
use engine_rapier::world::{
    PhysicsWorld,
    loading::response::compression::CompressionRejection,
    loading::response::dynamic_compression::{
        CompressionFailure, DynamicCompressionSummary, MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES,
    },
    loading::{EpisodeTransition, RegionToken},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
mod pair;
mod render;
mod scene;
pub use pair::{Pair, PairBody};
pub use render::controls as render_controls;
#[cfg(test)]
mod tests;

/// One displayed update advances this much physical time: about 68x slow motion.
pub const PHYSICS_DT: f32 = 1.0 / 4096.0;
/// The guided load/release case pauses here after its initial yielding phase.
pub const YIELD_CHECKPOINT_STEP: u32 = 128;
pub const MAX_SAVE_BYTES: u64 = 1_048_576;
const SAVE_MAGIC: &[u8; 8] = b"MATLAB01";
const LOAD_SAVE_MAGIC: &[u8; 8] = b"MATLAB02";
const PAIR_SAVE_MAGIC: &[u8; 8] = b"MATLAB03";
const FAILURE_SAVE_MAGIC: &[u8; 8] = b"MATLAB04";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Case {
    Elastic,
    #[default]
    PlasticReturn,
    Support,
    Overload,
    LoadRelease,
    // Appended: saved case tags of the platen cases stay fixed.
    WoodWood,
    WoodArmor,
    SupportFailure,
}
/// Initial condition of one free body along the +x contact normal.
#[derive(Clone, Copy)]
pub(crate) struct BodySetup {
    pub material: Material,
    pub mass: f32,
    pub position: f32,
    pub velocity: f32,
}
impl Case {
    pub const ALL: [Self; 8] = [
        Self::Elastic,
        Self::PlasticReturn,
        Self::Support,
        Self::Overload,
        Self::LoadRelease,
        Self::WoodWood,
        Self::WoodArmor,
        Self::SupportFailure,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Elastic => "Elastic bounce",
            Self::PlasticReturn => "Yield, rebound & return",
            Self::Support => "Yield & continuing support",
            Self::Overload => "Overloaded support",
            Self::LoadRelease => "Load, release & return",
            Self::WoodWood => "Wood strikes wood",
            Self::WoodArmor => "Wood meets armor",
            Self::SupportFailure => "Support gives way",
        }
    }
    /// The two free bodies of a paired collision, incoming body first. Both
    /// carry material, start 1/16 apart and feel no external force.
    pub(crate) fn free_pair(self) -> Option<[BodySetup; 2]> {
        let body = |material, mass, position, velocity| BodySetup {
            material,
            mass,
            position,
            velocity,
        };
        match self {
            // A light body strikes a heavier one at rest and rebounds from it.
            Self::WoodWood => Some([
                body(WOOD, 20.0, -0.5 - 1.0 / 16.0, 6.0),
                body(WOOD, 40.0, 0.5, 0.0),
            ]),
            // Equal masses meet head-on. Armor forces small solver steps; mass
            // 80, the solver's largest, needs the fewest and so stays inside the
            // solver's fixed error allowances.
            Self::WoodArmor => Some([
                body(WOOD, 80.0, -0.5 - 1.0 / 32.0, 2.0),
                body(ARMOR, 80.0, 0.5 + 1.0 / 32.0, -2.0),
            ]),
            _ => None,
        }
    }
    fn parameters(self) -> (f32, f32, u32) {
        match self {
            Self::Elastic => (1000.0, 1.0 / 64.0, 512),
            Self::PlasticReturn => (1000.0, 1.0 / 16.0, 716),
            Self::Support => (4000.0, 1.0 / 64.0, 512),
            Self::Overload => (8000.0, 1.0 / 64.0, 272),
            Self::LoadRelease => (8000.0, 1.0 / 64.0, 768),
            Self::WoodWood => (0.0, 1.0 / 16.0, 352),
            Self::WoodArmor => (0.0, 1.0 / 16.0, 512),
            Self::SupportFailure => (8000.0, 1.0 / 64.0, 144),
        }
    }
    pub fn next(self) -> Self {
        Self::ALL[(Self::ALL.iter().position(|c| *c == self).unwrap() + 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Load {
    Released,
    Light,
    Heavy,
}
impl Load {
    pub fn weight(self) -> f32 {
        match self {
            Self::Released => 0.0,
            Self::Light => 1000.0,
            Self::Heavy => 8000.0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LoadChange {
    step: u32,
    load: Load,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Phase {
    Falling,
    Impact,
    Support,
    Rebound,
    Separated,
    /// A free pair closing before its first contact.
    Approach,
    /// A free pair in contact and still closing.
    Compression,
    Collapsed,
}
impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Falling => "Falling",
            Self::Impact => "Impact",
            Self::Support => "Support",
            Self::Rebound => "Rebound",
            Self::Separated => "Separated",
            Self::Approach => "Approach",
            Self::Compression => "Compression",
            Self::Collapsed => "Collapsed",
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
    /// In body order; the ideal rigid platen of a fixed-support case has none.
    materials: [Option<RegionToken>; 2],
    pub case: Case,
    pub paused: bool,
    pub steps: u32,
    pub impacts: u32,
    pub separations: u32,
    pub events: Vec<Event>,
    pub stopped: Option<String>,
    load_changes: Vec<LoadChange>,
}
impl MaterialLabState {
    pub fn new(case: Case) -> Self {
        let (world, materials) = scene::build(case);
        Self {
            world,
            materials,
            case,
            paused: true,
            steps: 0,
            impacts: 0,
            separations: 0,
            events: Vec::new(),
            stopped: None,
            load_changes: Vec::new(),
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
    /// Remaining capacity of the support's material.
    pub fn remaining(&self) -> f64 {
        self.world
            .loading_material_remaining(self.materials[1].unwrap())
            .unwrap()
    }
    pub fn capacity(&self) -> f64 {
        let region = self.case.free_pair().map_or_else(
            || scene::support_region(self.case),
            |pair| scene::region(pair[1].material),
        );
        f64::from(region.capacity())
    }
    pub fn failure(&self) -> Option<CompressionFailure> {
        self.world.material_compression_failure()
    }
    pub fn weight(&self) -> f32 {
        -self.world.gravity().y * 20.0
    }
    pub fn gravity_impulse(&self) -> f64 {
        self.world
            .material_dynamic_compression_gravity_impulse()
            .unwrap_or(0.0)
    }
    /// A load command owns no tick and never resets an existing physical history.
    pub fn set_load(&mut self, load: Load) -> Result<(), String> {
        if self.case == Case::SupportFailure {
            return Err("This collapse case uses a fixed load. Choose another platen case.".into());
        }
        if self.case.free_pair().is_some() {
            return Err("A free pair takes no external load. Choose a platen case.".into());
        }
        if self.stopped.is_some() {
            return Err("Run ended. Restart the case to change its load.".into());
        }
        if self.steps == 0 {
            return Err("Start the run or single step before changing the load.".into());
        }
        let changed = self.weight() != load.weight();
        self.world
            .set_material_dynamic_compression_acceleration(load.weight() / 20.0)
            .map_err(|reason| {
                if reason == CompressionRejection::Limit {
                    "Load-change limit reached. Restart the case for a new run.".into()
                } else {
                    "This run cannot accept that load. Last valid state retained.".to_string()
                }
            })?;
        if changed {
            self.load_changes.push(LoadChange {
                step: self.steps,
                load,
            });
        }
        Ok(())
    }
    pub fn guidance(&self) -> Option<&'static str> {
        if self.case == Case::SupportFailure && self.stopped.is_none() {
            return Some(if self.failure().is_some() {
                "Support has collapsed. Run to watch the platen fall."
            } else {
                "Run until the support fails; the lab will pause."
            });
        }
        if self.case != Case::LoadRelease || self.stopped.is_some() {
            return None;
        }
        Some(
            if self.steps < YIELD_CHECKPOINT_STEP && self.weight() == Load::Heavy.weight() {
                "Run until the wood yields; the lab will pause."
            } else if self.weight() == Load::Heavy.weight() {
                "Release load, then Run to watch elastic recovery."
            } else if self.weight() == 0.0 && self.separations == 0 {
                "Run to separation; the lab will pause."
            } else if self.weight() == 0.0 {
                "Apply light load, then Run to return to the dent."
            } else {
                "Run to watch the return; permanent damage stays spent."
            },
        )
    }
    pub fn phase(&self) -> Phase {
        if self.failure().is_some() {
            return Phase::Collapsed;
        }
        let s = self.summary();
        let pair = self.case.free_pair().is_some();
        if s.indentation < s.plastic_indentation {
            if self.impacts > 0 {
                Phase::Separated
            } else if pair {
                Phase::Approach
            } else {
                Phase::Falling
            }
        } else if self.events.last().is_some_and(|e| e.label == "Impact") {
            Phase::Impact
        } else if s.velocity < 0.0 {
            Phase::Rebound
        } else if pair {
            Phase::Compression
        } else {
            Phase::Support
        }
    }
    pub fn advance(&mut self) {
        if self.stopped.is_some() {
            return;
        }
        let at = self.summary().elapsed;
        let result = if self.case == Case::SupportFailure {
            self.world.step_material_collapsing_support(PHYSICS_DT)
        } else {
            self.world.step_material_dynamic_compression(PHYSICS_DT)
        };
        match result {
            Ok(result) => {
                self.steps += 1;
                let mut separated = false;
                let collapsed_episode = result.failures.first().map(|f| f.episode);
                if collapsed_episode.is_some() {
                    self.paused = true;
                }
                for e in result.events {
                    let label = match e.event.transition {
                        EpisodeTransition::Impact => {
                            self.impacts += 1;
                            "Impact"
                        }
                        EpisodeTransition::Support => "Support",
                        EpisodeTransition::End if Some(e.event.id) == collapsed_episode => {
                            "Contact ended"
                        }
                        EpisodeTransition::End => {
                            self.separations += 1;
                            separated = true;
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
                for failure in result.failures {
                    if self.events.len() == 8 {
                        self.events.remove(0);
                    }
                    self.events.push(Event {
                        time: failure.elapsed,
                        label: "Collapse".into(),
                        episode: self.impacts,
                    });
                }
                if self.case == Case::LoadRelease
                    && (self.steps == YIELD_CHECKPOINT_STEP || (separated && self.weight() == 0.0))
                {
                    self.paused = true;
                }
                if self.steps == self.case.parameters().2
                    || (self.case == Case::LoadRelease && self.impacts >= 2)
                {
                    self.stopped = Some("Run complete — restart or choose another case".into());
                    self.paused = true;
                }
            }
            Err(reason) => {
                let reason = if (self.case.free_pair().is_some()
                    || self.case == Case::SupportFailure)
                    && reason == CompressionRejection::Numerical
                {
                    "Numerical accuracy limit reached".to_string()
                } else {
                    format!("{reason:?}")
                };
                self.stopped = Some(format!("Run stopped: {reason}. Last valid state retained."));
                self.paused = true;
            }
        }
    }
    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({ "case": self.case.name(), "paused": self.paused, "steps": self.steps,
            "impacts": self.impacts, "separations": self.separations, "phase": self.phase(),
            "summary": self.summary(), "remaining": self.remaining(), "events": self.events,
            "stopped": self.stopped, "weight": self.weight(),
            "gravity_impulse": self.gravity_impulse(), "load_changes": self.load_changes,
            "guidance": self.guidance(), "pair": self.pair(),
            "capacity": self.capacity(), "failure": self.failure() })
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
        // Fixed-load cases have no command list in their save envelope.
        let plain = self.load_changes.is_empty() && self.case != Case::LoadRelease;
        let mut bytes = if self.case == Case::SupportFailure {
            FAILURE_SAVE_MAGIC
        } else if self.case.free_pair().is_some() {
            PAIR_SAVE_MAGIC
        } else if plain {
            SAVE_MAGIC
        } else {
            LOAD_SAVE_MAGIC
        }
        .to_vec();
        bytes.extend(
            if plain {
                codec().serialize(&save)
            } else {
                codec().serialize(&(save, &self.load_changes))
            }
            .map_err(|e| e.to_string())?,
        );
        Ok(bytes)
    }
    /// Restore paused. The native snapshot owns physical history and numerical allowances.
    /// The bounded wrapper additionally verifies all saved display metadata by replay.
    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_SAVE_BYTES {
            return Err("Not a supported Material Lab save".into());
        }
        let (save, changes): (Save, Vec<LoadChange>) = if bytes.starts_with(SAVE_MAGIC)
            || bytes.starts_with(PAIR_SAVE_MAGIC)
            || bytes.starts_with(FAILURE_SAVE_MAGIC)
        {
            let save: Save = codec()
                .deserialize(&bytes[8..])
                .map_err(|e| e.to_string())?;
            if save.case == Case::LoadRelease {
                return Err("Load/release case requires a newer lab save".into());
            }
            (save, Vec::new())
        } else if bytes.starts_with(LOAD_SAVE_MAGIC) {
            codec()
                .deserialize(&bytes[8..])
                .map_err(|e| e.to_string())?
        } else {
            return Err("Not a supported Material Lab save".into());
        };
        if save.steps > save.case.parameters().2
            || save.events.len() > 8
            || changes.len() > MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES
            || changes.iter().any(|c| c.step == 0 || c.step > save.steps)
            || changes.windows(2).any(|c| c[0].step > c[1].step)
        {
            return Err("Invalid run history".into());
        }
        let world = PhysicsWorld::from_snapshot_bytes(&save.world).map_err(|e| e.to_string())?;
        let mut expected = Self::new(save.case);
        let mut commands = changes.iter().peekable();
        for step in 0..=save.steps {
            while commands.peek().is_some_and(|c| c.step == step) {
                expected
                    .set_load(commands.next().unwrap().load)
                    .map_err(|_| "Invalid run history".to_string())?;
            }
            if step < save.steps {
                expected.advance();
            }
        }
        if save.stopped.is_some() && expected.stopped.is_none() {
            // A saved numerical refusal retains the last successful tick. Verify
            // that the next tick actually refuses, without accepting a time reset.
            expected.advance();
        }
        if expected.snapshot_bytes()? != bytes {
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
