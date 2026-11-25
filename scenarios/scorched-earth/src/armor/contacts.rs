//! Tank armor and the contacts that charge it (#331).
//!
//! The engine's contact charges price each new impact between two bound
//! surfaces after the step, from the motion the bodies brought into it. The
//! duel binds its tanks and its terrain, and keeps their surfaces:
//!
//! - A tank's hull and wheels are its armor, plate `PLATE_DEPTH` deep. A hull
//!   meets others with a corner of radius `HULL_RADIUS`, and a wheel with its
//!   own radius.
//! - Terrain, attached or fallen, is rock as a half-space, which no impact
//!   wears.
//!
//! Each impact charges the armor of the tanks it involves with the work the
//! law consumed there, and the step lists it in the duel's contacts. Rock
//! yields at half the armor's stress, so the ground takes the work of
//! settling, driving and a landing on a hull's face or on a sprung tank's
//! wheels. What costs a tank is a surface as strong as its own, such as the
//! other tank's, or a blow on one corner with its weight behind it. Pricing
//! moves nothing, so Rapier still decides the bounce.
//!
//! Two things are not priced:
//!
//! - Supports, the steady loads of resting and driving.
//! - Loose dirt, which has no region. A grain's mass is 0.25, and as rock it
//!   would have to strike a hull at 56 u/s to cost it anything, and a burst
//!   throws dirt at 16 u/s at most. With every grain bound, capture added
//!   about 15% to a duel's step, against about 3% without.
//!
//! A world prices contacts once [`capture_contacts`] has turned capture on in
//! it. A host that doesn't call it has a duel whose contacts cost nothing.
//!
//! A host declares the surfaces of its own bodies through the engine's
//! [`ContactSurfaces`], for the sources it has bound. The duel makes the
//! step's one call to the charges, so it prices the host's impacts too.
//!
//! The world's lengths are the placement's scale times Scorched's. The duel's
//! bodies keep their densities there, so their masses grow with the square of
//! the scale, their speeds with it and their energies with its fourth power.
//! A material whose stiffness, strength and toughness grow with the scale
//! then loses the same share of its capacity to the same contact at every
//! scale. The duel's surfaces are such materials, and it sees the host's
//! surfaces the same way, as it does the host's recipients: each loses the
//! share of its capacity that the same contact would cost it at scale 1.
use super::*;
use crate::host::{Contact, ContactDiagnostics, ContactTimes};
use engine_material::ROCK;
use engine_rapier::world::loading::charges::{
    ContactCharge, ContactSurface, ContactSurfaces, Depth,
};
use engine_rapier::world::loading::{
    CaptureConfig, ContactSource, EpisodeTransition, RegionId, RegionToken,
};

/// The radius of curvature a hull meets others with at a corner, about that
/// of the rigid hull's chamfered ends, which are 0.4 by 0.5. An estimate, and
/// what a blow on one corner costs follows it. The corner's patch on flat
/// rock is half as wide, and the energy the rock takes before the plate
/// yields grows with the square of that width: 390 here, the rigid tank's
/// whole mass at 4.5 u/s.
const HULL_RADIUS: f32 = 0.5;
/// The rock behind the terrain's surface: the most a region holds, so that no
/// impact uses it up.
const UNWORN_VOLUME: f32 = 1.0e12;
/// How many retired regions' tokens to keep before forgetting them.
const PRUNE_SLACK: usize = 64;

/// What stands behind one of the duel's regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// A tank's hull, by slot: the rigid hull or the sprung chassis.
    Hull(usize),
    /// A sprung tank's wheel, by the tank's slot.
    Wheel(usize),
    /// Terrain, attached or fallen.
    Ground,
}

/// The regions contact charges keep for a duel.
#[derive(Debug, Clone, Default)]
pub(crate) struct Contacts {
    /// Each bound body's region. A removed body's token stays until a prune.
    regions: BTreeMap<BodyId, RegionToken>,
    parts: BTreeMap<RegionToken, Part>,
    /// How many tokens to hold before forgetting the retired ones.
    prune_at: usize,
    pub(crate) diagnostics: ContactDiagnostics,
    pub(crate) times: ContactTimes,
}

