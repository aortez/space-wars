//! The host for the engine's contact charges (#330). Each body a crash can
//! harm, or that a crash strikes, has a material region: ships and pods,
//! pilots on foot, rovers, tanks, asteroids, the sun and the planets. After
//! each step, charges price every new impact between two of them against the
//! surfaces here, and the step lists them in
//! [`SpacewarsState::contact_hits`]. Pricing moves nothing, so Rapier's
//! restitution still sets the bounce.
//!
//! Charges wear asteroids and tanks. Just after the contact rules deal damage,
//! each asteroid loses the health its impacts consumed, and the rules no
//! longer wear it. A tank has no contact rule, so its charges are all that its
//! contacts cost it (#238). Every other body takes the contact rules' damage as
//! before.
//! See `docs/design/material-contact-charges.md`.

use super::*;
use engine_material::{ROCK, Region};
use engine_rapier::world::loading::charges::{
    ContactCharge, ContactSurface, ContactSurfaces, Depth, Unpriced,
};
use engine_rapier::world::loading::{
    CaptureConfig, ContactSource, EpisodeId, EpisodeTransition, RegionId, RegionToken,
};
use engine_rapier::world::{BodyId as PhysicsBodyId, ColliderId, PhysicsId, PhysicsWorld};
use integrity::{HealthMap, THICKNESS};
use physics::SpacewarsPhysics;
use shots::Bodies;
use surface_sortie::{SurfacePilot, match_rules};
use tanks::{TankDamage, TankDamageCause};

/// A hull's radius of curvature where it meets others, `wu`, about that of a
/// ship's nose or wingtip. An estimate: four times it costs a crashing ship a
/// quarter to a half less.
const HULL_RADIUS: f32 = 1.0;
/// The least radius debris meets others with, `wu`.
const MIN_DEBRIS_RADIUS: f32 = 0.05;
/// The rock behind the sun's and each planet's surface, `wu^3`: the most a
/// region holds, so that no impact uses it up.
const UNWORN_VOLUME: f32 = 1.0e12;
/// How many retired regions' tokens to keep before forgetting them.
const PRUNE_SLACK: usize = 64;

/// One new impact between two bodies with regions, as charges priced it in
/// the step just taken. Each side's slot follows `entities`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactHit {
    /// The physics ids of the two bodies.
    pub entities: [u64; 2],
    /// The fastest approach that started the impact, `wu / s`.
    pub closing_speed: f32,
    /// The normal where the bodies met, from the first toward the second.
    /// Zero if capture kept no point of the impact.
    pub normal: Vec2,
    /// The health each side's work stands for. The sun and the planets have
    /// no health, so their side shows none.
    pub health: [f32; 2],
    /// Whether the game shields each side, as it does the sun, the planets,
    /// a pod or a pilot outside a match, and a body under ejection
    /// protection. An unpriced impact shows it too.
    pub shielded: [bool; 2],
    /// Whether each side's work took the last of its health.
    pub failed: [bool; 2],
    /// Why charges did not price the impact, if they did not. Such an impact
    /// shows no health.
    pub unpriced: Option<Unpriced>,
}

/// What contact charges missed since the world was made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContactDiagnostics {
    /// Contact batches capture rejected. Their impacts are missing from
    /// [`SpacewarsState::contact_hits`].
    pub rejected_batches: usize,
    /// Regions and bindings the world refused, each leaving a collider
    /// unbound for a step.
    pub refused_bindings: usize,
    /// The impacts that charges left unpriced. Each is listed in
    /// [`SpacewarsState::contact_hits`] with the reason.
    pub unpriced: usize,
}

/// The regions contact charges keep.
#[derive(Debug, Clone, Default)]
pub(super) struct Regions {
    /// Each bound body's region. A removed body's token stays until a prune.
    tokens: BTreeMap<PhysicsBodyId, RegionToken>,
    /// How many tokens to hold before forgetting the retired ones.
    prune_at: usize,
    /// Each new region whose body was already touching another, as a
    /// rebuilt body often is, with that other, until the contact rules see
    /// the two part.
    held: BTreeSet<(RegionToken, MechanicalEntity)>,
    diagnostics: ContactDiagnostics,
}

