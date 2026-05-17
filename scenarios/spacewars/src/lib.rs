//! Initial Spacewars scenario port.
//!
//! Current slices cover deterministic sun/planet setup, ship controls, gravity,
//! collision response, damage, debris, asteroids, weapons, exhaust trails, and
//! escape pods. Sounds, scoring, and ownership land in later slices.

use std::time::Duration;

use engine_common::{
    Action, Camera2, Fill, Observation, RenderCircle, RenderColor, RenderFrame, RenderPoint,
    RenderPolygon, RenderPrimitive, RenderText, Scenario, StepResult, Stroke, TextAnchor,
    TickModel,
};
use engine_core::{
    Bounds2, BoundsList, Circle, Color, Line, PlayerConfig, SpacewarsConfig, Transform2, Vec2,
    constants::{
        COLLISION_TRANSLATION_SCALAR, DEFAULT_ELASTICITY, PLANET_DAMAGE_SCALAR, REALLY_SMALL,
    },
    physics::gravity_acceleration_attracted_to,
    rng::{SpacewarsRng, random_range_f32, random_unit_f32, seeded_rng},
    triangle_high_bounds, triangle_low_bound,
};

const STARFIELD_LAYER: i32 = -30;
const WORLD_LAYER: i32 = -20;
const SUN_LAYER: i32 = -15;
const PLANET_LAYER: i32 = -10;
const SPACEPORT_LAYER: i32 = -5;
const EXHAUST_LAYER: i32 = -1;
const SHIP_LAYER: i32 = 0;
const LASER_LAYER: i32 = 2;
const PARTICLE_LAYER: i32 = 3;
const BOUNDS_HIGH_LAYER: i32 = 4;
const BOUNDS_LOW_LAYER: i32 = 5;
const LABEL_LAYER: i32 = 10;

const MAX_PLANETS: usize = 99;
const SUN_RADIUS: f32 = 200.0;
const MIN_PLANET_RADIUS: f32 = 15.0;
const MAX_PLANET_RADIUS: f32 = 150.0;
const MIN_PLANET_SPACING: f32 = 10.0;
const MAX_PLANET_SPACING: f32 = 50.0;
const PLANET_MASS_DENSITY: f32 = 750.0;
const PLANET_ORBIT_PERIOD_SCALAR: f32 = 14.0;
const BODY_BOUNDS_RADIUS_SCALE: f32 = 0.99;
const SPACEPORT_ARC_LENGTH: f32 = 94.24778;
const SPACEPORT_DEPTH_FACTOR: f32 = 0.4;
const SPACEPORT_MAX_ARC_ANGLE: f32 = 2.7488937;
const SPACEPORT_OUTER_POINTS: usize = 15;
const SPACEPORT_INNER_POINTS: usize = 7;
const SPACEPORT_DAMPING: f32 = 0.94;
const SPACEPORT_PULL_SCALE: f32 = 3.0;
const PLAYER_VIEW_HEIGHT: f32 = 320.0;
const DEBRIS_DEATH_SHRINK_FACTOR: f32 = 0.01;
const DEBRIS_DEATH_LIFE_FACTOR: f32 = 0.8;
const DEBRIS_BODY_DAMAGE_SCALAR: f32 = 0.05;
const CANNON_SHELL_SPEED: f32 = 300.0;
const CANNON_RECOIL_SPEED: f32 = 200.0;
const CANNON_COOLDOWN_SECS: f32 = 0.5;
const CANNON_SHELL_DAMAGE_SCALAR: f32 = 0.1;
const CANNON_SHELL_OMEGA: f32 = 2.0;
const CANNON_SHELL_SPAWN_OFFSET: f32 = 5.0;
const CANNON_SHELL_RADIUS: f32 = 2.0;
const LASER_GROWTH_PER_TICK: f32 = 50.0;
const LASER_BASE_DAMAGE: f32 = 10.0;
const ASTEROID_RNG_SALT: u64 = 0xA57E_201D_5EED;
const MAX_ASTEROIDS: usize = 100;
const ASTEROID_SPAWN_RADIUS_FACTOR: f32 = 0.99;
const ASTEROID_MIN_RADIUS: f32 = 2.5;
const ASTEROID_RADIUS_VARIATION: f32 = 5.0;
const ASTEROID_HUGE_CHANCE: f32 = 0.98;
const ASTEROID_HUGE_SIZE_MAX_SCALE: f32 = 10.0;
const ASTEROID_MAX_AIM_ANGLE: f32 = core::f32::consts::FRAC_PI_2;
const ASTEROID_MAX_SPEED: f32 = 200.0;
const ASTEROID_DAMAGE_SCALAR: f32 = 0.01;
const ASTEROID_MAX_OMEGA: f32 = 10.0;
const ASTEROID_GRAVITY_FRAME_MODULUS: u64 = 7;
const ASTEROID_GRAVITY_SCALE: f32 = 7.0;
const STARFIELD_RNG_SALT: u64 = 0x57A2_F13D_5EED_BA5E;
const STARFIELD_DENSITY: f32 = 0.0025;
const STARFIELD_COLOR_ROTATE_RATE: f32 = 0.02;
const STARFIELD_COLOR_ROTATE_RANGE: f32 = 0.2;
const STARFIELD_MAX_STARS: usize = 100_000;
const STARFIELD_POINTS: usize = 3;
const EXHAUST_RNG_SALT: u64 = 0xE7A7_5A11_5EED;
const EXHAUST_DECAY: f32 = 0.1;
const EXHAUST_MOVE_SCALE: f32 = 0.01;
const EXHAUST_LENGTH_SCALE: f32 = 0.025;
const SHIP_TURN_EXHAUST_SCALAR: f32 = 50.0;
const PARTICLE_RNG_SALT: u64 = 0x9A17_1C1E_5EED;
const MAX_PARTICLES: usize = 5_000;
const PARTICLE_FADE_RATE: f32 = 0.5882353;
const PARTICLE_DONE_THRESHOLD: f32 = 0.1;
const PARTICLE_GRAVITY_FRAME_MODULUS: u64 = 3;
const PARTICLE_GRAVITY_SCALE: f32 = 3.0;
const PARTICLE_IMPACT_RANDOM_ANGLE: f32 = 1.5;
const PARTICLE_IMPACT_SPEED_SCALE: f32 = 20.0;
const BREAKUP_RNG_SALT: u64 = 0xB2EA_4A9E_5EED;
const BREAKUP_FRAGMENT_SPEED: f32 = 50.0;
const BREAKUP_FRAGMENT_OMEGA: f32 = 1.0;
const BREAKUP_FRAGMENT_DAMAGE_SCALAR: f32 = 0.0;

const SHIP_THRUST_FORCE: f32 = 50_000.0;
const SHIP_TURN_FORCE: f32 = 200.0;
const SHIP_MASS: f32 = 31.25;
const BASE_MAX_OMEGA: f32 = 1.0;
const MAX_SPEED: f32 = 150.0;
const WING_DELTA_SPEED: f32 = 5.0;
const WING_CLOSED_SPEED: f32 = MAX_SPEED * 5.0;
const WING_CLOSED_MAX_OMEGA: f32 = BASE_MAX_OMEGA * 0.25;
const MAX_WING_THETA: f32 = core::f32::consts::FRAC_PI_4;
const SHIP_LEFT_WING_TRIANGLE_INDEX: usize = 0;
const SHIP_RIGHT_WING_TRIANGLE_INDEX: usize = 1;
const SHIP_THRUSTER_TRIANGLE_INDEX: usize = 3;
const SHIP_BODY_TRIANGLE_INDEX: usize = 4;
const SHIP_LASER_TRIANGLE_INDEX: usize = 5;
const POD_THRUST_FORCE: f32 = 50_000.0;
const POD_TURN_FORCE: f32 = 10.0;
const POD_MASS: f32 = 1.0;
const POD_MAX_SPEED: f32 = 500.0;
const POD_VELOCITY_DAMPING: f32 = 0.8;
const POD_TURN_EXHAUST_SCALAR: f32 = 15.0;
const POD_COCKPIT_RADIUS: f32 = 0.5;

const SHIP_BODY: [Vec2; 3] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(5.0, 0.0),
    Vec2::new(2.5, 7.0),
];
const SHIP_WING_MOUNT: [Vec2; 3] = [
    Vec2::new(2.5, 5.5),
    Vec2::new(0.77, 2.5),
    Vec2::new(4.23, 2.5),
];
const SHIP_THRUSTER: [Vec2; 3] = [
    Vec2::new(0.0, -1.0),
    Vec2::new(2.5, 0.0),
    Vec2::new(5.0, -1.0),
];
const SHIP_LASER: [Vec2; 3] = [
    Vec2::new(2.0, 6.0),
    Vec2::new(2.5, 7.0),
    Vec2::new(3.0, 6.0),
];
const SHIP_LEFT_WING: [Vec2; 3] = [
    Vec2::new(2.5, 2.0),
    Vec2::new(1.0, -0.5),
    Vec2::new(-3.0, 2.0),
];
const SHIP_RIGHT_WING: [Vec2; 3] = [
    Vec2::new(2.5, 2.0),
    Vec2::new(4.0, -0.5),
    Vec2::new(8.0, 2.0),
];
const SHELL_BODY: [Vec2; 3] = [
    Vec2::new(-2.0, 0.0),
    Vec2::new(1.0, -1.7320508),
    Vec2::new(1.0, 1.7320508),
];
const POD_BODY: [Vec2; 3] = [
    Vec2::new(0.0, 1.0),
    Vec2::new(-1.0, 0.0),
    Vec2::new(1.0, 0.0),
];
const POD_THRUSTER: [Vec2; 3] = [
    Vec2::new(0.0, 1.1),
    Vec2::new(-1.0, 0.0),
    Vec2::new(1.0, 0.0),
];
const POD_LASER: [Vec2; 3] = [
    Vec2::new(-0.5, 0.5),
    Vec2::new(0.0, 2.5),
    Vec2::new(0.5, 0.5),
];
const SHIP_PIVOT: Vec2 = Vec2::new(2.5, 3.5);
const SHIP_WING_PIVOT: Vec2 = Vec2::new(2.5, 2.0);
const POD_PIVOT: Vec2 = Vec2::new(0.0, 1.0 / 3.0);
const POD_COCKPIT_CENTER: Vec2 = POD_PIVOT;

pub struct SpacewarsScenario;

