//! A small artillery scene consuming the same terrain lifecycle as Spacewars.
//! Terrain, loose material and tanks share one canonical mechanics world.
use std::time::Duration;

use engine_common::{
    Action, Camera2, Observation, PointerPhase, RenderFrame, RenderPoint, Scenario, StepResult,
    TickModel,
};
use engine_core::Vec2;
use engine_rapier::{
    terrain::{
        GrainShape, LooseTerrain, LooseTerrainConfig, TerrainAssembly, TerrainFragment, TerrainSpec,
    },
    world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
        PhysicsStepMetrics, PhysicsWorld, PhysicsWorldConfig, RayCastOptions,
    },
};
use engine_terrain::{Material, MaterialId, Terrain, TerrainGeometry, TerrainSurface};
use serde::Serialize;

mod action;
mod bank;
mod bot;
pub mod bot_lab;
mod course;
mod driving;
pub mod driving_lab;
mod drop_test;
mod duel;
mod hillside;
pub mod host;
mod material;
mod render;
mod roof;
mod suspension;
#[cfg(test)]
mod tests;
pub use action::{Command, Controls, ScorchedAction};
pub use bot::{CpuPolicy, ShotObservation};
pub use course::DrivingCourse;
pub use driving::DrivingDiagnostics;
pub use drop_test::DropHeight;
pub use engine_rapier::terrain::DirtStrength as BankStrength;
pub use material::MaterialBalance;
pub use suspension::WheelTuning;

pub const FIXED_HZ: u32 = 60;
const DT: f32 = 1.0 / FIXED_HZ as f32;
const GRAVITY: f32 = 18.0;
const GROUND: PhysicsId = PhysicsId::new(1);
const DIRT: MaterialId = MaterialId(1);
const CELL_SIZE: f32 = 0.5;
const FIELD_WIDTH: u32 = 193;
const FIELD_HEIGHT: u32 = 129;
const BLAST_RADIUS: f32 = 2.5;
pub const MAX_SHELLS: usize = 8;
pub const SHELL_LIFETIME_TICKS: u32 = 10 * FIXED_HZ;
pub const MAX_TERRAIN_FRAGMENTS: usize = 64;
/// Native rigid-hull geometry shared by collision and embedded presentation.
pub const RIGID_TANK_HULL: [Vec2; 6] = [
    Vec2::new(-1.5, -0.8),
    Vec2::new(1.5, -0.8),
    Vec2::new(1.9, -0.3),
    Vec2::new(1.6, 0.6),
    Vec2::new(-1.6, 0.6),
    Vec2::new(-1.9, -0.3),
];

#[derive(Debug, Clone, Copy)]
pub struct ScorchedConfig {
    pub shape: GrainShape,
    pub max_grains: usize,
    pub demo: bool,
    /// Local CPU repositioning; disable for the retained stationary duel.
    pub cpu_driving: bool,
    /// Seek nearby flatter firing positions; per-tank switches permit isolated
    /// comparisons with the retained range-only CPU.
    pub cpu_footing: [bool; 2],
    /// Retain the sweep for controlled comparisons and scripted fixtures.
    pub cpu_policy: [CpuPolicy; 2],
    /// Compare real sprung wheels with the original rigid moving surface.
    pub suspension: bool,
    /// Explicit wheel experiments; the default retains the established assembly.
    pub wheel_tuning: WheelTuning,
    /// Repeatable driving obstacle, with firing disabled and the far tank parked.
    pub course: Option<DrivingCourse>,
    /// Course-only travel controller; ordinary combat retains its range policy.
    pub course_goal_driving: bool,
    /// Zero selects the normal duel. Otherwise rain scripted shells for this
    /// duration, then keep solving the altered world without new impacts.
    pub bombardment_seconds: u32,
    pub slumping: bool,
    pub cohesion: BankStrength,
    /// Repeatable support cut, isolated from the artillery scripts.
    pub undercut: bool,
    /// Playable ledge encounter, undermined only by normal tank shells.
    pub hillside: bool,
    /// Two-ended roof thinned by one ordinary tank shot.
    pub roof: bool,
    pub drop_height: Option<DropHeight>,
    /// Enable contact breakup independently of connected-bank failure.
    pub breakup: bool,
}
impl Default for ScorchedConfig {
    fn default() -> Self {
        Self {
            shape: GrainShape::Hexagon,
            max_grains: 192,
            demo: false,
            cpu_driving: true,
            cpu_footing: [false; 2],
            cpu_policy: [CpuPolicy::Sweep; 2],
            suspension: false,
            wheel_tuning: WheelTuning::default(),
            course: None,
            course_goal_driving: false,
            bombardment_seconds: 0,
            slumping: false,
            cohesion: BankStrength::Off,
            undercut: false,
            hillside: false,
            roof: false,
            drop_height: None,
            breakup: false,
        }
    }
}

