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

pub(crate) mod ids {
    /// The one shared arena, which the visit and its leasing events all insert
    /// into. Disjoint except for the aliases noted below, which the
    /// `canonical_entity_ranges_are_disjoint` test holds.
    pub(crate) mod canonical {
        use std::ops::Range;

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
    }

    #[cfg(test)]
    mod tests {
        use std::ops::Range;

        use super::canonical::*;
        use crate::{
            digits::{DIGIT_SLOT_COUNT, SegmentKind},
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
        fn a_course_surface_index_round_trips_through_its_body_id() {
            // Four sites convert one of these into the other; this is their
            // shared premise, including for the aliased drain banks.
            for index in 0..MAX_SURFACES {
                let id = COURSE.start + index as u64;
                assert!(COURSE.contains(&id));
                assert_eq!(id.checked_sub(COURSE.start), Some(index as u64));
            }
            for bank in DRAIN_BANKS {
                assert!(
                    COURSE.contains(&bank),
                    "a drain bank must read as a course surface"
                );
            }
        }

        #[test]
        fn falling_covers_every_slot_and_segment_it_allocates() {
            for slot in 0..DIGIT_SLOT_COUNT {
                for kind in 0..SegmentKind::ALL.len() {
                    let id = 3001 + (slot * SegmentKind::ALL.len() + kind) as u64;
                    assert!(
                        FALLING_SEGMENTS.contains(&id),
                        "slot {slot} segment {kind} falls outside the declared range"
                    );
                }
                assert!(FALLING_LETTERS.contains(&(3032 + slot as u64)));
            }
        }
    }
}
