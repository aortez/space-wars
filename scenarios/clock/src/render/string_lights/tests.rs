use super::*;
use crate::presentation::effects::smoothstep;
use crate::theme::tests::world;
use crate::theme::{Appearance, FADE_TICKS};
use crate::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario, ClockState};
use engine_common::{
    Action, ClockEventKind, ClockEventProfile, ClockLampMode, ClockMarqueePreset, ClockRainAmount,
    ClockSettings, ClockTheme, ClockThemeChoice, ClockTimeFormat, Scenario,
};
use std::time::Duration;

/// The settings `state` has, with `choice` for the theme: how the controls
/// change the theme (#458).
fn choose(state: &ClockState, choice: ClockThemeChoice) -> Action {
    ClockAction::configure(ClockSettings {
        theme: choice,
        ..state.settings()
    })
}

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
    render(&mut frame, layout, 1.0, 0.0);
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
    for (lit, dark) in wall[1..].iter().zip(string(layout)) {
        if fill(&dark).a >= 1.0 {
            assert_ne!(fill(lit), fill(&dark), "an opaque part stays dark");
        } else {
            assert_eq!(**lit, dark, "a glow is lit");
        }
    }
}

#[test]
fn a_flash_lights_a_fading_string_as_it_lights_the_whole_one() {
    let layout = Layout::new(800.0 / 480.0);
    // The string as the frame shows it once the room's flash has lit it.
    let lit = |opacity: f32, flash: f32| {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
        render(&mut frame, layout, opacity, flash);
        super::super::lightning::flash(&mut frame, flash);
        on_layer(&frame, LIGHTS_LAYER)
            .into_iter()
            .map(fill)
            .collect::<Vec<_>>()
    };
    for flash in [0.0, 0.4, 1.0] {
        let whole = lit(1.0, flash);
        // At every step of a fade, up to the last before it is over, each
        // part keeps the color the flash gives the whole string, only fainter.
        let mut look = crate::theme::Look::settled(ClockTheme::Normal);
        look.head_for(ClockTheme::Christmas);
        for tick in 1..FADE_TICKS {
            look.advance();
            let opacity = look.drawn().lights;
            let faded = lit(opacity, flash);
            assert_eq!(faded.len(), whole.len());
            for (part, whole) in faded.iter().zip(&whole) {
                let at = format!("flash {flash}, tick {tick}");
                assert_eq!(
                    [part.r, part.g, part.b],
                    [whole.r, whole.g, whole.b],
                    "{at}"
                );
                assert_eq!(part.a, whole.a * opacity, "{at}");
            }
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

/// Every paint on the face's layers in the order it is drawn: each
/// primitive's fill and outline, or its one color.
fn face_paints(frame: &RenderFrame) -> Vec<RenderColor> {
    FACE_LAYERS
        .iter()
        .flat_map(|&z| on_layer(frame, z))
        .flat_map(|primitive| match primitive {
            RenderPrimitive::Circle(circle) => {
                [circle.fill.map(|f| f.color), circle.stroke.map(|s| s.color)]
            }
            RenderPrimitive::Polygon(polygon) => [
                polygon.fill.map(|f| f.color),
                polygon.stroke.map(|s| s.color),
            ],
            RenderPrimitive::Line(line) => [Some(line.stroke.color), None],
            RenderPrimitive::Text(text) => [Some(text.color), None],
        })
        .flatten()
        .collect()
}

/// Every color on the face's layers, setting aside how opaque it is.
fn face_colors(frame: &RenderFrame) -> Vec<[f32; 3]> {
    face_paints(frame)
        .into_iter()
        .map(|c| [c.r, c.g, c.b])
        .collect()
}

/// The colors `look` gives the face, setting aside how opaque they are.
fn face_of(look: Appearance) -> [[f32; 3]; 4] {
    [
        look.digits.fill,
        look.digits.edge,
        look.digits.unlit,
        look.label,
    ]
    .map(|c| [c.r, c.g, c.b])
}

/// The fill of one of the string's parts, all of which are filled shapes.
fn fill(primitive: &RenderPrimitive) -> RenderColor {
    match primitive {
        RenderPrimitive::Circle(circle) => circle.fill.unwrap().color,
        RenderPrimitive::Polygon(polygon) => polygon.fill.unwrap().color,
        other => panic!("{other:?}"),
    }
}

/// The string as `frame` hangs it, just after the wall.
fn hung(frame: &RenderFrame, layout: Layout) -> Vec<RenderPrimitive> {
    let parts = string(layout).len();
    on_layer(frame, LIGHTS_LAYER)[1..=parts]
        .iter()
        .map(|&part| part.clone())
        .collect()
}

/// Checks that `themed` draws the scene `normal` draws, with only the face's
/// colors changed and, if it is `hung`, the string on the wall at whatever
/// opacity its fade has reached.
fn same_scene(normal: &RenderFrame, themed: &RenderFrame, layout: Layout, hung: bool, at: &str) {
    let mut bare = themed.clone();
    if hung {
        let string: Vec<_> = string(layout).iter().map(shape).collect();
        let wall = bare
            .layers
            .iter_mut()
            .find(|layer| layer.z == LIGHTS_LAYER)
            .unwrap();
        let hanging: Vec<_> = wall.primitives[1..=string.len()]
            .iter()
            .map(shape)
            .collect();
        assert_eq!(hanging, string, "{at}: the string");
        wall.primitives.drain(1..=string.len());
    }
    assert_eq!(normal.layers.len(), bare.layers.len(), "{at}");
    let shapes = |primitives: &[RenderPrimitive]| primitives.iter().map(shape).collect::<Vec<_>>();
    for (normal, themed) in normal.layers.iter().zip(&bare.layers) {
        assert_eq!(normal.z, themed.z, "{at}");
        if FACE_LAYERS.contains(&normal.z) {
            assert_eq!(
                shapes(&normal.primitives),
                shapes(&themed.primitives),
                "{at}: layer {}",
                normal.z
            );
        } else {
            assert_eq!(
                normal.primitives, themed.primitives,
                "{at}: layer {}",
                normal.z
            );
        }
    }
}

/// A clock's change of theme, `since` ticks ago.
#[derive(Debug, Clone, Copy)]
struct Turn {
    from: ClockTheme,
    to: ClockTheme,
    since: u32,
}

impl Turn {
    /// A clock that has worn `theme` since it began.
    fn settled(theme: ClockTheme) -> Self {
        Self {
            from: theme,
            to: theme,
            since: FADE_TICKS,
        }
    }
}

/// Checks the frame of a clock in the middle of `turn` or after it, against
/// the frames of twins that wear Normal and Christmas throughout. While it
/// fades, it draws the Normal twin's scene, and every color on its face is
/// the fade's share of the way from the color the twin it turns from has
/// there to the color the twin it turns to has. It hangs the Christmas twin's
/// string, lit by any flash as that one is, only as faint as the fade. Once
/// the fade is over, it draws exactly what the twin it turned to draws.
fn turns_cleanly(
    state: &ClockState,
    turn: Turn,
    [normal, dressed]: [&RenderFrame; 2],
    layout: Layout,
    at: &str,
) {
    let twin = |theme| match theme {
        ClockTheme::Normal => normal,
        ClockTheme::Christmas => dressed,
    };
    let frame = crate::render::render_frame(state);
    if turn.since >= FADE_TICKS {
        assert!(frame == *twin(turn.to), "{at}: {turn:?} unlike its twin");
        return;
    }
    let share = smoothstep(turn.since as f32 / FADE_TICKS as f32);
    let mix = |a: f32, b: f32| a * (1.0 - share) + b * share;
    let lights = mix(
        Appearance::of(turn.from).lights,
        Appearance::of(turn.to).lights,
    );
    same_scene(normal, &frame, layout, lights > 0.0, at);
    let [drawn, from, to] = [&frame, twin(turn.from), twin(turn.to)].map(face_paints);
    assert_eq!(drawn.len(), from.len(), "{at}");
    assert_eq!(drawn.len(), to.len(), "{at}");
    for ((drawn, from), to) in drawn.iter().zip(&from).zip(&to) {
        let off = [
            drawn.r - mix(from.r, to.r),
            drawn.g - mix(from.g, to.g),
            drawn.b - mix(from.b, to.b),
        ];
        assert!(
            off.iter().all(|off| off.abs() < 1e-4) && drawn.a == from.a && drawn.a == to.a,
            "{at}: {drawn:?} is not {share} of the way from {from:?} to {to:?}"
        );
    }
    if lights > 0.0 {
        for (part, whole) in hung(&frame, layout).iter().zip(&hung(dressed, layout)) {
            let (part, whole) = (fill(part), fill(whole));
            assert_eq!(
                [part.r, part.g, part.b],
                [whole.r, whole.g, whole.b],
                "{at}: the string's colors"
            );
            assert_eq!(part.a, whole.a * lights, "{at}: the string's opacity");
        }
    }
}

/// Whether an event or a visit is still going on.
fn busy(state: &ClockState) -> bool {
    state.event_kind().is_some()
        || state.has_duck_visit()
        || state.spider_state().is_some()
        || state.crow_state().is_some()
}

/// What `wears_the_theme` saw.
struct Worn {
    /// How many ticks the event or the visit ran.
    ticks: u32,
    /// How many frames a lightning flash lit while the turning clock faded.
    flashes_in_fades: u32,
}

/// Starts `kind` in three clocks: one that wears Normal throughout, one that
/// wears Christmas from the start, and one given Christmas `FIRST` ticks in
/// and then Normal and Christmas by turns every `PERIOD` ticks, so that it is
/// in the middle of a fade for most of the event. On every frame, the
/// Christmas clock draws the Normal one's scene with only the face recolored
/// and the string hung, and nothing of Normal's face, and the turning clock
/// passes `turns_cleanly`. When it is over, each clock settles on the theme
/// it was last given.
fn wears_the_theme(kind: ClockEventKind, tweak: impl Fn(&mut ClockConfig)) -> Worn {
    const FIRST: u32 = 20;
    const PERIOD: u32 = FADE_TICKS + 10;
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    let config = |config: &mut ClockConfig| {
        config.time_format = ClockTimeFormat::TwelveHour;
        tweak(config);
    };
    let mut normal = clock_with(aspect, ClockThemeChoice::Normal, config);
    let mut dressed = clock_with(aspect, ClockThemeChoice::Christmas, config);
    let mut turning = clock_with(aspect, ClockThemeChoice::Normal, config);
    for state in [&mut normal, &mut dressed, &mut turning] {
        ClockScenario::step(state, &[ClockAction::trigger_event(kind)], Duration::ZERO);
    }
    assert!(busy(&normal), "{kind:?} never started");
    let mut turn = Turn::settled(ClockTheme::Normal);
    let mut worn = Worn {
        ticks: 0,
        flashes_in_fades: 0,
    };
    while busy(&normal) {
        let mut actions = Vec::new();
        if worn.ticks >= FIRST && (worn.ticks - FIRST) % PERIOD == 0 {
            let (to, choice) = if (worn.ticks - FIRST) / PERIOD % 2 == 0 {
                (ClockTheme::Christmas, ClockThemeChoice::Christmas)
            } else {
                (ClockTheme::Normal, ClockThemeChoice::Normal)
            };
            actions.push(choose(&turning, choice));
            // The last change's fade is over, so this one turns from its
            // theme.
            turn = Turn {
                from: turn.to,
                to,
                since: 0,
            };
        }
        ClockScenario::step(&mut normal, &[], TICK);
        ClockScenario::step(&mut dressed, &[], TICK);
        ClockScenario::step(&mut turning, &actions, TICK);
        worn.ticks += 1;
        turn.since += 1;
        let at = format!("{kind:?} tick {}", worn.ticks);
        assert_eq!(busy(&dressed), busy(&normal), "{at}");
        assert_eq!(busy(&turning), busy(&normal), "{at}");
        // The theme leaves the world alone: the event, its bodies, the
        // visitors and the residents are the same in all three.
        let plain = world(&normal);
        assert_eq!(world(&dressed), plain, "{at}");
        assert_eq!(world(&turning), plain, "{at}");
        let normal_frame = crate::render::render_frame(&normal);
        let dressed_frame = crate::render::render_frame(&dressed);
        same_scene(&normal_frame, &dressed_frame, layout, true, &at);
        for color in face_colors(&dressed_frame) {
            assert!(
                !face_of(Appearance::NORMAL).contains(&color),
                "{at}: {color:?}"
            );
        }
        turns_cleanly(&turning, turn, [&normal_frame, &dressed_frame], layout, &at);
        if turn.since < FADE_TICKS && flash_level(&turning) > 0.0 {
            worn.flashes_in_fades += 1;
        }
        assert!(worn.ticks < 20_000, "{kind:?} never ended");
    }
    assert!(
        worn.ticks > FIRST + FADE_TICKS,
        "{kind:?}: ran {} ticks",
        worn.ticks
    );
    // Each ends on the theme it was last given, not the one it began in.
    assert_eq!(dressed.appearance(), Appearance::CHRISTMAS, "{kind:?}");
    assert_eq!(dressed.palette(), Appearance::CHRISTMAS.digits, "{kind:?}");
    while turn.since < FADE_TICKS {
        for state in [&mut normal, &mut dressed, &mut turning] {
            ClockScenario::step(state, &[], TICK);
        }
        turn.since += 1;
    }
    let frames = [&normal, &dressed].map(crate::render::render_frame);
    let at = format!("{kind:?} over");
    turns_cleanly(&turning, turn, [&frames[0], &frames[1]], layout, &at);
    worn
}

#[test]
fn falling_explosion_coins_and_meltdown_wear_the_theme() {
    for kind in [
        ClockEventKind::Falling,
        ClockEventKind::Explosion,
        ClockEventKind::Coins,
        ClockEventKind::Meltdown,
    ] {
        wears_the_theme(kind, |_| {});
    }
}

#[test]
fn rain_and_the_aquarium_wear_the_theme() {
    let rain = wears_the_theme(ClockEventKind::Rain, |config| {
        config.rain_amount = ClockRainAmount::Heavy;
    });
    // The lightning struck while the string faded, and lit it as it lights
    // the whole string.
    assert!(rain.flashes_in_fades > 0, "no flash while the string faded");
    wears_the_theme(ClockEventKind::Aquarium, |_| {});
}

#[test]
fn color_cycle_and_the_face_events_wear_the_theme() {
    for kind in [
        ClockEventKind::ColorCycle,
        ClockEventKind::CellSwap,
        ClockEventKind::SlotMachine,
        ClockEventKind::Grass,
        ClockEventKind::Tanks,
    ] {
        wears_the_theme(kind, |_| {});
    }
}

#[test]
fn every_marquee_wears_the_theme() {
    for preset in [
        ClockMarqueePreset::ClockChase,
        ClockMarqueePreset::ClockWave,
        ClockMarqueePreset::ClockSpin,
        ClockMarqueePreset::DigitSpin,
        ClockMarqueePreset::TextScroll,
        ClockMarqueePreset::TextRibbon,
        ClockMarqueePreset::TextSpin,
    ] {
        wears_the_theme(ClockEventKind::Marquee, |config| {
            config.marquee_preset = preset;
        });
    }
}

#[test]
fn a_visit_wears_the_theme() {
    for kind in [
        ClockEventKind::Duck,
        ClockEventKind::Spider,
        ClockEventKind::Crow,
    ] {
        let ticks = wears_the_theme(kind, |_| {}).ticks;
        assert!(ticks > 100, "{kind:?}: visited for {ticks} ticks");
    }
}

#[test]
fn auto_turns_the_theme_in_the_middle_of_the_minute_slide() {
    // Calm slides the digits whenever the minute turns over, so Auto's
    // change of theme at 18:00 and at 06:00 lands on a slide.
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    for (before, (hour, minute, second), after) in [
        (ClockTheme::Normal, (18, 0, 0), ClockTheme::Christmas),
        (ClockTheme::Christmas, (6, 0, 0), ClockTheme::Normal),
    ] {
        let clock = |theme| {
            let mut state = ClockScenario::init(
                ClockConfig {
                    aspect_ratio: aspect,
                    theme,
                    show_date: true,
                    lamps: ClockLampMode::Steady,
                    event_profile: ClockEventProfile::Calm,
                    time_format: ClockTimeFormat::TwelveHour,
                    ..ClockConfig::default()
                },
                1,
            );
            read(&mut state, (hour + 23) % 24, 59, 59, Duration::ZERO);
            state
        };
        let mut normal = clock(ClockThemeChoice::Normal);
        let mut dressed = clock(ClockThemeChoice::Christmas);
        let mut auto = clock(ClockThemeChoice::Auto);
        let frames = |normal: &ClockState, dressed: &ClockState| {
            [normal, dressed].map(crate::render::render_frame)
        };
        // A moment of the last minute, in the theme of the hour before.
        for tick in 1..=10 {
            for state in [&mut normal, &mut dressed, &mut auto] {
                ClockScenario::step(state, &[], TICK);
            }
            let [n, d] = frames(&normal, &dressed);
            let at = format!("{hour}:00, {tick} ticks before");
            turns_cleanly(&auto, Turn::settled(before), [&n, &d], layout, &at);
        }
        for state in [&mut normal, &mut dressed, &mut auto] {
            read(state, hour, minute, second, TICK);
        }
        // The turn of the hour starts the slide in all three, and Auto's fade
        // runs inside it.
        let mut turn = Turn {
            from: before,
            to: after,
            since: 1,
        };
        loop {
            let at = format!("{hour}:00, tick {}", turn.since);
            for state in [&normal, &dressed] {
                assert_eq!(state.event_kind(), auto.event_kind(), "{at}");
            }
            if turn.since < FADE_TICKS {
                assert_eq!(auto.event_kind(), Some(ClockEventKind::DigitSlide), "{at}");
            } else if auto.event_kind().is_none() {
                break;
            }
            let [n, d] = frames(&normal, &dressed);
            turns_cleanly(&auto, turn, [&n, &d], layout, &at);
            for state in [&mut normal, &mut dressed, &mut auto] {
                ClockScenario::step(state, &[], TICK);
            }
            turn.since += 1;
            assert!(turn.since < 1000, "{at}: the slide never ended");
        }
        assert!(turn.since > 40, "{hour}:00: slid for {} ticks", turn.since);
        // The slide ends on the theme of the new hour.
        assert_eq!(auto.appearance(), Appearance::of(after), "{hour}:00");
        let [n, d] = frames(&normal, &dressed);
        turns_cleanly(&auto, turn, [&n, &d], layout, "after the slide");
    }
}

/// Whether `frame` writes `text` anywhere.
fn shows(frame: &RenderFrame, text: &str) -> bool {
    frame
        .layers
        .iter()
        .flat_map(|layer| &layer.primitives)
        .any(|primitive| matches!(primitive, RenderPrimitive::Text(t) if t.text == text))
}

#[test]
fn a_notice_and_a_players_tag_wear_the_date_lines_color_through_a_fade() {
    const CHANGE: u32 = 10;
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    // A seated player's duck is tagged by the date line, and a player who
    // asks for a duck during Tanks is told no in the date line's place.
    for (start, text) in [
        (vec![ClockAction::toggle_player_duck(1)], "P1 DUCK"),
        (
            vec![
                ClockAction::trigger_event(ClockEventKind::Tanks),
                ClockAction::toggle_player_duck(1),
            ],
            "Tanks is using the arena",
        ),
    ] {
        let make = |theme| {
            let mut state = clock(aspect, theme, ClockTimeFormat::TwelveHour);
            ClockScenario::step(&mut state, &start, Duration::ZERO);
            state
        };
        let mut normal = make(ClockThemeChoice::Normal);
        let mut dressed = make(ClockThemeChoice::Christmas);
        let mut turning = make(ClockThemeChoice::Normal);
        let mut turn = Turn::settled(ClockTheme::Normal);
        for tick in 0..CHANGE + FADE_TICKS + 5 {
            let mut actions = Vec::new();
            if tick == CHANGE {
                actions.push(choose(&turning, ClockThemeChoice::Christmas));
                turn = Turn {
                    from: ClockTheme::Normal,
                    to: ClockTheme::Christmas,
                    since: 0,
                };
            }
            ClockScenario::step(&mut normal, &[], TICK);
            ClockScenario::step(&mut dressed, &[], TICK);
            ClockScenario::step(&mut turning, &actions, TICK);
            turn.since += 1;
            let at = format!("{text}, tick {tick}");
            let frames = [&normal, &dressed].map(crate::render::render_frame);
            for frame in &frames {
                assert!(shows(frame, text), "{at}");
            }
            same_scene(&frames[0], &frames[1], layout, true, &at);
            for color in face_colors(&frames[1]) {
                assert!(
                    !face_of(Appearance::NORMAL).contains(&color),
                    "{at}: {color:?}"
                );
            }
            turns_cleanly(&turning, turn, [&frames[0], &frames[1]], layout, &at);
        }
    }
}

#[test]
fn the_string_fades_in_and_out_with_its_theme() {
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    let full = string(layout);
    let mut state = clock(
        aspect,
        ClockThemeChoice::Normal,
        ClockTimeFormat::TwelveHour,
    );
    ClockScenario::step(&mut state, &[], TICK);
    for (theme, rising) in [
        (ClockThemeChoice::Christmas, true),
        (ClockThemeChoice::Normal, false),
    ] {
        let change = choose(&state, theme);
        ClockScenario::step(&mut state, &[change], TICK);
        let mut last = state.appearance().lights;
        for tick in 1..FADE_TICKS {
            // The whole string hangs at every step of the fade, each part as
            // faint as the fade has come.
            let frame = crate::render::render_frame(&state);
            let wall = on_layer(&frame, LIGHTS_LAYER);
            for (part, whole) in wall[1..=full.len()].iter().zip(&full) {
                assert_eq!(shape(part), shape(whole), "{theme:?} tick {tick}");
                assert_eq!(fill(part).a, fill(whole).a * last, "{theme:?} tick {tick}");
            }
            ClockScenario::step(&mut state, &[], TICK);
            let lights = state.appearance().lights;
            assert_eq!(lights > last, rising, "{theme:?} tick {tick}");
            last = lights;
        }
        assert_eq!(last, if rising { 1.0 } else { 0.0 }, "{theme:?}");
    }
    // Faded out, it is gone from the wall.
    let plain = clock(
        aspect,
        ClockThemeChoice::Normal,
        ClockTimeFormat::TwelveHour,
    );
    assert_eq!(
        on_layer(&crate::render::render_frame(&state), LIGHTS_LAYER),
        on_layer(&crate::render::render_frame(&plain), LIGHTS_LAYER)
    );
}

#[test]
fn a_theme_comes_down_completely_when_the_night_ends() {
    let aspect = 800.0 / 480.0;
    let format = ClockTimeFormat::TwelveHour;
    let mut auto = clock(aspect, ClockThemeChoice::Auto, format);
    let mut normal = clock(aspect, ClockThemeChoice::Normal, format);
    let layout = Layout::new(aspect);
    without_string(&crate::render::render_frame(&auto), layout);
    for state in [&mut auto, &mut normal] {
        read(state, 5, 59, 59, TICK);
        read(state, 6, 0, 0, TICK);
    }
    // The theme fades over half a second, and then nothing of it is left.
    for tick in 1..FADE_TICKS {
        assert_ne!(
            crate::render::render_frame(&auto),
            crate::render::render_frame(&normal),
            "tick {tick}"
        );
        for state in [&mut auto, &mut normal] {
            ClockScenario::step(state, &[], TICK);
        }
    }
    assert_eq!(
        crate::render::render_frame(&auto),
        crate::render::render_frame(&normal)
    );
}
