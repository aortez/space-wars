//! Weapon sources in a physics world. A shot is a real body. Each step sweeps
//! it through the tissue it passed, and the step's contacts stop it on the
//! body it hit. A beam is traced to the surface that stops it. An
//! [`engine_material::Source`] charges every recipient, and the reach geometry
//! divides blasts and beams.
//!
//! Tissue belongs to the host, which keeps it and charges it through
//! [`Recipients`]. A body is charged through the material registered for it
//! with the world's loading response, or through the region the host keeps
//! for it. A host keeps its own regions where the contact response cannot
//! run, and decides when its policy shields a recipient. A shot's body is not
//! bound to a loading region, so the contact response refuses the batches it
//! arrives in, as it refuses any unbound contact, and leaves the shot's hit to
//! its source.

use super::loading::RegionToken;
use super::{BodyId, BodyKind, BodyMotion, CollisionGroups, PhysicsId, PhysicsWorld};
use super::{ColliderId, RayCastOptions};
use engine_core::Vec2;
use engine_material::{
    ContactBody, ContactGeometry, Face, Impact, MAX_LENGTH, MIN_LENGTH, MaterialError, Outcome,
    Part, Projectile, Pulses, Region, Section, Share, Source, SourceError, SourceId, SourceLedger,
    SourceSpec, Target, beam_reach, blast_reach, closing_speed, effective_mass, lever,
    patch_width_from_radii, point_velocity, sweep_crossings,
};
use std::collections::{BTreeMap, BTreeSet};

/// Something a weapon's energy can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Recipient {
    /// Tissue that the host keys, keeps and charges, which no collider stands
    /// for, such as a blade of grass. Shots and beams pass through it.
    Tissue(u64),
    /// A body whose region the host keys, keeps and charges, such as a ship's
    /// hull or a pilot on foot. It stops shots and beams as any body does. The
    /// bodies of one entity that share a key, such as a rover's chassis and
    /// wheels, are one recipient.
    Body { key: u64, entity: PhysicsId },
    /// A region registered with the world's loading response, which the world
    /// charges.
    Region(RegionToken),
    /// A body outside the material model, such as terrain. It stops energy, and
    /// what it stops is reported rather than charged.
    Unmodeled(BodyId),
}

impl Recipient {
    fn entity(self) -> Option<PhysicsId> {
        match self {
            Recipient::Tissue(_) => None,
            Recipient::Body { entity, .. } => Some(entity),
            Recipient::Region(token) => Some(token.id.entity),
            Recipient::Unmodeled(body) => Some(body.entity),
        }
    }
}

/// One circle of a recipient, which a sweep or a front may meet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reachable {
    pub recipient: Recipient,
    pub center: Vec2,
    pub radius: f32,
    /// Represented thickness into the screen, `wu`. A recipient as thick as a
    /// front's height stops the front.
    pub thickness: f32,
}

/// The host's side of a delivery: what lies near a weapon, the regions the
/// host keeps, and its policy on who may be charged.
pub trait Recipients {
    /// The circles whose edges lie within `radius` of `center` that the host
    /// admits: the tissue a shot may pass, and the tissue and bodies a blast
    /// or a beam may meet. A shot passes only the tissue; the world's contacts
    /// find the body that stops it. A beam asks for its whole range, meets
    /// what lies across its path in the order it reaches it, and ends on the
    /// first solid surface its ray cast meets. That surface's recipient takes
    /// the part of the beam its own circles within the range cover, or all
    /// that reaches it if it has none, so a body's circles should cover its
    /// colliders. The host bounds the set. A recipient with a circle the
    /// reach geometry refuses is refused whole, but a beam first leaves out
    /// the other recipients' circles it cannot meet.
    fn near(&self, center: Vec2, radius: f32) -> Vec<Reachable>;
    /// The region the host keeps under `key`, for tissue or a body, with the
    /// capacity it has left, and its section.
    fn region(&self, key: u64) -> Option<(Region, Section)>;
    /// Charges the region the host keeps under `key` with the work a delivery
    /// consumed from it.
    fn absorb(&mut self, key: u64, work: f32);
    /// The key of the region the host keeps for `body`, if it keeps one. A
    /// body bound to a material registered with the world's loading response
    /// is charged there instead. Keys share one space with tissue's: a key
    /// names one region, kept for tissue or for bodies but not both. The
    /// bodies of one entity that share a key are one recipient.
    fn body(&self, _body: BodyId) -> Option<u64> {
        None
    }
    /// Whether the host's policy shields `recipient` from weapons for now, as
    /// it might shield a pilot just after ejection. A shielded recipient still
    /// stops what any recipient would stop, and the energy that meets it is
    /// reported as excluded rather than charged.
    fn protected(&self, _recipient: Recipient) -> bool {
        false
    }
}

/// A payload's energy, how far its blast reaches and the height of its front.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blast {
    pub energy: f32,
    pub radius: f32,
    pub height: f32,
}

