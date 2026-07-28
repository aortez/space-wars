//! Deterministic Rust port of the allan.pizza ball simulation.
//!
//! This first pass intentionally uses exact O(n²) collision and gravity loops.
//! The reference quadtree and Barnes-Hut optimizations remain a later,
//! measurable replacement for these correctness-oriented paths.

use std::time::{Duration, Instant};

use engine_common::{
    Action, Camera2, Observation, PointerAction, PointerPhase, RenderCircle, RenderColor,
    RenderFrame, RenderPoint, RenderPrimitive, RenderText, Scenario, StepResult, Stroke,
    TextAnchor, TickModel,
};
use engine_core::{Color, Vec2};
use engine_rapier::{BallBodySpec, BallPhysics, BallPhysicsBounds, BallPhysicsMetrics};
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
    pub rapier: BallPhysicsMetrics,
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
            benchmark,
        }
    }

    pub fn benchmark(benchmark: PizzaBenchmarkConfig, bounds: PizzaBounds) -> Self {
        let benchmark = benchmark.normalized();
        Self {
            desired_ball_count: benchmark.ball_count,
            ball_spawn_rate: MAX_SPAWN_RATE,
            bounds,
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
    rapier: Option<BallPhysics>,
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
        if benchmark.backend == PizzaPhysicsBackend::Rapier {
            let gravity = match benchmark.workload {
                PizzaBenchmarkWorkload::Sparse => Vec2::ZERO,
                PizzaBenchmarkWorkload::Dense | PizzaBenchmarkWorkload::Churn => {
                    Vec2::new(0.0, -0.18)
                }
            };
            let mut rapier = BallPhysics::new(
                BallPhysicsBounds {
                    width: self.config.bounds.width,
                    height: self.config.bounds.height,
                },
                gravity,
                false,
                4,
            );
            rapier.reserve(benchmark.ball_count);
            self.rapier = Some(rapier);
        }

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
        if let Some(rapier) = &mut self.rapier {
            let inserted = rapier.insert_ball(BallBodySpec {
                id: ball.id,
                position: ball.position,
                velocity: ball.velocity,
                radius: ball.radius,
                density: 1.0,
                restitution: COLLISION_ELASTICITY,
            });
            assert!(
                inserted,
                "benchmark fixture generated an invalid Rapier ball"
            );
        }
        self.balls.push(ball);
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
            let removed_ball = self.balls.swap_remove(index);
            if let Some(rapier) = &mut self.rapier {
                let removed_id = rapier
                    .swap_remove_ball(index)
                    .expect("Pizza and Rapier benchmark vectors stay aligned");
                assert_eq!(removed_id, removed_ball.id);
            }
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
        let Some(rapier) = &self.rapier else {
            return;
        };
        assert_eq!(self.balls.len(), rapier.len());
        for (ball, motion) in self.balls.iter_mut().zip(rapier.motions()) {
            assert_eq!(ball.id, motion.id);
            ball.position = motion.position;
            ball.velocity = motion.velocity;
        }
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
            active_bodies: if self.rapier.is_some() {
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

    fn advance_spawner(&mut self) {
        if self.balls.len() >= self.config.desired_ball_count
            || self.balls.len() >= MAX_BALLS
            || self.rng.random::<f32>() >= self.config.ball_spawn_rate
        {
            return;
        }
        let ball = self.random_ball();
        self.balls.push(ball);
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
            self.balls.push(ball);
            id
        };

        self.held_ball_id = Some(id);
        if let Some(ball) = self.ball_mut(id) {
            ball.moving = false;
            ball.invincible = true;
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
            self.balls.swap_remove(candidates[candidate]);
        }
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
            rapier: None,
        };
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
                    apply_exact_gravity(&mut state.balls);
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
                    let rapier = state
                        .rapier
                        .as_mut()
                        .expect("Rapier benchmark initializes a Rapier world");
                    metrics.rapier = rapier.step(dt);
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

        let workload_started = Instant::now();
        let movement_dt = dt * WORLD_TIME_SCALE;
        for ball in &mut state.balls {
            if ball.moving {
                ball.position += ball.velocity * movement_dt;
            }
            contain_ball(ball, state.config.bounds);
        }
        metrics.workload_time = workload_started.elapsed();

        let gravity_started = Instant::now();
        apply_exact_gravity(&mut state.balls);
        metrics.gravity_time = gravity_started.elapsed();

        let collision_started = Instant::now();
        resolve_collisions(&mut state.balls);
        for ball in &mut state.balls {
            contain_ball(ball, state.config.bounds);
        }
        metrics.collision_time = collision_started.elapsed();

        let lifecycle_started = Instant::now();
        let balls_before_explosions = state.balls.len();
        explode_dead_balls(state);
        let balls_after_explosions = state.balls.len();
        metrics.lifecycle_time = lifecycle_started.elapsed();
        metrics.added = balls_after_explosions.saturating_sub(balls_before_explosions);
        metrics.removed = balls_before_explosions.saturating_sub(balls_after_explosions);
        metrics.physics_time = metrics.gravity_time + metrics.collision_time;
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
                    "Pizza lab | {} | {} | {} balls | {} active | {} contacts",
                    benchmark.backend.label(),
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

fn apply_exact_gravity(balls: &mut [BallState]) {
    for left_index in 0..balls.len() {
        for right_index in left_index + 1..balls.len() {
            let (left_slice, right_slice) = balls.split_at_mut(right_index);
            let left = &mut left_slice[left_index];
            let right = &mut right_slice[0];
            let delta = right.position - left.position;
            let distance_squared =
                (delta.length_squared() + GRAVITY_SOFTENING * GRAVITY_SOFTENING).max(f32::EPSILON);
            let direction = delta.normalized();
            if direction == Vec2::ZERO {
                continue;
            }
            let force = GRAVITY * left.mass() * right.mass() / distance_squared;
            left.velocity += direction * (force / left.mass());
            right.velocity -= direction * (force / right.mass());
        }
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
    let mut dead = Vec::new();
    state.balls.retain(|ball| {
        if !ball.invincible && ball.hp <= 0.0 {
            dead.push(*ball);
            false
        } else {
            true
        }
    });

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
                state.balls.push(fragment);
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
        state.balls.push(ball);
        explode_dead_balls(&mut state);
        assert!(!state.balls.is_empty());
        assert!(state.balls.len() <= MAX_BALLS);
        assert!(state.balls.iter().all(|fragment| fragment.radius < 0.08));
    }

    #[test]
    fn camera_tracks_normalized_bounds_for_window_aspect() {
        let mut state = PizzaScenario::init(PizzaConfig::default(), 0);
        state.set_aspect_ratio(2.0);
        let frame = PizzaScenario::render_frame(&state);
        assert_eq!(frame.camera.center, RenderPoint::new(0.5, 0.25));
        assert_eq!(frame.camera.height, 0.5);
        assert_eq!(frame.camera.visible_width(2.0), 1.0);
    }

    #[test]
    fn benchmark_fixture_is_immediate_and_deterministic() {
        let benchmark = PizzaBenchmarkConfig {
            backend: PizzaPhysicsBackend::Classic,
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
    fn rapier_benchmark_steps_and_reports_contacts() {
        let benchmark = PizzaBenchmarkConfig {
            backend: PizzaPhysicsBackend::Rapier,
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
                    RenderPrimitive::Text(text) if text.text.contains("rapier | dense | 12 balls")
                ))
        );
    }
}
