//! Spacewars assemblies and gameplay-facing queries for the canonical physics world.

use std::collections::{BTreeMap, BTreeSet};

use engine_core::Vec2;
use engine_rapier::{
    rover::{RoverAssembly, RoverSnapshot, RoverSpawnPose, RoverSpec},
    world::{
        BodyId as PhysicsBodyId, BodyKind, BodyMotion, BodyRole, BodySpec, ColliderId,
        ColliderRole, ColliderSpec, CollisionGroups, PhysicsId, PhysicsStepMetrics, PhysicsWorld,
        PhysicsWorldConfig, RayCastOptions,
    },
};

use super::{
    BODY_BOUNDS_RADIUS_SCALE, BodyId, CANNON_SHELL_RADIUS, DEFAULT_ELASTICITY, DebrisKind,
    DebrisState, PLANET_ELASTICITY, POD_BODY, POD_LASER, POD_PIVOT, POD_THRUSTER, PlanetState,
    RoverState, SHELL_BODY, SHIP_BODY, SHIP_LASER, SHIP_LEFT_WING, SHIP_PIVOT, SHIP_RIGHT_WING,
    SHIP_THRUSTER, SHIP_WING_MOUNT, SHIP_WING_PIVOT, SPACEPORT_PULL_SCALE, ShipForm, ShipState,
    SunState, planet_surface_velocity, rotate_points, spaceport_docking_anchor,
    spaceport_local_points,
};

const WORLD_ENTITY_VALUE: u64 = 1;
const SUN_ENTITY_VALUE: u64 = 2;
const PLANET_ENTITY_BASE: u64 = 100;
const SHIP_ENTITY_BASE: u64 = 10_000;
const ROVER_ENTITY_BASE: u64 = 50_000;
const DEBRIS_ENTITY_BASE: u64 = 100_000;

const WORLD_ROLE: ColliderRole = ColliderRole::new(1);
const BODY_SURFACE_ROLE: ColliderRole = ColliderRole::new(2);
const SPACEPORT_SENSOR_ROLE: ColliderRole = ColliderRole::new(3);
const SHIP_HULL_ROLE: ColliderRole = ColliderRole::new(5);
const DEBRIS_ROLE: ColliderRole = ColliderRole::new(6);
const ROVER_SURFACE_ROLE: ColliderRole = ColliderRole::new(7);
const LANDING_FOOT_ROLE: ColliderRole = ColliderRole::new(8);
const OUTPOST_TERMINAL_ROLE: ColliderRole = ColliderRole::new(9);

pub(super) const LANDING_FOOT_RADIUS: f32 = 0.45;
pub(super) const LANDING_FEET: [Vec2; 2] = [Vec2::new(-3.0, -5.0), Vec2::new(3.0, -5.0)];
const RECOVERY_BREAKUP_GRACE_TICKS: u64 = 30;
pub(super) fn surface_landing_geometry(form: ShipForm) -> ([Vec2; 2], f32) {
    match form {
        ShipForm::Ship => (LANDING_FEET, LANDING_FOOT_RADIUS),
        ShipForm::EscapePod => ([Vec2::new(-0.7, -0.65), Vec2::new(0.7, -0.65)], 0.2),
    }
}
pub(super) const OUTPOST_TERMINAL_HALF_SIZE: Vec2 = Vec2::new(0.7, 1.1);

pub(super) fn is_planet_surface_support(collider: ColliderId, planet: usize) -> bool {
    planet_surface_support_index(collider) == Some(planet)
}

/// Decode contact identity directly; support lookup must not scan the world.
pub(super) fn planet_surface_support_index(collider: ColliderId) -> Option<usize> {
    if collider.role == ROVER_SURFACE_ROLE
        || collider.role == OUTPOST_TERMINAL_ROLE
        || collider.role.value() >= terrain_spec().first_chunk_role
    {
        planet_index(collider.entity)
    } else {
        None
    }
}

const GROUP_SHIP_0: u32 = 1 << 0;
const GROUP_SHIP_1: u32 = 1 << 1;
const GROUP_POD_0: u32 = 1 << 2;
const GROUP_POD_1: u32 = 1 << 3;
const GROUP_DEBRIS: u32 = 1 << 4;
const GROUP_BODY: u32 = 1 << 5;
const GROUP_WORLD: u32 = 1 << 6;
const GROUP_SPACEPORT_SENSOR: u32 = 1 << 8;
const GROUP_ROVER: u32 = 1 << 9;
const GROUP_ROVER_SURFACE: u32 = 1 << 10;
const GROUP_SPACELING: u32 = 1 << 11;
const GROUP_MATERIAL: u32 = 1 << 12;
const GROUP_ALL_SHIPS: u32 = GROUP_SHIP_0 | GROUP_SHIP_1 | GROUP_POD_0 | GROUP_POD_1;
// GROUP_BODY also includes dynamic terrain fragments. Celestial and boundary
// filters must accept that group as well as ordinary ships, debris, and rovers.
const GROUP_ALL_SOLIDS: u32 =
    GROUP_ALL_SHIPS | GROUP_DEBRIS | GROUP_BODY | GROUP_WORLD | GROUP_ROVER | GROUP_SPACELING;

pub(super) fn spaceling_collision_groups() -> CollisionGroups {
    CollisionGroups::new(
        GROUP_SPACELING,
        GROUP_ROVER_SURFACE
            | GROUP_MATERIAL
            | GROUP_ALL_SHIPS
            | GROUP_DEBRIS
            | GROUP_WORLD
            | GROUP_SPACELING,
    )
}

const WORLD_SURFACE_SEGMENTS: usize = 192;
// The full ship silhouette is intentionally broad. Use a body-sized,
// rotation-invariant contact shape throughout a landed stay so turning on the
// surface berth cannot introduce a new planet overlap.
const DOCKED_SHIP_COLLIDER_RADIUS: f32 = 2.5;
const CONTACT_REARM_TICKS: u64 = 6;

pub(super) fn terrain_spec() -> engine_rapier::terrain::TerrainSpec {
    engine_rapier::terrain::TerrainSpec {
        friction: 0.9,
        restitution: 0.1,
        collision_groups: CollisionGroups::new(GROUP_BODY | GROUP_MATERIAL, GROUP_ALL_SOLIDS),
        ..Default::default()
    }
}

pub(super) fn spacewars_rover_spec() -> RoverSpec {
    RoverSpec {
        suspension_stiffness: 10_000.0,
        suspension_damping: 350.0,
        suspension_max_force: 10_000.0,
        wheel_target_speed: 12.0,
        wheel_motor_torque: 100.0,
        wheel_brake_torque: 120.0,
        collision_groups: CollisionGroups::new(
            GROUP_ROVER,
            GROUP_BODY | GROUP_WORLD | GROUP_DEBRIS | GROUP_ROVER_SURFACE,
        ),
        ..RoverSpec::default()
    }
}

