//! Weapon geometry (#329): blast fronts and beam cross-sections divided among
//! targets, nearer targets shadowing farther ones, sweeps listing what a shot
//! touches, footprints sharing a load, and the chain from geometry through a
//! source into plant tissue.

use engine_core::Vec2;
use engine_core::plants::{Config, Environment, Exposure, Plants, Role, Support, Tissue, TissueId};
use engine_material::fixtures::{TICK, shell, tank};
use engine_material::{
    ContactBody, Crossing, Face, Impact, MaterialError, Outcome, Overlap, Projectile, Pulses,
    Reach, Region, SOFT_TISSUE, Section, Share, Source, SourceSpec, SustainedLoad, Target,
    TissueIntegrity, TissueMaterial, TissueTable, WOOD, beam_reach, blast_center, blast_reach,
    deformation_depth, effective_mass, footprint_overlaps, sweep_crossings,
};

fn target(key: u64, x: f32, y: f32, radius: f32, thickness: f32) -> Target {
    Target {
        key,
        center: Vec2::new(x, y),
        radius,
        thickness,
    }
}

fn coupling(reaches: &[Reach], key: u64) -> f32 {
    reaches
        .iter()
        .find(|reach| reach.key == key)
        .map_or(0.0, |reach| reach.coupling)
}

fn width(reaches: &[Reach], key: u64) -> f32 {
    reaches
        .iter()
        .find(|reach| reach.key == key)
        .map_or(0.0, |reach| reach.width)
}

fn total(reaches: &[Reach]) -> f64 {
    reaches.iter().map(|reach| f64::from(reach.coupling)).sum()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0e-6
}

/// The fraction of a blast's directions that a circle of `radius` at
/// `distance` subtends.
fn subtends(radius: f32, distance: f32) -> f32 {
    (radius / distance).asin() / std::f32::consts::PI
}

const ORIGIN: Vec2 = Vec2::ZERO;

#[test]
fn a_lone_target_takes_the_directions_it_subtends() {
    // Ahead, behind, and just below the axis, where its directions wrap
    // around both ends of the turn.
    for (x, y) in [(5.0, 0.0), (-5.0, 0.0), (4.999, -0.1), (0.0, 5.0)] {
        let distance = Vec2::new(x, y).length();
        let plate = blast_reach(ORIGIN, 10.0, 1.0, &[target(1, x, y, 1.0, 1.0)]).unwrap();
        assert!(
            close(coupling(&plate, 1), subtends(1.0, distance)),
            "{x} {y}"
        );
        assert!(close(width(&plate, 1), 2.0));
        // A hull thicker than the front takes no more than the plate.
        let hull = blast_reach(ORIGIN, 10.0, 1.0, &[target(1, x, y, 1.0, 3.0)]).unwrap();
        assert_eq!(hull, plate);
        // A blade a quarter of the front's height takes a quarter of that.
        let blade = blast_reach(ORIGIN, 10.0, 1.0, &[target(1, x, y, 1.0, 0.25)]).unwrap();
        assert!(close(coupling(&blade, 1), 0.25 * subtends(1.0, distance)));
    }
}

#[test]
fn a_crowd_divides_the_blast_and_cannot_multiply_it() {
    // Twelve plates, staggered in two rings so that none touches another,
    // cover every direction between them.
    let ring: Vec<Target> = (0..12)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::TAU / 12.0;
            let distance = if i % 2 == 0 { 3.0 } else { 4.5 };
            target(i, distance * angle.cos(), distance * angle.sin(), 1.0, 1.0)
        })
        .collect();
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &ring).unwrap();
    assert_eq!(reaches.len(), 12);
    // Rounding to f32 stays well within a source's allowance.
    assert!((total(&reaches) - 1.0).abs() <= 1.0e-7);
    // The inner ring takes all it subtends, and the outer only the gaps.
    let gap = 1.0 / 6.0 - subtends(1.0, 3.0);
    for reach in &reaches {
        let expected = if reach.key % 2 == 0 {
            subtends(1.0, 3.0)
        } else {
            gap
        };
        assert!(close(reach.coupling, expected), "{reach:?}");
    }
    // A hundred more behind them get nothing, and the rings keep their
    // shares.
    let mut crowd = ring.clone();
    crowd.extend((0..100).map(|i| {
        let angle = i as f32 * std::f32::consts::TAU / 100.0;
        target(100 + i, 7.0 * angle.cos(), 7.0 * angle.sin(), 0.2, 1.0)
    }));
    let shielded = blast_reach(ORIGIN, 10.0, 1.0, &crowd).unwrap();
    assert_eq!(shielded.len(), 12);
    for (shielded, reach) in shielded.iter().zip(&reaches) {
        assert_eq!(shielded.key, reach.key);
        assert!(close(shielded.coupling, reach.coupling));
    }
    // Rings of thin blades take no more than their thickness allows.
    let blades: Vec<Target> = ring
        .iter()
        .map(|plate| Target {
            thickness: 0.1,
            ..*plate
        })
        .collect();
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &blades).unwrap();
    assert!(total(&reaches) < 0.2);
}

