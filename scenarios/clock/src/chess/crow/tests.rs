use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld,
};

use super::*;
use crate::arena::ids::canonical;
use crate::chess::stage::{Scene, Stage};
use crate::floor::FloorGeometry;

/// More ticks than any flight here takes, so that one that never ends fails
/// its test.
const LONGEST: u64 = 2_000;

/// The views a board is staged in: the cabinets', the portrait one, and the
/// desktop's window.
const VIEWS: [f32; 4] = [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0, 1280.0 / 720.0];

/// How far a body may sink into what it rests on, in the crow's pitches.
const SUNK: f32 = 0.01;

/// A crow on its own on the closed floor of a view, with the world its body
/// is in. The renderer's tests draw the crow of one.
pub(crate) struct Bench {
    layout: Layout,
    stage: Stage,
    world: PhysicsWorld,
    pub(crate) crow: Crow,
    ticked: u64,
}

impl Bench {
    /// A crow in the view of `aspect`, with the duck at the `prefer` side of
    /// their board where that can be, so the crow at the other.
    pub(crate) fn new(aspect: f32, prefer: Side) -> Self {
        Self::staged(aspect, prefer).expect("a stage")
    }

    fn staged(aspect: f32, prefer: Side) -> Option<Self> {
        let layout = Layout::new(aspect);
        let support: Vec<_> = FloorGeometry::closed(layout).support().collect();
        let scene = Scene {
            layout,
            support: &support,
            worlds: &[],
            plants: &[],
            water: None,
        };
        let stage = Stage::find(&scene, prefer).ok()?;
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
        let crow = Crow::new(&stage, layout, &mut world);
        Some(Self {
            layout,
            stage,
            world,
            crow,
            ticked: 0,
        })
    }

    /// One tick, as the host takes it: the crow steers, the arena steps, and
    /// the crow reads its body back.
    pub(crate) fn tick(&mut self) {
        self.ticked += 1;
        self.crow.drive(&mut self.world);
        self.world.step(flight::DT);
        self.crow.observe(&self.world);
    }

    /// Ticks until `reached`, and returns how many ticks that took.
    fn until(&mut self, what: &str, reached: impl Fn(&Crow) -> bool) -> u64 {
        for ticks in 0..LONGEST {
            if reached(&self.crow) {
                return ticks;
            }
            self.tick();
        }
        panic!("the crow never {what}");
    }

    /// Flies the crow in and lets it settle at its seat.
    pub(crate) fn seat(&mut self) {
        self.until("sat down", |crow| crow.task() == Task::Seated);
        self.until("settled", Crow::settled);
    }

    /// The side of the board the crow is at.
    fn side(&self) -> Side {
        self.stage.duck.other()
    }

    /// The edge of the board on the crow's side.
    fn near(&self) -> f32 {
        match self.side() {
            Side::Left => self.stage.board.min.x,
            Side::Right => self.stage.board.max.x,
        }
    }

    /// How far beyond the edge of the view on its side the crow's feet are.
    fn beyond(&self) -> f32 {
        self.crow.toward * (self.crow.edge - self.crow.position().x)
    }

    /// A fixed block in the world, `half` across and up from its middle
    /// `at`, that meets whatever `groups` lets it.
    fn block(&mut self, at: Vec2, half: Vec2, groups: engine_rapier::world::CollisionGroups) {
        let entity = PhysicsId::new(canonical::ENTRANCE_WALL.start);
        let mut collider = ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            half.x,
            half.y,
        );
        collider.collision_groups = groups;
        assert!(self.world.insert_body(
            BodyId::new(entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: at,
                ..BodySpec::default()
            },
            &[collider],
        ));
    }

    /// Holds the crow's silhouette, as far as it may reach as it is, to its
    /// way and its room: below the top of its way, on the ground or above
    /// it, short of the board, and within its room wherever it is below its
    /// way.
    fn assert_kept(&self, what: &str) {
        let (pitch, toward) = (self.crow.pitch(), self.crow.toward);
        let at = self.crow.position();
        let (way, room) = (self.stage.way(self.side()), self.stage.room(self.side()));
        let reach = if self.crow.wing().is_some() {
            WING
        } else {
            TOP
        };
        let top = at.y + pitch * (reach + self.crow.hop() * 0.105);
        assert!(top <= way.max.y, "{what}: its top at {top}, at {at:?}");
        assert!(
            at.y >= self.stage.ground() - pitch * SUNK,
            "{what}: below the ground at {at:?}"
        );
        // Flying it leans nowhere, and turned either way it reaches as far
        // behind it as before it.
        assert_eq!(FRONT, BACK);
        let before = at.x + toward * pitch * (FRONT + LEAN * self.crow.lean());
        let frame = self.stage.side() * engine_chess_board::FRAME_SHARE;
        let over = toward * (before - self.near());
        if self.crow.task() == Task::Seated {
            assert!(over <= frame, "{what}: over the squares at {at:?}");
        } else {
            assert!(over < 0.0, "{what}: over the board at {at:?}");
        }
        if at.y < way.min.y - pitch * SUNK {
            let behind = at.x - toward * pitch * BACK;
            let out = match self.side() {
                Side::Left => room.min.x - behind,
                Side::Right => behind - room.max.x,
            };
            assert!(out <= 0.0, "{what}: {out} out of its room at {at:?}");
        }
    }
}

