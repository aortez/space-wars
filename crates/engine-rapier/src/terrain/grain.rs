//! Experimental cell-sized grains. Rigid contact proxies allow rearrangement;
//! material mass and inertia still belong to the original square cell.

use engine_core::Vec2;
pub use engine_terrain::DetachedCell as GrainSeed;
use engine_terrain::{Cell, MaterialId};

use super::TerrainSpec;
use crate::world::{
    BodyId, BodyMotion, BodyRole, BodySpec, ColliderId, ColliderMassProperties, ColliderRole,
    ColliderSpec, PhysicsId, PhysicsWorld,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GrainShape {
    #[default]
    Round,
    /// Regular hexagon, with the same bounding radius as the round proxy.
    /// Flat faces resist rolling through ordinary rigid contact, not damping.
    Hexagon,
}

impl GrainShape {
    /// Authoritative local contact vertices, also used for rendering.
    pub fn vertices(self, radius: f32) -> Option<Vec<Vec2>> {
        match self {
            Self::Round => None,
            Self::Hexagon => Some(
                (0..6)
                    .map(|i| Vec2::from_radians(i as f32 * std::f32::consts::TAU / 6.0) * radius)
                    .collect(),
            ),
        }
    }
}

/// One conserved cell with a contact proxy inscribed in its old square.
/// The proxy introduces pore space; it is not a volume-preserving soil model.
#[derive(Debug, Clone)]
pub struct TerrainGrain {
    id: PhysicsId,
    cell: Cell,
    cell_size: f32,
    shape: GrainShape,
}

impl TerrainGrain {
    /// The caller transfers ownership of the seed's cell. No source is edited
    /// here. Contact settings come from `spec`; terrain surface/chunk options do
    /// not affect the grain. A failed insertion leaves the world unchanged.
    pub fn insert(
        world: &mut PhysicsWorld,
        id: PhysicsId,
        seed: GrainSeed,
        parent: BodyMotion,
        parent_center: Vec2,
        spec: TerrainSpec,
    ) -> Option<Self> {
        Self::insert_with_shape(
            world,
            id,
            seed,
            parent,
            parent_center,
            spec,
            GrainShape::Round,
        )
    }

    pub fn insert_with_shape(
        world: &mut PhysicsWorld,
        id: PhysicsId,
        seed: GrainSeed,
        parent: BodyMotion,
        parent_center: Vec2,
        spec: TerrainSpec,
        shape: GrainShape,
    ) -> Option<Self> {
        if seed.cell.material == MaterialId::VOID
            || seed.cell.durability == 0
            || !seed.cell_size.is_finite()
            || seed.cell_size <= 0.0
            || world.contains_entity(id)
        {
            return None;
        }
        let grain = Self {
            id,
            cell: seed.cell,
            cell_size: seed.cell_size,
            shape,
        };
        let position = parent.position + seed.parent_offset.rotate_radians(parent.angle);
        let offset = position - parent_center;
        let velocity =
            parent.linear_velocity + Vec2::new(-offset.y, offset.x) * parent.angular_velocity;
        let mass = seed.cell_size.powi(2);
        let mut collider = match shape.vertices(grain.radius()) {
            Some(vertices) => ColliderSpec::convex_polygon(grain.collider(), vertices),
            None => ColliderSpec::ball(grain.collider(), grain.radius()),
        };
        collider.density = 0.0;
        collider.mass_properties = Some(ColliderMassProperties {
            center: Vec2::ZERO,
            mass,
            inertia: mass * mass / 6.0,
        });
        collider.friction = spec.friction;
        collider.restitution = spec.restitution;
        collider.collision_groups = spec.collision_groups;
        collider.solver_groups = spec.collision_groups;
        world
            .insert_body(
                grain.body(),
                BodySpec {
                    position,
                    angle: parent.angle,
                    linear_velocity: velocity,
                    angular_velocity: parent.angular_velocity,
                    ccd_enabled: true,
                    ..BodySpec::default()
                },
                &[collider],
            )
            .then_some(grain)
    }

    pub fn id(&self) -> PhysicsId {
        self.id
    }

    pub fn body(&self) -> BodyId {
        BodyId::new(self.id, BodyRole::PRIMARY)
    }

    pub fn collider(&self) -> ColliderId {
        ColliderId::new(self.id, ColliderRole::PRIMARY, 0)
    }

    pub fn cell(&self) -> Cell {
        self.cell
    }

    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }

    pub fn radius(&self) -> f32 {
        self.cell_size * 0.5
    }

    pub fn shape(&self) -> GrainShape {
        self.shape
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> GrainSeed {
        GrainSeed {
            cell: Cell {
                material: MaterialId(2),
                durability: 73,
            },
            cell_size: 0.5,
            parent_offset: Vec2::new(2.0, -3.0),
        }
    }

    fn parent() -> BodyMotion {
        BodyMotion {
            position: Vec2::new(9.0, 5.0),
            angle: 0.7,
            linear_velocity: Vec2::new(-2.0, 1.0),
            angular_velocity: 0.9,
        }
    }

    #[test]
    fn grain_retains_cell_mass_inertia_and_source_point_motion() {
        for shape in [GrainShape::Round, GrainShape::Hexagon] {
            let mut world = PhysicsWorld::new(Default::default());
            let center = Vec2::new(10.0, 6.0);
            let grain = TerrainGrain::insert_with_shape(
                &mut world,
                PhysicsId::new(2),
                seed(),
                parent(),
                center,
                TerrainSpec::default(),
                shape,
            )
            .unwrap();
            let actual = world.motion(grain.body()).unwrap();
            let expected = parent().position + seed().parent_offset.rotate_radians(parent().angle);
            assert_eq!(grain.cell(), seed().cell);
            assert_eq!(grain.radius(), 0.25);
            assert_eq!(grain.shape(), shape);
            assert!(actual.position.distance_to(expected) < 0.00001);
            let offset = expected - center;
            let velocity = parent().linear_velocity
                + Vec2::new(-offset.y, offset.x) * parent().angular_velocity;
            assert!(actual.linear_velocity.distance_to(velocity) < 0.00001);
            assert_eq!(actual.angular_velocity, parent().angular_velocity);
            assert_eq!(world.body_mass(grain.body()), Some(0.25));
            assert!(
                (world.dynamic_body_inertia(grain.body()).unwrap() - 0.25 * 0.25 / 6.0).abs()
                    < 0.00001
            );
        }
    }

    #[test]
    fn invalid_grain_or_duplicate_identity_does_not_mutate_world() {
        let mut world = PhysicsWorld::new(Default::default());
        for invalid in [
            GrainSeed {
                cell: Cell::VOID,
                ..seed()
            },
            GrainSeed {
                cell_size: f32::NAN,
                ..seed()
            },
            GrainSeed {
                cell_size: f32::MAX,
                ..seed()
            },
        ] {
            assert!(
                TerrainGrain::insert(
                    &mut world,
                    PhysicsId::new(2),
                    invalid,
                    parent(),
                    Vec2::ZERO,
                    TerrainSpec::default()
                )
                .is_none()
            );
            assert_eq!(world.body_count(), 0);
        }
        let grain = TerrainGrain::insert(
            &mut world,
            PhysicsId::new(2),
            seed(),
            parent(),
            Vec2::ZERO,
            TerrainSpec::default(),
        )
        .unwrap();
        let motion = world.motion(grain.body());
        assert!(
            TerrainGrain::insert(
                &mut world,
                PhysicsId::new(2),
                seed(),
                parent(),
                Vec2::ZERO,
                TerrainSpec::default()
            )
            .is_none()
        );
        assert_eq!(world.body_count(), 1);
        assert_eq!(world.motion(grain.body()), motion);
    }
}
