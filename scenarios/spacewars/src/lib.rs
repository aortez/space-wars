//! Initial Spacewars scenario port.
//!
//! M11a adds the first world slice on top of M10 ship controls: deterministic
//! sun/planet setup and simple circle rendering. Orbit advancement, gravity,
//! exhaust trails, weapons, collisions, sounds, asteroids, pods, and scoring
//! land in later slices.

use std::time::Duration;

use engine_common::{
    Action, Camera2, Fill, Observation, RenderCircle, RenderColor, RenderFrame, RenderPoint,
    RenderPolygon, RenderPrimitive, RenderText, Scenario, StepResult, Stroke, TextAnchor,
    TickModel,
};
use engine_core::{
    Bounds2, BoundsList, Circle, Color, PlayerConfig, SpacewarsConfig, Transform2, Vec2,
    physics::gravity_acceleration_attracted_to,
    rng::{SpacewarsRng, random_range_f32, random_unit_f32, seeded_rng},
    triangle_high_bounds, triangle_low_bound,
};

const WORLD_LAYER: i32 = -20;
const SUN_LAYER: i32 = -15;
const PLANET_LAYER: i32 = -10;
const SHIP_LAYER: i32 = 0;
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

const SHIP_THRUST_FORCE: f32 = 50_000.0;
const SHIP_TURN_FORCE: f32 = 200.0;
const SHIP_MASS: f32 = 31.25;
const BASE_MAX_OMEGA: f32 = 1.0;
const MAX_SPEED: f32 = 150.0;
const WING_DELTA_SPEED: f32 = 5.0;
const WING_CLOSED_SPEED: f32 = MAX_SPEED * 5.0;
const WING_CLOSED_MAX_OMEGA: f32 = BASE_MAX_OMEGA * 0.25;
const MAX_WING_THETA: f32 = core::f32::consts::FRAC_PI_4;
const SHIP_BOUNDS_RADIUS: f32 = 6.0;
const SHIP_BODY_TRIANGLE_INDEX: usize = 4;

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
const SHIP_PIVOT: Vec2 = Vec2::new(2.5, 3.5);
const SHIP_WING_PIVOT: Vec2 = Vec2::new(2.5, 2.0);

pub struct SpacewarsScenario;

#[derive(Debug, Clone)]
pub struct SpacewarsState {
    pub config: SpacewarsConfig,
    pub seed: u64,
    pub tick: u64,
    pub players: [PlayerState; 2],
    pub ships: [ShipState; 2],
    pub sun: Option<SunState>,
    pub planets: Vec<PlanetState>,
    pub body_collisions: Vec<BodyCollision>,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShipState {
    pub owner_id: usize,
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
    turn_power: f32,
    thrust_power: f32,
    current_max_omega: f32,
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
                delta_time,
            ),
            ShipState::new(
                1,
                Vec2::new(375.0, 500.0),
                config.players[1].color,
                delta_time,
            ),
        ];
        let (sun, planets) = build_world(&config, seed);

        SpacewarsState {
            config,
            seed,
            tick: 0,
            players,
            ships,
            sun,
            planets,
            body_collisions: Vec::new(),
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
            ship.update(dt);
            contain_ship(ship, state.config.universe_radius as f32);
        }

        state.body_collisions = detect_body_collisions(state);
        apply_world_gravity(state);

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

fn random_color(rng: &mut SpacewarsRng) -> Color {
    Color::rgb(
        random_unit_f32(rng),
        random_unit_f32(rng),
        random_unit_f32(rng),
    )
}

fn body_mass(radius: f32) -> f32 {
    core::f32::consts::PI * radius * radius * PLANET_MASS_DENSITY
}

fn apply_world_gravity(state: &mut SpacewarsState) {
    for ship in &mut state.ships {
        if let Some(sun) = state.sun {
            apply_gravity(ship, sun.position, sun.mass, 1.0);
        }

        for planet in &state.planets {
            apply_gravity(ship, planet.position, planet.mass, 1.0);
        }
    }
}

fn apply_gravity(ship: &mut ShipState, attractor_position: Vec2, attractor_mass: f32, scale: f32) {
    let offset = attractor_position - ship.position;
    let distance = offset.length();
    let acceleration = gravity_acceleration_attracted_to(attractor_mass, distance, scale);
    ship.velocity += offset.normalized() * acceleration;
}

