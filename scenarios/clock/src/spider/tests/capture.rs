//! Drawings of the spider on the actual digits, for the docs. Ignored; give
//! it a directory to write SVG files into:
//!
//! `SPACEWARS_CLOCK_SPIDER_CAPTURE=<dir> cargo test -p scenario-clock --lib spider::tests::capture -- --ignored`
//!
//! docs/clock.md says how they become the PNG and GIF files. They are
//! diagnostic drawings of the physics state and the drawn face, not the
//! Clock renderer; the visit's pictures come from the client's capture.

use std::{fmt::Write as _, path::Path};

use super::*;
use crate::arena_fixture::FixtureStart;

const PAPER: &str = "#f7f5f0";
const OUTSIDE: &str = "#ebe9e3";
const INK: &str = "#30343b";
const WOOD: &str = "#b9a98c";
const WOOD_EDGE: &str = "#8a7a5c";
const CELL: &str = "#e0552f";
const PROXY: &str = "#2f6db5";
const BODY: &str = "#262a30";
const EYE: &str = "#5d6570";
const SILK: &str = "#2f6db5";
const HOLD: &str = "#e07b39";

/// A drawing in world units, y up, of the region from `min` to `max`.
struct Sketch {
    min: Vec2,
    max: Vec2,
    px: f32,
    svg: String,
}

impl Sketch {
    fn new(min: Vec2, max: Vec2, px: f32) -> Self {
        let mut sketch = Self {
            min,
            max,
            px,
            svg: String::new(),
        };
        let (width, height) = sketch.size();
        sketch.raw(format!(
            r#"<rect width="{width:.1}" height="{height:.1}" fill="{PAPER}"/>"#
        ));
        sketch
    }

    fn size(&self) -> (f32, f32) {
        (
            (self.max - self.min).x * self.px,
            (self.max - self.min).y * self.px,
        )
    }

    fn at(&self, point: Vec2) -> (f32, f32) {
        (
            (point.x - self.min.x) * self.px,
            (self.max.y - point.y) * self.px,
        )
    }

    fn raw(&mut self, element: String) {
        self.svg.push_str(&element);
        self.svg.push('\n');
    }

    fn rect(&mut self, min: Vec2, max: Vec2, fill: &str, stroke: &str, dashed: bool) {
        let (x, y) = self.at(Vec2::new(min.x, max.y));
        let (width, height) = ((max.x - min.x) * self.px, (max.y - min.y) * self.px);
        let dash = if dashed {
            r#" stroke-dasharray="3 3""#
        } else {
            ""
        };
        self.raw(format!(
            r#"<rect x="{x:.1}" y="{y:.1}" width="{width:.1}" height="{height:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1"{dash}/>"#
        ));
    }

    fn polygon(&mut self, points: &[Vec2], fill: &str, opacity: f32) {
        let mut d = String::new();
        for (i, point) in points.iter().enumerate() {
            let (x, y) = self.at(*point);
            let _ = write!(d, "{}{x:.1},{y:.1} ", if i == 0 { "M" } else { "L" });
        }
        self.raw(format!(
            r#"<path d="{d}Z" fill="{fill}" fill-opacity="{opacity:.2}"/>"#
        ));
    }

    fn path(&mut self, points: &[Vec2], color: &str, width: f32, dashed: bool) {
        if points.len() < 2 {
            return;
        }
        let mut d = String::new();
        for (i, point) in points.iter().enumerate() {
            let (x, y) = self.at(*point);
            let _ = write!(d, "{}{x:.1},{y:.1} ", if i == 0 { "M" } else { "L" });
        }
        let dash = if dashed {
            r#" stroke-dasharray="5 4""#
        } else {
            ""
        };
        self.raw(format!(
            r#"<path d="{d}" fill="none" stroke="{color}" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round"{dash}/>"#
        ));
    }

    fn dot(&mut self, point: Vec2, radius: f32, color: &str) {
        let (x, y) = self.at(point);
        self.raw(format!(
            r#"<circle cx="{x:.1}" cy="{y:.1}" r="{radius}" fill="{color}"/>"#
        ));
    }