/// A shot to launch. Its body must already be in the world, moving at its
/// launch velocity: the body's mass and speed are the shot's. The body must be
/// alone in its entity, with no collider bound to a loading region. After
/// launch, the shot owns the entity and removes it when it retires.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotSpec {
    pub id: SourceId,
    pub owner: Option<u64>,
    pub body: BodyId,
    /// The shot's own material, which its hard hit charges.
    pub region: Region,
    pub section: Section,
    /// The radius its sweeps and its hit use, `wu`.
    pub radius: f32,
    /// Seconds an unspent shot flies before it retires.
    pub lifetime: f32,
    pub blast: Option<Blast>,
    /// An entity the shot never charges, such as its firer.
    pub protected: Option<PhysicsId>,
}

/// A beam to arm: its pulses, the width and height of its cross-section, and
/// what stops it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeamSpec {
    pub id: SourceId,
    pub owner: Option<u64>,
    pub pulses: Pulses,
    pub width: f32,
    pub height: f32,
    /// The groups whose solid colliders stop the beam, as for a ray cast.
    pub collision_groups: CollisionGroups,
    /// An entity the beam passes through without charging, such as its firer.
    pub protected: Option<PhysicsId>,
}

/// One pulse of a beam to fire: from `start` along `direction`, as far as
/// `range`, which may be how far the beam has grown, carrying `energy`, at
/// most the beam's pulse energy, such as less for a beam that weakens with
/// range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pulse {
    pub start: Vec2,
    pub direction: Vec2,
    pub range: f32,
    pub energy: f32,
}

/// Why energy that met a recipient was not delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exclusion {
    /// The host's protection policy.
    Protected,
    /// The recipient is outside the material model.
    Unmodeled,
}

/// What a weapon did. Each event is reported once.
#[derive(Debug, Clone, PartialEq)]
pub enum WeaponEvent {
    /// Energy that reached a recipient, which has absorbed the outcome. The
    /// recipient is the outcome's slot 0, except in a shot's hard hit, where
    /// the shot is slot 0 and the recipient slot 1.
    Delivered {
        source: SourceId,
        part: Part,
        recipient: Recipient,
        outcome: Outcome,
    },
    /// Energy that met a recipient and was not delivered.
    Excluded {
        source: SourceId,
        part: Part,
        recipient: Recipient,
        reason: Exclusion,
        energy: f32,
    },
    /// A delivery the source refused, such as a second report of a recipient it
    /// already reached or a hit after its first.
    Refused {
        source: SourceId,
        part: Part,
        recipient: Recipient,
        error: SourceError,
    },
    /// A shot stopped against a body.
    Stopped {
        source: SourceId,
        body: BodyId,
        point: Vec2,
    },
    /// A shot's blast released at `center`, before its deliveries. The push it
    /// gives bodies is the host's physics.
    Burst { source: SourceId, center: Vec2 },
    /// A beam's pulse ran from `start` to `end`, where it met `body`, if any.
    Traced {
        source: SourceId,
        start: Vec2,
        end: Vec2,
        body: Option<BodyId>,
    },
    /// A source that is done. Its ledger is final.
    Retired {
        source: SourceId,
        owner: Option<u64>,
        ledger: SourceLedger,
    },
}

/// Why a weapon call was refused. A refused call changes nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WeaponError {
    /// Another source already has this id.
    Exists,
    /// No live source has this id.
    UnknownSource,
    /// The shot's body is missing, not dynamic, already a shot, not alone in
    /// its entity, or bound to a loading region.
    InvalidBody,
    /// The world does not collect contact events, so no shot could stop.
    NoContactEvents,
    Source(SourceError),
}

