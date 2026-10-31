//! The run's shared rigid arena, and who owns which physical identity in it.
//!
//! A visit, the events that lease its arena and the run's residents all
//! simulate in one world, [`ClockArena`]. The world exists exactly while an
//! [`Owner`] holds it: the first owner to need it builds it, and the last to
//! let go takes it away. An owner that lets go while others stay takes its own
//! bodies with it, by the identities [`ids::canonical`] gives it.
//!
//! Each owner has at most its [`Owner::budget`] of bodies, colliders and
//! joints in the arena. A claim is granted only while the budgets of everyone
//! holding the arena stay within [`ARENA_CAPACITY`] and the residents within
//! [`MAX_RESIDENT_CLAIMS`]. Whatever would add an owner asks [`admits`] before
//! it changes anything, and a claim the arena cannot take is [`Unavailable`].
//! In debug builds each step audits that the world holds what its owners
//! claim and nothing else.
//!
//! A resize brings the resting residents within the new view as they are.
//! When they cannot come in that way, the arena keeps its world as it was and
//! takes no visit or event that uses the floor until a resize they fit, as
//! [`ClockArena::remap`] says.
//!
//! Every body, collider and joint in a Clock world is addressed by a
//! scenario-chosen `PhysicsId`. Those numbers used to be reserved in comments
//! beside the code that allocated them, which left five sites reading a range
//! as a meaning - "is this entity the real floor", "which course surface am I
//! standing on" - with nothing checking that two owners had not picked the same
//! number. This module is the one place the ranges are declared, so an
//! allocator and the code that interprets its numbers cannot drift apart.
//!
//! Only the shared arena needs this. A world owned by a single event and shared
//! with nobody - Rain's floats, the developer water lab, a standalone
//! `FallingWorld` before a player adopts it - carries no cross-owner meaning and
//! may reuse any number freely. [`ids::canonical`] is therefore the whole
//! contract, and its ranges must be disjoint apart from the two aliases it
//! declares.

use std::{ops::Range, time::Duration};

use engine_common::{
    ClockArenaClaim, ClockArenaLimits, ClockArenaOwner, ClockArenaState, ClockArenaStep,
    ClockEventKind,
};
use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsStepMetrics, PhysicsWorld, PhysicsWorldConfig,
};

use crate::{
    arena_fixture::MAX_FIXTURE_BALLS,
    digits::{DIGIT_SLOT_COUNT, SegmentKind},
    events::{
        ActiveEvent, aquarium::MAX_DIGIT_SOLIDS, duck::ArenaTerrain, duck::planner::MAX_SURFACES,
        explosion::MAX_EXPLOSION_CELLS, meltdown::MAX_MELTDOWN_CELLS,
    },
    floor::{FloorGeometry, responsive::ResponsiveFloor},
    grass::{MAX_SOIL_BODIES, MAX_SOIL_CHUNK_COLLIDERS, SoilHabitat},
    layout::Layout,
};

// The arena publishes its counts, refusals and remaps as they are.
pub use engine_common::{
    ClockArenaAccounting as ArenaAccounting, ClockArenaPopulation as Population,
    ClockArenaRemap as Remap, ClockArenaUnavailable as Unavailable,
};

/// Who holds the shared arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Owner {
    /// A visit's terrain and character. While an event holds the arena, the
    /// claim stays with the terrain after the character leaves, and a player
    /// who rejoins takes it back.
    Visit,
    /// An event leasing the visit's arena.
    Event(ClockEventKind),
    /// Retained soil. It is a resident: once it has material it keeps the
    /// arena for the rest of the run, with or without a visit.
    Soil,
    /// One of the arena fixture's owners, each with balls of its own: a
    /// resident that rests on whatever actually supports it.
    Fixture(u8),
}

impl Owner {
    /// A resident keeps the arena for the rest of the run, whoever else comes
    /// and goes.
    fn resident(self) -> bool {
        matches!(self, Self::Soil | Self::Fixture(_))
    }

    /// A resident that rests on the floor, so the arena keeps a floor under it
    /// while no visit's terrain is one. Soil is fixed in place and needs none.
    fn rests(self) -> bool {
        matches!(self, Self::Fixture(_))
    }

    /// Whether the entity `id` is one of this owner's.
    fn declares(self, id: u64) -> bool {
        self.ranges().iter().any(|range| range.contains(&id))
    }

    /// The most bodies, colliders and joints this owner has in the arena at
    /// once. No owner has joints.
    pub(crate) const fn budget(self) -> Population {
        match self {
            // A character, a course of at most `MAX_SURFACES` surfaces with
            // the entrance wall behind it, and two door flaps. A drain-joined
            // visit's two banks and two walls, or a responsive floor's two
            // panels, number fewer.
            Self::Visit => {
                let bodies = count(&ids::canonical::DUCK)
                    + MAX_SURFACES
                    + count(&ids::canonical::ENTRANCE_WALL)
                    + count(&ids::canonical::FLAPS);
                Population::new(bodies, bodies)
            }
            // A fixed compound of its solid digit cells and two liner panels.
            Self::Event(ClockEventKind::Aquarium) => Population::new(
                count(&ids::canonical::AQUARIUM_DIGITS) + count(&ids::canonical::AQUARIUM_LINER),
                MAX_DIGIT_SOLIDS + count(&ids::canonical::AQUARIUM_LINER),
            ),
            Self::Event(ClockEventKind::Explosion) => {
                Population::new(MAX_EXPLOSION_CELLS, MAX_EXPLOSION_CELLS)
            }
            // A bar for each lit segment and one for each of the two meridiem
            // letters, with a collider for each of their cells.
            Self::Event(ClockEventKind::Falling) => Population::new(
                DIGIT_SLOT_COUNT * SegmentKind::ALL.len() + 2,
                crate::fonts::MAX_DIGIT_CELLS + crate::meridiem::MAX_CELLS,
            ),
            Self::Event(ClockEventKind::Meltdown) => {
                Population::new(MAX_MELTDOWN_CELLS, MAX_MELTDOWN_CELLS)
            }
            Self::Event(_) => Population::NONE,
            Self::Soil => Population::new(MAX_SOIL_BODIES, MAX_SOIL_CHUNK_COLLIDERS),
            Self::Fixture(_) => Population::new(MAX_FIXTURE_BALLS, MAX_FIXTURE_BALLS),
        }
    }