#[derive(Clone, Copy, Serialize)]
struct Tank {
    body: BodyId,
    elevation: f32,
    speed: f32,
    facing: f32,
    health: f32,
    next_shot: u64,
    shots: u64,
}
impl Tank {
    fn direction(self) -> Vec2 {
        Vec2::new(self.facing * self.elevation.cos(), self.elevation.sin())
    }
}

#[derive(Clone, Copy, Serialize)]
struct Shell {
    owner: usize,
    position: Vec2,
    velocity: Vec2,
    age: u32,
    // Included separately in observations when the learned policy is enabled,
    // preserving the established sweep's physical replay identities.
    #[serde(skip)]
    flight: Option<bot::Flight>,
}

#[derive(Clone)]
/// World-independent simulation state. Standalone inspection fields remain
/// available through `ScorchedState`; hosts use `host::HostedDuel`.
pub struct Duel {
    pub tick: u64,
    pub config: ScorchedConfig,
    pub impacts: u64,
    pub rejected_blasts: u64,
    /// Most recent grain-capacity refusal, sampled before the blast response.
    /// Inspection only; it does not participate in simulation decisions/hashes.
    pub last_capacity_rejection: Option<engine_rapier::terrain::ReleaseCapacity>,
    pub shots: u64,
    pub last_physics: PhysicsStepMetrics,
    seed: u64,
    placement: host::Placement,
    firing: bool,
    retired: [Option<engine_rapier::world::BodyMotion>; 2],
    impact_events: Vec<host::Impact>,
    terrain: Vec<TerrainFragment>,
    loose: LooseTerrain,
    next_id: u64,
    initial_cells: u64,
    tanks: [Tank; 2],
    shells: Vec<Shell>,
    bots: [bot::Bot; 2],
    shot_observations: Vec<ShotObservation>,
    selected: usize,
    previous: Controls,
    driving: [driving::TankDrive; 2],
    course_goal: Option<driving_lab::TravelGoal>,
    course_decision: Option<driving_lab::GoalDecision>,
    running_gear: [Option<engine_rapier::rover::RoverAssembly>; 2],
    manual_drive_armed: bool,
    select_used: bool,
    pointer_drive: f32,
    nudge_drive: f32,
    nudge_until: u64,
    flashes: Vec<(Vec2, u64)>,
    status: &'static str,
}

#[derive(Clone)]
pub struct ScorchedState {
    physics: PhysicsWorld,
    duel: Duel,
}

impl std::ops::Deref for ScorchedState {
    type Target = Duel;
    fn deref(&self) -> &Duel {
        &self.duel
    }
}
impl std::ops::DerefMut for ScorchedState {
    fn deref_mut(&mut self) -> &mut Duel {
        &mut self.duel
    }
}

pub struct ScorchedScenario;

fn hill(x: f32, seed: u64) -> f32 {
    let phase = (seed % 1000) as f32 * 0.006;
    -5.0 + 3.8 * (x * 0.09 + phase).sin() + 1.3 * (x * 0.22 - phase).cos()
}

