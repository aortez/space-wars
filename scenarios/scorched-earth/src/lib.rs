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
mod material;
mod render;
#[cfg(test)]
mod tests;
pub use action::{Command, Controls, ScorchedAction};
pub use material::MaterialBalance;

pub const FIXED_HZ: u32 = 60;
const DT: f32 = 1.0 / FIXED_HZ as f32;
const GRAVITY: f32 = 18.0;
const GROUND: PhysicsId = PhysicsId::new(1);
const DIRT: MaterialId = MaterialId(1);
const CELL_SIZE: f32 = 0.5;
const BLAST_RADIUS: f32 = 2.5;
const MAX_SHELLS: usize = 8;

#[derive(Debug, Clone, Copy)]
pub struct ScorchedConfig {
    pub shape: GrainShape,
    pub max_grains: usize,
    pub demo: bool,
    /// Zero selects the normal duel. Otherwise rain scripted shells for this
    /// duration, then keep solving the altered world without new impacts.
    pub bombardment_seconds: u32,
}
impl Default for ScorchedConfig {
    fn default() -> Self {
        Self {
            shape: GrainShape::Hexagon,
            max_grains: 192,
            demo: false,
            bombardment_seconds: 0,
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
}

#[derive(Clone)]
pub struct ScorchedState {
    pub tick: u64,
    pub config: ScorchedConfig,
    pub impacts: u64,
    pub rejected_blasts: u64,
    pub shots: u64,
    pub last_physics: PhysicsStepMetrics,
    seed: u64,
    physics: PhysicsWorld,
    terrain: Vec<TerrainFragment>,
    loose: LooseTerrain,
    next_id: u64,
    initial_cells: u64,
    tanks: [Tank; 2],
    shells: Vec<Shell>,
    selected: usize,
    previous: Controls,
    flashes: Vec<(Vec2, u64)>,
    status: &'static str,
}

pub struct ScorchedScenario;

fn hill(x: f32, seed: u64) -> f32 {
    let phase = (seed % 1000) as f32 * 0.006;
    -5.0 + 3.8 * (x * 0.09 + phase).sin() + 1.3 * (x * 0.22 - phase).cos()
}

impl ScorchedState {
    pub fn new(mut config: ScorchedConfig, seed: u64) -> Self {
        config.max_grains = config.max_grains.clamp(1, 512);
        config.bombardment_seconds = config.bombardment_seconds.min(600);
        let field = Terrain::generate(
            193,
            129,
            CELL_SIZE,
            vec![Material {
                id: DIRT,
                hardness: 100,
            }],
            |c| {
                let x = (c.x as f32 + 0.5 - 193.0 * 0.5) * CELL_SIZE;
                let y = (c.y as f32 + 0.5 - 129.0 * 0.5) * CELL_SIZE;
                if y <= hill(x, seed) {
                    DIRT
                } else {
                    MaterialId::VOID
                }
            },
        )
        .expect("bounded hill field");
        let initial_cells = field
            .cells()
            .iter()
            .filter(|c| c.material != MaterialId::VOID)
            .count() as u64;
        let geometry = TerrainGeometry::with_surface(&field, TerrainSurface::Interpolated);
        let mut physics = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -GRAVITY),
            solver_iterations: 8,
            internal_stabilization_iterations: 2,
            max_ccd_substeps: 4,
            ..Default::default()
        });
        let assembly = TerrainAssembly::insert(
            &mut physics,
            GROUND,
            BodySpec {
                kind: BodyKind::Fixed,
                ..Default::default()
            },
            &field,
            &geometry,
            TerrainSpec {
                surface: TerrainSurface::Interpolated,
                ..Default::default()
            },
        )
        .expect("valid ground");
        let ground = TerrainFragment {
            id: GROUND,
            hash: field.hash(),
            terrain: field,
            geometry,
            assembly,
            edited_chunks: Vec::new(),
        };
        let tanks = std::array::from_fn(|index| {
            let id = PhysicsId::new(10 + index as u64);
            let body = BodyId::new(id, BodyRole::PRIMARY);
            let x = if index == 0 { -27.0 } else { 27.0 };
            let mut collider = ColliderSpec::convex_polygon(
                ColliderId::new(id, ColliderRole::PRIMARY, 0),
                vec![
                    Vec2::new(-1.5, -0.8),
                    Vec2::new(1.5, -0.8),
                    Vec2::new(1.9, -0.3),
                    Vec2::new(1.6, 0.6),
                    Vec2::new(-1.6, 0.6),
                    Vec2::new(-1.9, -0.3),
                ],
            );
            collider.density = 8.0;
            collider.friction = 1.0;
            assert!(physics.insert_body(
                body,
                BodySpec {
                    position: Vec2::new(x, hill(x, seed) + 2.0),
                    angular_damping: 2.0,
                    ccd_enabled: true,
                    ..Default::default()
                },
                &[collider]
            ));
            Tank {
                body,
                elevation: 50.0_f32.to_radians(),
                speed: 32.0,
                facing: if index == 0 { 1.0 } else { -1.0 },
                health: 100.0,
                next_shot: index as u64 * 300,
                shots: 0,
            }
        });
        physics.step(DT);
        Self {
            tick: 0,
            config,
            impacts: 0,
            rejected_blasts: 0,
            shots: 0,
            last_physics: PhysicsStepMetrics::default(),
            seed,
            physics,
            terrain: vec![ground],
            loose: LooseTerrain::new(LooseTerrainConfig {
                max_grains: config.max_grains,
                shape: config.shape,
                ..Default::default()
            })
            .expect("bounded loose material"),
            next_id: 100,
            initial_cells,
            tanks,
            shells: Vec::new(),
            selected: 0,
            previous: Controls::default(),
            flashes: Vec::new(),
            status: "Aim and fire. The other tank returns fire; Demo controls both.",
        }
    }

    pub fn camera(&self) -> Camera2 {
        Camera2::new(RenderPoint::new(0.0, 0.0), 64.0)
    }
    pub fn loose_cells(&self) -> usize {
        self.loose.len()
    }
    pub fn deposited_cells(&self) -> u64 {
        self.loose.deposited_cells()
    }
    pub fn settling_diagnostics(&self) -> engine_rapier::terrain::SettlingDiagnostics {
        self.loose.settling_diagnostics()
    }
    pub fn body_count(&self) -> usize {
        self.physics.body_count()
    }
    pub fn collider_count(&self) -> usize {
        self.physics.collider_count()
    }
    pub fn winner(&self) -> Option<usize> {
        match (self.tanks[0].health > 0.0, self.tanks[1].health > 0.0) {
            (true, false) => Some(0),
            (false, true) => Some(1),
            _ => None,
        }
    }
    fn fighting(&self) -> bool {
        self.tanks.iter().all(|t| t.health > 0.0)
    }
    fn muzzle(&self, player: usize) -> (Vec2, Vec2) {
        let tank = self.tanks[player];
        let motion = self.physics.motion(tank.body).unwrap();
        let pivot = motion.position + Vec2::Y.rotate_radians(motion.angle) * 0.85;
        (pivot, pivot + tank.direction() * 2.8)
    }
    fn fire(&mut self, player: usize) {
        if self.config.bombardment_seconds > 0
            || !self.fighting()
            || self.tick < self.tanks[player].next_shot
            || self.shells.len() >= MAX_SHELLS
        {
            return;
        }
        let tank = self.tanks[player];
        let motion = self.physics.motion(tank.body).unwrap();
        let (pivot, muzzle) = self.muzzle(player);
        // The barrel cannot shoot through the ground or another actor.
        let position = self
            .physics
            .cast_ray(
                pivot,
                muzzle - pivot,
                RayCastOptions {
                    max_distance: (muzzle - pivot).length(),
                    exclude_entity: Some(tank.body.entity),
                    ..Default::default()
                },
            )
            .map_or(muzzle, |hit| hit.point);
        self.shells.push(Shell {
            owner: player,
            position,
            velocity: tank.direction() * tank.speed + motion.linear_velocity,
            age: 0,
        });
        self.tanks[player].next_shot = self.tick + 75;
        self.tanks[player].shots += 1;
        self.shots += 1;
    }
    fn aim_bot(&mut self, player: usize) {
        let origin = self
            .physics
            .motion(self.tanks[player].body)
            .unwrap()
            .position
            + Vec2::Y;
        let target = self
            .physics
            .motion(self.tanks[1 - player].body)
            .unwrap()
            .position;
        // A repeatable aiming sweep, not privileged perfect targeting. Ground
        // deformation and intervening loose material can intercept any shot.
        let phase = (self.tanks[player].shots % 5 + self.seed % 5 + player as u64) % 5;
        let angle = (46.0 + phase as f32 * 3.0).to_radians();
        let dx = (target.x - origin.x).abs().max(1.0);
        let dy = target.y - origin.y;
        let denominator = 2.0 * angle.cos().powi(2) * (dx * angle.tan() - dy);
        let speed = if denominator > 0.0 {
            (GRAVITY * dx * dx / denominator).sqrt()
        } else {
            40.0
        };
        self.tanks[player].facing = if target.x >= origin.x { 1.0 } else { -1.0 };
        self.tanks[player].elevation = angle;
        self.tanks[player].speed = (speed * (0.93 + phase as f32 * 0.025)).clamp(12.0, 48.0);
    }
    fn advance(&mut self) {
        if self.config.bombardment_seconds > 0 {
            // Health-independent rain exercises the real swept-shell impact
            // path. Later shots encounter the already deformed terrain.
            if self.tick < u64::from(self.config.bombardment_seconds) * u64::from(FIXED_HZ)
                && self.tick % 120 == 0
                && self.shells.len() < MAX_SHELLS
            {
                let phase = (self.shots % 9 + self.seed % 9) % 9;
                self.shells.push(Shell {
                    owner: (self.shots % 2) as usize,
                    position: Vec2::new(-24.0 + phase as f32 * 6.0, 24.0),
                    velocity: Vec2::new(0.0, -8.0),
                    age: 10,
                });
                self.shots += 1;
            }
        } else if self.fighting() {
            for player in 0..2 {
                if (self.config.demo || player != self.selected)
                    && self.tick >= self.tanks[player].next_shot
                {
                    self.aim_bot(player);
                    self.fire(player);
                    self.tanks[player].next_shot = self.tick + 300;
                }
            }
        }
        self.last_physics = self.physics.step(DT);
        let mut impacts = Vec::new();
        self.shells.retain_mut(|shell| {
            let movement = shell.velocity * DT + Vec2::new(0.0, -0.5 * GRAVITY * DT * DT);
            let hit = self.physics.cast_ray(
                shell.position,
                movement,
                RayCastOptions {
                    max_distance: movement.length(),
                    exclude_entity: (shell.age < 8).then_some(self.tanks[shell.owner].body.entity),
                    ..Default::default()
                },
            );
            if let Some(hit) = hit {
                impacts.push(hit.point);
                return false;
            }
            shell.position += movement;
            shell.velocity.y -= GRAVITY * DT;
            shell.age += 1;
            shell.age < 600 && shell.position.y > -50.0 && shell.position.x.abs() < 100.0
        });
        for point in impacts {
            self.explode(point);
        }
        self.settle();
        for tank in &mut self.tanks {
            let position = self.physics.motion(tank.body).unwrap().position;
            if position.y < -45.0 {
                tank.health = 0.0
            }
        }
        self.tick += 1;
        self.flashes.retain(|(_, tick)| self.tick - tick < 22);
    }
    pub fn command(&mut self, command: Command) {
        match command {
            Command::AimDown => self.tanks[self.selected].elevation -= 3.0_f32.to_radians(),
            Command::AimUp => self.tanks[self.selected].elevation += 3.0_f32.to_radians(),
            Command::PowerDown => self.tanks[self.selected].speed -= 1.0,
            Command::PowerUp => self.tanks[self.selected].speed += 1.0,
            Command::Fire => self.fire(self.selected),
            Command::Select => self.selected = 1 - self.selected,
            Command::Demo => self.config.demo = !self.config.demo,
            Command::Shape => {
                self.config.shape = match self.config.shape {
                    GrainShape::Round => GrainShape::Hexagon,
                    _ => GrainShape::Round,
                };
                *self = Self::new(self.config, self.seed);
            }
            Command::Reset => *self = Self::new(self.config, self.seed),
        }
        self.clamp_aim();
    }
    fn clamp_aim(&mut self) {
        let tank = &mut self.tanks[self.selected];
        tank.elevation = tank
            .elevation
            .clamp(5.0_f32.to_radians(), 85.0_f32.to_radians());
        tank.speed = tank.speed.clamp(12.0, 48.0);
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
            return StepResult::default();
        }
        let mut controls = Controls::default();
        for action in actions {
            match ScorchedAction::decode(action) {
                Some(ScorchedAction::Controls(c)) => controls = c,
                Some(ScorchedAction::Command(c)) => state.command(c),
                None => {}
            }
            if let Action::Pointer(p) = action
                && p.phase == PointerPhase::Press
                && let Some(command) = render::button_at(Vec2::new(p.position.x, p.position.y))
            {
                state.command(command);
            }
        }
        for (held, before, command) in [
            (controls.select, state.previous.select, Command::Select),
            (controls.demo, state.previous.demo, Command::Demo),
            (controls.shape, state.previous.shape, Command::Shape),
        ] {
            if held && !before {
                state.command(command)
            }
        }
        if !state.config.demo {
            state.tanks[state.selected].elevation += controls.aim * DT * 40.0_f32.to_radians();
            state.tanks[state.selected].speed += controls.power * DT * 12.0;
            state.clamp_aim();
            if controls.fire {
                state.fire(state.selected)
            }
        }
        state.previous = controls;
        state.advance();
        StepResult::default()
    }
    fn observe(state: &Self::State) -> Observation {
        let motion = |body| {
            let m = state
                .physics
                .motion(body)
                .expect("retained physical entity");
            [
                m.position.x,
                m.position.y,
                m.angle,
                m.linear_velocity.x,
                m.linear_velocity.y,
                m.angular_velocity,
            ]
        };
        let fields: Vec<_> = state
            .terrain
            .iter()
            .map(|b| (b.id.value(), b.terrain.hash(), motion(b.assembly.body())))
            .collect();
        let grains: Vec<_> = state
            .loose
            .iter()
            .map(|g| (g.body(), g.cell(), motion(g.body())))
            .collect();
        let tanks: Vec<_> = state.tanks.iter().map(|t| (t, motion(t.body))).collect();
        Observation { payload: serde_json::to_vec(&serde_json::json!({
            "seed": state.seed, "tick": state.tick, "shape": format!("{:?}", state.config.shape),
            "limit": state.config.max_grains, "demo": state.config.demo, "selected": state.selected,
            "bombardment_seconds": state.config.bombardment_seconds,
            "settling_hash": state.loose.settling_hash(), "next_id": state.next_id,
            "shots": state.shots, "impacts": state.impacts, "rejected": state.rejected_blasts,
            "deposited": state.deposited_cells(), "fields": fields, "grains": grains,
            "tanks": tanks, "shells": state.shells,
        })).expect("finite scenario observation") }
    }
    fn render_frame(state: &Self::State) -> RenderFrame {
        render::frame(state)
    }
}
