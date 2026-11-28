use engine_rapier::world::{BodyKind, PhysicsWorld};

use super::*;
use crate::chess::stage::{Scene, Stage};
use crate::floor::FloorGeometry;

/// More ticks than any walk here takes, so that one that never ends fails
/// its test.
const LONGEST: u64 = 5_000;

/// The views a board is staged in: the cabinets', the portrait one, and the
/// desktop's window.
const VIEWS: [f32; 4] = [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0, 1280.0 / 720.0];

/// A duck on its own on the closed floor of a view, with the world its body
/// is in.
struct Bench {
    layout: Layout,
    stage: Stage,
    world: PhysicsWorld,
    duck: Duck,
    ticked: u64,
}

impl Bench {
    /// A duck in the view of `aspect`, at the `prefer` side of its board
    /// where that can be.
    fn new(aspect: f32, prefer: Side) -> Self {
        let layout = Layout::new(aspect);
        let support: Vec<_> = FloorGeometry::closed(layout).support().collect();
        let scene = Scene {
            layout,
            support: &support,
            worlds: &[],
            plants: &[],
            water: None,
        };
        let stage = Stage::find(&scene, prefer).expect("a stage");
        let mut world = PhysicsWorld::new(crate::arena::mechanics_config(layout));
        for (index, (min, max)) in support.iter().enumerate() {
            let entity = PhysicsId::new(canonical::ARENA_FLOOR.start + index as u64);
            let half = (*max - *min) * 0.5;
            assert!(world.insert_body(
                BodyId::new(entity, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: (*min + *max) * 0.5,
                    ..BodySpec::default()
                },
                &[ColliderSpec::cuboid(
                    ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                    half.x,
                    half.y,
                )],
            ));
        }
        let duck = Duck::new(&stage, layout, &mut world);
        Self {
            layout,
            stage,
            world,
            duck,
            ticked: 0,
        }
    }

    /// One tick, as the host takes it: the duck steers, the arena steps, and
    /// the duck reads its body back.
    fn tick(&mut self) {
        self.ticked += 1;
        self.duck.drive(&mut self.world);
        self.world.step(DT);
        self.duck.observe(&self.world);
    }

    /// Ticks until `reached`, and returns how many ticks that took.
    fn until(&mut self, what: &str, reached: impl Fn(&Duck) -> bool) -> u64 {
        for ticks in 0..LONGEST {
            if reached(&self.duck) {
                return ticks;
            }
            self.tick();
        }
        panic!("the duck never {what}");
    }

    /// Walks the duck in with the case, has it set the case down and step
    /// back, and seats it.
    fn seat(&mut self) {
        self.until("came", Duck::arrived);
        self.duck.begin(Task::Lowering);
        self.until("set the case down", Duck::finished);
        self.duck.begin(Task::Retiring);
        self.until("sat down", Duck::arrived);
        self.duck.begin(Task::Seated);
    }

    /// The edge of the view the duck comes by.
    fn edge(&self) -> f32 {
        match self.stage.duck {
            Side::Left => self.layout.bounds_min.x,
            Side::Right => self.layout.bounds_max.x,
        }
    }

    /// How far inside the view `x` is from the duck's edge.
    fn inside(&self, x: f32) -> f32 {
        match self.stage.duck {
            Side::Left => x - self.edge(),
            Side::Right => self.edge() - x,
        }
    }
}