    fn label(&mut self, point: Vec2, text: &str, anchor: &str) {
        let (x, y) = self.at(point);
        self.raw(format!(
            r#"<text x="{x:.1}" y="{y:.1}" font-family="DejaVu Sans, sans-serif" font-size="13" fill="{INK}" stroke="{PAPER}" stroke-width="4" paint-order="stroke" text-anchor="{anchor}">{text}</text>"#
        ));
    }

    /// The room: what lies beyond the view, the floor, the frame and the
    /// beam.
    fn room(&mut self, layout: Layout, floor: &[(Vec2, Vec2)]) {
        self.rect(
            Vec2::new(self.min.x, layout.bounds_max.y),
            Vec2::new(self.max.x, self.max.y),
            OUTSIDE,
            "none",
            false,
        );
        for &(min, max) in floor {
            self.rect(min, max, WOOD, WOOD_EDGE, false);
        }
        for part in FramePart::ALL {
            let (min, max) = part.rect(layout);
            self.rect(min, max, WOOD, WOOD_EDGE, false);
        }
    }

    /// The face as drawn, and the proxies standing for it.
    fn face(&mut self, frame: &Frame) {
        for (polygon, ink) in &frame.drawn {
            self.polygon(polygon, CELL, *ink);
        }
        for surface in frame.surfaces.iter().filter(|s| s.body.is_none()) {
            let Some(visible) = surface.visible() else {
                continue;
            };
            let corners = [
                visible.min,
                Vec2::new(visible.max.x, visible.min.y),
                visible.max,
                Vec2::new(visible.min.x, visible.max.y),
            ]
            .map(|local| surface.world(local));
            let mut outline = corners.to_vec();
            outline.push(corners[0]);
            self.path(&outline, PROXY, 0.8, true);
        }
    }

    /// The spider as its state has it, as the mechanics drawings draw it.
    fn spider(&mut self, fixture: &SpiderFixtureState, opacity: f32) {
        let state = &fixture.spider;
        let r = state.radius;
        let center = state.position;
        self.raw(format!(r#"<g opacity="{opacity}">"#));
        if let Some(line) = state.line {
            self.path(&[center, line.anchor], SILK, 1.5, false);
            self.dot(line.anchor, 2.5, SILK);
        }
        if let Some(shot) = state.shot {
            self.path(&[center, shot.tip], SILK, 1.5, true);
        }
        let up = state.up;
        let side = Vec2::new(up.y, -up.x);
        let mut legs = String::new();
        for sign in [-1.0, 1.0] {
            for (spread, lift) in [(0.6, 0.45), (1.05, 0.7), (1.5, 0.7), (1.9, 0.45)] {
                let knee = center + side * (sign * (0.55 + spread * 0.55) * r) + up * (lift * r);
                let foot = center + side * (sign * spread * r) - up * (0.95 * r);
                let [(cx, cy), (kx, ky), (fx, fy)] =
                    [self.at(center), self.at(knee), self.at(foot)];
                let _ = write!(legs, "M{cx:.1},{cy:.1} L{kx:.1},{ky:.1} L{fx:.1},{fy:.1} ");
            }
        }
        let (x, y) = self.at(center);
        let (hx, hy) = self.at(center + up * (0.4 * r));
        self.raw(format!(
            r#"<path d="{legs}" fill="none" stroke="{BODY}" stroke-width="1.2" stroke-linejoin="round"/><circle cx="{x:.1}" cy="{y:.1}" r="{body:.1}" fill="{BODY}"/><circle cx="{hx:.1}" cy="{hy:.1}" r="{eye:.1}" fill="{EYE}"/>"#,
            body = r * self.px,
            eye = r * self.px * 0.3,
        ));
        if let Some(support) = state.support {
            self.dot(support.point, 2.5, HOLD);
        }
        self.raw("</g>".to_owned());
    }

    fn finish(self) -> String {
        let (width, height) = self.size();
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.1} {height:.1}">
<defs><clipPath id="frame"><rect width="{width:.1}" height="{height:.1}"/></clipPath></defs>
<g clip-path="url(#frame)">
{}</g>
</svg>
"#,
            self.svg
        )
    }
}

