//! What the corner lamps give off at rest, and how they look as they
//! brighten.

use super::*;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario};
use engine_common::{ClockEventProfile, Scenario};
use std::time::Duration;

/// One lamp's full light, summed by alpha: twelve glow shells at 0.012,
/// bloom discs at 0.04, 0.075 and 0.15, and seven thrown discs fading from
/// 0.016.
const FULL: f32 = 12.0 * 0.012 + (0.04 + 0.075 + 0.15) + 0.016 * (7.0 - 0.55 * 3.5);

fn light(brightness: f32, layout: Layout) -> Light {
    Light::new(
        0,
        Look {
            brightness,
            ..Look::STEADY
        },
        layout,
    )
}

fn circles(frame: &RenderFrame, z: i32) -> impl Iterator<Item = &RenderCircle> {
    frame
        .layers
        .iter()
        .filter(move |layer| layer.z == z)
        .flat_map(|layer| &layer.primitives)
        .filter_map(|primitive| match primitive {
            RenderPrimitive::Circle(circle) => Some(circle),
            _ => None,
        })
}

fn fill(circle: &RenderCircle) -> RenderColor {
    circle.fill.as_ref().expect("a filled disc").color
}

/// The light a frame's lamps give off: the summed alpha of the light thrown
/// behind the cells, and of the translucent glow and bloom over the frame.
fn light_in(frame: &RenderFrame) -> f32 {
    let thrown = circles(frame, GLOW_LAYER)
        .map(fill)
        .filter(|color| RenderColor { a: 1.0, ..*color } == LIGHT_COLOR);
    let around = circles(frame, LAMP_LAYER)
        .map(fill)
        .filter(|color| color.a < 1.0);
    thrown.chain(around).map(|color| color.a).sum()
}

/// One lamp's light at a brightness no steady scene draws.
fn emitted(brightness: f32) -> f32 {
    let layout = Layout::new(800.0 / 480.0);
    let light = light(brightness, layout);
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
    throw(&mut frame, &light);
    glow(&mut frame, &light, layout);
    bloom(&mut frame, &light, layout);
    light_in(&frame)
}

/// The discs a lamp's head draws: the mount, the glass and, once it shows,
/// the hot core.
fn head_discs(brightness: f32) -> Vec<RenderCircle> {
    let layout = Layout::new(800.0 / 480.0);
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
    head(&mut frame, &light(brightness, layout), layout);
    let discs: Vec<_> = circles(&frame, LAMP_LAYER).cloned().collect();
    assert!(matches!(discs.len(), 2 | 3), "{} discs", discs.len());
    discs
}

#[test]
fn the_resting_lamps_give_forty_percent_of_their_full_light() {
    assert!((emitted(1.0) - FULL).abs() < 1e-5, "{}", emitted(1.0));
    // A steady scene draws both lamps at rest.
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        7,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 39, 50).unwrap(),
        )],
        Duration::ZERO,
    );
    let drawn = light_in(&ClockScenario::render_frame(&state));
    assert!((drawn - 2.0 * 0.4 * FULL).abs() < 1e-4, "{drawn}");
}

#[test]
fn the_glass_glows_amber_at_rest() {
    let [dark, rest, full] =
        [0.0, REST_BRIGHTNESS, 1.0].map(|brightness| fill(&head_discs(brightness)[1]));
    // Lit, nearly as red as at full but more orange.
    assert!(rest.r > 0.8 && rest.r > 3.0 * dark.r, "{rest:?}");
    assert!(
        rest.g / rest.r < full.g / full.r,
        "{rest:?} against {full:?}"
    );
    assert_eq!(full, RenderColor::rgb(1.0, 0.80, 0.46));
}

#[test]
fn the_hot_core_fades_in_above_rest() {
    let core = |brightness: f32| {
        head_discs(brightness)
            .get(2)
            .map_or(0.0, |core| fill(core).a)
    };
    assert_eq!(core(0.0), 0.0);
    assert_eq!(core(REST_BRIGHTNESS), 0.0);
    let faint = core(REST_BRIGHTNESS + 0.01);
    assert!(faint > 0.0 && faint < 0.05, "{faint}");
    let rising: Vec<f32> = (0..=20)
        .map(|step| core(REST_BRIGHTNESS + (1.0 - REST_BRIGHTNESS) * step as f32 / 20.0))
        .collect();
    assert!(
        rising.windows(2).all(|pair| pair[0] < pair[1]),
        "{rising:?}"
    );
    assert_eq!(core(1.0), 1.0);
    assert_eq!(core(1.15), 1.0);
}

