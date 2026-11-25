//! Tank armor and the shells that charge it (#331).
//!
//! A tank's armor is one material region that the duel keeps, and its health
//! is that region's integrity. A shell is a weapon source with two parts: a
//! kinetic part, its mass at its launch speed, and an explosive payload. A
//! shell that strikes a tank charges that tank's armor once, with the impact
//! measured from the shell's velocity and the tank's motion. Whatever else it
//! strikes, such as terrain, is outside the material model, and the hit's
//! energy is reported rather than charged. The shell then bursts just off the
//! struck surface, outside every tank, and the burst divides the payload among
//! the tanks within reach by the directions each fills, so the shares never
//! sum past the payload. Terrain, loose dirt and fragments neither take shares
//! nor shield the tanks, as before.
//!
//! A host can put its own recipients in the shells' way, through the
//! engine's [`Recipients`]: tissue that a shell passes on its way and that a
//! burst reaches, which the host keeps and charges, and bodies that stop a
//! burst's energy, which is reported rather than charged. Regions bound to the
//! world's loading response stop energy the same way; the duel does not charge
//! them.
//!
//! Lengths, masses and energies here are Scorched's own units, whatever the
//! placement's scale, so a hosted duel takes the same damage as a standalone
//! one. The duel sees the host's recipients at its own scale too: their
//! lengths divide by the scale, and a region keeps its material and the share
//! of its capacity it has left. Each then loses the share of its capacity that
//! the same delivery would cost it at scale 1.
use super::*;
use crate::host::Placement;
use engine_material::{
    ARMOR, ContactGeometry, Face, Impact, MAX_LENGTH, MIN_LENGTH, Projectile, SHELL, Section,
    Share, Source, SourceId, SourceSpec, Target, blast_reach, patch_width_from_radii,
    sweep_crossings,
};
use engine_rapier::world::RayHit;
use engine_rapier::world::weapon::{Reachable, Recipient, Recipients, projectile_impact};
use std::collections::{BTreeMap, BTreeSet};

/// Represented depth of a tank into the screen, and of a burst's front, so a
/// tank stops the whole front wherever it lies across it.
const TANK_THICKNESS: f32 = 2.0;
/// Depth of armor plate behind a struck face.
const PLATE_DEPTH: f32 = 0.2;
/// The armor's material quantity: plate `PLATE_DEPTH` deep around the rigid
/// hull's 9.36 perimeter, `TANK_THICKNESS` into the screen. Both running
/// gears carry the same armor.
const ARMOR_VOLUME: f32 = 9.36 * PLATE_DEPTH * TANK_THICKNESS;
const ARMOR_SECTION: Section = Section::new(TANK_THICKNESS, PLATE_DEPTH);
/// A shell's radius, for its contact patch and where it bursts on a surface.
const SHELL_RADIUS: f32 = 0.25;
/// A shell's mass, so a harder shot hits harder.
const SHELL_MASS: f32 = 7.0;
/// The shell's casing, the part of its mass that meets what it strikes.
const CASING_VOLUME: f32 = 0.05;
const CASING: Section = Section::new(0.5, 0.1);
/// Explosive energy of one shell.
const PAYLOAD: f32 = 28_000.0;
/// How far a burst reaches to a tank's surface.
pub(crate) const BLAST_REACH: f32 = 5.0;

/// Circles standing in for the rigid hull, in its body's frame. Like the
/// chassis's, they lie within the collider, so a shell bursting off the hull
/// is outside them.
const HULL: [(Vec2, f32); 3] = [
    (Vec2::new(-1.1, -0.1), 0.7),
    (Vec2::new(0.0, -0.1), 0.7),
    (Vec2::new(1.1, -0.1), 0.7),
];
/// Circles standing in for the sprung chassis, in its body's frame. The wheels
/// are circles of their own.
const CHASSIS: [(Vec2, f32); 3] = [
    (Vec2::new(-1.15, 0.0), 0.45),
    (Vec2::new(0.0, 0.0), 0.45),
    (Vec2::new(1.15, 0.0), 0.45),
];

/// A fresh tank's armor.
pub(crate) fn armor() -> Region {
    Region::new(ARMOR, ARMOR_VOLUME).expect("the armor's volume is in the law's range")
}

/// A shell's source: its mass leaving the barrel at `speed`, and its payload.
/// `owner` is the firing tank's entity, which the source keeps after the tank
/// is gone. A speed the law refuses, such as a non-finite one, leaves the
/// payload alone.
pub(crate) fn shell(id: SourceId, owner: Option<u64>, speed: f32) -> Source {
    let casing = Region::new(SHELL, CASING_VOLUME).expect("the casing's volume is in range");
    Source::new(SourceSpec {
        id,
        owner,
        projectile: Some(Projectile {
            region: casing,
            mass: SHELL_MASS,
            speed,
        }),
        payload: Some(PAYLOAD),
        beam: None,
    })
    .unwrap_or_else(|_| charge(id, owner))
}

