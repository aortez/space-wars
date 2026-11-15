use super::*;
use crate::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario};
use engine_common::{
    ClockEventProfile, ClockLampMode, ClockThemeChoice, ClockTimeFormat, Scenario,
};
use std::time::Duration;

const ASPECTS: [f32; 7] = [0.25, 0.6, 1.0, 4.0 / 3.0, 800.0 / 480.0, 2.0, 4.0];

/// An idle clock with its date, under Steady lamps, which never change a
/// frame on their own.
fn clock(aspect_ratio: f32, theme: ClockThemeChoice, format: ClockTimeFormat) -> ClockState {
    let config = ClockConfig {
        aspect_ratio,
        theme,
        time_format: format,
        show_date: true,
        lamps: ClockLampMode::Steady,
        event_profile: ClockEventProfile::Off,
        ..ClockConfig::default()
    };
    let mut state = ClockScenario::init(config, 1);
    read(&mut state, 21, 34);
    state
}

fn read(state: &mut ClockState, hour: u8, minute: u8) {
    let reading = ClockReading::new(hour, minute, 0)
        .unwrap()
        .with_date(ClockDate::new(2026, 12, 24).unwrap());
    ClockScenario::step(state, &[ClockAction::set_reading(reading)], Duration::ZERO);
}

fn on_layer(frame: &RenderFrame, z: i32) -> Vec<&RenderPrimitive> {
    frame
        .layers
        .iter()
        .filter(|layer| layer.z == z)
        .flat_map(|layer| &layer.primitives)
        .collect()
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
            let lights = on_layer(&frame, LIGHTS_LAYER);
            assert!(!lights.is_empty(), "{aspect} {format:?}");
            let [x0, y0, x1, y1] = lights.iter().map(|p| extent(p)).fold(
                [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
                |[a, b, c, d], [e, f, g, h]| [a.min(e), b.min(f), c.max(g), d.max(h)],
            );
            assert!(
                x0 >= layout.bounds_min.x && x1 <= layout.bounds_max.x,
                "{aspect}: {x0}..{x1} in {:?}..{:?}",
                layout.bounds_min,
                layout.bounds_max
            );
            // Only the wire touches the canopy, where it hangs from its hooks.
            assert!(y1 <= layout.canopy_y + wire_half(layout) + 1e-3, "{aspect}");
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
fn the_string_is_bounded_and_its_bulbs_take_their_colors_in_turn() {
    for aspect in ASPECTS {
        let layout = Layout::new(aspect);
        let hang = hang(layout);
        let swags = hang.hooks.len() - 1;
        assert!((2..=MAX_SWAGS).contains(&swags), "{aspect}: {swags}");
        assert_eq!(hang.bulbs.len(), swags * BULBS_PER_SWAG);
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
        render(&mut frame, layout);
        let drawn = on_layer(&frame, LIGHTS_LAYER);
        // A wire for each swag; a glow, a socket, a bulb and its core for
        // each bulb.
        let per_bulb = HALOS.len() + 3;
        assert_eq!(drawn.len(), swags + hang.bulbs.len() * per_bulb, "{aspect}");
        assert!(drawn.len() <= MAX_SWAGS * (1 + BULBS_PER_SWAG * per_bulb));
        let bulbs: Vec<_> = drawn[swags..]
            .chunks(per_bulb)
            .map(|parts| match parts[HALOS.len() + 1] {
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
            let normal =
                crate::render::render_frame(&clock(aspect, ClockThemeChoice::Normal, format));
            let christmas =
                crate::render::render_frame(&clock(aspect, ClockThemeChoice::Christmas, format));
            assert!(on_layer(&normal, LIGHTS_LAYER).is_empty(), "{aspect}");
            let shapes = |frame: &RenderFrame| -> Vec<(i32, Vec<RenderPrimitive>)> {
                frame
                    .layers
                    .iter()
                    .filter(|layer| layer.z != LIGHTS_LAYER)
                    .map(|layer| (layer.z, layer.primitives.iter().map(shape).collect()))
                    .collect()
            };
            assert_eq!(shapes(&normal), shapes(&christmas), "{aspect} {format:?}");
            assert_ne!(normal, christmas);
        }
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
    assert!(!on_layer(&crate::render::render_frame(&auto), LIGHTS_LAYER).is_empty());
    for state in [&mut auto, &mut normal] {
        read(state, 6, 0);
    }
    assert_eq!(
        crate::render::render_frame(&auto),
        crate::render::render_frame(&normal)
    );
}
