//! The spider in the run's arena, climbing the actual digit cells and the
//! frame. The mechanic itself is [`engine_rapier::spider`]; this module is the
//! adapter between it and Clock: the arena claim, the frame it climbs, a
//! proxy body for each digit surface the face publishes, and the predicate
//! that says what the spider may hold.
//!
//! A surface the face publishes gets a kinematic proxy body with one cuboid:
//! the part of its footprint that is drawn, posed in the surface's frame.
//! The proxy keeps the surface's identity for as long as the surface is
//! published, moves to where the next publication will put the surface just
//! before each step, so a cell carries the spider with the velocity it
//! actually has, and goes when the surface is retired, so the spider loses
//! its hold and falls as from any support that has gone. A surface carried
//! by a rigid body in the arena itself, as a shared Falling's or Explosion's
//! pieces are, gets no proxy over that body: the body is the support, and
//! the spider may hold it through its own colliders. Nothing else in the
//! arena is a support: soil, a fixture's balls, a visit's character and
//! course and an event's own bodies stay solid obstacles, and the spider
//! counts each refusal.
//!
//! The frame is the two side rails, the ceiling on either side of a hatch
//! above the middle of the face, and a beam just above the view spanning the
//! hatch, which the spider descends from and climbs back out to. The floor is
//! the one the arena keeps under whoever rests on it. The spider is an owner
//! of the arena while it is in it, comes and goes like a visit, and a resize
//! cancels it, because its frame is the old view's.

use std::{cell::Cell, collections::BTreeMap};

use engine_core::Vec2;
use engine_rapier::{
    spider::{Spider, SpiderCounts, SpiderIntent, SpiderSpec, SpiderState},
    world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
        PhysicsWorld,
    },
};

use crate::{
    arena::{ClockArena, MECHANICS_GRAVITY, Owner, Unavailable, ids::canonical},
    digit_surfaces::{DigitSurface, Footprint, SurfaceId, SurfaceSource},
    layout::Layout,
};

/// The spider's radius in pitches: a little smaller than a lit cell's 0.8.
pub const RADIUS_PITCH: f32 = 0.3;
/// The width of the hatch in the ceiling, in pitches.
pub const HATCH_WIDTH_PITCH: f32 = 4.0;
/// How far above the view the beam over the hatch starts, in pitches.
const BEAM_RISE_PITCH: f32 = 1.0;
/// How thick the beam is, in pitches.
const BEAM_THICKNESS_PITCH: f32 = 0.5;
/// Friction of a digit cell's proxy, as the digits' other solids have.
const CELL_FRICTION: f32 = 0.65;
/// The most proxies the spider keeps at once. The face draws at most 159
/// cells at once in the shipped fonts, during Falling's cross-fade in Sans,
/// and this leaves room above that while keeping the spider within the room
/// an event takes in the arena, so the arena's capacity stays as it was.
/// Were the face ever to draw more, the proxies would stand for the cells
/// nearest the spider and the rest be counted.
pub const MAX_PROXIES: usize = 233;

/// A part of the frame the spider climbs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePart {
    LeftRail,
    RightRail,
    /// The ceiling to the left of the hatch.
    CeilingLeft,
    /// The ceiling to the right of the hatch.
    CeilingRight,
    /// The beam above the hatch, out of the view, that the spider descends
    /// from and climbs out to.
    Beam,
}

impl FramePart {
    pub const ALL: [Self; 5] = [
        Self::LeftRail,
        Self::RightRail,
        Self::CeilingLeft,
        Self::CeilingRight,
        Self::Beam,
    ];

    fn entity(self) -> PhysicsId {
        PhysicsId::new(canonical::SPIDER_FRAME.start + self as u64)
    }

    fn of(entity: PhysicsId) -> Option<Self> {
        canonical::SPIDER_FRAME
            .contains(&entity.value())
            .then(|| Self::ALL[(entity.value() - canonical::SPIDER_FRAME.start) as usize])
    }