impl ScorchedState {
    pub fn new(config: ScorchedConfig, seed: u64) -> Self {
        let mut physics = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -GRAVITY),
            solver_iterations: 8,
            internal_stabilization_iterations: 2,
            max_ccd_substeps: 4,
            ..Default::default()
        });
        let mut duel = Duel::insert(&mut physics, config, seed, host::Placement::standalone());
        physics.step(DT);
        duel.driving = std::array::from_fn(|i| {
            driving::TankDrive::new(physics.motion(duel.tanks[i].body).unwrap().position)
        });
        let mut state = Self { physics, duel };
        if state.config.course == Some(DrivingCourse::Rubble) {
            state.release_course_rubble();
        }
        if state.config.roof {
            state.aim_at_roof();
        } else if state.config.hillside {
            state.aim_at_hillside();
        }
        state
    }
    pub fn world(&self) -> &PhysicsWorld {
        &self.physics
    }
    fn advance(&mut self, drive: f32) {
        let drives = self.duel.prepare_controls(&mut self.physics, drive);
        self.duel.last_physics = self.physics.step_with_surface_drives(DT, &drives);
        self.duel.finish_step(&mut self.physics);
    }
    fn advance_with_drive_override(
        &mut self,
        drive: f32,
        axis: Option<(usize, f32)>,
        ground: [driving::GroundCheckFn; 2],
        support: [driving::SupportCheckFn; 2],
    ) {
        let drives =
            self.duel
                .prepare_with_drive_override(&mut self.physics, drive, axis, ground, support);
        self.duel.last_physics = self.physics.step_with_surface_drives(DT, &drives);
        self.duel.finish_step(&mut self.physics);
    }

    pub fn camera(&self) -> Camera2 {
        Camera2::new(RenderPoint::new(0.0, 0.0), 64.0)
    }
    pub fn body_count(&self) -> usize {
        self.physics.body_count()
    }
    pub fn collider_count(&self) -> usize {
        self.physics.collider_count()
    }
    pub fn command(&mut self, command: Command) {
        match command {
            Command::AimDown => {
                self.duel.tanks[self.duel.selected].elevation -= 3.0_f32.to_radians()
            }
            Command::AimUp => self.duel.tanks[self.duel.selected].elevation += 3.0_f32.to_radians(),
            Command::PowerDown => self.duel.tanks[self.duel.selected].speed -= 1.0,
            Command::PowerUp => self.duel.tanks[self.duel.selected].speed += 1.0,
            Command::Fire => self.fire(self.selected),
            Command::Select => {
                if self.config.course.is_none() {
                    self.selected = 1 - self.selected;
                }
                self.clear_driving_input();
            }
            Command::Demo => {
                self.clear_driving_input();
                self.config.demo = !self.config.demo;
                if self.config.hillside || self.config.roof {
                    *self = Self::new(self.config, self.seed);
                } else if self.config.bombardment_seconds > 0
                    || self.config.undercut
                    || self.config.drop_height.is_some()
                {
                    self.config.bombardment_seconds = 0;
                    self.config.undercut = false;
                    self.config.drop_height = None;
                    *self = Self::new(self.config, self.seed);
                }
            }
            Command::Collapse => {
                if self.config.hillside || self.config.roof {
                    self.config = self.config.with_destruction(self.config.cohesion.next());
                } else if self.config.undercut || self.config.drop_height.is_some() {
                    self.config.cohesion = self.config.cohesion.next();
                } else {
                    self.config.slumping = !self.config.slumping;
                }
                *self = Self::new(self.config, self.seed);
            }
            Command::Barrage => {
                if let Some(course) = self.config.course {
                    self.config.course = course.next();
                } else if self.config.roof {
                    self.config.roof = false;
                    self.config.course = Some(DrivingCourse::Flat);
                } else if self.config.hillside {
                    self.config.hillside = false;
                    self.config.roof = true;
                    self.config = self.config.with_destruction(BankStrength::Weak);
                } else if self.config.drop_height == Some(DropHeight::High) {
                    self.config.drop_height = None;
                    self.config.hillside = true;
                    self.config = self.config.with_destruction(BankStrength::Weak);
                } else if self.config.drop_height.is_some() {
                    self.config.drop_height = match self.config.drop_height {
                        Some(DropHeight::Low) => Some(DropHeight::High),
                        _ => None,
                    };
                } else if self.config.undercut {
                    self.config.undercut = false;
                    self.config.drop_height = Some(DropHeight::Low);
                    self.config.breakup = true;
                } else if self.config.bombardment_seconds > 0 {
                    self.config.undercut = true;
                    self.config.breakup = true;
                    self.config.bombardment_seconds = 0;
                    self.config.slumping = false;
                    if self.config.cohesion == BankStrength::Off {
                        self.config.cohesion = BankStrength::Weak;
                    }
                } else {
                    self.config.bombardment_seconds = 60;
                }
                if self.config.course.is_none() {
                    self.config.demo = self.config.hillside || self.config.roof;
                }
                *self = Self::new(self.config, self.seed);
            }
            Command::Shape => {
                self.config.shape = match self.config.shape {
                    GrainShape::Round => GrainShape::Hexagon,
                    _ => GrainShape::Round,
                };
                *self = Self::new(self.config, self.seed);
            }
            Command::Reset => *self = Self::new(self.config, self.seed),
            Command::Roof => {
                self.config.course = None;
                self.config.roof = !self.config.roof;
                self.config.demo = self.config.roof;
                self.config.hillside = false;
                self.config.undercut = false;
                self.config.drop_height = None;
                self.config.bombardment_seconds = 0;
                if self.config.roof {
                    self.config = self.config.with_destruction(BankStrength::Weak);
                }
                *self = Self::new(self.config, self.seed);
            }
            Command::Hillside => {
                self.config.course = None;
                self.config.roof = false;
                self.config.hillside = !self.config.hillside;
                self.config.demo = self.config.hillside;
                self.config.undercut = false;
                self.config.drop_height = None;
                self.config.bombardment_seconds = 0;
                if self.config.hillside || self.config.roof {
                    self.config = self.config.with_destruction(BankStrength::Weak);
                }
                *self = Self::new(self.config, self.seed);
            }
            Command::Bank => {
                self.config.course = None;
                self.config.roof = false;
                self.config.hillside = false;
                self.config.undercut = !self.config.undercut;
                self.config.drop_height = None;
                if self.config.undercut {
                    self.config.breakup = true;
                    self.config.slumping = false;
                    if self.config.cohesion == BankStrength::Off {
                        self.config.cohesion = BankStrength::Weak;
                    }
                }
                *self = Self::new(self.config, self.seed);
            }
            Command::Drop => {
                self.config.course = None;
                self.config.roof = false;
                self.config.hillside = false;
                self.config.drop_height = match self.config.drop_height {
                    None => Some(DropHeight::Low),
                    Some(DropHeight::Low) => Some(DropHeight::High),
                    Some(DropHeight::High) => None,
                };
                self.config.undercut = false;
                self.config.breakup = true;
                self.config.slumping = false;
                self.config.bombardment_seconds = 0;
                if self.config.cohesion == BankStrength::Off {
                    self.config.cohesion = BankStrength::Weak;
                }
                *self = Self::new(self.config, self.seed);
            }
            Command::Strength => {
                if self.config.undercut || self.config.drop_height.is_some() {
                    self.config.cohesion = self.config.cohesion.next();
                } else {
                    self.config = self.config.with_destruction(self.config.cohesion.next());
                }
                *self = Self::new(self.config, self.seed);
            }
            Command::DriveLeft | Command::DriveRight => {
                self.nudge_drive = if command == Command::DriveLeft {
                    -1.0
                } else {
                    1.0
                };
                self.nudge_until = self.tick + 20;
                self.manual_drive_armed = true;
            }
            Command::CpuDrive => {
                if self.config.course.is_some()
                    && self.config.cpu_driving
                    && !self.config.course_goal_driving
                {
                    self.config.course_goal_driving = true;
                } else {
                    self.config.cpu_driving = !self.config.cpu_driving;
                    if self.config.cpu_driving && self.config.course.is_some() {
                        self.config.course_goal_driving = false;
                    }
                }
                self.clear_driving_input();
            }
            Command::Suspension => {
                self.config.suspension = !self.config.suspension;
                *self = Self::new(self.config, self.seed);
            }
            Command::CpuAim => {
                self.config.cpu_policy = [self.config.cpu_policy[0].next(); 2];
                *self = Self::new(self.config, self.seed);
            }
            Command::Course => {
                self.config.course = self
                    .config
                    .course
                    .map_or(Some(DrivingCourse::Flat), |c| c.next());
                self.config.roof = false;
                self.config.hillside = false;
                self.config.undercut = false;
                self.config.drop_height = None;
                self.config.bombardment_seconds = 0;
                *self = Self::new(self.config, self.seed);
            }
        }
        self.clamp_aim();
    }
    pub fn observation_hash(&self) -> u64 {
        ScorchedScenario::observe(self)
            .payload
            .into_iter()
            .fold(0xcbf29ce484222325, |h, b| {
                (h ^ b as u64).wrapping_mul(0x100000001b3)
            })
    }
}

