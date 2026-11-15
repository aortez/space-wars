//! How lightning draws: every shape of a bolt is a simple polygon, so both
//! renderers fill it alike; the bolt shows only while its light and sparks
//! last; and the flash lights only the room's opaque things, each layer by
//! its gain.

use super::*;
use crate::rain::RAINING_TICKS;
use crate::rain::lightning::SPARK_TICKS;
use crate::{ClockReading, DisplaySnapshot};
use engine_common::{ClockRainAmount, ClockTimeFormat, RenderLine};

fn display() -> DisplaySnapshot {
    crate::digits::snapshot(
        ClockReading::new(12, 34, 0).unwrap(),
        ClockTimeFormat::TwentyFourHour,
    )
}

/// Which side of the line through `a` and `b` `c` lies on.
fn side(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Whether two edges cross each other properly, not merely touch.
fn cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    side(a, b, c) * side(a, b, d) < 0.0 && side(c, d, a) * side(c, d, b) < 0.0
}

/// Whether no two edges of a polygon that are not neighbours cross. The
/// raster path fills by even and odd crossings and the GPU path by winding,
/// so only a polygon that is simple fills the same in both.
fn simple(points: &[RenderPoint]) -> bool {
    let points: Vec<Vec2> = points.iter().map(|p| Vec2::new(p.x, p.y)).collect();
    let n = points.len();
    let edge = |i: usize| (points[i], points[(i + 1) % n]);
    (0..n).all(|i| {
        (i + 2..n).filter(|&j| !(i == 0 && j == n - 1)).all(|j| {
            let ((a, b), (c, d)) = (edge(i), edge(j));
            !cross(a, b, c, d)
        })
    })
}

fn bolt_layer(frame: &RenderFrame) -> Option<&engine_common::RenderLayer> {
    frame.layers.iter().find(|layer| layer.z == BOLT_LAYER)
}

#[test]
fn every_shape_of_every_bolt_is_a_simple_polygon_and_shows_only_while_its_strike_lasts() {
    let mut strikes = 0;
    for aspect in [480.0 / 800.0, 1024.0 / 768.0, 800.0 / 480.0, 4.0] {
        let layout = Layout::new(aspect);
        for seed in 0..16 {
            let mut event = RainEvent::new(layout, seed, ClockRainAmount::Heavy, display());
            while event.tick < RAINING_TICKS + 60 {
                event.step();
                let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
                bolt(&mut frame, &event);
                if event.storm().began(event.tick).is_some() {
                    strikes += 1;
                }
                // The channel shows while it has light, and the bolt draws
                // nothing once that and every spark are gone.
                let strike = event.storm().latest();
                let lit = strike.is_some_and(|strike| strike.channel(event.tick) > 0.0);
                let sparking = strike.is_some_and(|strike| {
                    let age = event.tick - strike.tick;
                    age < u64::from(SPARK_TICKS)
                        && strike
                            .sparks
                            .iter()
                            .any(|spark| age < u64::from(spark.life))
                });
                let Some(layer) = bolt_layer(&frame) else {
                    assert!(!lit, "aspect {aspect}, seed {seed}, tick {}", event.tick);
                    continue;
                };
                assert!(
                    lit || sparking,
                    "aspect {aspect}, seed {seed}, tick {}",
                    event.tick
                );
                assert_eq!(frame.layers.len(), 1, "the bolt draws on its own layer");
                for primitive in &layer.primitives {
                    if let RenderPrimitive::Polygon(polygon) = primitive {
                        assert!(
                            simple(&polygon.points),
                            "aspect {aspect}, seed {seed}, tick {}: {:?}",
                            event.tick,
                            polygon.points
                        );
                    }
                }
            }
        }
    }
    assert!(strikes > 150, "{strikes}");
}

#[test]
fn the_flash_lights_the_rooms_opaque_things_by_their_layers_gain_and_leaves_lights_and_glows_alone()
{
    let wall = RenderColor::rgb(0.02, 0.03, 0.06);
    let lit = RenderColor::rgb(0.3, 0.9, 1.0);
    let square = |color: RenderColor| {
        RenderPrimitive::Polygon(RenderPolygon::filled(
            vec![
                RenderPoint::new(0.0, 0.0),
                RenderPoint::new(10.0, 0.0),
                RenderPoint::new(10.0, 10.0),
            ],
            color,
        ))
    };
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
    frame.push_primitive(BACKGROUND_LAYER, square(wall));
    frame.push_primitive(BACKGROUND_LAYER, square(RenderColor { a: 0.5, ..wall }));
    frame.push_primitive(INACTIVE_CELL_LAYER, square(wall));
    frame.push_primitive(GLOW_LAYER, square(wall));
    frame.push_primitive(ACTIVE_CELL_LAYER, square(lit));
    frame.push_primitive(
        FRAME_LAYER,
        RenderPrimitive::Line(RenderLine::new(
            RenderPoint::ZERO,
            RenderPoint::new(5.0, 5.0),
            Stroke::new(wall, 2.0),
        )),
    );
    let dark = frame.clone();
    flash(&mut frame, 0.0);
    assert_eq!(frame, dark);
    flash(&mut frame, 0.5);
    let color =
        |z: i32, index: usize| match &frame.layers.iter().find(|l| l.z == z).unwrap().primitives
            [index]
        {
            RenderPrimitive::Polygon(polygon) => polygon.fill.unwrap().color,
            RenderPrimitive::Line(line) => line.stroke.color,
            _ => unreachable!(),
        };
    let toward = |gain: f32| {
        let amount = gain * 0.5;
        RenderColor::rgb(
            wall.r + (FLASH_COLOR.r - wall.r) * amount,
            wall.g + (FLASH_COLOR.g - wall.g) * amount,
            wall.b + (FLASH_COLOR.b - wall.b) * amount,
        )
    };
    assert_eq!(color(BACKGROUND_LAYER, 0), toward(0.30));
    assert_eq!(color(INACTIVE_CELL_LAYER, 0), toward(0.30));
    assert_eq!(color(FRAME_LAYER, 0), toward(0.26));
    // A translucent glow, the cells' halos and the lit cells keep their light.
    assert_eq!(color(BACKGROUND_LAYER, 1), RenderColor { a: 0.5, ..wall });
    assert_eq!(color(GLOW_LAYER, 0), wall);
    assert_eq!(color(ACTIVE_CELL_LAYER, 0), lit);
}

#[test]
fn a_ribbon_runs_its_width_either_side_of_its_path_and_narrows_to_a_point() {
    let path = [
        Vec2::new(0.0, 10.0),
        Vec2::new(2.0, 8.0),
        Vec2::new(0.0, 6.0),
        Vec2::new(1.0, 4.0),
    ];
    let even = ribbon(&path, |_| 0.5);
    assert_eq!(even.len(), 8);
    assert!(simple(&even));
    // The ends lie square across the first and last steps.
    let (first, last) = (
        Vec2::new(even[0].x, even[0].y),
        Vec2::new(even[7].x, even[7].y),
    );
    assert!((first.distance_to(last) - 1.0).abs() < 1e-5);
    assert!((first.midpoint(last).distance_to(path[0])).abs() < 1e-5);
    let tapered = ribbon(&path, |point| 0.5 * (1.0 - point as f32 / 3.0));
    assert_eq!(tapered[3], tapered[4], "a tapered ribbon ends in a point");
    assert!(simple(&tapered));
}
