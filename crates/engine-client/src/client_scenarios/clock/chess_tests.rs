use super::*;
use crate::chess_board_visual_tests::captures;
use crate::gpu_scene::{mesh::MeshBuilder, plan};
use crate::thruster_visual_tests::{svg, write_png};
use engine_chess::{Color, Square};
use engine_chess_board::{DARK_SQUARE, LIGHT_SQUARE, MAX_PRIMITIVES};
use engine_common::{
    ClockEventProfile, ClockLampMode, ClockTimeFormat, RenderColor, RenderPoint, RenderRect,
};
use scenario_clock::chess::crow::{self, Crow};
use scenario_clock::chess::duck::{Duck, LOWER_TICKS, TURN_TICKS, Task};
use scenario_clock::chess::stage::Side;
use scenario_clock::chess::{Bird, Exhibition, Phase, Pose, Show, Visit};
use slint::{Rgb8Pixel, SharedPixelBuffer};

/// The sizes the board is qualified at, and the desktop's window, where the
/// board is the least whose pieces can be read.
const VIEWPORTS: [(&str, Viewport); 4] = [
    ("800x480", Viewport::new(800.0, 480.0)),
    ("1024x768", Viewport::new(1024.0, 768.0)),
    ("480x800", Viewport::new(480.0, 800.0)),
    ("1280x720", Viewport::new(1280.0, 720.0)),
];

const TICK: Duration = Duration::from_nanos(16_666_667);

/// How far from a shape's edge, in pixels, the edge of what a backend paints
/// for it may lie. The raster adapter rounds each row of a shape outward to
/// whole pixels, and may paint a pixel centred up to a pixel and a half
/// beyond it.
const EDGE: f32 = 2.0;

fn reading(minute: u8) -> ClockReading {
    ClockReading::new(22, minute, 0)
        .unwrap()
        .with_date(ClockDate::new(2026, 11, 27).unwrap())
}

/// A Clock at eight past ten in the evening, in the 12-hour format with its
/// date shown, under Steady lamps and with no events of its own.
fn clock(viewport: Viewport) -> ClockClientScenario {
    let config = ClockConfig {
        lamps: ClockLampMode::Steady,
        aspect_ratio: viewport.aspect_ratio(),
        event_profile: ClockEventProfile::Off,
        show_date: true,
        time_format: ClockTimeFormat::TwelveHour,
        ..ClockConfig::default()
    };
    let mut state = ClockScenario::init(config, 11);
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading(8))],
        Duration::ZERO,
    );
    ClockClientScenario {
        state,
        time: super::TimeSource::Host,
        last_emitted: Cell::new(None),
        benchmark: None,
    }
}

/// A rectangle of the view in pixels: its left, top, right and bottom.
type Pixels = [f32; 4];

