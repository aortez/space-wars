//! Deterministic Rust port of the allan.pizza ball simulation.
//!
//! Rapier owns rigid-body motion and contacts. The shared engine gravity
//! solver supplies either exact O(n²) gravity or deterministic Barnes-Hut
//! gravity, making this scenario both an interactive toy and a scale lab.

use std::time::{Duration, Instant};

use engine_common::{
    Action, Camera2, Observation, PointerAction, PointerPhase, RenderCircle, RenderColor,
    RenderFrame, RenderPoint, RenderPrimitive, RenderText, Scenario, StepResult, Stroke,
    TextAnchor, TickModel,
};
use engine_core::{Color, Vec2};
use engine_gravity::{
    GravityBackend, GravityConfig, GravityId, GravityParticipant, GravitySolver, GravityStepMetrics,
};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, ContactEvent,
    PhysicsId, PhysicsStepMetrics, PhysicsWorld, PhysicsWorldConfig,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

pub const MAX_BALLS: usize = 500;
pub const MAX_BENCHMARK_BALLS: usize = 10_000;
pub const DEFAULT_BENCHMARK_BALLS: usize = 300;
pub const MIN_SPAWN_RATE: f32 = 0.01;
pub const MAX_SPAWN_RATE: f32 = 0.99;

const GRAVITY: f32 = 0.0005;
const GRAVITY_SOFTENING: f32 = 0.000_001;
const WALL_ELASTICITY: f32 = 0.9;
const WALL_FUDGE: f32 = 0.000_01;
const COLLISION_ELASTICITY: f32 = 0.9;
const DAMAGE_SCALAR: f32 = 0.01;
const MIN_BALL_RADIUS: f32 = 0.002;
const MIN_FRAGMENT_RADIUS: f32 = 0.0002;
const MAX_RANDOM_BALL_RADIUS: f32 = 0.08;
const MIN_RANDOM_BALL_RADIUS: f32 = 0.01;
const EXPLOSION_DIVISIONS: usize = 2;
const EXPLOSION_VELOCITY_FACTOR: f32 = 0.1;
const EXPLOSION_SIZE_FACTOR: f32 = 1.5;
const EXPLOSION_PARENT_VELOCITY_FACTOR: f32 = 0.5;
const CURSOR_FORCE: f32 = 0.05;
const CURSOR_SMOOTHING: f32 = 0.80;
const WORLD_TIME_SCALE: f32 = 0.9;
const CURSOR_TIME_SCALE: f32 = 3.0;
const BALL_LAYER: i32 = 0;
const BENCHMARK_LABEL_LAYER: i32 = 10;
const BENCHMARK_CHURN_INTERVAL_TICKS: u64 = 120;
const BENCHMARK_CHURN_NUMERATOR: usize = 3;
const BENCHMARK_CHURN_DENOMINATOR: usize = 4;
const WALL_ENTITY: PhysicsId = PhysicsId::new(0);
const WALL_BODY: BodyId = BodyId::new(WALL_ENTITY, BodyRole::new(1));
const WALL_COLLIDER_ROLE: ColliderRole = ColliderRole::new(1);
const BALL_BODY_ROLE: BodyRole = BodyRole::new(2);
const BALL_COLLIDER_ROLE: ColliderRole = ColliderRole::new(2);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PizzaPhysicsBackend {
    Classic,
    #[default]
    Rapier,
}