#[derive(Debug, Clone, Copy)]
struct BodyBounds {
    id: BodyId,
    low: Circle,
    high: Circle,
}

fn detect_body_collisions(state: &SpacewarsState) -> Vec<BodyCollision> {
    let bodies = body_bounds(state);
    let mut collisions = Vec::new();

    for (ship_index, ship) in state.ships.iter().enumerate() {
        let triangles = ship_triangles(ship);
        let ship_low = Bounds2::Circle(ship_low_bounds(&triangles));
        let ship_high = Bounds2::List(ship_high_bounds(&triangles));

        for body in &bodies {
            if !ship_low.intersects(&Bounds2::Circle(body.low)) {
                continue;
            }

            if ship_high.intersects(&Bounds2::Circle(body.high)) {
                collisions.push(BodyCollision {
                    ship: ship_index,
                    body: body.id,
                });
            }
        }
    }

    collisions
}

fn body_bounds(state: &SpacewarsState) -> Vec<BodyBounds> {
    let mut bodies = Vec::new();

    if let Some(sun) = state.sun {
        bodies.push(BodyBounds {
            id: BodyId::Sun,
            low: body_circle(sun.position, sun.radius),
            high: body_circle(sun.position, sun.radius),
        });
    }

    bodies.extend(
        state
            .planets
            .iter()
            .enumerate()
            .map(|(index, planet)| BodyBounds {
                id: BodyId::Planet(index),
                low: body_circle(planet.position, planet.radius),
                high: body_circle(planet.position, planet.radius),
            }),
    );

    bodies
}

fn body_circle(position: Vec2, radius: f32) -> Circle {
    Circle::new(position, radius * BODY_BOUNDS_RADIUS_SCALE)
}

