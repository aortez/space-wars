//! Scenario impact policy. Contacts become bounded, unrecovered terrain damage
//! at the following lifecycle boundary; the field and Rapier remain independent.

use std::collections::BTreeMap;

use engine_core::Vec2;
use engine_rapier::world::{BodyId, BodyMotion, BodyRole, ContactEvent, ContactPoint, PhysicsId};
use engine_terrain::{Brush, CellCoord, EditMode, MaterialId, TerrainEdit};

use crate::{EditCause, PLANET_ID, PendingEdit, TerrainLabState};

/// Arcade damage tuning. Energy is a normal-speed/reduced-mass estimate, not a
/// reconstruction of the solver's full rotational contact energy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainImpactConfig {
    pub enabled: bool,
    pub min_speed: f32,
    pub damage_per_energy: f32,
    pub max_damage: u8,
    pub max_radius: f32,
    pub max_hits_per_tick: usize,
    pub rearm_ticks: u64,
}

impl Default for TerrainImpactConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_speed: 6.0,
            damage_per_energy: 1.0,
            max_damage: 220,
            max_radius: 1.0,
            max_hits_per_tick: 4,
            rearm_ticks: 8,
        }
    }
}

impl TerrainImpactConfig {
    pub(super) fn normalized(self) -> Self {
        let default = Self::default();
        let bounded = |value: f32, fallback, min, max| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        };
        Self {
            enabled: self.enabled,
            min_speed: bounded(self.min_speed, default.min_speed, 1.0, 100.0),
            damage_per_energy: bounded(
                self.damage_per_energy,
                default.damage_per_energy,
                0.01,
                10.0,
            ),
            max_damage: self.max_damage.max(1),
            max_radius: bounded(self.max_radius, default.max_radius, 0.0, 2.0),
            max_hits_per_tick: self.max_hits_per_tick.clamp(1, 8),
            rearm_ticks: self.rearm_ticks.clamp(1, 60),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TerrainImpactStats {
    pub hits: u64,
    pub damaged_cells: u64,
    pub destroyed_cells: u64,
    pub budget_dropped: u64,
    pub last_hits: usize,
    pub last_speed: f32,
    pub last_energy: f32,
    pub last_damage: u8,
}

#[derive(Clone, Copy)]
pub(super) struct ImpactFlash {
    pub point: Vec2,
    pub radius: f32,
    pub tick: u64,
}

#[derive(Clone, Default)]
pub(super) struct ImpactState {
    pub stats: TerrainImpactStats,
    pub contacts: BTreeMap<(PhysicsId, PhysicsId), u64>,
    pub flashes: Vec<ImpactFlash>,
}

impl ImpactState {
    /// A cut must not make a recently supported piece count the same collision
    /// again merely because it now has a new body ID.
    pub fn inherit_contacts(&mut self, parent: PhysicsId, child: PhysicsId) {
        let inherited = self
            .contacts
            .iter()
            .filter_map(|(&(a, b), &tick)| {
                let other = if a == parent {
                    b
                } else if b == parent {
                    a
                } else {
                    return None;
                };
                Some(((other.min(child), other.max(child)), tick))
            })
            .collect::<Vec<_>>();
        self.contacts.extend(inherited);
    }
}

#[derive(Clone, Copy)]
pub(super) struct ImpactBody {
    motion: BodyMotion,
    center: Vec2,
    inverse_mass: f32,
}

impl ImpactBody {
    pub fn set_kinematic_target(&mut self, position: Vec2, angle: f32, omega: f32, dt: f32) {
        let next_center = position
            + (self.center - self.motion.position).rotate_radians(angle - self.motion.angle);
        self.motion.linear_velocity = (next_center - self.center) / dt;
        self.motion.angular_velocity = omega;
    }

    fn velocity_at(&self, local: Vec2) -> Vec2 {
        let point = self.motion.position + local.rotate_radians(self.motion.angle);
        let offset = point - self.center;
        self.motion.linear_velocity + Vec2::new(-offset.y, offset.x) * self.motion.angular_velocity
    }
}

struct Candidate {
    pair: (PhysicsId, PhysicsId),
    speed: f32,
    energy: f32,
    damage: u8,
    edits: Vec<PendingEdit>,
    flash: ImpactFlash,
}

impl TerrainLabState {
    pub fn impact_stats(&self) -> TerrainImpactStats {
        self.impact.stats
    }

    /// Sample approach motion before this tick's gravity. Support acceleration
    /// alone must not turn a resting rock into a fresh impact after a rebuild.
    pub(super) fn capture_impact_motions(&self) -> BTreeMap<PhysicsId, ImpactBody> {
        if !self.config.impacts.enabled || self.fragments.is_empty() {
            return BTreeMap::new();
        }
        std::iter::once(PLANET_ID)
            .chain(self.fragments.iter().map(|f| f.id))
            .map(|id| {
                let body = BodyId::new(id, BodyRole::PRIMARY);
                (
                    id,
                    ImpactBody {
                        motion: self.physics.motion(body).expect("retained terrain body"),
                        center: self
                            .physics
                            .center_of_mass(body)
                            .expect("retained terrain mass"),
                        inverse_mass: if id == PLANET_ID {
                            0.0
                        } else {
                            1.0 / self
                                .physics
                                .body_mass(body)
                                .expect("positive fragment mass")
                        },
                    },
                )
            })
            .collect()
    }

    pub(super) fn queue_impact_damage(&mut self, before: &BTreeMap<PhysicsId, ImpactBody>) {
        let tick = self.tick;
        let config = self.config.impacts;
        self.impact.stats.last_hits = 0;
        self.impact
            .flashes
            .retain(|flash| tick.saturating_sub(flash.tick) < 18);
        self.impact.contacts.retain(|&(a, b), last| {
            before.contains_key(&a)
                && before.contains_key(&b)
                && tick.saturating_sub(*last) <= config.rearm_ticks
        });
        if before.is_empty() {
            return;
        }
        let mut current = Vec::new();
        let mut strongest = BTreeMap::<_, Candidate>::new();
        for event in self.physics.contact_events() {
            let pair = (event.collider_a.entity, event.collider_b.entity);
            if pair.0 == pair.1 || !before.contains_key(&pair.0) || !before.contains_key(&pair.1) {
                continue;
            }
            current.push(pair);
            if self.impact.contacts.contains_key(&pair) {
                continue;
            }
            let Some(candidate) = self.impact_candidate(event, before) else {
                continue;
            };
            if strongest
                .get(&pair)
                .is_none_or(|old| candidate.energy > old.energy)
            {
                strongest.insert(pair, candidate);
            }
        }
        for pair in current {
            self.impact.contacts.insert(pair, tick);
        }
        let mut candidates = strongest.into_values().collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.energy.total_cmp(&a.energy).then(a.pair.cmp(&b.pair)));
        self.impact.stats.budget_dropped +=
            candidates.len().saturating_sub(config.max_hits_per_tick) as u64;
        for candidate in candidates.into_iter().take(config.max_hits_per_tick) {
            self.pending_edits.extend(candidate.edits);
            self.impact.stats.hits += 1;
            self.impact.stats.last_hits += 1;
            self.impact.stats.last_speed = candidate.speed;
            self.impact.stats.last_energy = candidate.energy;
            self.impact.stats.last_damage = candidate.damage;
            self.impact.flashes.push(candidate.flash);
        }
        // Presentation remains bounded even when every admitted hit is visible.
        let excess = self.impact.flashes.len().saturating_sub(16);
        self.impact.flashes.drain(..excess);
    }

    fn impact_candidate(
        &self,
        event: &ContactEvent,
        before: &BTreeMap<PhysicsId, ImpactBody>,
    ) -> Option<Candidate> {
        let pair = (event.collider_a.entity, event.collider_b.entity);
        let a = before.get(&pair.0)?;
        let b = before.get(&pair.1)?;
        let local_a = event.local_contact_a?;
        let local_b = event.local_contact_b?;
        let normal = local_a.normal.rotate_radians(a.motion.angle).normalized();
        let speed = (a.velocity_at(local_a.position) - b.velocity_at(local_b.position)).dot(normal);
        let config = self.config.impacts;
        if !speed.is_finite() || speed <= config.min_speed || event.impulse_magnitude <= 0.0 {
            return None;
        }
        let mass = 1.0 / (a.inverse_mass + b.inverse_mass);
        let energy = 0.5 * mass * (speed * speed - config.min_speed * config.min_speed);
        if !energy.is_finite() || energy * config.damage_per_energy < 1.0 {
            return None;
        }
        let damage = (energy * config.damage_per_energy)
            .floor()
            .min(f32::from(config.max_damage)) as u8;
        let radius = (energy / 256.0).sqrt().min(config.max_radius);
        let edits = [(pair.0, local_a), (pair.1, local_b)]
            .into_iter()
            .filter_map(|(id, contact)| self.impact_edit(id, contact, radius, damage))
            .collect::<Vec<_>>();
        if edits.is_empty() {
            return None;
        }
        let (_, motion) = self.terrain_body(pair.0)?;
        Some(Candidate {
            pair,
            speed,
            energy,
            damage,
            edits,
            flash: ImpactFlash {
                point: motion.position + local_a.position.rotate_radians(motion.angle),
                radius,
                tick: self.tick,
            },
        })
    }

    fn impact_edit(
        &self,
        body: PhysicsId,
        contact: ContactPoint,
        radius: f32,
        damage: u8,
    ) -> Option<PendingEdit> {
        let (terrain, _) = self.terrain_body(body)?;
        let inside = contact.position - contact.normal * (terrain.cell_size() * 0.001);
        let sampled = terrain.local_to_cell(inside)?;
        // Contact manifolds often select a rectangle corner. Quantization there
        // can land just outside a one-cell fragment. Choose an occupied cell
        // touching that same point, with a bounded, stable neighborhood search.
        let epsilon = terrain.cell_size() * 0.002;
        let center = (-1..=1)
            .flat_map(|y| (-1..=1).map(move |x| CellCoord::new(sampled.x + x, sampled.y + y)))
            .filter_map(|coord| {
                if terrain.cell(coord)?.material == MaterialId::VOID {
                    return None;
                }
                let offset = terrain.cell_center(coord) - inside;
                let limit = terrain.cell_size() * 0.5 + epsilon;
                (offset.x.abs() <= limit && offset.y.abs() <= limit)
                    .then_some((coord, offset.length_squared()))
            })
            .min_by(|(a, ad), (b, bd)| ad.total_cmp(bd).then((a.y, a.x).cmp(&(b.y, b.x))))?
            .0;
        // At most an 81-cell bounding square per body, even at finer resolution.
        let radius = (radius / terrain.cell_size()).floor().min(4.0) as u32;
        let brush = Brush::Circle { center, radius };
        terrain.brush_cells(brush).ok()?.next()?;
        Some(PendingEdit {
            body,
            edit: TerrainEdit {
                brush,
                mode: EditMode::Damage(damage),
            },
            cause: EditCause::Impact,
        })
    }
}

#[cfg(test)]
mod tests;
