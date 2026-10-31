//! Aquarium-owned native digit cells, matching the fish/render geometry.
//! One fixed body lives in the leased arena and is removed on release.
use super::{DuckEvent, Obstacle};
use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, CollisionGroups,
    PhysicsId, PhysicsWorld,
};

const DIGITS: PhysicsId = PhysicsId::new(crate::arena::ids::canonical::AQUARIUM_DIGITS.start);

pub(super) struct DigitSolids {
    cells: Vec<Obstacle>,
    pub corrections: u64,
}

impl DigitSolids {
    pub fn new(cells: &[Obstacle], duck: Option<&DuckEvent>, world: &mut PhysicsWorld) -> Self {
        let mut result = Self {
            cells: Vec::new(),
            corrections: 0,
        };
        result.synchronize(cells, duck, world);
        result
    }

    pub fn synchronize(
        &mut self,
        cells: &[Obstacle],
        duck: Option<&DuckEvent>,
        world: &mut PhysicsWorld,
    ) {
        if self.cells == cells {
            return;
        }
        // Only reading/format changes rebuild this bounded cell list. A
        // blinking colon retains its visible dim cells and stays physical.
        assert!(cells.len() <= super::MAX_DIGIT_SOLIDS);
        let colliders: Vec<_> = cells
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let half = (cell.max - cell.min) * 0.5;
                let mut c = ColliderSpec::cuboid(
                    ColliderId::new(DIGITS, ColliderRole::PRIMARY, i as u16),
                    half.x,
                    half.y,
                );
                c.local_position = (cell.min + cell.max) * 0.5;
                c.friction = 0.0;
                c.restitution = 0.0;
                c
            })
            .collect();
        let body = BodyId::new(DIGITS, BodyRole::PRIMARY);
        if world.contains_entity(DIGITS) {
            assert!(world.replace_colliders(body, ColliderRole::PRIMARY, &colliders));
        } else {
            assert!(world.insert_body(
                body,
                BodySpec {
                    kind: BodyKind::Fixed,
                    ..BodySpec::default()
                },
                &colliders
            ));
        }
        self.cells.clear();
        self.cells.extend_from_slice(cells);
        self.clear_new_geometry(duck, world);
    }

    fn clear_new_geometry(&mut self, duck: Option<&DuckEvent>, world: &mut PhysicsWorld) {
        // A vacant arena has no character to keep clear.
        let Some(duck) = duck else {
            return;
        };
        let Some((position, radius)) = duck.clearance_hull(world) else {
            return;
        };
        let radius = radius as f32;
        let clear = |p: Vec2| {
            self.cells.iter().all(|c| {
                let near = Vec2::new(p.x.clamp(c.min.x, c.max.x), p.y.clamp(c.min.y, c.max.y));
                (p - near).length_squared() >= radius * radius
            })
        };
        if clear(position) {
            return;
        }
        let body = duck.actor_body().expect("live hull");
        let margin = radius + duck.layout.pitch * 0.01;
        // A clock edit can light a cell through the actor. Find the nearest
        // clear side of the new cells before stepping; never let a deep solver
        // penetration launch the duck. Ordinary swimming uses contacts only.
        let mut candidates: Vec<_> = self
            .cells
            .iter()
            .flat_map(|c| {
                [
                    Vec2::new(c.min.x - margin, position.y),
                    Vec2::new(c.max.x + margin, position.y),
                    Vec2::new(position.x, c.min.y - margin),
                    Vec2::new(position.x, c.max.y + margin),
                ]
            })
            .filter(|p| {
                p.x.abs() < duck.width * 0.5 - margin
                    && p.y > duck.layout.bounds_min.y + margin
                    && p.y < duck.layout.canopy_y - margin
                    && clear(*p)
            })
            .collect();
        candidates.sort_by(|a, b| {
            (*a - position)
                .length_squared()
                .total_cmp(&(*b - position).length_squared())
        });
        if let Some(next) = candidates.into_iter().find(|p| {
            world.current_cuboid_is_clear(
                *p,
                0.0,
                Vec2::splat(radius),
                CollisionGroups::ALL,
                &[body.entity],
            )
        }) {
            let motion = world.motion(body).expect("live actor");
            world.set_pose(body, next, motion.angle, true);
            self.corrections += 1;
        }
    }

    pub fn remove(&self, world: &mut PhysicsWorld) {
        assert!(world.remove_entity(DIGITS));
    }
}