impl Contacts {
    /// Binds `collider` to the region of `body`, its parent, making the
    /// region if the body has none. A sprung tank's bodies have a region
    /// each.
    fn bind(&mut self, world: &mut PhysicsWorld, collider: ColliderId, body: BodyId, part: Part) {
        let live = self
            .regions
            .get(&body)
            .copied()
            .filter(|&token| world.loading_region_is_live(token));
        let token = match live {
            Some(token) => token,
            None => {
                let id = RegionId {
                    entity: body.entity,
                    part: body.role.value(),
                };
                let Ok(token) = world.create_loading_region(id, body) else {
                    self.diagnostics.refused_bindings += 1;
                    return;
                };
                self.regions.insert(body, token);
                self.parts.insert(token, part);
                token
            }
        };
        let source = ContactSource {
            region: token,
            patch: 0,
        };
        if world.bind_loading_collider(collider, source).is_err() {
            self.diagnostics.refused_bindings += 1;
        }
    }

    /// Forgets the regions the world retired with their bodies, once enough
    /// have gathered.
    fn prune(&mut self, world: &PhysicsWorld) {
        if self.regions.len() > self.prune_at {
            self.regions
                .retain(|_, token| world.loading_region_is_live(*token));
            self.parts
                .retain(|token, _| world.loading_region_is_live(*token));
            self.prune_at = 2 * self.regions.len() + PRUNE_SLACK;
        }
    }
}

/// Capture's bounds for a world that holds a duel: its defaults, with room
/// for several times the bindings and the contacts of the busiest duels
/// measured. Capture visits loose dirt's contacts too, though it binds none.
fn capture() -> CaptureConfig {
    CaptureConfig {
        max_bindings: 8_192,
        max_episodes: 4_096,
        max_visits_per_batch: 65_536,
        max_contacts_per_batch: 8_192,
        max_records_per_step: 32_768,
        ..CaptureConfig::default()
    }
}

/// Turns on what a duel in `world` needs to price its tanks' contacts:
/// contact capture, with room for a duel unless the world already has its
/// own, and speculative arrivals. Neither changes the physics. Returns
/// whether the world now prices contacts. One with the engine's native
/// material response can't.
pub fn capture_contacts(world: &mut PhysicsWorld) -> bool {
    let _ = world.enable_loading_capture(capture());
    let _ = world.enable_speculative_arrivals();
    world.admits_speculative_arrivals()
}

/// `material` as a world `scale` times Scorched's size holds it.
fn scaled(material: engine_material::Material, scale: f32) -> engine_material::Material {
    engine_material::Material {
        density: material.density / scale,
        stiffness: material.stiffness * scale,
        strength: material.strength * scale,
        toughness: material.toughness * scale,
        ..material
    }
}

/// `region` made of `material`, `volume` of it, with the same share of its
/// capacity left, and the factor that takes work charged to it back to
/// `region`. There is none if the law would refuse the material or the
/// volume.
fn seen(
    region: &Region,
    material: engine_material::Material,
    volume: f32,
) -> Option<(Region, f64)> {
    let capacity = f64::from(Region::new(material, volume).ok()?.capacity());
    let share = region.remaining_precise() / f64::from(region.capacity());
    let seen =
        Region::with_remaining(material, volume, (share * capacity).clamp(0.0, capacity)).ok()?;
    Some((seen, f64::from(region.capacity()) / capacity))
}

/// A tank's armor as a world `scale` times Scorched's size prices it.
fn armor_at(armor: &Region, scale: f32) -> Option<(Region, f64)> {
    if scale == 1.0 {
        return Some((*armor, 1.0));
    }
    seen(armor, scaled(ARMOR, scale), ARMOR_VOLUME * scale.powi(3))
}