impl Regions {
    /// Binds `collider` to the region of `body`, its parent, making the
    /// region if the body has none, and returns the region it made. A body's
    /// region is its own even where one entity has several bodies, as a
    /// rover does.
    fn bind(
        &mut self,
        world: &mut PhysicsWorld,
        collider: ColliderId,
        body: PhysicsBodyId,
    ) -> Option<RegionToken> {
        let mut made = None;
        let live = self
            .tokens
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
                    return None;
                };
                self.tokens.insert(body, token);
                made = Some(token);
                token
            }
        };
        let source = ContactSource {
            region: token,
            patch: physics::contact_patch(collider),
        };
        if world.bind_loading_collider(collider, source).is_err() {
            self.diagnostics.refused_bindings += 1;
        }
        made
    }

    /// Whether `charge` is between a new region and a body it was already
    /// touching, which have not parted since.
    fn holds(&self, charge: &ContactCharge) -> bool {
        let (ContactCharge::Charged { sources, .. } | ContactCharge::Unpriced { sources, .. }) =
            charge;
        let entities = sources.map(|source| physics::classify_entity(source.region.id.entity));
        (0..2).any(|at| {
            entities[1 - at].is_some_and(|other| self.held.contains(&(sources[at].region, other)))
        })
    }

    /// Forgets the regions the world retired with their bodies, once enough
    /// have gathered.
    fn prune(&mut self, world: &PhysicsWorld) {
        if self.tokens.len() > self.prune_at {
            self.tokens
                .retain(|_, token| world.loading_region_is_live(*token));
            self.prune_at = 2 * self.tokens.len() + PRUNE_SLACK;
        }
    }
}

impl SpacewarsState {
    /// What contact charges missed since the world was made, or `None` if
    /// they are off.
    pub fn contact_diagnostics(&self) -> Option<ContactDiagnostics> {
        self.physics
            .contacts
            .as_ref()
            .map(|regions| regions.diagnostics)
    }
}

/// Turns contact charges on for a new world, with `capture`'s bounds.
pub(super) fn enable(physics: &mut SpacewarsPhysics, capture: CaptureConfig) {
    let enabled = physics
        .world
        .enable_loading_capture(capture)
        .and_then(|()| physics.world.enable_speculative_arrivals());
    debug_assert!(enabled.is_ok(), "contact charges: {enabled:?}");
    if enabled.is_ok() {
        physics.contacts = Some(Regions::default());
    }
}

/// Capture's bounds: its defaults, with room for every body a game holds at
/// once, up to `MAX_ASTEROIDS` asteroids beside the rest, and for several
/// times the contacts of the busiest games measured.
pub(super) fn capture() -> CaptureConfig {
    CaptureConfig {
        max_regions: 4_096,
        max_bindings: 16_384,
        max_episodes: 4_096,
        max_visits_per_batch: 65_536,
        max_contacts_per_batch: 8_192,
        max_records_per_step: 32_768,
        ..CaptureConfig::default()
    }
}

/// Binds each collider of a body charges price that has no region yet: a new
/// body's, one the game rebuilt, or one whose colliders were replaced. Runs
/// before each step.
pub(super) fn bind(physics: &mut SpacewarsPhysics) {
    let Some(regions) = &physics.contacts else {
        return;
    };
    let parted: Vec<_> = regions
        .held
        .iter()
        .copied()
        .filter(|&(token, other)| {
            !physics.world.loading_region_is_live(token)
                || physics::classify_entity(token.id.entity)
                    .is_none_or(|entity| !physics.touched_lately(entity, other))
        })
        .collect();
    let world = &physics.world;
    // A collider belongs to its body's entity, so whether charges price it is
    // settled before its binding and body are looked up. Most of a busy
    // game's colliders, its chips and fragments, go no further.
    let unbound: Vec<_> = world
        .collider_ids()
        .filter(|collider| priced(physics, collider.entity))
        .filter(|&collider| world.loading_binding(collider).is_none())
        .filter_map(|collider| Some((collider, world.collider_body(collider)?)))
        .collect();
    let Some(regions) = &mut physics.contacts else {
        return;
    };
    for pair in &parted {
        regions.held.remove(pair);
    }
    let mut made: BTreeMap<MechanicalEntity, Vec<RegionToken>> = BTreeMap::new();
    for (collider, body) in unbound {
        let token = regions.bind(&mut physics.world, collider, body);
        if let Some((token, entity)) = token.zip(physics::classify_entity(body.entity)) {
            made.entry(entity).or_default().push(token);
        }
    }
    regions.prune(&physics.world);
    if made.is_empty() {
        return;
    }
    // A new region goes on with the contacts its body already had, as a
    // rebuilt body's does.
    let held: Vec<_> = physics
        .recent_contacts()
        .flat_map(|(a, b)| [(a, b), (b, a)])
        .filter_map(|(entity, other)| Some((made.get(&entity)?, other)))
        .flat_map(|(tokens, other)| tokens.iter().map(move |&token| (token, other)))
        .collect();
    if let Some(regions) = &mut physics.contacts {
        regions.held.extend(held);
    }
}

