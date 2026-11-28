use engine_chess_board::Detail;
use engine_common::ClockEventKind;
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld,
};
use engine_water::{Boundary, PoolSpec, WaterConfig, WaterWorld};

use super::*;
use crate::floor::{FloorGeometry, FloorManager};

/// The side of the least board whose pieces can be read in the Clock's view.
const LEAST_SIDE: f32 = 85.0;

/// The sizes the board is qualified at.
const SIZES: [(&str, f32); 3] = [
    ("800x480", 800.0 / 480.0),
    ("1024x768", 1024.0 / 768.0),
    ("480x800", 480.0 / 800.0),
];

fn closed(layout: Layout) -> Vec<(Vec2, Vec2)> {
    FloorGeometry::closed(layout).support().collect()
}

/// The floor's support while Falling has opened the drain.
fn drained(layout: Layout) -> Vec<(Vec2, Vec2)> {
    let mut floor = FloorManager::default();
    floor.acquire(ClockEventKind::Falling);
    floor.geometry(layout).support().collect()
}

fn scene<'a>(layout: Layout, support: &'a [(Vec2, Vec2)], plants: &'a [Area]) -> Scene<'a> {
    Scene {
        layout,
        support,
        worlds: &[],
        plants,
        water: None,
    }
}

/// A plant on the ground at `x`, `wide` across and `tall` high.
fn plant(layout: Layout, x: f32, wide: f32, tall: f32) -> Area {
    Area::new(
        Vec2::new(x - wide * 0.5, layout.floor_y),
        Vec2::new(x + wide * 0.5, layout.floor_y + tall),
    )
}

fn rails(layout: Layout) -> [f32; 2] {
    [
        layout.bounds_min.x + layout.frame_width,
        layout.bounds_max.x - layout.frame_width,
    ]
}

/// Everything a stage promises in `scene`: a legible board on the ground,
/// below the face and clear of its letters, a room at each side of it and a
/// way in to each from its rail, the duck's along supported ground, with
/// nothing in any of them.
fn assert_staged(stage: &Stage, scene: &Scene<'_>, name: &str) {
    let layout = scene.layout;
    let board = stage.board;
    let gap = BELOW_FACE * layout.pitch;
    assert_eq!(board.min.y, layout.floor_y, "{name}: on the ground");
    assert!((board.width() - board.height()).abs() < 1e-3, "{name}");
    let laid = stage.board_for(Side::Left);
    let (drawn, placed) = (laid.bounds(), board.rect());
    for (a, b) in [
        (drawn.min.x, placed.min.x),
        (drawn.min.y, placed.min.y),
        (drawn.max.x, placed.max.x),
        (drawn.max.y, placed.max.y),
    ] {
        assert!((a - b).abs() < 1e-3, "{name}: drawn at {drawn:?}");
    }
    assert!(
        laid.square_side() * 8.5 >= LEAST_SIDE - 1e-3,
        "{name}: legible"
    );
    // The least board keeps less of the gap below the face.
    let face = face(layout);
    let kept = if board.width() < LEAST_SIDE * 1.002 {
        LEAST_BELOW_FACE * layout.pitch
    } else {
        gap
    };
    assert!(
        board.max.y <= face.min.y - kept + 1e-3,
        "{name}: below the face"
    );
    assert!(
        face.min.x <= board.min.x && board.max.x <= face.max.x,
        "{name}"
    );
    let rails = rails(layout);
    assert!(board.width() <= (rails[1] - rails[0]) * FLOOR_SHARE + 1e-3);
    let letters = letters(layout).grown(gap);
    let duck = stage.duck;
    let (walk, flight) = (stage.way(duck), stage.way(duck.other()));
    for (what, area) in [
        ("board", board),
        ("left room", stage.room(Side::Left)),
        ("right room", stage.room(Side::Right)),
        ("walk", walk),
        ("flight", flight),
    ] {
        if area.width() <= 0.0 {
            continue;
        }
        assert!(
            !area.overlaps(letters),
            "{name}: the {what} meets the letters"
        );
        assert!(
            area.min.x >= rails[0] - 1e-3 && area.max.x <= rails[1] + 1e-3,
            "{name}: the {what} lies beyond the rails"
        );
        assert!(scene.clear(area), "{name}: the {what} is taken");
        if what != "flight" {
            assert!(scene.supports(area), "{name}: the {what} is not supported");
        }
    }
    // The rooms stand at the board's sides. The duck's way runs along the
    // ground from its rail to its room, and the crow's from its rail through
    // the air to the board, over its own room.
    assert_eq!(stage.room(Side::Left).max.x, board.min.x, "{name}");
    assert_eq!(stage.room(Side::Right).min.x, board.max.x, "{name}");
    let left = stage.way(Side::Left);
    let right = stage.way(Side::Right);
    assert_eq!((left.min.x, right.max.x), (rails[0], rails[1]), "{name}");
    let room = |side| stage.room(side);
    let (inner, crow) = match duck {
        Side::Left => (
            (walk.max.x, room(Side::Left).min.x),
            (flight.min.x, board.max.x),
        ),
        Side::Right => (
            (walk.min.x, room(Side::Right).max.x),
            (flight.max.x, board.min.x),
        ),
    };
    assert_eq!(inner.0, inner.1, "{name}: the duck walks to its room");
    assert_eq!(crow.0, crow.1, "{name}: the crow flies to above its room");
    assert_eq!(walk.min.y, layout.floor_y, "{name}: the duck walks in");
    assert_eq!(walk.max.y, room(duck).max.y, "{name}");
    // The crow's way is as tall as the crow needs. It begins above the
    // ground and below the top of the crow's room, so the crow comes down
    // out of the one into the other, and it ends below the top of the board.
    let side = board.width();
    assert_eq!(flight.min.y, layout.floor_y + side * WAY_FOOT, "{name}");
    assert!(
        (flight.height() - side * crate::chess::crow::PITCH * crate::chess::crow::WAY).abs() < 1e-3,
        "{name}: {}",
        flight.height()
    );
    assert!(
        flight.min.y < room(duck.other()).max.y,
        "{name}: the crow comes down into its room"
    );
    assert!(flight.max.y <= board.max.y, "{name}");
    assert!(stage.board.contains(stage.case()), "{name}");
}

