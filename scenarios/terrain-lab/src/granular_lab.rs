//! Live terrain-to-grain sandbox. Actions and cameras live here; cell ownership
//! and rigid contacts stay in engine-terrain and engine-rapier respectively.
use std::time::Duration;

use engine_common::{
    Action, Camera2, Observation, PointerPhase, RenderFrame, RenderPoint, Scenario, StepResult,
    TickModel,
};
use engine_core::Vec2;
use engine_rapier::terrain::GrainShape;

use crate::{
    FIXED_HZ,
    blast_lab::{
        Blast, BlastLab, BlastLabConfig, BlastMode, BlastResult, Fixture, MaterialBalance,
        MotionStats,
    },
};

mod render;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GrainPreset {
    #[default]
    Round,
    Grippy,
    Angular,
}
impl GrainPreset {
    pub fn name(self) -> &'static str {
        match self {
            Self::Round => "Round",
            Self::Grippy => "Round + grip",
            Self::Angular => "Angular",
        }
    }
    pub fn friction(self) -> f32 {
        match self {
            Self::Grippy => 1.0,
            _ => 0.6,
        }
    }
    pub fn shape(self) -> GrainShape {
        match self {
            Self::Angular => GrainShape::Hexagon,
            _ => GrainShape::Round,
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Round => Self::Grippy,
            Self::Grippy => Self::Angular,
            Self::Angular => Self::Round,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GranularLabConfig {
    pub fixture: Fixture,
    pub preset: GrainPreset,
    pub cell_size: f32,
    pub max_loose_bodies: usize,
}
impl Default for GranularLabConfig {
    fn default() -> Self {
        Self {
            fixture: Fixture::Flat,
            preset: GrainPreset::Round,
            cell_size: 0.5,
            max_loose_bodies: 192,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GranularControls {
    pub aim: Vec2,
    pub fire: bool,
    pub drop: bool,
    pub preset: bool,
    pub fixture: bool,
    pub budget: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    Fixture,
    Preset,
    Budget,
    Resolution,
    Reset,
    Fire,
    Drop,
    Pause,
    Step,
    Speed,
}
impl Command {
    pub const ALL: [Self; 10] = [
        Self::Fixture,
        Self::Preset,
        Self::Budget,
        Self::Resolution,
        Self::Reset,
        Self::Fire,
        Self::Drop,
        Self::Pause,
        Self::Step,
        Self::Speed,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GranularLabAction {
    Controls(GranularControls),
    Command(Command),
    Aim(Vec2),
}
impl GranularLabAction {
    pub fn encode(self) -> Action {
        match self {
            Self::Controls(c) => {
                let mut bytes = Vec::new();
                bytes.extend(c.aim.x.to_le_bytes());
                bytes.extend(c.aim.y.to_le_bytes());
                bytes.extend([c.fire, c.drop, c.preset, c.fixture, c.budget].map(u8::from));
                Action::scenario(20, bytes)
            }
            Self::Command(c) => Action::scenario(21, vec![c as u8]),
            Self::Aim(p) => Action::scenario(22, [p.x.to_le_bytes(), p.y.to_le_bytes()].concat()),
        }
    }
    pub fn decode(action: &Action) -> Option<Self> {
        let Action::Scenario { kind, payload } = action else {
            return None;
        };
        let point = || {
            Vec2::new(
                f32::from_le_bytes(payload[0..4].try_into().unwrap()),
                f32::from_le_bytes(payload[4..8].try_into().unwrap()),
            )
        };
        match *kind {
            20 if payload.len() == 13 && payload[8..].iter().all(|v| *v <= 1) => {
                let p = point();
                (p.x.is_finite() && p.y.is_finite()).then_some(Self::Controls(GranularControls {
                    aim: Vec2::new(p.x.clamp(-1.0, 1.0), p.y.clamp(-1.0, 1.0)),
                    fire: payload[8] != 0,
                    drop: payload[9] != 0,
                    preset: payload[10] != 0,
                    fixture: payload[11] != 0,
                    budget: payload[12] != 0,
                }))
            }
            21 if payload.len() == 1 => Command::ALL
                .get(payload[0] as usize)
                .copied()
                .map(Self::Command),
            22 if payload.len() == 8 => {
                let p = point();
                (p.x.is_finite() && p.y.is_finite() && p.length() < 1000.0).then_some(Self::Aim(p))
            }
            _ => None,
        }
    }
}

pub struct GranularLabScenario;

#[derive(Clone)]
pub struct GranularLabState {
    pub config: GranularLabConfig,
    pub lab: BlastLab,
    pub paused: bool,
    pub slow: bool,
    pub last_blast: Option<BlastResult>,
    pub status: String,
    pub balance: MaterialBalance,
    pub motion: MotionStats,
    seed: u64,
    aim_local: Vec2,
    controls: GranularControls,
    previous: GranularControls,
    phase: u8,
    view: u8,
    flash: Option<(Vec2, u64)>,
}

impl GranularLabState {
    pub fn new(mut config: GranularLabConfig, seed: u64) -> Self {
        if !config.cell_size.is_finite() || !(0.25..=1.0).contains(&config.cell_size) {
            config.cell_size = 0.5
        }
        config.max_loose_bodies = config.max_loose_bodies.clamp(1, 512);
        let lab = BlastLab::new(BlastLabConfig {
            fixture: config.fixture,
            mode: BlastMode::Grains,
            seed,
            cell_size: config.cell_size,
            max_loose_bodies: config.max_loose_bodies,
            friction: config.preset.friction(),
            grain_shape: config.preset.shape(),
            ..BlastLabConfig::default()
        })
        .expect("bounded granular lab config");
        let balance = lab.audit().unwrap();
        let motion = lab.motion_stats();
        let mut state = Self {
            config,
            lab,
            paused: false,
            slow: false,
            last_blast: None,
            status: "Click the terrain to blast. Drop a box onto the loose dirt.".into(),
            balance,
            motion,
            seed,
            aim_local: Vec2::ZERO,
            controls: GranularControls::default(),
            previous: GranularControls::default(),
            phase: 0,
            view: 1,
            flash: None,
        };
        state.set_aim(state.lab.surface_point(0.0, 0.75));
        state
    }
    pub fn aim(&self) -> Vec2 {
        let m = self.lab.ground_motion();
        m.position + self.aim_local.rotate_radians(m.angle)
    }
    pub fn set_aim(&mut self, p: Vec2) {
        if !p.x.is_finite() || !p.y.is_finite() || p.length() > 1000.0 {
            return;
        }
        let m = self.lab.ground_motion();
        self.aim_local = (p - m.position).rotate_radians(-m.angle);
    }
    pub fn camera(&self) -> Camera2 {
        let (center, height) = if self.view == 0 {
            (
                if self.config.fixture == Fixture::MovingPlanet {
                    self.lab.ground_motion().position
                } else {
                    Vec2::ZERO
                },
                60.0,
            )
        } else {
            (
                self.lab.surface_point(0.0, -3.0),
                if self.view == 1 { 26.0 } else { 17.0 },
            )
        };
        Camera2::new(RenderPoint::new(center.x, center.y), height)
    }
    pub fn zoom_in(&mut self) {
        self.view = (self.view + 1).min(2);
    }
    pub fn zoom_out(&mut self) {
        self.view = self.view.saturating_sub(1);
    }
    pub fn button_center(&self, command: Command) -> Vec2 {
        let camera = self.camera();
        let n = command as usize;
        Vec2::new(camera.center.x, camera.center.y)
            + Vec2::new(
                (n % 5) as f32 * 0.2 - 0.4,
                if n < 5 { 0.405 } else { 0.335 },
            ) * camera.height
    }
    pub fn button_at(&self, p: Vec2) -> Option<Command> {
        let h = self.camera().height;
        Command::ALL.into_iter().find(|c| {
            let d = p - self.button_center(*c);
            d.x.abs() < h * 0.094 && d.y.abs() < h * 0.028
        })
    }
    pub fn button_label(&self, c: Command) -> String {
        match c {
            Command::Fixture => match self.config.fixture {
                Fixture::Flat => "Flat",
                Fixture::Slope => "Slope",
                Fixture::MovingPlanet => "Planet",
            }
            .into(),
            Command::Preset => self.config.preset.name().into(),
            Command::Budget => format!("Limit {}", self.config.max_loose_bodies),
            Command::Resolution => format!("Grain {}", self.config.cell_size),
            Command::Reset => "Reset".into(),
            Command::Fire => "Blast [Space]".into(),
            Command::Drop => "Box [K]".into(),
            Command::Pause => if self.paused { "Resume" } else { "Pause" }.into(),
            Command::Step => "Step".into(),
            Command::Speed => if self.slow { "¼ speed" } else { "Full speed" }.into(),
        }
    }
    fn rebuild(&mut self) {
        let mut replacement = Self::new(self.config, self.seed);
        replacement.controls = self.controls;
        replacement.previous = self.previous;
        replacement.paused = self.paused;
        replacement.slow = self.slow;
        replacement.view = self.view;
        *self = replacement;
    }
    pub fn command(&mut self, command: Command) {
        match command {
            Command::Fixture => {
                self.config.fixture = match self.config.fixture {
                    Fixture::Flat => Fixture::Slope,
                    Fixture::Slope => Fixture::MovingPlanet,
                    Fixture::MovingPlanet => Fixture::Flat,
                };
                self.rebuild()
            }
            Command::Preset => {
                self.config.preset = self.config.preset.next();
                self.rebuild()
            }
            Command::Budget => {
                self.config.max_loose_bodies = match self.config.max_loose_bodies {
                    0..=95 => 96,
                    96..=191 => 192,
                    192..=383 => 384,
                    _ => 96,
                };
                self.rebuild()
            }
            Command::Resolution => {
                self.config.cell_size = if self.config.cell_size == 0.5 {
                    0.25
                } else if self.config.cell_size == 0.25 {
                    1.0
                } else {
                    0.5
                };
                self.rebuild()
            }
            Command::Reset => self.rebuild(),
            Command::Fire => {
                let center = self.aim();
                match self.lab.blast(Blast {
                    center,
                    radius: 3.0,
                    speed: 26.0,
                }) {
                    Ok(result) => {
                        self.status = if result.admitted {
                            self.flash = Some((center, self.lab.tick));
                            format!(
                                "Released {} grains; hit {} loose bodies.",
                                result.spawned_grains, result.loose_bodies_hit
                            )
                        } else {
                            "Limit reached. Reset or raise the limit; the blast changed no material.".into()
                        };
                        self.last_blast = Some(result)
                    }
                    Err(error) => self.status = error.to_string(),
                }
            }
            Command::Drop => {
                self.status = match self.lab.drop_probe(self.aim()) {
                    Ok(()) => "Box dropped above the current surface.".into(),
                    Err(e) => e.to_string(),
                };
            }
            Command::Pause => {
                self.paused = !self.paused;
                self.phase = 0
            }
            Command::Step => {
                self.paused = true;
                self.lab.step()
            }
            Command::Speed => {
                self.slow = !self.slow;
                self.phase = 0
            }
        }
        self.refresh_metrics();
    }
    fn refresh_metrics(&mut self) {
        match self.lab.audit() {
            Ok(balance) => self.balance = balance,
            Err(error) => {
                self.paused = true;
                self.status = error.to_string();
            }
        }
        self.motion = self.lab.motion_stats();
    }
}

impl Scenario for GranularLabScenario {
    type State = GranularLabState;
    type Config = GranularLabConfig;
    fn init(config: Self::Config, seed: u64) -> Self::State {
        GranularLabState::new(config, seed)
    }
    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult {
        if dt.is_zero() {
            state.controls = GranularControls::default();
            state.previous = GranularControls::default();
            return StepResult::default();
        }
        for action in actions {
            match GranularLabAction::decode(action) {
                Some(GranularLabAction::Controls(c)) => state.controls = c,
                Some(GranularLabAction::Command(c)) => state.command(c),
                Some(GranularLabAction::Aim(p)) => state.set_aim(p),
                None => {}
            }
            if let Action::Pointer(pointer) = action
                && pointer.phase == PointerPhase::Press
            {
                let p = Vec2::new(pointer.position.x, pointer.position.y);
                if let Some(c) = state.button_at(p) {
                    state.command(c)
                } else if p.x.is_finite() && p.y.is_finite() && p.length() < 1000.0 {
                    state.set_aim(p);
                    state.command(Command::Fire)
                }
            }
        }
        let c = state.controls;
        let prev = state.previous;
        for (held, was, command) in [
            (c.fixture, prev.fixture, Command::Fixture),
            (c.preset, prev.preset, Command::Preset),
            (c.budget, prev.budget, Command::Budget),
            (c.drop, prev.drop, Command::Drop),
            (c.fire, prev.fire, Command::Fire),
        ] {
            if held && !was {
                state.command(command)
            }
        }
        state.previous = c;
        if c.aim.length_squared() > 0.0 {
            state.set_aim(state.aim() + c.aim * (8.0 / FIXED_HZ as f32))
        }
        if !state.paused {
            state.phase += 1;
            if !state.slow || state.phase >= 4 {
                state.phase = 0;
                state.lab.step();
                if state.lab.tick.is_multiple_of(15) {
                    state.refresh_metrics()
                }
            }
        }
        StepResult::default()
    }
    fn observe(state: &Self::State) -> Observation {
        let mut payload = vec![
            1,
            state.config.fixture as u8,
            state.config.preset as u8,
            u8::from(state.paused),
            u8::from(state.slow),
            state.phase,
            state.view,
        ];
        payload.extend(state.lab.content_motion_hash().to_le_bytes());
        payload.extend(state.aim_local.x.to_le_bytes());
        payload.extend(state.aim_local.y.to_le_bytes());
        payload.extend(state.config.cell_size.to_le_bytes());
        payload.extend((state.config.max_loose_bodies as u64).to_le_bytes());
        for control in [state.controls, state.previous] {
            if let Action::Scenario { payload: bytes, .. } =
                GranularLabAction::Controls(control).encode()
            {
                payload.extend(bytes)
            }
        }
        Observation { payload }
    }
    fn render_frame(state: &Self::State) -> RenderFrame {
        render::frame(state)
    }
    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: FIXED_HZ }
    }
}