#[test]
fn nearer_thick_targets_shadow_farther_ones() {
    // Both subtend thirty degrees each side of the same direction.
    let near = target(1, 2.0, 0.0, 1.0, 1.0);
    let behind = target(2, 6.0, 0.0, 3.0, 1.0);
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &[near, behind]).unwrap();
    assert_eq!(reaches.len(), 1);
    assert!(close(coupling(&reaches, 1), 1.0 / 6.0));
    // Turned thirty degrees, half the far plate shows past the near one.
    let (sin, cos) = 30.0f32.to_radians().sin_cos();
    let aside = target(2, 6.0 * cos, 6.0 * sin, 3.0, 1.0);
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &[near, aside]).unwrap();
    assert!(close(coupling(&reaches, 2), 1.0 / 12.0));
    assert!((width(&reaches, 2) - 3.0).abs() < 1.0e-5);
    // Across the zero direction too: a plate just below the axis hides one
    // farther along it.
    let below = target(1, 4.999, -0.1, 1.0, 1.0);
    let far = target(2, 9.0, 0.0, 1.0, 1.0);
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &[below, far]).unwrap();
    assert_eq!(reaches.len(), 1);
    assert!(close(coupling(&reaches, 1), subtends(1.0, 5.0)));
}

#[test]
fn energy_meets_targets_in_the_order_each_line_reaches_them() {
    // A blade 28 degrees off the plate's axis stands in front of its curved
    // face, which that line reaches at 4.27 wu, though the plate's nearest
    // point, at 3 wu, is nearer than the blade's.
    let plate = target(1, 6.0, 0.0, 3.0, 1.0);
    let (sin, cos) = 28.0f32.to_radians().sin_cos();
    let blade = target(2, 3.9 * cos, 3.9 * sin, 0.1, 0.2);
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &[plate, blade]).unwrap();
    let blade_share = 0.2 * subtends(0.1, 3.9);
    assert!(close(coupling(&reaches, 2), blade_share));
    assert!(close(
        coupling(&reaches, 1),
        subtends(3.0, 6.0) - blade_share
    ));
    // Along a beam, the same holds where a wall's face curves away from it.
    let wall = target(1, 5.0, 0.0, 3.0, 1.0);
    let blade = target(2, 2.02, 0.45, 0.01, 0.2);
    let path = beam_reach(ORIGIN, Vec2::new(10.0, 0.0), 1.0, 1.0, &[wall, blade]).unwrap();
    assert!(close(coupling(&path.reaches, 2), 0.2 * 0.02));
    assert!(close(coupling(&path.reaches, 1), 1.0 - 0.2 * 0.02));
}

#[test]
fn thin_tissue_takes_its_share_and_passes_the_rest() {
    let blade = target(1, 2.0, 0.0, 1.0, 0.2);
    let plate = target(2, 6.0, 0.0, 3.0, 1.0);
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &[blade, plate]).unwrap();
    assert!(close(coupling(&reaches, 1), 0.2 / 6.0));
    assert!(close(coupling(&reaches, 2), 0.8 / 6.0));
    // Alone, the plate takes the blade's share as well.
    let alone = blast_reach(ORIGIN, 10.0, 1.0, &[plate]).unwrap();
    assert!(close(coupling(&alone, 2), 1.0 / 6.0));
}

#[test]
fn the_blast_radius_bounds_its_reach() {
    let beyond = target(1, 12.0, 0.0, 1.0, 1.0);
    // Straddling the edge, it takes only the directions that meet it within
    // the radius: those inside the angle where the edge crosses its circle.
    let edge = target(2, 0.0, 10.5, 1.0, 1.0);
    // Lines that graze it meet it within the radius, so it takes all it
    // subtends.
    let inside = target(3, 0.0, -9.0, 1.0, 1.0);
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &[beyond, edge, inside]).unwrap();
    assert_eq!(reaches.len(), 2);
    let half = ((10.5f64 * 10.5 + 10.0 * 10.0 - 1.0) / (2.0 * 10.5 * 10.0)).acos();
    let subtended = (1.0f64 / 10.5).asin();
    assert!(close(
        coupling(&reaches, 2),
        (half / std::f64::consts::PI) as f32
    ));
    assert!(close(width(&reaches, 2), (2.0 * half / subtended) as f32));
    assert!(close(coupling(&reaches, 3), subtends(1.0, 9.0)));
    assert!(close(width(&reaches, 3), 2.0));
    // Leaving the blast, it takes next to nothing.
    let leaving = target(4, 10.9999, 0.0, 1.0, 1.0);
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &[leaving]).unwrap();
    assert!(coupling(&reaches, 4) < 0.02 * subtends(1.0, 10.9999));
}