#[test]
fn every_qualified_size_stages_a_legible_board_in_the_middle_below_the_digits() {
    for (name, aspect) in SIZES {
        let layout = Layout::new(aspect);
        let support = closed(layout);
        let scene = scene(layout, &support, &[]);
        let stage =
            Stage::find(&scene, Side::Left).unwrap_or_else(|refusal| panic!("{name}: {refusal:?}"));
        assert_staged(&stage, &scene, name);
        assert_eq!(stage.board.center().x, 0.0, "{name}: under the colon");
        assert_eq!(stage.duck, Side::Left);
        let board = stage.board_for(Side::Left);
        assert_eq!(board.detail(), Detail::Compact, "{name}");
        eprintln!(
            "{name}: a board of {:.1} with squares of {:.2}, {:.1} below the face",
            stage.side(),
            board.square_side(),
            layout.face_origin.y - stage.board.max.y,
        );
        // The duck can be put on the other side.
        let right = Stage::find(&scene, Side::Right).unwrap();
        assert_eq!((right.duck, right.board), (Side::Right, stage.board));
        // Turned a quarter, White sits at the side it is given.
        assert_eq!(
            stage.board_for(Side::Right).orientation(),
            Orientation::WhiteAtRight
        );
    }
}

#[test]
fn a_plant_in_the_middle_moves_the_board_aside_and_nothing_stands_on_it() {
    for (name, aspect) in SIZES {
        let layout = Layout::new(aspect);
        let support = closed(layout);
        let grass = [plant(layout, 3.0, 6.0, 12.0)];
        let scene = scene(layout, &support, &grass);
        let found = Stage::find(&scene, Side::Left);
        // The narrow view's board and rooms take most of its floor, and
        // moved clear of the grass they would leave the face or the rails.
        if name == "480x800" {
            assert_eq!(found, Err(Refusal::NoRoom));
            continue;
        }
        let stage = found.unwrap_or_else(|refusal| panic!("{name}: {refusal:?}"));
        assert_staged(&stage, &scene, name);
        assert!(stage.board.center().x != 0.0, "{name}");
        // The crow flies over the grass if its way lies there. Nothing
        // else does.
        for area in [
            stage.board,
            stage.room(Side::Left),
            stage.room(Side::Right),
            stage.way(stage.duck),
        ] {
            assert!(!area.overlaps(grass[0]), "{name}");
        }
        // The nearest place on either side is taken, so the board is no
        // further out than it has to be.
        let step = stage.side() * STEP;
        let nearer = stage.board.center().x.abs() - step;
        for sign in [-1.0, 1.0] {
            let middle = sign * nearer;
            let reach = stage.side() * (0.5 + ROOM_WIDTH);
            assert!(
                (middle - reach..middle + reach).contains(&grass[0].min.x)
                    || (middle - reach..middle + reach).contains(&grass[0].max.x),
                "{name}: a nearer place at {middle} was clear"
            );
        }
    }
}

