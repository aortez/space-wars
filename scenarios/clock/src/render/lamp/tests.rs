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