    /// The identities this owner's bodies take. While other owners keep the
    /// world, letting go removes whatever is left in them.
    fn ranges(self) -> &'static [Range<u64>] {
        use ids::canonical::*;
        match self {
            // A drain-joined visit holds both of Falling's walls.
            Self::Visit => &[
                DUCK,
                HURDLES,
                COURSE,
                RESPONSIVE_PANELS,
                FALLING_WALLS,
                FLAPS,
            ],
            Self::Event(ClockEventKind::Aquarium) => &[AQUARIUM_LINER, AQUARIUM_DIGITS],
            Self::Event(ClockEventKind::Explosion) => &[EXPLOSION_CELLS],
            Self::Event(ClockEventKind::Falling) => &[FALLING_SEGMENTS, FALLING_LETTERS],
            Self::Event(ClockEventKind::Meltdown) => &[MELTDOWN_CELLS],
            Self::Event(_) => &[],
            Self::Soil => &[SOIL],
            Self::Fixture(index) => &FIXTURE_OWNER_BALLS[index as usize],
        }
    }
}

impl From<Owner> for ClockArenaOwner {
    fn from(owner: Owner) -> Self {
        match owner {
            Owner::Visit => Self::Visit,
            Owner::Event(event) => Self::Event { event },
            Owner::Soil => Self::Soil,
            Owner::Fixture(index) => Self::Fixture { index },
        }
    }
}

const fn count(range: &Range<u64>) -> usize {
    (range.end - range.start) as usize
}

/// The most the floor kept under resting residents has: a slab for each of a
/// course's surfaces at most.
const FLOOR_BUDGET: Population = Population::new(
    count(&ids::canonical::ARENA_FLOOR),
    count(&ids::canonical::ARENA_FLOOR),
);

/// The largest budget of any event that shares the arena.
const LARGEST_EVENT: Population = {
    let mut largest = Population::NONE;
    let mut index = 0;
    while index < ClockEventKind::ALL.len() {
        largest = largest.max(Owner::Event(ClockEventKind::ALL[index]).budget());
        index += 1;
    }
    largest
};

/// The most bodies, colliders and joints the arena holds at once: a visit
/// and the largest event sharing its arena, with soil and one arena fixture.
/// Everything Clock ships fits, so only residents beyond those make the arena
/// refuse a claim.
pub const ARENA_CAPACITY: Population = Owner::Visit
    .budget()
    .plus(LARGEST_EVENT)
    .plus(Owner::Soil.budget())
    .plus(Owner::Fixture(0).budget());

/// The most residents that hold the arena at once, soil included.
pub const MAX_RESIDENT_CLAIMS: usize = 4;

/// The most owners that hold the arena at once: a visit, one event sharing
/// its arena, and the residents.
pub const MAX_ARENA_CLAIMS: usize = 2 + MAX_RESIDENT_CLAIMS;

const _: () = assert!(
    FLOOR_BUDGET
        .plus(Owner::Soil.budget())
        .within(ARENA_CAPACITY)
);

/// Whether `owners` can hold the arena together: no more residents than it
/// may hold, and budgets that together fit its capacity, with the floor kept
/// under resting residents while no visit's terrain is one. Soil's claim is
/// reserved for the whole run, so it counts whether or not the soil holds the
/// arena yet, and soil is never refused. Changes nothing.
pub(crate) fn admits(owners: &[Owner]) -> Result<(), Unavailable> {
    let mut held: Vec<Owner> = Vec::with_capacity(owners.len() + 1);
    for owner in owners.iter().copied().chain([Owner::Soil]) {
        if !held.contains(&owner) {
            held.push(owner);
        }
    }
    let events = held
        .iter()
        .filter(|owner| matches!(owner, Owner::Event(_)))
        .count();
    debug_assert!(events <= 1, "one event at a time: {held:?}");
    if held.iter().filter(|owner| owner.resident()).count() > MAX_RESIDENT_CLAIMS {
        return Err(Unavailable::Residents);
    }
    let floor = held.iter().any(|owner| owner.rests()) && !held.contains(&Owner::Visit);
    let floor = if floor {
        FLOOR_BUDGET
    } else {
        Population::NONE
    };
    let need = held
        .iter()
        .fold(floor, |need, owner| need.plus(owner.budget()));
    if need.within(ARENA_CAPACITY) {
        Ok(())
    } else {
        Err(Unavailable::Capacity)
    }
}

/// What actually supports a resting resident while no visit's terrain is the
/// floor.
#[derive(Clone, Copy)]
enum Support<'a> {
    /// Fixed slabs, as `FloorGeometry::slabs` or a course gives them.
    Slabs(&'a [(Vec2, Vec2)]),
    /// A standalone event's moving panels.
    Panels(&'a ResponsiveFloor),
}

/// The floor the arena last built under its resting residents.
#[derive(Default)]
enum KeptFloor {
    #[default]
    None,
    Slabs(Vec<(Vec2, Vec2)>),
    Panels,
}

/// Who moves the arena's responsive panels this tick. It is decided once per
/// tick, before anything steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelAuthority {
    /// The visit's terrain, or the terrain its character left.
    Visit,
    /// An event that moves them with its own water.
    Event(ClockEventKind),
}

impl PanelAuthority {
    /// Rain moves the panels it rains on, and an event sharing the arena with
    /// its own water moves the panels that water sits on.
    pub(crate) fn of(event: Option<&ActiveEvent>) -> Self {
        match event {
            Some(ActiveEvent::Rain(_)) => Self::Event(ClockEventKind::Rain),
            Some(event @ (ActiveEvent::Aquarium(_) | ActiveEvent::Meltdown(_)))
                if event.shares_visit_arena() =>
            {
                Self::Event(event.kind())
            }
            _ => Self::Visit,
        }
    }
}

/// The settings every arena world is built with. Whoever builds it chooses its
/// gravity and the solver's length scale, and a visit that enters it later
/// sets its own until it leaves.
pub(crate) fn world_config(gravity: Vec2, length_unit: f32) -> PhysicsWorldConfig {
    PhysicsWorldConfig {
        gravity,
        length_unit,
        max_ccd_substeps: 2,
        collect_events: false,
        ..PhysicsWorldConfig::default()
    }
}

/// The downward acceleration Clock's own mechanics are tuned to. A visit
/// brings its character's own, and the bodies of others that share its world
/// scale theirs to keep this one.
pub(crate) const MECHANICS_GRAVITY: f32 = 400.0;

/// The settings of a world no visit holds: a standalone Falling or Explosion
/// world, or the residents' own.
pub(crate) fn mechanics_config(layout: Layout) -> PhysicsWorldConfig {
    world_config(Vec2::new(0.0, -MECHANICS_GRAVITY), layout.pitch)
}

fn remove_floor(world: &mut PhysicsWorld) {
    for id in ids::canonical::ARENA_FLOOR {
        world.remove_entity(PhysicsId::new(id));
    }
}