/// A payload without a shell, such as a scripted burst.
pub(crate) fn charge(id: SourceId, owner: Option<u64>) -> Source {
    Source::new(SourceSpec {
        id,
        owner,
        projectile: None,
        payload: Some(PAYLOAD),
        beam: None,
    })
    .expect("the payload is finite")
}

impl Tank {
    /// Charges the armor with `work` and returns what it took. Health follows
    /// the armor's integrity until a scenario rule has ended the tank.
    pub(crate) fn absorb(&mut self, work: f32) -> f32 {
        let left = self.armor.absorb(work).unwrap_or(work);
        if self.health > 0.0 {
            self.health = self.armor.normalized_health();
        }
        work - left
    }
}

/// Moves a shell's burst `center` out of any target circle that holds it, to
/// just past that circle's edge. A shell's ray can pass closer to a wheel than
/// the shell's radius allows, so a burst off the ground beside a wheel can
/// land inside it, where it would meet the tank in every direction. A scripted
/// burst inside a tank stays there.
fn outside(targets: &[Target], mut center: Vec2) -> Vec2 {
    for target in targets {
        let offset = center - target.center;
        let distance = offset.length();
        if distance <= target.radius {
            let direction = if distance > 0.0 {
                offset / distance
            } else {
                Vec2::Y
            };
            center = target.center + direction * target.radius * 1.001;
        }
    }
    center
}

/// What one shell's hit and burst did to the tanks' armor.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Struck {
    /// Work each tank's armor absorbed, by slot.
    pub absorbed: [f32; 2],
    /// Energy stopped by bodies outside the material model: the hard hit's,
    /// on what the shell struck, and the burst's shares on the host's bodies.
    pub unmodeled: f32,
}

/// The host has no recipients of its own: standalone Scorched, and hosts that
/// supply none.
pub(crate) struct NoRecipients;

impl Recipients for NoRecipients {
    fn near(&self, _center: Vec2, _radius: f32) -> Vec<Reachable> {
        Vec::new()
    }

    fn region(&self, _key: u64) -> Option<(Region, Section)> {
        None
    }

    fn absorb(&mut self, _key: u64, _work: f32) {}
}

/// The keys a source knows the host's recipients by, after the tanks' slots,
/// in the order it first meets them, so it reaches each once.
#[derive(Clone, Default)]
pub(crate) struct Keys(BTreeMap<Recipient, u64>);

impl Keys {
    fn of(&mut self, recipient: Recipient) -> u64 {
        let next = 2 + self.0.len() as u64;
        *self.0.entry(recipient).or_insert(next)
    }
}

/// The host's recipients with a circle within `radius` of `center`, at the
/// duel's scale and keyed by `keys`. A recipient with a circle the reach
/// geometry would refuse is left out whole.
fn guests(
    placement: &Placement,
    recipients: &impl Recipients,
    keys: &mut Keys,
    center: Vec2,
    radius: f32,
) -> Vec<(Recipient, Target)> {
    let scale = placement.scale;
    let length = |length: f32| {
        let length = length / scale;
        (length.is_finite() && (MIN_LENGTH..=MAX_LENGTH).contains(&length)).then_some(length)
    };
    let circles = recipients.near(placement.point_to_world(center), radius * scale);
    let refused: BTreeSet<Recipient> = circles
        .iter()
        .filter(|circle| {
            length(circle.radius).is_none()
                || length(circle.thickness).is_none()
                || !(circle.center.x.is_finite() && circle.center.y.is_finite())
        })
        .map(|circle| circle.recipient)
        .collect();
    circles
        .into_iter()
        .filter(|circle| !refused.contains(&circle.recipient))
        .map(|circle| {
            let target = Target {
                key: keys.of(circle.recipient),
                center: placement.point_from_world(circle.center),
                radius: circle.radius / scale,
                thickness: circle.thickness / scale,
            };
            (circle.recipient, target)
        })
        .collect()
}