pub(super) fn spacewars_rover_spawn_pose(planet: &PlanetState) -> RoverSpawnPose {
    let spec = spacewars_rover_spec();
    let up_angle = planet.wrapper_angle + core::f32::consts::PI;
    let up = Vec2::from_radians(up_angle);
    RoverSpawnPose {
        wheel_center: planet.position
            + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE + spec.wheel_radius + 0.03),
        up_angle,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum MechanicalEntity {
    World,
    Body(BodyId),
    TerrainFragment(u64),
    Ship(usize),
    Rover(u64),
    Debris(u64),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MechanicalContact {
    pub a: MechanicalEntity,
    pub b: MechanicalEntity,
    pub point: Option<Vec2>,
    pub normal: Vec2,
    pub impulse_magnitude: f32,
    /// Pre-solver speed closing along the contact normal. Unlike impulse,
    /// this excludes positional correction and sustained support force.
    pub closing_speed: f32,
    /// True only on the first observed solver step for this entity pair.
    /// Sustained support forces remain contacts without becoming fresh impacts.
    pub started: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LaserTrace {
    pub target: Option<MechanicalEntity>,
    pub point: Vec2,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PhysicsLifecycle {
    pub added: usize,
    pub removed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShipColliderKey {
    form: ShipForm,
    wing_theta: u32,
    docked: bool,
    compact: bool,
    constrained: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlanetColliderKey {
    radius: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DebrisColliderKey {
    signature: u64,
    armed: bool,
}

#[derive(Debug, Clone)]
struct RoverPhysicsEntry {
    planet: usize,
    owner_id: usize,
    last_planet: PlanetState,
    assembly: RoverAssembly,
}

#[derive(Debug, Clone)]
pub(super) struct SpacewarsPhysics {
    pub(super) world: PhysicsWorld,
    pub(super) terrain_fragments: BTreeSet<u64>,
    pub(super) material_planets: BTreeSet<usize>,
    // Newly replaced/inserted terrain is indexed by the next ordinary step.
    // Until then no spatial query may authorize a transfer or placement.
    pub(super) material_queries_dirty: bool,
    disabled_spaceports: BTreeSet<usize>,
    sun_radius: Option<u32>,
    ship_keys: [Option<ShipColliderKey>; 2],
    docked_planets: [Option<usize>; 2],
    // Opt-in physical landing assembly; ordinary Spacewars retains its berths.
    surface_ships: Option<Vec<usize>>,
    surface_recovery: bool,
    tick: u64,
    contact_last_seen: BTreeMap<(MechanicalEntity, MechanicalEntity), u64>,
    pre_step_motions: BTreeMap<MechanicalEntity, BodyMotion>,
    planet_keys: Vec<Option<PlanetColliderKey>>,
    rovers: BTreeMap<u64, RoverPhysicsEntry>,
    debris_keys: BTreeMap<u64, DebrisColliderKey>,
    next_debris_entity: u64,
}

pub(super) struct PhysicsReconcileInput<'a> {
    pub tick: u64,
    pub dt_seconds: f32,
    pub ships: &'a mut [ShipState; 2],
    pub debris: &'a mut [DebrisState],
    pub sun: Option<SunState>,
    pub planets: &'a [PlanetState],
    pub rovers: &'a [RoverState],
    pub docked_planets: &'a [Option<usize>; 2],
}

impl SpacewarsPhysics {
    pub fn new(
        universe_radius: f32,
        ships: &[ShipState; 2],
        sun: Option<SunState>,
        planets: &[PlanetState],
    ) -> Self {
        let mut physics = Self {
            world: PhysicsWorld::new(PhysicsWorldConfig {
                gravity: Vec2::ZERO,
                length_unit: 10.0,
                solver_iterations: 8,
                internal_stabilization_iterations: 2,
                max_ccd_substeps: 4,
                collect_events: true,
            }),
            terrain_fragments: BTreeSet::new(),
            material_planets: BTreeSet::new(),
            material_queries_dirty: false,
            disabled_spaceports: BTreeSet::new(),
            sun_radius: None,
            ship_keys: [None, None],
            docked_planets: [None, None],
            surface_ships: None,
            surface_recovery: false,
            tick: 0,
            contact_last_seen: BTreeMap::new(),
            pre_step_motions: BTreeMap::new(),
            planet_keys: vec![None; planets.len()],
            rovers: BTreeMap::new(),
            debris_keys: BTreeMap::new(),
            next_debris_entity: DEBRIS_ENTITY_BASE,
        };
        physics.world.reserve(
            ships.len() + planets.len() + 2,
            ships.len() + planets.len() * 3 + 2,
            0,
        );
        let _ = physics.insert_world_boundary(universe_radius);
        if let Some(sun) = sun {
            let _ = physics.insert_sun(sun);
        }
        for (index, planet) in planets.iter().enumerate() {
            let _ = physics.insert_planet(index, planet);
        }
        for (index, ship) in ships.iter().enumerate() {
            let _ = physics.insert_ship(index, ship, false, false, false);
        }
        physics
    }

    pub fn reconcile(&mut self, input: PhysicsReconcileInput<'_>) -> PhysicsLifecycle {
        let PhysicsReconcileInput {
            tick,
            dt_seconds,
            ships,
            debris,
            sun,
            planets,
            rovers,
            docked_planets,
        } = input;
        self.tick = tick;
        let mut lifecycle = PhysicsLifecycle::default();
        match sun {
            Some(sun) if self.sun_radius != Some(sun.radius.to_bits()) => {
                lifecycle.removed += usize::from(self.world.remove_entity(sun_entity()));
                lifecycle.added += usize::from(self.insert_sun(sun));
            }
            Some(sun) => {
                let _ = self
                    .world
                    .set_pose(primary_body(sun_entity()), sun.position, 0.0, false);
            }
            None if self.sun_radius.take().is_some() => {
                lifecycle.removed += usize::from(self.world.remove_entity(sun_entity()));
            }
            None => {}
        }

        if self.planet_keys.len() > planets.len() {
            for index in planets.len()..self.planet_keys.len() {
                lifecycle.removed += usize::from(self.world.remove_entity(planet_entity(index)));
            }
            self.planet_keys.truncate(planets.len());
        } else if self.planet_keys.len() < planets.len() {
            self.planet_keys.resize(planets.len(), None);
        }
        for (index, planet) in planets.iter().enumerate() {
            let key = PlanetColliderKey {
                radius: planet.radius.to_bits(),
            };
            if self.planet_keys[index] != Some(key) {
                lifecycle.removed += usize::from(self.world.remove_entity(planet_entity(index)));
                lifecycle.added += usize::from(self.insert_planet(index, planet));
            } else {
                let _ = self.world.set_next_kinematic_pose(
                    primary_body(planet_entity(index)),
                    planet.position,
                    planet.wrapper_angle,
                );
            }
        }

        for (index, ship) in ships.iter_mut().enumerate() {
            if self
                .surface_ships
                .as_ref()
                .is_some_and(|active| !active.contains(&index))
            {
                // The single-pilot fixture retains the legacy two-slot data
                // layout, but does not simulate an invisible second craft.
                continue;
            }
            if self
                .surface_ships
                .as_ref()
                .is_some_and(|active| active.contains(&index))
            {
                let changed = self.reconcile_surface_vehicle(index, ship);
                lifecycle.added += changed.added;
                lifecycle.removed += changed.removed;
                continue;
            }
            // A pod is captured automatically so it can rebuild. A full ship
            // establishes the stronger docking hold with its brake; otherwise
            // a ship that merely coasts across the pad would be latched there
            // indefinitely. Once held, it may turn in place with the brake
            // released, and actual thrust begins departure.
            let held_planet = self.docked_planets[index];
            let departure_control = ship.form == ShipForm::Ship
                && held_planet.is_some()
                && ship.brake <= 0.0
                && ship.thrust.abs() > f32::EPSILON;
            let can_establish_hold = ship.form == ShipForm::EscapePod || ship.brake > 0.0;
            let can_retain_hold = held_planet.is_some() && !departure_control;
            let sensor_docked_planet = if departure_control {
                None
            } else {
                docked_planets[index]
            };
            // Keep an actively held ship in its stable docked representation
            // across transient sensor gaps. A new intersection can always
            // establish the hold again.
            let retained_dock = can_retain_hold;
            let docked_planet = sensor_docked_planet
                .or_else(|| ship.spaceport_ejection.map(|ejection| ejection.planet))
                .or_else(|| {
                    retained_dock
                        .then_some(self.docked_planets[index])
                        .flatten()
                })
                .filter(|planet| *planet < planets.len());
            let docked = docked_planet.is_some();
            let constrained = docked
                && (can_establish_hold || can_retain_hold)
                && ship.spaceport_ejection.is_none();
            // Only the deliberate physical hold is remembered. Raw sensor
            // overlap is recomputed by Rapier and must not feed back into an
            // automatic full-ship latch on the next tick.
            self.docked_planets[index] = constrained.then_some(docked_planet).flatten();
            let constrained_planet = docked_planet
                .filter(|_| constrained)
                .and_then(|index| planets.get(index));
            if let Some(planet) = constrained_planet {
                let center = spaceport_docking_anchor(planet);
                let frame_velocity = planet_surface_velocity(planet, center);
                // Docking is a gameplay constraint, not a free-orbit
                // collision. Match the moving pad and close the remaining
                // offset at a bounded linear rate after ship controls and
                // wing animation have updated for this tick.
                ship.velocity = frame_velocity + (center - ship.position) * SPACEPORT_PULL_SCALE;
            }
            // Once the full hull reaches the pad, retain it as a sensor probe
            // and use the compact physical shape throughout the docked stay.
            // This keeps the touchdown footprint stable while the ship turns.
            let compact = ship.form == ShipForm::Ship && docked;
            let key = ShipColliderKey {
                form: ship.form,
                wing_theta: ship.wing_theta.to_bits(),
                docked,
                compact,
                constrained,
            };
            if self.ship_keys[index] != Some(key) {
                lifecycle.removed += usize::from(self.world.remove_entity(ship_entity(index)));
                lifecycle.added += usize::from(self.insert_ship(
                    index,
                    ship,
                    key.docked,
                    key.compact,
                    key.constrained,
                ));
            } else {
                synchronize_ship_to_physics(&mut self.world, index, ship);
            }
        }

        self.reconcile_rovers(rovers, planets, dt_seconds, &mut lifecycle);
        self.reconcile_debris(tick, debris, &mut lifecycle);
        lifecycle
    }

    pub fn step(&mut self, dt_seconds: f32) -> PhysicsStepMetrics {
        self.capture_pre_step_motions();
        let metrics = self.world.step(dt_seconds);
        if dt_seconds.is_finite() && dt_seconds > 0.0 {
            self.material_queries_dirty = false;
        }
        metrics
    }

    pub fn ship_is_constrained(&self, index: usize) -> bool {
        self.ship_keys
            .get(index)
            .and_then(|key| *key)
            .is_some_and(|key| key.constrained)
    }

    /// Configure the fixture's active vehicles, with real landing feet and no ports.
    /// Called only during setup, before any actors occupy or leave these ships.
    pub(super) fn enable_surface_sortie(&mut self, indices: &[usize], ships: &[ShipState]) {
        self.surface_ships = Some(indices.to_vec());
        for (index, ship) in ships.iter().enumerate().take(self.ship_keys.len()) {
            self.world.remove_entity(ship_entity(index));
            self.ship_keys[index] = None;
            self.docked_planets[index] = None;
            if indices.contains(&index) {
                assert!(self.insert_ship(index, ship, false, false, false));
            }
        }
    }

    /// An intact terminal attached to existing terrain: one collider, no new body.
    /// This fixture has fixed planet geometry; regeneration/destruction is a later policy.
    pub(super) fn insert_surface_terminal(
        &mut self,
        planet: usize,
        radius: f32,
        local_angle: f32,
    ) -> bool {
        let entity = planet_entity(planet);
        let mut terminal = ColliderSpec::cuboid(
            collider_id(entity, OUTPOST_TERMINAL_ROLE, 0),
            OUTPOST_TERMINAL_HALF_SIZE.x,
            OUTPOST_TERMINAL_HALF_SIZE.y,
        );
        terminal.local_position = Vec2::from_radians(local_angle)
            * (radius * BODY_BOUNDS_RADIUS_SCALE + OUTPOST_TERMINAL_HALF_SIZE.y - 0.02);
        terminal.local_angle = local_angle - core::f32::consts::FRAC_PI_2;
        terminal.density = 0.0;
        terminal.friction = 0.8;
        terminal.collision_groups = CollisionGroups::new(
            GROUP_BODY | GROUP_ROVER_SURFACE,
            GROUP_ALL_SHIPS | GROUP_DEBRIS | GROUP_ROVER | GROUP_SPACELING,
        );
        terminal.solver_groups = terminal.collision_groups;
        self.world.insert_collider(primary_body(entity), &terminal)
    }

    pub(super) fn ship_body(&self, index: usize) -> PhysicsBodyId {
        primary_body(ship_entity(index))
    }

    pub(super) fn enable_surface_recovery(&mut self) {
        self.surface_recovery = true;
    }

    pub(super) fn surface_vehicle_changed(&self, index: usize, ship: &ShipState) -> bool {
        self.ship_keys[index].map(|key| key.form) != (!ship.dead).then_some(ship.form)
    }

    /// Materialize a loss/replacement without another physics step or touching
    /// terrain targets. Ordinary ships retain their existing lifecycle path.
    pub(super) fn reconcile_surface_vehicle(
        &mut self,
        index: usize,
        ship: &ShipState,
    ) -> PhysicsLifecycle {
        debug_assert!(
            self.surface_ships
                .as_ref()
                .is_some_and(|active| active.contains(&index))
        );
        let mut lifecycle = PhysicsLifecycle::default();
        let next = (!ship.dead).then_some(ShipColliderKey {
            form: ship.form,
            wing_theta: ship.wing_theta.to_bits(),
            docked: false,
            compact: false,
            constrained: false,
        });
        self.docked_planets[index] = None;
        if let (Some(previous), Some(next_key)) = (self.ship_keys[index], next)
            && previous.form == ShipForm::Ship
            && next_key.form == ShipForm::Ship
            && previous.wing_theta != next_key.wing_theta
        {
            // Folding changes only the hull. Keep the body and rear feet, and
            // preserve origin velocity when the new silhouette moves the COM.
            synchronize_ship_to_physics(&mut self.world, index, ship);
            let body = self.ship_body(index);
            let motion = self.world.motion(body).expect("existing surface ship");
            let origin_velocity = self.world.velocity_at_point(body, motion.position).unwrap();
            let colliders = surface_ship_colliders(ship_entity(index), ship, self.surface_recovery);
            assert!(
                self.world
                    .replace_colliders(body, SHIP_HULL_ROLE, &colliders[..1])
            );
            assert!(self.world.refresh_mass_properties(body));
            let offset = self.world.center_of_mass(body).unwrap() - motion.position;
            self.world.set_velocity(
                body,
                origin_velocity + Vec2::new(-offset.y, offset.x) * motion.angular_velocity,
                motion.angular_velocity,
                true,
            );
            self.ship_keys[index] = next;
            self.material_queries_dirty = true;
            return lifecycle;
        }
        if self.ship_keys[index] != next {
            lifecycle.removed += usize::from(self.world.remove_entity(ship_entity(index)));
            self.ship_keys[index] = None;
            // Replacement contacts belong to the new assembly, not its old hull.
            self.contact_last_seen.retain(|(a, b), _| {
                *a != MechanicalEntity::Ship(index) && *b != MechanicalEntity::Ship(index)
            });
            if next.is_some() {
                lifecycle.added += usize::from(self.insert_ship(index, ship, false, false, false));
            }
        } else if next.is_some() {
            synchronize_ship_to_physics(&mut self.world, index, ship);
        }
        lifecycle
    }

    pub(super) fn landing_geometry(&self, index: usize) -> ([Vec2; 2], f32) {
        surface_landing_geometry(self.ship_keys[index].map_or(ShipForm::Ship, |key| key.form))
    }

    pub(super) fn surface_vehicle_clearance_radius(ship: &ShipState) -> f32 {
        let (feet, radius) = surface_landing_geometry(ship.form);
        ship_collision_hull(ship)
            .iter()
            .map(|p| p.length())
            .chain(feet.map(|p| p.length() + radius))
            .fold(0.0, f32::max)
            + 0.1
    }

    pub(super) fn surface_vehicle_space_is_clear(
        &self,
        ship: &ShipState,
        center: Vec2,
        radius: f32,
    ) -> bool {
        if self.material_queries_dirty {
            return false;
        }
        let mut groups = ship_collision_groups(ship, false);
        groups.filter = (groups.filter & !(GROUP_BODY | GROUP_SPACEPORT_SENSOR))
            | GROUP_ROVER_SURFACE
            | GROUP_MATERIAL;
        self.world
            .capsule_is_clear(center, 0.0, 0.0, radius, groups)
    }

    pub(super) fn planet_body(&self, index: usize) -> PhysicsBodyId {
        primary_body(planet_entity(index))
    }

    pub(super) fn material_ground_ray(
        &self,
        planet: usize,
        origin: Vec2,
        direction: Vec2,
        distance: f32,
    ) -> Option<engine_rapier::world::RayHit> {
        if self.material_queries_dirty {
            return None;
        }
        let hit = self.world.cast_ray(
            origin,
            direction,
            RayCastOptions {
                max_distance: distance,
                collision_groups: CollisionGroups::new(GROUP_SPACELING, GROUP_MATERIAL),
                ..RayCastOptions::default()
            },
        )?;
        is_planet_surface_support(hit.collider, planet).then_some(hit)
    }

    /// Solver-backed rear-foot support within contact slop, not hull or port overlap.
    pub(super) fn landing_feet_supported(&self, index: usize, planet: usize, up: Vec2) -> usize {
        self.landing_support_contacts(index, planet, up)
            .into_iter()
            .flatten()
            .count()
    }

    pub(super) fn landing_support_contacts(
        &self,
        index: usize,
        planet: usize,
        up: Vec2,
    ) -> [Option<engine_rapier::world::SurfaceContact>; 2] {
        let body = self.ship_body(index);
        std::array::from_fn(|part| {
            self.world
                .surface_contacts(collider_id(
                    ship_entity(index),
                    LANDING_FOOT_ROLE,
                    part as u16,
                ))
                .find(|contact| {
                    let velocity = self
                        .world
                        .velocity_at_point(body, contact.position)
                        .unwrap();
                    is_planet_surface_support(contact.collider, planet)
                        && contact.separation <= 0.04
                        && contact.normal.dot(up) >= 0.7
                        && (velocity - contact.velocity).dot(contact.normal) <= 1.0
                })
        })
    }

    fn capture_pre_step_motions(&mut self) {
        let mut motions = BTreeMap::new();
        let mut capture = |entity, body| {
            if let Some(motion) = self.world.motion(body) {
                motions.insert(entity, motion);
            }
        };

        if self.sun_radius.is_some() {
            capture(
                MechanicalEntity::Body(BodyId::Sun),
                primary_body(sun_entity()),
            );
        }
        for (index, key) in self.planet_keys.iter().enumerate() {
            if key.is_some() {
                capture(
                    MechanicalEntity::Body(BodyId::Planet(index)),
                    primary_body(planet_entity(index)),
                );
            }
        }
        for index in 0..self.ship_keys.len() {
            if self.ship_keys[index].is_some() {
                capture(
                    MechanicalEntity::Ship(index),
                    primary_body(ship_entity(index)),
                );
            }
        }
        for id in &self.terrain_fragments {
            capture(
                MechanicalEntity::TerrainFragment(*id),
                primary_body(PhysicsId::new(*id)),
            );
        }
        for id in self.debris_keys.keys().copied() {
            capture(
                MechanicalEntity::Debris(id),
                primary_body(PhysicsId::new(id)),
            );
        }
        self.pre_step_motions = motions;
    }

    fn contact_closing_speed(
        &self,
        a: MechanicalEntity,
        b: MechanicalEntity,
        point: Option<Vec2>,
        normal: Vec2,
    ) -> f32 {
        let velocity_at = |entity| {
            let motion = self.pre_step_motions.get(&entity)?;
            let mut velocity = motion.linear_velocity;
            if let Some(point) = point {
                let lever = point - motion.position;
                velocity += Vec2::new(
                    -motion.angular_velocity * lever.y,
                    motion.angular_velocity * lever.x,
                );
            }
            Some(velocity)
        };
        let velocity_a = velocity_at(a).unwrap_or(Vec2::ZERO);
        let velocity_b = velocity_at(b).unwrap_or(Vec2::ZERO);
        (-(velocity_b - velocity_a).dot(normal)).max(0.0)
    }

    pub fn apply_velocity_delta(&mut self, entity: MechanicalEntity, delta_velocity: Vec2) -> bool {
        let entity = match entity {
            MechanicalEntity::Ship(index) => ship_entity(index),
            MechanicalEntity::Debris(id) | MechanicalEntity::TerrainFragment(id) => {
                PhysicsId::new(id)
            }
            MechanicalEntity::World | MechanicalEntity::Body(_) | MechanicalEntity::Rover(_) => {
                return false;
            }
        };
        self.world
            .apply_velocity_delta(primary_body(entity), delta_velocity, true)
    }

    pub fn rover_snapshot(&self, rover: &RoverState) -> Option<RoverSnapshot> {
        let entry = self.rover_entry(rover)?;
        entry.assembly.snapshot(&self.world)
    }

    pub fn rover_body_motions(&self, rover: &RoverState) -> Option<[BodyMotion; 3]> {
        let bodies = self.rover_entry(rover)?.assembly.bodies();
        Some([
            self.world.motion(bodies[0])?,
            self.world.motion(bodies[1])?,
            self.world.motion(bodies[2])?,
        ])
    }

    pub fn rover_total_mass(&self, rover: &RoverState) -> Option<f32> {
        self.rover_entry(rover)?
            .assembly
            .bodies()
            .into_iter()
            .try_fold(0.0, |total, body| Some(total + self.world.body_mass(body)?))
    }

    pub fn apply_rover_velocity_delta(
        &mut self,
        id: u64,
        body_index: usize,
        delta_velocity: Vec2,
    ) -> bool {
        let Some(body) = self
            .rovers
            .get(&id)
            .and_then(|entry| entry.assembly.bodies().get(body_index).copied())
        else {
            return false;
        };
        self.world.apply_velocity_delta(body, delta_velocity, true)
    }

    #[cfg(test)]
    pub fn body_count(&self) -> usize {
        self.world.body_count()
    }

    #[cfg(test)]
    pub fn snapshot_bytes(&self) -> Vec<u8> {
        self.world
            .snapshot_bytes()
            .expect("same-build Spacewars physics snapshot")
    }

    pub fn synchronize_motion(&self, ships: &mut [ShipState; 2], debris: &mut [DebrisState]) {
        for (index, ship) in ships.iter_mut().enumerate() {
            let Some(motion) = self.world.motion(primary_body(ship_entity(index))) else {
                continue;
            };
            ship.position = motion.position - ship_pivot(ship.form);
            ship.velocity = motion.linear_velocity;
            ship.rotation_radians = motion.angle;
            ship.direction = super::direction_from_rotation(motion.angle);
            ship.omega = control_angular_velocity(ship, motion.angular_velocity);
        }

        for item in debris {
            if item.physics_id == 0 {
                continue;
            }
            let Some(motion) = self
                .world
                .motion(primary_body(PhysicsId::new(item.physics_id)))
            else {
                continue;
            };
            item.position = motion.position;
            item.velocity = motion.linear_velocity;
            item.rotation_radians = motion.angle;
            item.omega = motion.angular_velocity;
        }
    }

    fn rover_entry(&self, rover: &RoverState) -> Option<&RoverPhysicsEntry> {
        let entry = self.rovers.get(&rover.id)?;
        (entry.planet == rover.planet && entry.owner_id == rover.owner_id).then_some(entry)
    }

    fn reconcile_rovers(
        &mut self,
        rovers: &[RoverState],
        planets: &[PlanetState],
        dt_seconds: f32,
        lifecycle: &mut PhysicsLifecycle,
    ) {
        let mut active = BTreeSet::new();
        for rover in rovers {
            if !active.insert(rover.id) {
                continue;
            }
            let key_matches = self
                .rovers
                .get(&rover.id)
                .map(|entry| entry.planet == rover.planet && entry.owner_id == rover.owner_id)
                .unwrap_or(false);
            if !key_matches {
                if let Some(entry) = self.rovers.remove(&rover.id) {
                    lifecycle.removed += usize::from(entry.assembly.remove(&mut self.world));
                }
                let Some(planet) = planets.get(rover.planet) else {
                    continue;
                };
                if let Some(entry) = self.insert_rover(rover, planet) {
                    self.rovers.insert(rover.id, entry);
                    lifecycle.added += 1;
                }
            }

            if let Some(entry) = self.rovers.get_mut(&rover.id) {
                if key_matches && let Some(planet) = planets.get(entry.planet) {
                    transport_rover_with_planet(&mut self.world, entry, planet, dt_seconds);
                }
                let _ = entry
                    .assembly
                    .set_control(&mut self.world, rover.intent.control());
            }
        }

        let stale = self
            .rovers
            .keys()
            .copied()
            .filter(|id| !active.contains(id))
            .collect::<Vec<_>>();
        for id in stale {
            if let Some(entry) = self.rovers.remove(&id) {
                lifecycle.removed += usize::from(entry.assembly.remove(&mut self.world));
            }
        }
    }

    fn insert_rover(
        &mut self,
        rover: &RoverState,
        planet: &PlanetState,
    ) -> Option<RoverPhysicsEntry> {
        let spec = spacewars_rover_spec();
        let assembly = RoverAssembly::insert_at(
            &mut self.world,
            rover_entity(rover.id),
            spacewars_rover_spawn_pose(planet),
            spec,
        )?;

        for body in assembly.bodies() {
            let Some(motion) = self.world.motion(body) else {
                assembly.remove(&mut self.world);
                return None;
            };
            if !self.world.set_velocity(
                body,
                planet_surface_velocity(planet, motion.position),
                planet.wrapper_omega,
                true,
            ) {
                assembly.remove(&mut self.world);
                return None;
            }
        }
        let _ = assembly.set_control(&mut self.world, rover.intent.control());
        Some(RoverPhysicsEntry {
            planet: rover.planet,
            owner_id: rover.owner_id,
            last_planet: *planet,
            assembly,
        })
    }

    pub fn contacts(&mut self) -> Vec<MechanicalContact> {
        let mut contacts = self
            .world
            .contact_events()
            .iter()
            .filter_map(|event| {
                let a = classify_entity(event.collider_a.entity)?;
                let b = classify_entity(event.collider_b.entity)?;
                (a != b).then_some(MechanicalContact {
                    a,
                    b,
                    point: event.point,
                    normal: event.normal,
                    impulse_magnitude: event.impulse_magnitude,
                    closing_speed: self.contact_closing_speed(a, b, event.point, event.normal),
                    started: false,
                })
            })
            .collect::<Vec<_>>();
        let current_pairs = contacts
            .iter()
            .map(|contact| ordered_entity_pair(contact.a, contact.b))
            .collect::<BTreeSet<_>>();
        for contact in &mut contacts {
            let pair = ordered_entity_pair(contact.a, contact.b);
            contact.started = self
                .contact_last_seen
                .get(&pair)
                .is_none_or(|last_seen| self.tick.saturating_sub(*last_seen) > CONTACT_REARM_TICKS);
        }
        for pair in current_pairs {
            self.contact_last_seen.insert(pair, self.tick);
        }
        self.contact_last_seen
            .retain(|_, last_seen| self.tick.saturating_sub(*last_seen) <= CONTACT_REARM_TICKS);
        contacts
    }

    pub fn spaceport_contacts(&self) -> Vec<(usize, usize, bool)> {
        let mut contacts = BTreeMap::new();
        for intersection in self.world.sensor_intersections() {
            let pair = [intersection.collider_a, intersection.collider_b];
            let Some(port) = pair
                .iter()
                .find(|collider| collider.role == SPACEPORT_SENSOR_ROLE)
            else {
                continue;
            };
            let Some(ship) = pair.iter().find_map(|collider| ship_index(collider.entity)) else {
                continue;
            };
            let Some(planet) = planet_index(port.entity) else {
                continue;
            };
            if !self.disabled_spaceports.contains(&planet) {
                contacts.entry((ship, planet)).or_insert(false);
            }
        }
        for (ship, planet) in self.docked_planets.iter().copied().enumerate() {
            if let Some(planet) = planet {
                contacts.insert((ship, planet), true);
            }
        }
        contacts
            .into_iter()
            .map(|((ship, planet), landed)| (ship, planet, landed))
            .collect()
    }

    pub fn cast_laser(
        &self,
        shooter: usize,
        origin: Vec2,
        direction: Vec2,
        max_distance: f32,
    ) -> Option<LaserTrace> {
        let hit = self.world.cast_ray(
            origin,
            direction,
            RayCastOptions {
                max_distance,
                solid: false,
                include_sensors: false,
                collision_groups: CollisionGroups::new(u32::MAX, GROUP_ALL_SOLIDS),
                exclude_entity: Some(ship_entity(shooter)),
            },
        )?;
        Some(LaserTrace {
            target: classify_entity(hit.collider.entity),
            point: hit.point,
        })
    }

    fn reconcile_debris(
        &mut self,
        tick: u64,
        debris: &mut [DebrisState],
        lifecycle: &mut PhysicsLifecycle,
    ) {
        let mut active = BTreeSet::new();
        for item in debris.iter_mut().filter(|item| !item.dead) {
            if item.physics_id == 0 || !active.insert(item.physics_id) {
                item.physics_id = self.next_debris_entity;
                self.next_debris_entity += 1;
                active.insert(item.physics_id);
            }

            let key = DebrisColliderKey {
                signature: debris_signature(item),
                armed: if self.surface_recovery
                    && item.kind == DebrisKind::Fragment
                    && item.owner_id.is_some()
                {
                    // Half a second at the scenario's fixed 60 Hz. Keep collisions
                    // with terrain/debris/opponent vehicles during breakup grace.
                    tick.saturating_sub(item.spawn_tick) >= RECOVERY_BREAKUP_GRACE_TICKS
                } else {
                    item.spawn_tick < tick
                },
            };
            if self.debris_keys.get(&item.physics_id) != Some(&key) {
                let entity = PhysicsId::new(item.physics_id);
                lifecycle.removed += usize::from(self.world.remove_entity(entity));
                lifecycle.added += usize::from(self.insert_debris(item, key.armed));
                self.debris_keys.insert(item.physics_id, key);
            } else {
                synchronize_debris_to_physics(&mut self.world, item);
            }
        }

        let stale = self
            .debris_keys
            .keys()
            .copied()
            .filter(|id| !active.contains(id))
            .collect::<Vec<_>>();
        for id in stale {
            lifecycle.removed += usize::from(self.world.remove_entity(PhysicsId::new(id)));
            self.debris_keys.remove(&id);
        }
    }

    fn insert_world_boundary(&mut self, radius: f32) -> bool {
        let entity = world_entity();
        let center = Vec2::new(radius, radius);
        let mut collider = ColliderSpec::polyline(
            collider_id(entity, WORLD_ROLE, 0),
            circle(radius, WORLD_SURFACE_SEGMENTS),
        );
        collider.restitution = DEFAULT_ELASTICITY;
        collider.friction = 0.0;
        collider.collision_groups = CollisionGroups::new(
            GROUP_WORLD,
            GROUP_ALL_SHIPS | GROUP_DEBRIS | GROUP_ROVER | GROUP_BODY | GROUP_SPACELING,
        );
        collider.solver_groups = collider.collision_groups;
        let inserted = self.world.insert_body(
            primary_body(entity),
            BodySpec {
                kind: BodyKind::Fixed,
                position: center,
                ..BodySpec::default()
            },
            &[collider],
        );
        debug_assert!(inserted);
        inserted
    }

    fn insert_sun(&mut self, sun: SunState) -> bool {
        let entity = sun_entity();
        let mut collider =
            ColliderSpec::ball(collider_id(entity, BODY_SURFACE_ROLE, 0), sun.radius);
        collider.density = 0.0;
        collider.friction = 0.0;
        collider.restitution = PLANET_ELASTICITY;
        collider.collision_groups =
            CollisionGroups::new(GROUP_BODY, GROUP_ALL_SHIPS | GROUP_DEBRIS | GROUP_BODY);
        collider.solver_groups = collider.collision_groups;
        let inserted = self.world.insert_body(
            primary_body(entity),
            BodySpec {
                kind: BodyKind::Fixed,
                position: sun.position,
                ..BodySpec::default()
            },
            &[collider],
        );
        debug_assert!(inserted);
        self.sun_radius = Some(sun.radius.to_bits());
        inserted
    }

    pub fn add_spaceport_sensor(&mut self, index: usize, planet: &PlanetState) {
        let entity = planet_entity(index);
        let mut sensor = ColliderSpec::convex_polygon(
            spaceport_sensor_id(index),
            spaceport_local_points(planet.radius),
        );
        sensor.density = 0.0;
        sensor.sensor = true;
        sensor.collision_groups = CollisionGroups::new(GROUP_SPACEPORT_SENSOR, GROUP_ALL_SHIPS);
        sensor.solver_groups = CollisionGroups::NONE;
        assert!(self.world.replace_colliders(
            primary_body(entity),
            SPACEPORT_SENSOR_ROLE,
            &[sensor]
        ));
    }

    pub fn disable_spaceport(&mut self, index: usize) {
        if !self.disabled_spaceports.insert(index) {
            return;
        }
        assert!(self.world.replace_colliders(
            primary_body(planet_entity(index)),
            SPACEPORT_SENSOR_ROLE,
            &[]
        ));
        for held in &mut self.docked_planets {
            if *held == Some(index) {
                *held = None;
            }
        }
    }

    fn insert_planet(&mut self, index: usize, planet: &PlanetState) -> bool {
        let entity = planet_entity(index);
        let body_groups =
            CollisionGroups::new(GROUP_BODY, GROUP_ALL_SHIPS | GROUP_DEBRIS | GROUP_BODY);
        let mut colliders = planet_solid_colliders(entity, planet.radius, body_groups);
        // Ordinary ships use the elastic surface. Surface vehicles and
        // spacelings opt into traction instead; never both coincident surfaces.
        let mut rover_surface = ColliderSpec::ball(
            collider_id(entity, ROVER_SURFACE_ROLE, 0),
            planet.radius * BODY_BOUNDS_RADIUS_SCALE,
        );
        rover_surface.density = 0.0;
        rover_surface.friction = 1.25;
        rover_surface.restitution = 0.0;
        rover_surface.collision_groups = CollisionGroups::new(
            GROUP_ROVER_SURFACE,
            GROUP_ROVER | GROUP_SPACELING | GROUP_ALL_SHIPS,
        );
        rover_surface.solver_groups = rover_surface.collision_groups;
        colliders.push(rover_surface);

        let mut sensor = ColliderSpec::convex_polygon(
            collider_id(entity, SPACEPORT_SENSOR_ROLE, 0),
            spaceport_local_points(planet.radius),
        );
        sensor.density = 0.0;
        sensor.sensor = true;
        sensor.collision_groups = CollisionGroups::new(GROUP_SPACEPORT_SENSOR, GROUP_ALL_SHIPS);
        sensor.solver_groups = CollisionGroups::NONE;

        colliders.push(sensor);

        let inserted = self.world.insert_body(
            primary_body(entity),
            BodySpec {
                kind: BodyKind::KinematicPosition,
                position: planet.position,
                angle: planet.wrapper_angle,
                can_sleep: false,
                ..BodySpec::default()
            },
            &colliders,
        );
        debug_assert!(inserted);
        self.planet_keys[index] = Some(PlanetColliderKey {
            radius: planet.radius.to_bits(),
        });
        inserted
    }

    fn insert_ship(
        &mut self,
        index: usize,
        ship: &ShipState,
        docked: bool,
        compact: bool,
        constrained: bool,
    ) -> bool {
        let entity = ship_entity(index);
        let colliders = if self
            .surface_ships
            .as_ref()
            .is_some_and(|active| active.contains(&index))
        {
            surface_ship_colliders(entity, ship, self.surface_recovery)
        } else {
            ship_colliders(entity, ship, docked, compact)
        };
        let inserted = self.world.insert_body(
            primary_body(entity),
            BodySpec {
                kind: if constrained {
                    BodyKind::KinematicVelocity
                } else {
                    BodyKind::Dynamic
                },
                position: ship.position + ship_pivot(ship.form),
                angle: ship.rotation_radians,
                linear_velocity: ship.velocity,
                angular_velocity: physical_angular_velocity(ship),
                gravity_scale: 0.0,
                can_sleep: false,
                ccd_enabled: true,
                additional_solver_iterations: 2,
                ..BodySpec::default()
            },
            &colliders,
        );
        debug_assert!(inserted);
        self.ship_keys[index] = Some(ShipColliderKey {
            form: ship.form,
            wing_theta: ship.wing_theta.to_bits(),
            docked,
            compact,
            constrained,
        });
        inserted
    }

    fn insert_debris(&mut self, debris: &DebrisState, armed: bool) -> bool {
        let entity = PhysicsId::new(debris.physics_id);
        let groups = debris_collision_groups(debris.owner_id, armed);
        let mut collider = match debris.kind {
            DebrisKind::Asteroid => {
                ColliderSpec::ball(collider_id(entity, DEBRIS_ROLE, 0), debris.radius)
            }
            DebrisKind::Fragment => ColliderSpec::convex_polygon(
                collider_id(entity, DEBRIS_ROLE, 0),
                debris
                    .fragment_shape
                    .unwrap_or([
                        Vec2::new(-debris.radius, -debris.radius),
                        Vec2::new(debris.radius, 0.0),
                        Vec2::new(-debris.radius, debris.radius),
                    ])
                    .to_vec(),
            ),
            DebrisKind::Shell => ColliderSpec::convex_polygon(
                collider_id(entity, DEBRIS_ROLE, 0),
                SHELL_BODY.to_vec(),
            ),
        };
        let area = match debris.kind {
            DebrisKind::Asteroid => core::f32::consts::PI * debris.radius * debris.radius,
            DebrisKind::Fragment => triangle_area(debris.fragment_shape.unwrap_or(SHELL_BODY)),
            DebrisKind::Shell => triangle_area(SHELL_BODY),
        };
        collider.density = debris.mass() / area.max(f32::EPSILON);
        collider.friction = 0.0;
        collider.restitution = DEFAULT_ELASTICITY;
        collider.collision_groups = groups;
        collider.solver_groups = groups;
        let inserted = self.world.insert_body(
            primary_body(entity),
            BodySpec {
                kind: BodyKind::Dynamic,
                position: debris.position,
                angle: debris.rotation_radians,
                linear_velocity: debris.velocity,
                angular_velocity: debris.omega,
                gravity_scale: 0.0,
                can_sleep: false,
                ccd_enabled: debris_uses_ccd(debris),
                ..BodySpec::default()
            },
            &[collider],
        );
        debug_assert!(inserted);
        inserted
    }
}

fn debris_uses_ccd(debris: &DebrisState) -> bool {
    // Breakup fragments are non-damaging visual debris and can use discrete collision. Cannon
    // shells and small damage-bearing asteroids retain CCD so fast gameplay impacts cannot tunnel.
    debris.kind == DebrisKind::Shell
        || (debris.kind == DebrisKind::Asteroid && debris.radius <= CANNON_SHELL_RADIUS)
}

fn synchronize_ship_to_physics(world: &mut PhysicsWorld, index: usize, ship: &ShipState) {
    let body = primary_body(ship_entity(index));
    let desired_position = ship.position + ship_pivot(ship.form);
    let Some(motion) = world.motion(body) else {
        return;
    };
    if motion.position != desired_position || motion.angle != ship.rotation_radians {
        let _ = world.set_pose(body, desired_position, ship.rotation_radians, true);
    }
    let angular_velocity = physical_angular_velocity(ship);
    if motion.linear_velocity != ship.velocity || motion.angular_velocity != angular_velocity {
        let _ = world.set_velocity(body, ship.velocity, angular_velocity, true);
    }
}

fn synchronize_debris_to_physics(world: &mut PhysicsWorld, debris: &DebrisState) {
    let body = primary_body(PhysicsId::new(debris.physics_id));
    let Some(motion) = world.motion(body) else {
        return;
    };
    if motion.position != debris.position || motion.angle != debris.rotation_radians {
        let _ = world.set_pose(body, debris.position, debris.rotation_radians, true);
    }
    if motion.linear_velocity != debris.velocity || motion.angular_velocity != debris.omega {
        let _ = world.set_velocity(body, debris.velocity, debris.omega, true);
    }
}

/// Carry free dynamic rover motion between successive poses of its scripted
/// kinematic planet. Rapier still resolves suspension, traction, contacts, and
/// gravity; this supplies the non-physical frame acceleration implied by the
/// legacy orbital path so the rover is not left behind by its moving terrain.
fn transport_rover_with_planet(
    world: &mut PhysicsWorld,
    entry: &mut RoverPhysicsEntry,
    planet: &PlanetState,
    dt_seconds: f32,
) {
    if !dt_seconds.is_finite() || dt_seconds <= 0.0 {
        entry.last_planet = *planet;
        return;
    }

    let previous = entry.last_planet;
    let angle_delta = planet.wrapper_angle - previous.wrapper_angle;
    for body in entry.assembly.bodies() {
        let Some(motion) = world.motion(body) else {
            continue;
        };
        let next_position =
            planet.position + (motion.position - previous.position).rotate_radians(angle_delta);
        let frame_velocity = planet_surface_velocity(planet, next_position);
        let relative_velocity =
            motion.linear_velocity - planet_surface_velocity(&previous, motion.position);
        let linear_velocity = frame_velocity + relative_velocity.rotate_radians(angle_delta);
        let relative_angular_velocity = motion.angular_velocity - previous.wrapper_omega;
        let angular_velocity = angle_delta / dt_seconds + relative_angular_velocity;
        let _ = world.set_velocity(body, linear_velocity, angular_velocity, true);
    }
    entry.last_planet = *planet;
}

fn ship_collision_groups(ship: &ShipState, docked: bool) -> CollisionGroups {
    let membership = match ship.form {
        ShipForm::Ship if ship.owner_id == 0 => GROUP_SHIP_0,
        ShipForm::Ship => GROUP_SHIP_1,
        ShipForm::EscapePod if ship.owner_id == 0 => GROUP_POD_0,
        ShipForm::EscapePod => GROUP_POD_1,
    };
    let mut filter = GROUP_WORLD | GROUP_DEBRIS | GROUP_SPACEPORT_SENSOR | GROUP_SPACELING;
    if !docked {
        filter |= GROUP_BODY | GROUP_ALL_SHIPS;
    }
    CollisionGroups::new(membership, filter)
}

fn debris_collision_groups(owner_id: Option<usize>, armed: bool) -> CollisionGroups {
    let filter = if armed {
        GROUP_ALL_SOLIDS
    } else {
        let ships = match owner_id {
            Some(0) => GROUP_SHIP_1 | GROUP_POD_1,
            Some(1) => GROUP_SHIP_0 | GROUP_POD_0,
            _ => 0,
        };
        GROUP_BODY | GROUP_WORLD | GROUP_DEBRIS | ships
    };
    CollisionGroups::new(GROUP_DEBRIS, filter)
}

fn ship_local_triangles(ship: &ShipState) -> Vec<[Vec2; 3]> {
    let pivot = ship_pivot(ship.form);
    let centered = |points: [Vec2; 3]| points.map(|point| point - pivot);
    if ship.form == ShipForm::EscapePod {
        return vec![
            centered(POD_LASER),
            centered(POD_THRUSTER),
            centered(POD_BODY),
        ];
    }

    vec![
        centered(rotate_points(
            SHIP_LEFT_WING,
            SHIP_WING_PIVOT,
            ship.wing_theta,
        )),
        centered(rotate_points(
            SHIP_RIGHT_WING,
            SHIP_WING_PIVOT,
            -ship.wing_theta,
        )),
        centered(SHIP_WING_MOUNT),
        centered(SHIP_THRUSTER),
        centered(SHIP_BODY),
        centered(SHIP_LASER),
    ]
}

fn ship_colliders(
    entity: PhysicsId,
    ship: &ShipState,
    docked: bool,
    compact: bool,
) -> Vec<ColliderSpec> {
    debug_assert!(!compact || docked);
    let groups = ship_collision_groups(ship, docked);
    let primary_id = collider_id(entity, SHIP_HULL_ROLE, 0);
    let compact_while_docked = compact && ship.form == ShipForm::Ship;
    let hull = ship_collision_hull(ship);
    let (mut collider, area) = if compact_while_docked {
        (
            ColliderSpec::ball(primary_id, DOCKED_SHIP_COLLIDER_RADIUS),
            core::f32::consts::PI * DOCKED_SHIP_COLLIDER_RADIUS.powi(2),
        )
    } else {
        let area = polygon_area(&hull);
        (ColliderSpec::convex_polygon(primary_id, hull.clone()), area)
    };
    collider.density = ship.mass() / area.max(f32::EPSILON);
    collider.friction = 0.0;
    collider.restitution = DEFAULT_ELASTICITY;
    collider.collision_groups = groups;
    collider.solver_groups = groups;
    let mut colliders = vec![collider];

    if compact_while_docked {
        // Retain the normal hull exclusively as a sensor probe. This keeps the
        // established landing/capture footprint stable when the physical hull
        // becomes compact, preventing shallow contacts from flickering.
        let mut probe = ColliderSpec::convex_polygon(collider_id(entity, SHIP_HULL_ROLE, 1), hull);
        probe.density = 0.0;
        probe.sensor = true;
        probe.collision_groups = CollisionGroups::new(groups.memberships, GROUP_SPACEPORT_SENSOR);
        probe.solver_groups = CollisionGroups::NONE;
        colliders.push(probe);
    }

    colliders
}

fn surface_ship_colliders(
    entity: PhysicsId,
    ship: &ShipState,
    pod_landing: bool,
) -> Vec<ColliderSpec> {
    let mut colliders = ship_colliders(entity, ship, false, false);
    let hull = &mut colliders[0];
    hull.friction = 0.8;
    hull.restitution = 0.0;
    hull.collision_groups.filter = (hull.collision_groups.filter
        & !(GROUP_BODY | GROUP_SPACEPORT_SENSOR))
        | GROUP_ROVER_SURFACE
        | GROUP_MATERIAL;
    hull.solver_groups = hull.collision_groups;
    let groups = hull.collision_groups;
    if ship.form == ShipForm::Ship || pod_landing {
        let (feet, radius) = surface_landing_geometry(ship.form);
        for (part, position) in feet.into_iter().enumerate() {
            let mut foot =
                ColliderSpec::ball(collider_id(entity, LANDING_FOOT_ROLE, part as u16), radius);
            foot.local_position = position;
            // Small feet extend behind the hull; no overlapping hull pieces,
            // extra bodies/joints, or changes to the ship's inertial mass.
            foot.density = 0.0;
            foot.friction = 1.0;
            foot.collision_groups = groups;
            foot.solver_groups = groups;
            colliders.push(foot);
        }
    }
    colliders
}

/// Build one stable convex collision silhouette from the independently rendered
/// ship parts. A single collider prevents overlapping decorative triangles from
/// producing several solver impulses for one impact.
fn ship_collision_hull(ship: &ShipState) -> Vec<Vec2> {
    let mut points = ship_local_triangles(ship)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    points.sort_unstable_by(|a, b| a.x.total_cmp(&b.x).then_with(|| a.y.total_cmp(&b.y)));
    points.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if points.len() <= 2 {
        return points;
    }

    let mut lower = Vec::with_capacity(points.len());
    for &point in &points {
        while lower.len() >= 2
            && cross(
                lower[lower.len() - 1] - lower[lower.len() - 2],
                point - lower[lower.len() - 1],
            ) <= 0.0
        {
            lower.pop();
        }
        lower.push(point);
    }

    let mut upper = Vec::with_capacity(points.len());
    for &point in points.iter().rev() {
        while upper.len() >= 2
            && cross(
                upper[upper.len() - 1] - upper[upper.len() - 2],
                point - upper[upper.len() - 1],
            ) <= 0.0
        {
            upper.pop();
        }
        upper.push(point);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

pub(super) fn ship_pivot(form: ShipForm) -> Vec2 {
    match form {
        ShipForm::Ship => SHIP_PIVOT,
        ShipForm::EscapePod => POD_PIVOT,
    }
}

pub(super) fn physical_angular_velocity(ship: &ShipState) -> f32 {
    let scale = 1.0 - ship.turn_power / ship.delta_time.max(f32::EPSILON);
    ship.omega * scale
}

pub(super) fn control_angular_velocity(ship: &ShipState, physical: f32) -> f32 {
    let scale = 1.0 - ship.turn_power / ship.delta_time.max(f32::EPSILON);
    if scale.abs() <= f32::EPSILON {
        0.0
    } else {
        physical / scale
    }
}

fn triangle_area(points: [Vec2; 3]) -> f32 {
    cross(points[1] - points[0], points[2] - points[0]).abs() * 0.5
}

fn polygon_area(points: &[Vec2]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(&a, &b)| cross(a, b))
        .sum::<f32>()
        .abs()
        * 0.5
}

fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

fn circle(radius: f32, segments: usize) -> Vec<Vec2> {
    (0..=segments)
        .map(|index| {
            Vec2::from_radians(core::f32::consts::TAU * index as f32 / segments as f32) * radius
        })
        .collect()
}

fn planet_solid_colliders(
    entity: PhysicsId,
    radius: f32,
    groups: CollisionGroups,
) -> Vec<ColliderSpec> {
    let mut surface = ColliderSpec::ball(
        collider_id(entity, BODY_SURFACE_ROLE, 0),
        radius * BODY_BOUNDS_RADIUS_SCALE,
    );
    surface.density = 0.0;
    surface.friction = 0.0;
    surface.restitution = PLANET_ELASTICITY;
    surface.collision_groups = groups;
    surface.solver_groups = groups;
    vec![surface]
}

fn debris_signature(debris: &DebrisState) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut mix = |value: u64| {
        hash ^= value;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    };
    mix(debris.kind as u64);
    mix(debris.radius.to_bits() as u64);
    if let Some(points) = debris.fragment_shape {
        for point in points {
            mix(point.x.to_bits() as u64);
            mix(point.y.to_bits() as u64);
        }
    }
    hash
}

pub(super) fn primary_body(entity: PhysicsId) -> PhysicsBodyId {
    PhysicsBodyId::new(entity, BodyRole::PRIMARY)
}

fn collider_id(entity: PhysicsId, role: ColliderRole, part: u16) -> ColliderId {
    ColliderId::new(entity, role, part)
}

pub(super) fn spaceport_sensor_id(index: usize) -> ColliderId {
    collider_id(planet_entity(index), SPACEPORT_SENSOR_ROLE, 0)
}

fn world_entity() -> PhysicsId {
    PhysicsId::new(WORLD_ENTITY_VALUE)
}

fn sun_entity() -> PhysicsId {
    PhysicsId::new(SUN_ENTITY_VALUE)
}

pub(super) fn planet_entity(index: usize) -> PhysicsId {
    PhysicsId::new(PLANET_ENTITY_BASE + index as u64)
}

fn ship_entity(index: usize) -> PhysicsId {
    PhysicsId::new(SHIP_ENTITY_BASE + index as u64)
}

fn rover_entity(id: u64) -> PhysicsId {
    debug_assert!(id < DEBRIS_ENTITY_BASE - ROVER_ENTITY_BASE);
    PhysicsId::new(ROVER_ENTITY_BASE + id)
}

pub(super) fn planet_index(entity: PhysicsId) -> Option<usize> {
    let value = entity.value();
    (PLANET_ENTITY_BASE..SHIP_ENTITY_BASE)
        .contains(&value)
        .then(|| (value - PLANET_ENTITY_BASE) as usize)
}

fn ship_index(entity: PhysicsId) -> Option<usize> {
    let value = entity.value();
    (SHIP_ENTITY_BASE..SHIP_ENTITY_BASE + 2)
        .contains(&value)
        .then(|| (value - SHIP_ENTITY_BASE) as usize)
}

fn ordered_entity_pair(
    a: MechanicalEntity,
    b: MechanicalEntity,
) -> (MechanicalEntity, MechanicalEntity) {
    if a <= b { (a, b) } else { (b, a) }
}

fn classify_entity(entity: PhysicsId) -> Option<MechanicalEntity> {
    match entity.value() {
        WORLD_ENTITY_VALUE => Some(MechanicalEntity::World),
        SUN_ENTITY_VALUE => Some(MechanicalEntity::Body(BodyId::Sun)),
        value if (PLANET_ENTITY_BASE..SHIP_ENTITY_BASE).contains(&value) => Some(
            MechanicalEntity::Body(BodyId::Planet((value - PLANET_ENTITY_BASE) as usize)),
        ),
        value if (SHIP_ENTITY_BASE..SHIP_ENTITY_BASE + 2).contains(&value) => {
            Some(MechanicalEntity::Ship((value - SHIP_ENTITY_BASE) as usize))
        }
        value if (ROVER_ENTITY_BASE..DEBRIS_ENTITY_BASE).contains(&value) => {
            Some(MechanicalEntity::Rover(value - ROVER_ENTITY_BASE))
        }
        value if value >= super::terrain::FRAGMENT_ID_BASE => {
            Some(MechanicalEntity::TerrainFragment(value))
        }
        value if value >= DEBRIS_ENTITY_BASE => Some(MechanicalEntity::Debris(value)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planet_ship_surface_is_one_continuous_circle() {
        let radius = 100.0;
        let colliders = planet_solid_colliders(
            planet_entity(0),
            radius,
            CollisionGroups::new(GROUP_BODY, GROUP_ALL_SHIPS),
        );

        assert_eq!(colliders.len(), 1);
        assert_eq!(
            colliders[0].shape,
            engine_rapier::world::ColliderShape::Ball {
                radius: radius * BODY_BOUNDS_RADIUS_SCALE
            }
        );
        assert_eq!(colliders[0].collision_groups.memberships, GROUP_BODY);
    }

    #[test]
    fn only_gameplay_sensitive_debris_uses_ccd() {
        let shell = DebrisState::new_shell(0, 1, Vec2::ZERO, Vec2::X * 300.0, 0.0);
        let small_asteroid = DebrisState::new(
            DebrisKind::Asteroid,
            Vec2::ZERO,
            Vec2::X * 200.0,
            CANNON_SHELL_RADIUS,
            1.0,
            engine_core::Color::WHITE,
        );
        let large_asteroid = DebrisState::new(
            DebrisKind::Asteroid,
            Vec2::ZERO,
            Vec2::X * 200.0,
            CANNON_SHELL_RADIUS + 0.1,
            1.0,
            engine_core::Color::WHITE,
        );
        let fragment = DebrisState::new_fragment(
            Vec2::ZERO,
            [Vec2::ZERO, Vec2::X, Vec2::Y],
            Vec2::X * 200.0,
            1.0,
            engine_core::Color::WHITE,
        );

        assert!(debris_uses_ccd(&shell));
        assert!(debris_uses_ccd(&small_asteroid));
        assert!(!debris_uses_ccd(&large_asteroid));
        assert!(!debris_uses_ccd(&fragment));
    }

    #[test]
    fn ship_geometry_uses_the_render_pivot_as_body_origin() {
        let ship =
            ShipState::new_with_default_life(0, Vec2::ZERO, engine_core::Color::WHITE, 1.0 / 60.0);
        let triangles = ship_local_triangles(&ship);
        let rendered_center =
            super::super::ship_low_bounds(&super::super::ship_triangles(&ship)).center;
        let local_center = super::super::ship_low_bounds(&triangles).center;

        assert!(rendered_center.distance_to(local_center + SHIP_PIVOT) < 1.0e-5);
    }

    #[test]
    fn ship_collision_hulls_enclose_rendered_parts_and_preserve_mass() {
        let mut ship =
            ShipState::new_with_default_life(0, Vec2::ZERO, engine_core::Color::WHITE, 1.0 / 60.0);
        let mut closed_ship = ship.clone();
        closed_ship.wing_theta = super::super::MAX_WING_THETA;
        let mut pod = ship.clone();
        pod.form = ShipForm::EscapePod;

        for ship in [&ship, &closed_ship, &pod] {
            let triangles = ship_local_triangles(ship);
            let hull = ship_collision_hull(ship);
            assert!(hull.len() >= 3);
            assert_eq!(hull, ship_collision_hull(ship));

            for (&start, &end) in hull.iter().zip(hull.iter().cycle().skip(1)) {
                for point in triangles.iter().flatten() {
                    assert!(
                        cross(end - start, *point - start) >= -1.0e-5,
                        "{point:?} should be inside hull {hull:?}"
                    );
                }
            }

            let mut colliders = ship_colliders(ship_entity(0), ship, false, false);
            assert_eq!(colliders.len(), 1);
            let collider = colliders.pop().unwrap();
            let engine_rapier::world::ColliderShape::ConvexPolygon { vertices } = collider.shape
            else {
                panic!("ship should use one convex polygon");
            };
            assert_eq!(vertices, hull);
            assert_eq!(collider.id.part, 0);
            assert!((collider.density * polygon_area(&vertices) - ship.mass()).abs() < 1.0e-4);
        }

        ship.wing_theta = super::super::MAX_WING_THETA * 0.5;
        assert_ne!(
            ship_collision_hull(&ship),
            ship_collision_hull(&closed_ship)
        );
    }

    #[test]
    fn docked_ship_uses_a_compact_rotation_invariant_collider() {
        let mut ship =
            ShipState::new_with_default_life(0, Vec2::ZERO, engine_core::Color::WHITE, 1.0 / 60.0);
        let open_hull = ship_collision_hull(&ship);
        let open = ship_colliders(ship_entity(0), &ship, true, true);
        ship.wing_theta = super::super::MAX_WING_THETA;
        let closed = ship_colliders(ship_entity(0), &ship, true, true);

        assert_eq!(open.len(), 2);
        assert_eq!(closed.len(), 2);
        let engine_rapier::world::ColliderShape::Ball { radius } = open[0].shape else {
            panic!("docked ship should use one circular collider");
        };
        assert_eq!(radius, DOCKED_SHIP_COLLIDER_RADIUS);
        assert_eq!(
            closed[0].shape,
            engine_rapier::world::ColliderShape::Ball { radius }
        );
        assert!(
            (open[0].density * core::f32::consts::PI * radius.powi(2) - ship.mass()).abs() < 1.0e-4
        );
        assert!(open[1].sensor);
        assert_eq!(open[1].density, 0.0);
        assert_eq!(open[1].solver_groups, CollisionGroups::NONE);
        assert_eq!(
            open[1].shape,
            engine_rapier::world::ColliderShape::ConvexPolygon {
                vertices: open_hull
            }
        );

        assert!(matches!(
            ship_colliders(ship_entity(0), &ship, false, false)[0].shape,
            engine_rapier::world::ColliderShape::ConvexPolygon { .. }
        ));
        assert!(matches!(
            ship_colliders(ship_entity(0), &ship, true, false)[0].shape,
            engine_rapier::world::ColliderShape::ConvexPolygon { .. }
        ));
    }

    #[test]
    fn braking_docked_full_ship_latches_the_compact_kinematic_hold() {
        let mut ships = [
            ShipState::new_with_default_life(
                0,
                Vec2::new(100.0, 100.0),
                engine_core::Color::WHITE,
                1.0 / 60.0,
            ),
            ShipState::new_with_default_life(
                1,
                Vec2::new(300.0, 300.0),
                engine_core::Color::WHITE,
                1.0 / 60.0,
            ),
        ];
        let config = super::super::SpacewarsConfig::default();
        let (sun, planets) = super::super::build_world(&config, 0);
        let mut physics =
            SpacewarsPhysics::new(config.universe_radius as f32, &ships, sun, &planets);
        let mut debris = Vec::new();
        ships[0].brake = 1.0;

        let lifecycle = physics.reconcile(PhysicsReconcileInput {
            tick: 1,
            dt_seconds: 1.0 / 60.0,
            ships: &mut ships,
            debris: &mut debris,
            sun,
            planets: &planets,
            rovers: &[],
            docked_planets: &[Some(0), None],
        });

        assert_eq!(lifecycle.removed, 1);
        assert_eq!(lifecycle.added, 1);
        assert_eq!(
            physics.ship_keys[0],
            Some(ShipColliderKey {
                form: ShipForm::Ship,
                wing_theta: 0.0_f32.to_bits(),
                docked: true,
                compact: true,
                constrained: true,
            })
        );
        assert!(!physics.ship_keys[1].unwrap().compact);

        let docked_groups = ship_collision_groups(&ships[0], true);
        assert_eq!(docked_groups.filter & GROUP_BODY, 0);
        assert_ne!(docked_groups.filter & GROUP_SPACEPORT_SENSOR, 0);

        let lifecycle = physics.reconcile(PhysicsReconcileInput {
            tick: 2,
            dt_seconds: 1.0 / 60.0,
            ships: &mut ships,
            debris: &mut debris,
            sun,
            planets: &planets,
            rovers: &[],
            docked_planets: &[None, None],
        });
        assert_eq!(lifecycle.removed, 0);
        assert_eq!(lifecycle.added, 0);
        assert!(physics.ship_keys[0].unwrap().docked);

        ships[0].brake = 0.0;
        ships[0].turn = 1.0;
        let lifecycle = physics.reconcile(PhysicsReconcileInput {
            tick: 3,
            dt_seconds: 1.0 / 60.0,
            ships: &mut ships,
            debris: &mut debris,
            sun,
            planets: &planets,
            rovers: &[],
            docked_planets: &[None, None],
        });
        assert_eq!(lifecycle.removed, 0);
        assert_eq!(lifecycle.added, 0);
        assert!(physics.ship_keys[0].unwrap().constrained);

        ships[0].thrust = 1.0;
        let lifecycle = physics.reconcile(PhysicsReconcileInput {
            tick: 4,
            dt_seconds: 1.0 / 60.0,
            ships: &mut ships,
            debris: &mut debris,
            sun,
            planets: &planets,
            rovers: &[],
            docked_planets: &[None, None],
        });
        assert_eq!(lifecycle.removed, 1);
        assert_eq!(lifecycle.added, 1);
        assert_eq!(
            physics.ship_keys[0],
            Some(ShipColliderKey {
                form: ShipForm::Ship,
                wing_theta: 0.0_f32.to_bits(),
                docked: false,
                compact: false,
                constrained: false,
            })
        );
    }

    #[test]
    fn unbraked_full_ship_contact_does_not_become_a_persistent_hold() {
        let mut ships = [
            ShipState::new_with_default_life(
                0,
                Vec2::new(100.0, 100.0),
                engine_core::Color::WHITE,
                1.0 / 60.0,
            ),
            ShipState::new_with_default_life(
                1,
                Vec2::new(300.0, 300.0),
                engine_core::Color::WHITE,
                1.0 / 60.0,
            ),
        ];
        let config = super::super::SpacewarsConfig::default();
        let (sun, planets) = super::super::build_world(&config, 0);
        let mut physics =
            SpacewarsPhysics::new(config.universe_radius as f32, &ships, sun, &planets);
        let mut debris = Vec::new();

        physics.reconcile(PhysicsReconcileInput {
            tick: 1,
            dt_seconds: 1.0 / 60.0,
            ships: &mut ships,
            debris: &mut debris,
            sun,
            planets: &planets,
            rovers: &[],
            docked_planets: &[Some(0), None],
        });
        assert!(physics.ship_keys[0].unwrap().docked);
        assert!(!physics.ship_keys[0].unwrap().constrained);
        assert_eq!(physics.docked_planets[0], None);

        physics.reconcile(PhysicsReconcileInput {
            tick: 2,
            dt_seconds: 1.0 / 60.0,
            ships: &mut ships,
            debris: &mut debris,
            sun,
            planets: &planets,
            rovers: &[],
            docked_planets: &[None, None],
        });
        assert!(!physics.ship_keys[0].unwrap().docked);
        assert!(!physics.ship_keys[0].unwrap().constrained);
    }

    #[test]
    fn fast_glancing_asteroid_hit_uses_one_ship_hull() {
        let mut ships = [
            ShipState::new_with_default_life(
                0,
                Vec2::new(1_000.0, 1_000.0),
                engine_core::Color::WHITE,
                1.0 / 60.0,
            ),
            ShipState::new_with_default_life(
                1,
                Vec2::new(3_000.0, 3_000.0),
                engine_core::Color::WHITE,
                1.0 / 60.0,
            ),
        ];
        ships[0].wing_theta = super::super::MAX_WING_THETA;
        ships[0].wing_state = super::super::WingState::Closed;
        ships[0].velocity = Vec2::Y * super::super::WING_CLOSED_SPEED;

        let mut physics = SpacewarsPhysics::new(2_000.0, &ships, None, &[]);
        assert_eq!(
            physics.world.collider_count(),
            3,
            "the world boundary and two ships should each use one collider"
        );

        let mut debris = vec![DebrisState::new(
            DebrisKind::Asteroid,
            ships[0].position + Vec2::new(5.0, 15.7),
            Vec2::ZERO,
            3.0,
            1.0,
            engine_core::Color::WHITE,
        )];
        let lifecycle = physics.reconcile(PhysicsReconcileInput {
            tick: 1,
            dt_seconds: 1.0 / 60.0,
            ships: &mut ships,
            debris: &mut debris,
            sun: None,
            planets: &[],
            rovers: &[],
            docked_planets: &[None; 2],
        });
        assert_eq!(lifecycle.added, 1);
        assert_eq!(physics.world.collider_count(), 4);

        let _ = physics.step(1.0 / 60.0);
        let _ = physics.step(1.0 / 60.0);

        let contacts = physics.contacts();
        assert!(
            contacts.iter().any(|contact| {
                [contact.a, contact.b].contains(&MechanicalEntity::Ship(0))
                    && [contact.a, contact.b]
                        .contains(&MechanicalEntity::Debris(debris[0].physics_id))
            }),
            "the resolving CCD substep should remain visible to gameplay"
        );

        physics.synchronize_motion(&mut ships, &mut debris);
        assert!(
            debris[0].velocity.length_squared() > 0.0,
            "the glancing setup should actually hit the asteroid"
        );
        assert!(
            ships[0].rotation_radians.abs() < core::f32::consts::FRAC_PI_2,
            "the resolving frame rotated the ship by {} radians",
            ships[0].rotation_radians
        );
    }

    #[test]
    fn cloned_world_keeps_spacewars_handle_mappings() {
        let ships = [
            ShipState::new_with_default_life(
                0,
                Vec2::new(10.0, 10.0),
                engine_core::Color::WHITE,
                1.0 / 60.0,
            ),
            ShipState::new_with_default_life(
                1,
                Vec2::new(20.0, 20.0),
                engine_core::Color::WHITE,
                1.0 / 60.0,
            ),
        ];
        let physics = SpacewarsPhysics::new(100.0, &ships, None, &[]);
        let cloned = physics.clone();

        assert!(cloned.world.motion(primary_body(ship_entity(0))).is_some());
        assert!(cloned.world.motion(primary_body(ship_entity(1))).is_some());
    }
}
