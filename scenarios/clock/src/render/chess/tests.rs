use std::time::Duration;

use engine_common::{ClockEventProfile, Scenario};

use super::*;
use crate::chess::duck::{self, Task};
use crate::chess::{Phase, stage::Side};
use crate::layout::Layout;
use crate::render::frame::DOOR_LAYER;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};

const TICK: Duration = Duration::from_nanos(16_666_667);

/// The corners of every rectangle drawn on `layer`: its lower left and its
/// upper right.
fn rectangles(frame: &RenderFrame, layer: i32) -> Vec<(Vec2, Vec2)> {
    filled_polygons_with_alpha(frame, layer)
        .into_iter()
        .map(|(points, _)| {
            let area = Area::around(&points).expect("a polygon has points");
            (area.min, area.max)
        })
        .collect()
}

/// The least area holding every one of `rectangles`.
fn bounds(rectangles: &[(Vec2, Vec2)]) -> Area {
    let corners: Vec<Vec2> = rectangles
        .iter()
        .flat_map(|(min, max)| [*min, *max])
        .collect();
    Area::around(&corners).expect("something is drawn")
}

/// A duck of radius one at the origin, as `change` has it.
fn drawn(change: impl FnOnce(&mut super::super::duck::Pose)) -> Vec<(Vec2, Vec2)> {
    let mut pose = super::super::duck::Pose {
        position: Vec2::ZERO,
        radius: 1.0,
        facing: 1.0,
        stride: 0.0,
        hanging: false,
        lean: 0.0,
        hop: 0.0,
    };
    change(&mut pose);
    let mut frame = RenderFrame::default();
    super::super::duck::silhouette(&mut frame, &pose, &|point| point);
    rectangles(&frame, ACTIVE_CELL_LAYER)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn the_chess_ducks_measures_are_those_of_the_silhouette_it_is_drawn_by() {
    // Upright, the silhouette reaches as far as the duck takes it to.
    let upright = drawn(|_| {});
    let whole = bounds(&upright);
    assert!(close(whole.min.x, -duck::BACK), "{whole:?}");
    assert!(close(whole.max.x, duck::FRONT), "{whole:?}");
    assert!(close(whole.max.y, duck::TOP), "{whole:?}");
    assert!(
        close(whole.min.y, -1.0),
        "it stands on the foot of its body"
    );
    // Its chest is the front of its body, below its head.
    let pixel = super::super::duck::PIXEL;
    let body: Vec<_> = upright
        .iter()
        .copied()
        .filter(|(_, max)| max.y <= -1.0 + 5.0 * pixel + 1e-4)
        .collect();
    assert!(
        close(bounds(&body).max.x, duck::CHEST),
        "{:?}",
        bounds(&body)
    );
    // A visit draws no more shapes for its duck than this.
    assert_eq!(upright.len(), 57);

    // Leaning all the way, its bill reaches that much further and its head
    // comes down a pixel, while its body and its feet stay where they are.
    let leaning = drawn(|pose| pose.lean = 1.0);
    let whole = bounds(&leaning);
    assert!(close(whole.max.x, duck::FRONT + duck::LEAN), "{whole:?}");
    assert!(close(whole.max.y, duck::TOP - pixel), "{whole:?}");
    assert!(close(whole.min.x, -duck::BACK) && close(whole.min.y, -1.0));
    let low = |shapes: &[(Vec2, Vec2)]| -> Vec<(Vec2, Vec2)> {
        shapes
            .iter()
            .copied()
            .filter(|(_, max)| max.y <= -1.0 + 4.0 * pixel + 1e-4)
            .collect()
    };
    assert_eq!(low(&leaning), low(&upright));
    assert_eq!(leaning.len(), upright.len());

    // Facing the other way it is the mirror of itself.
    let turned = bounds(&drawn(|pose| pose.facing = -1.0));
    assert!(close(turned.min.x, -duck::FRONT) && close(turned.max.x, duck::BACK));

    // Hopping, all of it rises, feet and all. Walking, only its feet move.
    let hopping = bounds(&drawn(|pose| pose.hop = 1.5));
    assert!(close(hopping.min.y, -1.0 + 1.5 * pixel));
    assert!(close(hopping.max.y, duck::TOP + 1.5 * pixel));
    let striding = drawn(|pose| pose.stride = 1.0);
    let moved: Vec<_> = striding
        .iter()
        .zip(&upright)
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(moved.len(), 2, "its two feet");
    assert!(moved.iter().all(|((_, max), _)| close(max.y, -1.0 + pixel)));
}

fn clock(aspect_ratio: f32) -> ClockState {
    let config = ClockConfig {
        event_profile: ClockEventProfile::Off,
        aspect_ratio,
        ..ClockConfig::default()
    };
    let mut state = ClockScenario::init(config, 3);
    let reading = ClockAction::set_reading(ClockReading::new(10, 8, 0).unwrap());
    ClockScenario::step(&mut state, &[reading], Duration::ZERO);
    state
}

#[test]
fn the_duck_is_drawn_where_its_body_is_in_front_of_what_it_carries_and_plays_at() {
    for (aspect, side) in [(800.0 / 480.0, Side::Left), (1280.0 / 720.0, Side::Right)] {
        let plain = clock(aspect);
        let mut state = clock(aspect);
        state.admit_chess_visit().expect("a visit");
        let visit = state.chess_visit().unwrap();
        assert_eq!(visit.stage().duck, side);
        let without = |state: &ClockState, layer: i32| rectangles(&render_unlit(state), layer);
        let cells = without(&plain, ACTIVE_CELL_LAYER);
        let floor = without(&plain, ARENA_LAYER);

        // Before its door has begun to open, the duck is beyond the edge
        // of the view with the case, and the frame is as it always is.
        let frame = render_unlit(&state);
        assert_eq!(rectangles(&frame, DOOR_LAYER), without(&plain, DOOR_LAYER));
        let view = Layout::new(aspect);
        let beyond = |x: f32| x <= view.bounds_min.x || x >= view.bounds_max.x;
        let added = |state: &ClockState, layer: i32, plain: &[(Vec2, Vec2)]| {
            let all = rectangles(&render_unlit(state), layer);
            assert_eq!(all[..plain.len()], *plain, "the visit draws after the rest");
            all[plain.len()..].to_vec()
        };
        let ducks = added(&state, ACTIVE_CELL_LAYER, &cells);
        assert_eq!(ducks.len(), 57);
        assert!(beyond(bounds(&ducks).min.x) && beyond(bounds(&ducks).max.x));

        // Walking in, once its door has shut behind it, it is drawn about
        // its body, in front of the case it holds before it.
        let mut opened = false;
        for _ in 0..5_000 {
            let door = state.chess_visit().unwrap().duck().door_area();
            if opened && door.is_none() {
                break;
            }
            opened |= door.is_some();
            ClockScenario::step(&mut state, &[], TICK);
        }
        let duck = state.chess_visit().unwrap().duck();
        assert_eq!(duck.task(), Task::Bringing);
        let (at, radius) = (duck.position(), duck.radius());
        let ducks = bounds(&added(&state, ACTIVE_CELL_LAYER, &cells));
        let (behind, before) = (
            at.x - duck.facing() * radius * duck::BACK,
            at.x + duck.facing() * radius * duck::FRONT,
        );
        assert!(close(ducks.min.x, behind.min(before)), "{ducks:?}");
        assert!(close(ducks.max.x, behind.max(before)), "{ducks:?}");
        assert!(close(ducks.min.y, at.y - radius) && close(ducks.max.y, at.y + radius * duck::TOP));
        let case = duck.case().expect("the duck has the case");
        let cases = bounds(&added(&state, ARENA_LAYER, &floor));
        assert!(close(cases.min.x, case.min.x) && close(cases.max.x, case.max.x));
        assert!(close(cases.min.y, case.min.y) && close(cases.max.y, case.max.y));

        // At its seat it leans over the board for the pieces it sets out,
        // and its bill never covers a square.
        let mut farthest = f32::NEG_INFINITY;
        while state.chess_visit().unwrap().phase() != Phase::Playing {
            ClockScenario::step(&mut state, &[], TICK);
            let visit = state.chess_visit().unwrap();
            if visit.duck().task() != Task::Seated {
                continue;
            }
            let ducks = bounds(&added(&state, ACTIVE_CELL_LAYER, &cells));
            let reach = match side {
                Side::Left => ducks.max.x - visit.stage().board.min.x,
                Side::Right => visit.stage().board.max.x - ducks.min.x,
            };
            farthest = farthest.max(reach);
        }
        let stage = state.chess_visit().unwrap().stage();
        let frame = stage.side() * engine_chess_board::FRAME_SHARE;
        assert!(farthest > 0.0, "it leans over the board's edge");
        assert!(farthest <= frame + 1e-3, "{farthest} of a frame of {frame}");
    }
}

#[test]
fn the_ducks_door_is_drawn_open_only_while_it_is() {
    let mut state = clock(800.0 / 480.0);
    let closed = rectangles(&render_unlit(&state), DOOR_LAYER);
    assert!(closed.is_empty(), "the frame's walls are shut");
    state.admit_chess_visit().expect("a visit");
    // The door on the duck's side opens for it to come in, and no other.
    let mut open = 0;
    while state.chess_visit().unwrap().duck().task() == Task::Bringing {
        ClockScenario::step(&mut state, &[], TICK);
        let door = state.chess_visit().unwrap().duck().door_area();
        let drawn = rectangles(&render_unlit(&state), DOOR_LAYER);
        assert_eq!(drawn.is_empty(), door.is_none());
        if let Some(door) = door {
            open += 1;
            // The pin of its hinge is a little outside it.
            let pin = Layout::new(800.0 / 480.0).frame_width * 0.2;
            let drawn = bounds(&drawn);
            assert!(drawn.min.x >= door.min.x - pin && drawn.max.x <= door.max.x + pin);
            assert!(drawn.min.y >= door.min.y - pin && drawn.max.y <= door.max.y + pin);
        }
    }
    assert!(open > 60, "{open} ticks");
    assert!(rectangles(&render_unlit(&state), DOOR_LAYER).is_empty());
}