/// A host's tissue as the duel sees it: its material at the duel's `scale`
/// with the same share of its capacity left, its section at that scale, and
/// the factor that takes work charged to it back to the tissue. Tissue the
/// law would refuse at that scale has none, and stops energy as a body does,
/// so it cannot spoil a delivery to the tanks.
fn proxy(recipients: &impl Recipients, key: u64, scale: f32) -> Option<(Region, Section, f64)> {
    let (region, section) = recipients.region(key)?;
    let volume = region.volume() / scale.powi(3);
    let capacity = f64::from(Region::new(region.material(), volume).ok()?.capacity());
    let share = region.remaining_precise() / f64::from(region.capacity());
    let proxy = Region::with_remaining(
        region.material(),
        volume,
        (share * capacity).clamp(0.0, capacity),
    )
    .ok()?;
    let section = Section::new(section.thickness / scale, section.depth / scale);
    section.validate().ok()?;
    Some((proxy, section, f64::from(region.capacity()) / capacity))
}

/// Passes the host's tissue that `shell` crosses on its way to `end`, in
/// order, each once. The shell flies on as it would have without it.
pub(super) fn pass_tissue(
    placement: &Placement,
    recipients: &mut impl Recipients,
    shell: &mut Shell,
    end: Vec2,
) {
    let start = shell.position;
    let tissue: Vec<(Recipient, Target)> = guests(
        placement,
        recipients,
        &mut shell.keys,
        start.midpoint(end),
        start.distance_to(end) * 0.5 + SHELL_RADIUS,
    )
    .into_iter()
    .filter(|(recipient, _)| matches!(recipient, Recipient::Tissue(_)))
    .collect();
    if tissue.is_empty() {
        return;
    }
    let targets: Vec<Target> = tissue.iter().map(|(_, target)| *target).collect();
    let Ok(crossings) = sweep_crossings(start, end, SHELL_RADIUS, &targets) else {
        return;
    };
    let speed = shell.velocity.length();
    for crossing in crossings {
        let Some((Recipient::Tissue(key), target)) =
            tissue.iter().find(|(_, target)| target.key == crossing.key)
        else {
            continue;
        };
        let Some((region, section, back)) = proxy(recipients, *key, placement.scale) else {
            continue;
        };
        let face = Face::new(2.0 * SHELL_RADIUS.min(target.radius), section);
        if let Ok(outcome) = shell.source.pass(crossing.key, &region, face, speed) {
            recipients.absorb(*key, (f64::from(outcome.consumed[0]) * back) as f32);
        }
    }
}

impl Duel {
    /// The circles of each tank still in the duel, in Scorched's units, keyed
    /// by its slot.
    fn armor_targets(&self, physics: &PhysicsWorld) -> Vec<Target> {
        let mut targets = Vec::new();
        for (slot, tank) in self.tanks.iter().enumerate() {
            if self.retired[slot].is_some() {
                continue;
            }
            let Some(motion) = physics.motion(tank.body) else {
                continue;
            };
            let position = self.placement.point_from_world(motion.position);
            let circles = if self.running_gear[slot].is_some() {
                CHASSIS
            } else {
                HULL
            };
            let target = |center, radius| Target {
                key: slot as u64,
                center,
                radius,
                thickness: TANK_THICKNESS,
            };
            for (offset, radius) in circles {
                targets.push(target(
                    position + offset.rotate_radians(motion.angle),
                    radius,
                ));
            }
            for wheel in self.running_gear[slot]
                .iter()
                .flat_map(|gear| gear.bodies().into_iter().skip(1))
            {
                if let Some(wheel) = physics.motion(wheel) {
                    targets.push(target(
                        self.placement.point_from_world(wheel.position),
                        self.config.wheel_tuning.radius,
                    ));
                }
            }
        }
        targets
    }