/// The arena's own floor as `support` shapes it: fixed slabs, as Falling's
/// banks are, or moving panels.
fn insert_floor(world: &mut PhysicsWorld, support: Option<Support<'_>>) -> KeptFloor {
    match support {
        None => KeptFloor::None,
        Some(Support::Slabs(slabs)) => {
            assert!(slabs.len() <= ids::canonical::ARENA_FLOOR.count());
            for (index, (min, max)) in slabs.iter().enumerate() {
                let entity = PhysicsId::new(ids::canonical::ARENA_FLOOR.start + index as u64);
                assert!(world.insert_body(
                    BodyId::new(entity, BodyRole::PRIMARY),
                    BodySpec {
                        kind: BodyKind::Fixed,
                        position: (*min + *max) * 0.5,
                        ..BodySpec::default()
                    },
                    &[ColliderSpec::cuboid(
                        ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                        (max.x - min.x) * 0.5,
                        (max.y - min.y) * 0.5
                    )],
                ));
            }
            KeptFloor::Slabs(slabs.to_vec())
        }
        Some(Support::Panels(floor)) => {
            floor.insert_panels_at(world, ids::canonical::ARENA_FLOOR.start);
            KeptFloor::Panels
        }
    }
}

/// The world a visit, the events leasing its arena and the run's residents
/// simulate in, the owners that keep it, and the terrain a visit's character
/// left while an event held the arena.
#[derive(Default)]
pub(crate) struct ClockArena {
    world: Option<PhysicsWorld>,
    owners: Vec<Owner>,
    /// The terrain of a visit whose character left while an event held the
    /// arena. The visit's claim stays with it.
    pub vacant: Option<ArenaTerrain>,
    /// The floor kept under resting residents, as last built.
    floor: KeptFloor,
    /// The world gravity resting residents' bodies were last scaled against.
    resident_gravity: Option<f32>,
    /// While the resting residents have not fitted the view since a resize,
    /// the layout they last fitted, whose floor the arena keeps under them.
    fitted: Option<Layout>,
    /// What the last resize did to the resting residents.
    last_remap: Option<Remap>,
    /// The last step's timings and populations.
    metrics: PhysicsStepMetrics,
    granted: u64,
    released: u64,
    refused: u64,
    steps: u64,
    worlds: u64,
}

impl ClockArena {
    pub fn world(&self) -> Option<&PhysicsWorld> {
        self.world.as_ref()
    }

    pub fn world_mut(&mut self) -> Option<&mut PhysicsWorld> {
        self.world.as_mut()
    }

    /// The world, while `owner` holds it. A visit that has let go no longer
    /// reaches the world residents keep after it.
    pub fn held(&self, owner: Owner) -> Option<&PhysicsWorld> {
        self.world.as_ref().filter(|_| self.holds(owner))
    }

    pub fn held_mut(&mut self, owner: Owner) -> Option<&mut PhysicsWorld> {
        if self.holds(owner) {
            self.world.as_mut()
        } else {
            None
        }
    }

    /// Nobody holds the arena, so there is no world.
    pub fn idle(&self) -> bool {
        self.owners.is_empty()
    }

    pub fn holds(&self, owner: Owner) -> bool {
        self.owners.contains(&owner)
    }

    /// The owners holding the arena, in the order they claimed it.
    pub fn owners(&self) -> &[Owner] {
        &self.owners
    }

    /// Whether the arena can take `owners` together, as [`admits`] says,
    /// unless a visit or an event is among them while the resting residents
    /// do not fit the view. Changes nothing.
    pub fn admissible(&self, owners: &[Owner]) -> Result<(), Unavailable> {
        if self.fitted.is_some() && owners.iter().any(|owner| !owner.resident()) {
            return Err(Unavailable::LayoutIncompatible);
        }
        admits(owners)
    }

    /// Whether the arena can take `owners` together, as
    /// [`ClockArena::admissible`] says, counting a refusal. Changes nothing
    /// else.
    pub fn admit(&mut self, owners: &[Owner]) -> Result<(), Unavailable> {
        self.admissible(owners).inspect_err(|_| self.refused += 1)
    }

    /// Counts a refusal made on the arena's behalf, as of an event that
    /// would use the floor while the resting residents do not fit the view.
    pub fn count_refusal(&mut self) {
        self.refused += 1;
    }

    /// Every claim is admitted before whatever asks for it changes anything.
    fn debug_assert_admitted(&self, owner: Owner) {
        if cfg!(debug_assertions) {
            let mut owners = self.owners.clone();
            owners.push(owner);
            assert!(
                self.admissible(&owners).is_ok(),
                "{owner:?} claims the arena beside {:?} unadmitted",
                self.owners
            );
        }
    }

    /// `owner` is the first to need a world and brings `world` with it.
    pub fn build(&mut self, owner: Owner, world: PhysicsWorld) -> &mut PhysicsWorld {
        assert!(self.idle(), "the arena has one world");
        self.debug_assert_admitted(owner);
        self.floor = KeptFloor::None;
        self.resident_gravity = None;
        self.owners.push(owner);
        self.granted += 1;
        self.worlds += 1;
        self.world.insert(world)
    }

    /// `owner` joins the world another owner built.
    pub fn claim(&mut self, owner: Owner) -> &mut PhysicsWorld {
        assert!(!self.holds(owner), "{owner:?} already holds the arena");
        self.debug_assert_admitted(owner);
        let world = self
            .world
            .as_mut()
            .expect("a claim joins an existing world");
        self.owners.push(owner);
        self.granted += 1;
        world
    }

    /// A resident claims the arena, building the world if nobody holds one.
    pub fn claim_resident(&mut self, owner: Owner, layout: Layout) -> &mut PhysicsWorld {
        if self.idle() {
            self.build(owner, PhysicsWorld::new(mechanics_config(layout)))
        } else {
            self.claim(owner)
        }
    }

    /// The soil's body takes the soil's current shape. The run calls this once
    /// per tick that moves time, just before the arena's step, where contacts
    /// with the soil form. Soil with material claims the arena the first time,
    /// and keeps it for the rest of the run.
    pub fn bind_soil(&mut self, soil: &mut SoilHabitat, layout: Layout) {
        if soil.accounting().added == 0 {
            return;
        }
        let world = if self.holds(Owner::Soil) {
            self.world.as_mut().expect("the soil's claim keeps a world")
        } else {
            self.claim_resident(Owner::Soil, layout)
        };
        soil.bind_current(world);
    }

    /// `owner` brings `world`, which becomes the arena's world as it is, with
    /// its contacts and settings. Residents that already keep a world move
    /// their bodies into it, which leaves only their contact history behind.
    pub fn adopt(&mut self, owner: Owner, mut world: PhysicsWorld) -> &mut PhysicsWorld {
        assert!(!self.holds(owner), "{owner:?} already holds the arena");
        self.debug_assert_admitted(owner);
        if let Some(mut kept) = self.world.take() {
            let entities: Vec<_> = kept.entity_ids().collect();
            for entity in entities {
                // The adopted world brings its own floor.
                if ids::canonical::ARENA_FLOOR.contains(&entity.value()) {
                    continue;
                }
                assert!(
                    kept.transfer_entity(entity, &mut world),
                    "{entity:?} moves into the adopted world"
                );
            }
        }
        self.floor = KeptFloor::None;
        self.resident_gravity = None;
        self.owners.push(owner);
        self.granted += 1;
        self.worlds += 1;
        self.world.insert(world)
    }