#[test]
fn a_blast_inside_a_target_meets_it_first_everywhere() {
    let hull = target(1, 0.2, 0.0, 1.0, 1.0);
    let outside = target(2, 5.0, 0.0, 1.0, 1.0);
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &[hull, outside]).unwrap();
    assert_eq!(
        reaches,
        vec![Reach {
            key: 1,
            coupling: 1.0,
            width: 2.0
        }]
    );
    // Inside a blade, the blade takes its thickness's share everywhere.
    let blade = Target {
        thickness: 0.3,
        ..hull
    };
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &[blade, outside]).unwrap();
    assert!(close(coupling(&reaches, 1), 0.3));
    assert!(close(coupling(&reaches, 2), 0.7 * subtends(1.0, 5.0)));
}

#[test]
fn a_blast_bound_inside_a_target_starts_just_past_its_edge() {
    let wheel = target(1, 0.0, 0.0, 0.5, 1.0);
    let hull = target(2, 3.0, 0.0, 1.0, 1.0);
    let open = Vec2::new(0.0, 2.0);
    assert_eq!(blast_center(open, &[wheel, hull]), open);
    // Out from the wheel's center through the point the blast was bound for.
    let moved = blast_center(Vec2::new(0.3, 0.0), &[wheel, hull]);
    assert!(close(moved.x, 0.5 * 1.001) && moved.y == 0.0);
    // From there the wheel no longer takes every direction, and the hull
    // takes a share of the rest.
    let reaches = blast_reach(moved, 10.0, 1.0, &[wheel, hull]).unwrap();
    assert!((0.45..0.5).contains(&coupling(&reaches, 1)));
    assert!(coupling(&reaches, 2) > 0.0);
    let centered = blast_center(ORIGIN, &[wheel]);
    assert!(centered.x == 0.0 && close(centered.y, 0.5 * 1.001));
}

/// Whether any of `targets` holds `point`.
fn held(point: Vec2, targets: &[Target]) -> bool {
    targets
        .iter()
        .any(|target| point.distance_to(target.center) <= target.radius)
}

#[test]
fn a_blast_bound_inside_overlapping_targets_starts_outside_them_all() {
    // A hull of three overlapping circles. Straight out of the first from a
    // point it shares with the second lands inside the second, and straight
    // out of the second lands back inside the first.
    let hull = [-1.1, 0.0, 1.1].map(|x| target(1, x, 0.0, 0.7, 1.0));
    let bound = Vec2::new(-0.5, 0.0);
    let moved = blast_center(bound, &hull);
    assert!(!held(moved, &hull), "{moved:?}");
    // It leaves through the nearest edge, the hull's side rather than an end.
    assert!(moved.distance_to(bound) < 0.75, "{moved:?}");
    // Two bodies that touch hold a center in the crevice between them, which
    // it leaves where their edges cross.
    let pair = [target(1, 0.0, 0.0, 2.0, 1.0), target(2, 2.4, 0.0, 0.5, 1.0)];
    let crevice = Vec2::new(1.95, 0.0);
    let moved = blast_center(crevice, &pair);
    assert!(!held(moved, &pair), "{moved:?}");
    assert!(moved.distance_to(crevice) < 0.6, "{moved:?}");
}

#[test]
fn a_recipients_circles_count_once() {
    let reaches = blast_reach(
        ORIGIN,
        10.0,
        1.0,
        &[
            target(5, 5.0, 0.0, 1.0, 1.0),
            target(5, -5.0, 0.0, 1.0, 1.0),
        ],
    )
    .unwrap();
    assert_eq!(reaches.len(), 1);
    assert!(close(coupling(&reaches, 5), 2.0 * subtends(1.0, 5.0)));
    assert!(close(width(&reaches, 5), 4.0));
    // A thin recipient drawn as two overlapping circles takes from each line
    // once, as its one circle would.
    let one = [target(5, 5.0, 0.0, 0.5, 0.2)];
    let two = [target(5, 5.0, 0.0, 0.5, 0.2), target(5, 5.1, 0.0, 0.5, 0.2)];
    let blast = |targets: &[Target]| blast_reach(ORIGIN, 10.0, 1.0, targets).unwrap();
    assert!(close(coupling(&blast(&two), 5), coupling(&blast(&one), 5)));
    assert!(close(width(&blast(&two), 5), width(&blast(&one), 5)));
    let end = Vec2::new(10.0, 0.0);
    let beam = beam_reach(ORIGIN, end, 1.0, 1.0, &two).unwrap();
    assert!(close(coupling(&beam.reaches, 5), 0.2));
    assert!(close(width(&beam.reaches, 5), 1.0));
    assert!(close(beam.through, 0.8));
}