#[test]
fn the_duck_is_as_large_as_its_room_allows_and_walks_as_a_visiting_duck_does() {
    let mut sides = Vec::new();
    for aspect in VIEWS {
        let mut bench = Bench::new(aspect, Side::Left);
        let stage = bench.stage;
        let side = stage.side();
        let radius = bench.duck.radius();
        sides.push(stage.duck);
        assert_eq!(radius, side * RADIUS, "{aspect}");
        // It walks at a visiting duck's pace in this view, whatever its
        // own size.
        let width = bench.layout.bounds_max.x - bench.layout.bounds_min.x;
        let pace = Movement::new(width, radius).walk_speed;
        assert_eq!(pace, width / 7.0);

        // Out of the view it walks level, a radius above the ground it has
        // yet to reach. In the view it stands on the floor.
        let level = stage.ground() + radius;
        let start = bench.duck.position();
        assert!(bench.inside(start.x) < -side * 0.5, "{aspect}: {start:?}");
        assert_eq!(start.y, level);
        let mut fastest = 0.0_f32;
        let mut last = start.x;
        while !bench.duck.arrived() {
            bench.tick();
            let at = bench.duck.position();
            fastest = fastest.max((at.x - last).abs() * 60.0);
            last = at.x;
            if bench.inside(at.x) < 0.0 {
                assert_eq!(at.y, level, "{aspect}: beyond the edge");
            } else {
                assert!((at.y - level).abs() < radius * 0.05, "{aspect}: {at:?}");
            }
            assert!(bench.ticked < LONGEST, "{aspect}: the duck never came");
        }
        assert!(
            (fastest - pace).abs() < pace * 0.01,
            "{aspect}: {fastest} and {pace}"
        );
        // With its door's opening, the walk takes four to five seconds in
        // every view.
        assert!(
            (4 * 60..=5 * 60 + 20).contains(&bench.ticked),
            "{aspect}: {}",
            bench.ticked
        );

        // It sets the case down where the board is to stand, and from its
        // seat its silhouette is within its room and reaches the inner edge
        // of the board's frame when it leans all the way.
        let held = bench.duck.case().expect("the duck has the case");
        let rest = stage.case();
        assert!((held.center().x - rest.center().x).abs() < radius * ARRIVED + 0.01);
        assert!((held.min.y - (rest.min.y + side * HOLD_LIFT)).abs() < radius * 0.11);
        bench.duck.begin(Task::Lowering);
        bench.until("set the case down", Duck::finished);
        let down = bench.duck.case().expect("the duck has the case");
        assert!((down.center() - rest.center()).length() < 1e-3, "{aspect}");
        bench.duck.begin(Task::Retiring);
        assert_eq!(bench.duck.case(), None);
        bench.until("sat down", Duck::arrived);
        let at = bench.duck.position();
        let room = stage.room(stage.duck);
        let toward = bench.duck.facing();
        let (tail, bill) = (
            at.x - toward * radius * BACK,
            at.x + toward * radius * (FRONT + LEAN),
        );
        let near = match stage.duck {
            Side::Left => stage.board.min.x,
            Side::Right => stage.board.max.x,
        };
        let frame = near + toward * side * engine_chess_board::FRAME_SHARE;
        assert!(
            (room.min.x..=room.max.x).contains(&tail),
            "{aspect}: {tail}"
        );
        let short = toward * (frame - bill);
        assert!((0.0..radius * 0.21).contains(&short), "{aspect}: {bill}");
        assert!(at.y + radius * TOP < room.max.y, "{aspect}");
        assert!(bench.duck.beside(stage.board), "{aspect}");
    }
    // The desktop's window has the duck come from the right.
    assert_eq!(
        sides,
        [Side::Left, Side::Left, Side::Left, Side::Right],
        "{sides:?}"
    );
}

