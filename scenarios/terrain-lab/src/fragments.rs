use std::{collections::BTreeMap, time::Instant};

use engine_core::Vec2;
use engine_gravity::{
    GravityBackend, GravityConfig, GravityId, GravityParticipant, GravitySourcePolicy,
};
use engine_rapier::{
    spaceling::SpacelingSpec,
    terrain::TerrainSpec,
    world::{BodyId, BodyMotion, BodyRole, PhysicsId},
};
use engine_terrain::{ChunkId, MaterialId, Terrain, TerrainError, TerrainGeometry};

pub use engine_rapier::terrain::TerrainFragment;

use crate::{EditCause, ORE, PLANET_ID, ROCK, TerrainLabMetrics, TerrainLabState};

impl TerrainLabState {
    pub fn fragments(&self) -> &[TerrainFragment] {
        &self.fragments
    }

    pub fn terrain_cell_bytes(&self) -> usize {
        self.terrain.cell_bytes()
            + self
                .fragments
                .iter()
                .map(|f| f.terrain.cell_bytes())
                .sum::<usize>()
    }

    pub fn terrain_body(&self, id: PhysicsId) -> Option<(&Terrain, BodyMotion)> {
        let terrain = if id == PLANET_ID {
            &self.terrain
        } else {
            &self
                .fragments
                .iter()
                .find(|fragment| fragment.id == id)?
                .terrain
        };
        Some((
            terrain,
            self.physics.motion(BodyId::new(id, BodyRole::PRIMARY))?,
        ))
    }

    pub(super) fn terrain_bodies(
        &self,
    ) -> impl Iterator<Item = (&Terrain, &TerrainGeometry, BodyMotion, &[ChunkId])> {
        std::iter::once((
            &self.terrain,
            &self.geometry,
            self.planet_motion(),
            self.edited_chunks.as_slice(),
        ))
        .chain(self.fragments.iter().map(|fragment| {
            (
                &fragment.terrain,
                &fragment.geometry,
                self.physics
                    .motion(fragment.assembly.body())
                    .expect("retained fragment"),
                fragment.edited_chunks.as_slice(),
            )
        }))
    }

    fn terrain_mut(&mut self, id: PhysicsId) -> Option<&mut Terrain> {
        if id == PLANET_ID {
            Some(&mut self.terrain)
        } else {
            self.fragments
                .iter_mut()
                .find(|fragment| fragment.id == id)
                .map(|f| &mut f.terrain)
        }
    }

