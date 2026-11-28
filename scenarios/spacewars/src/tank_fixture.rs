//! Shared, source-independent planetary tank acceptance script.
//! Timing belongs to callers; sampling, edits, replay and rendering are outside it.
use std::time::Duration;

use engine_common::{DirtMode, RenderFrame};
use engine_core::Vec2;
use engine_terrain::{Brush, EditMode, TerrainEdit};
use serde::Serialize;

use crate::{
    Allegiance, PlayerId, SpacewarsScenario, SpacewarsState,
    tanks::{
        ENDED_SHOTS, ShotBody, ShotLaunch, ShotOutcome, ShotStop, TankCommand, TankDamageCause,
        TankId, TankShot, TankShotId,
    },
};
use engine_rapier::cannon;

pub const FIXTURE_VERSION: &str = "planetary-tank-237-v3";
pub const WARMUP_STEPS: usize = 120;
pub const MEASURED_STEPS: usize = 600;
/// How fast the script carries the planet along world +X during the
/// measured steps.
const PLANET_DRIFT: f32 = 0.3;
/// The steps the firing case takes as it is built: one before each of its
/// three tanks is placed, and the warmup.
const FIRE_BUILD_STEPS: usize = WARMUP_STEPS + 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureCase {
    Drive,
    Cut,
    /// A government tank fires [`FIRE_SCRIPT`] at two players' tanks, a
    /// landed ship and a flying one (#239).
    Fire,
}

/// What a shot of the firing script is meant for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireTarget {
    /// The first player's tank.
    FirstTank,
    /// The second player's tank.
    SecondTank,
    /// The first seat's ship, landed beyond the firing tank's horizon.
    LandedShip,
    /// The second seat's ship, which the script carries over the planet.
    FlyingShip,
}

/// One shot of the firing script.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FireOrder {
    /// The measured step on which the tank pulls its trigger.
    pub tick: usize,
    /// How far the barrel aims above the horizontal, in degrees.
    pub elevation: f32,
    pub facing: f32,
    pub power: f32,
    pub target: FireTarget,
    /// Whether the round is to strike its target. Otherwise the ground
    /// between is to stop it.
    pub strikes: bool,
}

/// The firing case's script. Each aim was found by scanning and has room on
/// both sides. At these elevations the lobs at the tanks strike from power 12
/// to 14.4 and from 16.8 to 19, the lob at the landed ship from 23 to 25, and
/// the shot at the flying ship from 29 to 43. The flat shot comes down 20
/// units or more short of the landed ship at any power up to 22, and reaches
/// it at 26 to 28.
pub const FIRE_SCRIPT: [FireOrder; 5] = [
    FireOrder {
        tick: 0,
        elevation: 45.0,
        facing: 1.0,
        power: 13.2,
        target: FireTarget::FirstTank,
        strikes: true,
    },
    FireOrder {
        tick: 75,
        elevation: 45.0,
        facing: 1.0,
        power: 18.3,
        target: FireTarget::SecondTank,
        strikes: true,
    },
    FireOrder {
        tick: 150,
        elevation: 5.0,
        facing: -1.0,
        power: 20.0,
        target: FireTarget::LandedShip,
        strikes: false,
    },
    FireOrder {
        tick: 225,
        elevation: 40.0,
        facing: -1.0,
        power: 24.0,
        target: FireTarget::LandedShip,
        strikes: true,
    },
    FireOrder {
        tick: 300,
        elevation: 33.0,
        facing: 1.0,
        power: 35.0,
        target: FireTarget::FlyingShip,
        strikes: true,
    },
];

/// Where the firing case's cast is put, in units along the planet's surface
/// clockwise of the top the planet has as the fixture is built: each
/// player's tank, and the first seat's ship. The firing tank is the driving
/// cases' first, 4 units the other way. A ship is put by its position, which
/// is about 4 units from its hull's center of mass, so the landed ship's hull
/// stands about 56 units round.
const FIRST_TANK_ARC: f32 = 10.75;
const SECOND_TANK_ARC: f32 = 22.75;
const LANDED_SHIP_ARC: f32 = -60.0;
/// The flying ship's path: how far from the planet's center its position is
/// carried, how fast it goes round, in radians a second clockwise, and how
/// far clockwise of world up it is on measured step [`FLIGHT_PASS_STEP`]. Its
/// hull's center of mass rides about 4 units farther out.
const FLIGHT_RADIUS: f32 = 80.0;
const FLIGHT_RATE: f32 = -0.1;
const FLIGHT_PASS_ANGLE: f32 = 0.1;
const FLIGHT_PASS_STEP: f32 = 345.0;