#[test]
fn a_duck_at_the_right_of_the_board_is_the_mirror_of_one_at_the_left() {
    let mut left = Bench::new(800.0 / 480.0, Side::Left);
    let mut right = Bench::new(800.0 / 480.0, Side::Right);
    assert_eq!(
        (left.stage.duck, right.stage.duck),
        (Side::Left, Side::Right)
    );
    assert_eq!(left.stage.board, right.stage.board);
    let mirrored = |left: &Bench, right: &Bench, what: &str| {
        let (a, b) = (left.duck.position(), right.duck.position());
        assert!(
            (a.x + b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3,
            "{what}"
        );
        assert_eq!(left.duck.facing(), -right.duck.facing(), "{what}");
        match (left.duck.case(), right.duck.case()) {
            (Some(a), Some(b)) => {
                assert!((a.center().x + b.center().x).abs() < 1e-3, "{what}");
                assert!((a.center().y - b.center().y).abs() < 1e-3, "{what}");
            }
            (a, b) => assert_eq!(a, b, "{what}"),
        }
        match (left.duck.door(), right.duck.door()) {
            (Some(a), Some(b)) => {
                assert_eq!(
                    (a.hinge.x, a.angle, a.inward),
                    (-b.hinge.x, -b.angle, -b.inward)
                );
                assert_eq!((a.bottom, a.top, a.width), (b.bottom, b.top, b.width));
            }
            (a, b) => assert_eq!(a.is_some(), b.is_some(), "{what}"),
        }
    };
    for task in [
        Task::Lowering,
        Task::Retiring,
        Task::Seated,
        Task::Fetching,
        Task::Lifting,
        Task::Turning,
        Task::Taking,
    ] {
        loop {
            mirrored(
                &left,
                &right,
                &format!("before {task:?}, tick {}", left.ticked),
            );
            let done = |duck: &Duck| match duck.task() {
                Task::Bringing | Task::Retiring | Task::Fetching => duck.arrived(),
                Task::Lowering | Task::Lifting | Task::Turning => duck.finished(),
                Task::Seated => duck.ticks() >= 30,
                Task::Taking | Task::Gone => unreachable!(),
            };
            assert_eq!(done(&left.duck), done(&right.duck));
            if done(&left.duck) {
                break;
            }
            left.tick();
            right.tick();
            assert!(left.ticked < LONGEST);
        }
        left.duck.begin(task);
        right.duck.begin(task);
    }
    while !left.duck.arrived() {
        mirrored(&left, &right, &format!("leaving, tick {}", left.ticked));
        left.tick();
        right.tick();
        assert!(left.ticked < LONGEST);
    }
    assert!(right.duck.arrived());
}

#[test]
fn the_door_opens_before_the_duck_and_closes_behind_it() {
    let mut bench = Bench::new(800.0 / 480.0, Side::Left);
    let radius = bench.duck.radius();
    let ground = bench.stage.ground();
    let frame = bench.layout.frame_width;
    let edge = bench.edge();
    assert!(bench.duck.door().is_none() && bench.duck.door_closed());
    assert_eq!(bench.duck.door_area(), None);

    // The door opens first, and the duck waits beyond the edge until it
    // has.
    let start = bench.duck.position();
    for tick in 1..=DOOR_OPENING {
        bench.tick();
        let door = bench.duck.door().expect("the door is opening");
        // It is the frame's own wall on the duck's side, as tall as a
        // visiting duck's door, hinged at its top and swinging into the
        // view.
        assert_eq!((door.bottom, door.top), (ground, ground + radius * 4.25));
        assert_eq!(door.hinge, Vec2::new(edge, door.top));
        assert_eq!((door.width, door.inward), (frame, 1.0));
        assert!(door.angle > 0.0 && door.angle <= FRAC_PI_2, "tick {tick}");
        assert_eq!(bench.duck.position(), start, "tick {tick}");
    }
    assert_eq!(bench.duck.door().unwrap().angle, FRAC_PI_2);
    let area = bench.duck.door_area().expect("the door's place");
    assert_eq!(area.min, Vec2::new(edge, ground));
    assert_eq!(
        area.max,
        Vec2::new(edge + radius * 4.25 + frame, ground + radius * 4.25 + frame)
    );

    // It stays open for as long as any of the duck or the case it holds is
    // in the doorway, and closes once the duck's tail is clear of it.
    let doorway = area.width();
    let mut closing = None;
    while !bench.duck.arrived() {
        bench.tick();
        let at = bench.duck.position().x;
        let tail = at - radius * BACK;
        let open = bench.duck.door().map(|door| door.angle);
        if tail - edge < doorway {
            assert_eq!(open, Some(FRAC_PI_2), "at {at}");
        } else if closing.is_none() {
            closing = Some(bench.ticked);
        }
        assert!(bench.ticked < LONGEST);
    }
    let closing = closing.expect("the duck came clear of its door");
    assert!(bench.duck.door_closed());
    assert!(bench.ticked > closing + DOOR_CLOSING);

    // It stays closed while the duck is at the board, and opens again as
    // the duck turns to go.
    bench.duck.begin(Task::Lowering);
    bench.until("set the case down", Duck::finished);
    bench.duck.begin(Task::Retiring);
    bench.until("sat down", Duck::arrived);
    bench.duck.begin(Task::Seated);
    for _ in 0..120 {
        bench.tick();
        assert!(bench.duck.door_closed());
    }
    bench.duck.begin(Task::Fetching);
    bench.until("came for the case", Duck::arrived);
    bench.duck.begin(Task::Lifting);
    bench.until("picked the case up", Duck::finished);
    assert!(bench.duck.door_closed());
    bench.duck.begin(Task::Turning);
    bench.until("turned", Duck::finished);
    assert_eq!(bench.duck.door().unwrap().angle, FRAC_PI_2);
    bench.duck.begin(Task::Taking);
    bench.until("left", Duck::arrived);
    assert_eq!(bench.duck.door().unwrap().angle, FRAC_PI_2);

    // Once the duck has gone, the door closes.
    bench.duck.remove(&mut bench.world);
    assert_eq!(bench.duck.task(), Task::Gone);
    assert!(!bench.duck.present() && !bench.duck.lost());
    let closed = bench.until("door closed", Duck::door_closed);
    assert_eq!(closed, DOOR_CLOSING);
    assert!(!bench.world.contains_entity(ENTITY));
}

#[test]
fn a_duck_on_its_way_out_waits_for_its_door() {
    // In the portrait view the duck's seat is nearer the edge than its case
    // would clear the door from.
    let mut bench = Bench::new(480.0 / 800.0, Side::Left);
    bench.seat();
    let seat = bench.duck.position().x;
    let clear =
        bench.edge() + bench.duck.doorway() + bench.duck.reach + bench.duck.rest.width() * 0.5;
    assert!(seat < clear, "{seat} and {clear}");
    // Sent on its way with its door still shut, it stays where it is until
    // the door has opened, and then walks out.
    bench.duck.begin(Task::Taking);
    for tick in 1..DOOR_OPENING {
        bench.tick();
        let at = bench.duck.position().x;
        assert!((at - seat).abs() < 0.01, "tick {tick}: {at}");
        assert!(!bench.duck.late());
    }
    let left = bench.until("left", Duck::arrived);
    assert!(left > 30, "{left} ticks");
    assert!(!bench.duck.late());
    assert_eq!(bench.duck.door().unwrap().angle, FRAC_PI_2);
}

#[test]
fn whoever_sets_the_worlds_gravity_the_duck_walks_the_same() {
    let mut ours = Bench::new(800.0 / 480.0, Side::Left);
    let mut theirs = Bench::new(800.0 / 480.0, Side::Left);
    // A visiting duck's world on the portrait view pulls a third as hard as
    // the Clock's own.
    assert!(theirs.world.set_gravity(Vec2::new(0.0, -144.0)));
    while !ours.duck.arrived() {
        ours.tick();
        theirs.tick();
        assert_eq!(ours.duck.position(), theirs.duck.position());
        assert!(ours.ticked < LONGEST);
    }
    assert!(theirs.duck.arrived());
}

#[test]
fn the_walls_at_the_views_edges_let_the_duck_through_and_nothing_else_does() {
    let wall = |bench: &mut Bench, groups| {
        let entity = PhysicsId::new(canonical::ENTRANCE_WALL.start);
        let height = bench.layout.bounds_max.y - bench.layout.bounds_min.y;
        let mut collider = ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            bench.duck.radius(),
            height * 0.5,
        );
        collider.collision_groups = groups;
        assert!(bench.world.insert_body(
            BodyId::new(entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::new(bench.edge(), 0.0),
                ..BodySpec::default()
            },
            &[collider],
        ));
    };
    let mut open = Bench::new(800.0 / 480.0, Side::Left);
    open.until("came", Duck::arrived);

    // A wall of the frame, as a player's duck or a falling face puts up.
    let mut framed = Bench::new(800.0 / 480.0, Side::Left);
    wall(&mut framed, crate::arena::FRAME);
    framed.until("came", Duck::arrived);
    assert_eq!(framed.ticked, open.ticked);
    assert_eq!(framed.duck.position(), open.duck.position());

    // Any other body there stops it, and it is late.
    let mut walled = Bench::new(800.0 / 480.0, Side::Left);
    wall(&mut walled, engine_rapier::world::CollisionGroups::ALL);
    let late = walled.until("was late", Duck::late);
    assert!(!walled.duck.arrived() && !walled.duck.lost());
    assert!(walled.duck.position().x < walled.edge());
    // Half as long again as the walk takes, the door's opening, and two
    // seconds.
    let walk = (open.ticked - DOOR_OPENING) as f32;
    let allowed = walk * 1.5 + (DOOR_OPENING + 120) as f32;
    assert!(
        (allowed - 30.0..allowed + 30.0).contains(&(late as f32)),
        "late after {late} ticks, of {allowed}"
    );
}