#[test]
fn the_crow_is_as_large_as_its_room_allows_and_flies_within_its_way() {
    let mut sides = Vec::new();
    for aspect in VIEWS {
        let mut bench = Bench::new(aspect, Side::Left);
        let stage = bench.stage;
        let side = stage.side();
        let pitch = bench.crow.pitch();
        let toward = bench.crow.toward;
        sides.push(bench.side());
        assert_eq!(pitch, side * PITCH, "{aspect}");
        let (way, room) = (stage.way(bench.side()), stage.room(bench.side()));

        // It starts out of the view on its side, in the air at the height
        // it flies in at, facing the board.
        let cruise = way.min.y + pitch * SINK;
        let start = bench.crow.position();
        assert_eq!(bench.crow.task(), Task::Coming);
        assert!(bench.crow.present() && !bench.crow.lost());
        assert!((bench.beyond() - pitch * OUT).abs() < 1e-3, "{aspect}");
        assert_eq!(start.y, cruise, "{aspect}");
        assert_eq!(bench.crow.facing_right(), toward > 0.0);
        assert_eq!(bench.crow.destination(), Some(bench.crow.seat()));

        // It flies in along its way no faster than a visiting crow crosses
        // this view, with its feet no lower and no higher than its way has
        // room for, and comes down into its room at its seat.
        let width = bench.layout.bounds_max.x - bench.layout.bounds_min.x;
        let pace = (width * 0.55).max(pitch * 9.0);
        let mut last = start.x;
        while bench.crow.task() == Task::Coming {
            bench.tick();
            assert!(bench.ticked < LONGEST, "{aspect}: the crow never came");
            let at = bench.crow.position();
            assert!((at.x - last).abs() * 60.0 <= pace * 1.001, "{aspect}");
            last = at.x;
            bench.assert_kept(&format!("{aspect}, coming at tick {}", bench.ticked));
            let over_room = (room.min.x..=room.max.x).contains(&at.x);
            if !over_room {
                assert!(
                    (cruise - pitch * SINK..=cruise + pitch * RISE).contains(&at.y),
                    "{aspect}: {} of a pitch from its height",
                    (at.y - cruise) / pitch
                );
                assert!(bench.crow.wing().is_some());
            }
        }
        let came = bench.ticked;
        assert_eq!(bench.crow.task(), Task::Seated);
        assert!(came < FLIGHT_TICKS / 2, "{aspect}: {came} ticks");
        assert!(!bench.crow.late());
        assert_eq!(bench.crow.destination(), None);

        // It stands at its seat, facing the board with its wing folded, and
        // its silhouette is within its room. When it leans all the way its
        // bill reaches the inner edge of the board's frame, and no further.
        bench.until("settled", Crow::settled);
        let at = bench.crow.position();
        assert!((at.y - stage.ground()).abs() < pitch * SUNK, "{aspect}");
        assert_eq!(bench.crow.facing_right(), toward > 0.0);
        assert_eq!(bench.crow.wing(), None);
        let (tail, bill) = (
            at.x - toward * pitch * BACK,
            at.x + toward * pitch * (FRONT + LEAN),
        );
        assert!(
            (room.min.x..=room.max.x).contains(&tail),
            "{aspect}: {tail}"
        );
        assert!(at.y + pitch * WING < room.max.y, "{aspect}");
        let frame = bench.near() + toward * side * engine_chess_board::FRAME_SHARE;
        let short = toward * (frame - bill);
        assert!(
            (0.0..pitch * 2.0 * SETTLED).contains(&short),
            "{aspect}: {bill}"
        );
        for _ in 0..120 {
            bench.tick();
            assert!(bench.crow.settled(), "{aspect}");
            bench.assert_kept(&format!("{aspect}, seated"));
        }

        // Sent on its way it turns and flies out as it came, and is away
        // once it is out of the view.
        bench.crow.leave();
        assert_eq!(bench.crow.task(), Task::Going);
        let left = bench.ticked;
        while !bench.crow.away() {
            bench.tick();
            assert!(bench.ticked < LONGEST, "{aspect}: the crow never left");
            bench.assert_kept(&format!("{aspect}, going at tick {}", bench.ticked));
            let at = bench.crow.position();
            if !(room.min.x..=room.max.x).contains(&at.x) {
                assert!(
                    (cruise - pitch * SINK..=cruise + pitch * RISE).contains(&at.y),
                    "{aspect}: {} of a pitch from its height",
                    (at.y - cruise) / pitch
                );
            }
        }
        let went = bench.ticked - left;
        assert!(went < FLIGHT_TICKS / 2, "{aspect}: {went} ticks");
        assert!(!bench.crow.late());
        assert_eq!(bench.crow.facing_right(), toward < 0.0);
        assert!(bench.beyond() >= pitch * AWAY);
        eprintln!(
            "{aspect:.3}: a crow of pitch {pitch:.2} in {:.2} s and out in {:.2} s",
            came as f32 / 60.0,
            went as f32 / 60.0
        );

        // The visit takes it out of the arena.
        bench.crow.remove(&mut bench.world);
        assert_eq!(bench.crow.task(), Task::Gone);
        assert!(!bench.crow.present() && !bench.crow.lost());
        assert!(!bench.world.contains_entity(BODY.id().entity));
    }
    // The desktop's window has the crow come from the left.
    assert_eq!(
        sides,
        [Side::Right, Side::Right, Side::Right, Side::Left],
        "{sides:?}"
    );
}