impl std::fmt::Display for WeaponError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exists => f.write_str("another source already has this id"),
            Self::UnknownSource => f.write_str("no live source has this id"),
            Self::InvalidBody => f.write_str("the body cannot carry a shot"),
            Self::NoContactEvents => f.write_str("the world does not collect contact events"),
            Self::Source(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for WeaponError {}

impl From<SourceError> for WeaponError {
    fn from(error: SourceError) -> Self {
        WeaponError::Source(error)
    }
}

impl From<MaterialError> for WeaponError {
    fn from(error: MaterialError) -> Self {
        WeaponError::Source(SourceError::Material(error))
    }
}

/// The motion of the bodies entering a step, which its hits are measured
/// from. [`Weapons::prepare`] makes one for each step, and [`Weapons::settle`]
/// uses it up.
#[derive(Debug)]
pub struct Prepared {
    motions: BTreeMap<BodyId, BodyMotion>,
}

/// The live weapon sources of one world. A clone carries on as its original
/// would, given a clone of the same world.
#[derive(Debug, Clone, Default)]
pub struct Weapons {
    shots: BTreeMap<SourceId, Shot>,
    beams: BTreeMap<SourceId, Beam>,
    events: Vec<WeaponEvent>,
}

#[derive(Debug, Clone)]
struct Shot {
    source: Source,
    body: BodyId,
    /// The body's motion at launch, for a step prepared before the launch.
    launched: Option<BodyMotion>,
    section: Section,
    radius: f32,
    lifetime: f32,
    blast: Option<Blast>,
    protected: Option<PhysicsId>,
    keys: Keys,
    /// The tissue it passed while the host shielded it, which it does not
    /// meet again.
    shielded: BTreeSet<Recipient>,
}

#[derive(Debug, Clone)]
struct Beam {
    source: Source,
    pulse: f32,
    width: f32,
    height: f32,
    collision_groups: CollisionGroups,
    protected: Option<PhysicsId>,
    keys: Keys,
}

/// A contact a shot's body made in a step: the other collider, the normal
/// from the shot into it, the contact point and the impulse it took.
struct Contact {
    other: ColliderId,
    normal: Vec2,
    point: Option<Vec2>,
    impulse: f32,
}

/// Each recipient's key in one source's record of what it reached.
#[derive(Debug, Clone, Default)]
struct Keys(BTreeMap<Recipient, u64>);

impl Keys {
    fn of(&mut self, recipient: Recipient) -> u64 {
        let next = self.0.len() as u64;
        *self.0.entry(recipient).or_insert(next)
    }
}

/// A front's recipients, keyed for the reach geometry by first appearance.
#[derive(Default)]
struct Front {
    recipients: Vec<Recipient>,
    /// Each recipient's largest circle.
    radii: Vec<f32>,
    targets: Vec<Target>,
}

impl Front {
    /// The front of the recipients whose circles the reach geometry accepts,
    /// and the recipients with a circle it refuses.
    fn new(reachable: Vec<Reachable>) -> (Self, Vec<Recipient>) {
        let target = |circle: &Reachable, key| Target {
            key,
            center: circle.center,
            radius: circle.radius,
            thickness: circle.thickness,
        };
        let mut invalid = Vec::new();
        for circle in &reachable {
            if target(circle, 0).validate().is_err() && !invalid.contains(&circle.recipient) {
                invalid.push(circle.recipient);
            }
        }
        let mut front = Front::default();
        let mut keys = BTreeMap::new();
        for circle in reachable
            .iter()
            .filter(|circle| !invalid.contains(&circle.recipient))
        {
            let key = *keys.entry(circle.recipient).or_insert_with(|| {
                front.recipients.push(circle.recipient);
                front.radii.push(0.0);
                front.recipients.len() - 1
            });
            front.radii[key] = front.radii[key].max(circle.radius);
            front.targets.push(target(circle, key as u64));
        }
        (front, invalid)
    }
}

/// One recipient's share of a release or a pulse, before the source sees it.
struct Entry {
    recipient: Recipient,
    region: Region,
    face: Face,
    coupling: f32,
}

/// A length the material laws accept.
fn length_ok(value: f32) -> bool {
    value.is_finite() && (MIN_LENGTH..=MAX_LENGTH).contains(&value)
}

/// A finite, positive span, such as a lifetime, a range or a step.
fn positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

fn point_ok(point: Vec2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

/// A body's center of mass with `motion`'s pose.
fn center(world: &PhysicsWorld, body: BodyId, motion: BodyMotion) -> Option<Vec2> {
    let local = world.world_point_to_body(body, world.center_of_mass(body)?)?;
    Some(motion.position + local.rotate_radians(motion.angle))
}

impl Weapons {
    pub fn new() -> Self {
        Self::default()
    }

    /// A live shot's or beam's source.
    pub fn source(&self, id: SourceId) -> Option<&Source> {
        self.shots
            .get(&id)
            .map(|shot| &shot.source)
            .or_else(|| self.beams.get(&id).map(|beam| &beam.source))
    }

    /// The live shots and their bodies, in id order.
    pub fn shots(&self) -> impl Iterator<Item = (SourceId, BodyId)> + '_ {
        self.shots.iter().map(|(id, shot)| (*id, shot.body))
    }

    /// The events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<WeaponEvent> {
        std::mem::take(&mut self.events)
    }

    /// Takes over a shot's body, already moving at its launch velocity. The
    /// shot follows the body by its id, so a host may replace the body, and the
    /// shot flies on with the new one. Nothing checks the new body again: it
    /// must still be dynamic, alone in its entity and bound to no loading
    /// region. A hard hit takes the new body's mass, while passes through
    /// tissue keep the mass the shot launched with. A refused launch changes
    /// nothing: the id is taken, the world collects no contact events, the
    /// body cannot carry a shot, or the shot's geometry is outside what the
    /// material laws accept.
    pub fn launch(&mut self, world: &PhysicsWorld, spec: ShotSpec) -> Result<(), WeaponError> {
        if self.shots.contains_key(&spec.id) || self.beams.contains_key(&spec.id) {
            return Err(WeaponError::Exists);
        }
        if !world.collect_events {
            return Err(WeaponError::NoContactEvents);
        }
        if !self.can_carry(world, spec.body) {
            return Err(WeaponError::InvalidBody);
        }
        let mass = world.body_mass(spec.body).ok_or(WeaponError::InvalidBody)?;
        let motion = world.motion(spec.body).ok_or(WeaponError::InvalidBody)?;
        spec.section.validate()?;
        let blast_ok = spec
            .blast
            .is_none_or(|blast| length_ok(blast.radius) && length_ok(blast.height));
        if !(length_ok(spec.radius) && positive(spec.lifetime) && blast_ok) {
            return Err(MaterialError::InvalidGeometry.into());
        }
        let source = Source::new(SourceSpec {
            id: spec.id,
            owner: spec.owner,
            projectile: Some(Projectile {
                region: spec.region,
                mass,
                speed: motion.linear_velocity.length(),
            }),
            payload: spec.blast.map(|blast| blast.energy),
            beam: None,
        })?;
        self.shots.insert(
            spec.id,
            Shot {
                source,
                body: spec.body,
                launched: Some(motion),
                section: spec.section,
                radius: spec.radius,
                lifetime: spec.lifetime,
                blast: spec.blast,
                protected: spec.protected,
                keys: Keys::default(),
                shielded: BTreeSet::new(),
            },
        );
        Ok(())
    }

    /// Whether `body` can carry a shot: a dynamic body alone in its entity,
    /// with no collider bound to a loading region, that no shot owns.
    fn can_carry(&self, world: &PhysicsWorld, body: BodyId) -> bool {
        let Some(entity) = world.entities.get(&body.entity) else {
            return false;
        };
        world.body_kind(body) == Some(BodyKind::Dynamic)
            && entity.bodies == [body]
            && entity
                .colliders
                .iter()
                .all(|collider| world.loading_binding(*collider).is_none())
            && !self
                .shots
                .values()
                .any(|shot| shot.body.entity == body.entity)
    }

    /// Arms a beam to fire its pulses. A refused beam changes nothing: the id
    /// is taken or the beam's geometry is outside what the material laws
    /// accept.
    pub fn arm_beam(&mut self, spec: BeamSpec) -> Result<(), WeaponError> {
        if self.shots.contains_key(&spec.id) || self.beams.contains_key(&spec.id) {
            return Err(WeaponError::Exists);
        }
        if !(length_ok(spec.width) && length_ok(spec.height)) {
            return Err(MaterialError::InvalidGeometry.into());
        }
        let source = Source::new(SourceSpec {
            id: spec.id,
            owner: spec.owner,
            projectile: None,
            payload: None,
            beam: Some(spec.pulses),
        })?;
        self.beams.insert(
            spec.id,
            Beam {
                source,
                pulse: spec.pulses.energy,
                width: spec.width,
                height: spec.height,
                collision_groups: spec.collision_groups,
                protected: spec.protected,
                keys: Keys::default(),
            },
        );
        Ok(())
    }

    /// Records, while any shot is live, the motion each body brings into a
    /// step of `dt` seconds, with a position-driven body's prescribed
    /// velocity. Settle the step with what this returns. A shot launched after
    /// this sweeps from where it was launched, but a hit in that first step is
    /// measured against the other body's motion after the step.
    pub fn prepare(&self, world: &PhysicsWorld, dt: f32) -> Prepared {
        let motions = if self.shots.is_empty() {
            BTreeMap::new()
        } else {
            world
                .motions()
                .filter_map(|record| Some((record.id, world.incoming_motion(record.id, dt)?)))
                .collect()
        };
        Prepared { motions }
    }

    /// Sweeps, stops and retires the shots after a step of `dt` seconds. A
    /// paused step changes nothing. A shot stops on the contact that took the
    /// most impulse in the step, so a touch that pushed nothing does not stop
    /// it. The bodies of shots that retire leave the world once every shot
    /// has flown, so two shots that hit each other both stop.
    pub fn settle(
        &mut self,
        prepared: Prepared,
        world: &mut PhysicsWorld,
        dt: f32,
        recipients: &mut impl Recipients,
    ) {
        if !positive(dt) {
            return;
        }
        let mut contacts = self.contacts(world);
        let mut done = Vec::new();
        let ids: Vec<SourceId> = self.shots.keys().copied().collect();
        for id in ids {
            let mut shot = self.shots.remove(&id).unwrap();
            let touched = contacts.remove(&shot.body).unwrap_or_default();
            if self.fly(&prepared, world, dt, &mut shot, touched, recipients) {
                self.shots.insert(id, shot);
            } else {
                done.push(shot);
            }
        }
        for shot in done {
            self.finish(world, shot.source, Some(shot.body));
        }
    }

    /// The step's contacts that took impulse, for each shot's body, the
    /// strongest first.
    fn contacts(&self, world: &PhysicsWorld) -> BTreeMap<BodyId, Vec<Contact>> {
        let bodies: BTreeSet<BodyId> = self.shots.values().map(|shot| shot.body).collect();
        let mut contacts = BTreeMap::<BodyId, Vec<Contact>>::new();
        for event in world.contact_events() {
            if event.impulse_magnitude <= 0.0 {
                continue;
            }
            let sides = [
                (event.collider_a, event.collider_b, event.normal),
                (event.collider_b, event.collider_a, -event.normal),
            ];
            for (own, other, normal) in sides {
                if let Some(body) = world.collider_body(own)
                    && bodies.contains(&body)
                {
                    contacts.entry(body).or_default().push(Contact {
                        other,
                        normal,
                        point: event.point,
                        impulse: event.impulse_magnitude,
                    });
                }
            }
        }
        for list in contacts.values_mut() {
            list.sort_by(|a, b| b.impulse.total_cmp(&a.impulse).then(a.other.cmp(&b.other)));
        }
        contacts
    }

    /// Releases a live shot's blast where the shot is now and retires it, as a
    /// proximity or timed fuse would. A shot without a blast just retires.
    pub fn detonate(
        &mut self,
        world: &mut PhysicsWorld,
        id: SourceId,
        recipients: &mut impl Recipients,
    ) -> Result<(), WeaponError> {
        let mut shot = self.shots.remove(&id).ok_or(WeaponError::UnknownSource)?;
        if let Some(motion) = world.motion(shot.body)
            && let Some(at) = center(world, shot.body, motion)
        {
            self.burst(world, &mut shot, at, recipients);
        }
        self.finish(world, shot.source, Some(shot.body));
        Ok(())
    }

    /// Ends a live shot or beam early, as a released trigger or a dud does: a
    /// shot's body leaves the world without a blast, and the source reports
    /// its final ledger.
    pub fn retire(&mut self, world: &mut PhysicsWorld, id: SourceId) -> Result<(), WeaponError> {
        if let Some(shot) = self.shots.remove(&id) {
            self.finish(world, shot.source, Some(shot.body));
        } else {
            let beam = self.beams.remove(&id).ok_or(WeaponError::UnknownSource)?;
            self.finish(world, beam.source, None);
        }
        Ok(())
    }

    /// Fires one pulse of a beam to the first solid surface of its collision
    /// groups within the pulse's range.
    pub fn fire(
        &mut self,
        world: &mut PhysicsWorld,
        id: SourceId,
        pulse: Pulse,
        recipients: &mut impl Recipients,
    ) -> Result<(), WeaponError> {
        let beam = self.beams.get_mut(&id).ok_or(WeaponError::UnknownSource)?;
        let Pulse {
            start,
            direction,
            range,
            energy,
        } = pulse;
        if !point_ok(start)
            || !point_ok(direction)
            || direction.length_squared() <= f32::EPSILON
            || !positive(range)
        {
            return Err(MaterialError::InvalidGeometry.into());
        }
        if !(energy.is_finite() && (0.0..=beam.pulse).contains(&energy)) {
            return Err(MaterialError::InvalidLoad.into());
        }
        let hit = world.cast_ray(
            start,
            direction,
            RayCastOptions {
                max_distance: range,
                solid: true,
                include_sensors: false,
                collision_groups: beam.collision_groups,
                exclude_entity: beam.protected,
            },
        );
        let axis = direction.normalized();
        let end = hit.map_or(start + axis * range, |hit| hit.point);
        let length = (end - start).length();
        let stop = hit.and_then(|hit| hard(world, &*recipients, hit.collider));
        // The beam passes through its protected entity and meets the rest of
        // what the host lists across its range: each circle across its path
        // that begins in front of the surface it stops on, and each of the
        // stop's own circles that begins within the range, however far past
        // the surface it lies. Other circles are left out before they are
        // checked, unless they cannot be placed: a center that is not finite,
        // or a radius that is negative or not finite. A stop the host lists no
        // circles for takes all that passes the rest.
        let protected = beam.protected;
        let half = 0.5 * beam.width;
        let (front, invalid) = Front::new(
            recipients
                .near(start + axis * (0.5 * range), 0.5 * (range + beam.width))
                .into_iter()
                .filter(|circle| {
                    if protected.is_some() && circle.recipient.entity() == protected {
                        return false;
                    }
                    if Some(circle.recipient) == stop
                        || !(point_ok(circle.center)
                            && circle.radius.is_finite()
                            && circle.radius >= 0.0)
                    {
                        return true;
                    }
                    let offset = circle.center - start;
                    let along = offset.dot(axis);
                    let across = (offset.x * axis.y - offset.y * axis.x).abs();
                    along + circle.radius > 0.0
                        && along - circle.radius < length
                        && across - circle.radius < half
                })
                .collect(),
        );
        let along = |target: &Target| (target.center - start).dot(axis);
        let stops = |target: &Target| Some(front.recipients[target.key as usize]) == stop;
        let circled =
            front.targets.iter().any(stops) || stop.is_some_and(|stop| invalid.contains(&stop));
        let targets: Vec<Target> = front
            .targets
            .iter()
            .filter(|target| !stops(target) || along(target) - target.radius < range)
            .copied()
            .collect();
        let far = targets
            .iter()
            .filter(|target| stops(target))
            .map(|target| along(target) + target.radius)
            .fold(length, f32::max);
        let path = beam_reach(start, start + axis * far, beam.width, beam.height, &targets)?;
        let mut entries = Vec::new();
        let mut events = refused(id, Part::Beam, invalid);
        for reach in path.reaches {
            let recipient = front.recipients[reach.key as usize];
            match entry(world, recipients, recipient, reach.width, reach.coupling) {
                Ok(entry) => entries.push(entry),
                Err(reason) => events.push(WeaponEvent::Excluded {
                    source: id,
                    part: Part::Beam,
                    recipient,
                    reason,
                    energy: energy * reach.coupling,
                }),
            }
        }
        let body = hit.and_then(|hit| world.collider_body(hit.collider));
        if let Some(recipient) = stop.filter(|_| !circled) {
            match entry(world, recipients, recipient, beam.width, path.through) {
                Ok(entry) => entries.push(entry),
                Err(reason) => events.push(WeaponEvent::Excluded {
                    source: id,
                    part: Part::Beam,
                    recipient,
                    reason,
                    energy: energy * path.through,
                }),
            }
        }
        let shares = shares(&mut beam.keys, &entries);
        let delivered = beam.source.pulse_of(energy, &shares);
        drop(shares);
        self.events.extend(events);
        self.deliver(world, recipients, id, Part::Beam, entries, delivered);
        self.events.push(WeaponEvent::Traced {
            source: id,
            start,
            end,
            body,
        });
        if self.beams[&id].source.spent() {
            let beam = self.beams.remove(&id).unwrap();
            self.finish(world, beam.source, None);
        }
        Ok(())
    }

    /// One shot's step, with the contacts its body made, the strongest first.
    /// Returns whether it flies on.
    fn fly(
        &mut self,
        prepared: &Prepared,
        world: &mut PhysicsWorld,
        dt: f32,
        shot: &mut Shot,
        contacts: Vec<Contact>,
        recipients: &mut impl Recipients,
    ) -> bool {
        let id = shot.source.id();
        let launched = shot.launched.take();
        let Some(now) = world.motion(shot.body) else {
            return false;
        };
        let before = prepared
            .motions
            .get(&shot.body)
            .copied()
            .or(launched)
            .unwrap_or(now);
        let (Some(start), Some(end)) = (
            center(world, shot.body, before),
            center(world, shot.body, now),
        ) else {
            return false;
        };
        self.sweep(
            shot,
            start,
            end,
            before.linear_velocity.length(),
            recipients,
        );
        let mut stopped = false;
        for Contact {
            other,
            normal,
            point,
            ..
        } in contacts
        {
            let Some(recipient) = hard(world, &*recipients, other) else {
                continue;
            };
            if stopped {
                self.events.push(WeaponEvent::Refused {
                    source: id,
                    part: Part::Kinetic,
                    recipient,
                    error: SourceError::Spent(Part::Kinetic),
                });
                continue;
            }
            let point = point.unwrap_or(end + normal * shot.radius);
            let Some(load) = measure(prepared, world, shot, before, other, normal, point) else {
                continue;
            };
            if shot.protected.is_some() && recipient.entity() == shot.protected {
                self.events.push(WeaponEvent::Excluded {
                    source: id,
                    part: Part::Kinetic,
                    recipient,
                    reason: Exclusion::Protected,
                    energy: load.energy(),
                });
                continue;
            }
            stopped = true;
            if let Some(body) = world.collider_body(other) {
                self.events.push(WeaponEvent::Stopped {
                    source: id,
                    body,
                    point,
                });
            }
            if recipients.protected(recipient) {
                self.events.push(WeaponEvent::Excluded {
                    source: id,
                    part: Part::Kinetic,
                    recipient,
                    reason: Exclusion::Protected,
                    energy: load.energy(),
                });
            } else {
                self.hit(world, recipients, shot, recipient, load);
            }
        }
        if stopped {
            self.burst(world, shot, end, recipients);
            return false;
        }
        shot.lifetime -= dt;
        shot.lifetime > 0.0
    }

    /// Passes the tissue a shot crossed from `start` to `end`, in order.
    fn sweep(
        &mut self,
        shot: &mut Shot,
        start: Vec2,
        end: Vec2,
        speed: f32,
        recipients: &mut impl Recipients,
    ) {
        let id = shot.source.id();
        let (front, invalid) = Front::new(recipients.near(
            start.midpoint(end),
            (end - start).length() * 0.5 + shot.radius,
        ));
        self.events.extend(refused(id, Part::Kinetic, invalid));
        // The shot's radius was checked at launch, so only ends that are not
        // finite fail here, and a shot whose body has them sweeps nothing.
        let Ok(crossings) = sweep_crossings(start, end, shot.radius, &front.targets) else {
            return;
        };
        for crossing in crossings {
            let index = crossing.key as usize;
            let recipient = front.recipients[index];
            let Recipient::Tissue(key) = recipient else {
                continue;
            };
            let Some((region, section)) = recipients.region(key) else {
                continue;
            };
            if shot.shielded.contains(&recipient) {
                continue;
            }
            let face = Face::new(2.0 * shot.radius.min(front.radii[index]), section);
            if recipients.protected(recipient) {
                // A shielded pass is refused as the pass would be, and not
                // made.
                let offered = shot
                    .source
                    .offered(shot.keys.of(recipient), &region, speed)
                    .and_then(|energy| {
                        face.validate()?;
                        Ok(energy)
                    });
                match offered {
                    Ok(energy) => {
                        shot.shielded.insert(recipient);
                        self.events.push(WeaponEvent::Excluded {
                            source: id,
                            part: Part::Kinetic,
                            recipient,
                            reason: Exclusion::Protected,
                            energy,
                        });
                    }
                    Err(error) => self.events.push(WeaponEvent::Refused {
                        source: id,
                        part: Part::Kinetic,
                        recipient,
                        error,
                    }),
                }
                continue;
            }
            match shot
                .source
                .pass(shot.keys.of(recipient), &region, face, speed)
            {
                Ok(outcome) => {
                    recipients.absorb(key, outcome.consumed[0]);
                    self.events.push(WeaponEvent::Delivered {
                        source: id,
                        part: Part::Kinetic,
                        recipient,
                        outcome,
                    });
                }
                Err(error) => self.events.push(WeaponEvent::Refused {
                    source: id,
                    part: Part::Kinetic,
                    recipient,
                    error,
                }),
            }
        }
    }

    /// Charges the body a shot stopped on with its measured hit.
    fn hit(
        &mut self,
        world: &mut PhysicsWorld,
        recipients: &mut impl Recipients,
        shot: &mut Shot,
        recipient: Recipient,
        load: Impact,
    ) {
        let id = shot.source.id();
        let material = match recipient {
            Recipient::Region(token) => world.loading_material(token),
            Recipient::Body { key, .. } => recipients.region(key),
            _ => None,
        };
        let Some((region, section)) = material else {
            self.events.push(WeaponEvent::Excluded {
                source: id,
                part: Part::Kinetic,
                recipient,
                reason: Exclusion::Unmodeled,
                energy: load.energy(),
            });
            return;
        };
        let contact = patch_width_from_radii(shot.radius, None)
            .map(|width| ContactGeometry::new(width, shot.section, section));
        let result = contact.map_err(SourceError::Material).and_then(|contact| {
            shot.source
                .hit(shot.keys.of(recipient), &region, contact, load)
        });
        match result {
            Ok(outcome) => {
                absorb(world, recipients, recipient, outcome.consumed[1]);
                self.events.push(WeaponEvent::Delivered {
                    source: id,
                    part: Part::Kinetic,
                    recipient,
                    outcome,
                });
            }
            Err(error) => self.events.push(WeaponEvent::Refused {
                source: id,
                part: Part::Kinetic,
                recipient,
                error,
            }),
        }
    }

    /// Releases a shot's blast at `center`, if it carries one.
    fn burst(
        &mut self,
        world: &mut PhysicsWorld,
        shot: &mut Shot,
        center: Vec2,
        recipients: &mut impl Recipients,
    ) {
        let Some(blast) = shot.blast else {
            return;
        };
        let id = shot.source.id();
        let own = shot.body.entity;
        let (front, invalid) = Front::new(
            recipients
                .near(center, blast.radius)
                .into_iter()
                .filter(|circle| circle.recipient.entity() != Some(own))
                .collect(),
        );
        // The blast's radius and height were checked at launch, so only a
        // center that is not finite fails here, and such a shot cannot burst.
        let Ok(reaches) = blast_reach(center, blast.radius, blast.height, &front.targets) else {
            return;
        };
        self.events.push(WeaponEvent::Burst { source: id, center });
        self.events.extend(refused(id, Part::Payload, invalid));
        let mut entries = Vec::new();
        for reach in reaches {
            let recipient = front.recipients[reach.key as usize];
            let energy = blast.energy * reach.coupling;
            if shot.protected.is_some() && recipient.entity() == shot.protected {
                self.events.push(WeaponEvent::Excluded {
                    source: id,
                    part: Part::Payload,
                    recipient,
                    reason: Exclusion::Protected,
                    energy,
                });
                continue;
            }
            match entry(world, recipients, recipient, reach.width, reach.coupling) {
                Ok(entry) => entries.push(entry),
                Err(reason) => self.events.push(WeaponEvent::Excluded {
                    source: id,
                    part: Part::Payload,
                    recipient,
                    reason,
                    energy,
                }),
            }
        }
        let shares = shares(&mut shot.keys, &entries);
        let delivered = shot.source.release(&shares);
        drop(shares);
        self.deliver(world, recipients, id, Part::Payload, entries, delivered);
    }

    /// Absorbs each entry's outcome into its recipient and reports it, or
    /// reports why the source refused them all.
    fn deliver(
        &mut self,
        world: &mut PhysicsWorld,
        recipients: &mut impl Recipients,
        id: SourceId,
        part: Part,
        entries: Vec<Entry>,
        delivered: Result<Vec<Outcome>, SourceError>,
    ) {
        match delivered {
            Ok(outcomes) => {
                for (entry, outcome) in entries.into_iter().zip(outcomes) {
                    absorb(world, recipients, entry.recipient, outcome.consumed[0]);
                    self.events.push(WeaponEvent::Delivered {
                        source: id,
                        part,
                        recipient: entry.recipient,
                        outcome,
                    });
                }
            }
            Err(error) => {
                for entry in entries {
                    self.events.push(WeaponEvent::Refused {
                        source: id,
                        part,
                        recipient: entry.recipient,
                        error,
                    });
                }
            }
        }
    }

    fn finish(&mut self, world: &mut PhysicsWorld, mut source: Source, body: Option<BodyId>) {
        source.retire();
        if let Some(body) = body {
            world.remove_entity(body.entity);
        }
        self.events.push(WeaponEvent::Retired {
            source: source.id(),
            owner: source.owner(),
            ledger: source.ledger(),
        });
    }
}

