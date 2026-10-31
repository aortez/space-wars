//! The run's shared rigid arena, and who owns which physical identity in it.
//!
//! A visit, the events that lease its arena and the run's residents all
//! simulate in one world, [`ClockArena`]. The world exists exactly while an
//! [`Owner`] holds it: the first owner to need it builds it, and the last to
//! let go takes it away. An owner that lets go while others stay takes its own
//! bodies with it, by the identities [`ids::canonical`] gives it.
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

use std::ops::Range;

use engine_common::ClockEventKind;
use engine_core::Vec2;
use engine_rapier::world::{PhysicsId, PhysicsStepMetrics, PhysicsWorld, PhysicsWorldConfig};

use crate::{
    events::{ActiveEvent, duck::ArenaTerrain},
    grass::SoilHabitat,
    layout::Layout,
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
}

impl Owner {
    /// A resident keeps the arena for the rest of the run, whoever else comes
    /// and goes.
    fn resident(self) -> bool {
        matches!(self, Self::Soil)
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
        }
    }
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

/// What the arena holds now, and what it has done over the run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaAccounting {
    /// Owners holding the arena now.
    pub claims: usize,
    /// Those of them that are residents.
    pub residents: usize,
    pub bodies: usize,
    pub colliders: usize,
    pub joints: usize,
    /// Claims granted over the run.
    pub granted: u64,
    /// Claims let go over the run.
    pub released: u64,
    /// Rigid steps over the run. Exactly one per positive tick while a world
    /// exists.
    pub steps: u64,
    /// Worlds the arena has held over the run.
    pub worlds: u64,
}

/// The settings every arena world is built with. Whoever builds it chooses its
/// gravity and the solver's length scale, and a visit that enters it later
/// sets its own.
pub(crate) fn world_config(gravity: Vec2, length_unit: f32) -> PhysicsWorldConfig {
    PhysicsWorldConfig {
        gravity,
        length_unit,
        max_ccd_substeps: 2,
        collect_events: false,
        ..PhysicsWorldConfig::default()
    }
}

/// The settings of a world no visit has entered: a standalone Falling or
/// Explosion world, or the one residents build. Clock's own mechanics are
/// tuned to this gravity.
pub(crate) fn mechanics_config(layout: Layout) -> PhysicsWorldConfig {
    world_config(Vec2::new(0.0, -400.0), layout.pitch)
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
    /// The last step's timings and populations.
    metrics: PhysicsStepMetrics,
    granted: u64,
    released: u64,
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

    /// `owner` is the first to need a world and brings `world` with it.
    pub fn build(&mut self, owner: Owner, world: PhysicsWorld) -> &mut PhysicsWorld {
        assert!(self.idle(), "the arena has one world");
        self.owners.push(owner);
        self.granted += 1;
        self.worlds += 1;
        self.world.insert(world)
    }

    /// `owner` joins the world another owner built.
    pub fn claim(&mut self, owner: Owner) -> &mut PhysicsWorld {
        assert!(!self.holds(owner), "{owner:?} already holds the arena");
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
        if let Some(mut kept) = self.world.take() {
            let entities: Vec<_> = kept.entity_ids().collect();
            for entity in entities {
                assert!(
                    kept.transfer_entity(entity, &mut world),
                    "{entity:?} moves into the adopted world"
                );
            }
        }
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
        } else if held {
            let world = self.world.as_mut().expect("a held arena");
            for range in owner.ranges() {
                for id in range.clone() {
                    world.remove_entity(PhysicsId::new(id));
                }
            }
        }
    }

    /// The vacant terrain and the world it stands in, borrowed together.
    pub fn vacant_with_world(&mut self) -> Option<(&mut ArenaTerrain, &mut PhysicsWorld)> {
        Some((self.vacant.as_mut()?, self.world.as_mut()?))
    }

    /// The arena's one rigid step this tick, whoever holds it, residents with
    /// nobody else included. Nothing else steps the arena's world.
    pub fn step(&mut self, dt: f32) {
        if let Some(world) = &mut self.world {
            self.metrics = world.step(dt);
            self.steps += 1;
        }
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
            steps: self.steps,
            worlds: self.worlds,
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

pub(crate) mod ids {
    /// The one shared arena, which the visit and its leasing events all insert
    /// into. Disjoint except for the aliases noted below, which the
    /// `canonical_entity_ranges_are_disjoint` test holds.
    pub(crate) mod canonical {
        use std::ops::Range;

        use engine_rapier::world::PhysicsId;

        use crate::{
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
        /// Retained soil, the one resident today that outlives its admitting
        /// event.
        pub(crate) const SOIL: Range<u64> = 900_001..900_002;

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
            ("SOIL", SOIL),
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
