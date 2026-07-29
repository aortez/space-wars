//! Spacewars assemblies and gameplay-facing queries for the canonical physics world.

use std::collections::{BTreeMap, BTreeSet};

use engine_core::Vec2;
use engine_rapier::world::{
    BodyId as PhysicsBodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
    CollisionGroups, PhysicsId, PhysicsStepMetrics, PhysicsWorld, PhysicsWorldConfig,
    RayCastOptions,
};

use super::{
    BODY_BOUNDS_RADIUS_SCALE, BodyId, CANNON_SHELL_RADIUS, DEFAULT_ELASTICITY, DebrisKind,
    DebrisState, PLANET_ELASTICITY, POD_BODY, POD_LASER, POD_PIVOT, POD_THRUSTER, PlanetState,
    SHELL_BODY, SHIP_BODY, SHIP_LASER, SHIP_LEFT_WING, SHIP_PIVOT, SHIP_RIGHT_WING, SHIP_THRUSTER,
    SHIP_WING_MOUNT, SHIP_WING_PIVOT, SPACEPORT_ARC_LENGTH, SPACEPORT_DEPTH_FACTOR,
    SPACEPORT_OUTER_POINTS, ShipForm, ShipState, SunState, rotate_points, spaceport_local_points,
};

const WORLD_ENTITY_VALUE: u64 = 1;
const SUN_ENTITY_VALUE: u64 = 2;
const PLANET_ENTITY_BASE: u64 = 100;
const SHIP_ENTITY_BASE: u64 = 10_000;
const DEBRIS_ENTITY_BASE: u64 = 100_000;

const WORLD_ROLE: ColliderRole = ColliderRole::new(1);
const BODY_SURFACE_ROLE: ColliderRole = ColliderRole::new(2);
const SPACEPORT_SENSOR_ROLE: ColliderRole = ColliderRole::new(3);
const SPACEPORT_GATE_ROLE: ColliderRole = ColliderRole::new(4);
const SHIP_HULL_ROLE: ColliderRole = ColliderRole::new(5);
const DEBRIS_ROLE: ColliderRole = ColliderRole::new(6);

const GROUP_SHIP_0: u32 = 1 << 0;
const GROUP_SHIP_1: u32 = 1 << 1;
const GROUP_POD_0: u32 = 1 << 2;
const GROUP_POD_1: u32 = 1 << 3;
const GROUP_DEBRIS: u32 = 1 << 4;
const GROUP_BODY: u32 = 1 << 5;
const GROUP_WORLD: u32 = 1 << 6;
const GROUP_SPACEPORT_GATE: u32 = 1 << 7;
const GROUP_SPACEPORT_SENSOR: u32 = 1 << 8;
const GROUP_ALL_SHIPS: u32 = GROUP_SHIP_0 | GROUP_SHIP_1 | GROUP_POD_0 | GROUP_POD_1;
const GROUP_ALL_SOLIDS: u32 =
    GROUP_ALL_SHIPS | GROUP_DEBRIS | GROUP_BODY | GROUP_WORLD | GROUP_SPACEPORT_GATE;