    /// `owner` lets go, if it held the arena. The last owner to leave takes the
    /// world with it. Otherwise only `owner`'s own bodies go.
    pub fn release(&mut self, owner: Owner) {
        if owner == Owner::Visit {
            assert!(
                self.vacant.is_none(),
                "vacant terrain keeps its visit's claim"
            );
        }
        let held = self.holds(owner);
        self.owners.retain(|other| *other != owner);
        self.released += u64::from(held);
        if self.owners.is_empty() {
            assert!(
                self.vacant.is_none(),
                "vacant terrain keeps its visit's claim"
            );
            self.world = None;
            self.floor = KeptFloor::None;
            self.resident_gravity = None;
            self.fitted = None;
        } else if held {
            let world = self.world.as_mut().expect("a held arena");
            for range in owner.ranges() {
                for id in range.clone() {
                    world.remove_entity(PhysicsId::new(id));
                }
            }
            if owner.rests() && !self.owners.iter().any(|owner| owner.rests()) {
                remove_floor(world);
                self.floor = KeptFloor::None;
                // Nothing left beyond the view.
                self.fitted = None;
            }
        }
    }

    /// The vacant terrain and the world it stands in, borrowed together.
    pub fn vacant_with_world(&mut self) -> Option<(&mut ArenaTerrain, &mut PhysicsWorld)> {
        Some((self.vacant.as_mut()?, self.world.as_mut()?))
    }

    /// A resting resident holds the arena, which keeps a floor under it.
    pub fn keeps_floor(&self) -> bool {
        self.owners.iter().any(|owner| owner.rests())
    }

    /// The resting residents have not fitted the view since a resize.
    pub fn layout_incompatible(&self) -> bool {
        self.fitted.is_some()
    }

    /// The layout of the floor under resting residents: the view's, unless
    /// they have not fitted it since a resize, when it is the one they last
    /// fitted.
    pub fn floor_layout(&self, view: Layout) -> Layout {
        self.fitted.unwrap_or(view)
    }

    /// The arena's part of a resize to `floor`'s layout from `previous`, once
    /// visits and events have finished and only residents hold it. A resting
    /// resident that lies beyond the new view's edges moves in just far
    /// enough to lie within them, keeping its height, angle and motion, so
    /// the world creates, heals and resets nothing. The solver's length scale
    /// becomes the new layout's, as a world built for it would have. Soil
    /// keeps its own coordinates and accounts what a narrower view loses.
    ///
    /// A resident that could only come in by landing in another resident or
    /// in `floor`, or that is wider than the view, cannot come in that way.
    /// The arena then declines the remap and keeps the world exactly as it
    /// was, with the floor of the layout its residents last fitted. The view
    /// shows the part of that world within its bounds, and the arena takes no
    /// visit or event that uses the floor until a resize the residents fit,
    /// or until the last of them leaves.
    pub fn remap(&mut self, floor: FloorGeometry, previous: Layout) -> Remap {
        let layout = floor.layout();
        let Some(world) = &mut self.world else {
            return *self.last_remap.insert(Remap::Exact);
        };
        debug_assert!(
            self.owners.iter().all(|owner| owner.resident()),
            "a resize finishes visits and events first: {:?}",
            self.owners
        );
        // Each resting body's box, and how far it moves to lie within the view.
        let mut bodies = Vec::new();
        let mut fits = true;
        for owner in self.owners.iter().filter(|owner| owner.rests()) {
            for range in owner.ranges() {
                for id in range.clone() {
                    let body = BodyId::new(PhysicsId::new(id), BodyRole::PRIMARY);
                    let Some((min, max)) = world.body_solid_bounds(body) else {
                        continue;
                    };
                    fits &= max.x - min.x <= layout.bounds_max.x - layout.bounds_min.x;
                    let shift = if min.x < layout.bounds_min.x {
                        layout.bounds_min.x - min.x
                    } else {
                        (layout.bounds_max.x - max.x).min(0.0)
                    };
                    bodies.push((body, min, max, shift));
                }
            }
        }
        // Resting bodies in contact overlap by the solver's allowed error, a
        // thousandth of its length unit. Ten times that is a landing.
        let slack = world.length_unit() * 0.01;
        let meet = |(a_min, a_max): (Vec2, Vec2), (b_min, b_max): (Vec2, Vec2)| {
            a_max.x.min(b_max.x) - a_min.x.max(b_min.x) > slack
                && a_max.y.min(b_max.y) - a_min.y.max(b_min.y) > slack
        };
        let landed = |&(_, min, max, shift): &(BodyId, Vec2, Vec2, f32)| {
            let shift = Vec2::new(shift, 0.0);
            (min + shift, max + shift)
        };
        let slabs: Vec<_> = floor.support().collect();
        // Each body that moves lands clear of the floor and of every body that
        // moves otherwise. Bodies that move together, as a stack does, keep
        // their places to each other. Boxes stand in for shapes, so a remap
        // can decline balls that would have come in clear, but never lands
        // one in another.
        fits &= bodies.iter().filter(|body| body.3 != 0.0).all(|moving| {
            let place = landed(moving);
            !slabs.iter().any(|slab| meet(place, *slab))
                && !bodies
                    .iter()
                    .any(|other| other.3 != moving.3 && meet(place, landed(other)))
        });
        let remap = if fits {
            let mut moved = 0;
            for &(body, _, _, shift) in bodies.iter().filter(|body| body.3 != 0.0) {
                let motion = world.motion(body).expect("a resting resident's body");
                assert!(world.set_pose(
                    body,
                    motion.position + Vec2::new(shift, 0.0),
                    motion.angle,
                    false
                ));
                moved += 1;
            }
            assert!(world.set_length_unit(layout.pitch));
            self.fitted = None;
            if moved == 0 {
                Remap::Exact
            } else {
                Remap::Clamped { moved }
            }
        } else {
            self.fitted.get_or_insert(previous);
            Remap::Declined
        };
        self.last_remap = Some(remap);
        remap
    }