#[test]
fn an_open_drain_is_never_bridged() {
    for (name, aspect) in SIZES {
        let layout = Layout::new(aspect);
        let support = drained(layout);
        let scene = scene(layout, &support, &[]);
        let drain = Area::new(
            Vec2::new(-layout.drain_half_width(), layout.bounds_min.y),
            Vec2::new(layout.drain_half_width(), layout.floor_y + 1.0),
        );
        match Stage::find(&scene, Side::Left) {
            Ok(stage) => {
                assert_staged(&stage, &scene, name);
                for area in [
                    stage.board,
                    stage.room(Side::Left),
                    stage.room(Side::Right),
                    stage.way(stage.duck),
                ] {
                    assert!(!area.overlaps(drain), "{name}: {area:?} bridges the drain");
                }
            }
            // A bank of the narrow view is too short for the board and both
            // rooms.
            Err(refusal) => {
                assert_eq!((name, refusal), ("480x800", Refusal::NoRoom));
                let clear = Stage::find(&self::scene(layout, &closed(layout), &[]), Side::Left)
                    .expect("a stage on the closed floor");
                let bank = rails(layout)[1] - layout.drain_half_width();
                assert!(bank < clear.side() * (1.0 + 2.0 * ROOM_WIDTH));
            }
        }
    }
}

#[test]
fn a_blocked_way_in_puts_the_duck_on_the_other_side_or_turns_the_visit_away() {
    for (name, aspect) in SIZES {
        let layout = Layout::new(aspect);
        let support = closed(layout);
        let [left, right] = rails(layout);
        // Grass by the left rail stops the duck walking in there, but the
        // crow flies over it.
        let low = [plant(layout, left + 4.0, 6.0, 12.0)];
        let scene_low = scene(layout, &support, &low);
        let stage = Stage::find(&scene_low, Side::Left)
            .unwrap_or_else(|refusal| panic!("{name}: {refusal:?}"));
        assert_staged(&stage, &scene_low, name);
        assert_eq!(stage.duck, Side::Right, "{name}");
        // Grass by both rails leaves the duck no way in.
        let both = [low[0], plant(layout, right - 4.0, 6.0, 12.0)];
        assert_eq!(
            Stage::find(&scene(layout, &support, &both), Side::Left),
            Err(Refusal::NoRoom),
            "{name}"
        );
        // Something tall by the left rail stops the crow as well as the
        // duck: neither can come in on that side.
        let tall = [plant(
            layout,
            left + 4.0,
            6.0,
            layout.face_origin.y - layout.floor_y,
        )];
        assert_eq!(
            Stage::find(&scene(layout, &support, &tall), Side::Left),
            Err(Refusal::NoRoom),
            "{name}"
        );
    }
}