#[test]
fn a_beam_divides_its_width_and_passes_the_rest_to_its_end() {
    let end = Vec2::new(10.0, 0.0);
    // One blade covers a fifth of the beam's width; another, behind it, a
    // fifth too, with three quarters of that in the first one's shadow.
    let first = target(1, 3.0, 0.0, 0.1, 0.2);
    let second = target(2, 5.0, 0.05, 0.1, 0.2);
    let path = beam_reach(ORIGIN, end, 1.0, 1.0, &[second, first]).unwrap();
    assert!(close(coupling(&path.reaches, 1), 0.2 * 0.2));
    assert!(close(
        coupling(&path.reaches, 2),
        0.15 * 0.2 * 0.8 + 0.05 * 0.2
    ));
    assert!(close(width(&path.reaches, 1), 0.2));
    assert!(close(width(&path.reaches, 2), 0.2));
    assert!(close(path.through, 1.0 - 0.04 - 0.034));
    assert!((total(&path.reaches) + f64::from(path.through) - 1.0).abs() <= 1.0e-7);
}

#[test]
fn a_thick_target_across_the_beam_shadows_what_is_behind() {
    let end = Vec2::new(10.0, 0.0);
    let blade = target(1, 2.0, 0.0, 0.1, 0.2);
    let wall = target(2, 4.0, 0.0, 1.0, 1.0);
    let hidden = target(3, 6.0, 0.0, 0.1, 0.2);
    let path = beam_reach(ORIGIN, end, 1.0, 1.0, &[hidden, wall, blade]).unwrap();
    assert_eq!(path.reaches.len(), 2);
    assert!(close(coupling(&path.reaches, 1), 0.04));
    assert!(close(coupling(&path.reaches, 2), 0.96));
    assert!(close(width(&path.reaches, 2), 1.0));
    assert_eq!(path.through, 0.0);
    // A wide wall meets the beam at its near side, before a blade nearer its
    // center.
    let wide = target(2, 5.0, 0.0, 3.0, 1.0);
    let after = target(3, 2.5, 0.0, 0.1, 0.2);
    let path = beam_reach(ORIGIN, end, 1.0, 1.0, &[after, wide]).unwrap();
    assert_eq!(path.reaches.len(), 1);
    assert!(close(coupling(&path.reaches, 2), 1.0));
}

#[test]
fn targets_off_the_beam_get_nothing() {
    let end = Vec2::new(10.0, 0.0);
    let off = [
        target(1, 5.0, 1.0, 0.4, 1.0),
        target(2, -0.5, 0.0, 0.4, 1.0),
        target(3, 10.5, 0.0, 0.4, 1.0),
    ];
    let path = beam_reach(ORIGIN, end, 1.0, 1.0, &off).unwrap();
    assert!(path.reaches.is_empty());
    assert_eq!(path.through, 1.0);
    // One that straddles the end is on the path.
    let straddling = target(4, 10.2, 0.0, 0.4, 1.0);
    let path = beam_reach(ORIGIN, end, 1.0, 1.0, &[straddling]).unwrap();
    assert!(close(coupling(&path.reaches, 4), 0.8));
    // A beam stopped at its muzzle has no path.
    let path = beam_reach(ORIGIN, ORIGIN, 1.0, 1.0, &[straddling]).unwrap();
    assert!(path.reaches.is_empty());
    assert_eq!(path.through, 1.0);
}

#[test]
fn a_sweep_lists_what_the_shot_touches_in_order() {
    let end = Vec2::new(10.0, 0.0);
    let targets = [
        target(3, 8.0, 0.0, 0.1, 0.2),
        target(1, 2.0, 0.15, 0.1, 0.2),
        target(2, 5.0, 0.0, 0.1, 0.2),
        // A second circle of the same recipient, touched later.
        target(2, 6.0, 0.0, 0.1, 0.2),
        target(4, 5.0, 1.0, 0.1, 0.2),
        target(5, -1.0, 0.0, 0.1, 0.2),
        target(6, 10.5, 0.0, 0.1, 0.2),
        target(7, 0.1, 0.0, 0.1, 0.2),
    ];
    let crossings = sweep_crossings(ORIGIN, end, 0.1, &targets).unwrap();
    let grazing = 2.0 - (0.2f32 * 0.2 - 0.15 * 0.15).sqrt();
    let expected = [(7, 0.0), (1, grazing), (2, 4.8), (3, 7.8)];
    assert_eq!(crossings.len(), expected.len());
    for (crossing, (key, distance)) in crossings.iter().zip(expected) {
        assert_eq!(crossing.key, key);
        assert!(
            (crossing.distance - distance).abs() < 1.0e-5,
            "{crossing:?}"
        );
    }
    // A shot that has not moved touches only what it overlaps.
    let still = sweep_crossings(Vec2::new(5.0, 0.0), Vec2::new(5.0, 0.0), 0.1, &targets).unwrap();
    assert_eq!(
        still,
        vec![Crossing {
            key: 2,
            distance: 0.0
        }]
    );
}

