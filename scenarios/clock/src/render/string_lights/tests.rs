use super::*;
use crate::theme::Appearance;
use crate::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario};
use engine_common::{
    ClockEventKind, ClockEventProfile, ClockLampMode, ClockMarqueePreset, ClockThemeChoice,
    ClockTimeFormat, Scenario,
};
use std::time::Duration;

const ASPECTS: [f32; 7] = [0.25, 0.6, 1.0, 4.0 / 3.0, 800.0 / 480.0, 2.0, 4.0];
const TICK: Duration = Duration::from_nanos(16_666_667);
/// The layers whose colors follow the face: its cells and their glow, its
/// AM/PM mark, and the date line over the canopy.
const FACE_LAYERS: [i32; 5] = [
    GLOW_LAYER,
    INACTIVE_CELL_LAYER,
    ACTIVE_CELL_LAYER,
    LABEL_LAYER,
    100,
];

/// Aspects from the narrowest strip to the widest, much closer together than
/// the layouts the Clock is drawn at.
fn sweep() -> impl Iterator<Item = f32> {
    (0..=240).map(|i| 0.25 * 16f32.powf(i as f32 / 240.0))
}

/// An idle clock with its date, under Steady lamps, which never change a
/// frame on their own.
fn clock(aspect_ratio: f32, theme: ClockThemeChoice, format: ClockTimeFormat) -> ClockState {
    clock_with(aspect_ratio, theme, |config| config.time_format = format)
}

fn clock_with(
    aspect_ratio: f32,
    theme: ClockThemeChoice,
    tweak: impl FnOnce(&mut ClockConfig),
) -> ClockState {
    let mut config = ClockConfig {
        aspect_ratio,
        theme,
        show_date: true,
        lamps: ClockLampMode::Steady,
        event_profile: ClockEventProfile::Off,
        ..ClockConfig::default()
    };
    tweak(&mut config);
    let mut state = ClockScenario::init(config, 1);
    read(&mut state, 21, 34, 0, Duration::ZERO);
    state
}

/// Christmas Eve at `hour`:`minute`:`second`, delivered on a step of `dt`.
fn read(state: &mut ClockState, hour: u8, minute: u8, second: u8, dt: Duration) {
    let reading = ClockReading::new(hour, minute, second)
        .unwrap()
        .with_date(ClockDate::new(2026, 12, 24).unwrap());
    ClockScenario::step(state, &[ClockAction::set_reading(reading)], dt);
}

fn on_layer(frame: &RenderFrame, z: i32) -> Vec<&RenderPrimitive> {
    frame
        .layers
        .iter()
        .filter(|layer| layer.z == z)
        .flat_map(|layer| &layer.primitives)
        .collect()
}

/// The string's own primitives, drawn alone.
fn string(layout: Layout) -> Vec<RenderPrimitive> {
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
    render(&mut frame, layout);
    on_layer(&frame, LIGHTS_LAYER)
        .into_iter()
        .cloned()
        .collect()
}

/// `frame` with the string taken off the wall, checking that it hangs there,
/// drawn just after the wall itself.
fn without_string(frame: &RenderFrame, layout: Layout) -> RenderFrame {
    let string = string(layout);
    let mut frame = frame.clone();
    let wall = frame
        .layers
        .iter_mut()
        .find(|layer| layer.z == LIGHTS_LAYER)
        .unwrap();
    assert_eq!(wall.primitives[1..=string.len()], string[..]);
    wall.primitives.drain(1..=string.len());
    frame
}

/// The least and the most x and y a primitive covers.
fn extent(primitive: &RenderPrimitive) -> [f32; 4] {
    match primitive {
        RenderPrimitive::Circle(circle) => [
            circle.center.x - circle.radius,
            circle.center.y - circle.radius,
            circle.center.x + circle.radius,
            circle.center.y + circle.radius,
        ],
        RenderPrimitive::Polygon(polygon) => polygon.points.iter().fold(
            [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
            |[x0, y0, x1, y1], p| [x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)],
        ),
        other => panic!("the lights draw circles and polygons only: {other:?}"),
    }
}

fn overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// The primitive with every color set aside, leaving its shape.
fn shape(primitive: &RenderPrimitive) -> RenderPrimitive {
    let mut shape = primitive.clone();
    match &mut shape {
        RenderPrimitive::Circle(circle) => {
            circle.fill = circle.fill.map(|_| Fill::new(RenderColor::BLACK));
            circle.stroke = circle
                .stroke
                .map(|s| Stroke::new(RenderColor::BLACK, s.width));
        }
        RenderPrimitive::Polygon(polygon) => {
            polygon.fill = polygon.fill.map(|_| Fill::new(RenderColor::BLACK));
            polygon.stroke = polygon
                .stroke
                .map(|s| Stroke::new(RenderColor::BLACK, s.width));
        }
        RenderPrimitive::Line(line) => line.stroke.color = RenderColor::BLACK,
        RenderPrimitive::Text(text) => text.color = RenderColor::BLACK,
    }
    shape
}

