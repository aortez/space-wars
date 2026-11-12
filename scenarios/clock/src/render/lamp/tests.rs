//! A resting lamp's light against its full light (#629).

use super::*;

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

fn circles(frame: &RenderFrame) -> impl Iterator<Item = &RenderCircle> {
    frame
        .layers
        .iter()
        .flat_map(|layer| &layer.primitives)
        .filter_map(|primitive| match primitive {
            RenderPrimitive::Circle(circle) => Some(circle),
            _ => None,
        })
}

/// The light one lamp gives off: the summed alpha of the glow around its
/// corner, its bloom and the light it throws into the room.
fn emitted(brightness: f32) -> f32 {
    let layout = Layout::new(800.0 / 480.0);
    let light = light(brightness, layout);
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
    throw(&mut frame, &light);
    glow(&mut frame, &light, layout);
    bloom(&mut frame, &light, layout);
    circles(&frame)
        .filter_map(|circle| circle.fill.as_ref())
        .map(|fill| fill.color.a)
        .sum()
}

#[test]
fn a_resting_lamp_gives_forty_percent_of_its_full_light() {
    // Full light as #621 drew it: twelve glow shells at 0.012, bloom discs at
    // 0.04, 0.075 and 0.15, and seven thrown discs fading from 0.016.
    let full = 12.0 * 0.012 + (0.04 + 0.075 + 0.15) + 0.016 * (7.0 - 0.55 * 3.5);
    assert!((emitted(1.0) - full).abs() < 1e-5, "{}", emitted(1.0));
    assert!(
        (emitted(Look::STEADY.brightness) - 0.4 * full).abs() < 1e-5,
        "{}",
        emitted(Look::STEADY.brightness)
    );
}

#[test]
fn the_hot_core_shows_only_above_rest() {
    let layout = Layout::new(800.0 / 480.0);
    let head_circles = |brightness: f32| {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
        head(&mut frame, &light(brightness, layout), layout);
        circles(&frame).count()
    };
    // The mount and the glass, and the core once the lamp is above rest.
    assert_eq!(head_circles(0.0), 2);
    assert_eq!(head_circles(REST), 2);
    assert_eq!(head_circles(1.0), 3);
    // At rest the glass is amber; at full it is the warm light it was.
    let amber = glass(REST);
    assert!(
        amber.r > amber.g && amber.g > amber.b && amber.g < 0.6,
        "{amber:?}"
    );
    assert_eq!(glass(1.0), RenderColor::rgb(1.0, 0.80, 0.46));
}
