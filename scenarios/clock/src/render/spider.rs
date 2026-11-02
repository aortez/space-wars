//! The spider as the arena has it: one body with eight drawn legs posed by
//! how it holds on, its silk line, and a shot on its way. Code-native
//! primitives, shared by the raster and vector adapters, scaled by the
//! spider's own radius so it fits the digits of any layout.

use super::*;
use crate::spider::SpiderFixtureState;
use engine_common::RenderLine;
use engine_rapier::spider::SpiderMode;

// Greys a little lighter than the face's dark field, so the spider reads
// there as well as over the lit cells, with pale eyes.
const BODY: RenderColor = RenderColor::rgb(0.30, 0.28, 0.34);
const HEAD: RenderColor = RenderColor::rgb(0.38, 0.35, 0.42);
const LEGS: RenderColor = RenderColor::rgb(0.46, 0.43, 0.50);
const EYES: RenderColor = RenderColor::rgb(0.95, 0.88, 0.62);
const SILK: RenderColor = RenderColor::rgb(0.78, 0.82, 0.90);
/// The dark of the hatch in the canopy, and the lit edge of its sides.
const HATCH: RenderColor = RenderColor::rgb(0.012, 0.010, 0.008);
const HATCH_EDGE: RenderColor = RenderColor::rgb(0.24, 0.19, 0.12);
const LAYER: i32 = 8;

/// The hatch in the canopy the spider comes and goes by: an opening as wide
/// as the frame's, from the canopy's underside to the top of the view, with
/// a lit edge down either side. Drawn only while there is a spider.
pub(super) fn render_hatch(frame: &mut RenderFrame, layout: Layout) {
    let half = crate::spider::HATCH_WIDTH_PITCH * layout.pitch * 0.5;
    let edge = (layout.pitch * 0.08).clamp(1.0, 2.5);
    frame.push_primitive(
        ARENA_LAYER,
        rectangle(
            RenderPoint::new(-half, layout.canopy_y),
            RenderPoint::new(half, layout.bounds_max.y),
            HATCH,
            None,
        ),
    );
    for side in [-1.0, 1.0] {
        let x = side * half;
        frame.push_primitive(
            ARENA_LAYER,
            rectangle(
                RenderPoint::new(x.min(x - side * edge), layout.canopy_y),
                RenderPoint::new(x.max(x - side * edge), layout.bounds_max.y),
                HATCH_EDGE,
                None,
            ),
        );
    }
}

/// The spider at `state`, facing the way it last crawled.
pub(super) fn render(frame: &mut RenderFrame, state: &SpiderFixtureState) {
    let spider = &state.spider;
    let r = spider.radius;
    let center = spider.position;
    let point = |p: Vec2| RenderPoint::new(p.x, p.y);
    if let Some(line) = spider.line {
        frame.push_primitive(
            LAYER,
            RenderPrimitive::Line(RenderLine::new(
                point(center),
                point(line.anchor),
                Stroke::new(SILK, (0.16 * r).max(0.8)),
            )),
        );
    }
    if let Some(shot) = spider.shot {
        frame.push_primitive(
            LAYER,
            RenderPrimitive::Line(RenderLine::new(
                point(center),
                point(shot.tip),
                Stroke::new(SILK, (0.1 * r).max(0.6)),
            )),
        );
    }
    // The back faces `up`; the legs reach from the body's sides down toward
    // whatever it holds, or dangle when it hangs or falls.
    let up = spider.up;
    let side = Vec2::new(up.y, -up.x);
    let facing = if spider.intent.crawl != 0.0 {
        spider.intent.crawl.signum()
    } else if spider.velocity.dot(side).abs() > 0.1 * r {
        spider.velocity.dot(side).signum()
    } else {
        1.0
    };
    let hanging = spider.mode != SpiderMode::Crawling;
    // A gait from how far along the surface the body has come, so the feet
    // step as it crawls and rest when it stops.
    let gait = center.dot(side) / (0.9 * r);
    let legs = [(0.6, 0.45), (1.05, 0.7), (1.5, 0.7), (1.9, 0.45)];
    for sign in [-1.0_f32, 1.0] {
        for (index, (spread, lift)) in legs.into_iter().enumerate() {
            let step = if hanging {
                0.0
            } else {
                0.18 * (gait + index as f32 * std::f32::consts::FRAC_PI_2 + sign.max(0.0) * 1.1)
                    .sin()
            };
            let reach = if hanging { 0.8 } else { 1.0 };
            let knee =
                center + side * (sign * (0.55 + spread * 0.55) * r) + up * (lift * r * reach);
            let foot = center + side * ((sign * spread + step) * r)
                - up * (if hanging { 0.5 } else { 0.95 } * r);
            frame.push_primitive(
                LAYER,
                RenderPrimitive::Polygon(RenderPolygon {
                    points: vec![point(center), point(knee), point(foot)],
                    fill: None,
                    stroke: Some(Stroke::new(LEGS, (0.18 * r).max(0.8))),
                }),
            );
        }
    }
    frame.push_primitive(
        LAYER,
        RenderPrimitive::Circle(RenderCircle::filled(point(center), r, BODY)),
    );
    let head = center + side * (facing * 0.75 * r) + up * (0.1 * r);
    frame.push_primitive(
        LAYER,
        RenderPrimitive::Circle(RenderCircle::filled(point(head), 0.5 * r, HEAD)),
    );
    for eye in [-1.0, 1.0] {
        frame.push_primitive(
            LAYER,
            RenderPrimitive::Circle(RenderCircle::filled(
                point(head + side * (facing * 0.22 * r) + up * (eye * 0.18 * r)),
                (0.1 * r).max(0.5),
                EYES,
            )),
        );
    }
}
