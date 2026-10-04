use super::*;
use engine_rapier::terrain::{PreparedRelease, RadialImpulse, TerrainBodyMut};
use engine_terrain::{Brush, EditMode, TerrainEdit, TerrainError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MaterialBalance {
    pub initial: u64,
    pub solid: u64,
    pub loose: u64,
}

impl ScorchedState {
    fn release_material(&mut self, point: Vec2) -> bool {
        let mut plans = Vec::new();
        let mut grains = 0;
        let mut fragments = 0;
        let mut emptied = 0;
        for (index, body) in self.terrain.iter().enumerate() {
            let motion = self.physics.motion(body.assembly.body()).unwrap();
            let Some(center) = body
                .terrain
                .local_to_cell((point - motion.position).rotate_radians(-motion.angle))
            else {
                continue;
            };
            let brush = Brush::Circle {
                center,
                radius: (BLAST_RADIUS / body.terrain.cell_size()).round() as u32,
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
            emptied += usize::from(body.id != GROUND && plan.source_empty());
            plans.push((index, plan));
        }
        // Admit the entire event before touching any field, loose grain or ID.
        if !self.loose.can_admit(grains) || self.terrain.len() - 1 - emptied + fragments > 64 {
            self.rejected_blasts += 1;
            self.status = "Dirt capacity reached; material retained. Tanks still take damage.";
            return false;
        }
        let mut new_fragments = Vec::new();
        for (index, plan) in plans {
            let body = &mut self.terrain[index];
            let commit = self
                .loose
                .commit(
                    &mut self.physics,
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
            if body.id != GROUND && body.geometry.shape_count() == 0 {
                self.physics.remove_entity(body.id);
                false
            } else {
                true
            }
        });
        self.terrain.extend(new_fragments);
        let bodies: Vec<_> = self
            .loose
            .iter()
            .map(|g| g.body())
            .chain(
                self.terrain
                    .iter()
                    .filter(|b| b.id != GROUND)
                    .map(|b| b.assembly.body()),
            )
            .collect();
        RadialImpulse {
            center: point,
            radius: BLAST_RADIUS + 1.0,
            speed: 16.0,
        }
        .apply(&mut self.physics, bodies);
        self.status = "Impact: displaced dirt falls, piles up and returns to the ground.";
        true
    }

    pub(super) fn explode(&mut self, point: Vec2) {
        self.impacts += 1;
        self.flashes.push((point, self.tick));
        self.release_material(point);
        // Combat damage is independent of whether the bounded material pool
        // admitted this edit. Capacity cannot make a tank invulnerable.
        for tank in &mut self.tanks {
            let motion = self.physics.motion(tank.body).unwrap();
            let distance = motion.position.distance_to(point);
            if distance < 6.0 {
                tank.health = (tank.health - 42.0 * (1.0 - distance / 6.0)).max(0.0);
            }
        }
        RadialImpulse {
            center: point,
            radius: 6.0,
            speed: 4.0,
        }
        .apply(&mut self.physics, self.tanks.iter().map(|t| t.body));
    }

    pub(super) fn settle(&mut self) {
        let commits = self
            .loose
            .settle(
                &mut self.physics,
                self.terrain.iter_mut().map(|body| TerrainBodyMut {
                    terrain: &mut body.terrain,
                    geometry: &mut body.geometry,
                    assembly: &mut body.assembly,
                }),
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
    pub fn audit(&self) -> Result<MaterialBalance, TerrainError> {
        self.loose.audit(&self.physics)?;
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
        for motion in self.physics.motions() {
            let m = motion.motion;
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