#[test]
fn a_footprint_shares_its_load_among_what_lies_under_it() {
    let end = Vec2::new(4.0, 0.0);
    let targets = [
        target(1, 1.0, 0.0, 0.5, 0.2),
        target(2, 4.0, 0.0, 0.5, 0.2),
        // Beside the first, under the same stretch for 0.6 wu.
        target(3, 1.2, 0.4, 0.5, 0.2),
        target(4, 2.0, 1.0, 0.5, 0.2),
    ];
    let overlaps = footprint_overlaps(ORIGIN, end, &targets).unwrap();
    let expected = [(1, 0.1 + 0.075, 1.0), (2, 0.125, 0.5), (3, 0.075, 0.6)];
    assert_eq!(overlaps.len(), expected.len());
    for (overlap, (key, share, width)) in overlaps.iter().zip(expected) {
        assert_eq!(overlap.key, key);
        assert!(close(overlap.share, share), "{overlap:?}");
        assert!((overlap.width - width).abs() < 1.0e-5, "{overlap:?}");
    }
    let carried: f32 = overlaps.iter().map(|overlap| overlap.share).sum();
    assert!(close(carried, 1.5 / 4.0));
    // A recipient drawn as two overlapping circles shares a stretch as one.
    let targets = [
        target(1, 1.0, 0.0, 0.5, 0.2),
        target(1, 1.2, 0.0, 0.5, 0.2),
        target(2, 1.1, 0.3, 0.5, 0.2),
    ];
    let overlaps = footprint_overlaps(ORIGIN, end, &targets).unwrap();
    let expected = [(1, 0.2, 1.2), (2, 0.1, 0.8)];
    assert_eq!(overlaps.len(), expected.len());
    for (overlap, (key, share, width)) in overlaps.iter().zip(expected) {
        assert_eq!(overlap.key, key);
        assert!(close(overlap.share, share), "{overlap:?}");
        assert!((overlap.width - width).abs() < 1.0e-5, "{overlap:?}");
    }
}

#[test]
fn slivers_and_giants_still_make_valid_faces() {
    let section = Section::new(BLADE, 0.1);
    let valid = |width: f32| {
        Face::new(width, section).validate().unwrap();
        assert!(width >= 1.0e-6, "{width}");
    };
    // A blade grazing the beam's edge by a hair.
    let path = beam_reach(
        ORIGIN,
        Vec2::new(10.0, 0.0),
        1.0,
        1.0,
        &[target(1, 5.0, 0.599_999_96, 0.1, BLADE)],
    )
    .unwrap();
    assert_eq!(path.reaches.len(), 1);
    valid(path.reaches[0].width);
    // A blade whose edge, in `f32`, reaches a hair inside the end of a
    // footprint.
    let overlaps = footprint_overlaps(
        ORIGIN,
        Vec2::new(4.0, 0.0),
        &[target(1, 4.1, 0.0, 0.1, BLADE)],
    )
    .unwrap();
    assert_eq!(overlaps.len(), 1);
    valid(overlaps[0].width);
    // A vast body around a blast.
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &[target(1, 1.0, 0.0, 6.0e5, 1.0)]).unwrap();
    valid(reaches[0].width);
    assert_eq!(reaches[0].width, 1.0e6);
}

/// `point` turned 37 degrees about the origin and moved by (3, -2).
fn moved(point: Vec2) -> Vec2 {
    let (sin, cos) = 37.0f32.to_radians().sin_cos();
    Vec2::new(
        3.0 + point.x * cos - point.y * sin,
        -2.0 + point.x * sin + point.y * cos,
    )
}