const DARK: RenderColor = RenderColor::rgb(0.2, 0.2, 0.2);

/// A lamp's spotlight at full strength on `point`, following a mover of
/// `size` there if there is one.
fn spot_on(point: Vec2, size: Option<f32>, layout: Layout) -> Spot {
    let face = |p: Vec2| (p - Vec2::new(layout.face_origin.x, layout.face_origin.y)) / layout.pitch;
    let look = Look {
        brightness: 1.0,
        narrow: 1.0,
        target: face(point),
        subject: size.map(|size| crate::lamps::Subject {
            position: face(point),
            size: size / layout.pitch,
        }),
    };
    Spot::new(0, look, layout).expect("a spotlight has a pool")
}

fn disc(at: Vec2, radius: f32) -> RenderPrimitive {
    RenderPrimitive::Circle(RenderCircle::filled(
        RenderPoint::new(at.x, at.y),
        radius,
        DARK,
    ))
}

fn square(at: Vec2, half: f32) -> RenderPrimitive {
    RenderPrimitive::Polygon(RenderPolygon::filled(
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .map(|(x, y)| RenderPoint::new(at.x + x * half, at.y + y * half))
            .to_vec(),
        DARK,
    ))
}

/// The colors a primitive is drawn in.
fn colors(primitive: &RenderPrimitive) -> Vec<RenderColor> {
    match primitive {
        RenderPrimitive::Circle(RenderCircle { fill, stroke, .. })
        | RenderPrimitive::Polygon(RenderPolygon { fill, stroke, .. }) => fill
            .iter()
            .map(|fill| fill.color)
            .chain(stroke.iter().map(|stroke| stroke.color))
            .collect(),
        RenderPrimitive::Line(line) => vec![line.stroke.color],
        RenderPrimitive::Text(text) => vec![text.color],
    }
}

#[test]
fn a_pool_warms_the_small_things_in_it_in_place_and_leaves_the_rest() {
    let layout = Layout::new(800.0 / 480.0);
    let pitch = layout.pitch;
    let middle = Vec2::new(0.0, 0.0);
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
    // What each thing should end up as: `amount` of the way to warm white.
    let things = [
        // A cell, a dark guide, a creature's leg and its body, all in the
        // pool's full light.
        (
            ACTIVE_CELL_LAYER,
            square(middle + Vec2::new(0.0, pitch), pitch * 0.45),
            0.42,
        ),
        (
            INACTIVE_CELL_LAYER,
            square(middle + Vec2::new(pitch, 0.0), pitch * 0.45),
            0.22,
        ),
        (
            CREATURE_LAYER,
            RenderPrimitive::Line(engine_common::RenderLine::new(
                RenderPoint::new(-pitch * 0.3, 0.0),
                RenderPoint::new(pitch * 0.3, 0.0),
                Stroke::new(DARK, 2.0),
            )),
            0.42,
        ),
        (CREATURE_LAYER, disc(middle, pitch * 0.4), 0.42),
        // A creature too large to light whole and one past the pool's rim
        // stay as they are, as does what the pool's own discs light, under
        // them: the floor, and a ball rolling on it. The beam's glow is light
        // itself, and text is left as it is.
        (CREATURE_LAYER, square(middle, pitch * 2.0), 0.0),
        (
            CREATURE_LAYER,
            disc(middle + Vec2::new(pitch * 5.0, 0.0), pitch * 0.5),
            0.0,
        ),
        (ARENA_LAYER, square(middle, pitch * 20.0), 0.0),
        (ARENA_LAYER, disc(middle, pitch * 0.5), 0.0),
        (GLOW_LAYER, disc(middle, pitch * 0.5), 0.0),
        (
            LABEL_LAYER,
            RenderPrimitive::Text(RenderText {
                position: RenderPoint::ZERO,
                text: "P1 DUCK".into(),
                color: DARK,
                size: 18.0,
                anchor: TextAnchor::Center,
            }),
            0.0,
        ),
    ];
    for (z, primitive, _) in &things {
        frame.push_primitive(*z, primitive.clone());
    }
    light_scene(&mut frame, &[spot_on(middle, None, layout)]);
    for (z, primitive, amount) in &things {
        let lit = frame
            .layers
            .iter()
            .filter(|layer| layer.z == *z)
            .flat_map(|layer| &layer.primitives)
            .find(|lit| {
                // The same shape, wherever its colors went.
                let mut lit = (*lit).clone();
                warm(&mut lit, 0.0);
                std::mem::discriminant(&lit) == std::mem::discriminant(primitive)
                    && footprint(&lit) == footprint(primitive)
            })
            .expect("everything stays in its layer");
        for (color, was) in colors(lit).into_iter().zip(colors(primitive)) {
            let expected = was.r + (LIT_COLOR.r - was.r) * amount;
            assert!((color.r - expected).abs() < 1e-5, "{z}: {color:?}");
            assert!(
                color.g > was.g - 1e-6 && color.b > was.b - 1e-6,
                "{z}: {color:?}"
            );
            assert_eq!(color.a, was.a, "{z}: as opaque as it was");
        }
    }
    // The frame gains nothing.
    let count: usize = frame
        .layers
        .iter()
        .map(|layer| layer.primitives.len())
        .sum();
    assert_eq!(count, things.len());
}

