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
    constants::{
        COLLISION_TRANSLATION_SCALAR, DEFAULT_ELASTICITY, PLANET_DAMAGE_SCALAR, REALLY_SMALL,
    },
    physics::gravity_acceleration_attracted_to,
    rng::{SpacewarsRng, random_range_f32, random_unit_f32, seeded_rng},
    triangle_high_bounds, triangle_low_bound,
};

const WORLD_LAYER: i32 = -20;
const SUN_LAYER: i32 = -15;
const PLANET_LAYER: i32 = -10;
const SPACEPORT_LAYER: i32 = -5;
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
    pub debris: Vec<DebrisState>,
    pub sun: Option<SunState>,
    pub planets: Vec<PlanetState>,
    pub ship_collisions: Vec<ShipCollision>,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebrisState {
    pub kind: DebrisKind,
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
    pub rotation_radians: f32,
    pub omega: f32,
    pub damage_scalar: f32,
    pub life: f32,
    pub life_max: f32,
    pub dead: bool,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebrisKind {
    Asteroid,
    Fragment,
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
    pub life: f32,
    pub life_max: f32,
    pub dead: bool,
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

        SpacewarsState {
            config,
            seed,
            tick: 0,
            players,
            ships,
            debris: Vec::new(),
            sun,
            planets,
            ship_collisions: Vec::new(),
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
            ship.update(dt);
            contain_ship(ship, state.config.universe_radius as f32);
        }
        for debris in &mut state.debris {
            debris.update(dt);
        }

        apply_world_gravity(state);
        state.ship_collisions = resolve_ship_collisions(state);
        let collision_events = resolve_body_collisions(state);
        state.body_collisions = collision_events.body_collisions;
        state.spaceport_contacts = collision_events.spaceport_contacts;

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
    let bodies = body_physics(state);

    for ship in &mut state.ships {
        for body in &bodies {
            apply_gravity(ship, body.position, body.mass, 1.0);
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
            mass: SHIP_MASS,
            low: ship_low_bounds(&triangles),
        }
    }
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
            let ship = &mut state.ships[contact.ship];
            resolve_ship_body_collision(
                ship,
                contact.body_position,
                contact.body_radius,
                contact.ship_radius,
            );
            apply_body_collision_damage(ship);
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

fn apply_body_collision_damage(ship: &mut ShipState) -> f32 {
    let damage = ship.velocity.length() * PLANET_DAMAGE_SCALAR;
    ship.translate_life(-damage);
    damage
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
            rotation_radians: 0.0,
            omega: 0.0,
            damage_scalar,
            life,
            life_max: life,
            dead: false,
            color,
        }
    }

    pub fn mass(self) -> f32 {
        debris_mass(self.radius)
    }

    pub fn update(&mut self, dt: f32) {
        self.rotation_radians += self.omega * dt;
        self.position += self.velocity * dt;
    }

    pub fn translate_life(&mut self, delta: f32) {
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
            life,
            life_max: life,
            dead: false,
            turn_power: SHIP_TURN_FORCE / SHIP_MASS * delta_time,
            thrust_power: SHIP_THRUST_FORCE / SHIP_MASS * delta_time,
            current_max_omega: BASE_MAX_OMEGA,
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

    fn translate_life(&mut self, delta: f32) {
        self.life += delta;
        if self.life <= 0.0 {
            self.dead = true;
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
        render_ship(&mut frame, ship);
    }

    for debris in &state.debris {
        render_debris(&mut frame, debris);
    }

    for ship in &state.ships {
        render_ship_label(&mut frame, state, ship);
    }

    frame
}

fn render_debris(frame: &mut RenderFrame, debris: &DebrisState) {
    if debris.dead || debris.radius <= 0.0 {
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
        assert_eq!(state.ships[0].life, 50.0);
        assert_eq!(state.ships[0].life_max, 50.0);
        assert_eq!(state.ships[1].life, 50.0);
        assert_eq!(state.ships[1].life_max, 50.0);
        assert!(!state.ships[0].dead);
        assert!(!state.ships[1].dead);
        assert_eq!(state.players[0].name, "Player 1");
        assert_eq!(state.players[1].name, "Player 2");
        assert!(state.sun.is_none());
        assert!(state.planets.is_empty());
        assert!(state.ship_collisions.is_empty());
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
        assert!(state.ship_collisions.is_empty());
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
        assert_eq!(polygons, 12);
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
    fn render_frame_contains_default_sun_and_planets() {
        let state = init_default(123);
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

        assert_eq!(frame.camera.center, RenderPoint::new(1200.0, 1200.0));
        assert_eq!(circles, 2 + state.planets.len());
        assert_eq!(polygons, 12 + state.planets.len());
    }
}