    pub(super) fn commit_edits(&mut self) {
        if self.pending_edits.is_empty() {
            return;
        }
        let started = Instant::now();
        let mut metrics = TerrainLabMetrics::default();
        let mut changed = BTreeMap::<PhysicsId, bool>::new();
        // Commit all queued coordinates to their original owner before splitting
        // anything. They were quantized against the previous completed step.
        for pending in std::mem::take(&mut self.pending_edits) {
            let result = self
                .terrain_mut(pending.body)
                .ok_or(TerrainError("unknown terrain body"))
                .and_then(|terrain| terrain.apply(pending.edit));
            match result {
                Ok(result) => {
                    metrics.changed_cells += result.changed_cells;
                    let removed = result.removed.iter().map(|m| m.cells).sum::<u32>();
                    metrics.removed_cells += removed;
                    if result.changed_cells > 0 {
                        *changed.entry(pending.body).or_default() |= removed > 0;
                    }
                    if pending.cause == EditCause::Impact {
                        self.impact.stats.damaged_cells += u64::from(result.changed_cells);
                        self.impact.stats.destroyed_cells += u64::from(removed);
                    }
                    if pending.cause == EditCause::Mining {
                        for removed in result.removed {
                            match removed.material {
                                ROCK => self.recovered.rock_cells += u64::from(removed.cells),
                                ORE => self.recovered.ore_cells += u64::from(removed.cells),
                                _ => {}
                            }
                        }
                    }
                }
                Err(_) => self.rejected_edits += 1,
            }
        }
        metrics.edit_time = started.elapsed();
        for (id, removed) in changed {
            let body = BodyId::new(id, BodyRole::PRIMARY);
            let motion = self.physics.motion(body).expect("edited terrain body");
            let old_center = self
                .physics
                .center_of_mass(body)
                .expect("edited terrain mass");
            let started = Instant::now();
            let detached = if removed {
                self.terrain_mut(id)
                    .unwrap()
                    .detach_disconnected()
                    .expect("valid terrain split")
            } else {
                Vec::new()
            };
            metrics.connectivity_time += started.elapsed();
            let started = Instant::now();
            if id == PLANET_ID {
                self.edited_chunks = self.geometry.refresh(&self.terrain);
                metrics.rebuilt_chunks += self
                    .terrain_assembly
                    .synchronize(&mut self.physics, &self.terrain, &self.geometry)
                    .expect("valid planet cache");
                self.terrain_hash = self.terrain.hash();
            } else {
                let index = self
                    .fragments
                    .iter()
                    .position(|fragment| fragment.id == id)
                    .unwrap();
                let fragment = &mut self.fragments[index];
                fragment.edited_chunks = fragment.geometry.refresh(&fragment.terrain);
                metrics.rebuilt_chunks += fragment
                    .assembly
                    .synchronize(&mut self.physics, &fragment.terrain, &fragment.geometry)
                    .expect("valid fragment cache");
                fragment.hash = fragment.terrain.hash();
                if fragment.geometry.rectangle_count() == 0 {
                    self.physics.remove_entity(id);
                    self.fragments.remove(index);
                }
            }
            let parent_id = id;
            for detached in detached {
                metrics.detached_cells += detached
                    .terrain
                    .cells()
                    .iter()
                    .filter(|c| c.material != MaterialId::VOID)
                    .count() as u32;
                let id = PhysicsId::new(self.next_fragment_id);
                self.next_fragment_id = self
                    .next_fragment_id
                    .checked_add(1)
                    .expect("fragment ID exhausted");
                let fragment = TerrainFragment::insert(
                    &mut self.physics,
                    id,
                    detached,
                    motion,
                    old_center,
                    TerrainSpec::default(),
                )
                .expect("valid detached terrain");
                self.impact.inherit_contacts(parent_id, id);
                metrics.rebuilt_chunks += fragment.terrain.chunk_count();
                metrics.spawned_fragments += 1;
                self.fragments.push(fragment);
            }
            metrics.rebuild_time += started.elapsed();
        }
        self.removed_cells += u64::from(metrics.removed_cells);
        self.last_edit = metrics;
    }

    /// Fragments respond to the existing softened point source. Their mass and
    /// inertia affect collisions; they do not become new gravity sources yet.
    pub(super) fn apply_fragment_gravity(&mut self, dt: f32) -> Vec2 {
        let reference = self.config.radius + SpacelingSpec::default().half_height();
        let mut participants = vec![
            GravityParticipant {
                id: GravityId::new(1),
                position: self.planet_motion().position,
                source_mass: self.config.gravity_acceleration * reference * reference,
                response_scale: 0.0,
                source_policy: GravitySourcePolicy::Direct,
                source_shape: engine_gravity::GravitySourceShape::Point,
            },
            GravityParticipant::target(
                GravityId::new(2),
                self.spaceling_snapshot().motion.position,
                1.0,
            ),
        ];
        participants.extend(self.fragments.iter().map(|fragment| {
            GravityParticipant::target(
                GravityId::new(fragment.id.value()),
                self.physics
                    .center_of_mass(fragment.assembly.body())
                    .expect("fragment mass"),
                1.0,
            )
        }));
        let deltas = self
            .gravity_solver
            .solve(
                &participants,
                GravityConfig {
                    backend: GravityBackend::Exact,
                    softening: self.config.radius * 0.2,
                    interaction_scale: dt,
                },
            )
            .expect("valid lab gravity");
        for (fragment, delta) in self.fragments.iter().zip(&deltas[2..]) {
            self.physics
                .apply_velocity_delta(fragment.assembly.body(), delta.velocity_delta, true);
        }
        deltas[1].velocity_delta
    }
}

#[cfg(test)]
mod tests;