#[test]
fn the_duck_leans_and_nods_and_hops_only_from_its_seat() {
    let mut bench = Bench::new(800.0 / 480.0, Side::Left);
    // Walking, it does none of them whatever the visit has it do, and its
    // feet take a step every six ticks.
    bench.duck.set_pose(Pose::Won);
    let mut strides = Vec::new();
    while !bench.duck.arrived() {
        bench.tick();
        assert_eq!((bench.duck.lean(), bench.duck.hop()), (0.0, 0.0));
        strides.push(bench.duck.stride());
        assert!(bench.ticked < LONGEST);
    }
    let walking = &strides[DOOR_OPENING as usize + 10..strides.len() - 20];
    assert!(walking.iter().all(|stride| [0.0, 1.0].contains(stride)));
    let steps = walking.windows(2).filter(|pair| pair[0] != pair[1]).count();
    assert_eq!(steps, (walking.len() - 1) / STEP_TICKS as usize);
    assert!(
        strides[..DOOR_OPENING as usize]
            .iter()
            .all(|stride| *stride == 0.0)
    );

    // It leans all the way as it sets the case down.
    bench.duck.begin(Task::Lowering);
    bench.until("set the case down", Duck::finished);
    assert_eq!((bench.duck.lean(), bench.duck.stride()), (1.0, 0.0));
    bench.duck.begin(Task::Retiring);
    bench.until("sat down", Duck::arrived);
    bench.duck.begin(Task::Seated);
    bench.duck.set_pose(Pose::Waiting);
    for _ in 0..20 {
        bench.tick();
    }
    assert_eq!((bench.duck.lean(), bench.duck.hop()), (0.0, 0.0));

    // What it does from its seat, a hundred and twenty ticks of each pose:
    // how far it leans and how high it hops on each.
    let mut pose = |pose: Pose| -> Vec<(f32, f32)> {
        bench.duck.set_pose(pose);
        assert_eq!(bench.duck.pose(), pose);
        let seat = bench.duck.position();
        (0..120)
            .map(|_| {
                bench.tick();
                // Its body stays in its seat whatever it does.
                assert!((bench.duck.position() - seat).length() < 0.05);
                (bench.duck.lean(), bench.duck.hop())
            })
            .collect()
    };
    let leans = |seen: &[(f32, f32)]| seen.iter().map(|(lean, _)| *lean).collect::<Vec<_>>();
    let rises = |values: &[f32]| {
        values
            .windows(2)
            .filter(|pair| pair[0] == 0.0 && pair[1] > 0.0)
            .count()
    };

    // Its own move: all the way over within ten ticks, and held.
    let moving = pose(Pose::Moving);
    assert!(moving.iter().all(|(_, hop)| *hop == 0.0));
    assert!(leans(&moving).windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(moving[9], (1.0, 0.0));
    // Waiting: upright again within ten ticks.
    let waiting = pose(Pose::Waiting);
    assert_eq!(waiting[9], (0.0, 0.0));
    assert!(waiting[10..].iter().all(|seen| *seen == (0.0, 0.0)));
    // Setting out and packing: a nod every thirty ticks.
    for nodding in [Pose::Setting, Pose::Packing] {
        let nods = pose(nodding);
        assert!(nods.iter().all(|(_, hop)| *hop == 0.0));
        assert_eq!(
            rises(&leans(&nods)),
            120 / NOD_TICKS as usize,
            "{nodding:?}"
        );
        let most = leans(&nods).into_iter().fold(0.0, f32::max);
        assert_eq!(most, 1.0, "{nodding:?}");
        pose(Pose::Waiting);
    }
    // Won: three hops of a pixel and a half, and no lean.
    let won = pose(Pose::Won);
    assert!(won.iter().all(|(lean, _)| *lean == 0.0));
    let hops: Vec<_> = won.iter().map(|(_, hop)| *hop).collect();
    assert_eq!(
        rises(&hops),
        HOPS as usize - 1,
        "the first is under way at once"
    );
    assert!(hops[0] > 0.0);
    let highest = hops.iter().copied().fold(0.0, f32::max);
    assert!(
        (HOP_HEIGHT * 0.98..=HOP_HEIGHT).contains(&highest),
        "{highest}"
    );
    assert!(
        hops[(HOPS * HOP_TICKS) as usize..]
            .iter()
            .all(|hop| *hop == 0.0)
    );
    // Lost: it leans over the board and stays there.
    let lost = pose(Pose::Lost);
    assert!(lost[10..].iter().all(|seen| *seen == (1.0, 0.0)));
    pose(Pose::Waiting);
    // Drawn: one slow nod, under way at once.
    let drawn = pose(Pose::Drawn);
    assert!(drawn[0].0 > 0.0 && drawn[NOD_TICKS as usize - 2] == (1.0, 0.0));
    assert_eq!(rises(&leans(&drawn)), 0);
    assert_eq!(drawn.last(), Some(&(0.0, 0.0)));
}

#[test]
fn a_duck_whose_body_leaves_the_scene_is_lost() {
    // The floor goes from under it.
    let mut bench = Bench::new(800.0 / 480.0, Side::Left);
    bench.seat();
    let floor = PhysicsId::new(canonical::ARENA_FLOOR.start);
    assert!(bench.world.remove_entity(floor));
    let fell = bench.until("fell", Duck::lost);
    assert!(fell < 60, "{fell} ticks");
    assert!(bench.duck.present(), "its body is the visit's to take out");
    bench.duck.remove(&mut bench.world);
    assert!(!bench.duck.present() && bench.duck.lost());
    assert!(!bench.world.contains_entity(ENTITY));

    // Its body is taken from the arena under it.
    let mut bench = Bench::new(800.0 / 480.0, Side::Left);
    bench.seat();
    assert!(bench.world.remove_entity(ENTITY));
    bench.duck.observe(&bench.world);
    assert!(bench.duck.lost());
}