    /// The floor under resting residents takes the shape of what actually
    /// supports them now. The run calls this once per tick that moves time,
    /// just before the arena's step. A visit's terrain is the floor while the
    /// visit holds the arena, so the arena keeps none of its own. Otherwise
    /// an event with its own floor is the floor: Rain's course or panels, or
    /// Meltdown's or Aquarium's panels. Otherwise the ordinary `floor` is,
    /// open at the drain or closed. A floor that closes at once, as Rain's
    /// panels do when Rain finishes part-way open, can appear under a resident
    /// resting below its surface, and the solver lifts the resident out.
    pub fn bind_floor(&mut self, event: Option<&ActiveEvent>, floor: FloorGeometry) {
        if !self.keeps_floor() {
            return;
        }
        let slabs: Vec<_>;
        let support = if self.holds(Owner::Visit) {
            None
        } else {
            match event {
                Some(ActiveEvent::Rain(rain)) => match rain.course() {
                    Some(course) => {
                        slabs = course.slabs();
                        Some(Support::Slabs(&slabs))
                    }
                    None => rain.responsive_floor().map(Support::Panels),
                },
                Some(ActiveEvent::Meltdown(event)) => event.floor.as_ref().map(Support::Panels),
                Some(ActiveEvent::Aquarium(event)) => Some(Support::Panels(&event.floor)),
                _ => {
                    slabs = floor.support().collect();
                    Some(Support::Slabs(&slabs))
                }
            }
        };
        let world = self.world.as_mut().expect("a held arena");
        match (support, &self.floor) {
            (None, KeptFloor::None) | (Some(Support::Panels(_)), KeptFloor::Panels) => {}
            (Some(Support::Slabs(slabs)), KeptFloor::Slabs(kept)) if slabs == kept.as_slice() => {}
            _ => {
                remove_floor(world);
                self.floor = insert_floor(world, support);
                return;
            }
        }
        if let Some(Support::Panels(floor)) = support {
            floor.move_panels_at(world, ids::canonical::ARENA_FLOOR.start);
        }
    }

    /// While only residents hold the arena, its world has the settings they
    /// would build it with: Clock's own gravity, and the length scale of
    /// `layout`, the one they last fitted. A visit that entered set its own
    /// for as long as it held the arena. The run calls this once per tick that
    /// moves time, just before the arena's step.
    pub fn bind_settings(&mut self, layout: Layout) {
        let Some(world) = &mut self.world else {
            return;
        };
        if self.owners.iter().all(|owner| owner.resident()) {
            let config = mechanics_config(layout);
            assert!(world.set_gravity(config.gravity) && world.set_length_unit(config.length_unit));
        }
    }

    /// How much a resting resident scales the world's gravity by to fall at
    /// Clock's own, whoever set the world's.
    pub fn resident_gravity_scale(&self) -> f32 {
        self.world
            .as_ref()
            .map_or(1.0, |world| MECHANICS_GRAVITY / -world.gravity().y)
    }

    /// The arena's one rigid step this tick, whoever holds it, residents with
    /// nobody else included. Nothing else steps the arena's world. A visit
    /// brings its own gravity for as long as it holds the arena, so resting
    /// residents first rescale theirs whenever the world's has changed, and
    /// fall at Clock's own throughout.
    pub fn step(&mut self, dt: f32) {
        let scale = self.resident_gravity_scale();
        let Some(world) = &mut self.world else {
            return;
        };
        let gravity = world.gravity().y;
        if self.resident_gravity != Some(gravity) {
            for owner in self.owners.iter().filter(|owner| owner.rests()) {
                for range in owner.ranges() {
                    for id in range.clone() {
                        let body = BodyId::new(PhysicsId::new(id), BodyRole::PRIMARY);
                        if world.contains_body(body) {
                            assert!(world.set_gravity_scale(body, scale, false));
                        }
                    }
                }
            }
            self.resident_gravity = Some(gravity);
        }
        #[cfg(debug_assertions)]
        self.audit();
        let world = self.world.as_mut().expect("a held arena");
        self.metrics = world.step(dt);
        self.steps += 1;
    }

    /// The arena's integrity, checked before each step in debug builds. Its
    /// owners are distinct, within the claim limits and admitted together.
    /// The world's identities and Rapier's objects name each other one to
    /// one. Every entity belongs to exactly one owner holding the arena, by
    /// the identities it declares, or to the floor kept under resting
    /// residents while no visit's terrain is one. Each owner has no more than
    /// its budget.
    #[cfg(debug_assertions)]
    fn audit(&self) {
        let world = self.world.as_ref().expect("a held arena");
        for (index, owner) in self.owners.iter().enumerate() {
            assert!(
                !self.owners[index + 1..].contains(owner),
                "{owner:?} holds the arena twice"
            );
        }
        assert!(self.owners.len() <= MAX_ARENA_CLAIMS, "{:?}", self.owners);
        assert!(
            self.admissible(&self.owners).is_ok(),
            "{:?} hold the arena unadmitted",
            self.owners
        );
        if let Some(fault) = world.identity_fault() {
            panic!("the arena's identities: {fault}");
        }
        let floor = self.keeps_floor() && !self.holds(Owner::Visit);
        for entity in world.entity_ids() {
            let id = entity.value();
            #[cfg(test)]
            if ids::canonical::TEST_OBSTACLES.contains(&id) {
                continue;
            }
            if ids::canonical::ARENA_FLOOR.contains(&id) {
                assert!(floor, "floor {id} with no resting resident on it");
                continue;
            }
            match self
                .owners
                .iter()
                .filter(|owner| owner.declares(id))
                .count()
            {
                0 => panic!("entity {id} belongs to no owner of {:?}", self.owners),
                1 => {}
                _ => panic!("entity {id} has two owners"),
            }
        }
        let (populations, floor) = self.populations();
        for (owner, population) in self.owners.iter().zip(&populations) {
            let budget = owner.budget();
            assert!(
                population.within(budget),
                "{owner:?} has {population:?}, more than {budget:?}"
            );
        }
        assert!(
            floor.within(FLOOR_BUDGET),
            "the floor has {floor:?}, more than {FLOOR_BUDGET:?}"
        );
        assert_eq!(world.joint_count(), 0, "no owner has joints");
    }

    /// What each owner holding the arena has in its world, in the order they
    /// claimed it, and what the floor kept under resting residents has. An
    /// entity no owner declares counts for nobody.
    fn populations(&self) -> (Vec<Population>, Population) {
        let mut owners = vec![Population::NONE; self.owners.len()];
        let mut floor = Population::NONE;
        let Some(world) = &self.world else {
            return (owners, floor);
        };
        let mut tally = |entity: PhysicsId, add: fn(&mut Population)| {
            let id = entity.value();
            if ids::canonical::ARENA_FLOOR.contains(&id) {
                add(&mut floor);
            } else if let Some(index) = self.owners.iter().position(|owner| owner.declares(id)) {
                add(&mut owners[index]);
            }
        };
        for body in world.body_ids() {
            tally(body.entity, |population| population.bodies += 1);
        }
        for collider in world.collider_ids() {
            tally(collider.entity, |population| population.colliders += 1);
        }
        (owners, floor)
    }

