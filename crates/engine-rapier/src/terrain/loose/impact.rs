//! Bounded, contact-local chipping. Closing speed rejects support/correction
//! forces; solver impulse per clump mass rejects fast but insignificant hits.
use super::*;
use crate::world::{BodyKind, ContactPoint};
use engine_terrain::Brush;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImpactFractureConfig {
    /// Both incoming normal speed and impulse / source mass must exceed this.
    /// This is a gameplay material parameter, not a calibrated soil stress.
    pub yield_speed: f32,
    pub max_cells: usize,
    pub max_contacts: usize,
    pub cooldown_seconds: f32,
}
impl Default for ImpactFractureConfig {
    fn default() -> Self {
        Self {
            yield_speed: 6.0,
            max_cells: 16,
            max_contacts: 512,
            cooldown_seconds: 0.35,
        }
    }
}
impl ImpactFractureConfig {
    pub(super) fn valid(self) -> bool {
        self.yield_speed.is_finite()
            && (0.5..=1000.0).contains(&self.yield_speed)
            && (1..=24).contains(&self.max_cells)
            && (1..=4096).contains(&self.max_contacts)
            && self.cooldown_seconds.is_finite()
            && (0.1..=10.0).contains(&self.cooldown_seconds)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct ImpactFractureDiagnostics {
    pub pending: usize,
    pub scanned: usize,
    pub failures: u64,
    pub released_cells: u64,
    pub held_checks: u64,
    pub stale_hits: u64,
    pub capacity_denials: u64,
    pub fragment_denials: u64,
    pub contact_overflow: u64,
    pub budget_skips: u64,
    pub peak_severity: f32,
}

#[derive(Debug, Clone, Copy)]
struct Incoming {
    motion: BodyMotion,
    center: Vec2,
}
impl Incoming {
    fn velocity(self, local: Vec2) -> Vec2 {
        let offset = self.motion.position + local.rotate_radians(self.motion.angle) - self.center;
        self.motion.linear_velocity + Vec2::new(-offset.y, offset.x) * self.motion.angular_velocity
    }
}
#[derive(Debug, Clone, Copy)]
struct Hit {
    revision: u64,
    contact: ContactPoint,
    impulse: f32,
    strongest: f32,
    speed: f32,
    mass: f32,
}
impl Hit {
    fn severity(self) -> f32 {
        self.speed.min(self.impulse / self.mass)
    }
}
#[derive(Debug, Clone, Default)]
pub(super) struct ImpactState {
    clock: f64,
    incoming: BTreeMap<BodyId, Incoming>,
    sources: BTreeMap<BodyId, (u64, f32)>,
    pending: BTreeMap<BodyId, Hit>,
    cooldown: BTreeMap<BodyId, f64>,
    diagnostics: ImpactFractureDiagnostics,
}

impl LooseTerrain {
    /// Call immediately before the canonical solve. Only dynamic terrain can
    /// chip; the other bodies supply relative contact velocities, including
    /// prescribed translating/rotating supports. Does not apply forces.
    pub fn capture_impact_motion<'a>(
        &mut self,
        world: &PhysicsWorld,
        sources: impl IntoIterator<Item = (BodyId, &'a Terrain)>,
        dt: f32,
    ) {
        if self.config.impact_fracture.is_none() || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let state = &mut self.impact;
        state.clock += f64::from(dt);
        state
            .cooldown
            .retain(|id, until| *until > state.clock && world.contains_body(*id));
        state.sources.clear();
        state.incoming.clear();
        for (id, terrain) in sources {
            if world.body_kind(id) == Some(BodyKind::Dynamic) && !state.cooldown.contains_key(&id) {
                let mass = world.body_mass(id).unwrap_or(0.0);
                if mass.is_finite() && mass > 0.0 {
                    state.sources.insert(id, (terrain.revision(), mass));
                }
            }
        }
        if state.sources.is_empty() {
            return;
        }
        for entry in world.motions() {
            if let (Some(motion), Some(center)) = (
                world.incoming_motion(entry.id, dt),
                world.center_of_mass(entry.id),
            ) {
                state.incoming.insert(entry.id, Incoming { motion, center });
            }
        }
    }

    /// Call immediately after the solve, before editing any colliders. Owning
    /// this small queue preserves transient CCD hits across scenario clones.
    /// The next material boundary consumes each hit once, even if refused.
    pub fn sample_impact_contacts(&mut self, world: &PhysicsWorld) {
        let Some(config) = self.config.impact_fracture else {
            return;
        };
        let state = &mut self.impact;
        state.pending.clear();
        state.diagnostics.scanned = 0;
        if state.sources.is_empty() {
            return;
        }
        let events = world.contact_events();
        state.diagnostics.contact_overflow +=
            events.len().saturating_sub(config.max_contacts) as u64;
        for event in events.iter().take(config.max_contacts) {
            state.diagnostics.scanned += 1;
            let (Some(a), Some(b), Some(pa), Some(pb)) = (
                world.collider_body(event.collider_a),
                world.collider_body(event.collider_b),
                event.local_contact_a,
                event.local_contact_b,
            ) else {
                continue;
            };
            if !state.sources.contains_key(&a) && !state.sources.contains_key(&b) {
                continue;
            }
            let (Some(ma), Some(mb)) = (state.incoming.get(&a), state.incoming.get(&b)) else {
                continue;
            };
            let speed = (ma.velocity(pa.position) - mb.velocity(pb.position)).dot(event.normal);
            let impulse = event.impulse_magnitude;
            if !speed.is_finite()
                || !impulse.is_finite()
                || speed <= config.yield_speed
                || impulse <= 0.0
            {
                state.diagnostics.held_checks += 1;
                continue;
            }
            for (id, contact) in [(a, pa), (b, pb)] {
                let Some(&(revision, mass)) = state.sources.get(&id) else {
                    continue;
                };
                let hit = state.pending.entry(id).or_insert(Hit {
                    revision,
                    contact,
                    impulse: 0.0,
                    strongest: 0.0,
                    speed: 0.0,
                    mass,
                });
                hit.impulse += impulse;
                hit.speed = hit.speed.max(speed);
                if impulse > hit.strongest {
                    hit.strongest = impulse;
                    hit.contact = contact;
                }
            }
        }
        state.pending.retain(|_, hit| {
            let severity = hit.severity();
            state.diagnostics.peak_severity = state.diagnostics.peak_severity.max(severity);
            if severity <= config.yield_speed {
                state.diagnostics.held_checks += 1;
            }
            severity > config.yield_speed
        });
        state.sources.clear();
        state.incoming.clear();
    }

    /// Publish at most one contact patch per material boundary, before
    /// deposition. No retry of old impacts and no artificial explosion kick.
    pub fn fracture_impacts<'a>(
        &mut self,
        world: &mut PhysicsWorld,
        sources: impl IntoIterator<Item = TerrainBodyMut<'a>>,
        next_id: &mut u64,
        fragment_capacity: usize,
    ) -> Result<Option<SlumpCommit>, ReleaseError> {
        let _span = crate::profile::span("impact_fracture");
        let Some(config) = self.config.impact_fracture else {
            return Ok(None);
        };
        let pending = std::mem::take(&mut self.impact.pending);
        if pending.is_empty() {
            return Ok(None);
        }
        let mut fields: BTreeMap<_, _> = sources
            .into_iter()
            .map(|f| (f.assembly.body(), f))
            .collect();
        // Prefer the most severe hit, then stable body ID. Excess simultaneous
        // hits are consumed, not queued as delayed damage to a resting clump.
        let mut pending: Vec<_> = pending.into_iter().collect();
        pending.sort_by(|a, b| {
            b.1.severity()
                .total_cmp(&a.1.severity())
                .then(a.0.cmp(&b.0))
        });
        let count = pending.len();
        for (index, (body, hit)) in pending.into_iter().enumerate() {
            let Some(field) = fields.remove(&body) else {
                continue;
            };
            if field.terrain.revision() != hit.revision
                || world.body_kind(body) != Some(BodyKind::Dynamic)
            {
                self.impact.diagnostics.stale_hits += 1;
                continue;
            }
            // Connectivity work can span the source field: attempt only one
            // fresh patch per boundary, including capacity-refused patches.
            self.impact.diagnostics.budget_skips += (count - index - 1) as u64;
            let plan = impact_patch(field.terrain, hit, config).map_err(ReleaseError::Invalid)?;
            if plan.grain_count() == 0 {
                return Ok(None);
            }
            if !self.can_admit(plan.grain_count()) {
                self.impact.diagnostics.capacity_denials += 1;
                return Ok(None);
            }
            if plan.fragment_count()
                > fragment_capacity.saturating_add(usize::from(plan.source_empty()))
            {
                self.impact.diagnostics.fragment_denials += 1;
                return Ok(None);
            }
            let count = plan.grain_count() as u64;
            let release = self.commit(world, field, plan, next_id)?;
            let until = self.impact.clock + f64::from(config.cooldown_seconds);
            self.impact.cooldown.insert(body, until);
            for fragment in &release.fragments {
                self.impact.cooldown.insert(fragment.assembly.body(), until);
            }
            self.impact.diagnostics.failures += 1;
            self.impact.diagnostics.released_cells += count;
            return Ok(Some(SlumpCommit { body, release }));
        }
        Ok(None)
    }

    pub fn impact_diagnostics(&self) -> ImpactFractureDiagnostics {
        ImpactFractureDiagnostics {
            pending: self.impact.pending.len(),
            ..self.impact.diagnostics
        }
    }

    pub(super) fn hash_impacts(&self, write: &mut impl FnMut(u64)) {
        let Some(config) = self.config.impact_fracture else {
            return;
        };
        write(0x494d50414354);
        for v in [config.yield_speed, config.cooldown_seconds] {
            write(u64::from(v.to_bits()));
        }
        write(config.max_cells as u64);
        write(config.max_contacts as u64);
        write(self.impact.clock.to_bits());
        for (id, until) in &self.impact.cooldown {
            write(id.entity.value());
            write(u64::from(id.role.value()));
            write(until.to_bits());
        }
        write(u64::MAX);
        for (id, hit) in &self.impact.pending {
            write(id.entity.value());
            write(u64::from(id.role.value()));
            write(hit.revision);
            for v in [
                hit.contact.position.x,
                hit.contact.position.y,
                hit.contact.normal.x,
                hit.contact.normal.y,
                hit.impulse,
                hit.strongest,
                hit.speed,
                hit.mass,
            ] {
                write(u64::from(v.to_bits()));
            }
        }
        write(u64::MAX);
        for (id, (revision, mass)) in &self.impact.sources {
            write(id.entity.value());
            write(u64::from(id.role.value()));
            write(*revision);
            write(u64::from(mass.to_bits()));
        }
        write(u64::MAX);
        for (id, incoming) in &self.impact.incoming {
            write(id.entity.value());
            write(u64::from(id.role.value()));
            let m = incoming.motion;
            for v in [
                m.position.x,
                m.position.y,
                m.angle,
                m.linear_velocity.x,
                m.linear_velocity.y,
                m.angular_velocity,
                incoming.center.x,
                incoming.center.y,
            ] {
                write(u64::from(v.to_bits()));
            }
        }
    }
}

fn impact_patch(
    source: &Terrain,
    hit: Hit,
    config: ImpactFractureConfig,
) -> Result<PreparedRelease, TerrainError> {
    let mut terrain = source.clone();
    let cell = source.cell_size();
    let normal = hit.contact.normal.normalized();
    let entry = hit.contact.position - normal * (0.6 * cell);
    let mut selected = Vec::new();
    if let Some(start) = source.local_to_cell(entry) {
        let depth = (hit.severity() / config.yield_speed).clamp(1.0, 3.0);
        let end = source
            .local_to_cell(entry - normal * (depth * cell))
            .unwrap_or(start);
        selected = source
            .brush_cells(Brush::Capsule {
                start,
                end,
                radius: 1,
            })?
            .map(|(c, _)| c)
            .collect();
        selected.sort_by(|a, b| {
            source
                .cell_center(*a)
                .distance_to(entry)
                .total_cmp(&source.cell_center(*b).distance_to(entry))
                .then((a.y, a.x).cmp(&(b.y, b.x)))
        });
        selected.truncate(config.max_cells);
    }
    let grains = terrain.extract_individual_cells(&selected)?;
    let detached = if grains.is_empty() {
        Vec::new()
    } else {
        terrain.detach_disconnected()?
    };
    Ok(PreparedRelease {
        source: source.clone(),
        terrain,
        changed_cells: grains.len() as u64,
        grains,
        detached,
        discarded_cells: 0,
    })
}

#[cfg(test)]
mod tests;
