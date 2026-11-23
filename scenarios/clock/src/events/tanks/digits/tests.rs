use super::*;
use crate::arena::mechanics_config;
use engine_common::ClockFont;
use engine_rapier::world::RayCastOptions;

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
    solids.synchronize(&mut world, layout, midnight());
    let cells = cells(layout, midnight());
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
    solids.synchronize(&mut world, layout, midnight());
    let blinked = DisplaySnapshot {
        colon_lit: false,
        ..midnight()
    };
    assert_eq!(cells(layout, blinked), cells(layout, midnight()));
    // A minute later a one, with fewer cells, replaces the last zero.
    let later = DisplaySnapshot {
        digits: [Some(1), Some(2), Some(0), Some(1)],
        ..midnight()
    };
    solids.synchronize(&mut world, layout, later);
    world.step(1.0 / 60.0);
    let lit = cells(layout, later);
    assert_eq!(solids.cells(), lit);
    assert_eq!(
        world.collider_ids().filter(|id| id.entity == BODY).count(),
        lit.len()
    );
    let dark: Vec<_> = cells(layout, midnight())
        .into_iter()
        .filter(|cell| !lit.contains(cell))
        .collect();
    assert!(!dark.is_empty());
    for cell in dark {
        assert_eq!(box_at(&world, cell.center), None, "{cell:?}");
    }
}