    /// Releases `source`'s payload from `center` into the armor of the tanks
    /// and the host's tissue within reach. The host's bodies stop the energy
    /// that meets them first. Returns what the tanks' armor took and what the
    /// host's bodies stopped.
    pub(super) fn release_payload(
        &mut self,
        physics: &PhysicsWorld,
        recipients: &mut impl Recipients,
        center: Vec2,
        source: &mut Source,
        keys: &mut Keys,
    ) -> Struck {
        let mut released = Struck::default();
        let guests = guests(&self.placement, recipients, keys, center, BLAST_REACH);
        let mut targets = self.armor_targets(physics);
        targets.extend(guests.iter().map(|(_, target)| *target));
        let Ok(reaches) = blast_reach(center, BLAST_REACH, TANK_THICKNESS, &targets) else {
            return released;
        };
        // The host's tissue that can take a share, as the duel sees it, and
        // the directions its other bodies fill.
        let mut tissue = Vec::new();
        let mut blocked = 0.0;
        for reach in reaches.iter().filter(|reach| reach.key >= 2) {
            let recipient = guests
                .iter()
                .find(|(_, target)| target.key == reach.key)
                .map(|(recipient, _)| *recipient);
            let proxy = match recipient {
                Some(Recipient::Tissue(key)) => proxy(recipients, key, self.placement.scale)
                    .map(|(region, section, back)| (key, region, section, back)),
                _ => None,
            };
            match proxy {
                Some(proxy) => tissue.push((reach, proxy)),
                None => blocked += reach.coupling,
            }
        }
        let tanks = reaches.iter().filter(|reach| reach.key < 2);
        let mut shares: Vec<Share> = tanks
            .clone()
            .map(|reach| Share {
                key: reach.key,
                region: &self.tanks[reach.key as usize].armor,
                face: Face::new(reach.width, ARMOR_SECTION),
                coupling: reach.coupling,
            })
            .collect();
        shares.extend(tissue.iter().map(|(reach, (_, region, section, _))| Share {
            key: reach.key,
            region,
            face: Face::new(reach.width, *section),
            coupling: reach.coupling,
        }));
        let Ok(outcomes) = source.release(&shares) else {
            return released;
        };
        released.unmodeled = source.ledger().payload.budget as f32 * blocked;
        let mut outcomes = outcomes.into_iter();
        for (reach, outcome) in tanks.zip(outcomes.by_ref()) {
            let slot = reach.key as usize;
            released.absorbed[slot] += self.tanks[slot].absorb(outcome.consumed[0]);
        }
        for ((_, (key, _, _, back)), outcome) in tissue.iter().zip(outcomes) {
            recipients.absorb(*key, (f64::from(outcome.consumed[0]) * back) as f32);
        }
        released
    }

    /// The impact of a shell moving at `velocity` on what `hit` found, in
    /// Scorched's units: from the shell's velocity entering the tick and the
    /// struck body's motion where the ray found it.
    fn measure(&self, physics: &PhysicsWorld, velocity: Vec2, hit: &RayHit) -> Option<Impact> {
        let scale = self.placement.scale;
        let load = projectile_impact(
            physics,
            SHELL_MASS * scale * scale,
            velocity * scale,
            hit.collider,
            None,
            -hit.normal,
            hit.point,
        )?;
        Some(Impact {
            closing_speed: load.closing_speed / scale,
            effective_mass: load.effective_mass / (scale * scale),
        })
    }

    /// A shell's hard hit and burst. A tank it struck takes the hit in its
    /// armor once; anything else stops the hit's energy outside the material
    /// model. The burst leaves its crater at `point`, the ray's hit in
    /// Scorched's units, and releases the payload a shell's radius off the
    /// struck surface, outside every tank.
    pub(super) fn strike(
        &mut self,
        physics: &mut PhysicsWorld,
        recipients: &mut impl Recipients,
        shell: &mut Shell,
        point: Vec2,
        hit: &RayHit,
    ) -> Struck {
        let mut struck = Struck::default();
        let slot = (0..2).find(|&slot| {
            self.retired[slot].is_none() && hit.collider.entity == self.tanks[slot].body.entity
        });
        if let Some(load) = self.measure(physics, shell.velocity, hit) {
            match slot {
                Some(slot) => {
                    let outcome = patch_width_from_radii(SHELL_RADIUS, None)
                        .ok()
                        .map(|width| ContactGeometry::new(width, CASING, ARMOR_SECTION))
                        .and_then(|contact| {
                            shell
                                .source
                                .hit(slot as u64, &self.tanks[slot].armor, contact, load)
                                .ok()
                        });
                    if let Some(outcome) = outcome {
                        struck.absorbed[slot] += self.tanks[slot].absorb(outcome.consumed[1]);
                    }
                }
                None => struck.unmodeled = load.energy(),
            }
        }
        let center = outside(
            &self.armor_targets(physics),
            point + hit.normal * SHELL_RADIUS,
        );
        let released = self.burst(
            physics,
            recipients,
            point,
            center,
            &mut shell.source,
            &mut shell.keys,
        );
        for (absorbed, released) in struck.absorbed.iter_mut().zip(released.absorbed) {
            *absorbed += released;
        }
        struck.unmodeled += released.unmodeled;
        struck
    }

    /// Tells each bot what `owner`'s shot cost its tank since `before`.
    pub(super) fn notify_hits(&mut self, owner: Option<usize>, before: [f32; 2]) {
        for (player, before) in before.into_iter().enumerate() {
            let lost = before - self.tanks[player].health;
            if owner.is_some_and(|owner| owner != player) && lost > 0.0 {
                self.bots[player].hit(self.tick, lost);
            }
        }
    }
}

pub(crate) mod contacts;
#[cfg(test)]
mod tests;