const PLANET_SURFACE_SEGMENTS: usize = 24;
const WORLD_SURFACE_SEGMENTS: usize = 192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum MechanicalEntity {
    World,
    Body(BodyId),
    Ship(usize),
    Debris(u64),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MechanicalContact {
    pub a: MechanicalEntity,
    pub b: MechanicalEntity,
    pub point: Option<Vec2>,
    pub normal: Vec2,
    pub impulse_magnitude: f32,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlanetColliderKey {
    radius: u32,
    owner_id: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DebrisColliderKey {
    signature: u64,
    armed: bool,
}

#[derive(Debug, Clone)]
pub(super) struct SpacewarsPhysics {
    world: PhysicsWorld,
    sun_radius: Option<u32>,
    ship_keys: [Option<ShipColliderKey>; 2],
    planet_keys: Vec<Option<PlanetColliderKey>>,
    debris_keys: BTreeMap<u64, DebrisColliderKey>,
    next_debris_entity: u64,
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
            sun_radius: None,
            ship_keys: [None, None],
            planet_keys: vec![None; planets.len()],
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
            let _ = physics.insert_ship(index, ship, false);
        }
        physics
    }

    pub fn reconcile(
        &mut self,
        tick: u64,
        ships: &mut [ShipState; 2],
        debris: &mut [DebrisState],
        sun: Option<SunState>,
        planets: &[PlanetState],
        docked_ships: &[bool; 2],
    ) -> PhysicsLifecycle {
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
                owner_id: planet.owner_id,
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

        for (index, ship) in ships.iter().enumerate() {
            let key = ShipColliderKey {
                form: ship.form,
                wing_theta: ship.wing_theta.to_bits(),
                docked: docked_ships[index] || ship.spaceport_ejection.is_some(),
            };
            if self.ship_keys[index] != Some(key) {
                lifecycle.removed += usize::from(self.world.remove_entity(ship_entity(index)));
                lifecycle.added += usize::from(self.insert_ship(index, ship, key.docked));
            } else {
                synchronize_ship_to_physics(&mut self.world, index, ship);
            }
        }

        self.reconcile_debris(tick, debris, &mut lifecycle);
        lifecycle
    }

    pub fn step(&mut self, dt_seconds: f32) -> PhysicsStepMetrics {
        self.world.step(dt_seconds)
    }

    pub fn apply_velocity_delta(&mut self, entity: MechanicalEntity, delta_velocity: Vec2) -> bool {
        let entity = match entity {
            MechanicalEntity::Ship(index) => ship_entity(index),
            MechanicalEntity::Debris(id) => PhysicsId::new(id),
            MechanicalEntity::World | MechanicalEntity::Body(_) => return false,
        };
        self.world
            .apply_velocity_delta(primary_body(entity), delta_velocity, true)
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

    pub fn contacts(&self) -> Vec<MechanicalContact> {
        self.world
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
                })
            })
            .collect()
    }

    pub fn spaceport_contacts(&self) -> Vec<(usize, usize)> {
        let mut contacts = BTreeSet::new();
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
            contacts.insert((ship, planet));
        }
        contacts.into_iter().collect()
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
                collision_groups: CollisionGroups::ALL,
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
                armed: item.spawn_tick < tick,
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
        collider.collision_groups =
            CollisionGroups::new(GROUP_WORLD, GROUP_ALL_SHIPS | GROUP_DEBRIS);
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
            CollisionGroups::new(GROUP_BODY, GROUP_ALL_SHIPS | GROUP_DEBRIS);
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

    fn insert_planet(&mut self, index: usize, planet: &PlanetState) -> bool {
        let entity = planet_entity(index);
        let body_groups = CollisionGroups::new(GROUP_BODY, GROUP_ALL_SHIPS | GROUP_DEBRIS);
        let mut colliders = planet_solid_colliders(entity, planet.radius, body_groups);

        let mut sensor = ColliderSpec::convex_polygon(
            collider_id(entity, SPACEPORT_SENSOR_ROLE, 0),
            spaceport_local_points(planet.radius),
        );
        sensor.density = 0.0;
        sensor.sensor = true;
        sensor.collision_groups = CollisionGroups::new(GROUP_SPACEPORT_SENSOR, GROUP_ALL_SHIPS);
        sensor.solver_groups = CollisionGroups::NONE;

        let gate_points = planet_gate(planet.radius);
        let mut gate =
            ColliderSpec::polyline(collider_id(entity, SPACEPORT_GATE_ROLE, 0), gate_points);
        gate.density = 0.0;
        gate.friction = 0.0;
        gate.restitution = PLANET_ELASTICITY;
        gate.collision_groups =
            CollisionGroups::new(GROUP_SPACEPORT_GATE, blocked_pod_groups(planet.owner_id));
        gate.solver_groups = gate.collision_groups;
        colliders.push(sensor);
        colliders.push(gate);

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
            owner_id: planet.owner_id,
        });
        inserted
    }

    fn insert_ship(&mut self, index: usize, ship: &ShipState, docked: bool) -> bool {
        let entity = ship_entity(index);
        let collider = ship_collider(entity, ship, docked);
        let inserted = self.world.insert_body(
            primary_body(entity),
            BodySpec {
                kind: BodyKind::Dynamic,
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
            &[collider],
        );
        debug_assert!(inserted);
        self.ship_keys[index] = Some(ShipColliderKey {
            form: ship.form,
            wing_theta: ship.wing_theta.to_bits(),
            docked,
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
                ccd_enabled: debris.kind == DebrisKind::Shell
                    || debris.radius <= CANNON_SHELL_RADIUS,
                ..BodySpec::default()
            },
            &[collider],
        );
        debug_assert!(inserted);
        inserted
    }
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

fn ship_collision_groups(ship: &ShipState, docked: bool) -> CollisionGroups {
    let membership = match ship.form {
        ShipForm::Ship if ship.owner_id == 0 => GROUP_SHIP_0,
        ShipForm::Ship => GROUP_SHIP_1,
        ShipForm::EscapePod if ship.owner_id == 0 => GROUP_POD_0,
        ShipForm::EscapePod => GROUP_POD_1,
    };
    let mut filter = GROUP_BODY | GROUP_WORLD | GROUP_DEBRIS | GROUP_SPACEPORT_SENSOR;
    if !docked {
        filter |= GROUP_ALL_SHIPS;
    }
    if ship.form == ShipForm::EscapePod {
        filter |= GROUP_SPACEPORT_GATE;
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

fn blocked_pod_groups(owner_id: Option<usize>) -> u32 {
    match owner_id {
        Some(0) => GROUP_POD_1,
        Some(1) => GROUP_POD_0,
        _ => GROUP_POD_0 | GROUP_POD_1,
    }
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

fn ship_collider(entity: PhysicsId, ship: &ShipState, docked: bool) -> ColliderSpec {
    let hull = ship_collision_hull(ship);
    let density = ship.mass() / polygon_area(&hull).max(f32::EPSILON);
    let groups = ship_collision_groups(ship, docked);
    let mut collider = ColliderSpec::convex_polygon(collider_id(entity, SHIP_HULL_ROLE, 0), hull);
    collider.density = density;
    collider.friction = 0.0;
    collider.restitution = DEFAULT_ELASTICITY;
    collider.collision_groups = groups;
    collider.solver_groups = groups;
    collider
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

fn ship_pivot(form: ShipForm) -> Vec2 {
    match form {
        ShipForm::Ship => SHIP_PIVOT,
        ShipForm::EscapePod => POD_PIVOT,
    }
}

fn physical_angular_velocity(ship: &ShipState) -> f32 {
    let scale = 1.0 - ship.turn_power / ship.delta_time.max(f32::EPSILON);
    ship.omega * scale
}

fn control_angular_velocity(ship: &ShipState, physical: f32) -> f32 {
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
    let outer_radius = radius * BODY_BOUNDS_RADIUS_SCALE;
    let inner_radius = radius * SPACEPORT_DEPTH_FACTOR * BODY_BOUNDS_RADIUS_SCALE;
    let mut center = ColliderSpec::ball(collider_id(entity, BODY_SURFACE_ROLE, 0), inner_radius);
    center.density = 0.0;
    center.friction = 0.0;
    center.restitution = PLANET_ELASTICITY;
    center.collision_groups = groups;
    center.solver_groups = groups;

    let gap_end = port_gap_end(radius);
    let mut colliders = Vec::with_capacity(PLANET_SURFACE_SEGMENTS + 1);
    colliders.push(center);
    for index in 0..PLANET_SURFACE_SEGMENTS {
        let theta_a = gap_end
            + (core::f32::consts::TAU - gap_end) * index as f32 / PLANET_SURFACE_SEGMENTS as f32;
        let theta_b = gap_end
            + (core::f32::consts::TAU - gap_end) * (index + 1) as f32
                / PLANET_SURFACE_SEGMENTS as f32;
        let mut sector = ColliderSpec::convex_polygon(
            collider_id(entity, BODY_SURFACE_ROLE, index as u16 + 1),
            vec![
                Vec2::from_radians(theta_a) * inner_radius,
                Vec2::from_radians(theta_a) * outer_radius,
                Vec2::from_radians(theta_b) * outer_radius,
                Vec2::from_radians(theta_b) * inner_radius,
            ],
        );
        sector.density = 0.0;
        sector.friction = 0.0;
        sector.restitution = PLANET_ELASTICITY;
        sector.collision_groups = groups;
        sector.solver_groups = groups;
        colliders.push(sector);
    }
    colliders
}

#[cfg(test)]
fn planet_ring_sectors(radius: f32) -> Vec<[Vec2; 4]> {
    let outer_radius = radius * BODY_BOUNDS_RADIUS_SCALE;
    let inner_radius = radius * SPACEPORT_DEPTH_FACTOR * BODY_BOUNDS_RADIUS_SCALE;
    let gap_end = port_gap_end(radius);
    (0..PLANET_SURFACE_SEGMENTS)
        .map(|index| {
            let theta_a = gap_end
                + (core::f32::consts::TAU - gap_end) * index as f32
                    / PLANET_SURFACE_SEGMENTS as f32;
            let theta_b = gap_end
                + (core::f32::consts::TAU - gap_end) * (index + 1) as f32
                    / PLANET_SURFACE_SEGMENTS as f32;
            [
                Vec2::from_radians(theta_a) * inner_radius,
                Vec2::from_radians(theta_a) * outer_radius,
                Vec2::from_radians(theta_b) * outer_radius,
                Vec2::from_radians(theta_b) * inner_radius,
            ]
        })
        .collect()
}

fn planet_gate(radius: f32) -> Vec<Vec2> {
    let collision_radius = radius * BODY_BOUNDS_RADIUS_SCALE;
    [
        Vec2::from_radians(0.0) * collision_radius,
        Vec2::from_radians(port_gap_end(radius)) * collision_radius,
    ]
    .to_vec()
}

fn port_gap_end(radius: f32) -> f32 {
    let angle = SPACEPORT_ARC_LENGTH / radius;
    (SPACEPORT_OUTER_POINTS - 1) as f32 * angle / SPACEPORT_OUTER_POINTS as f32
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

fn primary_body(entity: PhysicsId) -> PhysicsBodyId {
    PhysicsBodyId::new(entity, BodyRole::PRIMARY)
}

fn collider_id(entity: PhysicsId, role: ColliderRole, part: u16) -> ColliderId {
    ColliderId::new(entity, role, part)
}

fn world_entity() -> PhysicsId {
    PhysicsId::new(WORLD_ENTITY_VALUE)
}

fn sun_entity() -> PhysicsId {
    PhysicsId::new(SUN_ENTITY_VALUE)
}

fn planet_entity(index: usize) -> PhysicsId {
    PhysicsId::new(PLANET_ENTITY_BASE + index as u64)
}

fn ship_entity(index: usize) -> PhysicsId {
    PhysicsId::new(SHIP_ENTITY_BASE + index as u64)
}

fn planet_index(entity: PhysicsId) -> Option<usize> {
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
        value if value >= DEBRIS_ENTITY_BASE => Some(MechanicalEntity::Debris(value)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planet_solid_ring_has_a_real_spaceport_cavity() {
        let radius = 100.0;
        let sectors = planet_ring_sectors(radius);
        let first_outer = sectors[0][1];
        let last_outer = sectors.last().expect("last sector")[2];
        let gap_chord = first_outer.distance_to(last_outer);

        assert!(gap_chord > radius * 0.25);
        assert!((first_outer.length() - radius * BODY_BOUNDS_RADIUS_SCALE).abs() < 0.001);
        assert!(
            (sectors[0][0].length() - radius * SPACEPORT_DEPTH_FACTOR * BODY_BOUNDS_RADIUS_SCALE)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn port_gate_only_blocks_pods_without_access() {
        assert_eq!(blocked_pod_groups(None), GROUP_POD_0 | GROUP_POD_1);
        assert_eq!(blocked_pod_groups(Some(0)), GROUP_POD_1);
        assert_eq!(blocked_pod_groups(Some(1)), GROUP_POD_0);
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

            let collider = ship_collider(ship_entity(0), ship, false);
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
        let lifecycle = physics.reconcile(1, &mut ships, &mut debris, None, &[], &[false; 2]);
        assert_eq!(lifecycle.added, 1);
        assert_eq!(physics.world.collider_count(), 4);

        let _ = physics.step(1.0 / 60.0);
        let _ = physics.step(1.0 / 60.0);

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