/// A host's region as the duel sees it at `scale`: its material at that
/// scale, in the volume the host gave.
fn guest_at(region: &Region, scale: f32) -> Option<(Region, f64)> {
    if scale == 1.0 {
        return Some((*region, 1.0));
    }
    seen(region, scaled(region.material(), scale), region.volume())
}

/// What `work` charged to `seen`, a region as the world prices it and the
/// factor back from it, comes to in `region`'s own units. A charge that takes
/// all the priced region had left takes all that `region` has left, so that
/// rounding on the way back never leaves a sliver of a failed region standing.
fn charged_back(region: &Region, (seen, back): (Region, f64), work: f32) -> f32 {
    if work >= seen.remaining() {
        region.remaining()
    } else {
        (f64::from(work) * back) as f32
    }
}

/// The host keeps no surfaces of its own: standalone Scorched, and hosts that
/// declare none.
pub(crate) struct NoSurfaces;

impl ContactSurfaces for NoSurfaces {
    fn surface(&self, _source: ContactSource) -> Option<ContactSurface> {
        None
    }

    fn absorb(&mut self, _source: ContactSource, _work: f32) {}
}

/// The surfaces behind the world's sources: the duel's own, and the host's as
/// the duel sees them.
struct Surfaces<'a, H> {
    parts: &'a BTreeMap<RegionToken, Part>,
    tanks: &'a mut [Tank; 2],
    wheel_radius: f32,
    scale: f32,
    host: &'a mut H,
}

impl<H: ContactSurfaces> ContactSurfaces for Surfaces<'_, H> {
    fn surface(&self, source: ContactSource) -> Option<ContactSurface> {
        let scale = self.scale;
        let Some(&part) = self.parts.get(&source.region) else {
            let surface = self.host.surface(source)?;
            let (region, _) = guest_at(&surface.region, scale)?;
            return Some(ContactSurface { region, ..surface });
        };
        let (slot, radius) = match part {
            Part::Hull(slot) => (slot, HULL_RADIUS),
            Part::Wheel(slot) => (slot, self.wheel_radius),
            Part::Ground => {
                return Some(ContactSurface {
                    region: Region::new(scaled(ROCK, scale), UNWORN_VOLUME).ok()?,
                    thickness: TANK_THICKNESS * scale,
                    depth: Depth::Patch(None),
                    radius: None,
                });
            }
        };
        let (region, _) = armor_at(&self.tanks[slot].armor, scale)?;
        Some(ContactSurface {
            region,
            thickness: ARMOR_SECTION.thickness * scale,
            depth: Depth::Fixed(ARMOR_SECTION.depth * scale),
            radius: Some(radius * scale),
        })
    }

    fn absorb(&mut self, source: ContactSource, work: f32) {
        let scale = self.scale;
        match self.parts.get(&source.region) {
            Some(&(Part::Hull(slot) | Part::Wheel(slot))) => {
                let armor = &self.tanks[slot].armor;
                if let Some(seen) = armor_at(armor, scale) {
                    let work = charged_back(armor, seen, work);
                    self.tanks[slot].absorb(work);
                }
            }
            Some(Part::Ground) => {}
            None => {
                let Some(surface) = self.host.surface(source) else {
                    return;
                };
                if let Some(seen) = guest_at(&surface.region, scale) {
                    self.host
                        .absorb(source, charged_back(&surface.region, seen, work));
                }
            }
        }
    }

    fn protected(&self, source: ContactSource) -> bool {
        match self.parts.get(&source.region) {
            Some(Part::Hull(_) | Part::Wheel(_)) => false,
            Some(Part::Ground) => true,
            None => self.host.protected(source),
        }
    }
}