/// The recipient a collider stands for: its registered region, the region
/// the host keeps for its body, or its body outside the material model.
fn hard(
    world: &PhysicsWorld,
    recipients: &impl Recipients,
    collider: ColliderId,
) -> Option<Recipient> {
    if let Some(binding) = world.loading_binding(collider)
        && world.loading_material(binding.region).is_some()
    {
        return Some(Recipient::Region(binding.region));
    }
    let body = world.collider_body(collider)?;
    Some(match recipients.body(body) {
        Some(key) => Recipient::Body {
            key,
            entity: body.entity,
        },
        None => Recipient::Unmodeled(body),
    })
}

/// Refusals of the recipients whose circles the reach geometry refused.
fn refused(id: SourceId, part: Part, recipients: Vec<Recipient>) -> Vec<WeaponEvent> {
    recipients
        .into_iter()
        .map(|recipient| WeaponEvent::Refused {
            source: id,
            part,
            recipient,
            error: SourceError::Material(MaterialError::InvalidGeometry),
        })
        .collect()
}

/// A recipient's share of a front, if it can take one: a region the host
/// keeps, or a region with a registered material. Otherwise why it takes
/// none: the host's policy shields it, or it is outside the material model.
fn entry(
    world: &PhysicsWorld,
    recipients: &impl Recipients,
    recipient: Recipient,
    width: f32,
    coupling: f32,
) -> Result<Entry, Exclusion> {
    if recipients.protected(recipient) {
        return Err(Exclusion::Protected);
    }
    let (region, section) = match recipient {
        Recipient::Tissue(key) | Recipient::Body { key, .. } => recipients.region(key),
        Recipient::Region(token) => world.loading_material(token),
        Recipient::Unmodeled(_) => None,
    }
    .ok_or(Exclusion::Unmodeled)?;
    Ok(Entry {
        recipient,
        region,
        face: Face::new(width, section),
        coupling,
    })
}