    /// Its box at `layout`, as its lower-left and upper-right corners.
    pub(crate) fn rect(self, layout: Layout) -> (Vec2, Vec2) {
        let (min, max) = (layout.bounds_min, layout.bounds_max);
        let half_hatch = HATCH_WIDTH_PITCH * layout.pitch * 0.5;
        let beam = max.y + BEAM_RISE_PITCH * layout.pitch;
        match self {
            Self::LeftRail => (
                Vec2::new(min.x, layout.floor_y),
                Vec2::new(min.x + layout.frame_width, layout.canopy_y),
            ),
            Self::RightRail => (
                Vec2::new(max.x - layout.frame_width, layout.floor_y),
                Vec2::new(max.x, layout.canopy_y),
            ),
            Self::CeilingLeft => (
                Vec2::new(min.x, layout.canopy_y),
                Vec2::new(-half_hatch, max.y),
            ),
            Self::CeilingRight => (
                Vec2::new(half_hatch, layout.canopy_y),
                Vec2::new(max.x, max.y),
            ),
            Self::Beam => (
                Vec2::new(-half_hatch - layout.pitch, beam),
                Vec2::new(
                    half_hatch + layout.pitch,
                    beam + BEAM_THICKNESS_PITCH * layout.pitch,
                ),
            ),
        }
    }
}

/// What the spider holds, or is tied to, as Clock knows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hold {
    /// A digit surface, through its proxy.
    Surface {
        id: SurfaceId,
        source: SurfaceSource,
        departing: bool,
    },
    /// The proxy of a surface retired since the last moving tick, which the
    /// next takes away. Only a reading or setting changed while paused shows
    /// this.
    Retired(SurfaceId),
    /// A rigid body carrying surfaces.
    Rigid(BodyId),
    Frame(FramePart),
    /// The floor the arena keeps under the spider.
    Floor,
}

/// Colliders the spider asked to hold on the last tick and was refused, by
/// reason.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Refusals {
    /// A departing surface, which carries what is on it but takes nothing new.
    pub departing: u64,
    /// Another owner's body, which is no support: soil, a fixture's ball, a
    /// visit's character or course, or an event's piece that carries no
    /// surface.
    pub foreign: u64,
}

/// The spider as Clock has it now.
#[derive(Debug, Clone, PartialEq)]
pub struct SpiderFixtureState {
    pub spider: SpiderState,
    /// What its legs hold.
    pub support: Option<Hold>,
    /// What its line is tied to.
    pub anchor: Option<Hold>,
    /// Surfaces with a proxy in the arena now.
    pub proxies: usize,
    /// Drawn surfaces without a proxy, because more are drawn than the
    /// spider keeps proxies for: none in the shipped fonts.
    pub unproxied: usize,
    /// Rigid bodies carrying surfaces the spider may hold now.
    pub rigid: usize,
    pub refusals: Refusals,
    /// Moving ticks the spider has been in the arena for.
    pub ticks: u64,
}

/// Why a spider could not be admitted. Nothing changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpiderRefusal {
    /// There is a spider already.
    Present,
    /// The arena cannot take it.
    Arena(Unavailable),
    /// The start is inside the frame, the floor, a digit cell or something
    /// else solid, or outside the room.
    Blocked,
}

/// Spiders admitted and gone over the run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpiderAccounting {
    pub admitted: u64,
    pub refused: u64,
    /// Taken out on request.
    pub removed: u64,
    /// Taken out by a resize.
    pub cancelled: u64,
}

/// A proxy standing in the arena for one surface.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Proxy {
    id: SurfaceId,
    source: SurfaceSource,
    departing: bool,
    /// The cuboid's half extents and its place on the body, in world units.
    half: Vec2,
    offset: Vec2,
}

/// Where a proxy stands after the coming step.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pose {
    origin: Vec2,
    angle: f32,
    source: SurfaceSource,
    departing: bool,
    half: Vec2,
    offset: Vec2,
}

impl Pose {
    /// The proxy a surface wants, if it has a visible part and no rigid body
    /// of its own in `world`. A body in an event's own world is not in the
    /// arena, so its surface gets a proxy like any other.
    fn of(surface: &DigitSurface, world: &PhysicsWorld) -> Option<Self> {
        if surface.body.is_some_and(|body| world.contains_body(body)) {
            return None;
        }
        let Footprint { min, max } = surface.visible()?;
        Some(Self {
            origin: surface.origin,
            angle: surface.angle,
            source: surface.source,
            departing: surface.departing,
            half: (max - min) * (0.5 * surface.scale),
            offset: (min + max) * (0.5 * surface.scale),
        })
    }
}

fn proxy_entity(index: usize) -> PhysicsId {
    debug_assert!(index < MAX_PROXIES);
    PhysicsId::new(canonical::SPIDER_SURFACES.start + index as u64)
}

