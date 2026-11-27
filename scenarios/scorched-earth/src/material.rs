use super::*;
use engine_rapier::terrain::{PreparedRelease, RadialImpulse, TerrainBodyMut};
use engine_rapier::world::weapon::{Recipients, body_push, bought};
use engine_terrain::{Brush, EditMode, TerrainEdit, TerrainError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MaterialBalance {
    pub initial: u64,
    pub solid: u64,
    pub loose: u64,
}

impl Duel {
    /// Development capture, outside simulation timing. No scenario state changes.
    pub fn capture_packing(
        &mut self,
        physics: &mut PhysicsWorld,
    ) -> Result<Vec<engine_rapier::terrain::PackingSnapshot>, TerrainError> {
        self.loose.capture_packing(
            physics,
            self.terrain.iter_mut().map(|body| TerrainBodyMut {
                terrain: &mut body.terrain,
                geometry: &mut body.geometry,
                assembly: &mut body.assembly,
            }),
        )
    }

    fn release_material(&mut self, physics: &mut PhysicsWorld, point: Vec2) -> bool {
        if !self.material_ids_available() {
            self.rejected_blasts += 1;
            return false;
        }
        let mut plans = Vec::new();
        let mut grains = 0;
        let mut fragments = 0;
        let mut emptied = 0;
        for (index, body) in self.terrain.iter().enumerate() {
            let motion = self.motion(physics, body.assembly.body()).unwrap();
            let Some(center) = body.terrain.local_to_cell(
                ((point - motion.position) * self.placement.scale).rotate_radians(-motion.angle),
            ) else {
                continue;
            };
            let brush = Brush::Circle {
                center,
                radius: (BLAST_RADIUS * self.placement.scale / body.terrain.cell_size()).round()
                    as u32,
            };
            if body
                .terrain
                .brush_cells(brush)
                .expect("bounded blast")
                .next()
                .is_none()
            {
                continue;
            }
            let plan = PreparedRelease::new(
                &body.terrain,
                &[TerrainEdit {
                    brush,
                    mode: EditMode::Remove,
                }],
            )
            .expect("valid release plan");
            grains += plan.grain_count();
            fragments += plan.fragment_count();
            emptied += usize::from(body.id != self.placement.ground() && plan.source_empty());
            plans.push((index, plan));
        }
        // Admit the whole material transfer before touching any field or ID.
        // The caller still applies the impact to existing bodies if it cannot fit.
        let capacity = self.loose.release_capacity(grains);
        if !capacity.fits() || self.terrain.len() - 1 - emptied + fragments > MAX_TERRAIN_FRAGMENTS
        {
            if !capacity.fits() {
                self.last_capacity_rejection = Some(capacity);
            }
            self.rejected_blasts += 1;
            self.status =
                "Dirt capacity reached; ground retained. Loose dirt and tanks still react.";
            return false;
        }
        let mut new_fragments = Vec::new();
        for (index, plan) in plans {
            let body = &mut self.terrain[index];
            let commit = self
                .loose
                .commit(
                    physics,
                    TerrainBodyMut {
                        terrain: &mut body.terrain,
                        geometry: &mut body.geometry,
                        assembly: &mut body.assembly,
                    },
                    plan,
                    &mut self.next_id,
                )
                .expect("pre-admitted material transfer");
            body.hash = body.terrain.hash();
            body.edited_chunks = commit.dirty_chunks;
            new_fragments.extend(commit.fragments);
        }
        self.terrain.retain(|body| {
            if body.id != self.placement.ground() && body.geometry.shape_count() == 0 {
                physics.remove_entity(body.id);
                false
            } else {
                true
            }
        });
        self.terrain.extend(new_fragments);
        self.status = "Impact: displaced dirt falls, piles up and returns to the ground.";
        true
    }

    #[cfg(test)]
    pub(super) fn explode(&mut self, physics: &mut PhysicsWorld, point: Vec2) {
        self.explode_from(physics, point, None);
    }

    /// A burst at `point` without a shell, such as a scripted charge: its
    /// payload alone. A bot hears of damage when `owner` is the other tank.
    #[cfg(test)]
    pub(super) fn explode_from(
        &mut self,
        physics: &mut PhysicsWorld,
        point: Vec2,
        owner: Option<usize>,
    ) {
        let before = self.tanks.map(|tank| tank.health);
        let owner_entity = owner.map(|slot| self.tanks[slot].body.entity.value());
        let mut source = armor::charge(0, owner_entity);
        let mut keys = armor::Keys::default();
        let mut recipients = armor::NoRecipients;
        self.burst(
            physics,
            &mut recipients,
            point,
            point,
            &mut source,
            &mut keys,
        );
        self.notify_hits(owner, before);
    }

    /// A burst's crater and pushes at `point`, and its payload released from
    /// `center` into the tanks' armor and the host's tissue. Returns what the
    /// tanks' armor took and what the host's bodies stopped.
    pub(super) fn burst(
        &mut self,
        physics: &mut PhysicsWorld,
        recipients: &mut impl Recipients,
        point: Vec2,
        center: Vec2,
        source: &mut Source,
        keys: &mut armor::Keys,
    ) -> armor::Struck {
        self.impacts += 1;
        self.flashes.push((point, self.tick));
        self.release_material(physics, point);
        // A refused crater retains its cells, but cannot shield existing dirt.
        // Apply once after admission so newly released bodies share the impulse.
        let bodies: Vec<_> = self
            .loose
            .iter()
            .map(|g| g.body())
            .chain(
                self.terrain
                    .iter()
                    .filter(|b| b.id != self.placement.ground())
                    .map(|b| b.assembly.body()),
            )
            .collect();
        RadialImpulse {
            center: self.placement.point_to_world(point),
            radius: (BLAST_RADIUS + 1.0) * self.placement.scale,
            speed: 16.0 * self.placement.scale,
        }
        .apply(physics, bodies);
        // Combat damage is independent of whether the bounded material pool
        // admitted this edit. Capacity cannot make a tank invulnerable.
        let pushes = self.tank_pushes(
            physics,
            RadialImpulse {
                center: self.placement.point_to_world(point),
                radius: 6.0 * self.placement.scale,
                speed: 4.0 * self.placement.scale,
            },
        );
        let released = self.release_payload(physics, recipients, center, source, keys, &pushes);
        // A share too small to pay for a tank's whole push moves it as far as
        // the share pays for.
        for (slot, (push, paid)) in pushes.iter().zip(released.pushed).enumerate() {
            if paid > 0.0 {
                let delta = bought(push.delta, push.energy, paid);
                physics.apply_velocity_delta(self.tanks[slot].body, delta, true);
            }
        }
        released
    }

    /// Each tank's push from `impulse` on its hull, by slot, in Scorched's
    /// units: a world at another scale carries energies times the scale's
    /// fourth power. A hull that can't move, such as one the bot lab fixes in
    /// place, takes none.
    fn tank_pushes(&self, physics: &PhysicsWorld, impulse: RadialImpulse) -> [armor::Push; 2] {
        let scale = self.placement.scale;
        self.tanks.each_ref().map(|tank| {
            body_push(physics, tank.body, impulse).map_or_else(
                armor::Push::default,
                |(delta, energy)| armor::Push {
                    delta,
                    energy: energy / scale.powi(4),
                },
            )
        })
    }

    pub fn slumping_diagnostics(&self) -> engine_rapier::terrain::SlumpingDiagnostics {
        self.loose.slumping_diagnostics()
    }

    pub fn cohesion_diagnostics(&self) -> engine_rapier::terrain::CohesionDiagnostics {
        self.loose.cohesion_diagnostics()
    }

    pub fn impact_diagnostics(&self) -> engine_rapier::terrain::ImpactFractureDiagnostics {
        self.loose.impact_diagnostics()
    }

    pub(super) fn fracture_impacts(&mut self, physics: &mut PhysicsWorld) {
        if !self.material_ids_available() {
            return;
        }
        if self.loose.config().impact_fracture.is_none() {
            return;
        }
        let capacity = MAX_TERRAIN_FRAGMENTS.saturating_sub(self.terrain.len() - 1);
        let commit = self
            .loose
            .fracture_impacts(
                physics,
                self.terrain.iter_mut().map(|body| TerrainBodyMut {
                    terrain: &mut body.terrain,
                    geometry: &mut body.geometry,
                    assembly: &mut body.assembly,
                }),
                &mut self.next_id,
                capacity,
            )
            .expect("valid shared impact fracture");
        if let Some(commit) = commit {
            self.register_yield(physics, commit);
        }
    }

    pub(super) fn fracture_banks(&mut self, physics: &mut PhysicsWorld) {
        if !self.material_ids_available() {
            return;
        }
        if self.loose.config().cohesion.is_none() {
            return;
        }
        let capacity = MAX_TERRAIN_FRAGMENTS.saturating_sub(self.terrain.len() - 1);
        let commit = self
            .loose
            .fracture_banks(
                physics,
                self.terrain.iter_mut().map(|body| TerrainBodyMut {
                    terrain: &mut body.terrain,
                    geometry: &mut body.geometry,
                    assembly: &mut body.assembly,
                }),
                |_, _| Vec2::new(0.0, -GRAVITY * self.placement.scale),
                &mut self.next_id,
                capacity,
                DT,
            )
            .expect("valid shared bank failure");
        if let Some(commit) = commit {
            self.register_yield(physics, commit);
        }
    }

    pub(super) fn slump(&mut self, physics: &mut PhysicsWorld) {
        if !self.material_ids_available() {
            return;
        }
        let capacity = MAX_TERRAIN_FRAGMENTS.saturating_sub(self.terrain.len() - 1);
        let commit = self
            .loose
            .slump(
                physics,
                self.terrain.iter_mut().map(|body| TerrainBodyMut {
                    terrain: &mut body.terrain,
                    geometry: &mut body.geometry,
                    assembly: &mut body.assembly,
                }),
                |_, _| Vec2::new(0.0, -GRAVITY * self.placement.scale),
                &mut self.next_id,
                capacity,
                DT,
            )
            .expect("valid shared slumping");
        if let Some(commit) = commit {
            self.register_yield(physics, commit);
        }
    }

    fn register_yield(
        &mut self,
        physics: &mut PhysicsWorld,
        commit: engine_rapier::terrain::SlumpCommit,
    ) {
        let body = self
            .terrain
            .iter_mut()
            .find(|b| b.assembly.body() == commit.body)
            .unwrap();
        body.hash = body.terrain.hash();
        body.edited_chunks = commit.release.dirty_chunks;
        self.terrain.retain(|body| {
            if body.id != self.placement.ground() && body.geometry.shape_count() == 0 {
                physics.remove_entity(body.id);
                false
            } else {
                true
            }
        });
        self.terrain.extend(commit.release.fragments);
    }

    pub(super) fn settle(&mut self, physics: &mut PhysicsWorld) {
        let commits = self
            .loose
            .settle_with_gravity(
                physics,
                self.terrain.iter_mut().map(|body| TerrainBodyMut {
                    terrain: &mut body.terrain,
                    geometry: &mut body.geometry,
                    assembly: &mut body.assembly,
                }),
                |_, _| Vec2::new(0.0, -GRAVITY * self.placement.scale),
                DT,
            )
            .expect("valid shared deposition");
        for commit in commits {
            let body = self
                .terrain
                .iter_mut()
                .find(|b| b.assembly.body() == commit.body)
                .unwrap();
            body.hash = body.terrain.hash();
            body.edited_chunks = commit.dirty_chunks;
        }
    }

    /// Kept outside timed simulation steps. Offscreen dirt remains conserved.
    pub fn audit(&self, physics: &PhysicsWorld) -> Result<MaterialBalance, TerrainError> {
        self.loose.audit(physics)?;
        let mut solid = 0;
        for body in &self.terrain {
            if !body.geometry.is_current(&body.terrain) || body.hash != body.terrain.hash() {
                return Err(TerrainError("stale terrain geometry"));
            }
            solid += body
                .terrain
                .cells()
                .iter()
                .filter(|c| c.material != MaterialId::VOID)
                .count() as u64;
        }
        for body in self.body_ids() {
            let m = physics
                .motion(body)
                .ok_or(TerrainError("missing owned body"))?;
            if !m.position.x.is_finite()
                || !m.position.y.is_finite()
                || !m.angle.is_finite()
                || !m.linear_velocity.x.is_finite()
                || !m.linear_velocity.y.is_finite()
                || !m.angular_velocity.is_finite()
            {
                return Err(TerrainError("nonfinite motion"));
            }
        }
        let balance = MaterialBalance {
            initial: self.initial_cells,
            solid,
            loose: self.loose.len() as u64,
        };
        if balance.solid + balance.loose != balance.initial {
            return Err(TerrainError("material conservation failed"));
        }
        Ok(balance)
    }
}

impl ScorchedState {
    pub fn capture_packing(
        &mut self,
    ) -> Result<Vec<engine_rapier::terrain::PackingSnapshot>, TerrainError> {
        self.duel.capture_packing(&mut self.physics)
    }

    #[cfg(test)]
    pub(super) fn explode(&mut self, point: Vec2) {
        self.duel.explode(&mut self.physics, point)
    }
    #[cfg(test)]
    pub(super) fn explode_from(&mut self, point: Vec2, owner: Option<usize>) {
        self.duel.explode_from(&mut self.physics, point, owner)
    }

    pub fn audit(&self) -> Result<MaterialBalance, TerrainError> {
        self.duel.audit(&self.physics)
    }
}