/// A world holding a fixed box between `min` and `max`, as any body in the
/// arena would be.
fn world_with_box(layout: Layout, min: Vec2, max: Vec2) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(crate::arena::mechanics_config(layout));
    let entity = PhysicsId::new(crate::arena::ids::canonical::FIXTURE_BALLS.start);
    assert!(world.insert_body(
        BodyId::new(entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: (min + max) * 0.5,
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            (max.x - min.x) * 0.5,
            (max.y - min.y) * 0.5,
        )],
    ));
    world
}

#[test]
fn a_body_in_the_arena_counts_as_taken_but_the_floor_under_the_stage_does_not() {
    for (name, aspect) in SIZES {
        let layout = Layout::new(aspect);
        let support = closed(layout);
        // The floor itself, as the arena keeps it under resting residents.
        let floor = support[0];
        let world = world_with_box(layout, floor.0, floor.1);
        let on_floor = Scene {
            worlds: &[&world],
            ..scene(layout, &support, &[])
        };
        let stage = Stage::find(&on_floor, Side::Left)
            .unwrap_or_else(|refusal| panic!("{name}: {refusal:?}"));
        assert_eq!(stage.board.center().x, 0.0, "{name}");
        // A ball resting in the middle of the floor.
        let ball = 6.0;
        let world = world_with_box(
            layout,
            Vec2::new(-ball, layout.floor_y),
            Vec2::new(ball, layout.floor_y + 2.0 * ball),
        );
        let taken = Scene {
            worlds: &[&world],
            ..scene(layout, &support, &[])
        };
        let found = Stage::find(&taken, Side::Left);
        // As with grass there, the narrow view has no room to move aside.
        if name == "480x800" {
            assert_eq!(found, Err(Refusal::NoRoom));
            continue;
        }
        let stage = found.unwrap_or_else(|refusal| panic!("{name}: {refusal:?}"));
        assert_staged(&stage, &taken, name);
        let ball = Area::new(
            Vec2::new(-ball, layout.floor_y),
            Vec2::new(ball, layout.floor_y + 2.0 * ball),
        );
        for area in [stage.board, stage.room(Side::Left), stage.room(Side::Right)] {
            assert!(!area.overlaps(ball), "{name}");
        }
    }
}

#[test]
fn water_standing_on_the_floor_turns_its_place_away() {
    for (name, aspect) in SIZES {
        let layout = Layout::new(aspect);
        let support = closed(layout);
        let (left, right) = (rails(layout)[0], rails(layout)[1]);
        let mut water = WaterWorld::new(
            WaterConfig::default(),
            vec![PoolSpec {
                left: f64::from(left),
                column_width: f64::from((right - left) / 8.0),
                bed: vec![f64::from(layout.floor_y); 8],
                boundaries: [Boundary::Closed; 2],
            }],
        )
        .unwrap();
        // A puddle across the middle of the floor.
        water
            .add_to_pool(0, 0.0, f64::from((right - left) / 8.0) * 0.5)
            .unwrap();
        let wet = Scene {
            water: Some(&water),
            ..scene(layout, &support, &[])
        };
        assert!(!dry(
            &water,
            Area::new(
                Vec2::new(-1.0, layout.floor_y),
                Vec2::new(1.0, layout.floor_y + 5.0)
            )
        ));
        match Stage::find(&wet, Side::Left) {
            Ok(stage) => {
                assert_staged(&stage, &wet, name);
                assert!(stage.board.center().x != 0.0, "{name}");
            }
            Err(refusal) => assert_eq!(refusal, Refusal::NoRoom, "{name}"),
        }
        // Flooded from rail to rail, nowhere is dry.
        let column = (right - left) / 8.0;
        for index in 0..8 {
            let x = left + column * (index as f32 + 0.5);
            water
                .add_to_pool(0, f64::from(x), f64::from(column) * 4.0)
                .unwrap();
        }
        let flooded = Scene {
            water: Some(&water),
            ..scene(layout, &support, &[])
        };
        assert_eq!(
            Stage::find(&flooded, Side::Left),
            Err(Refusal::NoRoom),
            "{name}"
        );
    }
}