    /// The last step's timings and populations.
    pub fn metrics(&self) -> PhysicsStepMetrics {
        self.metrics
    }

    pub fn accounting(&self) -> ArenaAccounting {
        let world = self.world.as_ref();
        ArenaAccounting {
            claims: self.owners.len(),
            residents: self.owners.iter().filter(|owner| owner.resident()).count(),
            bodies: world.map_or(0, PhysicsWorld::body_count),
            colliders: world.map_or(0, PhysicsWorld::collider_count),
            joints: world.map_or(0, PhysicsWorld::joint_count),
            granted: self.granted,
            released: self.released,
            refused: self.refused,
            steps: self.steps,
            worlds: self.worlds,
            layout_incompatible: self.layout_incompatible(),
            last_remap: self.last_remap,
        }
    }

    /// What the arena publishes: its accounting, what each of its owners and
    /// its floor have in its world, its last step and its limits.
    pub fn state(&self) -> ClockArenaState {
        let (populations, floor) = self.populations();
        let micros = |time: Duration| time.as_micros() as u64;
        let metrics = self.metrics;
        ClockArenaState {
            accounting: self.accounting(),
            claims: self
                .owners
                .iter()
                .zip(populations)
                .map(|(&owner, population)| ClockArenaClaim {
                    owner: owner.into(),
                    resident: owner.resident(),
                    population,
                    budget: owner.budget(),
                })
                .collect(),
            floor,
            last_step: (self.steps > 0).then(|| ClockArenaStep {
                wall_us: micros(metrics.wall_time),
                collision_detection_us: micros(metrics.collision_detection_time),
                solver_us: micros(metrics.solver_time),
                ccd_substeps: metrics.ccd_substeps,
                active_bodies: metrics.active_bodies,
                sleeping_bodies: metrics.sleeping_bodies,
                candidate_pairs: metrics.candidate_pairs,
                contact_pairs: metrics.contact_pairs,
                contacts: metrics.contacts,
            }),
            limits: ClockArenaLimits {
                claims: MAX_ARENA_CLAIMS,
                residents: MAX_RESIDENT_CLAIMS,
                capacity: ARENA_CAPACITY,
            },
        }
    }
}

