//! Moss succession on the floor a ceiling opening lit. The organism, its
//! surface supply and its shade tolerance all belong to `engine_core`; these
//! contracts cover the Clock's own admission, bounds and compatibility.
use super::*;
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState, ceiling::Phase,
    layout::Layout,
};
use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
use engine_core::plants::Role;
use std::time::Duration;

/// Opening, hold and closing of the reference sequence. Seed 4242 breaks one
/// opening in each corridor, so succession leaves two mats.
const CLOSED_AT: usize = 5 + 300 + 60 + 1;

fn fixture() -> ClockState {
    let mut s = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        4242,
    );
    ClockScenario::step(
        &mut s,
        &[
            ClockAction::set_reading(ClockReading::new(12, 34, 0).unwrap()),
            ClockAction::trigger_event(ClockEventKind::Grass),
        ],
        Duration::ZERO,
    );
    s
}
fn seconds(s: &mut ClockState, n: usize) {
    for _ in 0..n * 60 {
        ClockScenario::step(s, &[], Duration::from_nanos(16_666_667));
    }
}
/// `(organism, cell, health, shade ticks)` for every mat cell, living or dead.
fn mat(s: &ClockState) -> Vec<(u64, [i32; 2], u16, u32)> {
    s.plants()
        .plants
        .observe()
        .tissues
        .iter()
        .filter(|t| t.role == Role::Moss)
        .map(|t| (t.organism, t.cell, t.health, t.shade_ticks))
        .collect()
}
fn established() -> ClockState {
    let mut s = fixture();
    seconds(&mut s, CLOSED_AT);
    assert_eq!(s.ceiling.phase, Phase::Closed);
    s
}

#[test]
fn clock_moss_contract_succession_is_bounded_replayable_and_spares_the_grass_patch() {
    let mut s = fixture();
    seconds(&mut s, 60);
    // Nothing is admitted while the light still holds: the mat succeeds the
    // patch, so the grass bound is the one that decides the lit floor.
    assert_eq!(s.ceiling.phase, Phase::Open);
    assert!(mat(&s).is_empty());
    assert_eq!(s.grass_state().living_tissues, MAX_GRASS_TISSUES);
    assert_eq!(s.grass_state().moss_tissues, 0);

    seconds(&mut s, CLOSED_AT - 60);
    assert_eq!(s.ceiling.phase, Phase::Closed);
    let admitted = mat(&s);
    // One mat per opening, each its own organism, placed in the first air row
    // above the floor line.
    let organisms: std::collections::BTreeSet<_> = admitted.iter().map(|m| m.0).collect();
    assert_eq!(organisms.len(), 2);
    assert!((2..=4).contains(&admitted.len()), "{admitted:?}");
    assert!(
        admitted
            .iter()
            .all(|&(_, cell, health, shade)| cell[1] == MOSS_ROW && health == 100 && shade == 0)
    );
    // The grass patch keeps its own bound and its own published counts.
    assert_eq!(s.grass_state().living_tissues, MAX_GRASS_TISSUES);
    assert_eq!(s.grass_state().moss_tissues, admitted.len());

    // Paid spread, bounded by the mat budget rather than by the shared runtime
    // capacity, and confined to the corridors the openings were cut through.
    seconds(&mut s, 600);
    let grown = mat(&s);
    assert_eq!(grown.len(), MAX_MOSS_TISSUES);
    assert_eq!(s.grass_state().moss_tissues, MAX_MOSS_TISSUES);
    assert!(
        grown
            .iter()
            .all(|&(_, cell, health, _)| { cell[1] == MOSS_ROW && health == 100 })
    );
    let windows = s.plants().moss_columns.clone().expect("retained corridors");
    assert_eq!(windows.len(), crate::ceiling::CORRIDORS);
    assert!(grown.iter().all(|&(_, cell, _, _)| {
        windows
            .iter()
            .any(|&(low, high)| (low..=high).contains(&cell[0]))
    }));
    // The lit patch is gone by now, and the mat is what the floor retains.
    assert_eq!(s.grass_state().living_tissues, 0);
    assert!(s.plants().plants.observe().tissues.len() <= MAX_PLANT_TISSUES);

    let mut replay = fixture();
    seconds(&mut replay, CLOSED_AT + 600);
    assert_eq!(mat(&replay), grown);
    assert_eq!(replay.plants().moss_columns, s.plants().moss_columns);
}