/// Whether charges price the body `entity` stands for: a ship or a pod, a
/// pilot on foot, a rover, a tank, an asteroid, the sun or a planet. The
/// rest, such as shells, chips, breakup fragments, wreckage, loose terrain and
/// the universe's boundary, have no region, so charges price none of their
/// contacts.
fn priced(physics: &SpacewarsPhysics, entity: PhysicsId) -> bool {
    match physics::classify_entity(entity) {
        Some(
            MechanicalEntity::Body(_)
            | MechanicalEntity::Ship(_)
            | MechanicalEntity::Spaceling(_)
            | MechanicalEntity::Rover(_)
            | MechanicalEntity::Tank(_),
        ) => true,
        Some(MechanicalEntity::Debris(id)) => physics.debris_kind(id) == Some(DebrisKind::Asteroid),
        _ => false,
    }
}

/// Prices the step's new impacts between bodies with regions, and lists
/// them in `contact_hits`. Runs after each step, before any rule changes
/// health, so each body is priced at the health it brought into the step.
///
/// A body the game rebuilds, as it does a ship whose shape changes or an
/// asteroid that chips, gets a new region, and its contacts start anew.
/// Until the contact rules see it part from a body it was already touching,
/// its impacts with that body only go on with that contact, and the list
/// leaves them out.
pub(super) fn price(state: &mut SpacewarsState, pilots: &[SurfacePilot]) {
    state.contact_hits.clear();
    let physics = &mut state.physics;
    let Some(regions) = &mut physics.contacts else {
        return;
    };
    regions.diagnostics.rejected_batches += physics.world.loading_diagnostics().rejected_batches;
    let impacts = physics
        .world
        .loading_batches()
        .iter()
        .flat_map(|batch| &batch.events)
        .any(|event| event.transition == EpisodeTransition::Impact);
    if !impacts {
        return;
    }
    let exposed = match_rules::exposed(state, pilots);
    let tanks = state.physics.tanks.healths();
    let SpacewarsState {
        ships,
        debris,
        rovers,
        sun,
        planets,
        physics,
        contact_hits,
        ..
    } = state;
    // The normal of the first contact record capture kept for each impact.
    let batches = physics.world.loading_batches();
    let mut normals: BTreeMap<EpisodeId, Option<(ContactSource, Vec2)>> = batches
        .iter()
        .flat_map(|batch| &batch.events)
        .filter(|event| event.transition == EpisodeTransition::Impact)
        .map(|event| (event.id, None))
        .collect();
    for record in batches.iter().flat_map(|batch| &batch.contacts) {
        if let Some(slot @ None) = record.episode.and_then(|id| normals.get_mut(&id)) {
            *slot = Some((record.sources[0], record.normal));
        }
    }
    let mut surfaces = Surfaces {
        bodies: Bodies::new(ships, debris, rovers, tanks, exposed),
        ships,
        sun: *sun,
        planets,
        spent: BTreeMap::new(),
    };
    let charges = physics.world.charge_contact_impacts(&mut surfaces);
    debug_assert!(charges.is_ok(), "contact charges are on: {charges:?}");
    let Some(regions) = &mut physics.contacts else {
        return;
    };
    for charge in charges.unwrap_or_default() {
        if regions.holds(&charge) {
            continue;
        }
        let hit = surfaces.hit(charge, &normals);
        regions.diagnostics.unpriced += usize::from(hit.unpriced.is_some());
        contact_hits.push(hit);
    }
}