/// The rigid world the run's events are simulating, whoever is using it.
///
/// That is the run's arena unless an event simulates in its own world:
/// Falling's or Explosion's before a player joins, Meltdown's water lab, or
/// Rain's floats. Residents such as soil stay in the arena meanwhile, apart
/// from that event. `visit` says whether a visit exists, because Rain's floats
/// stop being canonical as soon as one does, before its first step has built
/// the arena. The parts are taken as separate borrows rather than through
/// `ClockState` because callers hold the soil and the plants at the same time.
pub(crate) fn canonical_world<'a>(
    event: &'a Option<ActiveEvent>,
    visit: bool,
    arena: &'a ClockArena,
) -> Option<&'a PhysicsWorld> {
    match event {
        Some(event) if event.simulates_apart(visit) => event.own_world(),
        _ => arena.world(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arena_takes_everything_clock_ships_beside_soil_and_one_fixture() {
        for kind in ClockEventKind::ALL {
            let owners = [
                Owner::Visit,
                Owner::Event(kind),
                Owner::Soil,
                Owner::Fixture(0),
            ];
            assert_eq!(admits(&owners), Ok(()), "{kind:?}");
            // A second fixture leaves no room for the largest events.
            let owners = [
                Owner::Visit,
                Owner::Event(kind),
                Owner::Fixture(0),
                Owner::Fixture(1),
            ];
            let largest = matches!(
                kind,
                ClockEventKind::Aquarium | ClockEventKind::Explosion | ClockEventKind::Meltdown
            );
            let refused = largest.then_some(Unavailable::Capacity);
            assert_eq!(admits(&owners).err(), refused, "{kind:?}");
        }
        // Soil's place is reserved among the residents.
        let fixtures: Vec<_> = (0..MAX_RESIDENT_CLAIMS as u8).map(Owner::Fixture).collect();
        assert_eq!(admits(&fixtures[..MAX_RESIDENT_CLAIMS - 1]), Ok(()));
        assert_eq!(admits(&fixtures), Err(Unavailable::Residents));
    }

    #[test]
    fn a_refusal_is_counted_and_changes_nothing_else() {
        let mut arena = ClockArena::default();
        let fixtures: Vec<_> = (0..MAX_RESIDENT_CLAIMS as u8).map(Owner::Fixture).collect();
        assert_eq!(
            arena.admit(&fixtures),
            Err(Unavailable::Residents),
            "soil's place is reserved"
        );
        assert_eq!(
            arena.accounting(),
            ArenaAccounting {
                refused: 1,
                ..ArenaAccounting::default()
            }
        );
        assert!(arena.idle());
    }

    /// A box of fixture 0's at `center`, as a resting resident with any
    /// shape would be.
    fn resident_box(arena: &mut ClockArena, slot: u64, center: Vec2, half: Vec2) -> BodyId {
        let entity = PhysicsId::new(ids::canonical::FIXTURE_OWNER_BALLS[0][0].start + slot);
        let body = BodyId::new(entity, BodyRole::PRIMARY);
        let world = arena.world_mut().expect("the fixture's world");
        assert!(world.insert_body(
            body,
            BodySpec {
                position: center,
                ..BodySpec::default()
            },
            &[ColliderSpec::cuboid(
                ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                half.x,
                half.y
            )],
        ));
        body
    }

    #[test]
    fn a_resize_brings_residents_in_together_or_keeps_the_world_as_it_was() {
        let (wide, narrow, narrower) = (
            Layout::new(800.0 / 480.0),
            Layout::new(1.0),
            Layout::new(0.5),
        );
        let mut arena = ClockArena::default();
        assert_eq!(
            arena.remap(FloorGeometry::closed(narrow), wide),
            Remap::Exact,
            "no world"
        );
        arena.claim_resident(Owner::Soil, wide);
        arena.claim_resident(Owner::Fixture(0), wide);
        // One box resting on another, overlapping it, both beyond the right
        // edge of the narrower view. They move together.
        let half = Vec2::new(10.0, 10.0);
        let lower = resident_box(&mut arena, 0, Vec2::new(300.0, 0.0), half);
        let upper = resident_box(&mut arena, 1, Vec2::new(300.0, 15.0), half);
        assert_eq!(
            arena.remap(FloorGeometry::closed(narrow), wide),
            Remap::Clamped { moved: 2 }
        );
        let world = arena.world().expect("the world");
        let at = |body| world.motion(body).expect("a resident").position;
        assert_eq!(
            (at(lower), at(upper)),
            (Vec2::new(230.0, 0.0), Vec2::new(230.0, 15.0))
        );
        assert!(!arena.layout_incompatible());

        // A resident wider than the view cannot come in, so nothing moves.
        let wide_box = resident_box(&mut arena, 2, Vec2::new(0.0, 100.0), Vec2::new(130.0, 5.0));
        let poses = |arena: &ClockArena| {
            let world = arena.world().expect("the world");
            [lower, upper, wide_box].map(|body| world.motion(body).expect("a resident").position)
        };
        let before = poses(&arena);
        assert_eq!(
            arena.remap(FloorGeometry::closed(narrower), narrow),
            Remap::Declined
        );
        assert_eq!(poses(&arena), before);
        assert!(arena.layout_incompatible());
        assert_eq!(
            arena.floor_layout(narrower).bounds_max.x,
            narrow.bounds_max.x
        );
        assert_eq!(
            arena.admissible(&[Owner::Soil, Owner::Fixture(0), Owner::Visit]),
            Err(Unavailable::LayoutIncompatible)
        );
        assert_eq!(
            arena.admissible(&[Owner::Soil, Owner::Fixture(0), Owner::Fixture(1)]),
            Ok(())
        );
        // A further resize that still does not fit keeps the layout the
        // residents last fitted.
        assert_eq!(
            arena.remap(FloorGeometry::closed(Layout::new(0.4)), narrower),
            Remap::Declined
        );
        assert_eq!(
            arena.floor_layout(narrower).bounds_max.x,
            narrow.bounds_max.x
        );
        assert_eq!(arena.accounting().last_remap, Some(Remap::Declined));
        // Without its resting residents the world fits any view.
        arena.release(Owner::Fixture(0));
        assert!(!arena.layout_incompatible());
        assert_eq!(arena.admissible(&[Owner::Soil, Owner::Visit]), Ok(()));
    }

    /// Debug builds catch a claim nobody admitted.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "unadmitted")]
    fn a_claim_beyond_the_arenas_capacity_fails_in_debug_builds() {
        let mut arena = ClockArena::default();
        arena.claim_resident(Owner::Fixture(0), Layout::new(1.0));
        arena.claim(Owner::Visit);
        arena.claim(Owner::Event(ClockEventKind::Meltdown));
        arena.claim_resident(Owner::Fixture(1), Layout::new(1.0));
    }
}

pub(crate) mod ids {
    /// The one shared arena, which the visit and its leasing events all insert
    /// into. Disjoint except for the aliases noted below, which the
    /// `canonical_entity_ranges_are_disjoint` test holds.
    pub(crate) mod canonical {
        use std::ops::Range;

        use engine_rapier::world::PhysicsId;

        use crate::{
            arena_fixture::{MAX_FIXTURE_BALLS, MAX_FIXTURES},
            digits::{DIGIT_SLOT_COUNT, SegmentKind},
            events::{
                duck::planner::MAX_SURFACES, explosion::MAX_EXPLOSION_CELLS,
                meltdown::MAX_MELTDOWN_CELLS,
            },
        };

        /// The duck character itself.
        pub(crate) const DUCK: Range<u64> = 1..2;
        /// Default pit and hurdle geometry, used when there is no course.
        pub(crate) const HURDLES: Range<u64> = 100..100 + MAX_SURFACES as u64;
        /// The floor the arena keeps under resting residents while no visit's
        /// terrain is one: slabs, a course's surfaces or two moving panels.
        pub(crate) const ARENA_FLOOR: Range<u64> = 800..800 + MAX_SURFACES as u64;
        /// Obstacle-course surfaces. A surface's index is recovered by
        /// subtracting this range's start, so the two must stay adjacent.
        pub(crate) const COURSE: Range<u64> = 1000..1000 + MAX_SURFACES as u64;
        /// Falling's two drain banks. Deliberately inside `COURSE`: a
        /// drain-joined player is given a fabricated two-surface course whose
        /// indices address these already-present bodies rather than inserting
        /// its own. See `DuckEvent::new_drain_player`.
        pub(crate) const DRAIN_BANKS: Range<u64> = 1000..1002;
        /// Responsive floor panels. Moved clear of `COURSE` so that the two are
        /// provably disjoint instead of merely never co-resident.
        pub(crate) const RESPONSIVE_PANELS: Range<u64> = 1900..1902;
        /// The boundary a player spawn needs behind the entrance.
        pub(crate) const ENTRANCE_WALL: Range<u64> = 2000..2001;
        /// Falling's standalone side walls. Deliberately overlaps
        /// `ENTRANCE_WALL`: a drain-joined player inherits Falling's left wall
        /// instead of inserting its own. See `DuckEvent::insert_entrance_wall`.
        pub(crate) const FALLING_WALLS: Range<u64> = 2000..2002;
        /// The two hinged door flaps.
        pub(crate) const FLAPS: Range<u64> = 2100..2102;
        /// Falling's bars: `DIGIT_SLOT_COUNT` slots of `SegmentKind::ALL`.
        pub(crate) const FALLING_SEGMENTS: Range<u64> =
            3001..3001 + (DIGIT_SLOT_COUNT * SegmentKind::ALL.len()) as u64;
        /// Falling's AM/PM letters.
        pub(crate) const FALLING_LETTERS: Range<u64> = 3032..3032 + DIGIT_SLOT_COUNT as u64;
        /// Meltdown's released cells.
        pub(crate) const MELTDOWN_CELLS: Range<u64> = 4001..4001 + MAX_MELTDOWN_CELLS as u64;
        /// Explosion's scattered cells.
        pub(crate) const EXPLOSION_CELLS: Range<u64> = 5001..5001 + MAX_EXPLOSION_CELLS as u64;
        /// Aquarium's lower liner, a second moving floor.
        pub(crate) const AQUARIUM_LINER: Range<u64> = 6001..6003;
        /// Aquarium's solid digit compound.
        pub(crate) const AQUARIUM_DIGITS: Range<u64> = 6100..6101;
        /// The arena fixture's balls, `MAX_FIXTURE_BALLS` for each of its
        /// owners.
        pub(crate) const FIXTURE_BALLS: Range<u64> =
            7001..7001 + (MAX_FIXTURES * MAX_FIXTURE_BALLS) as u64;
        /// Each fixture owner's share of `FIXTURE_BALLS`.
        pub(crate) static FIXTURE_OWNER_BALLS: [[Range<u64>; 1]; MAX_FIXTURES] = {
            let mut owners = [const { [0..0] }; MAX_FIXTURES];
            let mut index = 0;
            while index < MAX_FIXTURES {
                let start = FIXTURE_BALLS.start + (index * MAX_FIXTURE_BALLS) as u64;
                owners[index] = [start..start + MAX_FIXTURE_BALLS as u64];
                index += 1;
            }
            owners
        };
        /// Retained soil, the one resident today that outlives its admitting
        /// event.
        pub(crate) const SOIL: Range<u64> = 900_001..900_002;
        /// Obstacles unit tests drop into a visit's arena, which belong to no
        /// owner.
        #[cfg(test)]
        pub(crate) const TEST_OBSTACLES: Range<u64> = 9000..9100;

        /// The body of course surface `index`.
        pub(crate) fn course_surface(index: usize) -> PhysicsId {
            debug_assert!(index < MAX_SURFACES, "course surface {index}");
            PhysicsId::new(COURSE.start + index as u64)
        }

        /// The ids of a course's first `count` surfaces, for asking whether a
        /// contact is with the real floor.
        pub(crate) fn course_surfaces(count: usize) -> Range<u64> {
            debug_assert!(count <= MAX_SURFACES, "{count} course surfaces");
            COURSE.start..COURSE.start + count as u64
        }

        /// Which course surface `entity` is, if any. The drain banks read as
        /// surfaces 0 and 1, which is what lets a drain-joined player stand on
        /// them.
        pub(crate) fn course_surface_index(entity: PhysicsId) -> Option<usize> {
            COURSE
                .contains(&entity.value())
                .then(|| (entity.value() - COURSE.start) as usize)
        }
    }

    #[cfg(test)]
    mod tests {
        use std::ops::Range;

        use engine_rapier::world::PhysicsId;

        use super::canonical::*;
        use crate::{
            arena_fixture::MAX_FIXTURE_BALLS,
            digits::{DIGIT_SLOT_COUNT, SegmentId, SegmentKind},
            events::duck::planner::MAX_SURFACES,
        };

        /// Canonical ranges permitted to overlap, each because one owner
        /// deliberately addresses another's bodies.
        const ALIASES: &[(Range<u64>, Range<u64>)] =
            &[(COURSE, DRAIN_BANKS), (ENTRANCE_WALL, FALLING_WALLS)];

        /// Every canonical range, named for the assertion message.
        const CANONICAL: &[(&str, Range<u64>)] = &[
            ("DUCK", DUCK),
            ("HURDLES", HURDLES),
            ("ARENA_FLOOR", ARENA_FLOOR),
            ("COURSE", COURSE),
            ("DRAIN_BANKS", DRAIN_BANKS),
            ("RESPONSIVE_PANELS", RESPONSIVE_PANELS),
            ("ENTRANCE_WALL", ENTRANCE_WALL),
            ("FALLING_WALLS", FALLING_WALLS),
            ("FLAPS", FLAPS),
            ("FALLING_SEGMENTS", FALLING_SEGMENTS),
            ("FALLING_LETTERS", FALLING_LETTERS),
            ("MELTDOWN_CELLS", MELTDOWN_CELLS),
            ("EXPLOSION_CELLS", EXPLOSION_CELLS),
            ("AQUARIUM_LINER", AQUARIUM_LINER),
            ("AQUARIUM_DIGITS", AQUARIUM_DIGITS),
            ("FIXTURE_BALLS", FIXTURE_BALLS),
            ("SOIL", SOIL),
            ("TEST_OBSTACLES", TEST_OBSTACLES),
        ];

        fn overlaps(a: &Range<u64>, b: &Range<u64>) -> bool {
            a.start < b.end && b.start < a.end
        }

        #[test]
        fn canonical_entity_ranges_are_disjoint() {
            for (index, (name, range)) in CANONICAL.iter().enumerate() {
                assert!(range.start < range.end, "{name} is an empty id range");
                for (other_name, other) in &CANONICAL[index + 1..] {
                    let declared = ALIASES
                        .iter()
                        .any(|(a, b)| (a == range && b == other) || (a == other && b == range));
                    assert!(
                        !overlaps(range, other) || declared,
                        "canonical id ranges {name} ({range:?}) and {other_name} \
                         ({other:?}) overlap without a declared alias"
                    );
                }
            }
        }

        #[test]
        fn the_declared_aliases_are_the_ones_that_actually_overlap() {
            // An alias that stopped overlapping is a stale exemption, and would
            // hide a real collision if those ranges ever moved back together.
            for (a, b) in ALIASES {
                assert!(
                    overlaps(a, b),
                    "declared alias {a:?}/{b:?} no longer overlaps"
                );
            }
        }

        #[test]
        fn each_fixture_owner_has_its_own_share_of_the_fixtures_identities() {
            let mut next = FIXTURE_BALLS.start;
            for [share] in &FIXTURE_OWNER_BALLS {
                assert_eq!(share.start, next);
                assert_eq!(share.end - share.start, MAX_FIXTURE_BALLS as u64);
                next = share.end;
            }
            assert_eq!(next, FIXTURE_BALLS.end);
        }

        #[test]
        fn course_surfaces_and_their_bodies_convert_both_ways() {
            // The course producer, both ledge paths, supported_surface and
            // Meltdown's floor test all go through these three functions.
            for index in 0..MAX_SURFACES {
                let id = course_surface(index);
                assert_eq!(course_surface_index(id), Some(index));
                assert!(course_surfaces(index + 1).contains(&id.value()));
                assert!(!course_surfaces(index).contains(&id.value()));
            }
            for bank in DRAIN_BANKS {
                assert!(
                    course_surface_index(PhysicsId::new(bank)).is_some(),
                    "a drain bank must read as a course surface"
                );
            }
            for (name, range) in CANONICAL {
                if *range == COURSE || *range == DRAIN_BANKS {
                    continue;
                }
                for id in range.clone() {
                    assert_eq!(
                        course_surface_index(PhysicsId::new(id)),
                        None,
                        "{name} entity {id} reads as a course surface"
                    );
                }
            }
        }

        #[test]
        fn falling_gives_every_slot_and_segment_its_own_id_inside_its_ranges() {
            let mut seen = std::collections::BTreeSet::new();
            for slot in 0..DIGIT_SLOT_COUNT {
                for kind in SegmentKind::ALL {
                    let digit_slot = slot as u8;
                    let id = crate::physics::body_id(SegmentId { digit_slot, kind }).entity;
                    assert!(
                        FALLING_SEGMENTS.contains(&id.value()),
                        "slot {slot} {kind:?} falls outside the declared range"
                    );
                    assert!(seen.insert(id), "slot {slot} {kind:?} reuses {id:?}");
                }
                let id = crate::physics::letter_body_id(slot).entity;
                assert!(
                    FALLING_LETTERS.contains(&id.value()),
                    "letter {slot} falls outside the declared range"
                );
                assert!(seen.insert(id), "letter {slot} reuses {id:?}");
            }
        }
    }
}