impl Scenario for ScorchedScenario {
    type State = ScorchedState;
    type Config = ScorchedConfig;
    fn init(config: Self::Config, seed: u64) -> Self::State {
        ScorchedState::new(config, seed)
    }
    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: FIXED_HZ }
    }
    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult {
        if dt.is_zero() {
            state.previous = Controls::default();
            state.clear_driving_input();
            state.select_used = false;
            return StepResult::default();
        }
        let mut controls = Controls::default();
        for action in actions {
            match ScorchedAction::decode(action) {
                Some(ScorchedAction::Controls(c)) => controls = c,
                Some(ScorchedAction::Command(c)) => state.command(c),
                None => {}
            }
            if let Action::Pointer(p) = action {
                let command = render::button_at(Vec2::new(p.position.x, p.position.y));
                match p.phase {
                    PointerPhase::Press => match command {
                        Some(Command::DriveLeft | Command::DriveRight) => {
                            state.pointer_drive = if command == Some(Command::DriveLeft) {
                                -1.0
                            } else {
                                1.0
                            };
                            state.manual_drive_armed = true;
                        }
                        Some(c) => state.command(c),
                        None => {}
                    },
                    PointerPhase::Drag
                        if command
                            != Some(if state.pointer_drive < 0.0 {
                                Command::DriveLeft
                            } else {
                                Command::DriveRight
                            }) =>
                    {
                        state.pointer_drive = 0.0
                    }
                    PointerPhase::Release | PointerPhase::Cancel => state.pointer_drive = 0.0,
                    _ => {}
                }
            }
        }
        // Defer a tap until release so a later joystick/chord press cannot
        // inadvertently transfer control to the other tank first.
        let select_used = state.select_used
            || (controls.select && (controls.shape || controls.collapse || controls.drive != 0.0));
        if !controls.select && state.previous.select && !select_used {
            state.command(Command::Select);
        }
        for (held, before, command) in [
            (controls.demo, state.previous.demo, Command::Demo),
            (
                controls.shape && !controls.select,
                state.previous.shape,
                Command::Shape,
            ),
            (
                controls.collapse && !controls.select,
                state.previous.collapse,
                Command::Collapse,
            ),
            (controls.barrage, state.previous.barrage, Command::Barrage),
            (
                controls.select && controls.shape,
                state.previous.select && state.previous.shape,
                Command::Bank,
            ),
            (
                controls.select && controls.collapse,
                state.previous.select && state.previous.collapse,
                Command::Strength,
            ),
        ] {
            if held && !before {
                state.command(command)
            }
        }
        state.select_used = controls.select && select_used;
        if controls.drive == 0.0 {
            state.manual_drive_armed = true;
        }
        let manual_drive = if state.manual_drive_armed {
            if controls.drive != 0.0 {
                controls.drive
            } else if state.pointer_drive != 0.0 {
                state.pointer_drive
            } else if state.tick < state.nudge_until {
                state.nudge_drive
            } else {
                0.0
            }
        } else {
            0.0
        };
        if !state.config.demo {
            state.duel.tanks[state.duel.selected].elevation +=
                controls.aim * DT * 40.0_f32.to_radians();
            state.duel.tanks[state.duel.selected].speed += controls.power * DT * 12.0;
            state.clamp_aim();
            if controls.fire {
                state.fire(state.selected)
            }
        }
        state.previous = controls;
        state.advance(manual_drive);
        StepResult::default()
    }
    fn observe(state: &Self::State) -> Observation {
        state.duel.observe(&state.physics)
    }
    fn render_frame(state: &Self::State) -> RenderFrame {
        render::frame(state)
    }
}