/// The cannon's settings and the firing case's cast and script, for the
/// record of a run. Each number reads as it is written.
pub fn fire_settings() -> serde_json::Value {
    let plain = |value: f32| value.to_string().parse::<f64>().unwrap();
    let script = FIRE_SCRIPT.iter().map(|order| {
        serde_json::json!({
            "step": order.tick,
            "elevation_degrees": plain(order.elevation),
            "facing": plain(order.facing),
            "power": plain(order.power),
            "target": format!("{:?}", order.target),
            "strikes": order.strikes,
        })
    });
    let elevations = [cannon::MIN_ELEVATION, cannon::MAX_ELEVATION];
    let push = cannon::BLAST.push;
    serde_json::json!({
        "cannon": {
            "elevation_degrees": elevations.map(|angle| plain(angle.to_degrees())),
            "power": [plain(cannon::MIN_POWER), plain(cannon::MAX_POWER)],
            "reload_ticks": cannon::RELOAD_TICKS,
            "round_lifetime_ticks": cannon::ROUND_LIFETIME_TICKS,
            "clearance_ticks": cannon::CLEARANCE_TICKS,
            "rounds_per_cannon": cannon::MAX_ROUNDS_PER_CANNON,
            "rounds": cannon::MAX_ROUNDS,
            "pivot_height": plain(cannon::PIVOT_HEIGHT),
            "barrel_length": plain(cannon::BARREL_LENGTH),
            "round_radius": plain(cannon::ROUND_RADIUS),
            "round_mass": plain(cannon::ROUND_MASS),
            "casing_volume": plain(cannon::CASING_VOLUME),
            "payload": plain(cannon::PAYLOAD),
            "blast_radius": plain(cannon::BLAST.radius),
            "blast_height": plain(cannon::BLAST.height),
            "blast_push_radius": push.map(|push| plain(push.radius)),
            "blast_push_speed": push.map(|push| plain(push.speed)),
        },
        "cast": {
            "first_tank_arc": plain(FIRST_TANK_ARC),
            "second_tank_arc": plain(SECOND_TANK_ARC),
            "landed_ship_arc": plain(LANDED_SHIP_ARC),
            "flight_radius": plain(FLIGHT_RADIUS),
            "flight_rate": plain(FLIGHT_RATE),
            "flight_pass_angle": plain(FLIGHT_PASS_ANGLE),
            "flight_pass_step": plain(FLIGHT_PASS_STEP),
            "planet_drift": plain(PLANET_DRIFT),
        },
        "script": script.collect::<Vec<_>>(),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TankSample {
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub angle: f32,
    pub spin: f32,
    pub wheel_positions: [[f32; 2]; 2],
    pub wheel_velocities: [[f32; 2]; 2],
    pub supported: bool,
    pub relative_speed: Option<f32>,
    pub blocked: bool,
    pub elevation: f32,
    pub facing: f32,
}

/// A tank's round in flight.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoundSample {
    /// Its shot's place in the order the shots were fired.
    pub shot: usize,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
}

/// What the firing case adds to a step's sample.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FireSample {
    pub planet: [f32; 2],
    pub rounds: Vec<RoundSample>,
    /// Each tank's health, in the order of the sample's tanks.
    pub health: Vec<f32>,
    /// Each ship's position, velocity and life.
    pub ships: [[f32; 5]; 2],
    /// The shots the firing tank has records of, each by its place in the
    /// order the shots were fired.
    pub records: Vec<usize>,
    /// How many shots have ended so far.
    pub ended: usize,
    /// How many craters rounds have dug so far.
    pub craters: u64,
    /// How many casts along a barrel the step made.
    pub casts: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FixtureSample {
    pub tick: usize,
    pub tanks: Vec<TankSample>,
    pub pilot_motion: [f32; 6],
    pub queries: usize,
    pub round_grains: usize,
    pub angular_grains: usize,
    /// Includes pairs/manifolds even when they yield no contacts.
    pub contact_traversal: usize,
    pub spatial_traversal: usize,
    pub spatial_exhaustions: usize,
    pub physics_steps: u64,
    pub gravity_bodies: usize,
    pub gravity_applications: usize,
    pub duplicate_gravity: usize,
    pub bodies: usize,
    pub colliders: usize,
    pub joints: usize,
    pub material_cells: u64,
    pub removed_cells: u64,
    pub world_motion_hash: String,
    /// The firing case's part, which the driving cases leave out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fire: Option<FireSample>,
}

/// A shot the firing case fired.
#[derive(Debug, Clone, PartialEq)]
struct FiredShot {
    id: TankShotId,
    /// The measured step it was fired on.
    tick: usize,
    launch: ShotLaunch,
    /// Whether the planet's ground stood on the straight line from where the
    /// round started to its target.
    sight_blocked: bool,
    /// Its record, and the measured step it ended on.
    ended: Option<(usize, TankShot)>,
}

pub struct TankFixture {
    pub state: SpacewarsState,
    ids: Vec<TankId>,
    pilot: crate::surface_sortie::SurfacePilot,
    case: FixtureCase,
    tick: usize,
    initial_material: u64,
    initial_physics_steps: u64,
    cut_start: Option<(Vec2, Vec2)>,
    planned_edits: Vec<(usize, Vec2, u32)>,
    recorded_edits: Vec<(usize, Vec2, u32)>,
    /// The firing case's script, and the shots fired under it.
    script: Vec<FireOrder>,
    fired: Vec<FiredShot>,
    initial_craters: u64,
}

impl TankFixture {
    pub fn new(seed: u64, shape: DirtMode, case: FixtureCase) -> Self {
        let count = if case == FixtureCase::Fire { 3 } else { 2 };
        Self::with_tanks(seed, shape, case, count)
    }

    /// Zero-tank controls use the exact same local cut schedule obtained from a
    /// separate untimed two-tank rehearsal. Neither rehearsal nor warmup is timed.
    pub fn with_tanks(seed: u64, shape: DirtMode, case: FixtureCase, count: usize) -> Self {
        if case == FixtureCase::Fire {
            // The firing case cuts nothing, so it has no schedule to rehearse,
            // and without its cast of three tanks it has nothing to fire.
            assert_eq!(count, 3);
            return Self::build(seed, shape, case, count);
        }
        assert!(count == 0 || count == 2);
        let mut rehearsal = Self::build(seed, shape, case, 2);
        for _ in 0..MEASURED_STEPS {
            rehearsal.prepare_step();
            rehearsal.step();
            rehearsal.sample();
        }
        let mut fixture = Self::build(seed, shape, case, count);
        fixture.planned_edits = rehearsal.recorded_edits;
        fixture
    }

    fn build(seed: u64, shape: DirtMode, case: FixtureCase, count: usize) -> Self {
        let fire = case == FixtureCase::Fire;
        let mut state = SpacewarsScenario::init_tank_proving_ground(seed);
        if fire {
            // The first seat's ship comes down beyond the firing tank's
            // horizon and stands there on its tail, held by nothing but the
            // ordinary rules.
            let planet = state.planets[0];
            let (above, down) = Self::above(&state, LANDED_SHIP_ARC, 5.0);
            let ship = &mut state.ships[0];
            ship.position = above;
            ship.velocity = crate::planet_surface_velocity(&planet, above);
            ship.direction = -down;
            ship.rotation_radians = crate::rotation_for_direction(-down);
        } else {
            // Keep ordinary ships visible but out of the isolated driving route.
            state.ships[0].position = Vec2::new(700.0, 700.0);
            state.ships[1].position = Vec2::new(800.0, 500.0);
        }
        state
            .enable_loose_terrain(crate::surface_sortie::dirt_config(shape))
            .unwrap();
        let mut pilot = crate::surface_sortie::SurfacePilot::tank_comparison(&mut state);
        // The firing case carries its flying ship through every step, the
        // ones before the first measured step too.
        let mut steps = 0;
        let mut host = |state: &mut SpacewarsState, pilot: &mut _| {
            if fire {
                // The planet stays where it is until the measured steps.
                let step = steps as f32 - FIRE_BUILD_STEPS as f32;
                Self::carry_flying_ship(state, step, Vec2::ZERO);
            }
            steps += 1;
            Self::host_step(state, pilot);
        };
        host(&mut state, &mut pilot);
        let mut ids = Vec::new();
        if count > 0 {
            // Start on the real plateau: the original bearing put the route
            // footprint across a one-cell drop above the tank step limit.
            ids.push(
                state
                    .place_tank(
                        Vec2::new(496.0, 565.0),
                        -Vec2::Y,
                        Allegiance::INITIAL_GOVERNMENT,
                    )
                    .expect("top admission"),
            );
        }
        host(&mut state, &mut pilot);
        if fire {
            // A tank of each player's, clockwise of the firing tank.
            let targets = [
                (FIRST_TANK_ARC, PlayerId::PLAYER_1),
                (SECOND_TANK_ARC, PlayerId::PLAYER_2),
            ];
            for (at, (arc, player)) in targets.into_iter().enumerate() {
                if at > 0 {
                    host(&mut state, &mut pilot);
                }
                let (above, down) = Self::above(&state, arc, 5.0);
                ids.push(
                    state
                        .place_tank(above, down, Allegiance::Player(player))
                        .expect("target admission"),
                );
            }
        } else if count == 2 {
            ids.push(
                state
                    .place_tank(
                        Vec2::new(435.0, 500.0),
                        Vec2::X,
                        Allegiance::INITIAL_GOVERNMENT,
                    )
                    .expect("side admission"),
            );
        }
        for &id in &ids {
            assert!(state.command_tank(
                id,
                TankCommand {
                    brake: true,
                    ..Default::default()
                }
            ));
        }
        for _ in 0..WARMUP_STEPS {
            host(&mut state, &mut pilot);
        }
        let d = state.terrain_diagnostics();
        let initial_physics_steps = state.tank_step_audit().physics_steps;
        let initial_craters = state.terrain_cannon_hits();
        Self {
            state,
            ids,
            pilot,
            case,
            tick: 0,
            initial_physics_steps,
            initial_material: d.occupied_cells + d.loose_cells as u64 + d.removed_cells,
            cut_start: None,
            planned_edits: Vec::new(),
            recorded_edits: Vec::new(),
            script: if fire {
                FIRE_SCRIPT.to_vec()
            } else {
                Vec::new()
            },
            fired: Vec::new(),
            initial_craters,
        }
    }

    /// The point `height` above the planet's surface, `arc` units clockwise of
    /// the top the planet had before it turned, and the way down there.
    fn above(state: &SpacewarsState, arc: f32, height: f32) -> (Vec2, Vec2) {
        let planet = state.planets[0];
        let up = Vec2::Y.rotate_radians(planet.wrapper_angle - arc / planet.radius);
        (planet.position + up * (planet.radius + height), -up)
    }

    /// Carries the second seat's ship to where its path round the planet has
    /// it on measured step `step`, moving as the path does over a planet that
    /// moves at `drift`.
    fn carry_flying_ship(state: &mut SpacewarsState, step: f32, drift: Vec2) {
        let planet = state.planets[0];
        let angle = FLIGHT_PASS_ANGLE + FLIGHT_RATE * (step - FLIGHT_PASS_STEP) / 60.0;
        let out = Vec2::Y.rotate_radians(-angle);
        let along = Vec2::new(out.y, -out.x) * FLIGHT_RATE.signum();
        let ship = &mut state.ships[1];
        ship.position = planet.position + out * FLIGHT_RADIUS;
        ship.velocity = drift + along * (FLIGHT_RADIUS * FLIGHT_RATE.abs());
        ship.direction = along;
        ship.rotation_radians = crate::rotation_for_direction(along);
        ship.omega = 0.0;
    }

    fn host_step(state: &mut SpacewarsState, pilot: &mut crate::surface_sortie::SurfacePilot) {
        SpacewarsScenario::step_with_surface_pilots(
            state,
            &[],
            Duration::from_secs_f32(1.0 / 60.0),
            std::slice::from_mut(pilot),
            None,
            None,
            true,
        );
    }

    /// Script changes happen before the timed host step. Terrain cuts use actual
    /// local material coordinates, not a remembered circular support radius.
    pub fn prepare_step(&mut self) {
        if self.case == FixtureCase::Fire {
            self.prepare_fire();
            return;
        }
        let drive = self.tick < 120 || (240..420).contains(&self.tick);
        for (index, &id) in self.ids.iter().enumerate() {
            assert!(self.state.command_tank(
                id,
                TankCommand {
                    drive: if index == 0 && drive { 1.0 } else { 0.0 },
                    brake: index == 1 || !drive,
                    elevation: if index == 0 { 0.3 } else { 0.4 },
                    facing: if index == 0 { 1.0 } else { -1.0 },
                    ..TankCommand::default()
                }
            ));
        }
        if let Some((_, local, radius)) = self
            .planned_edits
            .iter()
            .find(|e| e.0 == self.tick)
            .copied()
        {
            if self.tick == 420 && !self.ids.is_empty() {
                let position = self
                    .state
                    .tank_snapshot(self.ids[0])
                    .unwrap()
                    .assembly
                    .chassis
                    .position;
                self.cut_start = Some((
                    position,
                    (position - self.state.planets[0].position).normalized(),
                ));
            }
            self.queue_local_cut(local, radius);
        }
        if self.tick == 240 && self.planned_edits.is_empty() {
            let s = self.state.tank_snapshot(self.ids[0]).unwrap();
            let support = s.support.expect("ground contact before gap");
            self.cut_at(
                support.position + Vec2::new(support.normal.y, -support.normal.x) * 7.0,
                3,
            );
        }
        if self.tick == 420 && self.case == FixtureCase::Cut && self.planned_edits.is_empty() {
            let s = self.state.tank_snapshot(self.ids[0]).unwrap();
            let position = s.assembly.chassis.position;
            let radial = (position - self.state.planets[0].position).normalized();
            self.cut_start = Some((position, radial));
            self.cut_at(position, 10);
        }
        if self.tick == 480 {
            // Away from the driving routes: exercise actual released dirt shapes
            // without letting falling traffic contaminate drive/brake measurements.
            self.state
                .queue_fixture_release(0, Vec2::new(-20.0, -50.0), 3)
                .unwrap();
        }
        // Prescribed translation is shared with the host; no assembly transport.
        self.state.planets[0].position += Vec2::X * (PLANET_DRIFT / 60.0);
    }

    /// The firing case's step. The firing tank holds the aim of its next
    /// shot, or of its last, and pulls its trigger on the shot's step. The
    /// planet is carried along as in the driving cases, and the flying ship
    /// along its path.
    fn prepare_fire(&mut self) {
        let next = self.script.iter().find(|order| order.tick >= self.tick);
        if let Some(order) = next.or(self.script.last()).copied() {
            let command = TankCommand {
                brake: true,
                elevation: order.elevation.to_radians(),
                facing: order.facing,
                power: order.power,
                fire: order.tick == self.tick,
                target: Some(self.fire_target(order.target)),
                ..TankCommand::default()
            };
            assert!(self.state.command_tank(self.ids[0], command));
        }
        self.state.planets[0].position += Vec2::X * (PLANET_DRIFT / 60.0);
        Self::carry_flying_ship(&mut self.state, self.tick as f32, Vec2::X * PLANET_DRIFT);
    }

    /// The body a target of the firing script is.
    fn fire_target(&self, target: FireTarget) -> ShotBody {
        match target {
            FireTarget::FirstTank => ShotBody::Tank(self.ids[1]),
            FireTarget::SecondTank => ShotBody::Tank(self.ids[2]),
            FireTarget::LandedShip => ShotBody::Ship(0),
            FireTarget::FlyingShip => ShotBody::Ship(1),
        }
    }

    fn cut_at(&mut self, point: Vec2, radius: u32) {
        let planet = self.state.planets[0];
        let local = (point - planet.position).rotate_radians(-planet.wrapper_angle);
        self.recorded_edits.push((self.tick, local, radius));
        self.queue_local_cut(local, radius);
    }

    fn queue_local_cut(&mut self, local: Vec2, radius: u32) {
        let center = self
            .state
            .planet_terrain(0)
            .unwrap()
            .local_to_cell(local)
            .expect("cut within material grid");
        self.state
            .queue_planet_edit(
                0,
                TerrainEdit {
                    brush: Brush::Circle { center, radius },
                    mode: EditMode::Remove,
                },
            )
            .unwrap();
    }

    pub fn step(&mut self) {
        Self::host_step(&mut self.state, &mut self.pilot);
        if self.case == FixtureCase::Fire {
            self.note_shots();
        }
    }

    /// Notes each shot the firing tank fired in the step, and the record of
    /// each that ended in it.
    fn note_shots(&mut self) {
        let planet = self.state.planets[0];
        let ground = planet.radius * crate::BODY_BOUNDS_RADIUS_SCALE;
        for shot in self.state.tank_shots(self.ids[0]) {
            if self.fired.iter().any(|fired| fired.id == shot.id) {
                continue;
            }
            // The ground stands between the round and its target if the
            // straight line from one to the other passes through the planet.
            let sight_blocked = shot.launch.target.is_some_and(|target| {
                let from = shot.launch.position - planet.position;
                let along = target.position - shot.launch.position;
                let reach = (-from.dot(along) / along.length_squared()).clamp(0.0, 1.0);
                (from + along * reach).length() < ground
            });
            self.fired.push(FiredShot {
                id: shot.id,
                tick: self.tick,
                launch: shot.launch,
                sight_blocked,
                ended: None,
            });
        }
        for shot in &self.state.tank_shots_ended {
            if let Some(fired) = self.fired.iter_mut().find(|fired| fired.id == shot.id) {
                fired.ended = Some((self.tick, shot.clone()));
            }
        }
    }

    /// Sampling scans diagnostics outside simulation timing.
    pub fn sample(&mut self) -> FixtureSample {
        let tanks = self
            .ids
            .iter()
            // A destroyed tank has no sample, which fails the checks that
            // count on it.
            .filter_map(|&id| {
                let s = self.state.tank_snapshot(id)?;
                let c = s.assembly.chassis;
                Some(TankSample {
                    position: [c.position.x, c.position.y],
                    velocity: [c.linear_velocity.x, c.linear_velocity.y],
                    angle: c.angle,
                    spin: c.angular_velocity,
                    wheel_positions: s.assembly.wheels.map(|w| [w.position.x, w.position.y]),
                    wheel_velocities: s
                        .assembly
                        .wheels
                        .map(|w| [w.linear_velocity.x, w.linear_velocity.y]),
                    supported: s.support.is_some(),
                    relative_speed: s.support.map(|support| {
                        (c.linear_velocity - support.velocity)
                            .dot(Vec2::new(support.normal.y, -support.normal.x))
                    }),
                    blocked: s.drive_blocked,
                    elevation: s.command.elevation,
                    facing: s.command.facing,
                })
            })
            .collect();
        let audit = self.state.tank_step_audit();
        let work = self.state.tank_work();
        let d = self.state.terrain_diagnostics();
        let pilot = self.pilot_snapshot().expect("live comparison pilot").motion;
        let sample = FixtureSample {
            pilot_motion: [
                pilot.position.x,
                pilot.position.y,
                pilot.linear_velocity.x,
                pilot.linear_velocity.y,
                pilot.angle,
                pilot.angular_velocity,
            ],
            tick: self.tick,
            tanks,
            queries: work.queries,
            round_grains: self
                .state
                .loose_terrain()
                .unwrap()
                .iter()
                .filter(|g| g.shape() == engine_rapier::terrain::GrainShape::Round)
                .count(),
            angular_grains: self
                .state
                .loose_terrain()
                .unwrap()
                .iter()
                .filter(|g| g.shape() == engine_rapier::terrain::GrainShape::Hexagon)
                .count(),
            contact_traversal: work.contact_traversal,
            spatial_traversal: work.spatial_traversal,
            spatial_exhaustions: work.spatial_exhaustions,
            physics_steps: audit.physics_steps,
            gravity_bodies: audit.gravity_bodies,
            gravity_applications: audit.gravity_applications,
            duplicate_gravity: audit.duplicate_gravity_applications,
            bodies: self.state.physics.world.body_count(),
            colliders: self.state.physics.world.collider_count(),
            joints: self.state.physics.world.joint_count(),
            material_cells: d.occupied_cells + d.loose_cells as u64 + d.removed_cells,
            removed_cells: d.removed_cells,
            world_motion_hash: d.motion_hash,
            fire: (self.case == FixtureCase::Fire).then(|| self.fire_sample()),
        };
        self.tick += 1;
        sample
    }

    fn fire_sample(&self) -> FireSample {
        let planet = self.state.planets[0].position;
        let rounds = self.state.debris.iter().filter(|shell| !shell.dead);
        let rounds = rounds.filter_map(|shell| {
            let id = shell.tank_round?.shot;
            Some(RoundSample {
                shot: self.fired.iter().position(|fired| fired.id == id)?,
                position: [shell.position.x, shell.position.y],
                velocity: [shell.velocity.x, shell.velocity.y],
            })
        });
        let health = self.ids.iter().map(|&id| {
            let tank = self.state.tank_snapshot(id);
            tank.map_or(0.0, |tank| tank.health)
        });
        FireSample {
            planet: [planet.x, planet.y],
            rounds: rounds.collect(),
            health: health.collect(),
            ships: [0, 1].map(|seat| {
                let ship = &self.state.ships[seat];
                [
                    ship.position.x,
                    ship.position.y,
                    ship.velocity.x,
                    ship.velocity.y,
                    ship.life,
                ]
            }),
            records: self
                .state
                .tank_shots(self.ids[0])
                .map(|shot| {
                    let at = self.fired.iter().position(|fired| fired.id == shot.id);
                    at.unwrap_or(usize::MAX)
                })
                .collect(),
            ended: self
                .fired
                .iter()
                .filter(|shot| shot.ended.is_some())
                .count(),
            craters: self.state.terrain_cannon_hits() - self.initial_craters,
            casts: self.state.tank_work().barrel_casts,
        }
    }

    pub fn pilot_snapshot(&self) -> Option<engine_rapier::spaceling::SpacelingSnapshot> {
        self.pilot.snapshot(&self.state.physics)
    }

    pub fn frame(&self) -> RenderFrame {
        // Ordinary world primitives and physical dimensions; only the inspection
        // camera is focused on this small material planet.
        let mut frame = crate::render_state_with_camera(
            &self.state,
            engine_common::Camera2::new(crate::render_point(self.state.planets[0].position), 220.0),
            crate::RenderOptions::player(),
        );
        self.pilot
            .render_comparison(&self.state.physics, &mut frame);
        frame
    }

    /// Actual lifecycle cleanup, retaining the monotonic IDs for stale-handle checks.
    pub fn cleanup(&mut self) -> bool {
        let before = (
            self.state.physics.world.body_count(),
            self.state.physics.world.collider_count(),
            self.state.physics.world.joint_count(),
        );
        self.state.clear_tanks();
        self.ids.iter().all(|&id| {
            self.state.tank_snapshot(id).is_none()
                && !self.state.command_tank(id, TankCommand::default())
                && self.state.tank_shots(id).next().is_none()
        }) && self.state.tank_ids().count() == 0
            && self.state.physics.world.body_count() + self.ids.len() * 3 == before.0
            && self.state.physics.world.collider_count() + self.ids.len() * 3 == before.1
            && self.state.physics.world.joint_count() + self.ids.len() * 2 == before.2
    }

    pub fn assertions(
        &self,
        trace: &[FixtureSample],
    ) -> std::collections::BTreeMap<&'static str, bool> {
        assert_eq!(trace.len(), MEASURED_STEPS);
        let expected = self.state.loose_terrain().unwrap().config().shape;
        let shapes_exercised = trace.iter().any(|s| match expected {
            engine_rapier::terrain::GrainShape::Round => {
                s.round_grains > 0 && s.angular_grains == 0
            }
            engine_rapier::terrain::GrainShape::Hexagon => {
                s.angular_grains > 0 && s.round_grains == 0
            }
        });
        if self.case == FixtureCase::Fire {
            return self.fire_assertions(trace, shapes_exercised);
        }
        if self.ids.is_empty() {
            return std::collections::BTreeMap::from([
                ("released_shape_exercised", shapes_exercised),
                (
                    "one_physics_step",
                    trace[0].physics_steps == self.initial_physics_steps + 1
                        && trace
                            .windows(2)
                            .all(|w| w[1].physics_steps == w[0].physics_steps + 1),
                ),
                (
                    "no_duplicate_gravity",
                    trace.iter().all(|s| {
                        s.gravity_applications == 0
                            && s.gravity_bodies == 0
                            && s.duplicate_gravity == 0
                    }),
                ),
                (
                    "material_conserved",
                    trace
                        .iter()
                        .all(|s| s.material_cells == self.initial_material),
                ),
                (
                    "bounded_work",
                    trace
                        .iter()
                        .all(|s| s.queries == 0 && s.contact_traversal == 0),
                ),
            ]);
        }
        let mean = |range: std::ops::Range<usize>, index: usize| {
            let values: Vec<_> = trace[range]
                .iter()
                .filter_map(|s| s.tanks[index].relative_speed)
                .collect();
            (
                values.len(),
                values.iter().sum::<f32>() / values.len().max(1) as f32,
            )
        };
        let (drive_n, drive) = mean(60..120, 0);
        let (brake_n, brake) = mean(180..240, 0);
        let (_, independent) = mean(60..240, 1);
        let mut checks = std::collections::BTreeMap::from([
            ("released_shape_exercised", shapes_exercised),
            (
                "one_physics_step",
                trace[0].physics_steps == self.initial_physics_steps + 1
                    && trace
                        .windows(2)
                        .all(|w| w[1].physics_steps == w[0].physics_steps + 1),
            ),
            (
                "no_duplicate_gravity",
                trace.iter().all(|s| {
                    s.gravity_applications == 6 && s.gravity_bodies == 6 && s.duplicate_gravity == 0
                }),
            ),
            (
                "material_conserved",
                trace
                    .iter()
                    .all(|s| s.material_cells == self.initial_material),
            ),
            (
                "bounded_work",
                trace.iter().all(|s| {
                    s.queries <= 12 && s.contact_traversal <= 256 && s.spatial_traversal <= 16384
                }),
            ),
            (
                "independent_controls",
                independent.abs() < 0.8
                    && trace.iter().all(|s| {
                        (s.tanks[0].elevation, s.tanks[0].facing) == (0.3, 1.0)
                            && (s.tanks[1].elevation, s.tanks[1].facing) == (0.4, -1.0)
                    }),
            ),
            (
                "supported_drive_and_brake",
                drive_n >= 50 && brake_n >= 50 && drive > 1.0 && brake.abs() < 0.8,
            ),
            (
                "unsafe_gap_refused",
                trace[240..420]
                    .iter()
                    .filter(|s| s.tanks[0].blocked)
                    .count()
                    > 30
                    && trace[240..420]
                        .iter()
                        .filter(|s| s.tanks[0].supported)
                        .count()
                        > 150,
            ),
        ]);
        if let Some((start, radial)) = self.cut_start {
            checks.insert(
                "support_loss_fall_or_fragment_ride",
                trace[420..430].iter().any(|s| !s.tanks[0].supported)
                    && trace[420..450].iter().any(|s| {
                        (Vec2::new(s.tanks[0].position[0], s.tanks[0].position[1]) - start)
                            .dot(radial)
                            < -1.0
                    }),
            );
        }
        checks
    }
}

/// How many steps after its launch a round's fall is measured.
const CURVE_STEPS: usize = 30;

impl TankFixture {
    /// The firing case's checks.
    fn fire_assertions(
        &self,
        trace: &[FixtureSample],
        shapes_exercised: bool,
    ) -> std::collections::BTreeMap<&'static str, bool> {
        let tanks = self.ids.len();
        let mut checks = std::collections::BTreeMap::from([
            ("released_shape_exercised", shapes_exercised),
            (
                "one_physics_step",
                trace[0].physics_steps == self.initial_physics_steps + 1
                    && trace
                        .windows(2)
                        .all(|w| w[1].physics_steps == w[0].physics_steps + 1),
            ),
            (
                "no_duplicate_gravity",
                trace.iter().all(|s| {
                    s.gravity_applications == 3 * tanks
                        && s.gravity_bodies == 3 * tanks
                        && s.duplicate_gravity == 0
                }),
            ),
            (
                "material_conserved",
                trace
                    .iter()
                    .all(|s| s.material_cells == self.initial_material),
            ),
            (
                "bounded_work",
                trace.iter().all(|s| {
                    s.queries <= 6 * tanks
                        && s.contact_traversal <= 256
                        && s.spatial_traversal <= 16384
                }),
            ),
        ]);
        let fire: Vec<&FireSample> = trace
            .iter()
            .map(|s| s.fire.as_ref().expect("a firing sample"))
            .collect();
        // The shot fired under each of the script's orders, and its record
        // once it has ended, with the measured step it ended on.
        let shot = |at: usize| self.fired.get(at);
        let ended = |at: usize| {
            let (tick, record) = shot(at)?.ended.as_ref()?;
            Some((*tick, record, record.end.as_ref()?))
        };
        let orders = 0..self.script.len();
        // Each shot the script orders was fired on its step as ordered, for
        // its target, and has ended. Nothing else was fired.
        let followed = self.fired.len() == self.script.len()
            && self.script.iter().enumerate().all(|(at, order)| {
                let target = Some(self.fire_target(order.target));
                let ordered = shot(at).is_some_and(|shot| {
                    shot.tick == order.tick
                        && shot.launch.elevation == order.elevation.to_radians()
                        && shot.launch.facing == order.facing
                        && shot.launch.power == order.power
                });
                ordered && ended(at).is_some_and(|(_, record, _)| record.target == target)
            });
        checks.insert("script_followed", followed);
        // What is left of a target after a measured step: a tank's health or
        // a ship's life.
        let left = |target: ShotBody, tick: usize| match target {
            ShotBody::Tank(id) => self
                .ids
                .iter()
                .position(|&tank| tank == id)
                .map(|at| fire[tick].health[at]),
            ShotBody::Ship(seat) => Some(fire[tick].ships[seat][4]),
            _ => None,
        };
        // The round stopped on its target and harmed nothing else, and the
        // target lost in that step what the record tells.
        let struck = |at: usize| {
            let target = self.fire_target(self.script[at].target);
            ended(at).is_some_and(|(tick, _, end)| {
                let struck = ShotOutcome::Burst {
                    on: Some(ShotStop::Body(target)),
                };
                let harm = end.harm.first().filter(|harm| harm.body == target);
                let before = tick.checked_sub(1).and_then(|tick| left(target, tick));
                let lost = before
                    .zip(left(target, tick))
                    .map(|(before, after)| before - after);
                end.outcome == struck
                    && end.harm.len() == 1
                    && harm.zip(lost).is_some_and(|(harm, lost)| {
                        harm.health > 1.0 && (harm.health - lost).abs() < 1.0e-3
                    })
            })
        };
        // The round's harm to a tank names the side that fired it.
        let credited = |target: ShotBody| {
            let ShotBody::Tank(id) = target else {
                return false;
            };
            let last = self.state.tank_snapshot(id).and_then(|t| t.last_damage);
            last.is_some_and(|damage| {
                damage.cause == TankDamageCause::Round
                    && damage.source == Some(Allegiance::INITIAL_GOVERNMENT)
            })
        };
        // Each of these holds only if the script orders such a shot, and
        // every such shot went as told.
        let names = [
            "first_tank_struck",
            "second_tank_struck",
            "landed_ship_struck",
            "flying_ship_struck",
            "ground_stops_the_flat_shot",
        ];
        let mut went: [Option<bool>; 5] = [None; 5];
        for (at, order) in self.script.iter().enumerate() {
            let target = self.fire_target(order.target);
            let blocked = shot(at).is_some_and(|shot| shot.sight_blocked);
            let (kind, well) = match order.target {
                _ if !order.strikes => {
                    // The ground stood between the round and its target and
                    // stopped it. It harmed nothing, and its target, which it
                    // never came within a blast's reach of, lost nothing.
                    let stopped = ended(at).is_some_and(|(tick, record, end)| {
                        let ground = ShotOutcome::Burst {
                            on: Some(ShotStop::Ground),
                        };
                        let closest = record.flight.closest.map(|c| c.distance);
                        end.outcome == ground
                            && end.harm.is_empty()
                            && closest.is_some_and(|d| d > cannon::BLAST.radius)
                            && left(target, tick) == left(target, order.tick)
                    });
                    (4, blocked && stopped)
                }
                FireTarget::FirstTank => (0, struck(at) && credited(target)),
                FireTarget::SecondTank => (1, struck(at) && credited(target)),
                // It lies beyond the horizon, and the lob went over.
                FireTarget::LandedShip => (2, struck(at) && blocked),
                FireTarget::FlyingShip => {
                    // It was under way as the tank fired.
                    let moving = shot(at)
                        .and_then(|shot| shot.launch.target)
                        .is_some_and(|target| target.velocity.length() > 1.0);
                    (3, struck(at) && moving)
                }
            };
            went[kind] = Some(went[kind].unwrap_or(true) && well);
        }
        for (name, went) in names.into_iter().zip(went) {
            checks.insert(name, went == Some(true));
        }
        // A round's motion after a measured step, and where the planet was.
        let round = |at: usize, tick: usize| {
            let sample = fire.get(tick)?;
            let round = sample.rounds.iter().find(|round| round.shot == at)?;
            Some((
                Vec2::new(round.position[0], round.position[1]),
                Vec2::new(round.velocity[0], round.velocity[1]),
                Vec2::new(sample.planet[0], sample.planet[1]),
            ))
        };
        // Half a second after it leaves, each round has fallen more than a
        // unit and a half from the straight line of its launch, toward the
        // planet.
        let curved = orders.clone().all(|at| {
            shot(at).is_some_and(|shot| {
                let after = round(at, shot.tick + CURVE_STEPS - 1);
                after.is_some_and(|(position, _, planet)| {
                    let seconds = CURVE_STEPS as f32 / 60.0;
                    let straight = shot.launch.position + shot.launch.velocity * seconds;
                    (position - straight).dot((planet - position).normalized()) > 1.5
                })
            })
        });
        checks.insert("curved_flight", curved);
        // The way a round falls over one step: its change of velocity, which
        // points at the planet's center from where the round is.
        let fall = |at: usize, tick: usize| {
            let (_, before, _) = round(at, tick)?;
            let (position, after, planet) = round(at, tick + 1)?;
            let fall = (after - before).normalized();
            (fall.dot((planet - position).normalized()) > 0.999).then_some(fall)
        };
        // Each round falls toward the planet early in its flight and late,
        // and over the longest flight that way turns by more than a third of
        // a right angle.
        let falls: Vec<_> = orders
            .clone()
            .map(|at| {
                let early = shot(at).and_then(|shot| fall(at, shot.tick + 2));
                let late = ended(at).and_then(|(tick, _, _)| fall(at, tick.checked_sub(4)?));
                early.zip(late)
            })
            .collect();
        let steps = |at: usize| ended(at).map(|(_, record, _)| record.flight.steps);
        let longest = orders.clone().max_by_key(|&at| steps(at));
        let turned = longest
            .and_then(|at| falls[at])
            .is_some_and(|(early, late)| early.dot(late) < 0.85);
        checks.insert(
            "fall_follows_the_planet",
            falls.iter().all(Option::is_some) && turned,
        );
        // A round leaves with the hull's motion on top of what the cannon
        // gives it, and on this turning planet the hull moves.
        let carried = orders.clone().all(|at| {
            shot(at).is_some_and(|shot| {
                let launch = shot.launch;
                let carried = launch.velocity - launch.direction * launch.power;
                carried.length() > 1.0 && (carried - launch.hull.linear_velocity).length() < 0.1
            })
        });
        checks.insert("launch_carries_the_hull", carried);
        // After every step, each shot fired so far either has its round in
        // flight or has ended, and the tank's records are those of its rounds
        // in flight and of the last few of its shots to end, by when they
        // ended. Two rounds flew at once, and none is left at the end.
        let accounted = fire.iter().enumerate().all(|(tick, f)| {
            let fired = self.fired.iter().filter(|shot| shot.tick <= tick).count();
            let shots = self.fired.iter().enumerate();
            let mut ended: Vec<(usize, usize)> = shots
                .filter_map(|(at, shot)| Some((shot.ended.as_ref()?.0, at)))
                .filter(|&(ended, _)| ended <= tick)
                .collect();
            ended.sort_unstable();
            let kept = ended.iter().rev().take(ENDED_SHOTS).map(|&(_, at)| at);
            let flying = f.rounds.iter().map(|round| round.shot);
            let mut expected: Vec<usize> = flying.chain(kept).collect();
            expected.sort_unstable();
            let mut records = f.records.clone();
            records.sort_unstable();
            f.rounds.len() + f.ended == fired && f.ended == ended.len() && records == expected
        });
        let most = fire.iter().map(|f| f.rounds.len()).max();
        checks.insert(
            "rounds_and_records_accounted",
            accounted && most == Some(2) && fire.last().is_some_and(|f| f.rounds.is_empty()),
        );
        // A step casts along a barrel once for each shot it fires, and makes
        // no cast otherwise.
        let cast = fire.iter().enumerate().all(|(tick, f)| {
            let fired = self.fired.iter().filter(|shot| shot.tick == tick);
            f.casts == fired.count()
        });
        checks.insert(
            "one_cast_for_each_shot",
            cast && fire.iter().any(|f| f.casts > 0),
        );
        // Each round carried the cannon's payload and released no more than
        // that, and the rounds that the ground stopped dug one crater each.
        let payload = f64::from(cannon::PAYLOAD);
        let budgeted = orders.clone().all(|at| {
            let ledger = ended(at).and_then(|(_, _, end)| end.ledger);
            ledger.is_some_and(|ledger| {
                ledger.payload.budget == payload
                    && ledger.payload.delivered <= payload
                    && ledger.kinetic.budget > 0.0
            })
        });
        let grounded = orders.filter(|&at| {
            let ground = ShotOutcome::Burst {
                on: Some(ShotStop::Ground),
            };
            ended(at).is_some_and(|(_, _, end)| end.outcome == ground)
        });
        checks.insert(
            "payload_accounted_and_one_crater_each",
            budgeted && fire.last().map(|f| f.craters) == Some(grounded.count() as u64),
        );
        checks
    }
}

