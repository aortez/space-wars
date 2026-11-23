use super::*;
use crate::arena::mechanics_config;
use engine_common::ClockFont;
use engine_rapier::world::RayCastOptions;

/// A face no shell has damaged.
const WHOLE: [Option<CellMask>; DIGIT_SLOT_COUNT] = [None; DIGIT_SLOT_COUNT];

/// 12:00 AM in the Classic font.
fn midnight() -> DisplaySnapshot {
    DisplaySnapshot {
        font: ClockFont::Classic,
        digits: [Some(1), Some(2), Some(0), Some(0)],
        colon_lit: true,
        meridiem: Some("AM"),
    }
}

/// The box, if any, that holds `point`.
fn box_at(world: &PhysicsWorld, point: Vec2) -> Option<ColliderId> {
    world
        .cast_ray(
            point,
            -Vec2::Y,
            RayCastOptions {
                max_distance: 0.1,
                ..Default::default()
            },
        )
        .map(|hit| hit.collider)
}

#[test]
fn rays_meet_the_face_and_bodies_fall_through_it() {
    let layout = Layout::new(4.0 / 3.0);
    let mut world = PhysicsWorld::new(mechanics_config(layout));
    let mut solids = Solids::default();
    solids.synchronize(&mut world, layout, midnight(), WHOLE);
    let cells = cells(layout, midnight(), WHOLE);
    assert_eq!(solids.cells(), cells);
    // A ball dropped onto the face's highest cell.
    let top = *cells
        .iter()
        .max_by(|a, b| a.center.y.total_cmp(&b.center.y))
        .unwrap();
    let ball = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
    assert!(world.insert_body(
        ball,
        BodySpec {
            kind: BodyKind::Dynamic,
            position: top.center + Vec2::Y * (top.half + 2.0),
            ..BodySpec::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(ball.entity, ColliderRole::PRIMARY, 0),
            1.0,
        )],
    ));
    for _ in 0..60 {
        world.step(1.0 / 60.0);
    }
    for (part, cell) in cells.iter().enumerate() {
        assert_eq!(
            box_at(&world, cell.center),
            Some(ColliderId::new(BODY, ColliderRole::PRIMARY, part as u16)),
            "{cell:?}"
        );
    }
    let fallen = world.motion(ball).unwrap().position;
    assert!(fallen.y < top.center.y - top.half - 1.0, "{fallen:?}");
    assert!((fallen.x - top.center.x).abs() < 1e-3, "{fallen:?}");
}

#[test]
fn the_boxes_follow_the_reading_but_not_the_blinking_colon() {
    let layout = Layout::new(4.0 / 3.0);
    let mut world = PhysicsWorld::new(mechanics_config(layout));
    let mut solids = Solids::default();
    solids.synchronize(&mut world, layout, midnight(), WHOLE);
    let blinked = DisplaySnapshot {
        colon_lit: false,
        ..midnight()
    };
    assert_eq!(
        cells(layout, blinked, WHOLE),
        cells(layout, midnight(), WHOLE)
    );
    // A minute later a one, with fewer cells, replaces the last zero.
    let later = DisplaySnapshot {
        digits: [Some(1), Some(2), Some(0), Some(1)],
        ..midnight()
    };
    solids.synchronize(&mut world, layout, later, WHOLE);
    world.step(1.0 / 60.0);
    let lit = cells(layout, later, WHOLE);
    assert_eq!(solids.cells(), lit);
    assert_eq!(
        world.collider_ids().filter(|id| id.entity == BODY).count(),
        lit.len()
    );
    let dark: Vec<_> = cells(layout, midnight(), WHOLE)
        .into_iter()
        .filter(|cell| !lit.contains(cell))
        .collect();
    assert!(!dark.is_empty());
    for cell in dark {
        assert_eq!(box_at(&world, cell.center), None, "{cell:?}");
    }
}

#[test]
fn a_damaged_digit_keeps_boxes_only_for_the_cells_that_stand() {
    let layout = Layout::new(4.0 / 3.0);
    let mut world = PhysicsWorld::new(mechanics_config(layout));
    let mut solids = Solids::default();
    solids.synchronize(&mut world, layout, midnight(), WHOLE);
    // The last zero loses its lowest cell, and the two is on its way home.
    let zero = crate::fonts::glyph(ClockFont::Classic, Some(0));
    let lost = zero.cells().min_by_key(|cell| (cell.y, cell.x)).unwrap();
    let mut standing = WHOLE;
    standing[3] = Some(CellMask(zero.0 & !(1 << (lost.y * 6 + lost.x))));
    standing[1] = Some(CellMask(0));
    solids.synchronize(&mut world, layout, midnight(), standing);
    world.step(1.0 / 60.0);
    assert_eq!(box_at(&world, layout.slot_cell_center(3, lost)), None);
    for cell in zero.cells().filter(|cell| *cell != lost) {
        assert!(
            box_at(&world, layout.slot_cell_center(3, cell)).is_some(),
            "{cell:?}"
        );
    }
    let two = crate::fonts::glyph(ClockFont::Classic, Some(2));
    for cell in two.cells() {
        assert_eq!(
            box_at(&world, layout.slot_cell_center(1, cell)),
            None,
            "{cell:?}"
        );
    }
    // The other digits keep every box.
    for (slot, digit) in [(0, 1), (2, 0)] {
        for cell in crate::fonts::glyph(ClockFont::Classic, Some(digit)).cells() {
            assert!(box_at(&world, layout.slot_cell_center(slot, cell)).is_some());
        }
    }
}