#[test]
fn clock_moss_contract_room_light_sustains_darkness_declines_and_restored_light_recovers() {
    let mut s = established();
    // Spread first: exposure is published from the observation a tick opens
    // with, so a cell grown during that tick has no sample until the next one.
    seconds(&mut s, 600);
    assert_eq!(mat(&s).len(), MAX_MOSS_TISSUES);
    // The ordinary dim room saturates moss where it leaves a grass shoot far
    // below its own threshold, so a closed ceiling still pays the mat.
    assert_eq!(s.grass_state().intensity, 50);
    let o = s.plants().plants.observe();
    for t in o.tissues.iter().filter(|t| t.role == Role::Moss) {
        let exposure = s.plants().exposure.get(&t.id).expect("sampled mat cell");
        assert_eq!(exposure.light, o.config.moss_light_saturation);
        assert!(exposure.surface_contact);
        assert_eq!((t.health, t.shade_ticks, t.shortage_ticks), (100, 0, 0));
    }

    // Sustained darkness stresses the mat, and once the shared decline grace
    // is spent it starts taking damage.
    assert!(s.set_plant_conditions(0, [1000; 2]));
    seconds(&mut s, 10);
    assert!(mat(&s).iter().all(|&(_, _, _, shade)| shade > 0));
    assert!(s.grass_state().moss_stressed > 0);
    assert!(mat(&s).iter().all(|&(_, _, health, _)| health == 100));
    seconds(&mut s, 70);
    let hurt = mat(&s);
    assert!(
        hurt.iter()
            .all(|&(_, _, health, _)| (1..100).contains(&health))
    );

    // Restored room light clears the deficit and heals what survived.
    assert!(s.set_plant_conditions(50, [1000; 2]));
    seconds(&mut s, 30);
    let healed = mat(&s);
    assert_eq!(healed.len(), hurt.len());
    assert!(
        healed
            .iter()
            .all(|&(_, _, health, shade)| health == 100 && shade == 0)
    );
    assert_eq!(s.grass_state().moss_stressed, 0);

    // Darkness held long enough kills it, and no light brings a dead cell back.
    assert!(s.set_plant_conditions(0, [1000; 2]));
    seconds(&mut s, 600);
    assert_eq!(s.grass_state().moss_tissues, 0);
    assert!(s.set_plant_conditions(50, [1000; 2]));
    seconds(&mut s, 120);
    assert_eq!(s.grass_state().moss_tissues, 0);
}