#[test]
fn the_crows_flights_keep_to_its_way_and_its_room_in_every_view_a_board_fits() {
    let mut staged = 0;
    for aspect in (56..=180).map(|hundredths| hundredths as f32 / 100.0) {
        for prefer in [Side::Left, Side::Right] {
            let Some(mut bench) = Bench::staged(aspect, prefer) else {
                continue;
            };
            staged += 1;
            let what = format!("{aspect} with the duck at the {:?}", bench.stage.duck);
            while bench.crow.task() == Task::Coming {
                bench.tick();
                bench.assert_kept(&what);
                assert!(!bench.crow.late(), "{what}");
            }
            bench.until("settled", Crow::settled);
            bench.crow.leave();
            while !bench.crow.away() {
                bench.tick();
                bench.assert_kept(&what);
                assert!(!bench.crow.late(), "{what}");
            }
        }
    }
    assert!(staged > 200, "{staged}");
}

#[test]
fn a_crow_at_the_left_of_the_board_is_the_mirror_of_one_at_the_right() {
    let mut right = Bench::new(800.0 / 480.0, Side::Left);
    let mut left = Bench::new(800.0 / 480.0, Side::Right);
    assert_eq!((left.side(), right.side()), (Side::Left, Side::Right));
    assert_eq!(left.stage.board, right.stage.board);
    let mirrored = |left: &Bench, right: &Bench, what: &str| {
        let (a, b) = (left.crow.position(), right.crow.position());
        assert!(
            (a.x + b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3,
            "{what}: {a:?} and {b:?}"
        );
        assert_eq!(
            left.crow.facing_right(),
            !right.crow.facing_right(),
            "{what}"
        );
        assert_eq!(left.crow.task(), right.crow.task(), "{what}");
        assert_eq!(left.crow.wing(), right.crow.wing(), "{what}");
    };
    while left.crow.task() == Task::Coming {
        mirrored(&left, &right, &format!("coming, tick {}", left.ticked));
        left.tick();
        right.tick();
        assert!(left.ticked < LONGEST);
    }
    for _ in 0..60 {
        mirrored(&left, &right, "seated");
        left.tick();
        right.tick();
    }
    left.crow.leave();
    right.crow.leave();
    while !left.crow.away() {
        mirrored(&left, &right, &format!("going, tick {}", left.ticked));
        left.tick();
        right.tick();
        assert!(left.ticked < LONGEST);
    }
    assert!(right.crow.away());
}

#[test]
fn whoever_sets_the_worlds_gravity_the_crow_flies_the_same() {
    let mut ours = Bench::new(800.0 / 480.0, Side::Left);
    let mut theirs = Bench::new(800.0 / 480.0, Side::Left);
    // A visiting duck's world on the portrait view pulls a third as hard as
    // the Clock's own.
    assert!(theirs.world.set_gravity(Vec2::new(0.0, -144.0)));
    while ours.crow.task() == Task::Coming {
        ours.tick();
        theirs.tick();
        assert_eq!(ours.crow.position(), theirs.crow.position());
        assert!(ours.ticked < LONGEST);
    }
    for _ in 0..60 {
        ours.tick();
        theirs.tick();
        assert_eq!(ours.crow.position(), theirs.crow.position());
    }
    assert_eq!(theirs.crow.task(), Task::Seated);
}

#[test]
fn the_walls_at_the_views_edges_let_the_crow_through_and_nothing_else_does() {
    let wall = |bench: &mut Bench, groups| {
        let height = bench.layout.bounds_max.y - bench.layout.bounds_min.y;
        let half = Vec2::new(bench.crow.pitch() * 0.5, height * 0.5);
        bench.block(Vec2::new(bench.crow.edge, 0.0), half, groups);
    };
    let mut open = Bench::new(800.0 / 480.0, Side::Left);
    open.until("sat down", |crow| crow.task() == Task::Seated);

    // A wall of the frame, as a player's duck or a falling face puts up.
    let mut framed = Bench::new(800.0 / 480.0, Side::Left);
    wall(&mut framed, crate::arena::FRAME);
    framed.until("sat down", |crow| crow.task() == Task::Seated);
    assert_eq!(framed.ticked, open.ticked);
    assert_eq!(framed.crow.position(), open.crow.position());

    // Any other body there stops it, and it is late. It is still on its way
    // then, in the air beyond the edge of the view.
    let mut walled = Bench::new(800.0 / 480.0, Side::Left);
    wall(&mut walled, engine_rapier::world::CollisionGroups::ALL);
    let late = walled.until("was late", Crow::late);
    assert_eq!(late, FLIGHT_TICKS + 1);
    assert_eq!(walled.crow.task(), Task::Coming);
    assert!(!walled.crow.lost() && walled.beyond() > 0.0);
    // Sent away, it goes from where it is, with as long again to do it in.
    walled.crow.leave();
    assert!(!walled.crow.late());
    let went = walled.until("left", Crow::away);
    assert!(went < FLIGHT_TICKS / 2, "{went} ticks");
}

#[test]
fn a_crow_pushed_from_its_seat_steps_back_and_one_knocked_off_it_flies_back() {
    let mut bench = Bench::new(800.0 / 480.0, Side::Left);
    bench.seat();
    let (pitch, toward) = (bench.crow.pitch(), bench.crow.toward);
    let seat = bench.crow.seat();

    // Pushed along the ground, it keeps its feet and steps back.
    bench.crow.drive(&mut bench.world);
    BODY.drive(&mut bench.world, Vec2::new(-toward * pitch * 6.0, 0.0));
    bench.world.step(flight::DT);
    bench.crow.observe(&bench.world);
    assert!(!bench.crow.settled(), "it was pushed");
    let back = bench.until("stepped back", Crow::settled);
    assert_eq!(bench.crow.task(), Task::Seated);
    assert!(back < 60, "{back} ticks");
    assert_eq!(bench.crow.wing(), None);

    // Knocked into the air and away from the board, it takes to its wings
    // once it is off its seat, and comes down to it again in good time.
    bench.crow.drive(&mut bench.world);
    BODY.drive(
        &mut bench.world,
        Vec2::new(-toward * pitch * 9.0, pitch * 9.0),
    );
    bench.world.step(flight::DT);
    bench.crow.observe(&bench.world);
    let flew = bench.until("took to the air", |crow| crow.task() == Task::Coming);
    assert!(flew < 20, "{flew} ticks");
    assert!((bench.crow.position().x - seat.x).abs() > pitch * KNOCKED_OFF);
    assert_eq!(bench.crow.destination(), Some(seat));
    let flown = bench.until("sat down again", |crow| crow.task() == Task::Seated);
    assert!(flown < FLIGHT_TICKS / 2, "{flown} ticks");
    bench.until("settled", Crow::settled);
    assert!(!bench.crow.late() && !bench.crow.lost());
}

#[test]
fn a_crow_that_cannot_come_down_at_its_seat_is_late() {
    let mut bench = Bench::new(800.0 / 480.0, Side::Left);
    let pitch = bench.crow.pitch();
    let seat = bench.crow.seat();
    // Something has come to stand on its seat, as tall as the crow.
    bench.block(
        seat + Vec2::new(0.0, pitch * 0.6),
        Vec2::new(pitch * 0.7, pitch * 0.6),
        engine_rapier::world::CollisionGroups::ALL,
    );
    let late = bench.until("was late", Crow::late);
    assert_eq!(late, FLIGHT_TICKS + 1);
    assert_eq!(bench.crow.task(), Task::Coming);
    assert!(bench.crow.present() && !bench.crow.lost());
    // It never stood there: what it came down on is not its seat.
    assert!(bench.crow.position().y > seat.y + pitch * SPOT_DEPTH);
    // Sent away, it flies out from where it is.
    bench.crow.leave();
    let went = bench.until("left", Crow::away);
    assert!(went < FLIGHT_TICKS / 2, "{went} ticks");
    assert!(!bench.crow.late());
}

#[test]
fn a_crow_on_its_way_in_turns_back_when_it_is_sent_away() {
    let mut bench = Bench::new(800.0 / 480.0, Side::Left);
    let toward = bench.crow.toward;
    bench.until("was in the view", |crow| {
        toward * (crow.position().x - crow.edge) > crow.pitch() * 4.0
    });
    assert_eq!(bench.crow.task(), Task::Coming);
    bench.crow.leave();
    assert_eq!(bench.crow.task(), Task::Going);
    assert_eq!(bench.crow.destination(), None);
    let went = bench.until("left", Crow::away);
    assert!(went < FLIGHT_TICKS / 2, "{went} ticks");
    // Sent away again, or once it has gone, nothing changes.
    let ticks = bench.crow.ticks();
    bench.crow.leave();
    assert_eq!(
        (bench.crow.task(), bench.crow.ticks()),
        (Task::Going, ticks)
    );
    bench.crow.remove(&mut bench.world);
    bench.crow.leave();
    assert_eq!(bench.crow.task(), Task::Gone);
}

#[test]
fn the_crow_leans_and_nods_and_hops_only_from_its_seat() {
    let mut bench = Bench::new(800.0 / 480.0, Side::Left);
    // Flying, it does none of them whatever the visit has it do, and its
    // wing is spread.
    bench.crow.set_pose(Pose::Won);
    while bench.crow.task() == Task::Coming {
        bench.tick();
        assert_eq!((bench.crow.lean(), bench.crow.hop()), (0.0, 0.0));
        assert!(!bench.crow.bowed());
        assert!(bench.ticked < LONGEST);
    }
    bench.crow.set_pose(Pose::Arriving);
    bench.until("settled", Crow::settled);

    // What it does from its seat, a hundred and twenty ticks of each pose:
    // how far it leans, how high it hops and how its wing is on each.
    let mut pose = |pose: Pose| -> Vec<(f32, f32, Option<f32>)> {
        bench.crow.set_pose(pose);
        assert_eq!(bench.crow.pose(), pose);
        let seat = bench.crow.position();
        (0..120)
            .map(|_| {
                bench.tick();
                // Its body stays in its seat whatever it does.
                assert!((bench.crow.position() - seat).length() < 0.05);
                assert_eq!(bench.crow.bowed(), pose == Pose::Lost);
                (bench.crow.lean(), bench.crow.hop(), bench.crow.wing())
            })
            .collect()
    };
    let leans = |seen: &[(f32, f32, Option<f32>)]| -> Vec<f32> {
        seen.iter().map(|(lean, ..)| *lean).collect()
    };
    let rises = |values: &[f32]| {
        values
            .windows(2)
            .filter(|pair| pair[0] == 0.0 && pair[1] > 0.0)
            .count()
    };
    let still = (0.0, 0.0, None);

    // Waiting for the duck to bring the board, and between its moves, it
    // stands upright.
    assert!(pose(Pose::Arriving).iter().all(|seen| *seen == still));
    // Its own move: all the way over within ten ticks, and held.
    let moving = pose(Pose::Moving);
    assert!(
        moving
            .iter()
            .all(|(_, hop, wing)| *hop == 0.0 && wing.is_none())
    );
    assert!(leans(&moving).windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(moving[9], (1.0, 0.0, None));
    // Waiting: upright again within ten ticks.
    let waiting = pose(Pose::Waiting);
    assert_eq!(waiting[9], still);
    assert!(waiting[10..].iter().all(|seen| *seen == still));
    // Setting out and packing: a nod every thirty ticks, on the half of
    // each that the duck does not nod on.
    for nodding in [Pose::Setting, Pose::Packing] {
        let nods = leans(&pose(nodding));
        let wait = (NOD_TICKS / 2) as usize - 1;
        assert!(nods[..wait].iter().all(|lean| *lean == 0.0), "{nodding:?}");
        assert!(nods[wait] > 0.0, "{nodding:?}");
        assert_eq!(rises(&nods), 120 / NOD_TICKS as usize, "{nodding:?}");
        assert_eq!(nods.iter().copied().fold(0.0, f32::max), 1.0);
        pose(Pose::Waiting);
    }
    // Won: three hops of two pixels, its wing raised at the top of each,
    // and no lean.
    let won = pose(Pose::Won);
    assert!(won.iter().all(|(lean, ..)| *lean == 0.0));
    let hops: Vec<_> = won.iter().map(|(_, hop, _)| *hop).collect();
    assert_eq!(
        rises(&hops),
        HOPS as usize - 1,
        "the first is under way at once"
    );
    assert!(hops[0] > 0.0);
    let highest = hops.iter().copied().fold(0.0, f32::max);
    assert_eq!(highest, HOP_HEIGHT);
    let hopping = (HOPS * HOP_TICKS) as usize - 1;
    for (tick, (_, hop, wing)) in won.iter().enumerate() {
        assert_eq!(wing.is_some(), tick < hopping, "tick {tick}");
        if *hop == highest {
            assert_eq!(*wing, Some(1.0), "tick {tick}");
        }
    }
    assert!(hops[hopping..].iter().all(|hop| *hop == 0.0));
    // Lost: it hangs its head, and leans no further.
    let lost = pose(Pose::Lost);
    assert!(lost.iter().all(|seen| *seen == still));
    pose(Pose::Waiting);
    // Drawn: one slow nod, under way at once.
    let drawn = pose(Pose::Drawn);
    assert!(drawn[0].0 > 0.0 && drawn[NOD_TICKS as usize - 2].0 == 1.0);
    assert_eq!(rises(&leans(&drawn)), 0);
    assert_eq!(drawn.last(), Some(&still));

    // On its way out it is upright again, with its wing spread.
    bench.crow.set_pose(Pose::Moving);
    for _ in 0..20 {
        bench.tick();
    }
    assert_eq!(bench.crow.lean(), 1.0);
    bench.crow.leave();
    bench.tick();
    assert!(bench.crow.lean() < 1.0 && bench.crow.wing().is_some());
    for _ in 0..10 {
        bench.tick();
    }
    assert_eq!(bench.crow.lean(), 0.0);
}

#[test]
fn a_crow_whose_body_leaves_the_scene_is_lost() {
    // Its body is taken from the arena under it.
    let mut bench = Bench::new(800.0 / 480.0, Side::Left);
    bench.seat();
    assert!(bench.world.remove_entity(BODY.id().entity));
    // It steers nothing that is not there.
    bench.tick();
    assert!(bench.crow.lost());
    assert!(bench.crow.present(), "it is the visit's to take out");
    bench.crow.remove(&mut bench.world);
    assert!(!bench.crow.present() && bench.crow.lost());
    assert_eq!(bench.crow.task(), Task::Gone);

    // It has fallen far below the ground, as down an open drain.
    let mut bench = Bench::new(800.0 / 480.0, Side::Left);
    bench.seat();
    let below = bench.crow.seat() - Vec2::new(0.0, bench.crow.pitch() * (FALLEN + 0.5));
    assert!(bench.world.set_pose(BODY.id(), below, 0.0, true));
    bench.crow.observe(&bench.world);
    assert!(bench.crow.lost());

    // With the floor gone from under it, it is not lost: it takes to the
    // air, finds nothing to come down on, and is late.
    let mut bench = Bench::new(800.0 / 480.0, Side::Left);
    bench.seat();
    let floor = PhysicsId::new(canonical::ARENA_FLOOR.start);
    assert!(bench.world.remove_entity(floor));
    let flew = bench.until("took to the air", |crow| crow.task() == Task::Coming);
    assert!(flew < 30, "{flew} ticks");
    bench.until("was late", Crow::late);
    assert!(!bench.crow.lost());
}
