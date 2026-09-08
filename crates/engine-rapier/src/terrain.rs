//! Chunk geometry binding. Terrain remains scenario-owned; this adapter only
//! replaces derived shapes in the canonical mechanics world.

use engine_core::Vec2;
use engine_terrain::{ChunkId, DetachedTerrain, Terrain, TerrainGeometry};

use crate::world::{
    BodyId, BodyMotion, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
    CollisionGroups, PhysicsId, PhysicsWorld,
};

#[derive(Debug, Clone, Copy)]
pub struct TerrainSpec {
    /// Reserve one consecutive role per chunk; other roles may hold sensors.
    pub first_chunk_role: u32,
    pub friction: f32,
    pub restitution: f32,
    pub collision_groups: CollisionGroups,
}

impl Default for TerrainSpec {
    fn default() -> Self {
        Self {
            first_chunk_role: 1000,
            friction: 0.9,
            restitution: 0.0,
            collision_groups: CollisionGroups::ALL,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TerrainAssembly {
    body: BodyId,
    spec: TerrainSpec,
    revisions: Vec<Option<u64>>,
}

impl TerrainAssembly {
    pub fn insert(
        world: &mut PhysicsWorld,
        id: PhysicsId,
        body_spec: BodySpec,
        terrain: &Terrain,
        geometry: &TerrainGeometry,
        spec: TerrainSpec,
    ) -> Option<Self> {
        spec.first_chunk_role
            .checked_add(terrain.chunk_count() as u32)?;
        if !spec.friction.is_finite()
            || spec.friction < 0.0
            || !spec.restitution.is_finite()
            || !(0.0..=1.0).contains(&spec.restitution)
        {
            return None;
        }
        let body = BodyId::new(id, BodyRole::PRIMARY);
        // An assembly exclusively owns its entity; this makes failed insertion
        // cleanup safe without touching preexisting bodies or sensors.
        if world.contains_entity(id) || !world.insert_body(body, body_spec, &[]) {
            return None;
        }
        let mut assembly = Self {
            body,
            spec,
            revisions: vec![None; terrain.chunk_count()],
        };
        if assembly.synchronize(world, terrain, geometry).is_none() {
            world.remove_entity(id);
            return None;
        }
        // BodySpec velocities describe the completed body's center of mass.
        world.set_velocity(
            body,
            body_spec.linear_velocity,
            body_spec.angular_velocity,
            true,
        );
        Some(assembly)
    }

    pub fn body(&self) -> BodyId {
        self.body
    }

    /// Reconcile only changed chunks. An empty chunk removes its previous shapes.
    /// The geometry cache must have been refreshed from this terrain first.
    pub fn synchronize(
        &mut self,
        world: &mut PhysicsWorld,
        terrain: &Terrain,
        geometry: &TerrainGeometry,
    ) -> Option<usize> {
        if !world.contains_body(self.body)
            || self.revisions.len() != terrain.chunk_count()
            || geometry.chunks().len() != terrain.chunk_count()
            || geometry
                .chunks()
                .iter()
                .any(|chunk| terrain.chunk_revision(chunk.id) != Some(chunk.revision))
        {
            return None;
        }
        let mut rebuilt = 0;
        let motion = world.motion(self.body)?;
        let old_center = world.center_of_mass(self.body)?;
        for chunk in geometry.chunks() {
            if self.revisions[chunk.id.0 as usize] == Some(chunk.revision) {
                continue;
            }
            let role = ColliderRole::new(self.spec.first_chunk_role + chunk.id.0);
            let colliders: Vec<_> = chunk
                .rectangles
                .iter()
                .enumerate()
                .map(|(part, rect)| {
                    let half = rect.half_extents(terrain);
                    let mut collider = ColliderSpec::cuboid(
                        ColliderId::new(self.body.entity, role, part as u16),
                        half.x,
                        half.y,
                    );
                    collider.local_position = rect.local_center(terrain);
                    collider.friction = self.spec.friction;
                    collider.restitution = self.spec.restitution;
                    collider.collision_groups = self.spec.collision_groups;
                    collider.solver_groups = self.spec.collision_groups;
                    collider
                })
                .collect();
            if !world.replace_colliders(self.body, role, &colliders) {
                return None;
            }
            self.revisions[chunk.id.0 as usize] = Some(chunk.revision);
            rebuilt += 1;
        }
        if rebuilt > 0 {
            world.refresh_mass_properties(self.body);
            let offset = world.center_of_mass(self.body)? - old_center;
            // A cut carries away its share of momentum. Preserve the velocity
            // of every surviving point when the body's center of mass changes.
            let velocity = motion.linear_velocity
                + engine_core::Vec2::new(-offset.y, offset.x) * motion.angular_velocity;
            world.set_velocity(self.body, velocity, motion.angular_velocity, true);
        }
        Some(rebuilt)
    }
}

/// Scenario-owned detached material and its derived caches. Both labs and game
/// scenarios use this path so a split preserves the same point velocities, mass,
/// cropped coordinates, and CCD behavior.
#[derive(Debug, Clone)]
pub struct TerrainFragment {
    pub id: PhysicsId,
    pub terrain: Terrain,
    pub geometry: TerrainGeometry,
    pub assembly: TerrainAssembly,
    pub hash: u64,
    pub edited_chunks: Vec<ChunkId>,
}

impl TerrainFragment {
    pub fn id(&self) -> PhysicsId {
        self.id
    }
    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    pub fn insert(
        world: &mut PhysicsWorld,
        id: PhysicsId,
        detached: DetachedTerrain,
        parent: BodyMotion,
        parent_center: Vec2,
        spec: TerrainSpec,
    ) -> Option<Self> {
        let terrain = detached.terrain;
        let geometry = TerrainGeometry::new(&terrain);
        let assembly = TerrainAssembly::insert(
            world,
            id,
            BodySpec {
                position: parent.position + detached.parent_offset.rotate_radians(parent.angle),
                angle: parent.angle,
                ccd_enabled: true,
                ..BodySpec::default()
            },
            &terrain,
            &geometry,
            spec,
        )?;
        let offset = world.center_of_mass(assembly.body())? - parent_center;
        let velocity =
            parent.linear_velocity + Vec2::new(-offset.y, offset.x) * parent.angular_velocity;
        world.set_velocity(assembly.body(), velocity, parent.angular_velocity, true);
        Some(Self {
            id,
            hash: terrain.hash(),
            terrain,
            geometry,
            assembly,
            edited_chunks: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests;