#[test]
fn clock_moss_contract_occupancy_refuses_replacement_and_a_later_opening_reseeds_grass() {
    let mut s = established();
    seconds(&mut s, 600);
    let settled = mat(&s);
    assert_eq!(settled.len(), MAX_MOSS_TISSUES);
    assert_eq!(s.grass_state().living_tissues, 0);

    // A retained mat is not grass occupancy, so the next opening still
    // delivers its layer and still plants its batch.
    ClockScenario::step(
        &mut s,
        &[ClockAction::trigger_event(ClockEventKind::Grass)],
        Duration::ZERO,
    );
    assert_eq!(s.ceiling.phase, Phase::Opening);
    seconds(&mut s, 10);
    assert_eq!(s.ceiling.sequence, 2);
    assert_eq!(s.ceiling.deliveries, 2);
    assert_eq!(s.grass_state().seeded, SEED_BATCH);
    assert_eq!(s.grass_state().refusal, None);
    // The mats are untouched by the new delivery and the new batch.
    assert_eq!(s.grass_state().moss_tissues, MAX_MOSS_TISSUES);

    // Both organisms share the row above the floor line, and no cell ever
    // carries two tissues whichever one reached it first.
    seconds(&mut s, 300);
    let o = s.plants().plants.observe();
    let cells: std::collections::BTreeSet<_> = o.tissues.iter().map(|t| t.cell).collect();
    assert_eq!(cells.len(), o.tissues.len());
    assert!(o.tissues.iter().any(|t| t.role == Role::Shoot));
    assert_eq!(s.grass_state().moss_tissues, MAX_MOSS_TISSUES);

    // The lit window is rebuilt for the new sequence rather than widened onto
    // the last one. It is kept per opening index, so a stale window held open
    // by the mat would merge two unrelated openings into a span straddling the
    // face, across floor that neither of them ever lit.
    let layout = Layout::new(s.aspect_ratio());
    let corridors: Vec<(i32, i32)> = crate::ceiling::Ceiling::corridors(layout)
        .into_iter()
        .filter_map(|(left, right)| s.soil().unwrap().columns_within(left, right))
        .collect();
    let lit = s.plants().lit_columns.clone().expect("a live opening");
    assert!(
        lit.iter()
            .all(|w| corridors.iter().any(|c| c.0 <= w.0 && w.1 <= c.1)),
        "lit={lit:?} corridors={corridors:?}"
    );
    // Policies retire with the organism they belong to, so repeated events
    // cannot stack dead ones onto a list that is planned through every tick.
    assert!(s.plants().policies.len() <= residents::MAX_PLANT_ORGANISMS);

    // Closing again leaves the openings that already carry moss alone rather
    // than collecting a second mat on every visit.
    seconds(&mut s, CLOSED_AT);
    assert_eq!(s.ceiling.phase, Phase::Closed);
    assert_eq!(s.grass_state().moss_tissues, MAX_MOSS_TISSUES);
    let organisms: std::collections::BTreeSet<_> = mat(&s).iter().map(|m| m.0).collect();
    assert_eq!(organisms.len(), 2);

    // A mat lies a row above the soil, so a seed still goes into the dirt
    // beneath it. What the mat refuses is the shoot that seed would raise:
    // that cell is occupied, so there is no replacement and no recharge.
    let mut s = established();
    seconds(&mut s, 600);
    let occupied = mat(&s)[0].1;
    let resident = s
        .plants()
        .plants
        .observe()
        .tissues
        .iter()
        .find(|t| t.cell == occupied)
        .cloned()
        .expect("a settled mat cell");
    s.plant_grass(occupied[0]).expect("dirt beneath the mat");
    seconds(&mut s, 120);
    let still = s
        .plants()
        .plants
        .observe()
        .tissues
        .iter()
        .find(|t| t.cell == occupied)
        .cloned()
        .expect("the mat still holds its cell");
    assert_eq!(
        (still.id, still.organism, still.role, still.health),
        (resident.id, resident.organism, Role::Moss, 100)
    );
    assert_eq!(s.grass_state().moss_tissues, MAX_MOSS_TISSUES);
}

#[test]
fn clock_moss_contract_contact_damage_support_loss_and_replacement_retire_the_mat() {
    let mut s = established();
    seconds(&mut s, 600);
    let settled = mat(&s);
    assert_eq!(settled.len(), MAX_MOSS_TISSUES);

    // A silhouette over the mat damages it on the same path a crow or a duck
    // uses. Roots stay protected by actual soil; a mat has none above it.
    let cell = settled[0].1;
    let center = s.soil().unwrap().world_point(cell).unwrap();
    let half = SOIL_CELL_SIZE * 0.5;
    let square = vec![
        center + Vec2::new(-half, -half),
        center + Vec2::new(half, -half),
        center + Vec2::new(half, half),
        center + Vec2::new(-half, half),
    ];
    let soil = s.soil.take().unwrap();
    s.plants.crush_polygons(&soil, &[square]);
    s.soil = Some(soil);
    assert_eq!(s.grass_state().moss_tissues, MAX_MOSS_TISSUES - 1);
    assert!(mat(&s).iter().all(|&(_, at, _, _)| at != cell));

    // Losing the floor retires the rest through the same bounded path as roots.
    let layout = Layout::new(s.aspect_ratio());
    let soil = s.soil.as_mut().unwrap();
    soil.set_surfaces(Vec::new(), SoilSupportState::Drain);
    let added = soil.accounting().added;
    for _ in 0..600 {
        if soil.accounting().exported == added {
            break;
        }
        soil.advance_loss(Duration::from_millis(250));
    }
    assert_eq!(soil.accounting().exported, added);
    soil.set_surfaces(
        surfaces::ordinary(crate::floor::FloorGeometry::closed(layout)),
        SoilSupportState::Ordinary,
    );
    s.plants.revalidate(s.soil.as_ref().unwrap());
    assert_eq!(s.grass_state().moss_tissues, 0);
    assert!(s.plants().plants.observe().tissues.is_empty());

    // Water and meltdown replacement retire the opening, not the material.
    // A mat is committed state on committed soil, and the dim room still feeds
    // it, so it stays exactly where the lit grass cannot: measured over two
    // further minutes, with no soil exported by either event.
    for kind in [ClockEventKind::Aquarium, ClockEventKind::Meltdown] {
        let mut s = established();
        seconds(&mut s, 600);
        let before = s.grass_state().moss_tissues;
        assert_eq!(before, MAX_MOSS_TISSUES);
        ClockScenario::step(&mut s, &[ClockAction::preview_event(kind)], Duration::ZERO);
        assert_eq!(s.ceiling.phase, Phase::Closed);
        assert!(s.plants.apertures.is_empty());
        seconds(&mut s, 120);
        assert_eq!(s.grass_state().moss_tissues, before, "{kind:?}");
        assert_eq!(s.grass_state().moss_dead, 0, "{kind:?}");
        assert_eq!(s.soil().unwrap().accounting().exported, 0, "{kind:?}");
    }

    // A duck crossing takes mat cells, through its own contact and through the
    // soil its visit exports, and the mat regrows from whatever survived.
    let mut s = established();
    seconds(&mut s, 600);
    ClockScenario::step(
        &mut s,
        &[ClockAction::preview_event(ClockEventKind::Duck)],
        Duration::ZERO,
    );
    seconds(&mut s, 30);
    let trampled = s.grass_state().moss_tissues;
    assert!(trampled < MAX_MOSS_TISSUES, "{trampled}");
    assert!(s.soil().unwrap().accounting().exported > 0);
    seconds(&mut s, 180);
    assert!(s.grass_state().moss_tissues > trampled);
}