fn proxy_index(entity: PhysicsId) -> Option<usize> {
    canonical::SPIDER_SURFACES
        .contains(&entity.value())
        .then(|| (entity.value() - canonical::SPIDER_SURFACES.start) as usize)
}

fn proxy_body(index: usize) -> BodyId {
    BodyId::new(proxy_entity(index), BodyRole::PRIMARY)
}

fn proxy_collider(index: usize) -> ColliderId {
    ColliderId::new(proxy_entity(index), ColliderRole::PRIMARY, 0)
}

/// The distance from `point` to the box from `min` to `max`, zero inside it.
fn distance_to_rect(point: Vec2, min: Vec2, max: Vec2) -> f32 {
    let nearest = Vec2::new(point.x.clamp(min.x, max.x), point.y.clamp(min.y, max.y));
    point.distance_to(nearest)
}

pub(crate) struct SpiderFixture {
    spider: Spider,
    intent: SpiderIntent,
    /// The proxy table: a surface's proxy takes the identity of its index.
    slots: Vec<Option<Proxy>>,
    indices: BTreeMap<SurfaceId, usize>,
    /// The colliders of rigid bodies carrying surfaces the spider may hold,
    /// as of the last bind.
    rigid: Vec<ColliderId>,
    rigid_bodies: usize,
    unproxied: usize,
    refusals: Refusals,
    ticks: u64,
}

impl SpiderFixture {
    /// The spider's radius at `layout`.
    pub(crate) fn radius(layout: Layout) -> f32 {
        RADIUS_PITCH * layout.pitch
    }

    /// The spot just under the beam, above the hatch, where a spider starts
    /// its descent.
    pub(crate) fn under_beam(layout: Layout) -> Vec2 {
        let (beam, _) = FramePart::Beam.rect(layout);
        Vec2::new(0.0, beam.y - Self::radius(layout) * 1.5)
    }

    /// Whether a spider of `radius` at `position` is clear of the room's own
    /// solids: the frame, the floor's `slabs` and the drawn part of every
    /// published surface, and within the room from the floor to the beam.
    pub(crate) fn clear(
        layout: Layout,
        slabs: &[(Vec2, Vec2)],
        published: &[DigitSurface],
        position: Vec2,
        radius: f32,
    ) -> bool {
        if !position.x.is_finite() || !position.y.is_finite() {
            return false;
        }
        let (beam, _) = FramePart::Beam.rect(layout);
        if position.x - radius < layout.bounds_min.x
            || position.x + radius > layout.bounds_max.x
            || position.y - radius < layout.bounds_min.y
            || position.y + radius > beam.y
        {
            return false;
        }
        let frame = FramePart::ALL.iter().map(|part| part.rect(layout));
        if frame
            .chain(slabs.iter().copied())
            .any(|(min, max)| distance_to_rect(position, min, max) < radius)
        {
            return false;
        }
        published.iter().all(|surface| {
            surface.visible().is_none_or(|footprint| {
                let local = surface.local(position);
                distance_to_rect(local, footprint.min, footprint.max) * surface.scale >= radius
            })
        })
    }

    /// Claims the arena for the spider and puts it, its frame and a proxy for
    /// each of `published` in. The arena admitted the claim first. A start
    /// inside something solid is refused, and nothing of the spider stays.
    pub(crate) fn admit(
        arena: &mut ClockArena,
        layout: Layout,
        slabs: &[(Vec2, Vec2)],
        published: &[DigitSurface],
        position: Vec2,
    ) -> Result<Self, SpiderRefusal> {
        let radius = Self::radius(layout);
        if !Self::clear(layout, slabs, published, position, radius) {
            return Err(SpiderRefusal::Blocked);
        }
        arena.claim_resident(Owner::Spider, layout);
        let gravity_scale = arena.resident_gravity_scale();
        let world = arena
            .held_mut(Owner::Spider)
            .expect("the spider's claim keeps a world");
        for part in FramePart::ALL {
            let (min, max) = part.rect(layout);
            let entity = part.entity();
            assert!(world.insert_body(
                BodyId::new(entity, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: (min + max) * 0.5,
                    ..BodySpec::default()
                },
                &[ColliderSpec::cuboid(
                    ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                    (max.x - min.x) * 0.5,
                    (max.y - min.y) * 0.5,
                )],
            ));
        }
        let mut fixture = Self {
            // Replaced below, once the proxies are in, by the spider itself.
            spider: Spider::insert(
                world,
                PhysicsId::new(canonical::SPIDER.start),
                position,
                SpiderSpec::new(radius, MECHANICS_GRAVITY),
            )
            .ok_or(SpiderRefusal::Blocked)
            .inspect_err(|_| arena.release(Owner::Spider))?,
            intent: SpiderIntent::default(),
            slots: vec![None; MAX_PROXIES],
            indices: BTreeMap::new(),
            rigid: Vec::new(),
            rigid_bodies: 0,
            unproxied: 0,
            refusals: Refusals::default(),
            ticks: 0,
        };
        let world = arena
            .held_mut(Owner::Spider)
            .expect("the spider's claim keeps a world");
        assert!(world.set_gravity_scale(fixture.spider.body(), gravity_scale, false));
        fixture.sync(world, published);
        Ok(fixture)
    }