#[test]
fn reach_depends_on_neither_place_nor_heading() {
    let targets = [
        target(1, 3.0, 0.15, 0.3, 0.2),
        target(2, 6.0, -0.1, 1.0, 1.0),
        target(3, 2.0, -0.4, 0.2, 0.5),
        target(3, -4.0, 1.0, 0.5, 0.5),
    ];
    let turned: Vec<Target> = targets
        .iter()
        .map(|target| Target {
            center: moved(target.center),
            ..*target
        })
        .collect();
    let end = Vec2::new(8.0, 0.0);
    let same = |here: &[Reach], there: &[Reach]| {
        assert_eq!(here.len(), there.len());
        for (here, there) in here.iter().zip(there) {
            assert_eq!(here.key, there.key);
            assert!(
                (here.coupling - there.coupling).abs() < 1.0e-5,
                "{here:?} {there:?}"
            );
            assert!(
                (here.width - there.width).abs() < 1.0e-4,
                "{here:?} {there:?}"
            );
        }
    };
    let here = blast_reach(ORIGIN, 10.0, 1.0, &targets).unwrap();
    assert_eq!(here.len(), 3);
    same(
        &here,
        &blast_reach(moved(ORIGIN), 10.0, 1.0, &turned).unwrap(),
    );
    let here = beam_reach(ORIGIN, end, 1.0, 1.0, &targets).unwrap();
    let there = beam_reach(moved(ORIGIN), moved(end), 1.0, 1.0, &turned).unwrap();
    assert_eq!(here.reaches.len(), 3);
    same(&here.reaches, &there.reaches);
    assert!((here.through - there.through).abs() < 1.0e-5);
    let here = sweep_crossings(ORIGIN, end, 0.1, &targets).unwrap();
    let there = sweep_crossings(moved(ORIGIN), moved(end), 0.1, &turned).unwrap();
    assert_eq!(here.len(), 2);
    assert_eq!(here.len(), there.len());
    for (here, there) in here.iter().zip(&there) {
        assert_eq!(here.key, there.key);
        assert!(
            (here.distance - there.distance).abs() < 1.0e-4,
            "{here:?} {there:?}"
        );
    }
    let here = footprint_overlaps(ORIGIN, end, &targets).unwrap();
    let there = footprint_overlaps(moved(ORIGIN), moved(end), &turned).unwrap();
    assert_eq!(here.len(), 2);
    assert_eq!(here.len(), there.len());
    for (here, there) in here.iter().zip(&there) {
        assert_eq!(here.key, there.key);
        assert!(
            (here.share - there.share).abs() < 1.0e-5,
            "{here:?} {there:?}"
        );
        assert!(
            (here.width - there.width).abs() < 1.0e-4,
            "{here:?} {there:?}"
        );
    }
}

#[test]
fn invalid_geometry_is_refused() {
    let invalid = Err(MaterialError::InvalidGeometry);
    let good = target(1, 5.0, 0.0, 1.0, 1.0);
    let blast = |center, radius, height, targets: &[Target]| {
        blast_reach(center, radius, height, targets).map(drop)
    };
    let beam =
        |end, width, targets: &[Target]| beam_reach(ORIGIN, end, width, 1.0, targets).map(drop);
    let sweep =
        |end, radius, targets: &[Target]| sweep_crossings(ORIGIN, end, radius, targets).map(drop);
    let footprint = |end, targets: &[Target]| footprint_overlaps(ORIGIN, end, targets).map(drop);
    for bad in [
        target(1, f32::NAN, 0.0, 1.0, 1.0),
        target(1, 5.0, 0.0, 0.0, 1.0),
        target(1, 5.0, 0.0, 1.0, 0.0),
        target(1, 5.0, 0.0, 1.0, f32::INFINITY),
    ] {
        assert_eq!(blast(ORIGIN, 10.0, 1.0, &[good, bad]), invalid);
        assert_eq!(beam(Vec2::X, 1.0, &[good, bad]), invalid);
        assert_eq!(sweep(Vec2::X, 0.1, &[good, bad]), invalid);
        assert_eq!(footprint(Vec2::X, &[good, bad]), invalid);
    }
    let nowhere = Vec2::new(f32::NAN, 0.0);
    assert_eq!(blast(nowhere, 10.0, 1.0, &[good]), invalid);
    assert_eq!(blast(ORIGIN, 0.0, 1.0, &[good]), invalid);
    assert_eq!(blast(ORIGIN, 10.0, -1.0, &[good]), invalid);
    assert_eq!(beam(nowhere, 1.0, &[good]), invalid);
    assert_eq!(beam(Vec2::X, 0.0, &[good]), invalid);
    assert_eq!(sweep(nowhere, 0.1, &[good]), invalid);
    assert_eq!(sweep(Vec2::X, 0.0, &[good]), invalid);
    assert_eq!(footprint(nowhere, &[good]), invalid);
    assert_eq!(footprint(ORIGIN, &[good]), invalid);
}

// The chain from geometry through a source into a plant world's tissue.

#[derive(Clone, Copy)]
struct Meadow;

impl Environment for Meadow {
    fn exposure(&self, _: &Tissue) -> Exposure {
        Exposure {
            dirt_contact: true,
            surface_contact: true,
            nutrients: 1000,
            water: 1000,
            light: 1000,
        }
    }

    fn space_available(&self, _: [i32; 2], _: Role, _: u16) -> bool {
        true
    }

    fn support_valid(&self, _: Option<Support>, _: [i32; 2]) -> bool {
        true
    }
}

/// Grass blades are 0.2 wu thick, a fifth of a 1 wu front, and reach 0.1 wu
/// behind their face.
const BLADE: f32 = 0.2;
const BLADE_EXTENT: f32 = 0.1;
const PLATE: u64 = 1000;