#[test]
fn a_view_a_little_short_of_room_gives_up_part_of_the_gap_for_the_least_board() {
    // The desktop's window, 16:9, has less room below its face than the
    // least board and the whole gap take.
    let layout = Layout::new(16.0 / 9.0);
    let room = face(layout).min.y - layout.floor_y;
    assert!(room - BELOW_FACE * layout.pitch < LEAST_SIDE);
    let support = closed(layout);
    let scene = scene(layout, &support, &[]);
    let stage = Stage::find(&scene, Side::Left).expect("a stage at 16:9");
    assert_staged(&stage, &scene, "16:9");
    assert_eq!(stage.board.center().x, 0.0);
    assert!((LEAST_SIDE..LEAST_SIDE * 1.002).contains(&stage.side()));
    let gap = face(layout).min.y - stage.board.max.y;
    eprintln!(
        "16:9: a board of {:.1}, {gap:.1} below the face of {room:.1}",
        stage.side()
    );
    assert!(gap >= LEAST_BELOW_FACE * layout.pitch && gap < BELOW_FACE * layout.pitch);
    // There the crow's way on the right would pass within the gap kept round
    // the letters, so the duck takes the right and the crow flies in on the
    // left.
    let lane = stage.ground() + stage.side() * (WAY_FOOT + WAY_HEIGHT);
    assert!(lane > letters(layout).grown(BELOW_FACE * layout.pitch).min.y);
    assert_eq!(stage.duck, Side::Right);
    // At the sizes the board is qualified at it passes below that gap, and
    // the duck takes the left.
    for (name, aspect) in SIZES {
        let layout = Layout::new(aspect);
        let support = closed(layout);
        let stage = Stage::find(&self::scene(layout, &support, &[]), Side::Left).unwrap();
        let lane = stage.ground() + stage.side() * (WAY_FOOT + WAY_HEIGHT);
        let kept = letters(layout).grown(BELOW_FACE * layout.pitch).min.y;
        assert!(lane <= kept, "{name}: {lane} and {kept}");
        eprintln!(
            "{name}: the crow's way ends {:.1} below the gap round the letters",
            kept - lane
        );
        assert_eq!(stage.duck, Side::Left, "{name}");
    }

    // A view with room for the whole gap keeps it.
    for (name, aspect) in SIZES {
        let layout = Layout::new(aspect);
        let support = closed(layout);
        let stage = Stage::find(&self::scene(layout, &support, &[]), Side::Left).unwrap();
        let gap = face(layout).min.y - stage.board.max.y;
        assert!(gap >= BELOW_FACE * layout.pitch - 1e-3, "{name}: {gap}");
    }
}

#[test]
fn a_view_with_no_room_for_a_legible_board_turns_every_visit_away() {
    for aspect in [0.25, 0.3, 2.0, 3.0, 4.0] {
        let layout = Layout::new(aspect);
        let support = closed(layout);
        assert_eq!(
            Stage::find(&scene(layout, &support, &[]), Side::Left),
            Err(Refusal::TooSmall),
            "{aspect}"
        );
    }
    // Between them, every view either holds a stage that keeps every
    // promise or turns the visit away.
    let mut staged = Vec::new();
    for step in 0..=150 {
        let aspect = 0.25 + step as f32 * 0.025;
        let layout = Layout::new(aspect);
        let support = closed(layout);
        let scene = scene(layout, &support, &[]);
        match Stage::find(&scene, Side::Left) {
            Ok(stage) => {
                assert_staged(&stage, &scene, &format!("{aspect}"));
                staged.push(aspect);
            }
            Err(refusal) => assert_eq!(refusal, Refusal::TooSmall, "{aspect}"),
        }
    }
    let (narrowest, widest) = (staged[0], staged[staged.len() - 1]);
    eprintln!("a stage fits views from {narrowest} to {widest} wide for each unit high");
    // From 9:16 to 16:9, give or take the sweep's step.
    assert!(narrowest <= 9.0 / 16.0 + 0.025 && widest >= 16.0 / 9.0 - 0.025);
    assert!(
        staged.windows(2).all(|pair| pair[1] - pair[0] < 0.03),
        "{staged:?}"
    );
}