impl SpacewarsScenario {
    /// The tanks' proving ground: the terrain fixture's world, with its planet
    /// weighing what the surface games' controlled planet does, so that tanks
    /// are tried under the pull they fight under. The terrain fixture's own
    /// planet has the arcade game's pull.
    pub(crate) fn init_tank_proving_ground(seed: u64) -> SpacewarsState {
        let mut state = Self::init_terrain_fixture(seed);
        let planet = &mut state.planets[0];
        planet.mass = crate::surface_mass(planet.radius);
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The proving ground's planet pulls the surface gravity at the height
    /// its weight is set for, and a tank on it, which rides higher, feels a
    /// little less. The terrain fixture's own planet pulls about 39 times as
    /// hard.
    #[test]
    fn the_proving_grounds_planet_pulls_the_surface_gravity() {
        let mut state = SpacewarsScenario::init_tank_proving_ground(42);
        let planet = state.planets[0];
        let pull = |mass: f32, distance: f32| {
            60.0 * engine_core::constants::GRAVITY * mass / distance.powi(2)
        };
        let distance = planet.radius * crate::BODY_BOUNDS_RADIUS_SCALE + crate::SURFACE_PULL_HEIGHT;
        let gravity = engine_rapier::SURFACE_GRAVITY;
        assert!((pull(planet.mass, distance) - gravity).abs() < 1.0e-3);
        let arcade = SpacewarsScenario::init_terrain_fixture(42).planets[0];
        let ratio = pull(arcade.mass, distance) / gravity;
        assert!((38.0..40.0).contains(&ratio), "{ratio}");

        use engine_common::Scenario;
        let dt = Duration::from_secs_f32(1.0 / 60.0);
        state.ships[0].position = Vec2::new(700.0, 700.0);
        SpacewarsScenario::step(&mut state, &[], dt);
        let tank = state
            .place_tank(
                Vec2::new(496.0, 565.0),
                -Vec2::Y,
                Allegiance::INITIAL_GOVERNMENT,
            )
            .expect("top admission");
        for _ in 0..60 {
            SpacewarsScenario::step(&mut state, &[], dt);
        }
        let felt = state.tank_snapshot(tank).unwrap().local_gravity.length();
        assert!((17.0..gravity).contains(&felt), "{felt}");
    }

    #[test]
    fn shared_script_measures_drive_brake_gap_cut_and_replay() {
        for shape in [DirtMode::Round, DirtMode::Angular] {
            for case in [FixtureCase::Drive, FixtureCase::Cut] {
                for count in [0, 2] {
                    let mut fixture = TankFixture::with_tanks(42, shape, case, count);
                    let mut replay = TankFixture::with_tanks(42, shape, case, count);
                    let mut trace = Vec::new();
                    for _ in 0..MEASURED_STEPS {
                        fixture.prepare_step();
                        fixture.step();
                        let pilot = fixture.pilot_snapshot().expect("live comparison pilot");
                        let center = fixture.state.planets[0].position;
                        assert!(
                            (pilot.motion.position - center).length() < 100.0,
                            "pilot outside comparison camera: {:?}",
                            pilot.motion.position
                        );
                        for id in &fixture.ids {
                            let tank = fixture.state.tank_snapshot(*id).unwrap();
                            assert!(
                                (pilot.motion.position - tank.assembly.chassis.position).length()
                                    > 12.0,
                                "comparison pilot entered driving course"
                            );
                        }
                        let ordinary = crate::render_state_with_camera(
                            &fixture.state,
                            fixture.frame().camera,
                            crate::RenderOptions::player(),
                        );
                        let count = |frame: RenderFrame| {
                            frame
                                .layers
                                .iter()
                                .map(|l| l.primitives.len())
                                .sum::<usize>()
                        };
                        assert!(
                            count(fixture.frame()) > count(ordinary),
                            "pilot drawing missing"
                        );
                        let sample = fixture.sample();
                        replay.prepare_step();
                        replay.step();
                        assert_eq!(sample, replay.sample());
                        trace.push(sample);
                    }
                    let checks = fixture.assertions(&trace);
                    eprintln!("{shape:?} {case:?}: {checks:?}");
                    assert!(checks.values().all(|&v| v), "{checks:?}");
                    assert!(fixture.cleanup());
                }
            }
        }
    }

    /// The firing case's checks, by name.
    const FIRE_CHECKS: [&str; 17] = [
        "bounded_work",
        "curved_flight",
        "fall_follows_the_planet",
        "first_tank_struck",
        "flying_ship_struck",
        "ground_stops_the_flat_shot",
        "landed_ship_struck",
        "launch_carries_the_hull",
        "material_conserved",
        "no_duplicate_gravity",
        "one_cast_for_each_shot",
        "one_physics_step",
        "payload_accounted_and_one_crater_each",
        "released_shape_exercised",
        "rounds_and_records_accounted",
        "script_followed",
        "second_tank_struck",
    ];

    /// A government tank on the turning, carried planet fires the script: it
    /// strikes each player's tank, the landed ship beyond its horizon and the
    /// flying ship, and the ground stops its flat shot. Every check holds for
    /// both dirt shapes, a second fixture built from the seed replays every
    /// sample, and the cleanup leaves nothing of the tanks or their records.
    #[test]
    fn firing_script_strikes_its_targets_and_replays_for_both_dirt_shapes() {
        for shape in [DirtMode::Round, DirtMode::Angular] {
            let mut fixture = TankFixture::new(42, shape, FixtureCase::Fire);
            let mut replay = TankFixture::new(42, shape, FixtureCase::Fire);
            let mut trace = Vec::new();
            for _ in 0..MEASURED_STEPS {
                fixture.prepare_step();
                fixture.step();
                let sample = fixture.sample();
                replay.prepare_step();
                replay.step();
                assert_eq!(sample, replay.sample());
                trace.push(sample);
            }
            let checks = fixture.assertions(&trace);
            eprintln!("{shape:?} Fire: {checks:?}");
            let names: Vec<_> = checks.keys().copied().collect();
            assert_eq!(names, FIRE_CHECKS, "{shape:?}");
            assert!(checks.values().all(|&v| v), "{shape:?}: {checks:?}");
            // The samples carried the rounds, so the replay compared them.
            let flown = trace.iter().filter_map(|s| s.fire.as_ref());
            assert!(flown.clone().count() == MEASURED_STEPS);
            assert!(flown.clone().any(|f| f.rounds.len() == 2), "{shape:?}");
            assert!(fixture.cleanup());
        }
    }

    /// The driving cases' samples leave the firing case's part out, so their
    /// traces read as they did before it was added.
    #[test]
    fn driving_samples_leave_the_firing_part_out() {
        let sample = |case| {
            let mut fixture = TankFixture::new(42, DirtMode::Round, case);
            fixture.prepare_step();
            fixture.step();
            serde_json::to_value(fixture.sample()).unwrap()
        };
        for case in [FixtureCase::Drive, FixtureCase::Cut] {
            let sample = sample(case);
            assert!(sample.get("fire").is_none(), "{case:?}: {sample}");
            assert!(sample.get("world_motion_hash").is_some(), "{case:?}");
        }
        let fire = sample(FixtureCase::Fire);
        let part = fire.get("fire").expect("the firing case's part");
        assert_eq!(part["rounds"].as_array().map(Vec::len), Some(1), "{part}");
    }
}