/// Three blades of moss on a soft-tissue table, at cells 0, 1 and 2.
fn meadow() -> (Plants, TissueIntegrity, Vec<TissueId>) {
    let mut plants = Plants::new(Config::default(), 3).unwrap();
    let blades = (0..3)
        .map(|x| {
            plants
                .seed(1, Role::Moss, 1, [x, 0], None, [0; 3], &Meadow)
                .unwrap()
        })
        .collect();
    let soft = TissueMaterial {
        material: SOFT_TISSUE,
        volume_per_size: 0.02,
    };
    let wood = TissueMaterial {
        material: WOOD,
        ..soft
    };
    let table = TissueTable {
        identity: 1,
        roles: [wood, soft, soft, soft, wood],
    };
    let integrity = TissueIntegrity::new(table, &plants).unwrap();
    (plants, integrity, blades)
}

/// Blades standing at x = 2, 3 and 4 in front of the tank's plate at x = 8.
fn targets(blades: &[TissueId]) -> Vec<Target> {
    let mut targets: Vec<Target> = blades
        .iter()
        .enumerate()
        .map(|(i, id)| target(*id, 2.0 + i as f32, 0.0, 0.1, BLADE))
        .collect();
    targets.push(target(PLATE, 8.0, 0.0, 2.0, 1.0));
    targets
}

fn blade_face(width: f32) -> Face {
    Face::new(
        width,
        Section::new(BLADE, deformation_depth(width, Some(BLADE_EXTENT)).unwrap()),
    )
}

fn plate_face(width: f32) -> Face {
    Face::new(width, tank().section(width))
}

/// Each reach's region: the blade's tissue, or the plate.
fn regions(reaches: &[Reach], integrity: &TissueIntegrity) -> Vec<Region> {
    reaches
        .iter()
        .map(|reach| match reach.key {
            PLATE => tank().region(),
            id => *integrity.region(id).unwrap(),
        })
        .collect()
}

fn shares<'a>(reaches: &[Reach], regions: &'a [Region]) -> Vec<Share<'a>> {
    reaches
        .iter()
        .zip(regions)
        .map(|(reach, region)| Share {
            key: reach.key,
            region,
            face: match reach.key {
                PLATE => plate_face(reach.width),
                _ => blade_face(reach.width),
            },
            coupling: reach.coupling,
        })
        .collect()
}

/// Charge each blade the work its share consumed, as the host does.
fn charge(integrity: &mut TissueIntegrity, reaches: &[Reach], outcomes: &[Outcome]) {
    for (reach, outcome) in reaches.iter().zip(outcomes) {
        if reach.key != PLATE {
            assert_eq!(integrity.charge(reach.key, outcome.consumed[0]), Ok(0.0));
        }
    }
}

fn source(id: u64) -> SourceSpec {
    SourceSpec {
        id,
        owner: Some(7),
        projectile: None,
        payload: None,
        beam: None,
    }
}

#[test]
fn a_blast_over_grass_charges_each_blade_once_and_shields_the_plate() {
    let (_, mut integrity, blades) = meadow();
    let targets = targets(&blades);
    let reaches = blast_reach(ORIGIN, 10.0, 1.0, &targets).unwrap();
    // Each blade lies wholly in the shadow of the ones before it, which pass
    // on four fifths of what they meet.
    for (i, id) in blades.iter().enumerate() {
        let distance = 2.0 + i as f32;
        let expected = BLADE * 0.8f32.powi(i as i32) * subtends(0.1, distance);
        assert!(close(coupling(&reaches, *id), expected), "{i}");
    }
    let alone = blast_reach(ORIGIN, 10.0, 1.0, &targets[3..]).unwrap();
    assert!(coupling(&reaches, PLATE) < coupling(&alone, PLATE));
    let regions = regions(&reaches, &integrity);
    let mut source = Source::new(SourceSpec {
        payload: Some(5000.0),
        ..source(1)
    })
    .unwrap();
    let outcomes = source.release(&shares(&reaches, &regions)).unwrap();
    charge(&mut integrity, &reaches, &outcomes);
    // Each recipient received its coupling's part of the payload.
    let ledger = source.ledger().payload;
    assert!((ledger.delivered - 5000.0 * total(&reaches)).abs() < 5.0e-3);
    // Every blade fails, and each failure is reported once.
    assert_eq!(integrity.take_failures(), blades);
    assert!(integrity.take_failures().is_empty());
}