impl PizzaPhysicsBackend {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Rapier => "rapier",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PizzaGravityModel {
    Exact,
    Full,
    #[default]
    Fast,
}

impl PizzaGravityModel {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Full => "barnes-hut-full",
            Self::Fast => "barnes-hut-fast",
        }
    }

    const fn backend(self) -> GravityBackend {
        match self {
            Self::Exact => GravityBackend::Exact,
            Self::Full => GravityBackend::BarnesHut { theta: 0.5 },
            Self::Fast => GravityBackend::BarnesHut { theta: 0.7 },
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PizzaBenchmarkWorkload {
    Sparse,
    #[default]
    Dense,
    Churn,
}

impl PizzaBenchmarkWorkload {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sparse => "sparse",
            Self::Dense => "dense",
            Self::Churn => "churn",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PizzaBenchmarkConfig {
    pub backend: PizzaPhysicsBackend,
    pub gravity: PizzaGravityModel,
    pub workload: PizzaBenchmarkWorkload,
    pub ball_count: usize,
}

impl PizzaBenchmarkConfig {
    pub fn normalized(self) -> Self {
        Self {
            ball_count: self.ball_count.min(MAX_BENCHMARK_BALLS),
            ..self
        }
    }
}

impl Default for PizzaBenchmarkConfig {
    fn default() -> Self {
        Self {
            backend: PizzaPhysicsBackend::Rapier,
            gravity: PizzaGravityModel::Fast,
            workload: PizzaBenchmarkWorkload::Dense,
            ball_count: DEFAULT_BENCHMARK_BALLS,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PizzaStepMetrics {
    pub workload_time: Duration,
    pub lifecycle_time: Duration,
    pub gravity_time: Duration,
    pub collision_time: Duration,
    pub physics_time: Duration,
    pub snapshot_time: Duration,
    pub added: usize,
    pub removed: usize,
    pub gravity: GravityStepMetrics,
    pub rapier: PhysicsStepMetrics,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PizzaBenchmarkCounts {
    pub balls: usize,
    pub active_bodies: usize,
    pub sleeping_bodies: usize,
    pub candidate_pairs: usize,
    pub contact_pairs: usize,
    pub contacts: usize,
    pub added: usize,
    pub removed: usize,
}

pub struct PizzaScenario;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PizzaBounds {
    pub width: f32,
    pub height: f32,
}

impl PizzaBounds {
    pub fn from_aspect_ratio(aspect_ratio: f32) -> Self {
        let aspect_ratio = if aspect_ratio.is_finite() && aspect_ratio > 0.0 {
            aspect_ratio
        } else {
            16.0 / 9.0
        };
        if aspect_ratio >= 1.0 {
            Self {
                width: 1.0,
                height: 1.0 / aspect_ratio,
            }
        } else {
            Self {
                width: aspect_ratio,
                height: 1.0,
            }
        }
    }

    fn center(self) -> Vec2 {
        Vec2::new(self.width * 0.5, self.height * 0.5)
    }
}

impl Default for PizzaBounds {
    fn default() -> Self {
        Self::from_aspect_ratio(16.0 / 9.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PizzaConfig {
    pub desired_ball_count: usize,
    pub ball_spawn_rate: f32,
    pub bounds: PizzaBounds,
    pub gravity: PizzaGravityModel,
    pub benchmark: Option<PizzaBenchmarkConfig>,
}

impl PizzaConfig {
    pub fn normalized(&self) -> Self {
        let benchmark = self.benchmark.map(PizzaBenchmarkConfig::normalized);
        let maximum_balls = if benchmark.is_some() {
            MAX_BENCHMARK_BALLS
        } else {
            MAX_BALLS
        };
        Self {
            desired_ball_count: benchmark
                .map(|benchmark| benchmark.ball_count)
                .unwrap_or(self.desired_ball_count)
                .min(maximum_balls),
            ball_spawn_rate: if self.ball_spawn_rate.is_finite() {
                self.ball_spawn_rate.clamp(MIN_SPAWN_RATE, MAX_SPAWN_RATE)
            } else {
                0.10
            },
            bounds: PizzaBounds::from_aspect_ratio(self.bounds.width / self.bounds.height),
            gravity: benchmark
                .map(|benchmark| benchmark.gravity)
                .unwrap_or(self.gravity),
            benchmark,
        }
    }

    pub fn benchmark(benchmark: PizzaBenchmarkConfig, bounds: PizzaBounds) -> Self {
        let benchmark = benchmark.normalized();
        Self {
            desired_ball_count: benchmark.ball_count,
            ball_spawn_rate: MAX_SPAWN_RATE,
            bounds,
            gravity: benchmark.gravity,
            benchmark: Some(benchmark),
        }
    }
}

impl Default for PizzaConfig {
    fn default() -> Self {
        Self {
            desired_ball_count: 24,
            ball_spawn_rate: 0.10,
            bounds: PizzaBounds::default(),
            gravity: PizzaGravityModel::Fast,
            benchmark: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallState {
    pub id: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
    pub color: Color,
    pub hp: f32,
    pub moving: bool,
    pub invincible: bool,
}

impl BallState {
    fn new(id: u64, position: Vec2, radius: f32, color: Color) -> Self {
        Self {
            id,
            position,
            velocity: Vec2::ZERO,
            radius,
            color,
            hp: radius,
            moving: true,
            invincible: false,
        }
    }

    fn mass(self) -> f32 {
        self.radius * self.radius
    }
}

pub struct PizzaState {
    pub config: PizzaConfig,
    pub balls: Vec<BallState>,
    pub tick: u64,
    pub held_ball_id: Option<u64>,
    pub pointer_position: Vec2,
    pub cursor_velocity: Vec2,
    pub last_step_metrics: PizzaStepMetrics,
    seed: u64,
    next_ball_id: u64,
    rng: StdRng,
    physics: Option<PhysicsWorld>,
    gravity_solver: GravitySolver,
    gravity_participants: Vec<GravityParticipant>,
}

impl PizzaState {
    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
        // Benchmark walls and fixture dimensions are part of the workload.
        // Resizing the presentation must not silently change the physics case.
        if self.config.benchmark.is_some() {
            return;
        }
        self.config.bounds = PizzaBounds::from_aspect_ratio(aspect_ratio);
        for ball in &mut self.balls {
            contain_ball(ball, self.config.bounds);
        }
        self.rebuild_physics();
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    fn allocate_ball(&mut self, position: Vec2, radius: f32, color: Color) -> BallState {
        let id = self.next_ball_id;
        self.next_ball_id += 1;
        BallState::new(id, position, radius, color)
    }

    fn random_ball(&mut self) -> BallState {
        let bounds = self.config.bounds;
        let max_radius = MAX_RANDOM_BALL_RADIUS
            .min(bounds.width * 0.5)
            .min(bounds.height * 0.5);
        let min_radius = MIN_RANDOM_BALL_RADIUS.min(max_radius);
        let radius = self.rng.random_range(min_radius..=max_radius);
        let x = random_position_component(&mut self.rng, radius, bounds.width);
        let y = random_position_component(&mut self.rng, radius, bounds.height);
        let color = random_color(&mut self.rng);
        self.allocate_ball(Vec2::new(x, y), radius, color)
    }

    fn initialize_benchmark(&mut self) {
        let Some(benchmark) = self.config.benchmark else {
            return;
        };

        self.balls.reserve(benchmark.ball_count);
        for _ in 0..benchmark.ball_count {
            let ball = self.random_benchmark_ball(benchmark);
            self.insert_benchmark_ball(ball);
        }
    }

    fn random_benchmark_ball(&mut self, benchmark: PizzaBenchmarkConfig) -> BallState {
        let bounds = self.config.bounds;
        let count = benchmark.ball_count.max(1) as f32;
        let ideal_radius = ((bounds.width * bounds.height) / (std::f32::consts::PI * count)).sqrt();
        let radius_factor = match benchmark.workload {
            PizzaBenchmarkWorkload::Sparse => 0.22,
            PizzaBenchmarkWorkload::Dense | PizzaBenchmarkWorkload::Churn => 0.72,
        };
        let radius = (ideal_radius * radius_factor * self.rng.random_range(0.55..=1.45))
            .clamp(MIN_FRAGMENT_RADIUS, bounds.width.min(bounds.height) * 0.1);
        let (minimum_x, maximum_x, minimum_y, maximum_y) = match benchmark.workload {
            PizzaBenchmarkWorkload::Sparse => (
                radius,
                (bounds.width - radius).max(radius),
                radius,
                (bounds.height - radius).max(radius),
            ),
            PizzaBenchmarkWorkload::Dense | PizzaBenchmarkWorkload::Churn => (
                bounds.width * 0.12 + radius,
                (bounds.width * 0.88 - radius).max(bounds.width * 0.12 + radius),
                bounds.height * 0.34 + radius,
                (bounds.height * 0.94 - radius).max(bounds.height * 0.34 + radius),
            ),
        };
        let position = Vec2::new(
            self.rng.random_range(minimum_x..=maximum_x),
            self.rng.random_range(minimum_y..=maximum_y),
        );
        let angle = self.rng.random::<f32>() * std::f32::consts::TAU;
        let speed = match benchmark.workload {
            PizzaBenchmarkWorkload::Sparse => self.rng.random_range(0.02..=0.10),
            PizzaBenchmarkWorkload::Dense | PizzaBenchmarkWorkload::Churn => {
                self.rng.random_range(0.0..=0.035)
            }
        };
        let color = random_color(&mut self.rng);
        let mut ball = self.allocate_ball(position, radius, color);
        ball.velocity = Vec2::new(angle.cos(), angle.sin()) * speed;
        // Controlled benchmark churn owns lifecycle. Collision damage must not
        // make Classic and Rapier receive different population schedules.
        ball.hp = f32::INFINITY;
        ball
    }

    fn insert_benchmark_ball(&mut self, ball: BallState) {
        self.push_ball(ball);
    }

    fn advance_benchmark_churn(&mut self) -> PizzaStepMetrics {
        let Some(benchmark) = self.config.benchmark else {
            return PizzaStepMetrics::default();
        };
        if benchmark.workload != PizzaBenchmarkWorkload::Churn
            || self.tick == 0
            || !self.tick.is_multiple_of(BENCHMARK_CHURN_INTERVAL_TICKS)
            || self.balls.is_empty()
        {
            return PizzaStepMetrics::default();
        }

        let started = Instant::now();
        let removed =
            (self.balls.len() * BENCHMARK_CHURN_NUMERATOR / BENCHMARK_CHURN_DENOMINATOR).max(1);
        for _ in 0..removed {
            let index = self.rng.random_range(0..self.balls.len());
            self.swap_remove_ball(index);
        }
        for _ in 0..removed {
            let ball = self.random_benchmark_ball(benchmark);
            self.insert_benchmark_ball(ball);
        }

        PizzaStepMetrics {
            lifecycle_time: started.elapsed(),
            added: removed,
            removed,
            ..PizzaStepMetrics::default()
        }
    }

    fn sync_from_rapier(&mut self) {
        let Some(physics) = &self.physics else {
            return;
        };
        let mut motions = physics
            .motions()
            .filter(|record| record.id.role == BALL_BODY_ROLE);
        for ball in &mut self.balls {
            let motion = motions
                .next()
                .expect("Pizza metadata and physics bodies stay aligned");
            assert_eq!(ball.id, motion.id.entity.value());
            ball.position = motion.motion.position;
            ball.velocity = motion.motion.linear_velocity;
        }
        assert!(motions.next().is_none());
    }

    pub fn benchmark_config(&self) -> Option<PizzaBenchmarkConfig> {
        self.config.benchmark
    }

    pub fn benchmark_counts(&self) -> PizzaBenchmarkCounts {
        if self.config.benchmark.is_none() {
            return PizzaBenchmarkCounts::default();
        }
        let rapier = self.last_step_metrics.rapier;
        PizzaBenchmarkCounts {
            balls: self.balls.len(),
            active_bodies: if self.physics.is_some() {
                rapier.active_bodies
            } else {
                self.balls.len()
            },
            sleeping_bodies: rapier.sleeping_bodies,
            candidate_pairs: rapier.candidate_pairs,
            contact_pairs: rapier.contact_pairs,
            contacts: rapier.contacts,
            added: self.last_step_metrics.added,
            removed: self.last_step_metrics.removed,
        }
    }

    fn uses_rapier(&self) -> bool {
        self.config
            .benchmark
            .is_none_or(|benchmark| benchmark.backend == PizzaPhysicsBackend::Rapier)
    }

    fn rebuild_physics(&mut self) {
        if !self.uses_rapier() {
            self.physics = None;
            return;
        }
        let mut physics = create_physics_world(&self.config);
        for ball in &self.balls {
            assert!(insert_physics_ball(
                &mut physics,
                *ball,
                self.held_ball_id == Some(ball.id),
            ));
        }
        self.physics = Some(physics);
    }

    fn push_ball(&mut self, ball: BallState) {
        if let Some(physics) = &mut self.physics {
            assert!(
                insert_physics_ball(physics, ball, self.held_ball_id == Some(ball.id)),
                "Pizza generated an invalid or duplicate physics ball"
            );
        }
        self.balls.push(ball);
    }

    fn swap_remove_ball(&mut self, index: usize) -> Option<BallState> {
        if index >= self.balls.len() {
            return None;
        }
        let id = self.balls[index].id;
        if let Some(physics) = &mut self.physics {
            assert!(
                physics.remove_entity(PhysicsId::new(id)),
                "Pizza metadata and physics lifecycle stay aligned"
            );
        }
        Some(self.balls.swap_remove(index))
    }

    fn advance_spawner(&mut self) {
        if self.balls.len() >= self.config.desired_ball_count
            || self.balls.len() >= MAX_BALLS
            || self.rng.random::<f32>() >= self.config.ball_spawn_rate
        {
            return;
        }
        let ball = self.random_ball();
        self.push_ball(ball);
    }

    fn apply_pointer_action(&mut self, action: PointerAction) {
        self.pointer_position = Vec2::new(action.position.x, action.position.y);
        match action.phase {
            PointerPhase::Press => self.press_pointer(),
            PointerPhase::Drag => {}
            PointerPhase::Release => self.release_pointer(),
            PointerPhase::Cancel => self.cancel_pointer(),
        }
    }

    fn press_pointer(&mut self) {
        if self.held_ball_id.is_some() {
            return;
        }

        let selected = self
            .balls
            .iter()
            .find(|ball| ball.position.distance_to(self.pointer_position) <= ball.radius)
            .map(|ball| ball.id);
        let id = if let Some(id) = selected {
            id
        } else {
            if self.balls.len() >= MAX_BALLS {
                self.remove_replaceable_ball();
            }
            let max_radius = MAX_RANDOM_BALL_RADIUS
                .min(self.config.bounds.width * 0.5)
                .min(self.config.bounds.height * 0.5);
            let min_radius = MIN_RANDOM_BALL_RADIUS.min(max_radius);
            let radius = self.rng.random_range(min_radius..=max_radius);
            let color = random_color(&mut self.rng);
            let ball = self.allocate_ball(self.pointer_position, radius, color);
            let id = ball.id;
            self.push_ball(ball);
            id
        };

        self.held_ball_id = Some(id);
        if let Some(ball) = self.ball_mut(id) {
            ball.moving = false;
            ball.invincible = true;
        }
        if let Some(physics) = &mut self.physics {
            let body = ball_body_id(id);
            assert!(physics.set_body_kind(body, BodyKind::KinematicPosition, true));
            assert!(physics.set_velocity(body, Vec2::ZERO, 0.0, true));
        }
    }

    fn release_pointer(&mut self) {
        let Some(id) = self.held_ball_id.take() else {
            return;
        };
        let cursor_velocity = self.cursor_velocity;
        if let Some(ball) = self.ball_mut(id) {
            ball.hp = ball.radius;
            ball.velocity = cursor_velocity;
            ball.moving = true;
            ball.invincible = false;
        }
        if let Some(physics) = &mut self.physics {
            let body = ball_body_id(id);
            assert!(physics.set_body_kind(body, BodyKind::Dynamic, true));
            assert!(physics.set_velocity(body, cursor_velocity, 0.0, true));
        }
        self.cursor_velocity = Vec2::ZERO;
    }

    fn cancel_pointer(&mut self) {
        let Some(id) = self.held_ball_id.take() else {
            return;
        };
        if let Some(ball) = self.ball_mut(id) {
            ball.hp = ball.radius;
            ball.velocity = Vec2::ZERO;
            ball.moving = true;
            ball.invincible = false;
        }
        if let Some(physics) = &mut self.physics {
            let body = ball_body_id(id);
            assert!(physics.set_body_kind(body, BodyKind::Dynamic, true));
            assert!(physics.set_velocity(body, Vec2::ZERO, 0.0, true));
        }
        self.cursor_velocity = Vec2::ZERO;
    }

    fn advance_held_ball(&mut self, dt: f32) {
        let Some(id) = self.held_ball_id else {
            return;
        };
        let pointer_position = self.pointer_position;
        let Some(ball_index) = self.balls.iter().position(|ball| ball.id == id) else {
            self.held_ball_id = None;
            return;
        };
        let ball = self.balls[ball_index];
        let displacement = pointer_position - ball.position;
        let distance = displacement.length();
        let direction = if distance > 0.0 {
            displacement / distance
        } else {
            Vec2::ZERO
        };
        let target_velocity = direction * (CURSOR_FORCE / ball.mass()) * distance.min(ball.radius);
        self.cursor_velocity =
            self.cursor_velocity * CURSOR_SMOOTHING + target_velocity * (1.0 - CURSOR_SMOOTHING);
        let ball = &mut self.balls[ball_index];
        ball.velocity = self.cursor_velocity;
        ball.position += self.cursor_velocity * (dt * CURSOR_TIME_SCALE);
        contain_ball(ball, self.config.bounds);
        if let Some(physics) = &mut self.physics {
            assert!(physics.set_next_kinematic_pose(ball_body_id(id), ball.position, 0.0,));
        }
    }

    fn ball_mut(&mut self, id: u64) -> Option<&mut BallState> {
        self.balls.iter_mut().find(|ball| ball.id == id)
    }

    fn remove_replaceable_ball(&mut self) {
        let held = self.held_ball_id;
        let candidates = self
            .balls
            .iter()
            .enumerate()
            .filter(|(_, ball)| Some(ball.id) != held)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if !candidates.is_empty() {
            let candidate = self.rng.random_range(0..candidates.len());
            self.swap_remove_ball(candidates[candidate]);
        }
    }
}

fn create_physics_world(config: &PizzaConfig) -> PhysicsWorld {
    let mut physics = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        length_unit: config.bounds.width.min(config.bounds.height).max(0.001),
        solver_iterations: 4,
        internal_stabilization_iterations: 1,
        max_ccd_substeps: 1,
        collect_events: config.benchmark.is_none(),
    });
    physics.reserve(
        config.desired_ball_count + 1,
        config.desired_ball_count + 4,
        0,
    );

    let thickness = (config.bounds.width.min(config.bounds.height) * 0.02).max(0.001);
    let half_width = config.bounds.width * 0.5;
    let half_height = config.bounds.height * 0.5;
    let wall_geometry = [
        (
            Vec2::new(half_width, -thickness * 0.5),
            half_width + thickness,
            thickness * 0.5,
        ),
        (
            Vec2::new(half_width, config.bounds.height + thickness * 0.5),
            half_width + thickness,
            thickness * 0.5,
        ),
        (
            Vec2::new(-thickness * 0.5, half_height),
            thickness * 0.5,
            half_height + thickness,
        ),
        (
            Vec2::new(config.bounds.width + thickness * 0.5, half_height),
            thickness * 0.5,
            half_height + thickness,
        ),
    ];
    let walls = wall_geometry
        .into_iter()
        .enumerate()
        .map(|(index, (position, wall_half_width, wall_half_height))| {
            let mut collider = ColliderSpec::cuboid(
                ColliderId::new(
                    WALL_ENTITY,
                    WALL_COLLIDER_ROLE,
                    u16::try_from(index).expect("four walls fit in u16"),
                ),
                wall_half_width,
                wall_half_height,
            );
            collider.local_position = position;
            collider.friction = 0.4;
            collider.restitution = WALL_ELASTICITY;
            collider
        })
        .collect::<Vec<_>>();
    assert!(physics.insert_body(
        WALL_BODY,
        BodySpec {
            kind: BodyKind::Fixed,
            ..BodySpec::default()
        },
        &walls,
    ));
    physics
}

fn ball_body_id(id: u64) -> BodyId {
    BodyId::new(PhysicsId::new(id), BALL_BODY_ROLE)
}

fn ball_collider_id(id: u64) -> ColliderId {
    ColliderId::new(PhysicsId::new(id), BALL_COLLIDER_ROLE, 0)
}

fn insert_physics_ball(physics: &mut PhysicsWorld, ball: BallState, held: bool) -> bool {
    let mut collider = ColliderSpec::ball(ball_collider_id(ball.id), ball.radius);
    collider.density = 1.0;
    collider.friction = 0.3;
    collider.restitution = COLLISION_ELASTICITY;
    physics.insert_body(
        ball_body_id(ball.id),
        BodySpec {
            kind: if held {
                BodyKind::KinematicPosition
            } else {
                BodyKind::Dynamic
            },
            position: ball.position,
            linear_velocity: ball.velocity,
            can_sleep: false,
            ..BodySpec::default()
        },
        &[collider],
    )
}

fn pizza_gravity_config(model: PizzaGravityModel) -> GravityConfig {
    GravityConfig {
        backend: model.backend(),
        softening: GRAVITY_SOFTENING,
        interaction_scale: GRAVITY,
    }
}

fn collect_gravity_participants(participants: &mut Vec<GravityParticipant>, balls: &[BallState]) {
    participants.clear();
    participants.extend(balls.iter().map(|ball| {
        let mut participant =
            GravityParticipant::dynamic(GravityId::new(ball.id), ball.position, ball.mass());
        if !ball.moving {
            participant.response_scale = 0.0;
        }
        participant
    }));
}

fn apply_gravity_to_physics(state: &mut PizzaState) -> GravityStepMetrics {
    let PizzaState {
        config,
        balls,
        physics,
        gravity_solver,
        gravity_participants,
        ..
    } = state;
    collect_gravity_participants(gravity_participants, balls);
    let outputs = gravity_solver
        .solve(gravity_participants, pizza_gravity_config(config.gravity))
        .expect("normalized Pizza bodies form valid gravity participants");
    let physics = physics
        .as_mut()
        .expect("Rapier Pizza uses canonical physics");
    for (ball, output) in balls.iter().zip(outputs) {
        debug_assert_eq!(output.id, GravityId::new(ball.id));
        if ball.moving {
            assert!(physics.apply_velocity_delta(
                ball_body_id(ball.id),
                output.velocity_delta,
                true,
            ));
        }
    }
    gravity_solver.metrics()
}

fn apply_gravity_to_classic(state: &mut PizzaState) -> GravityStepMetrics {
    let PizzaState {
        config,
        balls,
        gravity_solver,
        gravity_participants,
        ..
    } = state;
    collect_gravity_participants(gravity_participants, balls);
    let outputs = gravity_solver
        .solve(gravity_participants, pizza_gravity_config(config.gravity))
        .expect("normalized Pizza balls form valid gravity participants");
    for (ball, output) in balls.iter_mut().zip(outputs) {
        debug_assert_eq!(output.id, GravityId::new(ball.id));
        ball.velocity += output.velocity_delta;
    }
    gravity_solver.metrics()
}

fn apply_contact_damage(state: &mut PizzaState) {
    let contacts = state
        .physics
        .as_ref()
        .expect("interactive Pizza uses canonical physics")
        .contact_events()
        .to_vec();
    for contact in contacts {
        if contact.collider_a.role != BALL_COLLIDER_ROLE
            || contact.collider_b.role != BALL_COLLIDER_ROLE
        {
            continue;
        }
        apply_contact_damage_to_ball(state, contact.collider_a.entity, contact);
        apply_contact_damage_to_ball(state, contact.collider_b.entity, contact);
    }
}

fn apply_contact_damage_to_ball(state: &mut PizzaState, entity: PhysicsId, contact: ContactEvent) {
    let id = entity.value();
    let body = ball_body_id(id);
    let mass = state
        .physics
        .as_ref()
        .and_then(|physics| physics.body_mass(body))
        .unwrap_or(1.0)
        .max(f32::EPSILON);
    if let Some(ball) = state.ball_mut(id)
        && !ball.invincible
    {
        ball.hp -= contact.impulse_magnitude / mass * DAMAGE_SCALAR;
    }
}

impl Scenario for PizzaScenario {
    type State = PizzaState;
    type Config = PizzaConfig;

    fn init(config: Self::Config, seed: u64) -> Self::State {
        let mut state = PizzaState {
            config: config.normalized(),
            balls: Vec::new(),
            tick: 0,
            held_ball_id: None,
            pointer_position: Vec2::ZERO,
            cursor_velocity: Vec2::ZERO,
            last_step_metrics: PizzaStepMetrics::default(),
            seed,
            next_ball_id: 1,
            rng: StdRng::seed_from_u64(seed),
            physics: None,
            gravity_solver: GravitySolver::new(),
            gravity_participants: Vec::new(),
        };
        state.rebuild_physics();
        state.initialize_benchmark();
        state
    }

    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult {
        if state.config.benchmark.is_none() {
            for action in actions {
                if let Action::Pointer(pointer) = action {
                    state.apply_pointer_action(*pointer);
                }
            }
        }

        if dt.is_zero() {
            return StepResult::default();
        }

        let dt = dt.as_secs_f32();
        if let Some(benchmark) = state.config.benchmark {
            let mut metrics = state.advance_benchmark_churn();
            match benchmark.backend {
                PizzaPhysicsBackend::Classic => {
                    let workload_started = Instant::now();
                    let movement_dt = dt * WORLD_TIME_SCALE;
                    for ball in &mut state.balls {
                        ball.position += ball.velocity * movement_dt;
                        contain_ball(ball, state.config.bounds);
                    }
                    metrics.workload_time = workload_started.elapsed();

                    let gravity_started = Instant::now();
                    metrics.gravity = apply_gravity_to_classic(state);
                    metrics.gravity_time = gravity_started.elapsed();

                    let collision_started = Instant::now();
                    resolve_collisions(&mut state.balls);
                    for ball in &mut state.balls {
                        contain_ball(ball, state.config.bounds);
                    }
                    metrics.collision_time = collision_started.elapsed();
                    metrics.physics_time = metrics.gravity_time + metrics.collision_time;
                }
                PizzaPhysicsBackend::Rapier => {
                    let gravity_started = Instant::now();
                    metrics.gravity = apply_gravity_to_physics(state);
                    metrics.gravity_time = gravity_started.elapsed();

                    let physics = state
                        .physics
                        .as_mut()
                        .expect("Rapier benchmark initializes a Rapier world");
                    metrics.rapier = physics.step(dt);
                    metrics.physics_time = metrics.rapier.wall_time;

                    let snapshot_started = Instant::now();
                    state.sync_from_rapier();
                    metrics.snapshot_time = snapshot_started.elapsed();
                }
            }
            state.last_step_metrics = metrics;
            state.tick += 1;
            return StepResult::default();
        }

        let mut metrics = PizzaStepMetrics::default();
        state.advance_held_ball(dt);
        state.advance_spawner();

        let gravity_started = Instant::now();
        metrics.gravity = apply_gravity_to_physics(state);
        metrics.gravity_time = gravity_started.elapsed();

        let physics = state
            .physics
            .as_mut()
            .expect("interactive Pizza uses the canonical physics world");
        metrics.rapier = physics.step(dt * WORLD_TIME_SCALE);
        metrics.physics_time = metrics.rapier.wall_time;
        metrics.collision_time = metrics.rapier.narrow_phase_time + metrics.rapier.solver_time;

        let snapshot_started = Instant::now();
        state.sync_from_rapier();
        metrics.snapshot_time = snapshot_started.elapsed();
        apply_contact_damage(state);

        let lifecycle_started = Instant::now();
        let balls_before_explosions = state.balls.len();
        explode_dead_balls(state);
        let balls_after_explosions = state.balls.len();
        metrics.lifecycle_time = lifecycle_started.elapsed();
        metrics.added = balls_after_explosions.saturating_sub(balls_before_explosions);
        metrics.removed = balls_before_explosions.saturating_sub(balls_after_explosions);
        state.last_step_metrics = metrics;
        state.tick += 1;
        StepResult::default()
    }

    fn observe(_state: &Self::State) -> Observation {
        Observation {
            payload: Vec::new(),
        }
    }

    fn render_frame(state: &Self::State) -> RenderFrame {
        let bounds = state.config.bounds;
        let mut frame = RenderFrame::new(Camera2::new(
            RenderPoint::new(bounds.center().x, bounds.center().y),
            bounds.height,
        ));
        for ball in &state.balls {
            let color = RenderColor::rgba(ball.color.r, ball.color.g, ball.color.b, ball.color.a);
            let stroke_color = if ball.invincible {
                RenderColor::WHITE
            } else {
                RenderColor::rgba(0.08, 0.08, 0.12, 0.9)
            };
            frame.push_primitive(
                BALL_LAYER,
                RenderPrimitive::Circle(RenderCircle {
                    center: RenderPoint::new(ball.position.x, ball.position.y),
                    radius: ball.radius,
                    fill: Some(engine_common::Fill::new(color)),
                    stroke: Some(Stroke::new(stroke_color, 1.0)),
                }),
            );
        }
        if let Some(benchmark) = state.config.benchmark {
            let counts = state.benchmark_counts();
            let mut text = RenderText::new(
                RenderPoint::new(bounds.width * 0.01, bounds.height * 0.99),
                format!(
                    "Pizza lab | {} + {} | {} | {} balls | {} active | {} contacts",
                    benchmark.backend.label(),
                    benchmark.gravity.label(),
                    benchmark.workload.label(),
                    counts.balls,
                    counts.active_bodies,
                    counts.contacts,
                ),
            );
            text.color = RenderColor::rgb(0.92, 0.95, 1.0);
            text.size = 14.0;
            text.anchor = TextAnchor::TopLeft;
            frame.push_primitive(BENCHMARK_LABEL_LAYER, RenderPrimitive::Text(text));
        }
        frame
    }

    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: 60 }
    }
}

fn random_position_component(rng: &mut StdRng, radius: f32, maximum: f32) -> f32 {
    let available = (maximum - radius * 2.0).max(0.0);
    radius + rng.random::<f32>() * available
}

fn random_color(rng: &mut StdRng) -> Color {
    Color::rgb(rng.random(), rng.random(), rng.random())
}

fn contain_ball(ball: &mut BallState, bounds: PizzaBounds) {
    let max_x = (bounds.width - ball.radius - WALL_FUDGE).max(ball.radius);
    let max_y = (bounds.height - ball.radius - WALL_FUDGE).max(ball.radius);
    if ball.position.x + ball.radius >= bounds.width {
        ball.position.x = max_x;
        ball.velocity.x = -ball.velocity.x * WALL_ELASTICITY;
    }
    if ball.position.y + ball.radius >= bounds.height {
        ball.position.y = max_y;
        ball.velocity.y = -ball.velocity.y * WALL_ELASTICITY;
    }
    if ball.position.x - ball.radius <= 0.0 {
        ball.position.x = ball.radius + WALL_FUDGE;
        ball.velocity.x = -ball.velocity.x * WALL_ELASTICITY;
    }
    if ball.position.y - ball.radius <= 0.0 {
        ball.position.y = ball.radius + WALL_FUDGE;
        ball.velocity.y = -ball.velocity.y * WALL_ELASTICITY;
    }
}

fn resolve_collisions(balls: &mut [BallState]) {
    for left_index in 0..balls.len() {
        for right_index in left_index + 1..balls.len() {
            let (left_slice, right_slice) = balls.split_at_mut(right_index);
            let left = &mut left_slice[left_index];
            let right = &mut right_slice[0];
            if left.position.distance_to(right.position) < left.radius + right.radius {
                collide_balls(left, right);
            }
        }
    }
}

fn collide_balls(left: &mut BallState, right: &mut BallState) {
    let delta_vector = left.position - right.position;
    let delta = delta_vector.length();
    let normal = if delta == 0.0 {
        Vec2::X
    } else {
        delta_vector / delta
    };
    let translation = normal * (left.radius + right.radius - delta);
    let left_mass = left.mass();
    let right_mass = right.mass();
    let total_mass = left_mass + right_mass;

    if !left.moving {
        right.position -= translation;
    } else if !right.moving {
        left.position += translation;
    } else {
        left.position += translation * (right_mass / total_mass);
        right.position -= translation * (left_mass / total_mass);
    }
    if !left.moving && !right.moving {
        return;
    }

    let tangent = Vec2::new(normal.y, -normal.x);
    let left_normal = normal * left.velocity.dot(normal);
    let left_tangent = tangent * left.velocity.dot(tangent);
    let right_normal = normal * right.velocity.dot(normal);
    let right_tangent = tangent * right.velocity.dot(tangent);
    let left_delta_scale = (right_mass - left_mass) / total_mass * left_normal.length()
        + 2.0 * right_mass / total_mass * right_normal.length();
    let right_delta_scale = (left_mass - right_mass) / total_mass * right_normal.length()
        + 2.0 * left_mass / total_mass * left_normal.length();
    let raw_left_delta = normal * left_delta_scale;
    let raw_right_delta = normal * right_delta_scale;
    let left_delta = raw_left_delta * COLLISION_ELASTICITY;
    let right_delta = raw_right_delta * COLLISION_ELASTICITY;

    if left.moving {
        left.velocity = left_tangent + left_delta;
    }
    if right.moving {
        right.velocity = right_tangent - right_delta;
    }
    if !left.invincible {
        left.hp -= if left.moving {
            left_delta.length()
        } else {
            raw_left_delta.length()
        } * DAMAGE_SCALAR;
    }
    if !right.invincible {
        right.hp -= if right.moving {
            right_delta.length()
        } else {
            raw_right_delta.length()
        } * DAMAGE_SCALAR;
    }
}

fn explode_dead_balls(state: &mut PizzaState) {
    let dead_indices = state
        .balls
        .iter()
        .enumerate()
        .filter(|(_, ball)| !ball.invincible && ball.hp <= 0.0)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut dead = Vec::with_capacity(dead_indices.len());
    for index in dead_indices.into_iter().rev() {
        dead.push(
            state
                .swap_remove_ball(index)
                .expect("dead Pizza index remains valid"),
        );
    }
    dead.reverse();

    for parent in dead {
        if state.balls.len() >= MAX_BALLS {
            break;
        }
        let division_size = parent.radius / EXPLOSION_DIVISIONS as f32;
        for y_index in 0..EXPLOSION_DIVISIONS * 2 {
            for x_index in 0..EXPLOSION_DIVISIONS * 2 {
                let offset = Vec2::new(
                    -parent.radius + x_index as f32 * division_size,
                    -parent.radius + y_index as f32 * division_size,
                );
                if offset.length() > parent.radius {
                    continue;
                }
                let radius =
                    division_size * EXPLOSION_SIZE_FACTOR * state.rng.random_range(0.1..=1.0);
                if radius < MIN_FRAGMENT_RADIUS || radius < MIN_BALL_RADIUS {
                    continue;
                }
                let velocity_scale = state.rng.random::<f32>() * EXPLOSION_VELOCITY_FACTOR;
                let velocity =
                    (offset + parent.velocity * EXPLOSION_PARENT_VELOCITY_FACTOR) * velocity_scale;
                let color = parent.color.random_variation(100.0 / 255.0, &mut state.rng);
                let mut fragment = state.allocate_ball(parent.position + offset, radius, color);
                fragment.velocity = velocity;
                contain_ball(&mut fragment, state.config.bounds);
                state.push_ball(fragment);
                if state.balls.len() >= MAX_BALLS {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED_DT: Duration = Duration::from_nanos(16_666_667);

    fn step(state: &mut PizzaState, actions: &[Action]) {
        PizzaScenario::step(state, actions, FIXED_DT);
    }

    fn pointer(x: f32, y: f32, phase: PointerPhase) -> Action {
        Action::Pointer(PointerAction {
            position: RenderPoint::new(x, y),
            phase,
        })
    }

    #[test]
    fn seeded_spawner_is_deterministic_and_capped() {
        let config = PizzaConfig {
            desired_ball_count: 8,
            ball_spawn_rate: MAX_SPAWN_RATE,
            ..PizzaConfig::default()
        };
        let mut left = PizzaScenario::init(config.clone(), 42);
        let mut right = PizzaScenario::init(config, 42);
        for _ in 0..120 {
            step(&mut left, &[]);
            step(&mut right, &[]);
            assert!(left.balls.len() <= 8);
            assert_eq!(left.balls, right.balls);
        }
        assert_eq!(left.balls.len(), 8);
    }

    #[test]
    fn pointer_press_creates_holds_and_releases_a_ball() {
        let mut state = PizzaScenario::init(
            PizzaConfig {
                desired_ball_count: 0,
                ..PizzaConfig::default()
            },
            7,
        );
        step(&mut state, &[pointer(0.5, 0.25, PointerPhase::Press)]);
        assert_eq!(state.balls.len(), 1);
        assert!(state.balls[0].invincible);
        assert!(!state.balls[0].moving);

        step(&mut state, &[pointer(0.7, 0.25, PointerPhase::Drag)]);
        assert!(state.balls[0].velocity.x > 0.0);
        step(&mut state, &[pointer(0.7, 0.25, PointerPhase::Release)]);
        assert!(!state.balls[0].invincible);
        assert!(state.balls[0].moving);
        assert!(state.balls[0].velocity.x > 0.0);
    }

    #[test]
    fn pointer_cancel_releases_without_throwing_the_ball() {
        let mut state = PizzaScenario::init(
            PizzaConfig {
                desired_ball_count: 0,
                ..PizzaConfig::default()
            },
            7,
        );
        step(&mut state, &[pointer(0.5, 0.25, PointerPhase::Press)]);
        step(&mut state, &[pointer(0.7, 0.25, PointerPhase::Drag)]);
        assert!(state.balls[0].velocity.x > 0.0);
        let tick = state.tick;

        PizzaScenario::step(
            &mut state,
            &[pointer(0.7, 0.25, PointerPhase::Cancel)],
            Duration::ZERO,
        );

        assert_eq!(state.tick, tick);
        assert!(state.held_ball_id.is_none());
        assert!(!state.balls[0].invincible);
        assert!(state.balls[0].moving);
        assert_eq!(state.balls[0].velocity, Vec2::ZERO);
    }

    #[test]
    fn collision_separates_by_mass_and_applies_damage() {
        let mut left = BallState::new(1, Vec2::new(0.4, 0.3), 0.08, Color::RED);
        let mut right = BallState::new(2, Vec2::new(0.5, 0.3), 0.04, Color::BLUE);
        left.velocity = Vec2::new(0.03, 0.0);
        right.velocity = Vec2::new(-0.01, 0.0);
        collide_balls(&mut left, &mut right);
        assert!(left.position.distance_to(right.position) >= left.radius + right.radius - 1.0e-6);
        assert!(left.hp < left.radius);
        assert!(right.hp < right.radius);
    }

    #[test]
    fn interactive_collisions_use_canonical_rapier_motion_and_damage() {
        let mut state = PizzaScenario::init(
            PizzaConfig {
                desired_ball_count: 0,
                ..PizzaConfig::default()
            },
            11,
        );
        let mut left = state.allocate_ball(Vec2::new(0.40, 0.30), 0.08, Color::RED);
        let mut right = state.allocate_ball(Vec2::new(0.50, 0.30), 0.04, Color::BLUE);
        left.velocity = Vec2::new(0.03, 0.0);
        right.velocity = Vec2::new(-0.01, 0.0);
        state.push_ball(left);
        state.push_ball(right);

        for _ in 0..3 {
            step(&mut state, &[]);
        }

        assert!(state.balls[0].hp < state.balls[0].radius);
        assert!(state.balls[1].hp < state.balls[1].radius);
        assert!(state.balls[0].position.distance_to(state.balls[1].position) > 0.10);
        assert_eq!(
            state.physics.as_ref().unwrap().body_count(),
            state.balls.len() + 1
        );
    }

    #[test]
    fn dead_ball_explodes_into_capped_fragments() {
        let mut state = PizzaScenario::init(
            PizzaConfig {
                desired_ball_count: 0,
                ..PizzaConfig::default()
            },
            3,
        );
        let mut ball = state.allocate_ball(Vec2::new(0.5, 0.25), 0.08, Color::GREEN);
        ball.hp = 0.0;
        state.push_ball(ball);
        explode_dead_balls(&mut state);
        assert!(!state.balls.is_empty());
        assert!(state.balls.len() <= MAX_BALLS);
        assert!(state.balls.iter().all(|fragment| fragment.radius < 0.08));
        assert_eq!(
            state.physics.as_ref().unwrap().body_count(),
            state.balls.len() + 1
        );
    }

    #[test]
    fn camera_tracks_normalized_bounds_for_window_aspect() {
        let mut state = PizzaScenario::init(PizzaConfig::default(), 0);
        state.set_aspect_ratio(2.0);
        let frame = PizzaScenario::render_frame(&state);
        assert_eq!(frame.camera.center, RenderPoint::new(0.5, 0.25));
        assert_eq!(frame.camera.height, 0.5);
        assert_eq!(frame.camera.visible_width(2.0), 1.0);
        assert_eq!(
            state.physics.as_ref().unwrap().body_count(),
            state.balls.len() + 1
        );
    }

    #[test]
    fn benchmark_fixture_is_immediate_and_deterministic() {
        let benchmark = PizzaBenchmarkConfig {
            backend: PizzaPhysicsBackend::Classic,
            gravity: PizzaGravityModel::Fast,
            workload: PizzaBenchmarkWorkload::Dense,
            ball_count: 300,
        };
        let config = PizzaConfig::benchmark(benchmark, PizzaBounds::default());
        let left = PizzaScenario::init(config.clone(), 91);
        let replay = PizzaScenario::init(config, 91);
        let rapier = PizzaScenario::init(
            PizzaConfig::benchmark(
                PizzaBenchmarkConfig {
                    backend: PizzaPhysicsBackend::Rapier,
                    ..benchmark
                },
                PizzaBounds::default(),
            ),
            91,
        );

        assert_eq!(left.balls.len(), 300);
        assert_eq!(left.balls, replay.balls);
        assert_eq!(left.balls, rapier.balls);
        assert!(left.balls.iter().all(|ball| ball.hp.is_infinite()));
    }

    #[test]
    fn benchmark_can_compare_exact_and_barnes_hut_gravity() {
        let base = PizzaBenchmarkConfig {
            backend: PizzaPhysicsBackend::Classic,
            gravity: PizzaGravityModel::Exact,
            workload: PizzaBenchmarkWorkload::Sparse,
            ball_count: 300,
        };
        let mut exact =
            PizzaScenario::init(PizzaConfig::benchmark(base, PizzaBounds::default()), 0xA11A);
        let mut approximate = PizzaScenario::init(
            PizzaConfig::benchmark(
                PizzaBenchmarkConfig {
                    gravity: PizzaGravityModel::Fast,
                    ..base
                },
                PizzaBounds::default(),
            ),
            0xA11A,
        );

        let exact_metrics = apply_gravity_to_classic(&mut exact);
        let approximate_metrics = apply_gravity_to_classic(&mut approximate);

        assert_eq!(exact_metrics.approximations, 0);
        assert_eq!(exact_metrics.exact_interactions, 300 * 299 / 2);
        assert!(approximate_metrics.approximations > 0);
        assert!(approximate_metrics.applied_sources < exact_metrics.applied_sources);
        assert!(
            approximate
                .balls
                .iter()
                .all(|ball| ball.velocity.x.is_finite() && ball.velocity.y.is_finite())
        );
    }

    #[test]
    fn rapier_benchmark_steps_and_reports_contacts() {
        let benchmark = PizzaBenchmarkConfig {
            backend: PizzaPhysicsBackend::Rapier,
            gravity: PizzaGravityModel::Fast,
            workload: PizzaBenchmarkWorkload::Dense,
            ball_count: 64,
        };
        let mut state = PizzaScenario::init(
            PizzaConfig::benchmark(benchmark, PizzaBounds::default()),
            17,
        );

        let mut maximum_contact_pairs = 0;
        for _ in 0..5 {
            step(&mut state, &[]);
            maximum_contact_pairs =
                maximum_contact_pairs.max(state.benchmark_counts().contact_pairs);
        }

        let counts = state.benchmark_counts();
        assert_eq!(counts.balls, 64);
        assert_eq!(counts.active_bodies, 64);
        assert!(maximum_contact_pairs > 0);
        assert!(
            state
                .balls
                .iter()
                .all(|ball| ball.position.x.is_finite() && ball.position.y.is_finite())
        );
    }

    #[test]
    fn controlled_churn_replaces_three_quarters_without_changing_population() {
        let benchmark = PizzaBenchmarkConfig {
            backend: PizzaPhysicsBackend::Rapier,
            gravity: PizzaGravityModel::Fast,
            workload: PizzaBenchmarkWorkload::Churn,
            ball_count: 40,
        };
        let mut state = PizzaScenario::init(
            PizzaConfig::benchmark(benchmark, PizzaBounds::default()),
            19,
        );
        let initial_ids = state.balls.iter().map(|ball| ball.id).collect::<Vec<_>>();
        for _ in 0..=BENCHMARK_CHURN_INTERVAL_TICKS {
            step(&mut state, &[]);
        }

        assert_eq!(state.balls.len(), 40);
        assert_eq!(state.last_step_metrics.removed, 30);
        assert_eq!(state.last_step_metrics.added, 30);
        assert!(
            state
                .balls
                .iter()
                .any(|ball| !initial_ids.contains(&ball.id))
        );
    }

    #[test]
    fn benchmark_frame_identifies_the_visible_workload() {
        let benchmark = PizzaBenchmarkConfig {
            backend: PizzaPhysicsBackend::Rapier,
            gravity: PizzaGravityModel::Fast,
            workload: PizzaBenchmarkWorkload::Dense,
            ball_count: 12,
        };
        let state =
            PizzaScenario::init(PizzaConfig::benchmark(benchmark, PizzaBounds::default()), 5);
        let frame = PizzaScenario::render_frame(&state);
        assert!(
            frame
                .layers
                .iter()
                .flat_map(|layer| &layer.primitives)
                .any(|primitive| matches!(
                    primitive,
                    RenderPrimitive::Text(text)
                        if text
                            .text
                            .contains("rapier + barnes-hut-fast | dense | 12 balls")
                ))
        );
    }
}