#[test]
fn clock_moss_contract_cancelled_disabled_and_restarted_ceilings_leave_no_mat() {
    // A cancelled ceiling never reaches the closing boundary succession runs at.
    let mut s = fixture();
    seconds(&mut s, 60);
    s.cancel_ceiling();
    assert_eq!(s.ceiling.phase, Phase::Closed);
    seconds(&mut s, CLOSED_AT);
    assert!(mat(&s).is_empty());
    assert_eq!(s.plants().moss_columns, None);

    // Disabled future seeding refuses the mat as it refuses the seed batch.
    let mut s = fixture();
    let mut settings = s.settings();
    settings.events.grass = false;
    ClockScenario::step(&mut s, &[ClockAction::configure(settings)], Duration::ZERO);
    seconds(&mut s, CLOSED_AT);
    assert!(mat(&s).is_empty());
    assert_eq!(s.plants().moss_columns, None);

    // A restart keeps nothing: the mat is retained state, not a saved setting.
    let mut s = established();
    seconds(&mut s, 120);
    assert!(s.grass_state().moss_tissues > 0);
    let fresh = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        4242,
    );
    assert!(mat(&fresh).is_empty());
    assert_eq!(fresh.plants().moss_columns, None);
    assert_eq!(fresh.grass_state().moss_tissues, 0);
}

/// A 4:3 Clock at the cabinet's own reading and launch seed. Grass is requested
/// at tick 0 and the state returned at the second before the ceiling closes.
fn picade_fixture(seed: u64) -> ClockState {
    let mut s = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            aspect_ratio: 1024.0 / 768.0,
            ..Default::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut s,
        &[
            ClockAction::set_reading(ClockReading::new(19, 42, 0).unwrap()),
            ClockAction::trigger_event(ClockEventKind::Grass),
        ],
        Duration::ZERO,
    );
    seconds(&mut s, CLOSED_AT - 2);
    assert_eq!(s.ceiling.phase, Phase::Closing);
    s
}
/// Column ranges of the lit spans, and every column whose mat row is occupied.
fn spans_and_occupied(s: &ClockState) -> (Vec<(i32, i32)>, Vec<i32>) {
    let layout = Layout::new(s.aspect_ratio());
    let soil = s.soil.as_ref().expect("soil");
    let spans = s
        .ceiling
        .lit_spans(layout)
        .iter()
        .filter_map(|&(left, right)| soil.columns_within(left, right))
        .collect();
    let occupied = s
        .plants()
        .plants
        .observe()
        .tissues
        .iter()
        .filter(|t| t.cell[1] == MOSS_ROW)
        .map(|t| t.cell[0])
        .collect();
    (spans, occupied)
}