#[test]
fn a_beam_through_grass_charges_each_blade_and_ends_on_the_plate() {
    let (_, mut integrity, blades) = meadow();
    let targets = targets(&blades);
    // The beam's trace ends on the plate's face at x = 6. Each blade covers
    // two fifths of its width, in the shadow of the ones before it.
    let path = beam_reach(ORIGIN, Vec2::new(6.0, 0.0), 0.5, 1.0, &targets[..3]).unwrap();
    for (id, expected) in blades.iter().zip([0.08, 0.064, 0.0512]) {
        assert!(close(coupling(&path.reaches, *id), expected));
    }
    let mut reaches = path.reaches.clone();
    reaches.push(Reach {
        key: PLATE,
        coupling: path.through,
        width: 0.5,
    });
    assert!(close(path.through, 1.0 - 0.08 - 0.064 - 0.0512));
    let regions = regions(&reaches, &integrity);
    let mut source = Source::new(SourceSpec {
        beam: Some(Pulses {
            energy: 100.0,
            count: 1,
        }),
        ..source(3)
    })
    .unwrap();
    let outcomes = source.pulse(&shares(&reaches, &regions)).unwrap();
    charge(&mut integrity, &reaches, &outcomes);
    assert!((source.ledger().beam.delivered - 100.0).abs() < 1.0e-4);
    assert_eq!(integrity.take_failures(), blades);
}

#[test]
fn a_shot_through_grass_charges_what_it_crosses_and_still_hits_the_plate() {
    let (_, mut integrity, blades) = meadow();
    let targets = targets(&blades);
    let shell = shell();
    let speed = 300.0;
    let mut source = Source::new(SourceSpec {
        projectile: Some(Projectile {
            region: shell.region(),
            mass: shell.mass.unwrap(),
            speed,
        }),
        ..source(2)
    })
    .unwrap();
    // Two ticks of flight: the second sweep starts where the first ended,
    // over the middle blade, which both report.
    let ticks = [
        (ORIGIN, Vec2::new(3.0, 0.0)),
        (Vec2::new(3.0, 0.0), Vec2::new(5.8, 0.0)),
    ];
    let mut passed = Vec::new();
    let mut repeats = Vec::new();
    for (start, end) in ticks {
        // The plate is the physics engine's hard contact, not a crossing.
        for crossing in sweep_crossings(start, end, 0.1, &targets[..3]).unwrap() {
            let region = *integrity.region(crossing.key).unwrap();
            let width = engine_material::patch_width_from_radii(0.1, Some(0.1)).unwrap();
            match source.pass(crossing.key, &region, blade_face(width), speed) {
                Ok(outcome) => {
                    integrity.charge(crossing.key, outcome.consumed[0]).unwrap();
                    passed.push(crossing.key);
                }
                Err(error) => {
                    assert_eq!(error, engine_material::SourceError::Duplicate(crossing.key));
                    repeats.push(crossing.key);
                }
            }
        }
    }
    assert_eq!(passed, blades);
    assert_eq!(repeats, [blades[1]]);
    // The shot cut every blade and reaches the plate at full speed.
    assert_eq!(integrity.take_failures(), blades);
    assert_eq!(source.projectile().unwrap().speed, speed);
    let plate = tank().region();
    let load = Impact {
        closing_speed: speed,
        effective_mass: effective_mass(
            ContactBody::translating(shell.mass.unwrap()),
            tank().contact(),
        )
        .unwrap(),
    };
    let contact = engine_material::fixtures::geometry(&shell, &tank());
    let outcome = source.hit(PLATE, &plate, contact, load).unwrap();
    assert_eq!(outcome.ledger.available, load.energy());
}

#[test]
fn a_footprint_crushes_the_blades_under_it_by_their_share_of_the_load() {
    let (_, mut integrity, blades) = meadow();
    let targets = targets(&blades);
    // The tank's tread rests from x = 1.9 to x = 3.1, over the first two
    // blades, which each carry their part of its weight.
    let weight = tank().mass.unwrap() * engine_material::fixtures::GRAVITY;
    let overlaps = footprint_overlaps(Vec2::new(1.9, 0.0), Vec2::new(3.1, 0.0), &targets).unwrap();
    assert_eq!(
        overlaps
            .iter()
            .map(|overlap| overlap.key)
            .collect::<Vec<_>>(),
        blades[..2]
    );
    for Overlap { key, share, width } in overlaps {
        // Each blade lies under 0.2 wu of the tread's 1.2.
        assert!((share - 1.0 / 6.0).abs() < 1.0e-6, "{share}");
        assert!((width - 0.2).abs() < 1.0e-6, "{width}");
        let load = SustainedLoad {
            force: weight * share,
            width,
            section: blade_face(width).section,
            duration: TICK,
            measured_plastic_displacement: None,
        };
        // A sixth of the tank's weight still crushes it.
        let outcome = integrity.press(key, load).unwrap();
        assert!(outcome.failed[0]);
    }
    assert_eq!(integrity.take_failures(), blades[..2]);
    assert!(!integrity.region(blades[2]).unwrap().failed());
}