/// Panels side by side, each with a caption beneath.
fn panels(sketches: Vec<(Sketch, &str)>) -> String {
    let gap = 12.0;
    let caption = 26.0;
    let height = sketches.iter().map(|(s, _)| s.size().1).fold(0.0, f32::max) + caption;
    let width = sketches.iter().map(|(s, _)| s.size().0 + gap).sum::<f32>() - gap;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.1} {height:.1}">
<rect width="{width:.1}" height="{height:.1}" fill="white"/>
"#
    );
    let mut x = 0.0;
    for (i, (sketch, text)) in sketches.into_iter().enumerate() {
        let (w, h) = sketch.size();
        let _ = write!(
            svg,
            r#"<clipPath id="panel{i}"><rect width="{w:.1}" height="{h:.1}"/></clipPath>
<g transform="translate({x:.1},0)" clip-path="url(#panel{i})">
{}</g>
<text x="{cx:.1}" y="{ty:.1}" font-family="DejaVu Sans, sans-serif" font-size="13" fill="{INK}" text-anchor="middle">{text}</text>
"#,
            sketch.svg,
            cx = x + w / 2.0,
            ty = h + 18.0,
        );
        x += w + gap;
    }
    svg.push_str("</svg>\n");
    svg
}

fn save(dir: &Path, name: &str, svg: &str) {
    std::fs::write(dir.join(name), svg).unwrap();
}

/// The whole room with the strip above the view where the beam is.
fn whole_room(layout: Layout) -> (Vec2, Vec2) {
    let (_, beam_top) = FramePart::Beam.rect(layout);
    (
        Vec2::new(layout.bounds_min.x, layout.bounds_min.y),
        Vec2::new(layout.bounds_max.x, beam_top.y + 0.5 * layout.pitch),
    )
}

/// A strobe of `frames`: faded spiders every `every` ticks and the last in
/// full, over the room and face of the last frame.
fn strobe(layout: Layout, frames: &[Frame], every: usize, min: Vec2, max: Vec2, px: f32) -> Sketch {
    let last = frames.last().expect("frames to draw");
    let mut sketch = Sketch::new(min, max, px);
    sketch.room(layout, &last.floor);
    sketch.face(last);
    let trail: Vec<Vec2> = frames.iter().map(|f| f.fixture.spider.position).collect();
    sketch.path(&trail, "#8d8d8d", 1.0, true);
    for frame in frames.iter().step_by(every) {
        sketch.spider(&frame.fixture, 0.3);
    }
    sketch.spider(&last.fixture, 1.0);
    sketch
}