    /// Brings the proxies up to `published`: one for each surface with a
    /// drawn part and no rigid body in the arena, posed where the surface
    /// will stand after the coming step, and none for a surface no longer
    /// published. More such surfaces than the table holds leave the farthest
    /// from the spider without one.
    fn sync(&mut self, world: &mut PhysicsWorld, published: &[DigitSurface]) {
        let mut wanted: Vec<(SurfaceId, Pose)> = published
            .iter()
            .filter_map(|surface| Pose::of(surface, world).map(|pose| (surface.id, pose)))
            .collect();
        self.unproxied = wanted.len().saturating_sub(MAX_PROXIES);
        if self.unproxied > 0 {
            let center = world
                .motion(self.spider.body())
                .map_or(Vec2::ZERO, |motion| motion.position);
            wanted.sort_by(|(_, a), (_, b)| {
                a.origin
                    .distance_to(center)
                    .total_cmp(&b.origin.distance_to(center))
            });
            wanted.truncate(MAX_PROXIES);
        }
        let wanted: BTreeMap<SurfaceId, Pose> = wanted.into_iter().collect();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if let Some(proxy) = slot
                && !wanted.contains_key(&proxy.id)
            {
                assert!(world.remove_entity(proxy_entity(index)));
                self.indices.remove(&proxy.id);
                *slot = None;
            }
        }
        for (id, pose) in wanted {
            let index = match self.indices.get(&id) {
                Some(&index) => index,
                None => {
                    let index = self
                        .slots
                        .iter()
                        .position(Option::is_none)
                        .expect("the table has a place for every surface the face publishes");
                    let mut cuboid =
                        ColliderSpec::cuboid(proxy_collider(index), pose.half.x, pose.half.y);
                    cuboid.local_position = pose.offset;
                    cuboid.friction = CELL_FRICTION;
                    cuboid.restitution = 0.0;
                    assert!(world.insert_body(
                        proxy_body(index),
                        BodySpec {
                            kind: BodyKind::KinematicPosition,
                            position: pose.origin,
                            angle: pose.angle,
                            ..BodySpec::default()
                        },
                        &[cuboid],
                    ));
                    self.slots[index] = Some(Proxy {
                        id,
                        source: pose.source,
                        departing: pose.departing,
                        half: pose.half,
                        offset: pose.offset,
                    });
                    self.indices.insert(id, index);
                    index
                }
            };
            let proxy = self.slots[index].as_mut().expect("a proxy at its index");
            assert!(world.set_next_kinematic_pose(proxy_body(index), pose.origin, pose.angle));
            if proxy.half != pose.half || proxy.offset != pose.offset {
                assert!(world.set_cuboid(proxy_collider(index), pose.half, pose.offset));
                proxy.half = pose.half;
                proxy.offset = pose.offset;
            }
            proxy.source = pose.source;
            proxy.departing = pose.departing;
        }
        // The colliders of rigid bodies in the arena carrying a surface that
        // takes an attachment, which the spider holds as it holds any support.
        let mut bodies: Vec<BodyId> = published
            .iter()
            .filter(|surface| surface.eligible())
            .filter_map(|surface| surface.body)
            .filter(|body| world.contains_body(*body))
            .collect();
        bodies.sort_unstable();
        bodies.dedup();
        self.rigid_bodies = bodies.len();
        self.rigid = if bodies.is_empty() {
            Vec::new()
        } else {
            world
                .collider_ids()
                .filter(|collider| {
                    world
                        .collider_body(*collider)
                        .is_some_and(|body| bodies.contains(&body))
                })
                .collect()
        };
    }

    /// What the spider is asked to do on the next moving tick.
    pub fn set_intent(&mut self, intent: SpiderIntent) {
        self.intent = intent;
    }

    /// The spider's part of a moving tick before the arena's step: the
    /// proxies take the poses `published` says the step ends at, and the
    /// spider acts on its intent, holding only what the room lets it.
    pub fn drive(&mut self, arena: &mut ClockArena, published: &[DigitSurface], dt: f32) {
        let world = arena
            .held_mut(Owner::Spider)
            .expect("the spider's claim keeps a world");
        self.sync(world, published);
        let held: Vec<ColliderId> = self
            .spider
            .state(world)
            .into_iter()
            .flat_map(|state| {
                state
                    .support
                    .map(|support| support.collider)
                    .into_iter()
                    .chain(state.line.map(|line| line.collider))
            })
            .collect();
        let refusals = Cell::new(Refusals::default());
        let refuse = |reason: fn(&mut Refusals)| {
            let mut counts = refusals.get();
            reason(&mut counts);
            refusals.set(counts);
            false
        };
        let slots = &self.slots;
        let rigid = &self.rigid;
        let eligible = |collider: ColliderId| {
            let entity = collider.entity;
            if let Some(index) = proxy_index(entity) {
                match slots[index] {
                    // A departing surface carries what is on it, and takes
                    // nothing new.
                    Some(proxy) => {
                        !proxy.departing || held.contains(&collider) || refuse(|r| r.departing += 1)
                    }
                    None => refuse(|r| r.foreign += 1),
                }
            } else if FramePart::of(entity).is_some()
                || canonical::ARENA_FLOOR.contains(&entity.value())
                || rigid.contains(&collider)
            {
                true
            } else {
                refuse(|r| r.foreign += 1)
            }
        };
        assert!(self.spider.drive(world, dt, self.intent, eligible));
        self.refusals = refusals.get();
    }

    /// The spider's part of a moving tick after the arena's step.
    pub fn observe(&mut self) {
        self.ticks += 1;
    }

    /// What the spider is holding through `collider`, as the surfaces
    /// `published` says.
    fn hold(&self, world: &PhysicsWorld, published: &[DigitSurface], collider: ColliderId) -> Hold {
        let entity = collider.entity;
        if let Some(index) = proxy_index(entity) {
            let proxy = self.slots[index].expect("a held proxy is in the table");
            return match published.iter().find(|surface| surface.id == proxy.id) {
                Some(surface) => Hold::Surface {
                    id: surface.id,
                    source: surface.source,
                    departing: surface.departing,
                },
                None => Hold::Retired(proxy.id),
            };
        }
        if let Some(part) = FramePart::of(entity) {
            return Hold::Frame(part);
        }
        if canonical::ARENA_FLOOR.contains(&entity.value()) {
            return Hold::Floor;
        }
        Hold::Rigid(
            world
                .collider_body(collider)
                .expect("a held collider has a body"),
        )
    }

    pub fn state(&self, arena: &ClockArena, published: &[DigitSurface]) -> SpiderFixtureState {
        let world = arena
            .held(Owner::Spider)
            .expect("the spider's claim keeps a world");
        let spider = self
            .spider
            .state(world)
            .expect("the spider is in the arena");
        SpiderFixtureState {
            support: spider
                .support
                .map(|support| self.hold(world, published, support.collider)),
            anchor: spider
                .line
                .map(|line| self.hold(world, published, line.collider)),
            spider,
            proxies: self.indices.len(),
            unproxied: self.unproxied,
            rigid: self.rigid_bodies,
            refusals: self.refusals,
            ticks: self.ticks,
        }
    }

    /// Takes the spider out of the arena: its body, its line, its frame and
    /// its proxies, and nothing else. Returns the spider's final counts.
    pub fn remove(self, arena: &mut ClockArena) -> SpiderCounts {
        let world = arena
            .held_mut(Owner::Spider)
            .expect("the spider's claim keeps a world");
        let counts = self.spider.remove(world);
        arena.release(Owner::Spider);
        counts
    }
}

#[cfg(test)]
mod tests;