fn shares<'a>(keys: &mut Keys, entries: &'a [Entry]) -> Vec<Share<'a>> {
    entries
        .iter()
        .map(|entry| Share {
            key: keys.of(entry.recipient),
            region: &entry.region,
            face: entry.face,
            coupling: entry.coupling,
        })
        .collect()
}

fn absorb(
    world: &mut PhysicsWorld,
    recipients: &mut impl Recipients,
    recipient: Recipient,
    work: f32,
) {
    match recipient {
        Recipient::Tissue(key) | Recipient::Body { key, .. } if work > 0.0 => {
            recipients.absorb(key, work)
        }
        Recipient::Region(token) => absorb_region(world, token, work),
        _ => {}
    }
}

/// Charges a region's material with work, if there is any. The delivery just
/// read the material, so the region is live.
fn absorb_region(world: &mut PhysicsWorld, token: RegionToken, work: f32) {
    if work > 0.0 {
        let absorbed = world.absorb_loading_material(token, work);
        debug_assert!(absorbed.is_ok(), "{absorbed:?}");
    }
}

/// The impact a shot made on `other`'s body at `point`, from both bodies'
/// motion before the step. `normal` points from the shot into the body.
fn measure(
    prepared: &Prepared,
    world: &PhysicsWorld,
    shot: &Shot,
    before: BodyMotion,
    other: ColliderId,
    normal: Vec2,
    point: Vec2,
) -> Option<Impact> {
    let body = world.collider_body(other)?;
    projectile_impact(
        world,
        world.body_mass(shot.body)?,
        before.linear_velocity,
        other,
        prepared.motions.get(&body).copied(),
        normal,
        point,
    )
}

