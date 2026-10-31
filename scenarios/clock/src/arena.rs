//! Who owns which physical identity in the Clock.
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

use engine_rapier::world::PhysicsWorld;

use crate::events::ActiveEvent;

/// The one rigid world this run is simulating, whoever is using it.
///
/// That is the run's `arena` whenever a visit holds it or an event leases it,
/// and otherwise a standalone event's own world: Falling's or Explosion's
/// before a player joins, Meltdown's water lab, or Rain's floats. `visit` says
/// whether a visit exists, because Rain's floats stop being canonical as soon
/// as one does, before its first step has built the arena. The parts are taken
/// as separate borrows rather than through `ClockState` because callers hold
/// the soil and the plants at the same time.
pub(crate) fn canonical_world<'a>(
    event: &'a Option<ActiveEvent>,
    visit: bool,
    arena: &'a Option<PhysicsWorld>,
) -> Option<&'a PhysicsWorld> {
    match event {
        Some(event) if event.simulates_apart(visit) => event.own_world(),
        _ => arena.as_ref(),
    }
}

pub(crate) fn canonical_world_mut<'a>(
    event: &'a mut Option<ActiveEvent>,
    visit: bool,
    arena: &'a mut Option<PhysicsWorld>,
) -> Option<&'a mut PhysicsWorld> {
    match event {
        Some(event) if event.simulates_apart(visit) => event.own_world_mut(),
        _ => arena.as_mut(),
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