/// The surfaces behind the bodies with regions, as the step left them, and
/// the work each body's impacts consumed since.
struct Surfaces<'a> {
    bodies: Bodies<'a>,
    ships: &'a [ShipState; 2],
    sun: Option<SunState>,
    planets: &'a [PlanetState],
    /// The work consumed in this step, by the body's entity value.
    spent: BTreeMap<u64, f32>,
}

/// What stands behind a body with a region.
struct Side {
    /// The body's health map and its health before the step. The sun and the
    /// planets have none: they are rock that no impact uses up.
    health: Option<(HealthMap, f32)>,
    /// The radius it meets others with, `wu`.
    radius: f32,
    shielded: bool,
}

impl Side {
    /// A body nothing may harm here, priced at full health.
    fn unharmed(map: HealthMap, radius: f32) -> Self {
        Self {
            health: Some((map, map.full_health)),
            radius,
            shielded: true,
        }
    }
}

impl Surfaces<'_> {
    /// What stands behind the body whose entity value is `key`, if anything.
    fn side(&self, key: u64) -> Option<Side> {
        let rock = |radius| Side {
            health: None,
            radius,
            shielded: true,
        };
        let body = |health, radius| Side {
            health: Some(health),
            radius,
            shielded: self.bodies.shielded(key),
        };
        let health = self.bodies.health(key);
        match physics::classify_entity(PhysicsId::new(key))? {
            MechanicalEntity::Body(BodyId::Sun) => Some(rock(self.sun?.radius)),
            MechanicalEntity::Body(BodyId::Planet(index)) => {
                Some(rock(self.planets.get(index)?.radius))
            }
            MechanicalEntity::Ship(index) => match health {
                Some(health) => Some(body(health, HULL_RADIUS)),
                // Only a match harms a pod.
                None => self
                    .ships
                    .get(index)
                    .filter(|ship| !ship.dead && ship.form == ShipForm::EscapePod)
                    .map(|_| Side::unharmed(HealthMap::pod_hull(), HULL_RADIUS)),
            },
            // Only a match harms a pilot.
            MechanicalEntity::Spaceling(_) => Some(match health {
                Some(health) => body(health, spaceling_geometry::RADIUS),
                None => Side::unharmed(HealthMap::pilot(), spaceling_geometry::RADIUS),
            }),
            MechanicalEntity::Rover(_) => {
                Some(body(health?, physics::spacewars_rover_spec().wheel_radius))
            }
            MechanicalEntity::Tank(_) => Some(body(health?, physics::tank_spec().wheel_radius)),
            MechanicalEntity::Debris(id) => {
                let radius = self.bodies.debris(id)?.radius.max(MIN_DEBRIS_RADIUS);
                Some(body(health?, radius))
            }
            _ => None,
        }
    }

    /// What `charge` priced, for the list, with the normal from `normals`:
    /// each impact's first source and its normal from that source.
    fn hit(
        &self,
        charge: ContactCharge,
        normals: &BTreeMap<EpisodeId, Option<(ContactSource, Vec2)>>,
    ) -> ContactHit {
        let (ContactCharge::Charged {
            episode,
            sources,
            closing_speed,
            ..
        }
        | ContactCharge::Unpriced {
            episode,
            sources,
            closing_speed,
            ..
        }) = &charge;
        let (sources, closing_speed) = (*sources, *closing_speed);
        let normal =
            normals
                .get(episode)
                .copied()
                .flatten()
                .map_or(Vec2::ZERO, |(first, normal)| {
                    if first == sources[0] { normal } else { -normal }
                });
        let mut hit = ContactHit {
            entities: sources.map(entity),
            closing_speed,
            normal,
            health: [0.0; 2],
            shielded: [false; 2],
            failed: [false; 2],
            unpriced: None,
        };
        match charge {
            ContactCharge::Charged {
                outcome, protected, ..
            } => {
                hit.health = [0, 1].map(|at| {
                    self.side(entity(sources[at]))
                        .and_then(|side| side.health)
                        .map_or(0.0, |(map, _)| map.health(outcome.consumed[at]))
                });
                hit.shielded = protected;
                hit.failed = outcome.failed;
            }
            ContactCharge::Unpriced { reason, .. } => {
                hit.shielded = sources.map(|source| self.protected(source));
                hit.unpriced = Some(reason);
            }
        }
        hit
    }
}