#[test]
fn the_mover_a_lamp_follows_shades_what_is_behind_it_and_not_itself() {
    let layout = Layout::new(800.0 / 480.0);
    let size = layout.pitch * 0.5;
    let mover = Vec2::new(-layout.pitch * 4.0, 0.0);
    let away = (mover - crate::lamps::head(0, &layout)).normalized();
    let across = Vec2::new(-away.y, away.x);
    let spot = spot_on(mover, Some(size), layout);
    let full = spot.light_at(mover);
    assert!(full > 0.99, "{full}");
    // The mover itself is in full light, to its edges.
    for point in [mover + across * (size * 0.9), mover + away * (size * 0.9)] {
        assert_eq!(spot.light_at(point), full);
    }
    // Just behind it, as the lamp sees it, the shadow takes most of what
    // SHADOW_DEPTH allows, and it softens away from there; just in front of
    // it there is none.
    let behind = |distance: f32| spot.light_at(mover + away * (size * distance));
    assert!(
        behind(1.2) < full * (1.0 - SHADOW_DEPTH * 0.5),
        "{}",
        behind(1.2)
    );
    assert!(behind(1.2) < behind(1.5) && behind(1.5) < behind(1.8) && behind(1.8) < full);
    assert_eq!(behind(SHADOW_OFFSET + SHADOW_SPREAD), full);
    assert_eq!(spot.light_at(mover - away * (size * 1.2)), full);
    // A cell just behind it is warmed less than one just in front of it.
    let warmth = |at: Vec2| {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
        frame.push_primitive(ACTIVE_CELL_LAYER, square(at, size * 0.2));
        light_scene(&mut frame, &[spot_on(mover, Some(size), layout)]);
        colors(&frame.layers[0].primitives[0])[0].r
    };
    assert!(warmth(mover + away * (size * 1.3)) < warmth(mover - away * (size * 1.3)));
    // A pool with no mover in it shades nothing.
    let bare = spot_on(mover, None, layout);
    assert_eq!(bare.light_at(mover + away * (size * 1.2)), full);
}

#[test]
fn a_thing_growing_in_a_pool_fades_out_of_its_light_without_a_snap() {
    let layout = Layout::new(800.0 / 480.0);
    let spot = spot_on(Vec2::ZERO, None, layout);
    let radius = spot.pool.radius;
    // A thread of silk, say, lengthening either way from the pool's middle.
    let warmth = |reach: f32| {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
        frame.push_primitive(
            CREATURE_LAYER,
            RenderPrimitive::Line(engine_common::RenderLine::new(
                RenderPoint::new(-reach, 0.0),
                RenderPoint::new(reach, 0.0),
                Stroke::new(DARK, 1.0),
            )),
        );
        light_scene(&mut frame, std::slice::from_ref(&spot));
        colors(&frame.layers[0].primitives[0])[0].r - DARK.r
    };
    let full = warmth(radius * 0.1);
    assert!(
        (full - (LIT_COLOR.r - DARK.r) * 0.42).abs() < 1e-5,
        "{full}"
    );
    assert!((warmth(radius * SMALL_FADE) - full).abs() < 1e-6);
    assert_eq!(warmth(radius * SMALL), 0.0);
    assert_eq!(warmth(radius * 0.6), 0.0);
    // In between, each small step in length takes a small step in light.
    let fading: Vec<f32> = (0..=100)
        .map(|step| warmth(radius * (0.3 + 0.25 * step as f32 / 100.0)))
        .collect();
    for pair in fading.windows(2) {
        assert!(pair[1] <= pair[0] + 1e-6, "{fading:?}");
        assert!(pair[0] - pair[1] < full * 0.05, "{fading:?}");
    }
}