#[test]
#[ignore = "set SPACEWARS_CLOCK_SPIDER_CAPTURE to a directory to draw the spider on the digits for the docs"]
fn draw_the_spider_on_the_digits_for_the_docs() {
    let dir = std::path::PathBuf::from(
        std::env::var_os("SPACEWARS_CLOCK_SPIDER_CAPTURE")
            .expect("set SPACEWARS_CLOCK_SPIDER_CAPTURE"),
    );
    std::fs::create_dir_all(dir.join("route")).unwrap();
    let layout = Layout::new(ASPECT);
    let frames =
        record(|| climb_swing_and_ride_out(ASPECT, ClockFont::Classic, FixtureStart::Default));
    assert!(frames.len() > 200);
    let of = |phases: &[&str]| -> Vec<Frame> {
        frames
            .iter()
            .filter(|f| phases.contains(&f.phase))
            .cloned()
            .collect()
    };

    // The route in three strobes, around the digit it climbs.
    let target = lowest_cell_of(&frames[0], 2);
    let around = (
        Vec2::new(target.x - 5.0 * layout.pitch, layout.bounds_min.y),
        Vec2::new(target.x + 5.0 * layout.pitch, 5.5 * layout.pitch),
    );
    let mut sketches = Vec::new();
    let climb = of(&["walks under the cell", "shoots the cell", "climbs the line"]);
    sketches.push((
        strobe(layout, &climb, 8, around.0, around.1, 1.2),
        "up to the cell",
    ));
    let along = of(&["pays out line", "crawls the underside"]);
    sketches.push((
        strobe(layout, &along, 6, around.0, around.1, 1.2),
        "along the underside",
    ));
    let swing = of(&["swings on the line", "climbs back"]);
    sketches.push((
        strobe(layout, &swing, 10, around.0, around.1, 1.2),
        "off the end and back up",
    ));
    save(&dir, "climb.svg", &panels(sketches));

    // The minute change, frame by frame: the old glyph carries the spider
    // out of the slot, and the new glyph is other surfaces.
    let ride = of(&[
        "rests on the cell",
        "rides the old glyph out",
        "falls to the floor",
    ]);
    let first_ride = ride
        .iter()
        .position(|f| f.phase == "rides the old glyph out")
        .unwrap();
    let released = ride
        .iter()
        .position(|f| f.phase == "falls to the floor")
        .unwrap();
    let picks = [
        (first_ride - 1, "holding the old 3"),
        (first_ride + 3, "carried out of the slot"),
        (first_ride + 8, "off the cell's drawn part"),
        (released + 12, "falling past the new 4"),
    ];
    let slide = (
        Vec2::new(target.x - 4.5 * layout.pitch, layout.bounds_min.y),
        Vec2::new(target.x + 4.5 * layout.pitch, 5.5 * layout.pitch),
    );
    let sketches = picks
        .into_iter()
        .map(|(index, caption)| {
            let frame = &ride[index.min(ride.len() - 1)];
            let mut sketch = Sketch::new(slide.0, slide.1, 1.2);
            sketch.room(layout, &frame.floor);
            sketch.face(frame);
            sketch.spider(&frame.fixture, 1.0);
            if let Some(Hold::Surface { id, departing, .. }) = frame.fixture.support {
                sketch.label(
                    frame.fixture.spider.position
                        - Vec2::new(0.0, 2.4 * frame.fixture.spider.radius),
                    &format!(
                        "holds surface {}{}",
                        id.get(),
                        if departing { ", departing" } else { "" }
                    ),
                    "middle",
                );
            }
            (sketch, caption)
        })
        .collect();
    save(&dir, "slide.svg", &panels(sketches));

    // The way in: from the beam, down through the hatch, on the line.
    let descent = record(|| {
        let mut state = clock(ASPECT, ClockFont::Classic);
        descend_from_the_beam(&mut state, 0.0);
    });
    let (min, max) = whole_room(layout);
    let mut sketch = strobe(layout, &descent, 12, min, max, 1.0);
    let (beam, _) = FramePart::Beam.rect(layout);
    sketch.label(
        Vec2::new(beam.x - 8.0, beam.y + 4.0),
        "the beam above the hatch",
        "end",
    );
    sketch.label(
        Vec2::new(layout.bounds_min.x + 12.0, layout.bounds_max.y - 20.0),
        "the view ends here",
        "start",
    );
    save(&dir, "descent.svg", &sketch.finish());

    // The whole room, one frame every other tick, for the animation.
    for (index, frame) in frames.iter().step_by(2).enumerate() {
        let mut sketch = Sketch::new(min, max, 1.0);
        sketch.room(layout, &frame.floor);
        sketch.face(frame);
        sketch.spider(&frame.fixture, 1.0);
        sketch.label(
            Vec2::new(layout.bounds_min.x + 12.0, layout.bounds_max.y - 20.0),
            frame.phase,
            "start",
        );
        save(
            &dir.join("route"),
            &format!("frame-{index:04}.svg"),
            &sketch.finish(),
        );
    }
}

/// Where the lowest cell of `slot` is in `frame`.
fn lowest_cell_of(frame: &Frame, slot: u8) -> Vec2 {
    frame
        .surfaces
        .iter()
        .filter(|s| s.slot == slot)
        .min_by(|a, b| a.origin.y.total_cmp(&b.origin.y))
        .map(|s| s.origin)
        .expect("a lit slot")
}
