//! Completed-step surface membership, separate from eligibility to work a flag.
//! Contacts establish membership; proximity and gravity selection never do.
use super::*;
use engine_rapier::world::{BodyId, ColliderId};
use std::collections::BTreeMap;

/// Covers a short hop or lost wheel contact without retaining a flying force.
pub const SURFACE_CONTACT_GRACE: Duration = Duration::from_secs(1);
/// Measured from the last supported body position in the planet's moving frame.
pub const SURFACE_DEPARTURE_DISTANCE: f32 = 8.0;
pub const MAX_GROUND_FORCES: usize = 32;
pub const MAX_FORCE_COLLIDERS: usize = 4;

/// Unit allegiance does not depend on the planet's current flag or player seats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceAllegiance {
    Player(PlayerId),
    Government(u64),
}

impl SurfaceAllegiance {
    fn opposes(self, player: PlayerId) -> bool {
        self != Self::Player(player)
    }
}

/// Counts reveal why a claim is blocked without supplying target positions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SurfaceOpposition {
    pub pilots: u16,
    pub ships: u16,
    pub ground_units: u16,
}

impl SurfaceOpposition {
    pub fn is_empty(&self) -> bool {
        self.pilots == 0 && self.ships == 0 && self.ground_units == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceForceId {
    Pilot(u64),
    Ship(usize),
    Ground(PhysicsId),
}

/// A live ground actor registers its own native assembly. The owning actor must
/// unregister on retirement/destruction even if a physical wreck is retained.
/// Missing native bodies are also pruned at the completed-step boundary.
#[derive(Debug, Clone)]
pub struct GroundSurfaceForce {
    pub body: BodyId,
    pub colliders: Vec<ColliderId>,
    pub allegiance: Option<SurfaceAllegiance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SurfaceForceObservation {
    pub id: SurfaceForceId,
    pub allegiance: SurfaceAllegiance,
    pub planet: usize,
    pub contact_gap_seconds: f32,
}

#[derive(Debug, Clone, Copy)]
struct Membership {
    planet: usize,
    local_position: Vec2,
    contact_gap: Duration,
}

#[derive(Debug, Clone, Copy)]
struct Presence {
    allegiance: SurfaceAllegiance,
    membership: Membership,
}

#[derive(Clone, Default)]
pub(super) struct SurfaceForces {
    present: BTreeMap<SurfaceForceId, Presence>,
    ground: BTreeMap<PhysicsId, GroundSurfaceForce>,
}

struct ForceSample {
    id: SurfaceForceId,
    allegiance: SurfaceAllegiance,
    position: Vec2,
    support: Option<usize>,
    retain_contact_gap: bool,
}

impl SurfaceForces {
    pub(super) fn observations(&self) -> Vec<SurfaceForceObservation> {
        self.present
            .iter()
            .map(|(&id, p)| SurfaceForceObservation {
                id,
                allegiance: p.allegiance,
                planet: p.membership.planet,
                contact_gap_seconds: p.membership.contact_gap.as_secs_f32(),
            })
            .collect()
    }

    pub(super) fn opposition(&self, planet: usize, player: PlayerId) -> SurfaceOpposition {
        let mut counts = SurfaceOpposition::default();
        for (&id, p) in &self.present {
            if p.membership.planet != planet || !p.allegiance.opposes(player) {
                continue;
            }
            match id {
                SurfaceForceId::Pilot(_) => counts.pilots += 1,
                SurfaceForceId::Ship(_) => counts.ships += 1,
                SurfaceForceId::Ground(_) => counts.ground_units += 1,
            }
        }
        counts
    }

    fn update(&mut self, samples: &[ForceSample], frames: &[motion::SurfaceFrame], dt: Duration) {
        self.present
            .retain(|id, _| samples.iter().any(|s| s.id == *id));
        for sample in samples {
            let local = |planet: usize| {
                (sample.position - frames[planet].position).rotate_radians(-frames[planet].angle)
            };
            let membership = if let Some(planet) = sample.support.filter(|&p| p < frames.len()) {
                Some(Membership {
                    planet,
                    local_position: local(planet),
                    contact_gap: Duration::ZERO,
                })
            } else {
                self.present.get(&sample.id).and_then(|previous| {
                    let mut m = previous.membership;
                    m.contact_gap = m.contact_gap.saturating_add(dt);
                    (sample.retain_contact_gap
                        && sample.allegiance == previous.allegiance
                        && m.planet < frames.len()
                        && m.contact_gap < SURFACE_CONTACT_GRACE
                        && local(m.planet).distance_to(m.local_position)
                            <= SURFACE_DEPARTURE_DISTANCE)
                        .then_some(m)
                })
            };
            if let Some(membership) = membership {
                self.present.insert(
                    sample.id,
                    Presence {
                        allegiance: sample.allegiance,
                        membership,
                    },
                );
            } else {
                self.present.remove(&sample.id);
            }
        }
    }
}

impl SurfaceSortieState {
    /// Bounded extension for future tanks. Registration cannot fabricate support
    /// or duplicate a pilot/ship; the next completed step samples real contacts.
    pub fn register_ground_surface_force(&mut self, force: GroundSurfaceForce) -> bool {
        let id = force.body.entity;
        if force.colliders.is_empty()
            || force.colliders.len() > MAX_FORCE_COLLIDERS
            || self.world.physics.world.motion(force.body).is_none()
            || force
                .colliders
                .iter()
                .any(|&c| c.entity != id || self.world.physics.world.collider_body(c).is_none())
            || self.pilots.iter().any(|p| {
                pilot_physics_id(p.owner) == id
                    || self.world.physics.ship_body(p.vehicle.0).entity == id
            })
            || (!self.surface_forces.ground.contains_key(&id)
                && self.surface_forces.ground.len() >= MAX_GROUND_FORCES)
        {
            return false;
        }
        // Updating ownership never refreshes contact history. Replacing the
        // assembly clears it; an airborne new actor cannot inherit an old seat.
        if self
            .surface_forces
            .ground
            .get(&id)
            .is_some_and(|old| old.body != force.body || old.colliders != force.colliders)
        {
            self.surface_forces
                .present
                .remove(&SurfaceForceId::Ground(id));
        }
        self.surface_forces.ground.insert(id, force);
        true
    }

    pub fn unregister_ground_surface_force(&mut self, id: PhysicsId) {
        self.surface_forces.ground.remove(&id);
        self.surface_forces
            .present
            .remove(&SurfaceForceId::Ground(id));
    }

    pub fn surface_force_observations(&self) -> Vec<SurfaceForceObservation> {
        self.surface_forces.observations()
    }

    pub(super) fn update_surface_forces(&mut self, dt: Duration) {
        let mut samples =
            Vec::with_capacity(self.player_count() * 2 + self.surface_forces.ground.len());
        for (seat, pilot) in self.pilots.iter().enumerate() {
            if pilot.vitals.is_none_or(match_rules::PilotVitals::alive)
                && let Some(snapshot) = self.spaceling_snapshot(seat)
            {
                samples.push(ForceSample {
                    id: SurfaceForceId::Pilot(pilot.id.0),
                    allegiance: SurfaceAllegiance::Player(pilot.owner),
                    position: snapshot.motion.position,
                    support: snapshot
                        .support
                        .and_then(|s| physics::planet_surface_support_index(s.collider))
                        .filter(|&p| p < self.world.planets.len()),
                    // Foreign support (including detached material) ends the
                    // old association instead of borrowing its source planet.
                    retain_contact_gap: snapshot.support.is_none(),
                });
            }
            let vehicle = pilot.vehicle.0;
            let ship = &self.world.ships[vehicle];
            let id = SurfaceForceId::Ship(vehicle);
            if ship.dead
                || ship.life <= 0.0
                || ship.thrust > 0.0
                || samples.iter().any(|s| s.id == id)
            {
                continue;
            }
            let Some(owner) = PlayerId::from_index(ship.owner_id) else {
                continue;
            };
            let Some(motion) = self
                .world
                .physics
                .world
                .motion(self.world.physics.ship_body(vehicle))
            else {
                continue;
            };
            samples.push(ForceSample {
                id,
                allegiance: SurfaceAllegiance::Player(owner),
                position: motion.position,
                support: self.ship_support_planet(seat),
                // Ships need live landing contact. Takeoff and flight never
                // acquire the ground-unit grace interval or an orbital zone.
                retain_contact_gap: false,
            });
        }
        self.surface_forces
            .ground
            .retain(|_, force| self.world.physics.world.motion(force.body).is_some());
        for (&id, force) in &self.surface_forces.ground {
            let Some(allegiance) = force.allegiance else {
                continue;
            };
            let body = self
                .world
                .physics
                .world
                .motion(force.body)
                .expect("retained live body");
            let mut foreign_support = false;
            let support = force
                .colliders
                .iter()
                .flat_map(|&collider| self.world.physics.world.surface_contacts(collider))
                .filter(|c| c.separation <= 0.04 && c.collider.entity != id)
                .filter_map(|c| {
                    let Some(planet) = physics::planet_surface_support_index(c.collider)
                        .filter(|&p| p < self.world.planets.len())
                    else {
                        foreign_support = true;
                        return None;
                    };
                    let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
                    let up = (body.position - frame.position).normalized();
                    (c.normal.dot(up) > 0.2).then_some(planet)
                })
                .min();
            samples.push(ForceSample {
                id: SurfaceForceId::Ground(id),
                allegiance,
                position: body.position,
                support,
                retain_contact_gap: !foreign_support,
            });
        }
        let frames: Vec<_> = (0..self.world.planets.len())
            .map(|p| motion::SurfaceFrame::read(&self.world.physics, p))
            .collect();
        self.surface_forces.update(&samples, &frames, dt);
    }
}

#[cfg(test)]
mod tests;