fn crosses(a: RenderPoint, b: RenderPoint, c: RenderPoint, d: RenderPoint) -> bool {
    let side = |p: RenderPoint, q: RenderPoint, r: RenderPoint| {
        (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x)
    };
    side(a, b, c) * side(a, b, d) < 0.0 && side(c, d, a) * side(c, d, b) < 0.0
}

/// No two edges of `points` cross, other than neighbors meeting at a corner.
fn simple(points: &[RenderPoint]) -> bool {
    let n = points.len();
    (0..n).all(|i| {
        (i + 2..n).all(|j| {
            (i == 0 && j == n - 1)
                || !crosses(
                    points[i],
                    points[(i + 1) % n],
                    points[j],
                    points[(j + 1) % n],
                )
        })
    })
}

#[test]
fn the_string_hangs_under_the_canopy_from_rail_to_rail_and_clear_of_the_face() {
    for aspect in ASPECTS {
        for format in [ClockTimeFormat::TwelveHour, ClockTimeFormat::TwentyFourHour] {
            let state = clock(aspect, ClockThemeChoice::Christmas, format);
            let frame = crate::render::render_frame(&state);
            let layout = Layout::new(state.aspect_ratio());
            let lights = string(layout);
            let [x0, y0, x1, y1] = lights.iter().map(extent).fold(
                [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
                |[a, b, c, d], [e, f, g, h]| [a.min(e), b.min(f), c.max(g), d.max(h)],
            );
            // It is tied to one rail's inner face and to the other's, and
            // pinned between them, all at one height under the canopy.
            let pins = hang(layout).pins;
            let (first, last) = (pins[0], pins[pins.len() - 1]);
            let unit = layout.frame_width;
            assert!(
                (first.x - (layout.bounds_min.x + unit)).abs() < 1e-3
                    && (last.x - (layout.bounds_max.x - unit)).abs() < 1e-3,
                "{aspect}: {first:?} to {last:?}"
            );
            assert!(
                pins.iter().all(|pin| pin.y == first.y),
                "{aspect}: {pins:?}"
            );
            let half = wire_half(layout);
            assert!(
                x0 <= first.x + half && x1 >= last.x - half,
                "{aspect}: the string covers {x0}..{x1}, not {}..{}",
                first.x,
                last.x
            );
            assert!(
                x0 >= layout.bounds_min.x && x1 <= layout.bounds_max.x,
                "{aspect}: {x0}..{x1} in {:?}..{:?}",
                layout.bounds_min,
                layout.bounds_max
            );
            assert!(y1 < layout.canopy_y, "{aspect}");
            // The face's cells and its AM/PM mark all stay below the lowest
            // bulb's glow.
            let face = [INACTIVE_CELL_LAYER, ACTIVE_CELL_LAYER, LABEL_LAYER]
                .into_iter()
                .flat_map(|z| on_layer(&frame, z))
                .map(|p| extent(p)[3])
                .fold(f32::MIN, f32::max);
            assert!(
                y0 > face,
                "{aspect} {format:?}: lights reach {y0}, face {face}"
            );
        }
    }
}

#[test]
fn nothing_of_the_string_touches_the_canopy_where_its_holes_open() {
    // The spider's hatch and the ceiling's openings are cut in the canopy,
    // from its underside up, so a string kept under it never hangs from a
    // hole. The clips are the string's highest parts.
    for aspect in sweep() {
        let layout = Layout::new(aspect);
        let top = string(layout)
            .iter()
            .map(|p| extent(p)[3])
            .fold(f32::MIN, f32::max);
        let clip = hang(layout).pins[0].y + clip_radius(layout);
        assert!((top - clip).abs() < 1e-3, "{aspect}: {top} against {clip}");
        assert!(top < layout.canopy_y, "{aspect}: {top}");
        let mut hatch = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
        super::super::spider::render_hatch(&mut hatch, layout);
        let hatch = on_layer(&hatch, ARENA_LAYER);
        assert!(!hatch.is_empty());
        for primitive in hatch {
            assert!(extent(primitive)[1] >= layout.canopy_y, "{aspect}");
        }
    }
}

#[test]
fn the_string_is_bounded_and_its_bulbs_take_their_colors_in_turn() {
    // A wire for each swag and a clip between each two; a glow, a socket, a
    // bulb and its core for each bulb.
    let per_bulb = HALOS.len() + 3;
    for aspect in sweep() {
        let layout = Layout::new(aspect);
        let hang = hang(layout);
        let swags = hang.pins.len() - 1;
        assert!((2..=MAX_SWAGS).contains(&swags), "{aspect}: {swags}");
        assert_eq!(hang.bulbs.len(), swags * BULBS_PER_SWAG, "{aspect}");
        let drawn = string(layout);
        let strung = 2 * swags - 1;
        assert_eq!(
            drawn.len(),
            strung + hang.bulbs.len() * per_bulb,
            "{aspect}"
        );
        assert!(drawn.len() < 2 * MAX_SWAGS + MAX_SWAGS * BULBS_PER_SWAG * per_bulb);
        let bulbs: Vec<_> = drawn[strung..]
            .chunks(per_bulb)
            .map(|parts| match &parts[HALOS.len() + 1] {
                RenderPrimitive::Polygon(bulb) => bulb.fill.unwrap().color,
                other => panic!("a bulb is a polygon: {other:?}"),
            })
            .collect();
        for (index, color) in bulbs.iter().enumerate() {
            assert_eq!(*color, BULB_COLORS[index % BULB_COLORS.len()], "{aspect}");
        }
        for primitive in &drawn {
            if let RenderPrimitive::Polygon(polygon) = primitive {
                assert!(simple(&polygon.points), "{aspect}: {polygon:?}");
            }
        }
    }
}

#[test]
fn christmas_changes_only_the_faces_colors_and_hangs_the_string_and_normal_hangs_nothing() {
    for aspect in ASPECTS {
        for format in [ClockTimeFormat::TwelveHour, ClockTimeFormat::TwentyFourHour] {
            let layout = Layout::new(aspect);
            let normal =
                crate::render::render_frame(&clock(aspect, ClockThemeChoice::Normal, format));
            let christmas =
                crate::render::render_frame(&clock(aspect, ClockThemeChoice::Christmas, format));
            assert_eq!(on_layer(&normal, LIGHTS_LAYER).len(), 1, "a bare wall");
            let bare = without_string(&christmas, layout);
            let z = |frame: &RenderFrame| frame.layers.iter().map(|l| l.z).collect::<Vec<_>>();
            assert_eq!(z(&normal), z(&bare), "{aspect} {format:?}");
            for (normal, christmas) in normal.layers.iter().zip(&bare.layers) {
                if FACE_LAYERS.contains(&normal.z) {
                    let shapes = |primitives: &[RenderPrimitive]| {
                        primitives.iter().map(shape).collect::<Vec<_>>()
                    };
                    assert_eq!(
                        shapes(&normal.primitives),
                        shapes(&christmas.primitives),
                        "{aspect} {format:?}: layer {}",
                        normal.z
                    );
                } else {
                    assert_eq!(
                        normal.primitives, christmas.primitives,
                        "{aspect} {format:?}: layer {}",
                        normal.z
                    );
                }
            }
            assert_ne!(normal, bare);
        }
    }
}

#[test]
fn the_string_hangs_behind_everything_in_the_room() {
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    let state = clock_with(aspect, ClockThemeChoice::Christmas, |config| {
        config.marquee_preset = ClockMarqueePreset::ClockSpin;
    });
    let frame = crate::render::render_frame(&state);
    // It is the first thing drawn after the wall.
    without_string(&frame, layout);
    assert!(frame.layers.iter().all(|layer| layer.z >= LIGHTS_LAYER));
    // So Clock Spin, which carries the lit cells up past the bulbs, shows
    // them in front of the string all the way.
    let mut state = state;
    ClockScenario::step(
        &mut state,
        &[ClockAction::trigger_event(ClockEventKind::Marquee)],
        Duration::ZERO,
    );
    let lights = string(layout);
    let opaque: Vec<_> = lights
        .iter()
        .filter(|p| match p {
            RenderPrimitive::Polygon(polygon) => polygon.fill.is_some_and(|f| f.color.a >= 1.0),
            _ => false,
        })
        .map(extent)
        .collect();
    let mut crossings = 0;
    while state.event_kind() == Some(ClockEventKind::Marquee) {
        let frame = crate::render::render_frame(&state);
        // Where the string is drawn, and where each lit cell is, in order.
        let order: Vec<(i32, usize, &RenderPrimitive)> = frame
            .layers
            .iter()
            .flat_map(|layer| {
                layer
                    .primitives
                    .iter()
                    .enumerate()
                    .map(move |(i, p)| (layer.z, i, p))
            })
            .collect();
        let string_at = order
            .iter()
            .position(|&(_, _, p)| *p == lights[lights.len() - 1])
            .unwrap();
        for (at, &(z, _, cell)) in order.iter().enumerate() {
            if z != ACTIVE_CELL_LAYER || !opaque.iter().any(|&o| overlap(o, extent(cell))) {
                continue;
            }
            crossings += 1;
            assert!(at > string_at, "a lit cell drawn under the string");
        }
        ClockScenario::step(&mut state, &[], TICK);
    }
    assert!(crossings > 0, "the cells never reached the string");
}

#[test]
fn the_lightning_flash_lights_the_string_but_not_its_glow() {
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    let state = clock(
        aspect,
        ClockThemeChoice::Christmas,
        ClockTimeFormat::TwelveHour,
    );
    let mut frame = crate::render::render_frame(&state);
    super::super::lightning::flash(&mut frame, 1.0);
    let wall = on_layer(&frame, LIGHTS_LAYER);
    let fill = |p: &RenderPrimitive| match p {
        RenderPrimitive::Circle(circle) => circle.fill.unwrap().color,
        RenderPrimitive::Polygon(polygon) => polygon.fill.unwrap().color,
        other => panic!("{other:?}"),
    };
    for (lit, dark) in wall[1..].iter().zip(string(layout)) {
        if fill(&dark).a >= 1.0 {
            assert_ne!(fill(lit), fill(&dark), "an opaque part stays dark");
        } else {
            assert_eq!(**lit, dark, "a glow is lit");
        }
    }
}

#[test]
fn the_minute_slide_keeps_the_themes_face() {
    for theme in [ClockThemeChoice::Normal, ClockThemeChoice::Christmas] {
        // Calm slides the digits whenever the minute turns over.
        let mut state = clock_with(800.0 / 480.0, theme, |config| {
            config.event_profile = ClockEventProfile::Calm;
            config.time_format = ClockTimeFormat::TwelveHour;
        });
        read(&mut state, 21, 34, 59, Duration::ZERO);
        let still = face_colors(&crate::render::render_frame(&state));
        read(&mut state, 21, 35, 0, TICK);
        let face = Appearance::of(state.theme());
        let mut ticks = 0;
        while state.event_kind() == Some(ClockEventKind::DigitSlide) {
            assert_eq!(state.palette(), face.digits, "{theme:?} tick {ticks}");
            assert_eq!(state.appearance(), face, "{theme:?} tick {ticks}");
            // The digits, the PM mark and the date line move in the colors
            // they rest in.
            for color in face_colors(&crate::render::render_frame(&state)) {
                assert!(still.contains(&color), "{theme:?} tick {ticks}: {color:?}");
            }
            ClockScenario::step(&mut state, &[], TICK);
            ticks += 1;
        }
        assert!(ticks > 40, "{theme:?}: slid for {ticks} ticks");
    }
}

/// Every color on the face's layers, setting aside how opaque it is.
fn face_colors(frame: &RenderFrame) -> Vec<[f32; 3]> {
    let rgb = |c: RenderColor| [c.r, c.g, c.b];
    FACE_LAYERS
        .iter()
        .flat_map(|&z| on_layer(frame, z))
        .flat_map(|primitive| match primitive {
            RenderPrimitive::Circle(circle) => [
                circle.fill.map(|f| rgb(f.color)),
                circle.stroke.map(|s| rgb(s.color)),
            ],
            RenderPrimitive::Polygon(polygon) => [
                polygon.fill.map(|f| rgb(f.color)),
                polygon.stroke.map(|s| rgb(s.color)),
            ],
            RenderPrimitive::Line(line) => [Some(rgb(line.stroke.color)), None],
            RenderPrimitive::Text(text) => [Some(rgb(text.color)), None],
        })
        .flatten()
        .collect()
}

#[test]
fn an_event_draws_the_normal_face_under_the_string_until_events_follow_the_theme() {
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    for kind in [
        ClockEventKind::Falling,
        ClockEventKind::ColorCycle,
        ClockEventKind::Meltdown,
        ClockEventKind::Explosion,
    ] {
        let format = ClockTimeFormat::TwelveHour;
        let mut normal = clock(aspect, ClockThemeChoice::Normal, format);
        let mut christmas = clock(aspect, ClockThemeChoice::Christmas, format);
        for state in [&mut normal, &mut christmas] {
            ClockScenario::step(state, &[ClockAction::trigger_event(kind)], Duration::ZERO);
        }
        assert_eq!(christmas.event_kind(), Some(kind));
        // The string stays up and adds the same primitives to every frame;
        // the digits, the AM/PM mark and the date line are Normal's.
        let mut ticks = 0;
        while christmas.event_kind().is_some() {
            assert_eq!(
                without_string(&crate::render::render_frame(&christmas), layout),
                crate::render::render_frame(&normal),
                "{kind:?} tick {ticks}"
            );
            for state in [&mut normal, &mut christmas] {
                ClockScenario::step(state, &[], TICK);
            }
            ticks += 1;
        }
        assert_eq!(normal.event_kind(), None, "{kind:?}");
        assert!(ticks > 100, "{kind:?}: ran {ticks} ticks");
        assert_eq!(christmas.appearance(), Appearance::CHRISTMAS, "{kind:?}");
        assert_eq!(
            christmas.palette(),
            Appearance::CHRISTMAS.digits,
            "{kind:?}"
        );
    }
}

#[test]
fn a_visit_wears_the_themes_face_under_the_string() {
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    let normal_face = [
        DigitPalette::NORMAL.fill,
        DigitPalette::NORMAL.edge,
        DigitPalette::NORMAL.unlit,
        Appearance::NORMAL.label,
    ]
    .map(|c| [c.r, c.g, c.b]);
    let visiting = |state: &ClockState| {
        state.has_duck_visit() || state.spider_state().is_some() || state.crow_state().is_some()
    };
    for kind in [
        ClockEventKind::Duck,
        ClockEventKind::Spider,
        ClockEventKind::Crow,
    ] {
        let format = ClockTimeFormat::TwelveHour;
        let mut normal = clock(aspect, ClockThemeChoice::Normal, format);
        let mut christmas = clock(aspect, ClockThemeChoice::Christmas, format);
        for state in [&mut normal, &mut christmas] {
            ClockScenario::step(state, &[ClockAction::trigger_event(kind)], Duration::ZERO);
        }
        let mut ticks = 0;
        while visiting(&christmas) {
            let normal_frame = crate::render::render_frame(&normal);
            let christmas_frame = crate::render::render_frame(&christmas);
            // The visitor and the room are the same in both, the string stays
            // up, and only the face's colors differ.
            let bare = without_string(&christmas_frame, layout);
            assert_eq!(normal_frame.layers.len(), bare.layers.len(), "{kind:?}");
            for (normal, christmas) in normal_frame.layers.iter().zip(&bare.layers) {
                assert_eq!(normal.z, christmas.z, "{kind:?} tick {ticks}");
                if FACE_LAYERS.contains(&normal.z) {
                    let shapes = |primitives: &[RenderPrimitive]| {
                        primitives.iter().map(shape).collect::<Vec<_>>()
                    };
                    assert_eq!(
                        shapes(&normal.primitives),
                        shapes(&christmas.primitives),
                        "{kind:?} tick {ticks}: layer {}",
                        normal.z
                    );
                } else {
                    assert_eq!(
                        normal.primitives, christmas.primitives,
                        "{kind:?} tick {ticks}: layer {}",
                        normal.z
                    );
                }
            }
            // And none of them is Normal's.
            for color in face_colors(&christmas_frame) {
                assert!(
                    !normal_face.contains(&color),
                    "{kind:?} tick {ticks}: {color:?}"
                );
            }
            for state in [&mut normal, &mut christmas] {
                ClockScenario::step(state, &[], TICK);
            }
            ticks += 1;
            assert!(ticks < 20_000, "{kind:?} never left");
        }
        assert!(!visiting(&normal), "{kind:?}");
        assert!(ticks > 100, "{kind:?}: visited for {ticks} ticks");
    }
}

#[test]
fn a_theme_comes_down_completely_when_the_night_ends() {
    let mut auto = clock(
        800.0 / 480.0,
        ClockThemeChoice::Auto,
        ClockTimeFormat::TwelveHour,
    );
    let mut normal = clock(
        800.0 / 480.0,
        ClockThemeChoice::Normal,
        ClockTimeFormat::TwelveHour,
    );
    let layout = Layout::new(800.0 / 480.0);
    without_string(&crate::render::render_frame(&auto), layout);
    for state in [&mut auto, &mut normal] {
        read(state, 6, 0, 0, Duration::ZERO);
    }
    assert_eq!(
        crate::render::render_frame(&auto),
        crate::render::render_frame(&normal)
    );
}