impl ContactSurfaces for Surfaces<'_> {
    fn surface(&self, source: ContactSource) -> Option<ContactSurface> {
        let key = entity(source);
        let side = self.side(key)?;
        let radius = Some(side.radius);
        let Some((map, health)) = side.health else {
            return Some(ContactSurface {
                region: Region::new(ROCK, UNWORN_VOLUME).ok()?,
                thickness: THICKNESS,
                depth: Depth::Patch(None),
                radius,
            });
        };
        let spent = map.health(self.spent.get(&key).copied().unwrap_or_default());
        Some(ContactSurface {
            region: map.region(health - spent)?,
            thickness: map.section.thickness,
            depth: Depth::Fixed(map.section.depth),
            radius,
        })
    }

    fn absorb(&mut self, source: ContactSource, work: f32) {
        *self.spent.entry(entity(source)).or_default() += work;
    }

    fn protected(&self, source: ContactSource) -> bool {
        self.side(entity(source)).is_some_and(|side| side.shielded)
    }
}

/// Whether charges wear `debris`, so that the contact rules leave it be. A
/// shot still charges it.
pub(super) fn wears(state: &SpacewarsState, debris: &DebrisState) -> bool {
    state.physics.contacts.is_some() && debris.kind == DebrisKind::Asteroid
}

/// Wears each asteroid by the health its impacts consumed in the step,
/// chipping and breaking it as the contact rules did. Runs just after those
/// rules, before broken debris breaks up. A pilot on foot leaves an asteroid
/// as it found it, as the rules did.
pub(super) fn wear_asteroids(state: &mut SpacewarsState) {
    if state.contact_hits.is_empty() {
        return;
    }
    let index: BTreeMap<u64, usize> = state
        .debris
        .iter()
        .enumerate()
        .filter(|(_, item)| item.physics_id != 0)
        .map(|(at, item)| (item.physics_id, at))
        .collect();
    let hits = std::mem::take(&mut state.contact_hits);
    for hit in hits.iter().filter(|hit| hit.unpriced.is_none()) {
        for side in 0..2 {
            let (rock, striker) = (hit.entities[side], hit.entities[1 - side]);
            let Some(&at) = index.get(&rock) else {
                continue;
            };
            let on_foot = matches!(
                physics::classify_entity(PhysicsId::new(striker)),
                Some(MechanicalEntity::Spaceling(_))
            );
            if on_foot || !wears(state, &state.debris[at]) {
                continue;
            }
            // Chips fly away from the striker.
            let away = if side == 1 { hit.normal } else { -hit.normal };
            let salt = 0xC0A7_AC70 ^ rock ^ striker.rotate_left(24);
            damage_debris(state, at, hit.health[side], away, salt);
        }
    }
    state.contact_hits = hits;
}

/// Wears each tank by the health its impacts consumed in the step. A tank has
/// no crash rule, so its charges are all that its contacts cost it. Runs with
/// [`wear_asteroids`], just after the contact rules.
pub(super) fn wear_tanks(state: &mut SpacewarsState) {
    let SpacewarsState {
        contact_hits,
        physics,
        tick,
        ..
    } = state;
    for hit in contact_hits.iter().filter(|hit| hit.unpriced.is_none()) {
        for (&entity, &amount) in hit.entities.iter().zip(&hit.health) {
            if let Some(id) = physics.tanks.by_entity(entity) {
                let damage = TankDamage {
                    tick: *tick + 1,
                    cause: TankDamageCause::Contact,
                    amount,
                    source: None,
                };
                physics.tanks.damage(id, damage);
            }
        }
    }
}

/// The entity value of the body behind `source`.
fn entity(source: ContactSource) -> u64 {
    source.region.id.entity.value()
}

#[cfg(test)]
mod tests;