/// A frame as each render path draws it, by the path's name.
type Drawn = [(&'static str, SharedPixelBuffer<Rgb8Pixel>); 2];

/// Where a rectangle of the Clock's world lies in `viewport`.
fn pixels_of(rect: RenderRect, viewport: Viewport) -> Pixels {
    let scale = viewport.height / 480.0;
    [
        viewport.width * 0.5 + rect.min.x * scale,
        viewport.height * 0.5 - rect.max.y * scale,
        viewport.width * 0.5 + rect.max.x * scale,
        viewport.height * 0.5 - rect.min.y * scale,
    ]
}

/// Whether two pixels differ by more than a backend's rounding.
fn differs(a: Rgb8Pixel, b: Rgb8Pixel) -> bool {
    a.r.abs_diff(b.r)
        .max(a.g.abs_diff(b.g))
        .max(a.b.abs_diff(b.b))
        > 2
}

/// Whether `inner` lies within `outer`, give or take `slack` pixels.
fn within(inner: Pixels, outer: Pixels, slack: f32) -> bool {
    inner[0] >= outer[0] - slack
        && inner[1] >= outer[1] - slack
        && inner[2] <= outer[2] + slack
        && inner[3] <= outer[3] + slack
}

/// Whether `a` and `b` are the same rectangle, give or take `slack` pixels.
fn same(a: Pixels, b: Pixels, slack: f32) -> bool {
    within(a, b, slack) && within(b, a, slack)
}

fn primitives(frame: &RenderFrame) -> usize {
    frame
        .layers
        .iter()
        .map(|layer| layer.primitives.len())
        .sum()
}

fn is(pixel: Rgb8Pixel, color: RenderColor) -> bool {
    let channel = |value: f32| (value * 255.0).round() as u8;
    !differs(
        pixel,
        Rgb8Pixel::new(channel(color.r), channel(color.g), channel(color.b)),
    )
}

/// The least rectangle holding every pixel within `clip` that differs
/// between `a` and `b`.
fn changed_in(
    a: &SharedPixelBuffer<Rgb8Pixel>,
    b: &SharedPixelBuffer<Rgb8Pixel>,
    clip: Pixels,
) -> Option<Pixels> {
    let width = a.width() as usize;
    let mut bounds: Option<Pixels> = None;
    for (index, (now, before)) in a.as_slice().iter().zip(b.as_slice()).enumerate() {
        let (x, y) = ((index % width) as f32, (index / width) as f32);
        if !differs(*now, *before) || !within([x, y, x + 1.0, y + 1.0], clip, 0.0) {
            continue;
        }
        let within = bounds.get_or_insert([x, y, x + 1.0, y + 1.0]);
        *within = [
            within[0].min(x),
            within[1].min(y),
            within[2].max(x + 1.0),
            within[3].max(y + 1.0),
        ];
    }
    bounds
}

/// A pixel that differs between `a` and `b` and lies in none of `allowed`,
/// give or take `slack` pixels, if there is one.
fn stray(
    a: &SharedPixelBuffer<Rgb8Pixel>,
    b: &SharedPixelBuffer<Rgb8Pixel>,
    allowed: &[Pixels],
    slack: f32,
) -> Option<(usize, usize)> {
    let width = a.width() as usize;
    a.as_slice()
        .iter()
        .zip(b.as_slice())
        .enumerate()
        .filter(|(_, (now, before))| differs(**now, **before))
        .map(|(index, _)| (index % width, index / width))
        .find(|&(x, y)| {
            let pixel = [x as f32, y as f32, x as f32 + 1.0, y as f32 + 1.0];
            !allowed.iter().any(|area| within(pixel, *area, slack))
        })
}

/// The pixels a duck may be drawn in, however it leans or hops: its
/// silhouette is under five radii long and three high.
fn duck_pixels(duck: &Duck, viewport: Viewport) -> Pixels {
    let (at, radius) = (duck.position(), duck.radius());
    pixels_of(
        RenderRect {
            min: RenderPoint::new(at.x - radius * 3.2, at.y - radius * 1.2),
            max: RenderPoint::new(at.x + radius * 3.2, at.y + radius * 2.7),
        },
        viewport,
    )
}

/// The pixels a crow may be drawn in, however it faces, leans, flies or
/// hops: its silhouette reaches under a pitch to either side of its feet,
/// and under one and two thirds above them.
fn crow_pixels(crow: &Crow, viewport: Viewport) -> Pixels {
    let (at, pitch) = (crow.position(), crow.pitch());
    pixels_of(
        RenderRect {
            min: RenderPoint::new(at.x - pitch, at.y),
            max: RenderPoint::new(at.x + pitch, at.y + pitch * 1.65),
        },
        viewport,
    )
}

/// The pixels the middle of a crow's body is drawn in, whatever the crow
/// does: clear of its head, its bill, its tail and its legs, and of the
/// board it stands beside.
fn crow_body_pixels(crow: &Crow, viewport: Viewport) -> Pixels {
    let (at, pitch) = (crow.position(), crow.pitch());
    pixels_of(
        RenderRect {
            min: RenderPoint::new(at.x - pitch * 0.3, at.y + pitch * 0.32),
            max: RenderPoint::new(at.x + pitch * 0.3, at.y + pitch * 0.62),
        },
        viewport,
    )
}

/// How far along its way between the edge of the view and its room the duck
/// is, from 0 at the edge to 1 at its room.
fn along_its_way(visit: &Visit) -> f32 {
    let stage = visit.stage();
    let way = stage.way(stage.duck);
    let from_left = (visit.duck().position().x - way.min.x) / way.width();
    match stage.duck {
        Side::Left => from_left,
        Side::Right => 1.0 - from_left,
    }
}

/// How far along its way between the frame at the edge of the view and the
/// board the crow is, from 0 at the frame to 1 at the board.
fn along_the_crows_way(visit: &Visit) -> f32 {
    let stage = visit.stage();
    let side = stage.duck.other();
    let way = stage.way(side);
    let from_left = (visit.crow().position().x - way.min.x) / way.width();
    match side {
        Side::Left => from_left,
        Side::Right => 1.0 - from_left,
    }
}

/// How far a move is on its way across the board, if one is.
fn move_on_its_way(visit: &Visit, exhibition: &Exhibition) -> Option<f32> {
    match visit.show(exhibition) {
        Show::Board(_, presentation) => presentation.transition.map(|moving| moving.progress),
        _ => None,
    }
}

/// A moment of a visit, by what the visit and its match have come to.
type Moment = (&'static str, Phase, fn(&Visit, &Exhibition) -> bool);

/// The moments a visit is drawn at, in the order they come.
const MOMENTS: [Moment; 16] = [
    ("01-over-the-frame", Phase::Arriving, |visit, _| {
        visit.crow().task() == crow::Task::Coming && along_the_crows_way(visit) >= 0.0
    }),
    ("02-flying-in", Phase::Arriving, |visit, _| {
        visit.crow().task() == crow::Task::Coming && along_the_crows_way(visit) >= 0.5
    }),
    ("03-coming", Phase::Arriving, |visit, _| {
        visit.duck().task() == Task::Bringing && along_its_way(visit) >= 0.5
    }),
    ("04-setting-down", Phase::Arriving, |visit, _| {
        visit.duck().task() == Task::Lowering && visit.duck().ticks() >= LOWER_TICKS / 2
    }),
    ("05-opening", Phase::Opening, |visit, _| visit.ticks() >= 22),
    ("06-placing", Phase::Placing, |visit, _| visit.ticks() >= 60),
    ("07-set", Phase::Playing, |_, _| true),
    ("08-first-move", Phase::Playing, |visit, exhibition| {
        move_on_its_way(visit, exhibition).is_some_and(|progress| progress >= 0.5)
    }),
    ("09-second-move", Phase::Playing, |visit, exhibition| {
        let moves = exhibition.current().unwrap().game().moves().len();
        moves == 2 && move_on_its_way(visit, exhibition).is_some_and(|progress| progress >= 0.5)
    }),
    ("10-clearing", Phase::Clearing, |visit, _| {
        visit.ticks() >= 45
    }),
    ("11-closing", Phase::Closing, |visit, _| visit.ticks() >= 22),
    ("12-picking-up", Phase::Leaving, |visit, _| {
        visit.duck().task() == Task::Lifting && visit.duck().ticks() >= LOWER_TICKS / 2
    }),
    ("13-turning", Phase::Leaving, |visit, _| {
        visit.duck().task() == Task::Turning && visit.duck().ticks() >= TURN_TICKS / 3
    }),
    ("14-flying-out", Phase::Leaving, |visit, _| {
        visit.crow().task() == crow::Task::Going && along_the_crows_way(visit) <= 0.5
    }),
    ("15-going", Phase::Leaving, |visit, _| {
        visit.duck().task() == Task::Taking && along_its_way(visit) <= 0.5
    }),
    ("16-through-the-door", Phase::Leaving, |visit, _| {
        visit.duck().task() == Task::Taking && along_its_way(visit) <= -0.02
    }),
];

/// The shapes a visit adds to the Clock's frame beyond its board: the
/// crow's silhouette with its wing spread, the duck's silhouette, and what
/// the frame draws of a door that is at all open, over what it draws of its
/// closed wall.
const CROW_SHAPES: usize = 74;
const DUCK_SHAPES: usize = 57;
const DOOR_SHAPES: usize = 14;

/// A visit is drawn by the CPU raster and the vector paths where its stage
/// stands, at every size the board is qualified at: the crow flying in over
/// the frame to its seat, the duck bringing the case and setting it down,
/// the board opening, being set up and in play with each bird's move on its
/// way, and all of it packed and taken away again as the crow flies out.
/// The GPU scene prepares each of those frames. Nothing else in the view
/// changes for the visit: not the digits, which go on telling the time, the
/// AM/PM letters, the date or the frame the crow passes behind. With
/// `SPACEWARS_CLOCK_ARTIFACTS` set, each path's pixels and the vector path's
/// shapes are written there.
#[test]
fn a_chess_visit_draws_its_board_and_its_birds_below_the_digits_on_both_render_paths() {
    let output = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (name, viewport) in VIEWPORTS {
        let mut plain = clock(viewport);
        let mut visited = clock(viewport);
        visited.state.admit_chess_visit().expect("a visit");
        let visit = visited.state.chess_visit().unwrap();
        let stage = *visit.stage();
        let laid = stage.board_for(visit.side(Color::White));
        let board = pixels_of(stage.board.rect(), viewport);
        // The board stands below the face, in the lower half of the view.
        assert!(board[1] > viewport.height * 0.5, "{name}: {board:?}");

        let mut ticks = 0;
        let mut packed = false;
        let mut shapes = Vec::new();
        let mut leaned = Vec::new();
        // The plain Clock's frame, and each path's pixels of it, drawn again
        // only when the frame changes.
        let mut plain_drawn: Option<(RenderFrame, Drawn)> = None;
        for (moment, phase, reached) in MOMENTS {
            // Both Clocks step to the moment. The time goes on meanwhile,
            // and the digits with it.
            loop {
                let visit = visited.state.chess_visit().expect("a visit");
                let exhibition = visited.state.chess();
                // The visit is asked to pack up while its second move is on
                // its way.
                let moves = exhibition.current().unwrap().game().moves().len();
                let asked = !packed
                    && moves == 2
                    && move_on_its_way(visit, exhibition).is_some_and(|progress| progress >= 0.3);
                if visit.phase() == phase && reached(visit, exhibition) {
                    break;
                }
                assert!(ticks < 6_000, "{name}: the visit never came to {moment}");
                let actions = if visit.phase() == Phase::Playing && visit.ticks() == 30 {
                    vec![ClockAction::set_reading(reading(9))]
                } else {
                    Vec::new()
                };
                if asked {
                    assert!(visited.state.pack_chess_visit(), "{name}");
                    packed = true;
                }
                plain.step(&actions, TICK);
                visited.step(&actions, TICK);
                ticks += 1;
            }
            let visit = visited.state.chess_visit().expect("a visit");
            let exhibition = visited.state.chess();
            let duck = visit.duck();
            let crow = visit.crow();
            let frames = visited.render_frames(RenderBackend::Raster, viewport);
            assert_eq!(
                frames,
                visited.render_frames(RenderBackend::Vector, viewport),
                "{name} {moment}"
            );
            let background = plain.render_frames(RenderBackend::Raster, viewport);
            let added = primitives(&frames[0]) - primitives(&background[0]);
            let door = duck.door_area();
            // The crow's wing is a shape of its own, while it is spread.
            let crows = match (crow.present(), crow.wing()) {
                (false, _) => 0,
                (true, None) => CROW_SHAPES - 1,
                (true, Some(_)) => CROW_SHAPES,
            };
            let birds = crows + DUCK_SHAPES + door.map_or(0, |_| DOOR_SHAPES);
            assert!(
                added <= MAX_PRIMITIVES + birds,
                "{name} {moment}: {added} shapes"
            );
            // Packed, the visit is its case, its birds and its door.
            if let Show::Case(_) = visit.show(exhibition) {
                assert_eq!(added, 3 + birds, "{name} {moment}");
            }
            shapes.push(added);
            let moving = |bird| visit.pose(bird, exhibition) == Pose::Moving;
            if moving(Bird::Crow) {
                leaned.push((moment, Bird::Crow, crow.lean(), duck.lean()));
            }
            if moving(Bird::Duck) {
                leaned.push((moment, Bird::Duck, duck.lean(), crow.lean()));
            }
            // The GPU scene, which the cabinets draw through, prepares the
            // same frame.
            let scene = plan::build(&frames, viewport, visited.frame_layout(), 1.0).unwrap();
            let mut mesh = MeshBuilder::default();
            mesh.rebuild(&scene).unwrap();
            assert!(!mesh.buffers.indices.is_empty(), "{name} {moment}");
            if let Some(output) = &output {
                let paths = svg(&frames[0], viewport);
                std::fs::write(output.join(format!("chess-{name}-{moment}.svg")), paths).unwrap();
            }
            let drawn = captures(&frames[0], viewport);
            if plain_drawn
                .as_ref()
                .is_none_or(|(frame, _)| *frame != background[0])
            {
                let pixels = captures(&background[0], viewport);
                plain_drawn = Some((background[0].clone(), pixels));
            }
            let (_, before) = plain_drawn.as_ref().unwrap();
            // Where the visit draws: its board or its case, its birds, and
            // its door while that is open, with the shadow the frame gives
            // an open door.
            let shown = match visit.show(exhibition) {
                Show::Case(case) => pixels_of(case.rect(), viewport),
                Show::Unfolding { .. } | Show::Board(..) => board,
            };
            let ducks = duck_pixels(duck, viewport);
            let frame_width = viewport.width.min(viewport.height) * 0.022;
            let doors = door.map(|door| {
                let [left, top, right, bottom] = pixels_of(door.rect(), viewport);
                [
                    left - frame_width,
                    top - frame_width,
                    right + frame_width,
                    bottom + frame_width,
                ]
            });
            let mut allowed = vec![shown, ducks];
            allowed.extend(crow.present().then(|| crow_pixels(crow, viewport)));
            allowed.extend(doors);
            // The frame's rail at the edge of the view the crow flies in
            // over, but for the pixels along its inner edge.
            let rail = match stage.duck.other() {
                Side::Left => [0.0, 0.0, frame_width - EDGE, viewport.height],
                Side::Right => [
                    viewport.width - frame_width + EDGE,
                    0.0,
                    viewport.width,
                    viewport.height,
                ],
            };
            for ((backend, pixels), (_, before)) in drawn.iter().zip(before) {
                let what = format!("{name} {moment} {backend}");
                if let Some(output) = &output {
                    write_png(
                        &output.join(format!("chess-{name}-{moment}-{backend}.png")),
                        pixels,
                    );
                }
                // Only the visit's own places change.
                assert_eq!(
                    stray(pixels, before, &allowed, EDGE),
                    None,
                    "{what}: a pixel changed outside {allowed:?}"
                );
                // The duck is drawn where its body is, while any of it is
                // in the view.
                let in_view = ducks[2] > EDGE && ducks[0] < viewport.width - EDGE;
                let seen = changed_in(pixels, before, ducks);
                assert_eq!(seen.is_some(), in_view, "{what}: the duck at {ducks:?}");
                // The crow is drawn where its body is, wherever that is
                // inside the frame, and the frame is drawn over it.
                if crow.present() {
                    let body = crow_body_pixels(crow, viewport);
                    let seen = changed_in(pixels, before, body).is_some();
                    let inside = frame_width + EDGE..=viewport.width - frame_width - EDGE;
                    if inside.contains(&body[0]) && inside.contains(&body[2]) {
                        assert!(seen, "{what}: no crow at {body:?}");
                    } else if body[2] <= 0.0 || body[0] >= viewport.width {
                        assert!(!seen, "{what}: a crow at {body:?}");
                    }
                }
                assert_eq!(
                    changed_in(pixels, before, rail),
                    None,
                    "{what}: the frame at {rail:?}"
                );
                // All of the board's place changes: the case wherever it is
                // while it is packed, and the board's square once it is
                // open.
                let place = changed_in(
                    pixels,
                    before,
                    [
                        shown[0] - EDGE,
                        shown[1] - EDGE,
                        shown[2] + EDGE,
                        shown[3] + EDGE,
                    ],
                );
                let in_view = [
                    shown[0].max(0.0),
                    shown[1],
                    shown[2].min(viewport.width),
                    shown[3],
                ];
                match visit.show(exhibition) {
                    Show::Case(_) | Show::Board(..) => {
                        if in_view[0] < in_view[2] {
                            let place = place.unwrap_or_else(|| panic!("{what}: nothing drawn"));
                            assert!(same(place, in_view, EDGE), "{what}: {place:?}");
                        }
                    }
                    Show::Unfolding { .. } => {
                        let case = pixels_of(stage.case().rect(), viewport);
                        let place = place.unwrap_or_else(|| panic!("{what}: nothing drawn"));
                        assert!(within(case, place, EDGE), "{what}: {place:?}");
                    }
                }
                if moment != "07-set" {
                    continue;
                }
                // Set up, every empty square shows its own colour all over,
                // and a piece covers much of each square of the four ranks
                // the players start on. A compact rook covers all of its.
                let width = viewport.width as usize;
                for index in 0..64 {
                    let square = Square::new(index % 8, index / 8).unwrap();
                    let [left, top, right, bottom] =
                        pixels_of(laid.square_rect(square), viewport).map(f32::round);
                    let base = if (square.file() + square.rank()) % 2 == 0 {
                        DARK_SQUARE
                    } else {
                        LIGHT_SQUARE
                    };
                    // The square's pixels, but for two along each edge,
                    // which the raster may paint for the square beside it.
                    let inside: Vec<_> = (top as usize + 2..bottom as usize - 2)
                        .flat_map(|y| (left as usize + 2..right as usize - 2).map(move |x| (x, y)))
                        .map(|(x, y)| pixels.as_slice()[y * width + x])
                        .collect();
                    let bare = inside.iter().filter(|pixel| is(**pixel, base)).count();
                    if (2..6).contains(&square.rank()) {
                        assert_eq!(bare, inside.len(), "{what}: {square:?} is empty");
                    } else {
                        assert!(
                            bare * 4 < inside.len() * 3,
                            "{what}: {square:?} shows {bare} of {} pixels bare",
                            inside.len()
                        );
                    }
                }
            }
        }
        // The board with half its pieces out takes fewer shapes than with
        // them all, and a piece on its way is still one piece.
        let of = |moment: &str| {
            let index = MOMENTS.iter().position(|(name, ..)| *name == moment);
            shapes[index.unwrap()]
        };
        assert!(of("06-placing") < of("07-set"), "{name}");
        assert!(of("10-clearing") < of("09-second-move"), "{name}");
        // Each bird leans over the board for its own move, and not for the
        // other's.
        assert_eq!(leaned.len(), 2, "{name}: {leaned:?}");
        assert_ne!(leaned[0].1, leaned[1].1, "{name}: {leaned:?}");
        for (_, _, own, others) in &leaned {
            assert!(*own > 0.9 && *others < 0.1, "{name}: {leaned:?}");
        }

        // Once the birds have gone with the board, the view is as it would
        // have been.
        while visited.state.chess_visit().is_some() {
            assert!(ticks < 6_000, "{name}: the visit never left");
            plain.step(&[], TICK);
            visited.step(&[], TICK);
            ticks += 1;
        }
        let frames = visited.render_frames(RenderBackend::Raster, viewport);
        assert_eq!(frames, plain.render_frames(RenderBackend::Raster, viewport));
        let played = visited.state.chess().current().unwrap().game();
        assert_eq!(played.moves().len(), 2, "{name}");
    }
}

/// The Clock's state publishes a visit to the board for as long as it
/// lasts, with its crow and its duck under it as the visit's own birds: in
/// the visit's claim on the arena, in no duck's or crow's state, and gone
/// from the state once they have left.
#[test]
fn clock_state_publishes_a_chess_visit_with_its_birds_as_the_visits_own() {
    use engine_common::{ClockArenaOwner, ClockChessBird, ClockChessPose};
    let mut visited = clock(Viewport::new(800.0, 480.0));
    assert_eq!(visited.clock_state().unwrap().chess, None);
    visited.state.admit_chess_visit().expect("a visit");
    let white = {
        let current = visited.state.chess().current().unwrap();
        match current.bird(Color::White) {
            Bird::Crow => ClockChessBird::Crow,
            Bird::Duck => ClockChessBird::Duck,
        }
    };
    let mut birds = Vec::new();
    let mut poses = [Vec::new(), Vec::new()];
    let mut carried = Vec::new();
    let mut packed = false;
    let mut ticks = 0;
    loop {
        let state = visited.clock_state().unwrap();
        let claim = state
            .arena
            .as_ref()
            .unwrap()
            .claims
            .iter()
            .find(|claim| claim.owner == ClockArenaOwner::Chess);
        let Some(chess) = &state.chess else {
            assert!(claim.is_none(), "the visit's claim goes with it");
            break;
        };
        let claim = claim.expect("the visit holds the arena");
        assert_eq!((chess.visit_id, chess.match_number), (1, 1));
        // The visit's birds are in its own state and in no other's.
        assert!(state.duck.is_none() && state.player_duck.is_none());
        assert!(state.crow.is_none() && state.crows.is_empty());
        assert!(claim.population.bodies >= chess.actors.len());
        assert!(claim.population.within(claim.budget));
        let present: Vec<_> = chess.actors.iter().map(|actor| actor.bird).collect();
        if birds.last() != Some(&present) {
            birds.push(present);
        }
        for actor in &chess.actors {
            assert_eq!(actor.white, actor.bird == white);
            let (poses, carries) = match actor.bird {
                ClockChessBird::Crow => (&mut poses[0], false),
                ClockChessBird::Duck => (&mut poses[1], true),
            };
            if poses.last() != Some(&actor.pose) {
                poses.push(actor.pose);
            }
            // Only the duck ever has the case.
            if !carries {
                assert!(!actor.carrying);
            } else if carried.last() != Some(&actor.carrying) {
                carried.push(actor.carrying);
            }
        }
        // The state goes over the wire and comes back as it was.
        if ticks % 60 == 0 {
            let json = state.to_json().unwrap();
            assert_eq!(
                spacewars_control::ClockState::from_json(&json).unwrap(),
                state
            );
        }
        let moves = visited
            .state
            .chess()
            .current()
            .unwrap()
            .game()
            .moves()
            .len();
        if !packed && moves == 2 {
            packed = visited.state.pack_chess_visit();
        }
        visited.step(&[], TICK);
        ticks += 1;
        assert!(ticks < 6_000, "the visit never left");
    }
    // Both birds were there from the first, the crow before the duck, and
    // the crow had flown out before the duck had walked out.
    assert_eq!(
        birds,
        [
            vec![ClockChessBird::Crow, ClockChessBird::Duck],
            vec![ClockChessBird::Duck],
            vec![],
        ]
    );
    // The duck came with the case, sat without it, and left with it again.
    assert_eq!(carried, [true, false, true]);
    let [crows, ducks] = poses;
    let (first, second) = match white {
        ClockChessBird::Crow => (crows, ducks),
        ClockChessBird::Duck => (ducks, crows),
    };
    assert_eq!(
        first,
        [
            ClockChessPose::Arriving,
            ClockChessPose::Setting,
            ClockChessPose::Waiting,
            ClockChessPose::Moving,
            ClockChessPose::Waiting,
            ClockChessPose::Packing,
            ClockChessPose::Leaving,
        ]
    );
    // The visit was asked to pack up as the second move set out, and the
    // bird that made it went from its move to packing.
    assert_eq!(
        second,
        [
            ClockChessPose::Arriving,
            ClockChessPose::Setting,
            ClockChessPose::Waiting,
            ClockChessPose::Moving,
            ClockChessPose::Packing,
            ClockChessPose::Leaving,
        ]
    );
}