#[derive(Debug, Clone)]
pub struct SpacewarsState {
    pub config: SpacewarsConfig,
    pub seed: u64,
    pub tick: u64,
    pub players: [PlayerState; 2],
    pub ships: [ShipState; 2],
    pub debris: Vec<DebrisState>,
    pub sun: Option<SunState>,
    pub planets: Vec<PlanetState>,
    pub starfield: Option<StarFieldState>,
    pub particles: Vec<ParticleState>,
    pub laser_hits: Vec<LaserHit>,
    pub ship_collisions: Vec<ShipCollision>,
    pub ship_debris_collisions: Vec<ShipDebrisCollision>,
    pub debris_collisions: Vec<DebrisCollision>,
    pub debris_body_collisions: Vec<DebrisBodyCollision>,
    pub body_collisions: Vec<BodyCollision>,
    pub spaceport_contacts: Vec<SpaceportContact>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    pub id: usize,
    pub name: String,
    pub health_percent: u32,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SunState {
    pub position: Vec2,
    pub radius: f32,
    pub mass: f32,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanetState {
    pub position: Vec2,
    pub radius: f32,
    pub mass: f32,
    pub color: Color,
    pub orbit_radius: f32,
    pub orbit_angle: f32,
    pub orbit_omega: f32,
    pub wrapper_angle: f32,
    pub wrapper_omega: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StarFieldState {
    pub stars: Vec<StarState>,
    pub base_color: Color,
    pub color_theta: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarState {
    pub points: [Vec2; STARFIELD_POINTS],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebrisState {
    pub kind: DebrisKind,
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
    pub breakup_radius: f32,
    pub fragment_shape: Option<[Vec2; 3]>,
    pub rotation_radians: f32,
    pub omega: f32,
    pub damage_scalar: f32,
    pub life: f32,
    pub life_max: f32,
    pub dead: bool,
    pub fragmented: bool,
    pub color: Color,
    pub owner_id: Option<usize>,
    pub spawn_tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebrisKind {
    Asteroid,
    Fragment,
    Shell,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaserBeamState {
    pub head: Vec2,
    pub tail: Vec2,
    pub direction: Vec2,
}

impl LaserBeamState {
    fn length(self) -> f32 {
        self.head.distance_to(self.tail)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyCollision {
    pub ship: usize,
    pub body: BodyId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyId {
    Sun,
    Planet(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShipCollision {
    pub a: usize,
    pub b: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaserHit {
    pub shooter: usize,
    pub target: LaserTarget,
    pub point: Vec2,
    pub damage: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaserTarget {
    Ship(usize),
    Debris(usize),
    Body(BodyId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShipDebrisCollision {
    pub ship: usize,
    pub debris: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebrisCollision {
    pub a: usize,
    pub b: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebrisBodyCollision {
    pub debris: usize,
    pub body: BodyId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpaceportContact {
    pub ship: usize,
    pub planet: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundsDrawMode {
    High,
    LowAndHigh,
    Low,
}

impl BoundsDrawMode {
    fn show_low(self) -> bool {
        matches!(self, Self::Low | Self::LowAndHigh)
    }

    fn show_high(self) -> bool {
        matches!(self, Self::High | Self::LowAndHigh)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShipState {
    pub owner_id: usize,
    pub form: ShipForm,
    pub position: Vec2,
    pub velocity: Vec2,
    pub rotation_radians: f32,
    pub direction: Vec2,
    pub omega: f32,
    pub color: Color,
    pub wing_theta: f32,
    pub wing_state: WingState,
    pub wing_behavior: WingBehavior,
    pub thrust_behavior: ThrustBehavior,
    pub turn_behavior: TurnBehavior,
    pub laser_firing: bool,
    pub cannon_firing: bool,
    pub laser_beam: Option<LaserBeamState>,
    pub exhaust_trails: Vec<ExhaustTrailState>,
    pub life: f32,
    pub life_max: f32,
    pub dead: bool,
    pub fragmented: bool,
    turn_power: f32,
    thrust_power: f32,
    current_max_omega: f32,
    cannon_cooldown_remaining: f32,
    delta_time: f32,
    death_impulse: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipForm {
    Ship,
    EscapePod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WingState {
    Opened,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WingBehavior {
    None,
    Close,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrustBehavior {
    None,
    Full,
    Brake,
    Reverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnBehavior {
    None,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExhaustTrailState {
    pub start: Vec2,
    pub end: Vec2,
    pub velocity: Vec2,
    pub color: Color,
    pub decay: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleState {
    pub points: [Vec2; 3],
    pub velocity: Vec2,
    pub color: Color,
    pub fade_rate: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SpacewarsActionKind {
    CloseWings = 1,
    OpenWings = 2,
    Thrust = 3,
    ThrustHalt = 4,
    Reverse = 5,
    Brake = 6,
    BrakeHalt = 7,
    TurnLeft = 8,
    TurnRight = 9,
    TurnHalt = 10,
    FireLaser = 11,
    FireLaserHalt = 12,
    FireCannon = 13,
    FireCannonHalt = 14,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpacewarsAction {
    pub player: usize,
    pub kind: SpacewarsActionKind,
}

impl SpacewarsAction {
    pub fn new(player: usize, kind: SpacewarsActionKind) -> Self {
        Self { player, kind }
    }

    pub fn encode(self) -> Action {
        Action {
            kind: self.kind as u32,
            payload: vec![self.player as u8],
        }
    }

    pub fn decode(action: &Action) -> Option<Self> {
        let [player] = action.payload.as_slice() else {
            return None;
        };
        let player = *player as usize;
        if player >= 2 {
            return None;
        }
        Some(Self {
            player,
            kind: SpacewarsActionKind::from_u32(action.kind)?,
        })
    }

    pub fn close_wings(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::CloseWings).encode()
    }

    pub fn open_wings(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::OpenWings).encode()
    }

    pub fn thrust(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::Thrust).encode()
    }

    pub fn thrust_halt(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::ThrustHalt).encode()
    }

    pub fn reverse(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::Reverse).encode()
    }

    pub fn brake(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::Brake).encode()
    }

    pub fn brake_halt(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::BrakeHalt).encode()
    }

    pub fn turn_left(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::TurnLeft).encode()
    }

    pub fn turn_right(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::TurnRight).encode()
    }

    pub fn turn_halt(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::TurnHalt).encode()
    }

    pub fn fire_laser(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::FireLaser).encode()
    }

    pub fn fire_laser_halt(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::FireLaserHalt).encode()
    }

    pub fn fire_cannon(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::FireCannon).encode()
    }

    pub fn fire_cannon_halt(player: usize) -> Action {
        Self::new(player, SpacewarsActionKind::FireCannonHalt).encode()
    }
}

impl SpacewarsActionKind {
    fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::CloseWings),
            2 => Some(Self::OpenWings),
            3 => Some(Self::Thrust),
            4 => Some(Self::ThrustHalt),
            5 => Some(Self::Reverse),
            6 => Some(Self::Brake),
            7 => Some(Self::BrakeHalt),
            8 => Some(Self::TurnLeft),
            9 => Some(Self::TurnRight),
            10 => Some(Self::TurnHalt),
            11 => Some(Self::FireLaser),
            12 => Some(Self::FireLaserHalt),
            13 => Some(Self::FireCannon),
            14 => Some(Self::FireCannonHalt),
            _ => None,
        }
    }
}

impl Scenario for SpacewarsScenario {
    type State = SpacewarsState;
    type Config = SpacewarsConfig;

    fn init(config: Self::Config, seed: u64) -> Self::State {
        let players = [
            player_state(0, &config.players[0]),
            player_state(1, &config.players[1]),
        ];
        let delta_time = config.delta_time();
        let ships = [
            ShipState::new(
                0,
                Vec2::new(375.0, 450.0),
                config.players[0].color,
                config.players[0].health_percent,
                delta_time,
            ),
            ShipState::new(
                1,
                Vec2::new(375.0, 500.0),
                config.players[1].color,
                config.players[1].health_percent,
                delta_time,
            ),
        ];
        let (sun, planets) = build_world(&config, seed);
        let starfield = build_starfield(&config, seed);

        SpacewarsState {
            config,
            seed,
            tick: 0,
            players,
            ships,
            debris: Vec::new(),
            sun,
            planets,
            starfield,
            particles: Vec::new(),
            laser_hits: Vec::new(),
            ship_collisions: Vec::new(),
            ship_debris_collisions: Vec::new(),
            debris_collisions: Vec::new(),
            debris_body_collisions: Vec::new(),
            body_collisions: Vec::new(),
            spaceport_contacts: Vec::new(),
        }
    }

    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult {
        for action in actions.iter().filter_map(SpacewarsAction::decode) {
            state.apply_action(action);
        }

        let dt = dt.as_secs_f32();
        if let Some(sun) = state.sun {
            for planet in &mut state.planets {
                planet.update_orbit(sun.position, dt);
            }
        }

        for ship in &mut state.ships {
            ship.update(dt, state.seed, state.tick);
            contain_ship(ship, state.config.universe_radius as f32);
        }
        let new_shells = spawn_cannon_shells(state, dt);
        state.debris.extend(new_shells);
        update_ship_lasers(state);
        state.laser_hits = resolve_laser_hits(state);
        handle_ship_deaths(state);

        apply_world_gravity(state);
        state.ship_debris_collisions = resolve_ship_debris_collisions(state);
        handle_ship_deaths(state);
        state.ship_collisions = resolve_ship_collisions(state);
        let collision_events = resolve_body_collisions(state);
        state.body_collisions = collision_events.body_collisions;
        state.spaceport_contacts = collision_events.spaceport_contacts;
        handle_ship_deaths(state);
        if state.tick % ASTEROID_GRAVITY_FRAME_MODULUS == 0 {
            apply_debris_gravity(state);
        }
        state.debris_body_collisions = resolve_debris_body_collisions(state);
        state.debris_collisions = resolve_debris_collisions(state);
        spawn_debris_breakup_fragments(state);
        for debris in &mut state.debris {
            debris.update(dt);
        }
        remove_finished_debris(state);
        update_particles(state, dt);
        spawn_random_asteroid(state, dt);

        state.tick += 1;
        StepResult::default()
    }

    fn observe(_state: &Self::State) -> Observation {
        Observation {
            payload: Vec::new(),
        }
    }

    fn render_frame(state: &Self::State) -> RenderFrame {
        render_state(state)
    }

    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: 60 }
    }
}

impl SpacewarsState {
    fn apply_action(&mut self, action: SpacewarsAction) {
        let Some(ship) = self.ships.get_mut(action.player) else {
            return;
        };

        match action.kind {
            SpacewarsActionKind::CloseWings => ship.close_wings(),
            SpacewarsActionKind::OpenWings => ship.open_wings(),
            SpacewarsActionKind::Thrust => ship.thrust(),
            SpacewarsActionKind::ThrustHalt => ship.thrust_halt(),
            SpacewarsActionKind::Reverse => ship.reverse(),
            SpacewarsActionKind::Brake => ship.brake(),
            SpacewarsActionKind::BrakeHalt => ship.brake_halt(),
            SpacewarsActionKind::TurnLeft => ship.turn_left(),
            SpacewarsActionKind::TurnRight => ship.turn_right(),
            SpacewarsActionKind::TurnHalt => ship.turn_halt(),
            SpacewarsActionKind::FireLaser => ship.fire_laser(),
            SpacewarsActionKind::FireLaserHalt => ship.fire_laser_halt(),
            SpacewarsActionKind::FireCannon => ship.fire_cannon(),
            SpacewarsActionKind::FireCannonHalt => ship.fire_cannon_halt(),
        }
    }
}

impl SpacewarsScenario {
    pub fn render_player_frames(state: &SpacewarsState) -> Vec<RenderFrame> {
        (0..state.ships.len())
            .map(|player| render_state_with_camera(state, player_camera(state, player)))
            .collect()
    }
}

pub fn render_ship_bounds_debug_frame(ship: &ShipState, mode: BoundsDrawMode) -> RenderFrame {
    let triangles = ship_triangles(ship);
    let low = ship_low_bounds(&triangles);
    let mut frame = RenderFrame::new(Camera2::new(
        render_point(low.center),
        (low.radius * 2.4).max(30.0),
    ));

    render_ship(&mut frame, ship);
    render_ship_bounds(&mut frame, low, &ship_high_bounds(&triangles), mode);
    frame
}

fn build_world(config: &SpacewarsConfig, seed: u64) -> (Option<SunState>, Vec<PlanetState>) {
    if !config.use_planets {
        return (None, Vec::new());
    }

    let universe_radius = config.universe_radius as f32;
    let sun = SunState {
        position: Vec2::new(universe_radius, universe_radius),
        radius: SUN_RADIUS,
        mass: body_mass(SUN_RADIUS),
        color: Color::YELLOW,
    };
    let mut planets = Vec::new();
    let mut rng = seeded_rng(seed);
    let mut planet_min_orbit = SUN_RADIUS + 20.0;

    while planet_min_orbit < universe_radius && planets.len() < MAX_PLANETS {
        let radius = random_range_f32(&mut rng, MIN_PLANET_RADIUS, MAX_PLANET_RADIUS);
        let spacing = random_range_f32(&mut rng, MIN_PLANET_SPACING, MAX_PLANET_SPACING);
        let orbit_angle = random_range_f32(&mut rng, 0.0, core::f32::consts::TAU);
        let orbit_radius = planet_min_orbit + radius + spacing;

        if orbit_radius + radius >= universe_radius {
            break;
        }

        let max_speed = core::f32::consts::TAU / orbit_radius * PLANET_ORBIT_PERIOD_SCALAR;
        let orbit_omega = random_range_f32(&mut rng, -max_speed, max_speed);
        let wrapper_omega = random_range_f32(
            &mut rng,
            -core::f32::consts::FRAC_PI_6,
            core::f32::consts::FRAC_PI_6,
        );
        let position = sun.position + Vec2::from_radians(orbit_angle) * orbit_radius;

        planets.push(PlanetState {
            position,
            radius,
            mass: body_mass(radius),
            color: random_color(&mut rng),
            orbit_radius,
            orbit_angle,
            orbit_omega,
            wrapper_angle: 0.0,
            wrapper_omega,
        });

        planet_min_orbit += radius * 2.0 + spacing;
    }

    (Some(sun), planets)
}

fn build_starfield(config: &SpacewarsConfig, seed: u64) -> Option<StarFieldState> {
    if !config.use_starfield {
        return None;
    }

    let radius = config.universe_radius as f32;
    let center = universe_center(radius);
    let total_area = core::f32::consts::PI * radius * radius;
    let mut area_filled = 0.0;
    let mut rng = seeded_rng(seed ^ STARFIELD_RNG_SALT);
    let base_color = Color::scale_255(
        255.0,
        150.0 + random_unit_f32(&mut rng) * 100.0,
        150.0 + random_unit_f32(&mut rng) * 100.0,
    )
    .with_intensity(random_unit_f32(&mut rng) * 0.5 + 0.5);
    let mut stars = Vec::new();

    while area_filled / total_area < STARFIELD_DENSITY && stars.len() < STARFIELD_MAX_STARS {
        let size = random_unit_f32(&mut rng).powf(3.0) * 0.5 + 1.2;
        let angle = random_unit_f32(&mut rng) * core::f32::consts::TAU;
        let distance = random_unit_f32(&mut rng) * (radius - size).max(0.0);
        let position = center + Vec2::from_radians(angle) * distance;
        let rotation = random_unit_f32(&mut rng) * core::f32::consts::PI;
        let points = core::array::from_fn(|index| {
            position
                + Vec2::from_radians(
                    rotation + core::f32::consts::TAU / STARFIELD_POINTS as f32 * index as f32,
                ) * size
        });

        stars.push(StarState { points });
        area_filled += core::f32::consts::TAU * size;
    }

    Some(StarFieldState {
        stars,
        base_color,
        color_theta: 1.0,
    })
}

fn random_color(rng: &mut SpacewarsRng) -> Color {
    Color::rgb(
        random_unit_f32(rng),
        random_unit_f32(rng),
        random_unit_f32(rng),
    )
}

fn spawn_random_asteroid(state: &mut SpacewarsState, dt: f32) {
    if asteroid_count(state) > MAX_ASTEROIDS {
        return;
    }

    let mut rng = asteroid_rng_for_tick(state.seed, state.tick);
    if state.config.asteroid_probability_per_sec * dt <= random_unit_f32(&mut rng) {
        return;
    }

    let universe_radius = state.config.universe_radius as f32;
    let center = universe_center(universe_radius);
    let spawn_angle = random_unit_f32(&mut rng) * core::f32::consts::TAU;
    let position =
        center + Vec2::from_radians(spawn_angle) * (universe_radius * ASTEROID_SPAWN_RADIUS_FACTOR);

    let mut radius = ASTEROID_MIN_RADIUS + ASTEROID_RADIUS_VARIATION * random_unit_f32(&mut rng);
    if random_unit_f32(&mut rng) > ASTEROID_HUGE_CHANCE {
        radius *= ASTEROID_HUGE_SIZE_MAX_SCALE * random_unit_f32(&mut rng);
    }

    let aim = (center - position)
        .normalized()
        .rotate_radians(random_range_f32(
            &mut rng,
            -ASTEROID_MAX_AIM_ANGLE,
            ASTEROID_MAX_AIM_ANGLE,
        ));
    let velocity = aim * (ASTEROID_MAX_SPEED * random_unit_f32(&mut rng));
    let color = Color::DIM_GREY.random_variation(0.2, &mut rng);
    let mut asteroid = DebrisState::new(
        DebrisKind::Asteroid,
        position,
        velocity,
        radius,
        ASTEROID_DAMAGE_SCALAR,
        color,
    );
    asteroid.omega = random_unit_f32(&mut rng) * ASTEROID_MAX_OMEGA;
    state.debris.push(asteroid);
}

fn spawn_cannon_shells(state: &mut SpacewarsState, dt: f32) -> Vec<DebrisState> {
    state
        .ships
        .iter_mut()
        .filter_map(|ship| ship.update_cannon(dt, state.tick))
        .collect()
}

fn update_ship_lasers(state: &mut SpacewarsState) {
    for ship in &mut state.ships {
        if ship.cannon_firing {
            ship.laser_beam = None;
            continue;
        }

        ship.update_laser();
    }
}

fn resolve_laser_hits(state: &mut SpacewarsState) -> Vec<LaserHit> {
    let bodies = body_physics(state);
    let ship_bounds = state
        .ships
        .iter()
        .map(|ship| {
            let triangles = ship_triangles(ship);
            (
                ship_low_bounds(&triangles),
                Bounds2::List(ship_high_bounds(&triangles)),
            )
        })
        .collect::<Vec<_>>();
    let mut hits = Vec::new();

    for shooter in 0..state.ships.len() {
        let Some(beam) = state.ships[shooter].laser_beam else {
            continue;
        };

        let Some(hit) = nearest_laser_hit(state, shooter, beam, &ship_bounds, &bodies) else {
            continue;
        };

        if let Some(beam) = &mut state.ships[shooter].laser_beam {
            beam.tail = hit.point;
        }
        spawn_laser_hit_particles(state, beam.direction, hit);
        apply_laser_hit(state, hit);
        hits.push(hit);
    }

    hits
}

fn nearest_laser_hit(
    state: &SpacewarsState,
    shooter: usize,
    beam: LaserBeamState,
    ship_bounds: &[(Circle, Bounds2)],
    bodies: &[BodyPhysics],
) -> Option<LaserHit> {
    let line = Line::new(beam.head, beam.tail);
    let damage = laser_damage(beam);
    let mut nearest: Option<LaserHit> = None;

    let mut consider_hit = |target: LaserTarget, point: Vec2| {
        let hit = LaserHit {
            shooter,
            target,
            point,
            damage,
        };
        match nearest {
            Some(current)
                if current.point.distance_to(beam.head) <= hit.point.distance_to(beam.head) => {}
            _ => nearest = Some(hit),
        }
    };

    for (debris_index, debris) in state.debris.iter().enumerate() {
        if debris.dead {
            continue;
        }

        let bounds = debris_bounds(debris);
        if !line.intersects_circle(bounds) {
            continue;
        }
        if let Some(point) = line.nearest_circle_intersection(bounds) {
            consider_hit(LaserTarget::Debris(debris_index), point);
        }
    }

    for body in bodies {
        let bounds = Circle::new(body.position, body.radius);
        if !line.intersects_circle(bounds) {
            continue;
        }
        if let Some(point) = line.nearest_circle_intersection(bounds) {
            consider_hit(LaserTarget::Body(body.id), point);
        }
    }

    for (target, (low, high)) in ship_bounds.iter().enumerate() {
        if target == shooter {
            continue;
        }
        if !Bounds2::Line(line).intersects(&Bounds2::Circle(*low)) {
            continue;
        }
        if let Some(point) = nearest_bounds_intersection(line, high) {
            consider_hit(LaserTarget::Ship(target), point);
        }
    }

    nearest
}

fn nearest_bounds_intersection(line: Line, bounds: &Bounds2) -> Option<Vec2> {
    match bounds {
        Bounds2::Circle(circle) => line.nearest_circle_intersection(*circle),
        Bounds2::List(list) => list
            .iter()
            .filter_map(|bounds| nearest_bounds_intersection(line, bounds))
            .min_by(|a, b| {
                a.distance_to(line.start)
                    .total_cmp(&b.distance_to(line.start))
            }),
        Bounds2::Line(_) => None,
    }
}

fn apply_laser_hit(state: &mut SpacewarsState, hit: LaserHit) {
    match hit.target {
        LaserTarget::Ship(ship) => {
            let impulse = state.ships[hit.shooter].direction * hit.damage;
            state.ships[ship].translate_life_with_impulse(-hit.damage, impulse);
        }
        LaserTarget::Debris(debris) => state.debris[debris].translate_life(-hit.damage),
        LaserTarget::Body(_) => {}
    }
}

fn spawn_laser_hit_particles(state: &mut SpacewarsState, direction: Vec2, hit: LaserHit) {
    let Some((center, color, scale)) = impact_target_data(state, hit.target) else {
        return;
    };
    spawn_impact_particles(
        state,
        direction,
        center,
        hit.point,
        color,
        hit.damage,
        scale,
        laser_target_salt(hit.target),
    );
}

fn impact_target_data(state: &SpacewarsState, target: LaserTarget) -> Option<(Vec2, Color, f32)> {
    match target {
        LaserTarget::Ship(ship) => state
            .ships
            .get(ship)
            .map(|ship| (ship.position, ship.color, 10.0)),
        LaserTarget::Debris(debris) => state
            .debris
            .get(debris)
            .map(|debris| (debris.position, debris.color, 10.0)),
        LaserTarget::Body(body) => {
            body_impact_data(state, body).map(|(position, color)| (position, color, 1.0))
        }
    }
}

fn body_impact_data(state: &SpacewarsState, body: BodyId) -> Option<(Vec2, Color)> {
    match body {
        BodyId::Sun => state.sun.map(|sun| (sun.position, sun.color)),
        BodyId::Planet(index) => state
            .planets
            .get(index)
            .map(|planet| (planet.position, planet.color)),
    }
}

fn laser_target_salt(target: LaserTarget) -> u64 {
    match target {
        LaserTarget::Ship(ship) => 0x5100_0000 ^ ship as u64,
        LaserTarget::Debris(debris) => 0xDEB0_0000 ^ debris as u64,
        LaserTarget::Body(BodyId::Sun) => 0x5A00_0000,
        LaserTarget::Body(BodyId::Planet(index)) => 0xB0D0_0000 ^ index as u64,
    }
}

fn spawn_impact_particles(
    state: &mut SpacewarsState,
    flack_dir: Vec2,
    entity_center: Vec2,
    intercept: Vec2,
    color: Color,
    damage: f32,
    scale: f32,
    salt: u64,
) {
    let flack_count = damage * scale;
    if flack_count <= 0.0 || state.particles.len() as f32 + flack_count > MAX_PARTICLES as f32 {
        return;
    }

    let normal = intercept - entity_center;
    let laser_theta = flack_dir.y.atan2(flack_dir.x);
    let normal_theta = normal.y.atan2(normal.x);
    let flack_theta = core::f32::consts::PI + laser_theta - (laser_theta - normal_theta) * 2.0;
    let triangle = [
        intercept + Vec2::from_radians(flack_theta),
        intercept + Vec2::from_radians(flack_theta + 1.0),
        intercept + Vec2::from_radians(flack_theta - 1.0),
    ];
    let mut rng = particle_rng_for_spawn(state, salt ^ damage.to_bits() as u64);

    for _ in 0..flack_count.ceil() as usize {
        if state.particles.len() >= MAX_PARTICLES {
            break;
        }

        let rand_angle =
            random_unit_f32(&mut rng) * random_unit_f32(&mut rng) * PARTICLE_IMPACT_RANDOM_ANGLE;
        let velocity = Vec2::from_radians(flack_theta + rand_angle)
            * (damage * random_unit_f32(&mut rng) * PARTICLE_IMPACT_SPEED_SCALE);
        state
            .particles
            .push(ParticleState::new(triangle, velocity, color));
    }
}

fn laser_damage(beam: LaserBeamState) -> f32 {
    LASER_BASE_DAMAGE / beam.length().max(REALLY_SMALL)
}

fn asteroid_count(state: &SpacewarsState) -> usize {
    state
        .debris
        .iter()
        .filter(|debris| debris.kind == DebrisKind::Asteroid)
        .count()
}

fn asteroid_rng_for_tick(seed: u64, tick: u64) -> SpacewarsRng {
    seeded_rng(seed ^ ASTEROID_RNG_SALT ^ tick.wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

fn exhaust_rng_for_tick(seed: u64, tick: u64, owner_id: usize) -> SpacewarsRng {
    seeded_rng(
        seed ^ EXHAUST_RNG_SALT
            ^ tick.wrapping_mul(0xD1B5_4A32_D192_ED03)
            ^ (owner_id as u64).wrapping_mul(0x94D0_49BB_1331_11EB),
    )
}

fn particle_rng_for_spawn(state: &SpacewarsState, salt: u64) -> SpacewarsRng {
    seeded_rng(
        state.seed
            ^ PARTICLE_RNG_SALT
            ^ state.tick.wrapping_mul(0xB5AD_4ECEDA1CE2A9)
            ^ (state.particles.len() as u64).wrapping_mul(0x94D0_49BB_1331_11EB)
            ^ salt,
    )
}

fn breakup_rng_for_event(seed: u64, tick: u64, salt: u64) -> SpacewarsRng {
    seeded_rng(seed ^ BREAKUP_RNG_SALT ^ tick.wrapping_mul(0xA24B_AED4_963E_E407) ^ salt)
}

fn remove_finished_debris(state: &mut SpacewarsState) {
    let universe = universe_bounds(state.config.universe_radius as f32);
    state
        .debris
        .retain(|debris| !debris.dead && debris_bounds(debris).intersects_circle(universe));
}

fn universe_bounds(radius: f32) -> Circle {
    Circle::new(universe_center(radius), radius)
}

fn universe_center(radius: f32) -> Vec2 {
    Vec2::new(radius, radius)
}

fn body_mass(radius: f32) -> f32 {
    core::f32::consts::PI * radius * radius * PLANET_MASS_DENSITY
}

fn apply_world_gravity(state: &mut SpacewarsState) {
    let bodies = body_physics(state);

    for ship in &mut state.ships {
        for body in &bodies {
            apply_gravity(ship, body.position, body.mass, 1.0);
        }
    }
}

fn apply_gravity(ship: &mut ShipState, attractor_position: Vec2, attractor_mass: f32, scale: f32) {
    apply_gravity_to_velocity(
        ship.position,
        &mut ship.velocity,
        attractor_position,
        attractor_mass,
        scale,
    );
}

fn apply_gravity_to_velocity(
    position: Vec2,
    velocity: &mut Vec2,
    attractor_position: Vec2,
    attractor_mass: f32,
    scale: f32,
) {
    let offset = attractor_position - position;
    let distance = offset.length();
    let acceleration = gravity_acceleration_attracted_to(attractor_mass, distance, scale);
    *velocity += offset.normalized() * acceleration;
}

#[derive(Debug, Clone, Copy)]
struct BodyPhysics {
    id: BodyId,
    order: usize,
    position: Vec2,
    radius: f32,
    mass: f32,
    low: Circle,
    high: Circle,
    spaceport: Option<SpaceportPhysics>,
}

#[derive(Debug, Clone, Copy)]
struct SpaceportPhysics {
    planet: usize,
    bounds: Circle,
}

#[derive(Debug, Clone, Copy)]
struct BodyContact {
    ship: usize,
    body: BodyId,
    body_order: usize,
    body_position: Vec2,
    body_radius: f32,
    ship_radius: f32,
    overlap: f32,
    spaceport: Option<SpaceportPhysics>,
}

#[derive(Debug, Clone, Copy)]
struct DebrisBodyContact {
    debris: usize,
    body: BodyId,
    body_order: usize,
    body_position: Vec2,
    body_radius: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CollisionEvents {
    body_collisions: Vec<BodyCollision>,
    spaceport_contacts: Vec<SpaceportContact>,
}

#[derive(Debug, Clone, Copy)]
struct EntityCollisionBody {
    position: Vec2,
    velocity: Vec2,
    mass: f32,
    low: Circle,
}

impl EntityCollisionBody {
    fn from_ship(ship: &ShipState) -> Self {
        let triangles = ship_triangles(ship);
        Self {
            position: ship.position,
            velocity: ship.velocity,
            mass: ship.mass(),
            low: ship_low_bounds(&triangles),
        }
    }

    fn from_debris(debris: &DebrisState) -> Self {
        Self {
            position: debris.position,
            velocity: debris.velocity,
            mass: debris.mass(),
            low: debris_bounds(debris),
        }
    }
}

fn resolve_ship_debris_collisions(state: &mut SpacewarsState) -> Vec<ShipDebrisCollision> {
    let collisions = detect_ship_debris_collisions(state);

    for collision in &collisions {
        let ship = &state.ships[collision.ship];
        let debris = &state.debris[collision.debris];
        let mut ship_body = EntityCollisionBody::from_ship(ship);
        let mut debris_body = EntityCollisionBody::from_debris(debris);

        collide_entities(&mut ship_body, &mut debris_body);

        state.ships[collision.ship].position = ship_body.position;
        state.ships[collision.ship].velocity = ship_body.velocity;
        state.debris[collision.debris].position = debris_body.position;
        state.debris[collision.debris].velocity = debris_body.velocity;

        let damage =
            state.debris[collision.debris].damage_amount(state.ships[collision.ship].velocity);
        let impulse = if state.debris[collision.debris].velocity.length_squared() <= REALLY_SMALL {
            Vec2::ZERO
        } else {
            state.debris[collision.debris].velocity.normalized() * damage
        };
        state.ships[collision.ship].translate_life_with_impulse(-damage, impulse);
        state.debris[collision.debris].translate_life(-damage);

        let ship = &state.ships[collision.ship];
        let debris = &state.debris[collision.debris];
        let normal = collision_normal(ship.position, debris.position);
        let ship_radius = ship_low_bounds(&ship_triangles(ship)).radius;
        let intercept = ship.position - normal * ship_radius;
        let flack_dir = if debris.velocity.length_squared() <= REALLY_SMALL {
            -normal
        } else {
            -debris.velocity.normalized()
        };
        spawn_impact_particles(
            state,
            flack_dir,
            ship.position,
            intercept,
            ship.color,
            damage,
            10.0,
            0x51DE_BA5E ^ collision.ship as u64 ^ ((collision.debris as u64) << 16),
        );
    }

    collisions
}

fn detect_ship_debris_collisions(state: &SpacewarsState) -> Vec<ShipDebrisCollision> {
    let ship_bounds = state
        .ships
        .iter()
        .map(|ship| {
            let triangles = ship_triangles(ship);
            (
                ship_low_bounds(&triangles),
                Bounds2::List(ship_high_bounds(&triangles)),
            )
        })
        .collect::<Vec<_>>();
    let mut collisions = Vec::new();

    for (ship_index, (ship_low, ship_high)) in ship_bounds.iter().enumerate() {
        for (debris_index, debris) in state.debris.iter().enumerate() {
            if debris.dead {
                continue;
            }
            if debris.owner_id == Some(ship_index) && debris.spawn_tick == state.tick {
                continue;
            }

            let debris_bounds = debris_bounds(debris);
            if !Bounds2::Circle(*ship_low).intersects(&Bounds2::Circle(debris_bounds)) {
                continue;
            }
            if !ship_high.intersects(&Bounds2::Circle(debris_bounds)) {
                continue;
            }

            collisions.push(ShipDebrisCollision {
                ship: ship_index,
                debris: debris_index,
            });
        }
    }

    collisions
}

fn resolve_ship_collisions(state: &mut SpacewarsState) -> Vec<ShipCollision> {
    let collisions = detect_ship_collisions(state);

    for collision in &collisions {
        let (a, b) = ship_pair_mut(&mut state.ships, collision.a, collision.b);
        let mut a_body = EntityCollisionBody::from_ship(a);
        let mut b_body = EntityCollisionBody::from_ship(b);

        collide_entities(&mut a_body, &mut b_body);

        a.position = a_body.position;
        a.velocity = a_body.velocity;
        b.position = b_body.position;
        b.velocity = b_body.velocity;
    }

    collisions
}

fn resolve_debris_collisions(state: &mut SpacewarsState) -> Vec<DebrisCollision> {
    let collisions = detect_debris_collisions(state);

    for collision in &collisions {
        let (a_effect, b_effect) = {
            let (a, b) = debris_pair_mut(&mut state.debris, collision.a, collision.b);
            let damage_to_a = b.damage_amount(a.velocity);
            a.translate_life(-damage_to_a);
            let damage_to_b = a.damage_amount(b.velocity);
            b.translate_life(-damage_to_b);
            let normal = collision_normal(a.position, b.position);
            let intercept = a.position - normal * a.radius;
            let a_effect = (
                normal,
                b.position,
                intercept,
                b.color,
                damage_to_a,
                0xDEB1_0000 ^ collision.a as u64 ^ ((collision.b as u64) << 16),
            );
            let b_effect = (
                -normal,
                a.position,
                intercept,
                a.color,
                damage_to_b,
                0xDEB2_0000 ^ collision.a as u64 ^ ((collision.b as u64) << 16),
            );

            let mut a_body = EntityCollisionBody::from_debris(a);
            let mut b_body = EntityCollisionBody::from_debris(b);
            collide_entities(&mut a_body, &mut b_body);

            a.position = a_body.position;
            a.velocity = a_body.velocity;
            b.position = b_body.position;
            b.velocity = b_body.velocity;

            (a_effect, b_effect)
        };

        let (dir, center, intercept, color, damage, salt) = a_effect;
        spawn_impact_particles(state, dir, center, intercept, color, damage, 5.0, salt);
        let (dir, center, intercept, color, damage, salt) = b_effect;
        spawn_impact_particles(state, dir, center, intercept, color, damage, 5.0, salt);
    }

    collisions
}

fn detect_debris_collisions(state: &SpacewarsState) -> Vec<DebrisCollision> {
    let mut collisions = Vec::new();

    for a in 0..state.debris.len() {
        if state.debris[a].dead {
            continue;
        }

        for b in a + 1..state.debris.len() {
            if state.debris[b].dead {
                continue;
            }

            if debris_bounds(&state.debris[a]).intersects_circle(debris_bounds(&state.debris[b])) {
                collisions.push(DebrisCollision { a, b });
            }
        }
    }

    collisions
}

fn apply_debris_gravity(state: &mut SpacewarsState) {
    let bodies = body_physics(state);

    for debris in &mut state.debris {
        if debris.dead {
            continue;
        }

        for body in &bodies {
            apply_gravity_to_velocity(
                debris.position,
                &mut debris.velocity,
                body.position,
                body.mass,
                ASTEROID_GRAVITY_SCALE,
            );
        }
    }
}

fn update_particles(state: &mut SpacewarsState, dt: f32) {
    let bodies = body_physics(state);
    let apply_gravity = state.tick % PARTICLE_GRAVITY_FRAME_MODULUS == 0;

    for particle in &mut state.particles {
        if apply_gravity {
            for body in &bodies {
                apply_gravity_to_velocity(
                    particle.center(),
                    &mut particle.velocity,
                    body.position,
                    body.mass,
                    PARTICLE_GRAVITY_SCALE,
                );
            }
        }

        if let Some(body) = bodies
            .iter()
            .find(|body| particle.bounds().intersects_circle(body.high))
        {
            resolve_particle_body_collision(particle, body.position, body.radius);
        }

        particle.update(dt);
    }

    state.particles.retain(|particle| !particle.done());
}

fn resolve_particle_body_collision(
    particle: &mut ParticleState,
    body_position: Vec2,
    body_radius: f32,
) {
    let bounds = particle.bounds();
    let normal = collision_normal(bounds.center, body_position);
    let target_center = body_position + normal * (body_radius + bounds.radius);
    particle.translate(target_center - bounds.center);
    particle.velocity = (particle.velocity - normal * (2.0 * particle.velocity.dot(normal))) * 0.5;
}

fn handle_ship_deaths(state: &mut SpacewarsState) {
    let mut fragments = Vec::new();

    for ship in &mut state.ships {
        if !ship.dead || ship.fragmented || ship.form != ShipForm::Ship {
            continue;
        }

        fragments.extend(ship_breakup_fragments(
            ship,
            state.seed,
            state.tick,
            fragments.len() as u64,
        ));
        ship.fragmented = true;
        ship.change_to_escape_pod();
    }

    state.debris.extend(fragments);
}

fn spawn_debris_breakup_fragments(state: &mut SpacewarsState) {
    let mut fragments = Vec::new();

    for (debris_index, debris) in state.debris.iter_mut().enumerate() {
        if !debris.dead || debris.fragmented || debris.kind == DebrisKind::Fragment {
            continue;
        }

        let source = *debris;
        debris.fragmented = true;
        fragments.extend(debris_breakup_fragments(
            &source,
            state.seed,
            state.tick,
            debris_index,
            fragments.len() as u64,
        ));
    }

    state.debris.extend(fragments);
}

fn ship_breakup_fragments(ship: &ShipState, seed: u64, tick: u64, salt: u64) -> Vec<DebrisState> {
    let primitives = ship_fragment_primitives(ship);
    breakup_fragments(
        ship.position,
        ship.velocity,
        primitives,
        seed,
        tick,
        0x51A9_0000 ^ ship.owner_id as u64 ^ salt,
    )
}

fn debris_breakup_fragments(
    debris: &DebrisState,
    seed: u64,
    tick: u64,
    debris_index: usize,
    salt: u64,
) -> Vec<DebrisState> {
    let primitives = debris_fragment_primitives(debris);
    breakup_fragments(
        debris.position,
        debris.velocity,
        primitives,
        seed,
        tick,
        0xDEB2_0000 ^ debris_index as u64 ^ salt,
    )
}

fn breakup_fragments(
    position: Vec2,
    base_velocity: Vec2,
    primitives: Vec<BreakupPrimitive>,
    seed: u64,
    tick: u64,
    salt: u64,
) -> Vec<DebrisState> {
    if primitives.is_empty() {
        return Vec::new();
    }

    let mut rng = breakup_rng_for_event(seed, tick, salt);
    let delta_theta = core::f32::consts::TAU / primitives.len() as f32;
    let mut theta = random_unit_f32(&mut rng) * core::f32::consts::TAU;
    let mut fragments = Vec::with_capacity(primitives.len());

    for primitive in primitives {
        let velocity = Vec2::from_radians(theta) * BREAKUP_FRAGMENT_SPEED + base_velocity;
        let color = Color::DIM_GREY.random_variation(0.2, &mut rng);
        fragments.push(DebrisState::new_fragment(
            position,
            primitive.local_points,
            velocity,
            BREAKUP_FRAGMENT_OMEGA,
            color,
        ));
        theta += delta_theta;
    }

    fragments
}

#[derive(Debug, Clone, Copy)]
struct BreakupPrimitive {
    local_points: [Vec2; 3],
}

fn ship_fragment_primitives(ship: &ShipState) -> Vec<BreakupPrimitive> {
    let triangles = ship_triangles(ship);

    vec![
        BreakupPrimitive::from_world_triangle(triangles[SHIP_LASER_TRIANGLE_INDEX]),
        BreakupPrimitive::from_world_triangle(triangles[SHIP_THRUSTER_TRIANGLE_INDEX]),
        BreakupPrimitive::from_world_triangle(triangles[SHIP_LEFT_WING_TRIANGLE_INDEX]),
        BreakupPrimitive::from_world_triangle(triangles[SHIP_RIGHT_WING_TRIANGLE_INDEX]),
        BreakupPrimitive::from_world_triangle(triangles[SHIP_BODY_TRIANGLE_INDEX]),
    ]
}

fn debris_fragment_primitives(debris: &DebrisState) -> Vec<BreakupPrimitive> {
    if let Some(shape) = debris.fragment_shape {
        return vec![BreakupPrimitive {
            local_points: shape,
        }];
    }

    match debris.kind {
        DebrisKind::Shell => {
            let transform = Transform2 {
                translation: debris.position,
                scale: Vec2::splat(1.0),
                rotation_radians: debris.rotation_radians,
                pivot: Vec2::ZERO,
            };
            vec![BreakupPrimitive::from_world_triangle(transform_points(
                transform, SHELL_BODY,
            ))]
        }
        DebrisKind::Asteroid => asteroid_breakup_primitives(debris),
        DebrisKind::Fragment => Vec::new(),
    }
}

fn asteroid_breakup_primitives(debris: &DebrisState) -> Vec<BreakupPrimitive> {
    let radius = debris.breakup_radius.max(debris.radius).max(1.0);
    (0..3)
        .map(|index| {
            let a = index as f32 * core::f32::consts::TAU / 3.0;
            let b = (index + 1) as f32 * core::f32::consts::TAU / 3.0;
            BreakupPrimitive::from_local_triangle([
                Vec2::ZERO,
                Vec2::from_radians(a) * radius,
                Vec2::from_radians(b) * radius,
            ])
        })
        .collect()
}

impl BreakupPrimitive {
    fn from_world_triangle(points: [Vec2; 3]) -> Self {
        Self {
            local_points: center_triangle(points),
        }
    }

    fn from_local_triangle(points: [Vec2; 3]) -> Self {
        Self {
            local_points: center_triangle(points),
        }
    }
}

fn resolve_debris_body_collisions(state: &mut SpacewarsState) -> Vec<DebrisBodyCollision> {
    let contacts = select_debris_body_contacts(state);
    let mut collisions = Vec::new();

    for contact in &contacts {
        collisions.push(DebrisBodyCollision {
            debris: contact.debris,
            body: contact.body,
        });

        let (intercept, color, damage, flack_dir) = {
            let debris = &mut state.debris[contact.debris];
            resolve_debris_body_collision(debris, contact.body_position, contact.body_radius);
            let damage = apply_debris_body_collision_damage(debris);
            let collision_dir = (contact.body_position - debris.position).normalized();
            let intercept = contact.body_position - collision_dir * contact.body_radius;
            let flack_dir = if collision_dir.length_squared() <= REALLY_SMALL {
                -debris.velocity.normalized()
            } else {
                collision_dir
            };
            (intercept, debris.color, damage, flack_dir)
        };

        spawn_impact_particles(
            state,
            flack_dir,
            state.debris[contact.debris].position,
            intercept,
            color,
            damage,
            5.0,
            0xDEB0_B0D0 ^ contact.debris as u64 ^ ((contact.body_order as u64) << 16),
        );
    }

    collisions
}

fn select_debris_body_contacts(state: &SpacewarsState) -> Vec<DebrisBodyContact> {
    let contacts = detect_debris_body_contacts(state);
    let mut selected = Vec::new();

    for debris in 0..state.debris.len() {
        let contact = contacts
            .iter()
            .copied()
            .filter(|contact| contact.debris == debris)
            .min_by(|a, b| a.body_order.cmp(&b.body_order));

        if let Some(contact) = contact {
            selected.push(contact);
        }
    }

    selected
}

fn detect_debris_body_contacts(state: &SpacewarsState) -> Vec<DebrisBodyContact> {
    let bodies = body_physics(state);
    let mut contacts = Vec::new();

    for (debris_index, debris) in state.debris.iter().enumerate() {
        if debris.dead {
            continue;
        }

        let debris_bounds = debris_bounds(debris);
        for body in &bodies {
            if !Bounds2::Circle(debris_bounds).intersects(&Bounds2::Circle(body.low)) {
                continue;
            }
            if !Bounds2::Circle(debris_bounds).intersects(&Bounds2::Circle(body.high)) {
                continue;
            }

            contacts.push(DebrisBodyContact {
                debris: debris_index,
                body: body.id,
                body_order: body.order,
                body_position: body.position,
                body_radius: body.radius,
            });
        }
    }

    contacts
}

fn detect_ship_collisions(state: &SpacewarsState) -> Vec<ShipCollision> {
    let ship_bounds = state
        .ships
        .iter()
        .map(|ship| {
            let triangles = ship_triangles(ship);
            (
                ship_low_bounds(&triangles),
                Bounds2::List(ship_high_bounds(&triangles)),
            )
        })
        .collect::<Vec<_>>();
    let mut collisions = Vec::new();

    for a in 0..state.ships.len() {
        for b in a + 1..state.ships.len() {
            let (a_low, a_high) = &ship_bounds[a];
            let (b_low, b_high) = &ship_bounds[b];
            if !Bounds2::Circle(*a_low).intersects(&Bounds2::Circle(*b_low)) {
                continue;
            }
            if !a_high.intersects(b_high) {
                continue;
            }

            collisions.push(ShipCollision { a, b });
        }
    }

    collisions
}

fn collide_entities(a: &mut EntityCollisionBody, b: &mut EntityCollisionBody) {
    let angle = collision_normal(a.position, b.position);
    let v1 = a.velocity;
    let v2 = b.velocity;
    let m1 = a.mass;
    let m2 = b.mass;
    let total_velocity = v1.length() + v2.length();
    let (a_velocity_percent, b_velocity_percent) = if total_velocity < REALLY_SMALL {
        (0.5, 0.5)
    } else {
        (v1.length() / total_velocity, v2.length() / total_velocity)
    };

    a.velocity = ((v1 * (m1 - m2) + v2 * (2.0 * m2)) / (m1 + m2)) * DEFAULT_ELASTICITY;
    b.velocity = ((v2 * (m2 - m1) + v1 * (2.0 * m1)) / (m1 + m2)) * DEFAULT_ELASTICITY;

    let overlap = a.low.center.distance_to(b.low.center) - a.low.radius - b.low.radius;
    a.position += angle * (-a_velocity_percent * overlap * COLLISION_TRANSLATION_SCALAR);
    b.position += angle * (b_velocity_percent * overlap * COLLISION_TRANSLATION_SCALAR);
}

fn ship_pair_mut(
    ships: &mut [ShipState; 2],
    a: usize,
    b: usize,
) -> (&mut ShipState, &mut ShipState) {
    assert!(a < b);
    let (left, right) = ships.split_at_mut(b);
    (&mut left[a], &mut right[0])
}

fn debris_pair_mut(
    debris: &mut [DebrisState],
    a: usize,
    b: usize,
) -> (&mut DebrisState, &mut DebrisState) {
    assert!(a < b);
    let (left, right) = debris.split_at_mut(b);
    (&mut left[a], &mut right[0])
}

#[cfg(test)]
fn detect_body_collisions(state: &SpacewarsState) -> Vec<BodyCollision> {
    select_body_contacts(state)
        .into_iter()
        .map(|contact| BodyCollision {
            ship: contact.ship,
            body: contact.body,
        })
        .collect()
}

fn resolve_body_collisions(state: &mut SpacewarsState) -> CollisionEvents {
    let contacts = select_body_contacts(state);
    let mut events = CollisionEvents {
        body_collisions: Vec::new(),
        spaceport_contacts: Vec::new(),
    };

    for contact in &contacts {
        events.body_collisions.push(BodyCollision {
            ship: contact.ship,
            body: contact.body,
        });

        if let Some(spaceport) = contact.spaceport {
            resolve_spaceport_contact(&mut state.ships[contact.ship], spaceport.bounds.center);
            events.spaceport_contacts.push(SpaceportContact {
                ship: contact.ship,
                planet: spaceport.planet,
            });
        } else {
            let (ship_position, ship_color, damage, intercept, flack_dir) = {
                let ship = &mut state.ships[contact.ship];
                resolve_ship_body_collision(
                    ship,
                    contact.body_position,
                    contact.body_radius,
                    contact.ship_radius,
                );
                let damage = apply_body_collision_damage(ship);
                let collision_dir = (contact.body_position - ship.position).normalized();
                let intercept = contact.body_position - collision_dir * contact.body_radius;
                (ship.position, ship.color, damage, intercept, collision_dir)
            };
            spawn_impact_particles(
                state,
                flack_dir,
                ship_position,
                intercept,
                ship_color,
                damage,
                5.0,
                0x51B0_D000 ^ contact.ship as u64 ^ ((contact.body_order as u64) << 16),
            );
        }
    }

    events
}

fn select_body_contacts(state: &SpacewarsState) -> Vec<BodyContact> {
    let contacts = detect_body_contacts(state);
    let mut selected = Vec::new();

    for ship in 0..state.ships.len() {
        let contact = contacts
            .iter()
            .copied()
            .filter(|contact| contact.ship == ship)
            .max_by(|a, b| {
                a.overlap
                    .total_cmp(&b.overlap)
                    .then_with(|| b.body_order.cmp(&a.body_order))
            });

        if let Some(contact) = contact {
            selected.push(contact);
        }
    }

    selected
}

fn detect_body_contacts(state: &SpacewarsState) -> Vec<BodyContact> {
    let bodies = body_physics(state);
    let mut contacts = Vec::new();

    for (ship_index, ship) in state.ships.iter().enumerate() {
        let triangles = ship_triangles(ship);
        let ship_low_circle = ship_low_bounds(&triangles);
        let ship_low = Bounds2::Circle(ship_low_circle);
        let ship_high = Bounds2::List(ship_high_bounds(&triangles));

        for body in &bodies {
            if !ship_low.intersects(&Bounds2::Circle(body.low)) {
                continue;
            }

            if ship_high.intersects(&Bounds2::Circle(body.high)) {
                let spaceport = body
                    .spaceport
                    .filter(|spaceport| ship_high.intersects(&Bounds2::Circle(spaceport.bounds)));

                contacts.push(BodyContact {
                    ship: ship_index,
                    body: body.id,
                    body_order: body.order,
                    body_position: body.position,
                    body_radius: body.radius,
                    ship_radius: ship_low_circle.radius,
                    overlap: (ship_low_circle.radius + body.radius
                        - ship_low_circle.center.distance_to(body.position))
                    .max(0.0),
                    spaceport,
                });
            }
        }
    }

    contacts
}

fn resolve_ship_body_collision(
    ship: &mut ShipState,
    body_position: Vec2,
    body_radius: f32,
    ship_radius: f32,
) {
    let normal = collision_normal(ship.position, body_position);
    ship.position = body_position + normal * (ship_radius + body_radius);
    ship.velocity = (ship.velocity - normal * (2.0 * ship.velocity.dot(normal))) * 0.5;
}

fn resolve_debris_body_collision(debris: &mut DebrisState, body_position: Vec2, body_radius: f32) {
    let normal = collision_normal(debris.position, body_position);
    debris.position = body_position + normal * (debris.radius + body_radius);
    debris.velocity = (debris.velocity - normal * (2.0 * debris.velocity.dot(normal))) * 0.5;
}

fn apply_body_collision_damage(ship: &mut ShipState) -> f32 {
    let damage = ship.velocity.length() * PLANET_DAMAGE_SCALAR;
    ship.translate_life_with_impulse(-damage, ship.direction * -damage);
    damage
}

fn apply_debris_body_collision_damage(debris: &mut DebrisState) -> f32 {
    let impact_damage = debris.velocity.length() * DEBRIS_BODY_DAMAGE_SCALAR;
    debris.translate_life(-impact_damage);
    let body_damage = debris.velocity.length() * PLANET_DAMAGE_SCALAR;
    debris.translate_life(-body_damage);
    impact_damage + body_damage
}

fn resolve_spaceport_contact(ship: &mut ShipState, spaceport_center: Vec2) {
    let offset = spaceport_center - ship.position;
    let force = offset.length() * SPACEPORT_PULL_SCALE;
    ship.velocity *= SPACEPORT_DAMPING;
    ship.velocity += offset * (force / SHIP_MASS);
}

fn collision_normal(ship_position: Vec2, body_position: Vec2) -> Vec2 {
    let offset = ship_position - body_position;
    if offset.length() == 0.0 {
        Vec2::X
    } else {
        offset.normalized()
    }
}

fn body_physics(state: &SpacewarsState) -> Vec<BodyPhysics> {
    let mut bodies = Vec::new();

    bodies.extend(
        state
            .planets
            .iter()
            .enumerate()
            .map(|(index, planet)| BodyPhysics {
                id: BodyId::Planet(index),
                order: index,
                position: planet.position,
                radius: planet.radius,
                mass: planet.mass,
                low: body_circle(planet.position, planet.radius),
                high: body_circle(planet.position, planet.radius),
                spaceport: Some(spaceport_physics(index, planet)),
            }),
    );

    if let Some(sun) = state.sun {
        bodies.push(BodyPhysics {
            id: BodyId::Sun,
            order: bodies.len(),
            position: sun.position,
            radius: sun.radius,
            mass: sun.mass,
            low: body_circle(sun.position, sun.radius),
            high: body_circle(sun.position, sun.radius),
            spaceport: None,
        });
    }

    bodies
}

fn body_circle(position: Vec2, radius: f32) -> Circle {
    Circle::new(position, radius * BODY_BOUNDS_RADIUS_SCALE)
}

fn debris_bounds(debris: &DebrisState) -> Circle {
    Circle::new(debris.position, debris.radius)
}

fn spaceport_physics(planet: usize, state: &PlanetState) -> SpaceportPhysics {
    SpaceportPhysics {
        planet,
        bounds: polygon_bound(&spaceport_points(state)),
    }
}

fn polygon_bound(points: &[Vec2]) -> Circle {
    let center = polygon_center(points);
    let area = polygon_area(center, points);
    Circle::new(center, (area * 0.99 / core::f32::consts::PI).sqrt())
}

fn polygon_center(points: &[Vec2]) -> Vec2 {
    points
        .iter()
        .copied()
        .fold(Vec2::ZERO, |sum, point| sum + point)
        / points.len() as f32
}

fn center_triangle(points: [Vec2; 3]) -> [Vec2; 3] {
    let center = polygon_center(&points);
    points.map(|point| point - center)
}

fn polygon_area(center: Vec2, points: &[Vec2]) -> f32 {
    if points.len() < 3 {
        return 1.0;
    }

    let mut area = 0.0;
    for index in 0..points.len() - 1 {
        area += triangle_area(center, points[index], points[index + 1]);
    }
    area + triangle_area(center, points[0], points[points.len() - 1])
}

fn triangle_area(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    let ab = a.distance_to(b);
    let bc = b.distance_to(c);
    let ca = c.distance_to(a);
    let s = (ab + bc + ca) * 0.5;
    (s * (s - ab) * (s - bc) * (s - ca)).sqrt()
}

fn spaceport_points(planet: &PlanetState) -> Vec<Vec2> {
    let local = spaceport_local_points(planet.radius);
    local
        .into_iter()
        .map(|point| planet.position + point.rotate_radians(planet.wrapper_angle))
        .collect()
}

fn spaceport_local_points(planet_radius: f32) -> Vec<Vec2> {
    let depth = planet_radius * SPACEPORT_DEPTH_FACTOR;
    let angle = SPACEPORT_ARC_LENGTH / planet_radius;
    let mut points = Vec::with_capacity(SPACEPORT_OUTER_POINTS + SPACEPORT_INNER_POINTS);

    for index in 0..SPACEPORT_OUTER_POINTS {
        let theta = index as f32 * angle / SPACEPORT_OUTER_POINTS as f32;
        points.push(Vec2::new(
            theta.cos() * planet_radius,
            theta.sin() * planet_radius,
        ));
    }

    if angle < SPACEPORT_MAX_ARC_ANGLE {
        for index in 0..SPACEPORT_INNER_POINTS {
            let theta =
                (SPACEPORT_INNER_POINTS - index - 1) as f32 * angle / SPACEPORT_INNER_POINTS as f32;
            points.push(Vec2::new(theta.cos() * depth, theta.sin() * depth));
        }
    } else {
        let first = points[0];
        let last = points[SPACEPORT_OUTER_POINTS - 1];
        for index in 0..SPACEPORT_INNER_POINTS {
            points
                .push((first - last) / SPACEPORT_INNER_POINTS as f32 * (index as f32 + 1.0) + last);
        }
    }

    points
}

fn ship_low_bounds(triangles: &[[Vec2; 3]]) -> Circle {
    let center = triangles
        .iter()
        .copied()
        .max_by(|a, b| {
            polygon_area(polygon_center(a), a).total_cmp(&polygon_area(polygon_center(b), b))
        })
        .map(|triangle| triangle_low_bound(triangle).center)
        .unwrap_or(Vec2::ZERO);
    let radius = triangles
        .iter()
        .map(|triangle| {
            let bounds = triangle_low_bound(*triangle);
            center.distance_to(bounds.center) + bounds.radius
        })
        .fold(0.0, f32::max);

    Circle::new(center, radius)
}

fn ship_high_bounds(triangles: &[[Vec2; 3]]) -> BoundsList {
    let mut bounds = BoundsList::new();
    for triangle in triangles {
        bounds.extend(triangle_high_bounds(*triangle));
    }
    bounds
}

fn ship_triangles(ship: &ShipState) -> Vec<[Vec2; 3]> {
    let transform = ship_transform(ship);
    if ship.form == ShipForm::EscapePod {
        return vec![
            transform_points(transform, POD_LASER),
            transform_points(transform, POD_THRUSTER),
            transform_points(transform, POD_BODY),
        ];
    }

    vec![
        transform_points(
            transform,
            rotate_points(SHIP_LEFT_WING, SHIP_WING_PIVOT, ship.wing_theta),
        ),
        transform_points(
            transform,
            rotate_points(SHIP_RIGHT_WING, SHIP_WING_PIVOT, -ship.wing_theta),
        ),
        transform_points(transform, SHIP_WING_MOUNT),
        transform_points(transform, SHIP_THRUSTER),
        transform_points(transform, SHIP_BODY),
        transform_points(transform, SHIP_LASER),
    ]
}

fn ship_mount_center(ship: &ShipState) -> Vec2 {
    if ship.form == ShipForm::EscapePod {
        return triangle_low_bound(transform_points(ship_transform(ship), POD_LASER)).center;
    }

    triangle_low_bound(ship_triangles(ship)[SHIP_LASER_TRIANGLE_INDEX]).center
}

fn ship_thruster_center(ship: &ShipState) -> Vec2 {
    let thruster = if ship.form == ShipForm::EscapePod {
        POD_THRUSTER
    } else {
        SHIP_THRUSTER
    };
    triangle_low_bound(transform_points(ship_transform(ship), thruster)).center
}

fn ship_wing_centers(ship: &ShipState) -> (Vec2, Vec2) {
    let transform = ship_transform(ship);
    let left = rotate_points(SHIP_LEFT_WING, SHIP_WING_PIVOT, ship.wing_theta);
    let right = rotate_points(SHIP_RIGHT_WING, SHIP_WING_PIVOT, -ship.wing_theta);

    (
        triangle_low_bound(transform_points(transform, left)).center,
        triangle_low_bound(transform_points(transform, right)).center,
    )
}

fn transform_points(transform: Transform2, points: [Vec2; 3]) -> [Vec2; 3] {
    points.map(|point| transform.transform_point(point))
}

impl PlanetState {
    fn update_orbit(&mut self, sun_position: Vec2, dt: f32) {
        self.orbit_angle += self.orbit_omega * dt;
        self.wrapper_angle += self.wrapper_omega * dt;
        self.position = sun_position + Vec2::from_radians(self.orbit_angle) * self.orbit_radius;
    }
}

impl DebrisState {
    pub fn new(
        kind: DebrisKind,
        position: Vec2,
        velocity: Vec2,
        radius: f32,
        damage_scalar: f32,
        color: Color,
    ) -> Self {
        let life = debris_mass(radius) * 0.5;
        Self {
            kind,
            position,
            velocity,
            radius,
            breakup_radius: radius,
            fragment_shape: None,
            rotation_radians: 0.0,
            omega: 0.0,
            damage_scalar,
            life,
            life_max: life,
            dead: false,
            fragmented: false,
            color,
            owner_id: None,
            spawn_tick: 0,
        }
    }

    pub fn new_shell(
        owner_id: usize,
        spawn_tick: u64,
        position: Vec2,
        velocity: Vec2,
        rotation_radians: f32,
    ) -> Self {
        let mut shell = Self::new(
            DebrisKind::Shell,
            position,
            velocity,
            CANNON_SHELL_RADIUS,
            CANNON_SHELL_DAMAGE_SCALAR,
            Color::DIM_GREY,
        );
        shell.rotation_radians = rotation_radians;
        shell.omega = CANNON_SHELL_OMEGA;
        shell.life = 1.0;
        shell.life_max = 1.0;
        shell.owner_id = Some(owner_id);
        shell.spawn_tick = spawn_tick;
        shell
    }

    pub fn new_fragment(
        position: Vec2,
        local_shape: [Vec2; 3],
        velocity: Vec2,
        omega: f32,
        color: Color,
    ) -> Self {
        let radius = triangle_low_bound(local_shape).radius.max(1.0);
        let mut fragment = Self::new(
            DebrisKind::Fragment,
            position,
            velocity,
            radius,
            BREAKUP_FRAGMENT_DAMAGE_SCALAR,
            color,
        );
        fragment.fragment_shape = Some(local_shape);
        fragment.omega = omega;
        fragment
    }

    pub fn mass(self) -> f32 {
        debris_mass(self.radius)
    }

    pub fn update(&mut self, dt: f32) {
        self.rotation_radians += self.omega * dt;
        self.position += self.velocity * dt;
    }

    pub fn translate_life(&mut self, delta: f32) {
        if delta < 0.0 {
            self.breakup_radius = self.radius;
        }
        self.life += delta;
        self.update_size();
    }

    pub fn damage_amount(self, relative_velocity: Vec2) -> f32 {
        self.damage_scalar * (relative_velocity - self.velocity).length()
    }

    fn update_size(&mut self) {
        let factor = self.life / self.life_max;
        if factor < DEBRIS_DEATH_LIFE_FACTOR {
            self.life = 0.0;
            self.dead = true;
            self.shrink_to(DEBRIS_DEATH_SHRINK_FACTOR);
        } else {
            self.shrink_to(factor);
        }
    }

    fn shrink_to(&mut self, factor: f32) {
        self.radius *= factor;
        if let Some(shape) = &mut self.fragment_shape {
            for point in shape {
                *point *= factor;
            }
        }
    }
}

impl ExhaustTrailState {
    fn new(position: Vec2, velocity: Vec2, rng: &mut SpacewarsRng) -> Self {
        Self {
            start: position,
            end: position + velocity * EXHAUST_LENGTH_SCALE,
            velocity,
            color: Color::scale_255(
                255.0,
                random_unit_f32(rng) * 50.0,
                random_unit_f32(rng) * 50.0,
            ),
            decay: EXHAUST_DECAY,
        }
    }

    fn update(&mut self, dt: f32) {
        let movement = self.velocity * (dt * EXHAUST_MOVE_SCALE);
        self.start += movement;
        self.end += movement;
        self.color.r = (self.color.r - self.decay).max(0.0);
        self.color.g = (self.color.g - self.decay).max(0.0);
        self.color.b = (self.color.b - self.decay).max(0.0);
    }

    fn done(self) -> bool {
        self.color.r == 0.0 && self.color.g == 0.0 && self.color.b == 0.0
    }
}

impl ParticleState {
    fn new(points: [Vec2; 3], velocity: Vec2, color: Color) -> Self {
        Self {
            points,
            velocity,
            color,
            fade_rate: PARTICLE_FADE_RATE,
        }
    }

    fn update(&mut self, dt: f32) {
        self.translate(self.velocity * dt);
        self.color.r -= self.fade_rate * dt;
        self.color.g -= self.fade_rate * dt;
        self.color.b -= self.fade_rate * dt;
    }

    fn translate(&mut self, offset: Vec2) {
        for point in &mut self.points {
            *point += offset;
        }
    }

    fn center(self) -> Vec2 {
        polygon_center(&self.points)
    }

    fn bounds(self) -> Circle {
        polygon_bound(&self.points)
    }

    fn done(self) -> bool {
        self.color.r <= PARTICLE_DONE_THRESHOLD
            && self.color.g <= PARTICLE_DONE_THRESHOLD
            && self.color.b <= PARTICLE_DONE_THRESHOLD
    }
}

fn debris_mass(radius: f32) -> f32 {
    core::f32::consts::TAU * radius
}

impl ShipState {
    fn new(
        owner_id: usize,
        position: Vec2,
        color: Color,
        health_percent: u32,
        delta_time: f32,
    ) -> Self {
        let life = health_percent as f32;
        Self {
            owner_id,
            form: ShipForm::Ship,
            position,
            velocity: Vec2::ZERO,
            rotation_radians: 0.0,
            direction: direction_from_rotation(0.0),
            omega: 0.0,
            color,
            wing_theta: 0.0,
            wing_state: WingState::Opened,
            wing_behavior: WingBehavior::None,
            thrust_behavior: ThrustBehavior::None,
            turn_behavior: TurnBehavior::None,
            laser_firing: false,
            cannon_firing: false,
            laser_beam: None,
            exhaust_trails: Vec::new(),
            life,
            life_max: life,
            dead: false,
            fragmented: false,
            turn_power: SHIP_TURN_FORCE / SHIP_MASS * delta_time,
            thrust_power: SHIP_THRUST_FORCE / SHIP_MASS * delta_time,
            current_max_omega: BASE_MAX_OMEGA,
            cannon_cooldown_remaining: 0.0,
            delta_time,
            death_impulse: Vec2::ZERO,
        }
    }

    #[cfg(test)]
    fn new_with_default_life(
        owner_id: usize,
        position: Vec2,
        color: Color,
        delta_time: f32,
    ) -> Self {
        Self::new(owner_id, position, color, 100, delta_time)
    }

    #[cfg(test)]
    fn translate_life(&mut self, delta: f32) {
        self.translate_life_with_impulse(delta, Vec2::ZERO);
    }

    fn translate_life_with_impulse(&mut self, delta: f32, impulse: Vec2) {
        if self.form == ShipForm::EscapePod {
            return;
        }

        let was_dead = self.dead;
        self.life += delta;
        if !was_dead && self.life <= 0.0 {
            self.dead = true;
            self.death_impulse = impulse;
        }
    }

    fn close_wings(&mut self) {
        if self.form == ShipForm::EscapePod {
            return;
        }
        self.wing_behavior = WingBehavior::Close;
    }

    fn open_wings(&mut self) {
        if self.form == ShipForm::EscapePod {
            return;
        }
        self.wing_behavior = WingBehavior::Open;
    }

    fn thrust(&mut self) {
        self.thrust_behavior = ThrustBehavior::Full;
    }

    fn thrust_halt(&mut self) {
        self.thrust_behavior = ThrustBehavior::None;
    }

    fn reverse(&mut self) {
        self.thrust_behavior = ThrustBehavior::Reverse;
    }

    fn brake(&mut self) {
        self.thrust_behavior = ThrustBehavior::Brake;
    }

    fn brake_halt(&mut self) {
        self.thrust_behavior = ThrustBehavior::None;
    }

    fn turn_left(&mut self) {
        self.turn_behavior = TurnBehavior::Left;
    }

    fn turn_right(&mut self) {
        self.turn_behavior = TurnBehavior::Right;
    }

    fn turn_halt(&mut self) {
        self.turn_behavior = TurnBehavior::None;
    }

    fn fire_laser(&mut self) {
        if self.form == ShipForm::EscapePod {
            return;
        }
        self.laser_firing = true;
    }

    fn fire_laser_halt(&mut self) {
        self.laser_firing = false;
    }

    fn fire_cannon(&mut self) {
        if self.form == ShipForm::EscapePod {
            return;
        }
        self.cannon_firing = true;
    }

    fn fire_cannon_halt(&mut self) {
        self.cannon_firing = false;
    }

    fn update_cannon(&mut self, dt: f32, tick: u64) -> Option<DebrisState> {
        if self.form == ShipForm::EscapePod {
            self.cannon_cooldown_remaining = 0.0;
            return None;
        }

        let shell = if self.cannon_firing && self.cannon_cooldown_remaining <= 0.0 {
            let mount_center = ship_mount_center(self);
            let shell = DebrisState::new_shell(
                self.owner_id,
                tick,
                mount_center + self.direction * CANNON_SHELL_SPAWN_OFFSET,
                self.direction * CANNON_SHELL_SPEED + self.velocity,
                -self.direction.angle_radians(),
            );
            self.velocity -= self.direction * CANNON_RECOIL_SPEED;
            self.cannon_cooldown_remaining = CANNON_COOLDOWN_SECS;
            Some(shell)
        } else {
            None
        };

        self.cannon_cooldown_remaining = (self.cannon_cooldown_remaining - dt).max(0.0);
        shell
    }

    fn update_laser(&mut self) {
        if self.form == ShipForm::EscapePod {
            self.laser_beam = None;
            self.laser_firing = false;
            return;
        }

        if !self.laser_firing {
            self.laser_beam = None;
            return;
        }

        let head = ship_mount_center(self);
        let direction = self.direction.normalized();
        let length = self
            .laser_beam
            .map(|beam| beam.length() + LASER_GROWTH_PER_TICK)
            .unwrap_or(LASER_GROWTH_PER_TICK);

        self.laser_beam = Some(LaserBeamState {
            head,
            tail: head + direction * length,
            direction,
        });
    }

    fn update(&mut self, dt: f32, seed: u64, tick: u64) {
        if self.form == ShipForm::EscapePod {
            self.update_escape_pod(dt, seed, tick);
            return;
        }

        self.update_exhaust_trails(dt);
        let mut exhaust_rng = exhaust_rng_for_tick(seed, tick, self.owner_id);

        self.rotate_ship(&mut exhaust_rng, tick, SHIP_TURN_EXHAUST_SCALAR);
        self.position += self.velocity * dt;
        self.rotation_radians += self.omega * dt;
        self.update_wings(dt);
        self.update_thrust(&mut exhaust_rng, tick);
        self.update_turn();
    }

    fn update_escape_pod(&mut self, dt: f32, seed: u64, tick: u64) {
        self.velocity *= POD_VELOCITY_DAMPING;
        self.update_exhaust_trails(dt);
        let mut exhaust_rng = exhaust_rng_for_tick(seed, tick, self.owner_id);

        self.rotate_ship(&mut exhaust_rng, tick, POD_TURN_EXHAUST_SCALAR);
        self.position += self.velocity * dt;
        self.rotation_radians += self.omega * dt;
        self.update_pod_thrust(&mut exhaust_rng, tick);
        self.update_turn();
    }

    fn rotate_ship(&mut self, rng: &mut SpacewarsRng, tick: u64, exhaust_scalar: f32) {
        let delta_theta = self.turn_power * self.omega;
        let theta = self.rotation_radians - delta_theta;
        self.rotation_radians = theta;
        self.direction = direction_from_rotation(theta);
        if delta_theta.abs() > 0.001 {
            self.fire_exhaust(
                self.direction.rotate_radians(core::f32::consts::FRAC_PI_2)
                    * (delta_theta * exhaust_scalar),
                rng,
                tick,
            );
        }
    }

    fn update_wings(&mut self, dt: f32) {
        match self.wing_behavior {
            WingBehavior::None => {}
            WingBehavior::Close => {
                self.wing_theta += dt * WING_DELTA_SPEED;
                if self.wing_theta >= MAX_WING_THETA {
                    self.wing_behavior = WingBehavior::None;
                    self.wing_state = WingState::Closed;
                    self.wing_theta = MAX_WING_THETA;
                }
                self.update_wing_position();
            }
            WingBehavior::Open => {
                self.wing_theta -= dt * WING_DELTA_SPEED;
                if self.wing_theta <= 0.0 {
                    self.wing_behavior = WingBehavior::None;
                    self.wing_state = WingState::Opened;
                    self.current_max_omega = BASE_MAX_OMEGA;
                    self.wing_theta = 0.0;
                }
                self.update_wing_position();
                if self.wing_behavior == WingBehavior::None {
                    self.thrust_behavior = ThrustBehavior::None;
                }
            }
        }

        if self.wing_state == WingState::Closed {
            self.thrust_behavior = ThrustBehavior::Full;
        }
    }

    fn update_wing_position(&mut self) {
        let max_velocity =
            (self.wing_theta + 0.46) / MAX_WING_THETA * WING_CLOSED_SPEED + MAX_SPEED;
        let speed = self.velocity.length();
        self.velocity = self.velocity.normalized() * (speed * 0.8 + max_velocity * 0.15);
        self.current_max_omega =
            ((1.0 - self.wing_theta / MAX_WING_THETA) * BASE_MAX_OMEGA).max(WING_CLOSED_MAX_OMEGA);
        self.thrust_behavior = ThrustBehavior::Full;
    }

    fn update_thrust(&mut self, rng: &mut SpacewarsRng, tick: u64) {
        match self.thrust_behavior {
            ThrustBehavior::None => {}
            ThrustBehavior::Full => {
                self.velocity += self.direction * self.thrust_power;
                if self.wing_state == WingState::Closed {
                    self.velocity +=
                        self.direction * (self.wing_theta / MAX_WING_THETA * WING_CLOSED_SPEED);
                    self.cap_speed(WING_CLOSED_SPEED);
                } else {
                    self.cap_speed(MAX_SPEED);
                }
                self.fire_exhaust(self.direction, rng, tick);
            }
            ThrustBehavior::Brake => {
                if self.wing_state == WingState::Opened {
                    if self.velocity.length() > MAX_SPEED * 0.25 {
                        self.velocity -= self.velocity.normalized() * self.thrust_power;
                    } else {
                        self.velocity *= 0.9;
                    }

                    if self.omega.abs() > 0.01 {
                        self.omega -= self.omega.signum() * self.turn_power;
                    } else {
                        self.omega = 0.0;
                    }
                }
            }
            ThrustBehavior::Reverse => {
                self.velocity -= self.direction * self.thrust_power;
                self.cap_speed(MAX_SPEED);
                self.fire_exhaust(self.direction * -10.0, rng, tick);
            }
        }
    }

    fn update_pod_thrust(&mut self, rng: &mut SpacewarsRng, tick: u64) {
        match self.thrust_behavior {
            ThrustBehavior::None => {}
            ThrustBehavior::Full => {
                self.velocity += self.direction * self.thrust_power;
                self.cap_speed(POD_MAX_SPEED);
                self.fire_exhaust(self.direction, rng, tick);
            }
            ThrustBehavior::Brake => {
                if self.velocity.length() > POD_MAX_SPEED * 0.25 {
                    self.velocity -= self.velocity.normalized() * self.thrust_power;
                } else {
                    self.velocity *= 0.9;
                }

                if self.omega.abs() > 0.01 {
                    self.omega -= self.omega.signum() * self.turn_power;
                } else {
                    self.omega = 0.0;
                }
            }
            ThrustBehavior::Reverse => {
                self.velocity -= self.direction * self.thrust_power;
                self.cap_speed(POD_MAX_SPEED);
                self.fire_exhaust(self.direction * -10.0, rng, tick);
            }
        }
    }

    fn update_exhaust_trails(&mut self, dt: f32) {
        for trail in &mut self.exhaust_trails {
            trail.update(dt);
        }
        self.exhaust_trails.retain(|trail| !trail.done());
    }

    fn fire_exhaust(&mut self, direction: Vec2, rng: &mut SpacewarsRng, tick: u64) {
        if direction.length_squared() <= REALLY_SMALL {
            return;
        }

        if self.form == ShipForm::EscapePod {
            let pulse = ((tick % 614) as f32 * 0.001).cos() + core::f32::consts::FRAC_PI_2;
            let velocity = direction * (self.thrust_power * -0.5 * pulse * 0.001);
            let thruster = ship_thruster_center(self);
            self.exhaust_trails.push(ExhaustTrailState::new(
                thruster,
                velocity.rotate_radians(-0.01),
                rng,
            ));
            self.exhaust_trails.push(ExhaustTrailState::new(
                thruster,
                velocity.rotate_radians(0.01),
                rng,
            ));
            return;
        }

        if self.velocity.length() > MAX_SPEED {
            let (left_wing, right_wing) = ship_wing_centers(self);
            let variance = ((tick % 621) as f32 * 0.001 * 0.09).sin();
            let random = random_unit_f32(rng);
            let left_velocity = (self.velocity
                + direction * (self.thrust_power * -0.5 * 0.00025 * (0.5 + random_unit_f32(rng))))
            .rotate_radians(variance)
            .rotate_radians(2.5);
            let right_velocity = (self.velocity
                + direction * (self.thrust_power * -0.5 * 0.00025 * (1.5 - random)))
                .rotate_radians(-variance)
                .rotate_radians(-2.5);

            self.exhaust_trails
                .push(ExhaustTrailState::new(left_wing, left_velocity, rng));
            self.exhaust_trails
                .push(ExhaustTrailState::new(right_wing, right_velocity, rng));
        }

        let pulse = ((tick as f32 + self.owner_id as f32 * 17.0) * 0.073).sin()
            + core::f32::consts::FRAC_PI_2;
        let velocity = direction * (self.thrust_power * -0.5 * pulse * 0.03);
        let thruster = ship_thruster_center(self);
        self.exhaust_trails
            .push(ExhaustTrailState::new(thruster, velocity, rng));
        self.exhaust_trails
            .push(ExhaustTrailState::new(thruster, velocity, rng));
    }

    fn update_turn(&mut self) {
        match self.turn_behavior {
            TurnBehavior::None => {
                self.omega = 0.0;
            }
            TurnBehavior::Left => {
                self.omega = (self.omega - self.turn_power).max(-self.current_max_omega);
                self.turn_behavior = TurnBehavior::None;
            }
            TurnBehavior::Right => {
                self.omega = (self.omega + self.turn_power).min(self.current_max_omega);
                self.turn_behavior = TurnBehavior::None;
            }
        }
    }

    fn cap_speed(&mut self, max_speed: f32) {
        let speed = self.velocity.length();
        if speed > max_speed {
            self.velocity = self.velocity.normalized() * max_speed;
        }
    }

    fn mass(&self) -> f32 {
        if self.form == ShipForm::EscapePod {
            POD_MASS
        } else {
            SHIP_MASS
        }
    }

    fn change_to_escape_pod(&mut self) {
        self.form = ShipForm::EscapePod;
        self.dead = false;
        self.life = 0.0;
        self.velocity += self.death_impulse;
        self.death_impulse = Vec2::ZERO;
        self.laser_firing = false;
        self.cannon_firing = false;
        self.laser_beam = None;
        self.cannon_cooldown_remaining = 0.0;
        self.turn_power = POD_TURN_FORCE / POD_MASS * self.delta_time;
        self.thrust_power = POD_THRUST_FORCE / POD_MASS * self.delta_time;
        self.current_max_omega = BASE_MAX_OMEGA;
    }
}

fn direction_from_rotation(rotation_radians: f32) -> Vec2 {
    let (sin, cos) = (rotation_radians + core::f32::consts::FRAC_PI_2).sin_cos();
    Vec2::new(cos, sin)
}

fn contain_ship(ship: &mut ShipState, universe_radius: f32) {
    let universe_center = Vec2::new(universe_radius, universe_radius);
    let bounds = ship_low_bounds(&ship_triangles(ship));
    let offset = bounds.center - universe_center;
    let max_distance = (universe_radius - bounds.radius).max(0.0);
    let distance = offset.length();

    if distance > max_distance {
        let contained_center = universe_center + offset.normalized() * max_distance;
        ship.position += contained_center - bounds.center;
    }
}

fn player_state(id: usize, config: &PlayerConfig) -> PlayerState {
    PlayerState {
        id,
        name: config.name.clone(),
        health_percent: config.health_percent,
        color: config.color,
    }
}

fn render_state(state: &SpacewarsState) -> RenderFrame {
    let radius = state.config.universe_radius as f32;
    let center = Vec2::new(radius, radius);
    render_state_with_camera(state, Camera2::new(render_point(center), radius * 2.2))
}

fn player_camera(state: &SpacewarsState, player: usize) -> Camera2 {
    let center = state
        .ships
        .get(player)
        .map(|ship| ship.position)
        .unwrap_or(Vec2::ZERO);
    Camera2::new(render_point(center), PLAYER_VIEW_HEIGHT)
}

fn render_state_with_camera(state: &SpacewarsState, camera: Camera2) -> RenderFrame {
    let radius = state.config.universe_radius as f32;
    let center = Vec2::new(radius, radius);
    let mut frame = RenderFrame::new(camera);

    render_starfield(&mut frame, state);

    frame.push_primitive(
        WORLD_LAYER,
        RenderPrimitive::Circle(RenderCircle {
            center: render_point(center),
            radius,
            fill: None,
            stroke: Some(Stroke::new(RenderColor::rgba(0.45, 0.5, 0.56, 0.75), 2.0)),
        }),
    );

    if let Some(sun) = state.sun {
        render_body(
            &mut frame,
            SUN_LAYER,
            sun.position,
            sun.radius,
            RenderColor::rgba(1.0, 0.93, 0.2, 0.85),
            RenderColor::rgba(1.0, 1.0, 0.65, 0.9),
        );
    }

    for planet in &state.planets {
        render_body(
            &mut frame,
            PLANET_LAYER,
            planet.position,
            planet.radius,
            render_color(planet.color),
            RenderColor::rgba(0.72, 0.78, 0.84, 0.65),
        );
        render_spaceport(&mut frame, planet);
    }

    for ship in &state.ships {
        render_exhaust(&mut frame, ship);
    }

    for ship in &state.ships {
        render_ship(&mut frame, ship);
    }

    for ship in &state.ships {
        render_laser(&mut frame, ship);
    }

    for debris in &state.debris {
        render_debris(&mut frame, debris);
    }

    render_particles(&mut frame, state);

    for ship in &state.ships {
        render_ship_label(&mut frame, state, ship);
    }

    frame
}

fn render_starfield(frame: &mut RenderFrame, state: &SpacewarsState) {
    let Some(starfield) = &state.starfield else {
        return;
    };
    let color = starfield_color(starfield, state.tick);

    for star in &starfield.stars {
        frame.push_primitive(
            STARFIELD_LAYER,
            RenderPrimitive::Polygon(RenderPolygon {
                points: star.points.into_iter().map(render_point).collect(),
                fill: Some(Fill::new(color)),
                stroke: None,
            }),
        );
    }
}

fn starfield_color(starfield: &StarFieldState, tick: u64) -> RenderColor {
    let intensity = 1.0 - STARFIELD_COLOR_ROTATE_RANGE * 0.5
        + (starfield.color_theta + tick as f32 * STARFIELD_COLOR_ROTATE_RATE).sin()
            * STARFIELD_COLOR_ROTATE_RANGE;
    render_color(starfield.base_color.with_intensity(intensity))
}

fn render_exhaust(frame: &mut RenderFrame, ship: &ShipState) {
    for trail in &ship.exhaust_trails {
        frame.push_primitive(
            EXHAUST_LAYER,
            RenderPrimitive::Line(engine_common::RenderLine::new(
                render_point(trail.start),
                render_point(trail.end),
                Stroke::new(exhaust_color(trail.color), 1.15),
            )),
        );
    }
}

fn exhaust_color(color: Color) -> RenderColor {
    RenderColor::rgba(
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
        color.r.clamp(0.0, 0.85),
    )
}

fn render_debris(frame: &mut RenderFrame, debris: &DebrisState) {
    if debris.dead || debris.radius <= 0.0 {
        return;
    }

    if let Some(shape) = debris.fragment_shape {
        push_filled_polygon(
            frame,
            Transform2 {
                translation: debris.position,
                scale: Vec2::splat(1.0),
                rotation_radians: debris.rotation_radians,
                pivot: Vec2::ZERO,
            },
            &shape,
            render_color(debris.color),
            RenderColor::rgba(0.74, 0.78, 0.84, 0.85),
        );
        return;
    }

    if debris.kind == DebrisKind::Shell {
        push_filled_polygon(
            frame,
            Transform2 {
                translation: debris.position,
                scale: Vec2::splat(1.0),
                rotation_radians: debris.rotation_radians,
                pivot: Vec2::ZERO,
            },
            &SHELL_BODY,
            render_color(debris.color),
            RenderColor::rgba(0.74, 0.78, 0.84, 0.85),
        );
        return;
    }

    frame.push_primitive(
        SHIP_LAYER,
        RenderPrimitive::Circle(RenderCircle {
            center: render_point(debris.position),
            radius: debris.radius,
            fill: Some(Fill::new(render_color(debris.color))),
            stroke: Some(Stroke::new(RenderColor::rgba(0.74, 0.78, 0.84, 0.85), 0.75)),
        }),
    );
}

fn render_particles(frame: &mut RenderFrame, state: &SpacewarsState) {
    for particle in &state.particles {
        frame.push_primitive(
            PARTICLE_LAYER,
            RenderPrimitive::Polygon(RenderPolygon {
                points: particle.points.into_iter().map(render_point).collect(),
                fill: Some(Fill::new(particle_color(particle.color))),
                stroke: None,
            }),
        );
    }
}

fn particle_color(color: Color) -> RenderColor {
    RenderColor::rgba(
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
        color.r.max(color.g).max(color.b).clamp(0.0, 0.9),
    )
}

fn render_laser(frame: &mut RenderFrame, ship: &ShipState) {
    let Some(beam) = ship.laser_beam else {
        return;
    };

    let colors = [
        (RenderColor::rgba(1.0, 0.0, 0.0, 0.9), 0.75),
        (RenderColor::rgba(150.0 / 255.0, 0.0, 0.0, 0.75), 1.5),
        (RenderColor::rgba(75.0 / 255.0, 0.0, 0.0, 0.55), 2.0),
    ];

    for (color, width) in colors {
        frame.push_primitive(
            LASER_LAYER,
            RenderPrimitive::Line(engine_common::RenderLine::new(
                render_point(beam.head),
                render_point(beam.tail),
                Stroke::new(color, width),
            )),
        );
    }
}

fn render_body(
    frame: &mut RenderFrame,
    layer: i32,
    position: Vec2,
    radius: f32,
    fill: RenderColor,
    stroke: RenderColor,
) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Circle(RenderCircle {
            center: render_point(position),
            radius,
            fill: Some(Fill::new(fill)),
            stroke: Some(Stroke::new(stroke, 1.25)),
        }),
    );
}

fn render_spaceport(frame: &mut RenderFrame, planet: &PlanetState) {
    frame.push_primitive(
        SPACEPORT_LAYER,
        RenderPrimitive::Polygon(RenderPolygon {
            points: spaceport_points(planet)
                .into_iter()
                .map(render_point)
                .collect(),
            fill: Some(Fill::new(RenderColor::rgba(1.0, 1.0, 1.0, 0.82))),
            stroke: Some(Stroke::new(RenderColor::rgba(0.05, 0.08, 0.1, 0.8), 0.75)),
        }),
    );
}

fn render_ship(frame: &mut RenderFrame, ship: &ShipState) {
    if ship.form == ShipForm::EscapePod {
        render_escape_pod(frame, ship);
        return;
    }

    let transform = ship_transform(ship);
    let base = render_color(ship.color);
    let outline = RenderColor::rgba(0.02, 0.02, 0.03, 0.9);
    let left_wing = rotate_points(SHIP_LEFT_WING, SHIP_WING_PIVOT, ship.wing_theta);
    let right_wing = rotate_points(SHIP_RIGHT_WING, SHIP_WING_PIVOT, -ship.wing_theta);

    push_filled_polygon(frame, transform, &left_wing, dim(base, 0.72), outline);
    push_filled_polygon(frame, transform, &right_wing, dim(base, 0.72), outline);
    push_filled_polygon(
        frame,
        transform,
        &SHIP_WING_MOUNT,
        RenderColor::rgba(10.0 / 255.0, 180.0 / 255.0, 50.0 / 255.0, 1.0),
        outline,
    );
    push_filled_polygon(frame, transform, &SHIP_THRUSTER, dim(base, 0.58), outline);
    push_filled_polygon(frame, transform, &SHIP_BODY, base, outline);
    push_filled_polygon(frame, transform, &SHIP_LASER, dim(base, 1.15), outline);
}

fn render_escape_pod(frame: &mut RenderFrame, ship: &ShipState) {
    let transform = ship_transform(ship);
    let base = render_color(ship.color);
    let outline = RenderColor::rgba(0.02, 0.02, 0.03, 0.9);

    push_filled_polygon(frame, transform, &POD_THRUSTER, dim(base, 0.72), outline);
    push_filled_polygon(frame, transform, &POD_BODY, base, outline);
    push_filled_polygon(frame, transform, &POD_LASER, RenderColor::BLUE, outline);
    frame.push_primitive(
        SHIP_LAYER,
        RenderPrimitive::Circle(RenderCircle {
            center: render_point(transform.transform_point(POD_COCKPIT_CENTER)),
            radius: POD_COCKPIT_RADIUS,
            fill: Some(Fill::new(RenderColor::BLUE)),
            stroke: Some(Stroke::new(outline, 0.75)),
        }),
    );
}

fn render_ship_bounds(
    frame: &mut RenderFrame,
    low: Circle,
    high: &BoundsList,
    mode: BoundsDrawMode,
) {
    if mode.show_high() {
        for bounds in high.iter() {
            if let Bounds2::Circle(circle) = bounds {
                push_bounds_circle(
                    frame,
                    BOUNDS_HIGH_LAYER,
                    *circle,
                    RenderColor::rgba(1.0, 0.85, 0.05, 0.72),
                    0.45,
                );
            }
        }
    }

    if mode.show_low() {
        push_bounds_circle(
            frame,
            BOUNDS_LOW_LAYER,
            low,
            RenderColor::rgba(0.05, 0.8, 1.0, 0.95),
            1.0,
        );
    }
}

fn push_bounds_circle(
    frame: &mut RenderFrame,
    layer: i32,
    circle: Circle,
    color: RenderColor,
    width: f32,
) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Circle(RenderCircle {
            center: render_point(circle.center),
            radius: circle.radius,
            fill: None,
            stroke: Some(Stroke::new(color, width)),
        }),
    );
}

fn ship_transform(ship: &ShipState) -> Transform2 {
    Transform2 {
        translation: ship.position,
        scale: Vec2::splat(1.0),
        rotation_radians: ship.rotation_radians,
        pivot: if ship.form == ShipForm::EscapePod {
            POD_PIVOT
        } else {
            SHIP_PIVOT
        },
    }
}

fn rotate_points(points: [Vec2; 3], pivot: Vec2, radians: f32) -> [Vec2; 3] {
    points.map(|point| (point - pivot).rotate_radians(radians) + pivot)
}

fn render_ship_label(frame: &mut RenderFrame, state: &SpacewarsState, ship: &ShipState) {
    let player = &state.players[ship.owner_id];
    let mut text = RenderText::new(
        render_point(ship.position + Vec2::new(2.5, 18.0)),
        format!("{} {:.1}", player.name, ship.life.max(0.0)),
    );
    text.color = render_color(player.color);
    text.size = 14.0;
    text.anchor = TextAnchor::Center;
    frame.push_primitive(LABEL_LAYER, RenderPrimitive::Text(text));
}

fn push_filled_polygon(
    frame: &mut RenderFrame,
    transform: Transform2,
    points: &[Vec2],
    fill: RenderColor,
    outline: RenderColor,
) {
    frame.push_primitive(
        SHIP_LAYER,
        RenderPrimitive::Polygon(RenderPolygon {
            points: points
                .iter()
                .map(|point| render_point(transform.transform_point(*point)))
                .collect(),
            fill: Some(Fill::new(fill)),
            stroke: Some(Stroke::new(outline, 0.75)),
        }),
    );
}

fn render_point(point: Vec2) -> RenderPoint {
    RenderPoint::new(point.x, point.y)
}

fn render_color(color: Color) -> RenderColor {
    RenderColor::rgba(color.r, color.g, color.b, color.a)
}

fn dim(color: RenderColor, scale: f32) -> RenderColor {
    RenderColor::rgba(
        (color.r * scale).clamp(0.0, 1.0),
        (color.g * scale).clamp(0.0, 1.0),
        (color.b * scale).clamp(0.0, 1.0),
        color.a,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::{Path, PathBuf};

    use engine_common::RenderLine;
    use image::{Rgba, RgbaImage};

    const EPS: f32 = 1.0e-4;
    const SNAPSHOT_SIZE: u32 = 768;

    fn init_deathmatch() -> SpacewarsState {
        SpacewarsScenario::init(SpacewarsConfig::deathmatch(), 123)
    }

    fn init_deathmatch_no_asteroids() -> SpacewarsState {
        let mut config = SpacewarsConfig::deathmatch();
        config.asteroid_probability_per_sec = 0.0;
        config.universe_radius = 1_000;
        let mut state = SpacewarsScenario::init(config, 123);
        state.ships[1].position = Vec2::new(800.0, 800.0);
        state
    }

    fn init_default(seed: u64) -> SpacewarsState {
        SpacewarsScenario::init(SpacewarsConfig::default(), seed)
    }

    fn step(state: &mut SpacewarsState, actions: &[Action]) -> StepResult {
        SpacewarsScenario::step(state, actions, Duration::from_secs_f32(1.0 / 60.0))
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPS,
            "actual {actual} expected {expected}"
        );
    }

    fn assert_vec_close(actual: Vec2, expected: Vec2) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
    }

    fn expected_gravity_delta(state: &SpacewarsState, ship_position: Vec2) -> Vec2 {
        let mut delta = Vec2::ZERO;

        for body in body_physics(state) {
            delta += gravity_delta(ship_position, body.position, body.mass);
        }

        delta
    }

    fn gravity_delta(ship_position: Vec2, attractor_position: Vec2, attractor_mass: f32) -> Vec2 {
        let offset = attractor_position - ship_position;
        let distance = offset.length();
        offset.normalized() * gravity_acceleration_attracted_to(attractor_mass, distance, 1.0)
    }

    fn test_planet(position: Vec2, radius: f32) -> PlanetState {
        PlanetState {
            position,
            radius,
            mass: 0.0,
            color: Color::GREEN,
            orbit_radius: 0.0,
            orbit_angle: 0.0,
            orbit_omega: 0.0,
            wrapper_angle: 0.0,
            wrapper_omega: 0.0,
        }
    }

    fn circle_primitive_count(frame: &RenderFrame) -> usize {
        frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Circle(_)))
            .count()
    }

    fn polygon_primitive_count(frame: &RenderFrame) -> usize {
        frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Polygon(_)))
            .count()
    }

    fn test_particle_points(center: Vec2) -> [Vec2; 3] {
        [
            center + Vec2::new(0.0, 1.0),
            center + Vec2::new(-0.8660254, -0.5),
            center + Vec2::new(0.8660254, -0.5),
        ]
    }

    fn artifact_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("target/test-artifacts/spacewars-bounds")
    }

    fn write_debug_png(frame: &RenderFrame, path: &Path) {
        let mut image = RgbaImage::from_pixel(SNAPSHOT_SIZE, SNAPSHOT_SIZE, Rgba([6, 8, 18, 255]));

        for layer in frame.ordered_layers() {
            for primitive in &layer.primitives {
                match primitive {
                    RenderPrimitive::Circle(circle) => draw_circle(&mut image, frame, circle),
                    RenderPrimitive::Line(line) => draw_line(&mut image, frame, line),
                    RenderPrimitive::Polygon(polygon) => draw_polygon(&mut image, frame, polygon),
                    RenderPrimitive::Text(_) => {}
                }
            }
        }

        image.save(path).expect("debug bounds PNG should save");
    }

    fn draw_circle(image: &mut RgbaImage, frame: &RenderFrame, circle: &RenderCircle) {
        let (cx, cy) = project(frame, circle.center);
        let radius = circle.radius * SNAPSHOT_SIZE as f32 / frame.camera.height;
        let stroke_width = circle
            .stroke
            .as_ref()
            .map(|stroke| stroke.width.max(1.0))
            .unwrap_or(0.0);
        let min_x = (cx - radius - stroke_width).floor().max(0.0) as u32;
        let max_x = (cx + radius + stroke_width)
            .ceil()
            .min(SNAPSHOT_SIZE as f32 - 1.0) as u32;
        let min_y = (cy - radius - stroke_width).floor().max(0.0) as u32;
        let max_y = (cy + radius + stroke_width)
            .ceil()
            .min(SNAPSHOT_SIZE as f32 - 1.0) as u32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let distance = (dx * dx + dy * dy).sqrt();

                if let Some(fill) = circle.fill
                    && distance <= radius
                {
                    blend_pixel(image, x, y, fill.color);
                }

                if let Some(stroke) = circle.stroke
                    && (distance - radius).abs() <= stroke_width * 0.5
                {
                    blend_pixel(image, x, y, stroke.color);
                }
            }
        }
    }

    fn draw_line(image: &mut RgbaImage, frame: &RenderFrame, line: &RenderLine) {
        let (x1, y1) = project(frame, line.start);
        let (x2, y2) = project(frame, line.end);
        draw_projected_line(
            image,
            x1,
            y1,
            x2,
            y2,
            line.stroke.color,
            line.stroke.width.max(1.0),
        );
    }

    fn draw_polygon(image: &mut RgbaImage, frame: &RenderFrame, polygon: &RenderPolygon) {
        let points = polygon
            .points
            .iter()
            .map(|point| project(frame, *point))
            .collect::<Vec<_>>();
        if points.is_empty() {
            return;
        }

        let min_x = points
            .iter()
            .map(|point| point.0)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as u32;
        let max_x = points
            .iter()
            .map(|point| point.0)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(SNAPSHOT_SIZE as f32 - 1.0) as u32;
        let min_y = points
            .iter()
            .map(|point| point.1)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as u32;
        let max_y = points
            .iter()
            .map(|point| point.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(SNAPSHOT_SIZE as f32 - 1.0) as u32;

        if let Some(fill) = polygon.fill {
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    if point_in_polygon((x as f32 + 0.5, y as f32 + 0.5), &points) {
                        blend_pixel(image, x, y, fill.color);
                    }
                }
            }
        }

        if let Some(stroke) = polygon.stroke {
            for index in 0..points.len() {
                let start = points[index];
                let end = points[(index + 1) % points.len()];
                draw_projected_line(
                    image,
                    start.0,
                    start.1,
                    end.0,
                    end.1,
                    stroke.color,
                    stroke.width.max(1.0),
                );
            }
        }
    }

    fn draw_projected_line(
        image: &mut RgbaImage,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: RenderColor,
        width: f32,
    ) {
        let min_x = x1.min(x2).floor().max(0.0) as u32;
        let max_x = x1.max(x2).ceil().min(SNAPSHOT_SIZE as f32 - 1.0) as u32;
        let min_y = y1.min(y2).floor().max(0.0) as u32;
        let max_y = y1.max(y2).ceil().min(SNAPSHOT_SIZE as f32 - 1.0) as u32;
        let half_width = width * 0.5;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if distance_to_segment((x as f32 + 0.5, y as f32 + 0.5), (x1, y1), (x2, y2))
                    <= half_width
                {
                    blend_pixel(image, x, y, color);
                }
            }
        }
    }

    fn point_in_polygon(point: (f32, f32), polygon: &[(f32, f32)]) -> bool {
        let mut inside = false;
        let mut previous = polygon.len() - 1;
        for current in 0..polygon.len() {
            let (xi, yi) = polygon[current];
            let (xj, yj) = polygon[previous];
            if ((yi > point.1) != (yj > point.1))
                && (point.0 < (xj - xi) * (point.1 - yi) / (yj - yi) + xi)
            {
                inside = !inside;
            }
            previous = current;
        }
        inside
    }

    fn distance_to_segment(point: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 {
        let segment = (end.0 - start.0, end.1 - start.1);
        let len_sq = segment.0 * segment.0 + segment.1 * segment.1;
        if len_sq == 0.0 {
            return ((point.0 - start.0).powi(2) + (point.1 - start.1).powi(2)).sqrt();
        }

        let t = (((point.0 - start.0) * segment.0 + (point.1 - start.1) * segment.1) / len_sq)
            .clamp(0.0, 1.0);
        let closest = (start.0 + segment.0 * t, start.1 + segment.1 * t);
        ((point.0 - closest.0).powi(2) + (point.1 - closest.1).powi(2)).sqrt()
    }

    fn project(frame: &RenderFrame, point: RenderPoint) -> (f32, f32) {
        let viewport = frame.camera.world_to_viewport(point, 1.0);
        (
            viewport.x * (SNAPSHOT_SIZE - 1) as f32,
            viewport.y * (SNAPSHOT_SIZE - 1) as f32,
        )
    }

    fn blend_pixel(image: &mut RgbaImage, x: u32, y: u32, color: RenderColor) {
        let alpha = color.a.clamp(0.0, 1.0);
        let pixel = image.get_pixel_mut(x, y);
        let dst = pixel.0;
        let src = [
            (color.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (color.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (color.b.clamp(0.0, 1.0) * 255.0).round() as u8,
        ];

        pixel.0 = [
            blend_channel(src[0], dst[0], alpha),
            blend_channel(src[1], dst[1], alpha),
            blend_channel(src[2], dst[2], alpha),
            255,
        ];
    }

    fn blend_channel(src: u8, dst: u8, alpha: f32) -> u8 {
        (src as f32 * alpha + dst as f32 * (1.0 - alpha)).round() as u8
    }

    #[test]
    fn init_builds_original_two_ship_starting_positions() {
        let state = init_deathmatch();

        assert_eq!(state.seed, 123);
        assert_eq!(state.tick, 0);
        assert_eq!(state.config.universe_radius, 300);
        assert_eq!(state.ships[0].owner_id, 0);
        assert_eq!(state.ships[0].form, ShipForm::Ship);
        assert_eq!(state.ships[0].position, Vec2::new(375.0, 450.0));
        assert_eq!(state.ships[1].owner_id, 1);
        assert_eq!(state.ships[1].form, ShipForm::Ship);
        assert_eq!(state.ships[1].position, Vec2::new(375.0, 500.0));
        assert_eq!(state.ships[0].life, 50.0);
        assert_eq!(state.ships[0].life_max, 50.0);
        assert_eq!(state.ships[1].life, 50.0);
        assert_eq!(state.ships[1].life_max, 50.0);
        assert!(!state.ships[0].dead);
        assert!(!state.ships[1].dead);
        assert!(state.ships[0].exhaust_trails.is_empty());
        assert!(state.ships[1].exhaust_trails.is_empty());
        assert_eq!(state.players[0].name, "Player 1");
        assert_eq!(state.players[1].name, "Player 2");
        assert!(state.sun.is_none());
        assert!(state.planets.is_empty());
        assert!(
            !state
                .starfield
                .as_ref()
                .expect("deathmatch keeps the original startup starfield enabled")
                .stars
                .is_empty()
        );
        assert!(state.particles.is_empty());
        assert!(state.laser_hits.is_empty());
        assert!(state.ship_collisions.is_empty());
        assert!(state.ship_debris_collisions.is_empty());
        assert!(state.debris_collisions.is_empty());
        assert!(state.debris_body_collisions.is_empty());
        assert!(state.body_collisions.is_empty());
        assert!(state.spaceport_contacts.is_empty());
    }

    #[test]
    fn default_config_builds_original_style_sun_and_planet_bands() {
        let state = init_default(123);
        let sun = state.sun.expect("default config should create a sun");
        let universe_radius = state.config.universe_radius as f32;

        assert_eq!(state.config.universe_radius, 1200);
        assert_eq!(state.ships[0].life, 100.0);
        assert_eq!(state.ships[0].life_max, 100.0);
        assert_eq!(state.ships[1].life, 100.0);
        assert_eq!(state.ships[1].life_max, 100.0);
        assert_eq!(sun.position, Vec2::new(universe_radius, universe_radius));
        assert_eq!(sun.radius, SUN_RADIUS);
        assert_close(sun.mass, body_mass(SUN_RADIUS));
        assert!(!state.planets.is_empty());
        assert!(state.planets.len() <= MAX_PLANETS);
        assert!(
            !state
                .starfield
                .as_ref()
                .expect("default config should create a starfield")
                .stars
                .is_empty()
        );

        for planet in &state.planets {
            assert!(planet.radius >= MIN_PLANET_RADIUS);
            assert!(planet.radius < MAX_PLANET_RADIUS);
            assert!(planet.orbit_radius + planet.radius < universe_radius);
            assert_close(
                planet.position.distance_to(sun.position),
                planet.orbit_radius,
            );
            assert_close(planet.mass, body_mass(planet.radius));
        }
    }

    #[test]
    fn world_generation_replays_from_seed() {
        let first = init_default(7);
        let replay = init_default(7);
        let different = init_default(8);

        assert_eq!(first.sun, replay.sun);
        assert_eq!(first.planets, replay.planets);
        assert_eq!(first.starfield, replay.starfield);
        assert_ne!(first.planets, different.planets);
        assert_ne!(first.starfield, different.starfield);
    }

    #[test]
    fn starfield_generation_stays_inside_universe() {
        let state = init_deathmatch();
        let starfield = state
            .starfield
            .as_ref()
            .expect("deathmatch should create a starfield");
        let universe_radius = state.config.universe_radius as f32;
        let center = universe_center(universe_radius);

        assert!(!starfield.stars.is_empty());
        assert!(starfield.stars.len() <= STARFIELD_MAX_STARS);
        for star in &starfield.stars {
            for point in star.points {
                assert!(point.distance_to(center) <= universe_radius + EPS);
            }
        }
    }

    #[test]
    fn starfield_respects_config_flag() {
        let mut config = SpacewarsConfig::deathmatch();
        config.use_starfield = false;

        let state = SpacewarsScenario::init(config, 123);

        assert!(state.starfield.is_none());
    }

    #[test]
    fn planets_advance_on_original_orbit_rates() {
        let mut state = init_default(123);
        let sun = state.sun.expect("default config should create a sun");
        let planet_index = state
            .planets
            .iter()
            .position(|planet| planet.orbit_omega.abs() > EPS)
            .expect("seed should produce at least one moving planet");
        let start = state.planets[planet_index];

        step(&mut state, &[]);

        let updated = state.planets[planet_index];
        assert_close(
            updated.orbit_angle,
            start.orbit_angle + start.orbit_omega / 60.0,
        );
        assert_close(
            updated.wrapper_angle,
            start.wrapper_angle + start.wrapper_omega / 60.0,
        );
        assert_vec_close(
            updated.position,
            sun.position + Vec2::from_radians(updated.orbit_angle) * updated.orbit_radius,
        );
        assert_close(
            updated.position.distance_to(sun.position),
            updated.orbit_radius,
        );
        assert_ne!(updated.position, start.position);
    }

    #[test]
    fn planet_orbits_replay_from_seed_and_tick_count() {
        let mut first = init_default(7);
        let mut replay = init_default(7);
        let mut shorter = init_default(7);

        for _ in 0..10 {
            step(&mut first, &[]);
            step(&mut replay, &[]);
        }
        for _ in 0..9 {
            step(&mut shorter, &[]);
        }

        assert_eq!(first.planets, replay.planets);
        assert_ne!(first.planets, shorter.planets);
    }

    #[test]
    fn world_gravity_applies_original_post_update_impulse() {
        let mut state = init_default(123);
        let start_position = state.ships[0].position;

        step(&mut state, &[]);

        let expected_velocity = expected_gravity_delta(&state, start_position);
        assert_eq!(state.ships[0].position, start_position);
        assert_vec_close(state.ships[0].velocity, expected_velocity);
        assert!(
            state.ships[0].velocity.dot(
                state
                    .sun
                    .expect("default config should create a sun")
                    .position
                    - start_position
            ) > 0.0
        );
    }

    #[test]
    fn world_gravity_moves_ships_on_following_tick() {
        let mut state = init_default(123);
        let start_position = state.ships[0].position;

        step(&mut state, &[]);
        let velocity_after_first_gravity = state.ships[0].velocity;
        step(&mut state, &[]);

        assert_vec_close(
            state.ships[0].position,
            start_position + velocity_after_first_gravity / 60.0,
        );
        assert!(state.ships[0].velocity.length() > velocity_after_first_gravity.length());
    }

    #[test]
    fn gravity_at_zero_distance_leaves_velocity_unchanged() {
        let mut ship =
            ShipState::new_with_default_life(0, Vec2::new(10.0, 20.0), Color::WHITE, 1.0 / 60.0);
        ship.velocity = Vec2::new(1.0, 2.0);

        apply_gravity(&mut ship, Vec2::new(10.0, 20.0), body_mass(10.0), 1.0);

        assert_eq!(ship.velocity, Vec2::new(1.0, 2.0));
    }

    #[test]
    fn ship_bounds_replay_from_same_state() {
        let state = init_deathmatch();
        let first = ship_triangles(&state.ships[0]);
        let replay = ship_triangles(&state.ships[0]);

        assert_eq!(first, replay);
        assert_eq!(ship_low_bounds(&first), ship_low_bounds(&replay));
        assert_eq!(ship_high_bounds(&first), ship_high_bounds(&replay));
    }

    #[test]
    fn bounds_debug_modes_control_low_and_high_visibility() {
        let state = init_deathmatch();
        let high = render_ship_bounds_debug_frame(&state.ships[0], BoundsDrawMode::High);
        let low_high = render_ship_bounds_debug_frame(&state.ships[0], BoundsDrawMode::LowAndHigh);
        let low = render_ship_bounds_debug_frame(&state.ships[0], BoundsDrawMode::Low);

        assert!(circle_primitive_count(&high) > 1);
        assert_eq!(circle_primitive_count(&low), 1);
        assert_eq!(
            circle_primitive_count(&low_high),
            circle_primitive_count(&high) + 1
        );
    }

    #[test]
    fn bounds_debug_pngs_are_written_for_all_modes() {
        let state = init_deathmatch();
        let output_dir = artifact_dir();
        std::fs::create_dir_all(&output_dir).expect("debug bounds artifact dir should exist");

        let cases = [
            ("ship-bounds-high.png", BoundsDrawMode::High),
            ("ship-bounds-low-high.png", BoundsDrawMode::LowAndHigh),
            ("ship-bounds-low.png", BoundsDrawMode::Low),
        ];

        for (filename, mode) in cases {
            let path = output_dir.join(filename);
            let frame = render_ship_bounds_debug_frame(&state.ships[0], mode);
            write_debug_png(&frame, &path);

            assert!(path.exists());
            assert!(
                std::fs::metadata(path)
                    .expect("debug bounds PNG should stat")
                    .len()
                    > 0
            );
        }
    }

    #[test]
    fn body_collision_detection_uses_ship_high_bounds() {
        let mut state = init_deathmatch();
        let right_wing_tip = ship_triangles(&state.ships[0])[1][2];
        state.sun = Some(SunState {
            position: right_wing_tip,
            radius: 2.0,
            mass: 0.0,
            color: Color::YELLOW,
        });

        assert_eq!(
            detect_body_collisions(&state),
            vec![BodyCollision {
                ship: 0,
                body: BodyId::Sun,
            }]
        );
    }

    #[test]
    fn body_collision_detection_rejects_distant_body() {
        let mut state = init_deathmatch();
        state.sun = Some(SunState {
            position: Vec2::new(10_000.0, 10_000.0),
            radius: 2.0,
            mass: 0.0,
            color: Color::YELLOW,
        });

        assert!(detect_body_collisions(&state).is_empty());
    }

    #[test]
    fn body_collision_detection_selects_deepest_contact_per_ship() {
        let mut state = init_deathmatch();
        let ship_low = ship_low_bounds(&ship_triangles(&state.ships[0]));
        state.planets = vec![test_planet(ship_low.center, 5.0)];
        state.sun = Some(SunState {
            position: ship_low.center,
            radius: 20.0,
            mass: 0.0,
            color: Color::YELLOW,
        });

        assert_eq!(
            detect_body_collisions(&state),
            vec![BodyCollision {
                ship: 0,
                body: BodyId::Sun,
            }]
        );
    }

    #[test]
    fn ship_collision_detection_uses_high_bounds_after_low_hit() {
        let mut state = init_deathmatch();
        state.ships[1].position = state.ships[0].position + Vec2::new(11.5, 0.0);
        let first_low = ship_low_bounds(&ship_triangles(&state.ships[0]));
        let second_low = ship_low_bounds(&ship_triangles(&state.ships[1]));

        assert!(Bounds2::Circle(first_low).intersects(&Bounds2::Circle(second_low)));
        assert!(detect_ship_collisions(&state).is_empty());
    }

    #[test]
    fn collide_entities_exchanges_equal_mass_velocity_and_separates_by_speed_share() {
        let mut a = EntityCollisionBody {
            position: Vec2::new(-5.0, 0.0),
            velocity: Vec2::new(10.0, 0.0),
            mass: 2.0,
            low: Circle::new(Vec2::new(-5.0, 0.0), 6.0),
        };
        let mut b = EntityCollisionBody {
            position: Vec2::new(5.0, 0.0),
            velocity: Vec2::new(-4.0, 0.0),
            mass: 2.0,
            low: Circle::new(Vec2::new(5.0, 0.0), 6.0),
        };

        collide_entities(&mut a, &mut b);

        assert_vec_close(a.velocity, Vec2::new(-3.6, 0.0));
        assert_vec_close(b.velocity, Vec2::new(9.0, 0.0));
        assert_vec_close(a.position, Vec2::new(-6.428571, 0.0));
        assert_vec_close(b.position, Vec2::new(5.571429, 0.0));
    }

    #[test]
    fn collide_entities_splits_static_overlap_evenly() {
        let mut a = EntityCollisionBody {
            position: Vec2::new(-5.0, 0.0),
            velocity: Vec2::ZERO,
            mass: 2.0,
            low: Circle::new(Vec2::new(-5.0, 0.0), 6.0),
        };
        let mut b = EntityCollisionBody {
            position: Vec2::new(5.0, 0.0),
            velocity: Vec2::ZERO,
            mass: 2.0,
            low: Circle::new(Vec2::new(5.0, 0.0), 6.0),
        };

        collide_entities(&mut a, &mut b);

        assert_eq!(a.velocity, Vec2::ZERO);
        assert_eq!(b.velocity, Vec2::ZERO);
        assert_vec_close(a.position, Vec2::new(-6.0, 0.0));
        assert_vec_close(b.position, Vec2::new(6.0, 0.0));
    }

    #[test]
    fn resolve_ship_collisions_bounces_ships_without_damage() {
        let mut state = init_deathmatch();
        let start_life = [state.ships[0].life, state.ships[1].life];
        state.ships[1].position = state.ships[0].position + Vec2::new(0.0, 3.0);
        state.ships[0].velocity = Vec2::new(0.0, 20.0);
        state.ships[1].velocity = Vec2::new(0.0, -10.0);
        let start_distance = state.ships[0].position.distance_to(state.ships[1].position);

        let collisions = resolve_ship_collisions(&mut state);

        assert_eq!(collisions, vec![ShipCollision { a: 0, b: 1 }]);
        assert_vec_close(state.ships[0].velocity, Vec2::new(0.0, -9.0));
        assert_vec_close(state.ships[1].velocity, Vec2::new(0.0, 18.0));
        assert!(state.ships[0].position.distance_to(state.ships[1].position) > start_distance);
        assert_eq!(state.ships[0].life, start_life[0]);
        assert_eq!(state.ships[1].life, start_life[1]);
    }

    #[test]
    fn ship_debris_collision_bounces_then_applies_shared_damage() {
        let mut state = init_deathmatch();
        let ship_low = ship_low_bounds(&ship_triangles(&state.ships[0]));
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            ship_low.center,
            Vec2::new(0.0, -15.0),
            8.0,
            0.01,
            Color::DIM_GREY,
        ));
        state.ships[0].velocity = Vec2::new(0.0, 20.0);
        let ship_life = state.ships[0].life;
        let debris_life = state.debris[0].life;

        let collisions = resolve_ship_debris_collisions(&mut state);

        let damage = state.debris[0].damage_amount(state.ships[0].velocity);
        assert_eq!(collisions, vec![ShipDebrisCollision { ship: 0, debris: 0 }]);
        assert!(state.ships[0].velocity.y < 0.0);
        assert!(state.debris[0].velocity.y > 0.0);
        assert_close(state.ships[0].life, ship_life - damage);
        assert_close(state.debris[0].life, debris_life - damage);
    }

    #[test]
    fn debris_debris_collision_applies_mutual_damage_before_bounce() {
        let mut state = init_deathmatch();
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            Vec2::new(-4.0, 0.0),
            Vec2::new(10.0, 0.0),
            5.0,
            0.01,
            Color::DIM_GREY,
        ));
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            Vec2::new(4.0, 0.0),
            Vec2::new(-10.0, 0.0),
            5.0,
            0.01,
            Color::DIM_GREY,
        ));
        let start_life = state.debris[0].life;

        let collisions = resolve_debris_collisions(&mut state);

        assert_eq!(collisions, vec![DebrisCollision { a: 0, b: 1 }]);
        assert_close(state.debris[0].life, start_life - 0.2);
        assert_close(state.debris[1].life, start_life - 0.2);
        assert_vec_close(state.debris[0].velocity, Vec2::new(-9.0, 0.0));
        assert_vec_close(state.debris[1].velocity, Vec2::new(9.0, 0.0));
        assert!(state.debris[0].position.x < -4.0);
        assert!(state.debris[1].position.x > 4.0);
    }

    #[test]
    fn spaceport_geometry_uses_original_polygon_bound_and_wrapper_rotation() {
        let mut planet = test_planet(Vec2::new(100.0, 200.0), 50.0);
        let points = spaceport_points(&planet);
        let bounds = spaceport_physics(0, &planet).bounds;

        planet.wrapper_angle = core::f32::consts::FRAC_PI_2;
        let rotated_bounds = spaceport_physics(0, &planet).bounds;

        assert_eq!(
            points.len(),
            SPACEPORT_OUTER_POINTS + SPACEPORT_INNER_POINTS
        );
        assert!(bounds.radius > 0.0);
        assert_close(
            bounds.center.distance_to(planet.position),
            rotated_bounds.center.distance_to(planet.position),
        );
        assert_ne!(bounds.center, rotated_bounds.center);
    }

    #[test]
    fn body_collision_response_pushes_to_surface_and_reflects_velocity() {
        let mut ship =
            ShipState::new_with_default_life(0, Vec2::new(10.0, 0.0), Color::WHITE, 1.0 / 60.0);
        ship.velocity = Vec2::new(-20.0, 0.0);

        resolve_ship_body_collision(&mut ship, Vec2::ZERO, 10.0, 5.0);

        assert_eq!(ship.position, Vec2::new(15.0, 0.0));
        assert_eq!(ship.velocity, Vec2::new(10.0, 0.0));
    }

    #[test]
    fn body_collision_damage_uses_post_bounce_velocity_once() {
        let mut ship =
            ShipState::new_with_default_life(0, Vec2::new(10.0, 0.0), Color::WHITE, 1.0 / 60.0);
        ship.velocity = Vec2::new(-20.0, 0.0);

        resolve_ship_body_collision(&mut ship, Vec2::ZERO, 10.0, 5.0);
        let damage = apply_body_collision_damage(&mut ship);

        assert_close(damage, 0.1);
        assert_close(ship.life, 99.9);
        assert!(!ship.dead);
    }

    #[test]
    fn body_collision_damage_marks_ship_dead_when_life_reaches_zero() {
        let mut ship =
            ShipState::new_with_default_life(0, Vec2::new(10.0, 0.0), Color::WHITE, 1.0 / 60.0);
        ship.life = 0.05;
        ship.velocity = Vec2::new(10.0, 0.0);

        apply_body_collision_damage(&mut ship);

        assert!(ship.life <= 0.0);
        assert!(ship.dead);
    }

    #[test]
    fn spaceport_contact_damps_and_pulls_without_body_bounce() {
        let mut state = init_deathmatch();
        state.planets = vec![test_planet(Vec2::new(420.0, 450.0), 50.0)];
        let spaceport = spaceport_physics(0, &state.planets[0]);
        let start_position = spaceport.bounds.center - Vec2::X;
        let offset = spaceport.bounds.center - start_position;
        let start_life = state.ships[0].life;
        state.ships[0].position = start_position;
        state.ships[0].velocity = Vec2::ZERO;

        step(&mut state, &[]);

        assert_eq!(
            state.body_collisions,
            vec![BodyCollision {
                ship: 0,
                body: BodyId::Planet(0),
            }]
        );
        assert_eq!(
            state.spaceport_contacts,
            vec![SpaceportContact { ship: 0, planet: 0 }]
        );
        assert_eq!(state.ships[0].position, start_position);
        assert_vec_close(
            state.ships[0].velocity,
            offset * (offset.length() * SPACEPORT_PULL_SCALE / SHIP_MASS),
        );
        assert_eq!(state.ships[0].life, start_life);
    }

    #[test]
    fn step_applies_gravity_before_resolving_body_collision() {
        let mut state = init_deathmatch();
        let body_position = state.ships[0].position + Vec2::new(8.0, 0.0);
        let body_radius = 20.0;
        let body_mass = body_mass(body_radius);
        let ship_radius = ship_low_bounds(&ship_triangles(&state.ships[0])).radius;
        let normal = (state.ships[0].position - body_position).normalized();
        let gravity = gravity_delta(state.ships[0].position, body_position, body_mass);
        let expected_velocity = (gravity - normal * (2.0 * gravity.dot(normal))) * 0.5;
        let expected_position = body_position + normal * (ship_radius + body_radius);
        let expected_life = state.ships[0].life - expected_velocity.length() * PLANET_DAMAGE_SCALAR;

        state.sun = Some(SunState {
            position: body_position,
            radius: body_radius,
            mass: body_mass,
            color: Color::YELLOW,
        });

        step(&mut state, &[]);

        assert_eq!(
            state.body_collisions,
            vec![BodyCollision {
                ship: 0,
                body: BodyId::Sun,
            }]
        );
        assert_vec_close(state.ships[0].position, expected_position);
        assert_vec_close(state.ships[0].velocity, expected_velocity);
        assert_close(state.ships[0].life, expected_life);
    }

    #[test]
    fn step_advances_tick_without_moving_idle_ships() {
        let mut state = init_deathmatch();
        let start_positions = [state.ships[0].position, state.ships[1].position];

        let result = step(&mut state, &[]);

        assert!(!result.terminated);
        assert_eq!(state.tick, 1);
        assert_eq!(state.ships[0].position, start_positions[0]);
        assert_eq!(state.ships[1].position, start_positions[1]);
        assert_eq!(state.ships[0].velocity, Vec2::ZERO);
        assert_eq!(state.ships[1].velocity, Vec2::ZERO);
        assert!(state.ships[0].exhaust_trails.is_empty());
        assert!(state.ships[1].exhaust_trails.is_empty());
        assert!(state.particles.is_empty());
        assert!(state.laser_hits.is_empty());
        assert!(state.ship_collisions.is_empty());
        assert!(state.ship_debris_collisions.is_empty());
        assert!(state.debris_collisions.is_empty());
        assert!(state.debris_body_collisions.is_empty());
    }

    #[test]
    fn thrust_and_reverse_match_original_per_tick_power() {
        let mut state = init_deathmatch();

        step(&mut state, &[SpacewarsAction::thrust(0)]);
        assert_vec_close(
            state.ships[0].velocity,
            direction_from_rotation(0.0) * (SHIP_THRUST_FORCE / SHIP_MASS / 60.0),
        );
        assert_eq!(state.ships[0].position, Vec2::new(375.0, 450.0));

        step(&mut state, &[SpacewarsAction::reverse(0)]);
        assert_vec_close(state.ships[0].velocity, Vec2::ZERO);
        assert_close(
            state.ships[0].position.y,
            450.0 + (SHIP_THRUST_FORCE / SHIP_MASS / 60.0) / 60.0,
        );
    }

    #[test]
    fn thrust_emits_deterministic_exhaust_trails() {
        let mut first = init_deathmatch_no_asteroids();
        let mut replay = init_deathmatch_no_asteroids();

        step(&mut first, &[SpacewarsAction::thrust(0)]);
        step(&mut replay, &[SpacewarsAction::thrust(0)]);

        assert_eq!(
            first.ships[0].exhaust_trails,
            replay.ships[0].exhaust_trails
        );
        assert_eq!(first.ships[0].exhaust_trails.len(), 2);
        for trail in &first.ships[0].exhaust_trails {
            assert_vec_close(trail.start, ship_thruster_center(&first.ships[0]));
            assert!(trail.end.y < trail.start.y);
            assert_close(trail.color.r, 1.0);
        }
    }

    #[test]
    fn turn_emits_exhaust_after_rotation_begins() {
        let mut state = init_deathmatch_no_asteroids();

        step(&mut state, &[SpacewarsAction::turn_right(0)]);
        assert!(state.ships[0].exhaust_trails.is_empty());

        step(&mut state, &[SpacewarsAction::turn_right(0)]);

        assert_eq!(state.ships[0].exhaust_trails.len(), 2);
    }

    #[test]
    fn exhaust_trails_fade_and_are_removed() {
        let mut state = init_deathmatch_no_asteroids();

        step(&mut state, &[SpacewarsAction::thrust(0)]);
        assert_eq!(state.ships[0].exhaust_trails.len(), 2);

        for _ in 0..12 {
            step(&mut state, &[SpacewarsAction::thrust_halt(0)]);
        }

        assert!(state.ships[0].exhaust_trails.is_empty());
    }

    #[test]
    fn held_turn_sets_omega_then_rotation_advances_next_tick() {
        let mut state = init_deathmatch();
        let turn_power = SHIP_TURN_FORCE / SHIP_MASS / 60.0;

        step(&mut state, &[SpacewarsAction::turn_right(0)]);
        assert_close(state.ships[0].omega, turn_power);
        assert_close(state.ships[0].rotation_radians, 0.0);

        step(&mut state, &[]);
        assert_close(state.ships[0].omega, 0.0);
        assert_close(state.ships[0].rotation_radians, -0.0096);
    }

    #[test]
    fn wings_close_then_open_with_original_hold_release_semantics() {
        let mut state = init_deathmatch();

        step(&mut state, &[SpacewarsAction::close_wings(0)]);
        assert_close(state.ships[0].wing_theta, WING_DELTA_SPEED / 60.0);
        assert_eq!(state.ships[0].wing_behavior, WingBehavior::Close);
        assert_eq!(state.ships[0].thrust_behavior, ThrustBehavior::Full);

        for _ in 0..9 {
            step(&mut state, &[SpacewarsAction::close_wings(0)]);
        }
        assert_close(state.ships[0].wing_theta, MAX_WING_THETA);
        assert_eq!(state.ships[0].wing_state, WingState::Closed);
        assert_close(state.ships[0].current_max_omega, WING_CLOSED_MAX_OMEGA);

        step(&mut state, &[SpacewarsAction::open_wings(0)]);
        assert_eq!(state.ships[0].wing_behavior, WingBehavior::Open);
        assert!(state.ships[0].wing_theta < MAX_WING_THETA);
    }

    #[test]
    fn speed_caps_match_open_and_closed_wing_modes() {
        let mut state = init_deathmatch();
        state.ships[0].velocity = Vec2::new(0.0, MAX_SPEED * 2.0);

        step(&mut state, &[SpacewarsAction::thrust(0)]);
        assert_close(state.ships[0].velocity.length(), MAX_SPEED);

        state.ships[0].wing_state = WingState::Closed;
        state.ships[0].wing_theta = MAX_WING_THETA;
        state.ships[0].velocity = Vec2::new(0.0, WING_CLOSED_SPEED * 2.0);
        step(&mut state, &[SpacewarsAction::thrust(0)]);
        assert_close(state.ships[0].velocity.length(), WING_CLOSED_SPEED);
    }

    #[test]
    fn ship_is_kept_inside_universe_bounds() {
        let mut state = init_deathmatch();
        let radius = state.config.universe_radius as f32;

        state.ships[0].position = Vec2::new(radius * 2.0, radius);
        step(&mut state, &[]);

        let center = Vec2::new(radius, radius);
        let bounds = ship_low_bounds(&ship_triangles(&state.ships[0]));
        let distance = (bounds.center - center).length();
        assert!(distance <= radius - bounds.radius + EPS);
    }

    #[test]
    fn invalid_actions_are_ignored() {
        let mut state = init_deathmatch();
        let start = state.ships[0].clone();
        let invalid = Action {
            kind: 999,
            payload: vec![0],
        };

        step(&mut state, &[invalid]);

        assert_eq!(state.ships[0], start);
    }

    #[test]
    fn fire_cannon_spawns_original_shell_and_recoil() {
        let mut state = init_deathmatch_no_asteroids();
        let start_ship = state.ships[0].clone();
        let mount_center = ship_mount_center(&start_ship);

        step(&mut state, &[SpacewarsAction::fire_cannon(0)]);

        assert_eq!(state.debris.len(), 1);
        let shell = state.debris[0];
        assert_eq!(shell.kind, DebrisKind::Shell);
        assert_eq!(shell.owner_id, Some(0));
        assert_eq!(shell.spawn_tick, 0);
        assert_close(shell.life, 1.0);
        assert_close(shell.life_max, 1.0);
        assert_close(shell.radius, CANNON_SHELL_RADIUS);
        assert_close(shell.damage_scalar, CANNON_SHELL_DAMAGE_SCALAR);
        assert_close(shell.omega, CANNON_SHELL_OMEGA);
        assert_close(
            shell.rotation_radians,
            core::f32::consts::FRAC_PI_2 + CANNON_SHELL_OMEGA / 60.0,
        );
        assert_vec_close(
            shell.position,
            mount_center + Vec2::Y * (CANNON_SHELL_SPAWN_OFFSET + CANNON_SHELL_SPEED / 60.0),
        );
        assert_vec_close(shell.velocity, Vec2::new(0.0, CANNON_SHELL_SPEED));
        assert_vec_close(
            state.ships[0].velocity,
            Vec2::new(0.0, -CANNON_RECOIL_SPEED),
        );
    }

    #[test]
    fn held_cannon_respects_original_cooldown() {
        let mut state = init_deathmatch_no_asteroids();

        step(&mut state, &[SpacewarsAction::fire_cannon(0)]);
        for _ in 0..29 {
            step(&mut state, &[SpacewarsAction::fire_cannon(0)]);
        }

        assert_eq!(
            state
                .debris
                .iter()
                .filter(|debris| debris.kind == DebrisKind::Shell)
                .count(),
            1
        );

        step(&mut state, &[SpacewarsAction::fire_cannon(0)]);

        assert_eq!(
            state
                .debris
                .iter()
                .filter(|debris| debris.kind == DebrisKind::Shell)
                .count(),
            2
        );
    }

    #[test]
    fn cannon_shell_damages_other_ship_and_is_removed() {
        let mut state = init_deathmatch_no_asteroids();
        let shell_spawn_position = ship_mount_center(&state.ships[0])
            + state.ships[0].direction * CANNON_SHELL_SPAWN_OFFSET;
        let target_low_offset =
            ship_low_bounds(&ship_triangles(&state.ships[1])).center - state.ships[1].position;
        state.ships[1].position = shell_spawn_position - target_low_offset;
        let start_life = state.ships[1].life;

        step(&mut state, &[SpacewarsAction::fire_cannon(0)]);

        assert_eq!(
            state.ship_debris_collisions,
            vec![ShipDebrisCollision { ship: 1, debris: 0 }]
        );
        assert!(state.ships[1].life < start_life);
        assert!(
            state
                .debris
                .iter()
                .all(|debris| debris.kind != DebrisKind::Shell)
        );
    }

    #[test]
    fn cannon_shell_damages_asteroid_and_leaves_breakup_fragments() {
        let mut state = init_deathmatch_no_asteroids();
        let shell_spawn_position = ship_mount_center(&state.ships[0])
            + state.ships[0].direction * CANNON_SHELL_SPAWN_OFFSET;
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            shell_spawn_position,
            Vec2::ZERO,
            20.0,
            ASTEROID_DAMAGE_SCALAR,
            Color::DIM_GREY,
        ));
        let start_life = state.debris[0].life;

        step(&mut state, &[SpacewarsAction::fire_cannon(0)]);

        assert_eq!(
            state.debris_collisions,
            vec![DebrisCollision { a: 0, b: 1 }]
        );
        assert!(start_life > 0.0);
        assert_eq!(state.debris.len(), 4);
        assert!(
            state
                .debris
                .iter()
                .all(|debris| debris.kind == DebrisKind::Fragment)
        );
    }

    #[test]
    fn cannon_shell_hits_body_and_leaves_breakup_fragment() {
        let mut state = init_deathmatch_no_asteroids();

        step(&mut state, &[SpacewarsAction::fire_cannon(0)]);
        let shell_position = state.debris[0].position;
        state.sun = Some(SunState {
            position: shell_position,
            radius: 5.0,
            mass: 0.0,
            color: Color::YELLOW,
        });

        step(&mut state, &[]);

        assert_eq!(
            state.debris_body_collisions,
            vec![DebrisBodyCollision {
                debris: 0,
                body: BodyId::Sun,
            }]
        );
        assert_eq!(state.debris.len(), 1);
        assert_eq!(state.debris[0].kind, DebrisKind::Fragment);
    }

    #[test]
    fn fire_laser_starts_beam_and_hold_extends_it() {
        let mut state = init_deathmatch_no_asteroids();

        step(&mut state, &[SpacewarsAction::fire_laser(0)]);
        let first = state.ships[0].laser_beam.expect("laser should start");
        assert_close(first.length(), LASER_GROWTH_PER_TICK);
        assert_close(
            laser_damage(first),
            LASER_BASE_DAMAGE / LASER_GROWTH_PER_TICK,
        );

        step(&mut state, &[SpacewarsAction::fire_laser(0)]);
        let second = state.ships[0].laser_beam.expect("laser should continue");
        assert_close(second.length(), LASER_GROWTH_PER_TICK * 2.0);
        assert_close(
            laser_damage(second),
            LASER_BASE_DAMAGE / (LASER_GROWTH_PER_TICK * 2.0),
        );
    }

    #[test]
    fn fire_laser_halt_clears_beam() {
        let mut state = init_deathmatch_no_asteroids();

        step(&mut state, &[SpacewarsAction::fire_laser(0)]);
        assert!(state.ships[0].laser_beam.is_some());

        step(&mut state, &[SpacewarsAction::fire_laser_halt(0)]);

        assert!(state.ships[0].laser_beam.is_none());
        assert!(state.laser_hits.is_empty());
    }

    #[test]
    fn laser_hits_nearest_target_and_truncates_beam() {
        let mut state = init_deathmatch_no_asteroids();
        let head = ship_mount_center(&state.ships[0]);
        let asteroid_center = head + state.ships[0].direction * 40.0;
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            asteroid_center,
            Vec2::ZERO,
            5.0,
            ASTEROID_DAMAGE_SCALAR,
            Color::DIM_GREY,
        ));
        let target_low_offset =
            ship_low_bounds(&ship_triangles(&state.ships[1])).center - state.ships[1].position;
        state.ships[1].position = head + state.ships[0].direction * 90.0 - target_low_offset;

        step(&mut state, &[SpacewarsAction::fire_laser(0)]);

        let beam = state.ships[0]
            .laser_beam
            .expect("beam should remain active");
        let expected_hit = Line::new(
            head,
            head + state.ships[0].direction * LASER_GROWTH_PER_TICK,
        )
        .nearest_circle_intersection(Circle::new(asteroid_center, 5.0))
        .expect("asteroid should intersect first beam");

        assert_eq!(state.laser_hits.len(), 1);
        assert_eq!(state.laser_hits[0].target, LaserTarget::Debris(0));
        assert_vec_close(state.laser_hits[0].point, expected_hit);
        assert_vec_close(beam.tail, expected_hit);
        assert!(state.ships[1].life == state.ships[1].life_max);
    }

    #[test]
    fn cannon_input_suppresses_laser_for_that_tick() {
        let mut state = init_deathmatch_no_asteroids();

        step(
            &mut state,
            &[
                SpacewarsAction::fire_laser(0),
                SpacewarsAction::fire_cannon(0),
            ],
        );

        assert!(state.ships[0].laser_beam.is_none());
        assert_eq!(
            state
                .debris
                .iter()
                .filter(|debris| debris.kind == DebrisKind::Shell)
                .count(),
            1
        );
    }

    #[test]
    fn render_frame_includes_active_laser_lines() {
        let mut state = init_deathmatch_no_asteroids();

        step(&mut state, &[SpacewarsAction::fire_laser(0)]);
        let frame = SpacewarsScenario::render_frame(&state);
        let lines = frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Line(_)))
            .count();

        assert_eq!(lines, 3);
    }

    #[test]
    fn render_frame_includes_exhaust_lines_behind_ships() {
        let mut state = init_deathmatch_no_asteroids();

        step(&mut state, &[SpacewarsAction::thrust(0)]);
        let frame = SpacewarsScenario::render_frame(&state);
        let exhaust_layer = frame
            .layers
            .iter()
            .find(|layer| layer.z == EXHAUST_LAYER)
            .expect("exhaust should render on its own layer");

        assert_eq!(
            exhaust_layer.primitives.len(),
            state.ships[0].exhaust_trails.len()
        );
        assert!(EXHAUST_LAYER < SHIP_LAYER);
        assert!(
            exhaust_layer
                .primitives
                .iter()
                .all(|primitive| matches!(primitive, RenderPrimitive::Line(_)))
        );
    }

    #[test]
    fn laser_hit_spawns_deterministic_impact_particles() {
        let mut first = init_deathmatch_no_asteroids();
        let mut replay = init_deathmatch_no_asteroids();
        let head = ship_mount_center(&first.ships[0]);
        let asteroid_center = head + first.ships[0].direction * 40.0;

        for state in [&mut first, &mut replay] {
            state.debris.push(DebrisState::new(
                DebrisKind::Asteroid,
                asteroid_center,
                Vec2::ZERO,
                5.0,
                ASTEROID_DAMAGE_SCALAR,
                Color::DIM_GREY,
            ));
            step(state, &[SpacewarsAction::fire_laser(0)]);
        }

        assert_eq!(first.particles, replay.particles);
        assert_eq!(first.laser_hits.len(), 1);
        assert_eq!(first.particles.len(), 2);
        assert!(first.particles.iter().all(|particle| {
            particle.velocity.length() > 0.0 && particle.color.r < Color::DIM_GREY.r
        }));
    }

    #[test]
    fn particles_get_gravity_every_third_frame() {
        let mut state = init_deathmatch_no_asteroids();
        let body_radius = 10.0;
        let body_mass = body_mass(body_radius);
        let center = universe_center(state.config.universe_radius as f32);
        state.sun = Some(SunState {
            position: center + Vec2::new(100.0, 0.0),
            radius: body_radius,
            mass: body_mass,
            color: Color::YELLOW,
        });
        state.particles.push(ParticleState::new(
            test_particle_points(center),
            Vec2::ZERO,
            Color::WHITE,
        ));

        step(&mut state, &[]);
        let expected_velocity =
            Vec2::X * gravity_acceleration_attracted_to(body_mass, 100.0, PARTICLE_GRAVITY_SCALE);
        assert_vec_close(state.particles[0].velocity, expected_velocity);

        step(&mut state, &[]);

        assert_vec_close(state.particles[0].velocity, expected_velocity);
    }

    #[test]
    fn faded_particles_are_removed() {
        let mut state = init_deathmatch_no_asteroids();
        let center = universe_center(state.config.universe_radius as f32);
        state.particles.push(ParticleState::new(
            test_particle_points(center),
            Vec2::ZERO,
            Color::rgb(0.101, 0.101, 0.101),
        ));

        step(&mut state, &[]);

        assert!(state.particles.is_empty());
    }

    #[test]
    fn render_frame_includes_particle_polygons_above_ships() {
        let mut state = init_deathmatch_no_asteroids();
        let center = universe_center(state.config.universe_radius as f32);
        state.particles.push(ParticleState::new(
            test_particle_points(center),
            Vec2::ZERO,
            Color::WHITE,
        ));

        let frame = SpacewarsScenario::render_frame(&state);
        let particle_layer = frame
            .layers
            .iter()
            .find(|layer| layer.z == PARTICLE_LAYER)
            .expect("particles should render on their own layer");

        assert_eq!(particle_layer.primitives.len(), 1);
        assert!(PARTICLE_LAYER > SHIP_LAYER);
        assert!(
            particle_layer
                .primitives
                .iter()
                .all(|primitive| matches!(primitive, RenderPrimitive::Polygon(_)))
        );
    }

    #[test]
    fn newly_fired_shell_does_not_hit_its_owner_on_spawn_tick() {
        let mut state = init_deathmatch_no_asteroids();
        let start_life = state.ships[0].life;

        step(&mut state, &[SpacewarsAction::fire_cannon(0)]);

        assert_eq!(state.ships[0].life, start_life);
        assert!(
            state
                .ship_debris_collisions
                .iter()
                .all(|collision| collision.ship != 0),
            "owner shell should not collide with firing ship on spawn tick"
        );
    }

    #[test]
    fn debris_constructor_uses_original_mass_and_life_rules() {
        let debris = DebrisState::new(
            DebrisKind::Asteroid,
            Vec2::new(1.0, 2.0),
            Vec2::new(3.0, 4.0),
            5.0,
            0.01,
            Color::WHITE,
        );

        assert_eq!(debris.kind, DebrisKind::Asteroid);
        assert_eq!(debris.position, Vec2::new(1.0, 2.0));
        assert_eq!(debris.velocity, Vec2::new(3.0, 4.0));
        assert_eq!(debris.radius, 5.0);
        assert_close(debris.mass(), core::f32::consts::TAU * 5.0);
        assert_close(debris.life, core::f32::consts::TAU * 5.0 * 0.5);
        assert_eq!(debris.life, debris.life_max);
        assert!(!debris.dead);
    }

    #[test]
    fn debris_update_moves_and_rotates() {
        let mut debris = DebrisState::new(
            DebrisKind::Fragment,
            Vec2::new(1.0, 2.0),
            Vec2::new(3.0, 4.0),
            5.0,
            0.0,
            Color::WHITE,
        );
        debris.omega = 2.0;

        debris.update(0.5);

        assert_eq!(debris.position, Vec2::new(2.5, 4.0));
        assert_close(debris.rotation_radians, 1.0);
    }

    #[test]
    fn debris_life_loss_shrinks_radius_by_current_life_fraction() {
        let mut debris = DebrisState::new(
            DebrisKind::Fragment,
            Vec2::ZERO,
            Vec2::ZERO,
            10.0,
            0.0,
            Color::WHITE,
        );

        debris.translate_life(-debris.life_max * 0.1);

        assert_close(debris.life, debris.life_max * 0.9);
        assert_close(debris.radius, 9.0);
        assert!(!debris.dead);
        assert_close(debris.mass(), core::f32::consts::TAU * 9.0);
    }

    #[test]
    fn debris_life_below_original_threshold_kills_and_compound_shrinks() {
        let mut debris = DebrisState::new(
            DebrisKind::Fragment,
            Vec2::ZERO,
            Vec2::ZERO,
            10.0,
            0.0,
            Color::WHITE,
        );

        debris.translate_life(-debris.life_max * 0.21);

        assert_eq!(debris.life, 0.0);
        assert!(debris.dead);
        assert_close(debris.radius, 0.1);
    }

    #[test]
    fn debris_damage_amount_uses_relative_velocity_to_debris() {
        let debris = DebrisState::new(
            DebrisKind::Asteroid,
            Vec2::ZERO,
            Vec2::new(3.0, 4.0),
            5.0,
            0.01,
            Color::WHITE,
        );

        assert_close(debris.damage_amount(Vec2::new(6.0, 8.0)), 0.05);
    }

    #[test]
    fn dead_ship_spawns_original_primitive_breakup_fragments_once() {
        let mut state = init_deathmatch_no_asteroids();
        let mut replay = init_deathmatch_no_asteroids();
        state.ships[0].velocity = Vec2::new(3.0, -4.0);
        replay.ships[0].velocity = state.ships[0].velocity;
        let life = state.ships[0].life;
        state.ships[0].translate_life(-life);
        replay.ships[0].translate_life(-life);

        handle_ship_deaths(&mut state);
        handle_ship_deaths(&mut replay);

        assert!(state.ships[0].fragmented);
        assert_eq!(state.ships[0].form, ShipForm::EscapePod);
        assert!(!state.ships[0].dead);
        assert_eq!(state.ships[0].life, 0.0);
        assert_eq!(state.debris.len(), 5);
        assert_eq!(state.debris, replay.debris);
        assert!(state.debris.iter().all(|fragment| {
            fragment.kind == DebrisKind::Fragment
                && fragment.fragment_shape.is_some()
                && fragment.damage_scalar == BREAKUP_FRAGMENT_DAMAGE_SCALAR
        }));
        for fragment in &state.debris {
            assert_eq!(fragment.position, state.ships[0].position);
            assert_close(fragment.omega, BREAKUP_FRAGMENT_OMEGA);
            assert_close(
                (fragment.velocity - state.ships[0].velocity).length(),
                BREAKUP_FRAGMENT_SPEED,
            );
            assert!((fragment.color.r - Color::DIM_GREY.r).abs() <= 0.1 + EPS);
            assert!((fragment.color.g - Color::DIM_GREY.g).abs() <= 0.1 + EPS);
            assert!((fragment.color.b - Color::DIM_GREY.b).abs() <= 0.1 + EPS);
        }

        let first_fragments = state.debris.clone();
        handle_ship_deaths(&mut state);

        assert_eq!(state.debris, first_fragments);
    }

    #[test]
    fn escape_pod_ignores_damage_and_weapons() {
        let mut state = init_deathmatch_no_asteroids();
        state.ships[0].change_to_escape_pod();

        state.ships[0].translate_life(-100.0);
        step(
            &mut state,
            &[
                SpacewarsAction::fire_laser(0),
                SpacewarsAction::fire_cannon(0),
            ],
        );

        assert_eq!(state.ships[0].form, ShipForm::EscapePod);
        assert_eq!(state.ships[0].life, 0.0);
        assert!(!state.ships[0].dead);
        assert!(state.ships[0].laser_beam.is_none());
        assert!(state.debris.is_empty());
    }

    #[test]
    fn escape_pod_uses_pod_motion_limits() {
        let mut state = init_deathmatch_no_asteroids();
        state.ships[0].change_to_escape_pod();

        step(&mut state, &[SpacewarsAction::thrust(0)]);

        assert_eq!(state.ships[0].form, ShipForm::EscapePod);
        assert!(state.ships[0].velocity.length() > 0.0);
        assert!(state.ships[0].velocity.length() <= POD_MAX_SPEED);
    }

    #[test]
    fn render_frame_draws_escape_pod_geometry() {
        let mut state = init_deathmatch();
        state.ships[0].change_to_escape_pod();
        let star_count = state
            .starfield
            .as_ref()
            .expect("deathmatch should create a starfield")
            .stars
            .len();

        let frame = SpacewarsScenario::render_frame(&state);

        assert_eq!(circle_primitive_count(&frame), 2);
        assert_eq!(polygon_primitive_count(&frame), 9 + star_count);
    }

    #[test]
    fn dead_asteroid_spawns_breakup_fragments_before_cleanup() {
        let mut state = init_deathmatch_no_asteroids();
        let center = universe_center(state.config.universe_radius as f32);
        let velocity = Vec2::new(2.0, -3.0);
        let mut asteroid = DebrisState::new(
            DebrisKind::Asteroid,
            center,
            velocity,
            12.0,
            ASTEROID_DAMAGE_SCALAR,
            Color::DIM_GREY,
        );
        let life = asteroid.life;
        asteroid.translate_life(-life);

        assert!(asteroid.dead);
        assert_close(asteroid.breakup_radius, 12.0);
        state.debris.push(asteroid);

        spawn_debris_breakup_fragments(&mut state);

        assert!(state.debris[0].fragmented);
        assert_eq!(state.debris.len(), 4);
        for fragment in &state.debris[1..] {
            assert_eq!(fragment.kind, DebrisKind::Fragment);
            assert!(fragment.fragment_shape.is_some());
            assert_close(
                (fragment.velocity - velocity).length(),
                BREAKUP_FRAGMENT_SPEED,
            );
        }

        remove_finished_debris(&mut state);

        assert_eq!(state.debris.len(), 3);
        assert!(
            state
                .debris
                .iter()
                .all(|debris| debris.kind == DebrisKind::Fragment && !debris.dead)
        );
    }

    #[test]
    fn render_frame_includes_fragment_polygon() {
        let mut state = init_deathmatch();
        let before = polygon_primitive_count(&SpacewarsScenario::render_frame(&state));
        state.debris.push(DebrisState::new_fragment(
            Vec2::new(400.0, 450.0),
            [
                Vec2::new(0.0, 2.0),
                Vec2::new(-1.7320508, -1.0),
                Vec2::new(1.7320508, -1.0),
            ],
            Vec2::ZERO,
            0.0,
            Color::DIM_GREY,
        ));

        let frame = SpacewarsScenario::render_frame(&state);

        assert_eq!(polygon_primitive_count(&frame), before + 1);
    }

    #[test]
    fn step_spawns_asteroids_deterministically_from_seed() {
        let mut first = init_deathmatch();
        let mut replay = init_deathmatch();
        let mut different = SpacewarsScenario::init(SpacewarsConfig::deathmatch(), 124);

        step(&mut first, &[]);
        step(&mut replay, &[]);
        step(&mut different, &[]);

        assert_eq!(first.debris.len(), 1);
        assert_eq!(first.debris, replay.debris);
        assert_ne!(first.debris, different.debris);

        let asteroid = first.debris[0];
        let universe_radius = first.config.universe_radius as f32;
        assert_eq!(asteroid.kind, DebrisKind::Asteroid);
        assert_close(
            asteroid
                .position
                .distance_to(universe_center(universe_radius)),
            universe_radius * ASTEROID_SPAWN_RADIUS_FACTOR,
        );
        assert!(asteroid.radius >= ASTEROID_MIN_RADIUS);
        assert!(asteroid.velocity.length() <= ASTEROID_MAX_SPEED);
        assert_eq!(asteroid.damage_scalar, ASTEROID_DAMAGE_SCALAR);
        assert!(!asteroid.dead);
    }

    #[test]
    fn asteroid_spawn_respects_zero_probability() {
        let mut config = SpacewarsConfig::deathmatch();
        config.asteroid_probability_per_sec = 0.0;
        let mut state = SpacewarsScenario::init(config, 123);

        step(&mut state, &[]);

        assert!(state.debris.is_empty());
    }

    #[test]
    fn asteroid_gravity_runs_every_seventh_frame() {
        let mut config = SpacewarsConfig::deathmatch();
        config.asteroid_probability_per_sec = 0.0;
        let mut state = SpacewarsScenario::init(config, 123);
        let body_radius = 10.0;
        let body_mass = body_mass(body_radius);
        let debris_position = universe_center(state.config.universe_radius as f32);
        state.sun = Some(SunState {
            position: debris_position + Vec2::new(100.0, 0.0),
            radius: body_radius,
            mass: body_mass,
            color: Color::YELLOW,
        });
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            debris_position,
            Vec2::ZERO,
            5.0,
            ASTEROID_DAMAGE_SCALAR,
            Color::DIM_GREY,
        ));

        step(&mut state, &[]);
        let expected_velocity =
            Vec2::X * gravity_acceleration_attracted_to(body_mass, 100.0, ASTEROID_GRAVITY_SCALE);
        assert_vec_close(state.debris[0].velocity, expected_velocity);

        step(&mut state, &[]);

        assert_vec_close(state.debris[0].velocity, expected_velocity);
    }

    #[test]
    fn debris_body_collision_bounces_and_applies_original_damage_pair() {
        let mut state = init_deathmatch();
        state.sun = Some(SunState {
            position: Vec2::ZERO,
            radius: 10.0,
            mass: 0.0,
            color: Color::YELLOW,
        });
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            Vec2::new(12.0, 0.0),
            Vec2::new(-20.0, 0.0),
            5.0,
            ASTEROID_DAMAGE_SCALAR,
            Color::DIM_GREY,
        ));
        let start_life = state.debris[0].life;

        let collisions = resolve_debris_body_collisions(&mut state);

        assert_eq!(
            collisions,
            vec![DebrisBodyCollision {
                debris: 0,
                body: BodyId::Sun,
            }]
        );
        assert_eq!(state.debris[0].position, Vec2::new(15.0, 0.0));
        assert_eq!(state.debris[0].velocity, Vec2::new(10.0, 0.0));
        assert_close(
            state.debris[0].life,
            start_life - 10.0 * DEBRIS_BODY_DAMAGE_SCALAR - 10.0 * PLANET_DAMAGE_SCALAR,
        );
    }

    #[test]
    fn cleanup_removes_dead_and_out_of_bounds_debris() {
        let mut state = init_deathmatch();
        let center = universe_center(state.config.universe_radius as f32);
        let mut dead = DebrisState::new(
            DebrisKind::Asteroid,
            center,
            Vec2::ZERO,
            5.0,
            ASTEROID_DAMAGE_SCALAR,
            Color::DIM_GREY,
        );
        dead.dead = true;
        state.debris.push(dead);
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            center + Vec2::new(state.config.universe_radius as f32 + 20.0, 0.0),
            Vec2::ZERO,
            5.0,
            ASTEROID_DAMAGE_SCALAR,
            Color::DIM_GREY,
        ));
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            center,
            Vec2::ZERO,
            5.0,
            ASTEROID_DAMAGE_SCALAR,
            Color::DIM_GREY,
        ));

        remove_finished_debris(&mut state);

        assert_eq!(state.debris.len(), 1);
        assert_eq!(state.debris[0].position, center);
    }

    #[test]
    fn render_frame_contains_world_two_ships_and_labels() {
        let state = init_deathmatch();
        let frame = SpacewarsScenario::render_frame(&state);
        let star_count = state
            .starfield
            .as_ref()
            .expect("deathmatch should create a starfield")
            .stars
            .len();

        let circles = frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Circle(_)))
            .count();
        let polygons = frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Polygon(_)))
            .count();
        let text = frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Text(_)))
            .count();
        let labels = frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter_map(|primitive| match primitive {
                RenderPrimitive::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(frame.camera.center, RenderPoint::new(300.0, 300.0));
        assert_eq!(circles, 1);
        assert_eq!(polygons, 12 + star_count);
        assert_eq!(text, 2);
        assert_eq!(labels, ["Player 1 50.0", "Player 2 50.0"]);
    }

    #[test]
    fn render_frame_includes_visible_debris_circle() {
        let mut state = init_deathmatch();
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            Vec2::new(400.0, 450.0),
            Vec2::ZERO,
            5.0,
            0.01,
            Color::WHITE,
        ));

        let frame = SpacewarsScenario::render_frame(&state);
        let circles = frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Circle(_)))
            .count();

        assert_eq!(circles, 2);
    }

    #[test]
    fn player_cameras_center_on_each_ship_with_equal_zoom() {
        let state = init_deathmatch();
        let player_1 = player_camera(&state, 0);
        let player_2 = player_camera(&state, 1);

        assert_eq!(player_1.center, render_point(state.ships[0].position));
        assert_eq!(player_2.center, render_point(state.ships[1].position));
        assert_eq!(player_1.height, PLAYER_VIEW_HEIGHT);
        assert_eq!(player_2.height, PLAYER_VIEW_HEIGHT);
    }

    #[test]
    fn player_render_frames_use_per_player_cameras() {
        let state = init_deathmatch();
        let frames = SpacewarsScenario::render_player_frames(&state);

        assert_eq!(frames.len(), 2);
        assert_eq!(
            frames[0].camera.center,
            render_point(state.ships[0].position)
        );
        assert_eq!(
            frames[1].camera.center,
            render_point(state.ships[1].position)
        );
        assert_eq!(frames[0].camera.height, PLAYER_VIEW_HEIGHT);
        assert_eq!(frames[1].camera.height, PLAYER_VIEW_HEIGHT);
        assert_eq!(
            frames[0].layers,
            SpacewarsScenario::render_frame(&state).layers
        );
    }

    #[test]
    fn render_frame_draws_starfield_behind_world() {
        let state = init_deathmatch();
        let star_count = state
            .starfield
            .as_ref()
            .expect("deathmatch should create a starfield")
            .stars
            .len();
        let frame = SpacewarsScenario::render_frame(&state);
        let starfield_layer = frame
            .layers
            .iter()
            .find(|layer| layer.z == STARFIELD_LAYER)
            .expect("starfield should render on its own layer");

        assert_eq!(
            frame.ordered_layers().first().map(|layer| layer.z),
            Some(STARFIELD_LAYER)
        );
        assert_eq!(starfield_layer.primitives.len(), star_count);
        assert!(
            starfield_layer
                .primitives
                .iter()
                .all(|primitive| matches!(primitive, RenderPrimitive::Polygon(_)))
        );
    }

    #[test]
    fn render_frame_contains_default_sun_and_planets() {
        let state = init_default(123);
        let frame = SpacewarsScenario::render_frame(&state);
        let star_count = state
            .starfield
            .as_ref()
            .expect("default config should create a starfield")
            .stars
            .len();

        let circles = frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Circle(_)))
            .count();
        let polygons = polygon_primitive_count(&frame);

        assert_eq!(frame.camera.center, RenderPoint::new(1200.0, 1200.0));
        assert_eq!(circles, 2 + state.planets.len());
        assert_eq!(polygons, 12 + state.planets.len() + star_count);
    }
}
