use super::*;
use engine_rapier::world::BodyMotion;
use engine_terrain::DetachedTerrain;

pub(super) const BODY: BodyId = BodyId::new(PhysicsId::new(2), BodyRole::PRIMARY);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropHeight {
    Low,
    High,
}
impl DropHeight {
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
        }
    }
    fn height(self) -> f32 {
        match self {
            Self::Low => 0.3,
            Self::High => 12.0,
        }
    }
}

pub(super) fn floor() -> Terrain {
    Terrain::generate(
        97,
        65,
        1.0,
        vec![Material {
            id: DIRT,
            hardness: 100,
        }],
        |c| if c.y <= 22 { DIRT } else { MaterialId::VOID },
    )
    .unwrap()
}

pub(super) fn clump(world: &mut PhysicsWorld, height: DropHeight) -> TerrainFragment {
    let terrain = Terrain::generate(
        12,
        8,
        1.0,
        vec![Material {
            id: DIRT,
            hardness: 100,
        }],
        |c| {
            if (2..10).contains(&c.x) && (2..6).contains(&c.y) {
                DIRT
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let position = Vec2::new(0.0, -7.5 + height.height());
    let clump = TerrainFragment::insert(
        world,
        BODY.entity,
        DetachedTerrain {
            terrain,
            parent_offset: Vec2::ZERO,
        },
        BodyMotion {
            position,
            angle: 0.0,
            linear_velocity: Vec2::ZERO,
            angular_velocity: 0.0,
        },
        position,
        TerrainSpec {
            surface: TerrainSurface::Interpolated,
            ..Default::default()
        },
    )
    .unwrap();
    world.set_body_kind(BODY, BodyKind::KinematicPosition, true);
    clump
}