/// The impact of a projectile of `mass` moving at `velocity` on `collider`'s
/// body at `point`, measured as [`Weapons`] measures its shots' hits. It is
/// for hosts that sweep their own projectiles, which need not be bodies.
///
/// The unit `normal` points from the projectile into the body. `motion` is the
/// body's motion entering the step, where the host recorded it, and its
/// motion now otherwise. A dynamic body gives way by its mass and, unless its
/// rotation is locked, by its inertia about the point; any other body is
/// immovable. A projectile moving away closes at zero. `None` means there is
/// no such body or the load is invalid, such as a mass that isn't positive.
pub fn projectile_impact(
    world: &PhysicsWorld,
    mass: f32,
    velocity: Vec2,
    collider: ColliderId,
    motion: Option<BodyMotion>,
    normal: Vec2,
    point: Vec2,
) -> Option<Impact> {
    let body = world.collider_body(collider)?;
    let motion = motion.or_else(|| world.motion(body))?;
    // Where its center was when the contact was found.
    let com = center(world, body, motion)?;
    let at = point_velocity(motion.linear_velocity, motion.angular_velocity, point - com);
    let target = match world.body_kind(body)? {
        BodyKind::Dynamic => {
            let mass = world.body_mass(body)?;
            match world.dynamic_body_inertia(body) {
                Some(inertia) if world.body_rotation_locked(body) == Some(false) => {
                    ContactBody::rotating(mass, inertia, lever(point - com, normal))
                }
                _ => ContactBody::translating(mass),
            }
        }
        _ => ContactBody::IMMOVABLE,
    };
    let load = Impact {
        closing_speed: closing_speed(normal, velocity, at),
        effective_mass: effective_mass(ContactBody::translating(mass), target).ok()?,
    };
    load.validate().ok()?;
    Some(load)
}
