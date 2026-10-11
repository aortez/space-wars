use engine_core::Vec2;
use engine_rapier::{
    terrain::{GrainSeed, TerrainGrain, TerrainSpec},
    world::{
        BodyId, BodyKind, BodyMotion, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
        PhysicsId, PhysicsWorld, PhysicsWorldConfig,
    },
};
use engine_soil::{Environment, Seed};
use engine_terrain::Cell;

use super::{DT, Fixture, Ground, Point, RADIUS, SPACING};

const BASE: PhysicsId = PhysicsId::new(1);
const SUPPORT: PhysicsId = PhysicsId::new(2);
fn body(id: PhysicsId) -> BodyId {
    BodyId::new(id, BodyRole::PRIMARY)
}
fn collider(id: PhysicsId) -> ColliderId {
    ColliderId::new(id, ColliderRole::PRIMARY, 0)
}

#[derive(Clone)]
pub(super) struct Grains {
    world: PhysicsWorld,
    grains: Vec<TerrainGrain>,
}

impl Grains {
    pub fn new(ground: Ground) -> Self {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::ZERO,
            solver_iterations: 8,
            internal_stabilization_iterations: 2,
            max_ccd_substeps: 4,
            ..PhysicsWorldConfig::default()
        });
        let mut shape = match ground.fixture {
            Fixture::Flat => {
                let mut shape = ColliderSpec::cuboid(collider(BASE), 40.0, 4.0);
                shape.local_position = Vec2::new(0.0, -4.0);
                shape
            }
            Fixture::MovingPlanet => ColliderSpec::ball(collider(BASE), RADIUS),
        };
        shape.friction = 0.6;
        shape.restitution = 0.0;
        let (position, angle) = ground.fixture.pose(0.0);
        assert!(world.insert_body(
            body(BASE),
            BodySpec {
                kind: BodyKind::KinematicPosition,
                position,
                angle,
                ..BodySpec::default()
            },
            &[shape]
        ));
        let mut grains = Self {
            world,
            grains: Vec::new(),
        };
        grains.replace_support(ground, 0.0);
        grains
    }

    pub fn replace_support(&mut self, ground: Ground, seconds: f64) {
        self.world.remove_entity(SUPPORT);
        let polygon = ground.platform();
        if polygon.is_empty() {
            return;
        }
        let mut shape = ColliderSpec::convex_polygon(collider(SUPPORT), polygon);
        shape.friction = 0.6;
        shape.restitution = 0.0;
        let (position, angle) = ground.fixture.pose(seconds);
        assert!(self.world.insert_body(
            body(SUPPORT),
            BodySpec {
                kind: BodyKind::KinematicPosition,
                position,
                angle,
                ..BodySpec::default()
            },
            &[shape]
        ));
    }

    pub fn insert(&mut self, seeds: &[Seed]) {
        for seed in seeds {
            let grain = TerrainGrain::insert(
                &mut self.world,
                PhysicsId::new(self.grains.len() as u64 + 10),
                GrainSeed {
                    cell: Cell {
                        material: seed.material,
                        durability: 100,
                    },
                    cell_size: SPACING,
                    parent_offset: Vec2::ZERO,
                },
                BodyMotion {
                    position: seed.position,
                    angle: 0.0,
                    linear_velocity: seed.velocity,
                    angular_velocity: 0.0,
                },
                seed.position,
                TerrainSpec {
                    friction: 0.6,
                    restitution: 0.0,
                    ..TerrainSpec::default()
                },
            )
            .expect("valid grain seed");
            self.grains.push(grain);
        }
    }

    pub fn step(&mut self, ground: Ground, seconds: f64) {
        let (position, angle) = ground.fixture.pose(seconds + DT);
        self.world
            .set_next_kinematic_pose(body(BASE), position, angle);
        self.world
            .set_next_kinematic_pose(body(SUPPORT), position, angle);
        self.world.clear_forces();
        for grain in &self.grains {
            let motion = self.world.motion(grain.body()).unwrap();
            self.world.apply_acceleration(
                grain.body(),
                ground.gravity(motion.position, seconds),
                false,
            );
        }
        self.world.step(DT as f32);
    }

    pub fn blast(&mut self, center: Vec2, radius: f32, speed: f32) -> usize {
        let mut hit = 0;
        for grain in &self.grains {
            let motion = self.world.motion(grain.body()).unwrap();
            let offset = motion.position - center;
            let distance = offset.length();
            if distance > 0.0 && distance < radius {
                self.world.apply_velocity_delta(
                    grain.body(),
                    offset * (speed * (1.0 - distance / radius) / distance),
                    true,
                );
                hit += 1;
            }
        }
        hit
    }

    pub fn points(&self) -> Vec<Point> {
        self.grains
            .iter()
            .map(|grain| {
                let motion = self.world.motion(grain.body()).unwrap();
                Point {
                    position: motion.position,
                    velocity: motion.linear_velocity,
                    mass: SPACING.powi(2),
                    material: grain.cell().material,
                }
            })
            .collect()
    }

    pub fn state_hash(&self) -> u64 {
        self.world
            .snapshot_bytes()
            .expect("serializable world")
            .into_iter()
            .fold(0xcbf29ce484222325_u64, |hash, b| {
                (hash ^ b as u64).wrapping_mul(0x100000001b3)
            })
    }
}