/// The sequence measured on `sw-picade` on 2026-11-02: one opening seven
/// columns wide, and the batch raises a shoot in every one of them, so the
/// near edge is occupied when the ceiling closes. The mat is seated in the
/// nearest free column beside the span instead of being refused outright.
#[test]
fn clock_moss_contract_a_filled_span_seats_its_mat_beside_the_patch() {
    let mut s = picade_fixture(12616578969279246616);
    let (spans, occupied) = spans_and_occupied(&s);
    assert_eq!(spans.len(), 1, "{spans:?}");
    let (low, high) = spans[0];
    assert!(
        (low..=high).all(|x| occupied.contains(&x)),
        "{low}..={high} against {occupied:?}"
    );
    assert!(mat(&s).is_empty());

    seconds(&mut s, 2);
    assert_eq!(s.ceiling.phase, Phase::Closed);
    let admitted = mat(&s);
    assert_eq!(admitted.len(), 1, "{admitted:?}");
    let (_, cell, health, shade) = admitted[0];
    assert_eq!(cell[1], MOSS_ROW);
    assert_eq!((health, shade), (100, 0));
    assert!(
        cell[0] == low - 1 || cell[0] == high + 1,
        "{cell:?} is not beside {low}..={high}"
    );
    let windows = s.plants().moss_columns.clone().expect("retained corridors");
    let corridor = windows
        .iter()
        .find(|&&(start, end)| start <= low && high <= end)
        .copied()
        .expect("the span's corridor");
    assert!((corridor.0..=corridor.1).contains(&cell[0]));
    assert_eq!(s.grass_state().moss_tissues, 1);
    assert_eq!(s.grass_state().living_tissues, 28);

    // From there the mat spreads along its corridor like any other, to every
    // column of it: at 4:3 the corridor is narrower than the 18-cell bound.
    seconds(&mut s, 600);
    let grown = mat(&s);
    let width = (corridor.1 - corridor.0 + 1) as usize;
    assert!(width < MAX_MOSS_TISSUES, "{corridor:?}");
    assert_eq!(grown.len(), width, "{grown:?}");
    assert!(grown.iter().all(|&(_, cell, health, _)| {
        cell[1] == MOSS_ROW && health == 100 && (corridor.0..=corridor.1).contains(&cell[0])
    }));
    assert_eq!(s.grass_state().moss_tissues, width);
    assert_eq!(s.grass_state().living_tissues, 0);
}

/// Twelve seeds at 4:3, where the corridors seat spans of three to twelve
/// columns and the batch fills most of them to the edge: every opening seats
/// one mat, in a free column of its own corridor.
#[test]
fn clock_moss_contract_four_by_three_openings_each_seat_a_mat() {
    let mut filled_edges = 0;
    for seed in 1..=12 {
        let mut s = picade_fixture(seed);
        let (spans, occupied) = spans_and_occupied(&s);
        filled_edges += spans
            .iter()
            .filter(|(low, _)| occupied.contains(low))
            .count();
        seconds(&mut s, 2);
        assert_eq!(s.ceiling.phase, Phase::Closed);
        let admitted = mat(&s);
        let organisms: std::collections::BTreeSet<_> = admitted.iter().map(|m| m.0).collect();
        assert_eq!(
            organisms.len(),
            spans.len(),
            "seed {seed}: {spans:?} {admitted:?}"
        );
        let windows = s.plants().moss_columns.clone().expect("retained corridors");
        for &(_, cell, health, shade) in &admitted {
            assert_eq!(cell[1], MOSS_ROW);
            assert_eq!((health, shade), (100, 0));
            assert!(!occupied.contains(&cell[0]), "seed {seed}: {cell:?}");
            assert!(
                windows
                    .iter()
                    .any(|&(start, end)| (start..=end).contains(&cell[0]))
            );
        }
        // A mat may already have paid for a first neighbour at the boundary.
        assert!(
            admitted.len() <= 2 * spans.len(),
            "seed {seed}: {admitted:?}"
        );
        assert_eq!(s.grass_state().moss_tissues, admitted.len());
    }
    // The rule is exercised: before it, each of these spans kept no mat.
    assert!(filled_edges >= 8, "{filled_edges} filled near edges");
}
