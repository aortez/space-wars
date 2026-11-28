use super::*;
use crate::chess_board_visual_tests::captures;
use crate::gpu_scene::{mesh::MeshBuilder, plan};
use crate::thruster_visual_tests::{svg, write_png};
use engine_chess::{Color, Square};
use engine_chess_board::{DARK_SQUARE, LIGHT_SQUARE, MAX_PRIMITIVES};
use engine_common::{ClockEventProfile, ClockLampMode, ClockTimeFormat, RenderColor, RenderRect};
use scenario_clock::chess::{Pace, Phase};
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

/// The least rectangle holding every pixel that differs between `a` and `b`,
/// by the pixels' edges.
fn changed(a: &SharedPixelBuffer<Rgb8Pixel>, b: &SharedPixelBuffer<Rgb8Pixel>) -> Option<Pixels> {
    let width = a.width() as usize;
    let mut bounds: Option<Pixels> = None;
    for (index, (now, before)) in a.as_slice().iter().zip(b.as_slice()).enumerate() {
        if !differs(*now, *before) {
            continue;
        }
        let (x, y) = ((index % width) as f32, (index / width) as f32);
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

/// A visit's board is drawn by the CPU raster and the vector paths where its
/// stage stands, at every size the board is qualified at: packed, opening,
/// being set up, in play with a move on its way, and packed away again. The
/// GPU scene prepares each of those frames. Nothing else in the view changes
/// for the board: not the digits, which go on telling the time, the AM/PM
/// letters or the date. With `SPACEWARS_CLOCK_ARTIFACTS` set, each path's
/// pixels and the vector path's shapes are written there.
#[test]
fn a_chess_visit_draws_its_board_below_the_digits_on_both_render_paths() {
    let output = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    let setup: u64 = [Phase::Arriving, Phase::Opening, Phase::Placing]
        .iter()
        .map(|phase| phase.ticks().unwrap())
        .sum();
    let turn = Pace::CLOCK.turn;
    // The visit is asked to pack up while its second move is on its way.
    let packing = setup + 2 * turn + 10;
    let cleared = packing + 20 + Phase::Clearing.ticks().unwrap();
    let closed = cleared + Phase::Closing.ticks().unwrap();
    let gone = closed + Phase::Leaving.ticks().unwrap();
    let moments = [
        ("1-packed", 15, Phase::Arriving),
        ("2-opening", 30 + 22, Phase::Opening),
        ("3-placing", 75 + 60, Phase::Placing),
        ("4-set", setup, Phase::Playing),
        ("5-moving", setup + turn + 15, Phase::Playing),
        ("6-moved", setup + 2 * turn - 1, Phase::Playing),
        ("7-clearing", cleared - 45, Phase::Clearing),
        ("8-closing", closed - 22, Phase::Closing),
        ("9-packed", gone - 15, Phase::Leaving),
    ];
    for (name, viewport) in VIEWPORTS {
        let mut plain = clock(viewport);
        let mut visited = clock(viewport);
        visited.state.admit_chess_visit().expect("a visit");
        let visit = visited.state.chess_visit().unwrap();
        let stage = *visit.stage();
        let laid = stage.board_for(visit.side(Color::White));
        let board = pixels_of(stage.board.rect(), viewport);
        let case = pixels_of(stage.case().rect(), viewport);
        // The board stands below the face, in the lower half of the view.
        assert!(board[1] > viewport.height * 0.5, "{name}: {board:?}");

        let mut tick = 0;
        // Steps both Clocks to `until` ticks after the visit's admission.
        let mut step_to =
            |plain: &mut ClockClientScenario, visited: &mut ClockClientScenario, until: u64| {
                while tick < until {
                    // The time goes on, and the digits with it.
                    let actions = if tick == setup + 30 {
                        vec![ClockAction::set_reading(reading(9))]
                    } else {
                        Vec::new()
                    };
                    if tick == packing {
                        assert!(visited.state.pack_chess_visit(), "{name}");
                    }
                    plain.step(&actions, TICK);
                    visited.step(&actions, TICK);
                    tick += 1;
                }
            };
        let mut shapes = Vec::new();
        // The plain Clock's frame, and each path's pixels of it, drawn again
        // only when the frame changes.
        let mut plain_drawn: Option<(RenderFrame, Drawn)> = None;
        for (moment, at, phase) in moments {
            step_to(&mut plain, &mut visited, at);
            let visit = visited.state.chess_visit().expect("a visit");
            assert_eq!(visit.phase(), phase, "{name} {moment}");
            let frames = visited.render_frames(RenderBackend::Raster, viewport);
            assert_eq!(
                frames,
                visited.render_frames(RenderBackend::Vector, viewport),
                "{name} {moment}"
            );
            let background = plain.render_frames(RenderBackend::Raster, viewport);
            let added = primitives(&frames[0]) - primitives(&background[0]);
            assert!(added <= MAX_PRIMITIVES, "{name} {moment}: {added}");
            shapes.push(added);
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
            for ((backend, pixels), (_, before)) in drawn.iter().zip(before) {
                let what = format!("{name} {moment} {backend}");
                if let Some(output) = &output {
                    write_png(
                        &output.join(format!("chess-{name}-{moment}-{backend}.png")),
                        pixels,
                    );
                }
                // Only the board's own place changes, and all of it: the
                // case while it is packed, and the board's square once it
                // is open.
                let drawn =
                    changed(pixels, before).unwrap_or_else(|| panic!("{what}: nothing is drawn"));
                match phase {
                    Phase::Arriving | Phase::Leaving => {
                        assert!(same(drawn, case, EDGE), "{what}: {drawn:?}");
                        assert_eq!(added, 3, "{what}");
                    }
                    Phase::Opening | Phase::Closing => {
                        assert!(within(drawn, board, EDGE), "{what}: {drawn:?}");
                        assert!(within(case, drawn, EDGE), "{what}: {drawn:?}");
                    }
                    Phase::Placing | Phase::Playing | Phase::Clearing => {
                        assert!(same(drawn, board, EDGE), "{what}: {drawn:?}");
                    }
                }
                if moment != "4-set" {
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
            let index = moments.iter().position(|(name, ..)| *name == moment);
            shapes[index.unwrap()]
        };
        assert!(of("3-placing") < of("4-set"), "{name}");
        assert!(of("7-clearing") < of("6-moved"), "{name}");

        // Once the board has gone, the view is as it would have been.
        step_to(&mut plain, &mut visited, gone);
        assert!(visited.state.chess_visit().is_none(), "{name}");
        let frames = visited.render_frames(RenderBackend::Raster, viewport);
        assert_eq!(frames, plain.render_frames(RenderBackend::Raster, viewport));
        let played = visited.state.chess().current().unwrap().game();
        assert_eq!(played.moves().len(), 2, "{name}");
    }
}
