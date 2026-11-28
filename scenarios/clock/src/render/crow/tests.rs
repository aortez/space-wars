use super::*;
use crate::chess::crow as chess;
use crate::chess::stage::Area;

/// The corners of every shape drawn for a crow of pitch one with its feet at
/// the origin, as `change` has it: each shape's lower left and upper right.
fn drawn(change: impl FnOnce(&mut Pose)) -> Vec<(Vec2, Vec2)> {
    let mut pose = Pose {
        position: Vec2::ZERO,
        pitch: 1.0,
        facing_right: true,
        pecking: false,
        wing: None,
        lean: 0.0,
        hop: 0.0,
    };
    change(&mut pose);
    let mut frame = RenderFrame::default();
    silhouette(&mut frame, &pose);
    filled_polygons_with_alpha(&frame, CREATURE_LAYER)
        .into_iter()
        .map(|(points, _)| {
            let area = Area::around(&points).expect("a polygon has points");
            (area.min, area.max)
        })
        .collect()
}

/// The least area holding every one of `shapes`.
fn bounds(shapes: &[(Vec2, Vec2)]) -> Area {
    let corners: Vec<Vec2> = shapes.iter().flat_map(|(min, max)| [*min, *max]).collect();
    Area::around(&corners).expect("something is drawn")
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn the_chess_crows_measures_are_those_of_the_silhouette_it_is_drawn_by() {
    // Standing, the silhouette reaches as far as the crow takes it to, from
    // the ground its feet are on.
    let standing = drawn(|_| {});
    let whole = bounds(&standing);
    assert!(close(whole.min.x, -chess::BACK), "{whole:?}");
    assert!(close(whole.max.x, chess::FRONT), "{whole:?}");
    assert!(close(whole.max.y, chess::TOP), "{whole:?}");
    assert!(close(whole.min.y, 0.0), "{whole:?}");

    // Its wing spread, the silhouette is one shape more. Raised all the
    // way, the wing reaches above its head, and lowered or level it stays
    // within the rest of it.
    let raised = drawn(|pose| pose.wing = Some(1.0));
    assert_eq!(raised.len(), standing.len() + 1);
    assert_eq!(raised[1..], standing[..], "the wing is drawn behind it");
    let whole = bounds(&raised);
    assert!(close(whole.max.y, chess::WING), "{whole:?}");
    assert!(close(whole.min.x, -chess::BACK) && close(whole.max.x, chess::FRONT));
    for wing in [0.0, -1.0] {
        assert_eq!(
            bounds(&drawn(|pose| pose.wing = Some(wing))),
            bounds(&standing)
        );
    }
    // A visit draws no more shapes for its crow than this.
    assert_eq!(raised.len(), 74);

    // Leaning all the way, its bill reaches that much further and its head
    // comes down a pixel, while its body and its legs stay where they are.
    let leaning = drawn(|pose| pose.lean = 1.0);
    let whole = bounds(&leaning);
    assert!(close(whole.max.x, chess::FRONT + chess::LEAN), "{whole:?}");
    assert!(close(whole.max.y, chess::TOP - PIXEL), "{whole:?}");
    assert!(close(whole.min.x, -chess::BACK) && close(whole.min.y, 0.0));
    let low = |shapes: &[(Vec2, Vec2)]| -> Vec<(Vec2, Vec2)> {
        shapes
            .iter()
            .copied()
            .filter(|(_, max)| max.y <= 6.0 * PIXEL + 1e-4)
            .collect()
    };
    assert_eq!(low(&leaning), low(&standing));
    assert_eq!(leaning.len(), standing.len());

    // With its head down at the ground it reaches no further than it does
    // standing, and a lean changes nothing of it.
    let bowed = drawn(|pose| pose.pecking = true);
    let whole = bounds(&bowed);
    assert!(close(whole.min.x, -chess::BACK) && close(whole.max.x, chess::FRONT));
    assert!(close(whole.min.y, 0.0) && whole.max.y < chess::TOP - 2.0 * PIXEL);
    assert_eq!(bowed.len(), standing.len());
    assert_eq!(
        drawn(|pose| {
            pose.pecking = true;
            pose.lean = 1.0;
        }),
        bowed
    );

    // Facing the other way it is the mirror of itself.
    let turned = bounds(&drawn(|pose| {
        pose.facing_right = false;
        pose.lean = 1.0;
    }));
    assert!(close(turned.min.x, -chess::FRONT - chess::LEAN) && close(turned.max.x, chess::BACK));

    // Hopping, all of it rises, feet and wing and all.
    let hopping = bounds(&drawn(|pose| {
        pose.hop = 2.0;
        pose.wing = Some(1.0);
    }));
    assert!(close(hopping.min.y, 2.0 * PIXEL));
    assert!(close(hopping.max.y, chess::WING + 2.0 * PIXEL));
}