impl Duel {
    /// Binds each collider of a tank or of the terrain that has no region
    /// yet: a new body's, or a body's new colliders, as dug terrain's are.
    /// Runs before each step. The host binds its own.
    pub(crate) fn bind_contacts(&mut self, world: &mut PhysicsWorld) {
        if !world.admits_speculative_arrivals() {
            return;
        }
        let part = |collider: ColliderId| {
            let slot = (0..2).find(|&slot| {
                self.retired[slot].is_none() && collider.entity == self.tanks[slot].body.entity
            });
            if let Some(slot) = slot {
                let hull = world.collider_body(collider)? == self.tanks[slot].body;
                return Some(if hull {
                    Part::Hull(slot)
                } else {
                    Part::Wheel(slot)
                });
            }
            self.terrain
                .iter()
                .any(|fragment| fragment.assembly.body().entity == collider.entity)
                .then_some(Part::Ground)
        };
        let unbound: Vec<(ColliderId, BodyId, Part)> = world
            .collider_ids()
            .filter_map(|collider| Some((collider, part(collider)?)))
            .filter(|&(collider, _)| world.loading_binding(collider).is_none())
            .filter_map(|(collider, part)| Some((collider, world.collider_body(collider)?, part)))
            .collect();
        for (collider, body, part) in unbound {
            self.contacts.bind(world, collider, body, part);
        }
        self.contacts.prune(world);
    }

    /// Prices the step's new impacts, charges each tank's armor with what
    /// its own consumed and lists them in the duel's contacts. Runs after
    /// each step, before the step's shells land, so each impact is priced on
    /// the armor its tank brought into the step, less what the step's
    /// earlier impacts consumed.
    pub(crate) fn charge_contacts(
        &mut self,
        world: &mut PhysicsWorld,
        host: &mut impl ContactSurfaces,
    ) {
        if !world.admits_speculative_arrivals() {
            return;
        }
        let scale = self.placement.scale;
        let Self {
            contacts,
            contact_events,
            tanks,
            config,
            ..
        } = self;
        contacts.diagnostics.rejected_batches += world.loading_diagnostics().rejected_batches;
        // Most steps start no impact, and have nothing to price.
        let impacts = world
            .loading_batches()
            .iter()
            .flat_map(|batch| &batch.events)
            .any(|event| event.transition == EpisodeTransition::Impact);
        if !impacts {
            return;
        }
        let mut surfaces = Surfaces {
            parts: &contacts.parts,
            tanks,
            wheel_radius: config.wheel_tuning.radius,
            scale,
            host,
        };
        let Ok(charges) = world.charge_contact_impacts(&mut surfaces) else {
            return;
        };
        let slot = |source: ContactSource| match contacts.parts.get(&source.region) {
            Some(&(Part::Hull(slot) | Part::Wheel(slot))) => Some(slot),
            _ => None,
        };
        // Work and energy in Scorched's units.
        let back = f64::from(scale).powi(-4);
        for charge in charges {
            let (ContactCharge::Charged {
                sources,
                closing_speed,
                ..
            }
            | ContactCharge::Unpriced {
                sources,
                closing_speed,
                ..
            }) = &charge;
            let (sources, closing_speed) = (*sources, *closing_speed);
            let slots = sources.map(slot);
            if slots == [None, None] {
                continue;
            }
            let mut contact = Contact {
                entities: sources.map(|source| source.region.id.entity),
                tanks: slots,
                closing_speed,
                energy: 0.0,
                absorbed: [0.0; 2],
                unpriced: None,
            };
            match charge {
                ContactCharge::Charged { outcome, .. } => {
                    contact.energy = (f64::from(outcome.ledger.available) * back) as f32;
                    contact.absorbed = [0, 1].map(|side| {
                        slots[side]
                            .map_or(0.0, |_| (f64::from(outcome.consumed[side]) * back) as f32)
                    });
                }
                ContactCharge::Unpriced { energy, reason, .. } => {
                    contact.energy = energy.map_or(0.0, |energy| (f64::from(energy) * back) as f32);
                    contact.unpriced = Some(reason);
                    contacts.diagnostics.unpriced += 1;
                }
            }
            contact_events.push(contact);
        }
    }
}

#[cfg(test)]
mod tests;