fn ship_low_bounds(triangles: &[[Vec2; 3]]) -> Circle {
    let center = triangle_low_bound(triangles[SHIP_BODY_TRIANGLE_INDEX]).center;
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

impl ShipState {
    fn new(owner_id: usize, position: Vec2, color: Color, delta_time: f32) -> Self {
        Self {
            owner_id,
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
            turn_power: SHIP_TURN_FORCE / SHIP_MASS * delta_time,
            thrust_power: SHIP_THRUST_FORCE / SHIP_MASS * delta_time,
            current_max_omega: BASE_MAX_OMEGA,
        }
    }

    fn close_wings(&mut self) {
        self.wing_behavior = WingBehavior::Close;
    }

    fn open_wings(&mut self) {
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
        self.laser_firing = true;
    }

    fn fire_laser_halt(&mut self) {
        self.laser_firing = false;
    }

    fn fire_cannon(&mut self) {
        self.cannon_firing = true;
    }

    fn fire_cannon_halt(&mut self) {
        self.cannon_firing = false;
    }

    fn update(&mut self, dt: f32) {
        self.rotate_ship();
        self.position += self.velocity * dt;
        self.rotation_radians += self.omega * dt;
        self.update_wings(dt);
        self.update_thrust();
        self.update_turn();
    }

    fn rotate_ship(&mut self) {
        let theta = self.rotation_radians - self.turn_power * self.omega;
        self.rotation_radians = theta;
        self.direction = direction_from_rotation(theta);
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

    fn update_thrust(&mut self) {
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
            }
        }
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
}

fn direction_from_rotation(rotation_radians: f32) -> Vec2 {
    let (sin, cos) = (rotation_radians + core::f32::consts::FRAC_PI_2).sin_cos();
    Vec2::new(cos, sin)
}

fn contain_ship(ship: &mut ShipState, universe_radius: f32) {
    let universe_center = Vec2::new(universe_radius, universe_radius);
    let ship_center = ship.position + SHIP_PIVOT;
    let offset = ship_center - universe_center;
    let max_distance = (universe_radius - SHIP_BOUNDS_RADIUS).max(0.0);
    let distance = offset.length();

    if distance > max_distance {
        let contained_center = universe_center + offset.normalized() * max_distance;
        ship.position = contained_center - SHIP_PIVOT;
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
    let mut frame = RenderFrame::new(Camera2::new(render_point(center), radius * 2.2));

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
    }

    for ship in &state.ships {
        render_ship(&mut frame, ship);
    }

    for ship in &state.ships {
        render_ship_label(&mut frame, state, ship);
    }

    frame
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

fn render_ship(frame: &mut RenderFrame, ship: &ShipState) {
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
        pivot: SHIP_PIVOT,
    }
}

fn rotate_points(points: [Vec2; 3], pivot: Vec2, radians: f32) -> [Vec2; 3] {
    points.map(|point| (point - pivot).rotate_radians(radians) + pivot)
}

fn render_ship_label(frame: &mut RenderFrame, state: &SpacewarsState, ship: &ShipState) {
    let player = &state.players[ship.owner_id];
    let mut text = RenderText::new(
        render_point(ship.position + Vec2::new(2.5, 18.0)),
        player.name.as_str(),
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

        if let Some(sun) = state.sun {
            delta += gravity_delta(ship_position, sun.position, sun.mass);
        }

        for planet in &state.planets {
            delta += gravity_delta(ship_position, planet.position, planet.mass);
        }

        delta
    }

    fn gravity_delta(ship_position: Vec2, attractor_position: Vec2, attractor_mass: f32) -> Vec2 {
        let offset = attractor_position - ship_position;
        let distance = offset.length();
        offset.normalized() * gravity_acceleration_attracted_to(attractor_mass, distance, 1.0)
    }

    fn circle_primitive_count(frame: &RenderFrame) -> usize {
        frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Circle(_)))
            .count()
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
        assert_eq!(state.ships[0].position, Vec2::new(375.0, 450.0));
        assert_eq!(state.ships[1].owner_id, 1);
        assert_eq!(state.ships[1].position, Vec2::new(375.0, 500.0));
        assert_eq!(state.players[0].name, "Player 1");
        assert_eq!(state.players[1].name, "Player 2");
        assert!(state.sun.is_none());
        assert!(state.planets.is_empty());
        assert!(state.body_collisions.is_empty());
    }

    #[test]
    fn default_config_builds_original_style_sun_and_planet_bands() {
        let state = init_default(123);
        let sun = state.sun.expect("default config should create a sun");
        let universe_radius = state.config.universe_radius as f32;

        assert_eq!(state.config.universe_radius, 1200);
        assert_eq!(sun.position, Vec2::new(universe_radius, universe_radius));
        assert_eq!(sun.radius, SUN_RADIUS);
        assert_close(sun.mass, body_mass(SUN_RADIUS));
        assert!(!state.planets.is_empty());
        assert!(state.planets.len() <= MAX_PLANETS);

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
        assert_ne!(first.planets, different.planets);
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
        let mut ship = ShipState::new(0, Vec2::new(10.0, 20.0), Color::WHITE, 1.0 / 60.0);
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
    fn step_records_body_collision_without_response_yet() {
        let mut state = init_deathmatch();
        let start_position = state.ships[0].position;
        let start_velocity = state.ships[0].velocity;
        state.sun = Some(SunState {
            position: ship_low_bounds(&ship_triangles(&state.ships[0])).center,
            radius: 10.0,
            mass: 0.0,
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
        assert_eq!(state.ships[0].position, start_position);
        assert_eq!(state.ships[0].velocity, start_velocity);
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
        let distance = (state.ships[0].position + SHIP_PIVOT - center).length();
        assert!(distance <= radius - SHIP_BOUNDS_RADIUS + EPS);
    }

    #[test]
    fn invalid_actions_are_ignored() {
        let mut state = init_deathmatch();
        let start = state.ships[0];
        let invalid = Action {
            kind: 999,
            payload: vec![0],
        };

        step(&mut state, &[invalid]);

        assert_eq!(state.ships[0], start);
    }

    #[test]
    fn render_frame_contains_world_two_ships_and_labels() {
        let state = init_deathmatch();
        let frame = SpacewarsScenario::render_frame(&state);

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

        assert_eq!(frame.camera.center, RenderPoint::new(300.0, 300.0));
        assert_eq!(circles, 1);
        assert_eq!(polygons, 12);
        assert_eq!(text, 2);
    }

    #[test]
    fn render_frame_contains_default_sun_and_planets() {
        let state = init_default(123);
        let frame = SpacewarsScenario::render_frame(&state);

        let circles = frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Circle(_)))
            .count();

        assert_eq!(frame.camera.center, RenderPoint::new(1200.0, 1200.0));
        assert_eq!(circles, 2 + state.planets.len());
    }
}
