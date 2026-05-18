//! Gameplay constants recovered from `reference/src-decompiled/Common.java`.

pub const PLAYER_GRAVITY: bool = true;
pub const BENCHMARK_FRAMES: u32 = 750;
pub const MIN_VIEW_SIZE: f64 = 15.0;
pub const STARTUP_ZOOM_FACTOR: u32 = 800;
pub const PARTICLE_GRAV_SKIP_LEVEL: u64 = 3;
pub const ASTEROID_GRAV_SKIP_LEVEL: u64 = 7;

pub const LASER_DAMAGE: f32 = 10.0;
pub const LASER_VELOCITY: f32 = 50.0;
pub const LASER_MASS: f32 = 0.5;

pub const PARTICLE_MASS: f32 = 0.1;
pub const DEFAULT_PARTICLE_FADE_RATE: f32 = 0.196_078_43;

pub const CANNON_DAMAGE: f32 = 0.1;
pub const CANNON_COOL_DOWN_TIME: f32 = 0.5;
pub const CANNON_VELOCITY: f32 = 300.0;
pub const SHELL_LIFE: f32 = 1.0;

pub const ASTEROID_DAMAGE: f32 = 0.01;
pub const DEFAULT_ASTEROID_SIZE: f32 = 5.0;
pub const PROB_HUGE_ASTEROID: f32 = 0.98;

pub const PLAYER1: usize = 0;
pub const PLAYER2: usize = 1;
pub const PLAYER_COUNT: usize = 2;

pub const REALLY_SMALL: f32 = 1.0e-7;
pub const PI: f32 = core::f32::consts::PI;

pub const PLANET_MASS_SCALAR: f32 = 750.0;
pub const GRAVITY: f32 = 0.005;
pub const PLANET_DAMAGE_SCALAR: f32 = 0.01;
pub const COLLISION_TRANSLATION_SCALAR: f32 = 1.0;
pub const DEFAULT_ELASTICITY: f32 = 0.9;
pub const PLANET_ELASTICITY: f32 = 0.5;

pub const PARTICLE_VELOCITY_SCALAR: f32 = 20.0;
pub const FLACK_SCALAR: f32 = 1.0;
pub const EXHAUST_MASS: f32 = 0.5;

pub const Z_BGSTARS: f32 = 0.9;
pub const Z_LASER: f32 = 0.8;
pub const Z_PLANETS: f32 = 0.7;
pub const Z_PARTICLES: f32 = -0.9;
pub const Z_SHIP: f32 = -0.8;
