use std::time::Duration;

use engine_common::{ClockEventProfile, Scenario};

use super::*;
use crate::chess::duck::{self, Task};
use crate::chess::{Phase, Pose, crow, stage::Side};
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

/// The corners of every shape the plants take for a bird's or a fish's in
/// `state`, which crushes them and stands in their light.
fn seen_by_plants(state: &ClockState) -> Vec<(Vec2, Vec2)> {
    plant_actor_polygons(state)
        .iter()
        .map(|points| {
            let area = Area::around(points).expect("a polygon has points");
            (area.min, area.max)
        })
        .collect()
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

#[test]
fn the_crow_is_drawn_where_its_body_is_in_front_of_the_board_and_over_no_square() {
    for (aspect, side) in [(800.0 / 480.0, Side::Right), (1280.0 / 720.0, Side::Left)] {
        let plain = clock(aspect);
        let mut state = clock(aspect);
        state.admit_chess_visit().expect("a visit");
        let stage = *state.chess_visit().unwrap().stage();
        assert_eq!(stage.duck.other(), side);
        // The crow is drawn with the Clock's other birds, in front of the
        // board and behind the frame, after whatever else is drawn there.
        let creatures = rectangles(&render_unlit(&plain), CREATURE_LAYER);
        let added = |state: &ClockState| {
            let all = rectangles(&render_unlit(state), CREATURE_LAYER);
            assert_eq!(all[..creatures.len()], *creatures);
            all[creatures.len()..].to_vec()
        };

        // At first it is beyond the edge of the view on its side, on the
        // wing: a visit draws no more shapes for its crow than these.
        let view = Layout::new(aspect);
        let beyond = |x: f32| x <= view.bounds_min.x || x >= view.bounds_max.x;
        let crows = added(&state);
        assert_eq!(crows.len(), 74);
        assert!(beyond(bounds(&crows).min.x) && beyond(bounds(&crows).max.x));
        // The plants take what is drawn of it for a bird's, as they do what
        // is drawn of a crow that visits the face.
        assert!(seen_by_plants(&plain).is_empty());
        assert_eq!(seen_by_plants(&state), crows);

        // Flying in, it is drawn about its feet, as wide as its body and no
        // taller than its raised wing.
        for _ in 0..60 {
            ClockScenario::step(&mut state, &[], TICK);
        }
        let flying = state.chess_visit().unwrap().crow();
        assert_eq!(flying.task(), crow::Task::Coming);
        let (at, pitch) = (flying.position(), flying.pitch());
        let crows = added(&state);
        assert_eq!(crows.len(), 74);
        assert_eq!(seen_by_plants(&state), crows);
        let crows = bounds(&crows);
        assert!(!beyond(crows.min.x) && !beyond(crows.max.x), "{crows:?}");
        assert!(close(crows.min.x, at.x - pitch * crow::BACK), "{crows:?}");
        assert!(close(crows.max.x, at.x + pitch * crow::FRONT), "{crows:?}");
        assert!(crows.min.y >= at.y - 1e-4 && crows.max.y <= at.y + pitch * crow::WING + 1e-4);

        // At its seat its wing is folded. It nods over the pieces that
        // come out, and its bill never covers a square.
        let mut farthest = f32::NEG_INFINITY;
        while state.chess_visit().unwrap().phase() != Phase::Playing {
            ClockScenario::step(&mut state, &[], TICK);
            let visit = state.chess_visit().unwrap();
            if visit.crow().task() != crow::Task::Seated {
                continue;
            }
            let crows = added(&state);
            assert_eq!(crows.len(), 73);
            let crows = bounds(&crows);
            let reach = match side {
                Side::Left => crows.max.x - stage.board.min.x,
                Side::Right => stage.board.max.x - crows.min.x,
            };
            farthest = farthest.max(reach);
            let feet = visit.crow().position().y;
            assert!(close(crows.min.y, feet), "{crows:?}");
        }
        let frame = stage.side() * engine_chess_board::FRAME_SHARE;
        assert!(farthest > 0.0, "it leans over the board's edge");
        assert!(farthest <= frame + 1e-3, "{farthest} of a frame of {frame}");

        // Once it has flown out, nothing of it is drawn.
        assert!(state.pack_chess_visit());
        while state
            .chess_visit()
            .is_some_and(|visit| visit.crow().present())
        {
            ClockScenario::step(&mut state, &[], TICK);
        }
        assert!(state.chess_visit().is_some(), "the duck is still going");
        assert!(added(&state).is_empty());
        assert!(seen_by_plants(&state).is_empty());
    }
}

#[test]
fn the_crow_is_drawn_leaning_bowed_and_hopping_as_the_visit_has_it() {
    // A crow on its own, at its seat at the right of its board in the
    // 800×480 view.
    let mut bench = crate::chess::crow::tests::Bench::new(800.0 / 480.0, Side::Left);
    bench.seat();
    // What is drawn of it, from its feet.
    let drawn = |bird: &Crow| -> Vec<(Vec2, Vec2)> {
        let mut frame = RenderFrame::default();
        super::super::crow::silhouette(&mut frame, &crow_pose(bird));
        let feet = bird.position();
        rectangles(&frame, CREATURE_LAYER)
            .into_iter()
            .map(|(min, max)| (min - feet, max - feet))
            .collect()
    };
    let pitch = bench.crow.pitch();
    let pixel = pitch * super::super::crow::PIXEL;

    // Upright it faces the board, as wide as its body and as tall as its
    // head.
    let upright = bounds(&drawn(&bench.crow));
    assert!(close(upright.min.x, -pitch * crow::FRONT), "{upright:?}");
    assert!(close(upright.max.x, pitch * crow::BACK), "{upright:?}");
    assert!(close(upright.min.y, 0.0) && close(upright.max.y, pitch * crow::TOP));

    // Its move: its bill reaches further toward the board, and its head
    // comes down a pixel.
    bench.crow.set_pose(Pose::Moving);
    for _ in 0..12 {
        bench.tick();
    }
    let leaning = bounds(&drawn(&bench.crow));
    let reach = -pitch * (crow::FRONT + crow::LEAN);
    assert!(close(leaning.min.x, reach), "{leaning:?}");
    assert!(close(leaning.max.y, upright.max.y - pixel), "{leaning:?}");
    assert!(close(leaning.max.x, upright.max.x) && close(leaning.min.y, 0.0));

    // It lost: its head hangs to the ground before it, within what it
    // takes up upright.
    bench.crow.set_pose(Pose::Lost);
    for _ in 0..12 {
        bench.tick();
    }
    let bowed = drawn(&bench.crow);
    assert_eq!(bowed.len(), 73);
    let bowed = bounds(&bowed);
    assert!(close(bowed.max.y, upright.max.y - 3.0 * pixel), "{bowed:?}");
    assert!(close(bowed.min.x, upright.min.x) && close(bowed.max.x, upright.max.x));

    // It won: all of it rises with each hop, feet and all, and its wing
    // comes up over its head.
    bench.crow.set_pose(Pose::Won);
    let mut highest: f32 = 0.0;
    for _ in 0..9 {
        bench.tick();
        let hopping = drawn(&bench.crow);
        assert_eq!(hopping.len(), 74);
        let lift = bench.crow.hop() * pixel;
        assert!(lift > highest, "it is on its way up");
        highest = lift;
        // Its wing is drawn first, behind the rest of it.
        let body = bounds(&hopping[1..]);
        assert!(close(body.min.y, lift), "{body:?}");
        assert!(close(body.max.y, upright.max.y + lift), "{body:?}");
    }
    assert!(close(highest, 2.0 * pixel));
    let (_, wing) = drawn(&bench.crow)[0];
    assert!(wing.y > upright.max.y + highest, "{wing:?}");
}
