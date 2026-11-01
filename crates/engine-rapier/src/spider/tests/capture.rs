//! Drawings of the spider on its fixture courses, for the docs. Ignored; give
//! it a directory to write SVG files into:
//!
//! `SPACEWARS_SPIDER_CAPTURE=<dir> cargo test -p engine-rapier --lib spider::tests::capture -- --ignored`
//!
//! docs/clock-spider-mechanics.md says how they become the PNG and GIF files.
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::*;

const PAPER: &str = "#f7f5f0";
const INK: &str = "#30343b";
const SOLID: &str = "#a7afb8";
const SOLID_EDGE: &str = "#6b7380";
const FOREIGN_SOLID: &str = "#d9908a";
const BODY: &str = "#262a30";
const EYE: &str = "#5d6570";
const SILK: &str = "#2f6db5";
const HOLD: &str = "#e07b39";
const TRAIL: &str = "#8d8d8d";
const FALL: &str = "#c0392b";

const SHELF: PhysicsId = PhysicsId::new(7);
const PANE: PhysicsId = PhysicsId::new(FOREIGN.value() + 1);

/// A drawing in radii, y up, of the region from `min` to `max`.
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
            r#"<rect width="{width}" height="{height}" fill="{PAPER}"/>"#
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

    fn block(&mut self, center: Vec2, half: Vec2, fill: &str, outline_only: bool) {
        let (x, y) = self.at(center + Vec2::new(-half.x, half.y));
        let (width, height) = (half.x * 2.0 * self.px, half.y * 2.0 * self.px);
        let style = if outline_only {
            format!(r#"fill="none" stroke="{SOLID_EDGE}" stroke-dasharray="4 3""#)
        } else {
            format!(r#"fill="{fill}" stroke="{SOLID_EDGE}""#)
        };
        self.raw(format!(
            r#"<rect x="{x:.1}" y="{y:.1}" width="{width:.1}" height="{height:.1}" {style}/>"#
        ));
    }

    fn course(&mut self) {
        for (_, center, half) in COURSE {
            self.block(center, half, SOLID, false);
        }
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

    fn cross(&mut self, point: Vec2, color: &str) {
        let (x, y) = self.at(point);
        self.raw(format!(
            r#"<path d="M{a:.1},{b:.1} L{c:.1},{e:.1} M{a:.1},{e:.1} L{c:.1},{b:.1}" stroke="{color}" stroke-width="2.5"/>"#,
            a = x - 6.0,
            b = y - 6.0,
            c = x + 6.0,
            e = y + 6.0,
        ));
    }

    fn label(&mut self, point: Vec2, text: &str, anchor: &str) {
        let (x, y) = self.at(point);
        self.raw(format!(
            r#"<text x="{x:.1}" y="{y:.1}" font-family="DejaVu Sans, sans-serif" font-size="13" fill="{INK}" stroke="{PAPER}" stroke-width="4" paint-order="stroke" text-anchor="{anchor}">{text}</text>"#
        ));
    }

    /// A caption along the top edge.
    fn title(&mut self, text: &str) {
        let point = Vec2::new(self.min.x, self.max.y) + Vec2::new(10.0, -18.0) / self.px;
        let (x, y) = self.at(point);
        self.raw(format!(
            r#"<text x="{x:.1}" y="{y:.1}" font-family="DejaVu Sans, sans-serif" font-size="14" font-weight="bold" fill="{INK}">{text}</text>"#
        ));
    }

    /// The spider as its state has it: the body the physics moves, eight
    /// drawn legs splayed from its back toward its feet, the line or shot it
    /// holds, and the point it holds.
    fn spider(&mut self, state: &SpiderState, r: f32, opacity: f32) {
        let center = state.position / r;
        self.raw(format!(r#"<g opacity="{opacity}">"#));
        if let Some(line) = state.line {
            let anchor = line.anchor / r;
            self.path(&[center, anchor], SILK, 2.0, false);
            self.dot(anchor, 3.5, SILK);
        }
        if let Some(shot) = state.shot {
            self.path(&[center, shot.tip / r], SILK, 2.0, true);
            self.dot(shot.tip / r, 3.5, SILK);
        }
        let up = state.up;
        let side = Vec2::new(up.y, -up.x);
        let mut legs = String::new();
        for sign in [-1.0, 1.0] {
            for (spread, lift) in [(0.6, 0.45), (1.05, 0.7), (1.5, 0.7), (1.9, 0.45)] {
                let knee = center + side * (sign * (0.55 + spread * 0.55)) + up * lift;
                let foot = center + side * (sign * spread) - up * 0.95;
                let [(cx, cy), (kx, ky), (fx, fy)] =
                    [self.at(center), self.at(knee), self.at(foot)];
                let _ = write!(legs, "M{cx:.1},{cy:.1} L{kx:.1},{ky:.1} L{fx:.1},{fy:.1} ");
            }
        }
        let (x, y) = self.at(center);
        let (hx, hy) = self.at(center + up * 0.4);
        self.raw(format!(
            r#"<path d="{legs}" fill="none" stroke="{BODY}" stroke-width="1.6" stroke-linejoin="round"/><circle cx="{x:.1}" cy="{y:.1}" r="{body:.1}" fill="{BODY}"/><circle cx="{hx:.1}" cy="{hy:.1}" r="{eye:.1}" fill="{EYE}"/>"#,
            body = self.px,
            eye = self.px * 0.3,
        ));
        if let Some(support) = state.support {
            self.dot(support.point / r, 3.5, HOLD);
        }
        self.raw("</g>".to_owned());
    }

    fn finish(self) -> String {
        let (width, height) = self.size();
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<defs><clipPath id="frame"><rect width="{width}" height="{height}"/></clipPath></defs>
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
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<rect width="{width}" height="{height}" fill="white"/>
"#
    );
    let mut x = 0.0;
    for (i, (sketch, text)) in sketches.into_iter().enumerate() {
        let (w, h) = sketch.size();
        let _ = write!(
            svg,
            r#"<clipPath id="panel{i}"><rect width="{w}" height="{h}"/></clipPath>
<g transform="translate({x},0)" clip-path="url(#panel{i})">
{}</g>
<text x="{cx}" y="{ty}" font-family="DejaVu Sans, sans-serif" font-size="13" fill="{INK}" text-anchor="middle">{text}</text>
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

#[test]
#[ignore = "set SPACEWARS_SPIDER_CAPTURE to a directory to draw the spider for the docs"]
fn draw_the_spider_for_the_docs() {
    let dir = PathBuf::from(
        std::env::var_os("SPACEWARS_SPIDER_CAPTURE").expect("set SPACEWARS_SPIDER_CAPTURE"),
    );
    std::fs::create_dir_all(dir.join("swing")).unwrap();
    climb(&dir);
    swing(&dir);
    shot(&dir);
    grip_limit(&dir);
    let_go(&dir);
}

/// Around the room from the floor to the ceiling.
fn climb(dir: &Path) {
    let scale = CLOCK;
    let mut run = Run::new(course(scale), scale, Vec2::new(-4.0, 1.0));
    let mut sketch = Sketch::new(Vec2::new(-65.0, -5.0), Vec2::new(65.0, 49.0), 8.0);
    sketch.course();
    let mut trail = Vec::new();
    let mut strobe = Vec::new();
    for tick in 0..1860 {
        let state = run.tick(crawl(-1.0));
        trail.push(state.position / scale.r);
        if tick % 60 == 0 {
            strobe.push(state);
        }
    }
    sketch.path(&trail, TRAIL, 1.5, true);
    for state in &strobe {
        sketch.spider(state, scale.r, 0.3);
    }
    sketch.spider(&run.state(), scale.r, 1.0);
    sketch.label(Vec2::new(-4.0, 3.5), "start", "middle");
    sketch.label(Vec2::new(-9.5, 7.0), "inside corner", "start");
    sketch.label(Vec2::new(-9.5, 18.5), "outside corner", "start");
    sketch.label(Vec2::new(-57.5, 20.0), "up the wall", "start");
    sketch.label(
        Vec2::new(-30.0, 36.0),
        "upside down along the ceiling",
        "middle",
    );
    sketch.title("Crawling the room against gravity: floor, mesa face, mesa top, wall, ceiling");
    save(dir, "climb.svg", &sketch.finish());
}

/// Across the gap on a line, and the frames of it for an animation.
fn swing(dir: &Path) {
    let scale = CLOCK;
    let mut run = Run::new(course(scale), scale, GapCrossing::START);
    let mut brain = GapCrossing::default();
    let (min, max) = (Vec2::new(-30.0, -2.0), Vec2::new(30.0, 44.0));
    let mut ticks = Vec::new();
    for tick in 0..900 {
        let state = run.state();
        let intent = brain.intent(&state, scale);
        let state = run.tick(intent);
        ticks.push((brain.stage, state));
        if tick % 3 == 0 {
            let mut frame = Sketch::new(min, max, 10.0);
            frame.course();
            frame.spider(&state, scale.r, 1.0);
            frame.title(&format!(
                "{:?}   t = {:.2} s",
                brain.stage,
                (tick + 1) as f32 * DT
            ));
            save(
                &dir.join("swing"),
                &format!("frame-{:04}.svg", tick / 3),
                &frame.finish(),
            );
        }
        if brain.stage == GapStage::Climb && state.line.is_none() {
            break;
        }
    }
    let mut sketch = Sketch::new(min, max, 10.0);
    sketch.course();
    for (stages, color) in [
        (
            &[
                GapStage::Settle,
                GapStage::Shoot,
                GapStage::PayOut,
                GapStage::Edge,
            ][..],
            TRAIL,
        ),
        (&[GapStage::Drop], FALL),
        (&[GapStage::Swing], SILK),
        (&[GapStage::Climb], HOLD),
    ] {
        let points: Vec<Vec2> = ticks
            .iter()
            .filter(|(stage, _)| stages.contains(stage))
            .map(|(_, state)| state.position / scale.r)
            .collect();
        sketch.path(&points, color, 2.0, false);
    }
    for (i, (stage, state)) in ticks.iter().enumerate() {
        if i % 9 == 0 && matches!(stage, GapStage::Drop | GapStage::Swing) {
            sketch.spider(state, scale.r, 0.3);
        }
    }
    let (_, last) = ticks.last().unwrap();
    sketch.spider(last, scale.r, 1.0);
    sketch.label(Vec2::new(-15.0, 20.0), "crawl to the edge", "middle");
    sketch.label(Vec2::new(-9.0, 9.0), "let go", "end");
    sketch.label(Vec2::new(0.0, 3.5), "swing", "middle");
    sketch.label(Vec2::new(10.5, 9.0), "catch", "end");
    sketch.label(Vec2::new(16.0, 21.0), "climb, cut", "start");
    sketch.title("Across the gap: tie to the ceiling, let go, swing, catch the far face");
    save(dir, "swing.svg", &sketch.finish());
}

/// A shot's visible extension, its tie, and a second line replacing it.
fn shot(dir: &Path) {
    let scale = CLOCK;
    let mut run = Run::new(course(scale), scale, Vec2::new(0.0, 1.0));
    run.ticks(2, grip());
    let view = || {
        let mut sketch = Sketch::new(Vec2::new(-12.0, -1.0), Vec2::new(12.0, 42.5), 8.0);
        sketch.course();
        sketch
    };
    let draw = |state: &SpiderState| {
        let mut sketch = view();
        sketch.spider(state, scale.r, 1.0);
        sketch
    };
    run.tick(shoot(Vec2::Y));
    let flying = run.ticks(7, grip());
    let tied = run
        .until(30, grip(), |s| s.line.is_some())
        .map(|_| run.state())
        .unwrap();
    run.tick(shoot(Vec2::new(0.15, 1.0)));
    let replacing = run.ticks(6, grip());
    run.until(30, grip(), |s| s.shot.is_none()).unwrap();
    let replaced = run.state();
    let mut retired = draw(&replaced);
    retired.cross(tied.line.unwrap().anchor / scale.r, FALL);
    save(
        dir,
        "shot.svg",
        &panels(vec![
            (draw(&flying), "the shot extends"),
            (draw(&tied), "it ties on arrival"),
            (draw(&replacing), "a second shot"),
            (retired, "the old line retired"),
        ]),
    );
}

/// A weak grip flying off a corner it cannot round, and a full grip going
/// round under the shelf.
fn grip_limit(dir: &Path) {
    let scale = CLOCK;
    let mut sketches = Vec::new();
    for (grip_weights, caption) in [
        (1.1, "grip 1.1 weights: overloaded at the corner"),
        (3.0, "grip 3 weights: on round underneath"),
    ] {
        let mut world = course(scale);
        block(
            &mut world,
            scale,
            SHELF,
            Vec2::new(0.0, 26.0),
            Vec2::new(6.0, 2.0),
        );
        let spec = SpiderSpec {
            grip: grip_weights,
            ..SpiderSpec::new(scale.r, scale.g)
        };
        let mut run = Run::with_spec(world, scale, Vec2::new(-4.0, 29.0), spec);
        let mut sketch = Sketch::new(Vec2::new(-11.0, 13.0), Vec2::new(15.0, 33.0), 12.0);
        sketch.block(Vec2::new(0.0, 26.0), Vec2::new(6.0, 2.0), SOLID, false);
        let mut held = Vec::new();
        let mut fallen = Vec::new();
        let mut strobe = Vec::new();
        let mut released = None;
        for tick in 0..330 {
            let state = run.tick(crawl(1.0));
            if released.is_none() && state.last_release.is_some() {
                released = Some(state);
            }
            if released.is_some() {
                fallen.push(state.position / scale.r);
                if fallen.len() == 26 {
                    break;
                }
            } else {
                held.push(state.position / scale.r);
                if tick % 30 == 0 {
                    strobe.push(state);
                }
            }
        }
        sketch.path(&held, TRAIL, 1.5, true);
        sketch.path(&fallen, FALL, 2.0, true);
        for state in &strobe {
            sketch.spider(state, scale.r, 0.3);
        }
        match released {
            Some(state) => {
                sketch.spider(&state, scale.r, 1.0);
                sketch.cross(state.position / scale.r, FALL);
                let reason = state.last_release.unwrap().reason;
                sketch.label(
                    state.position / scale.r + Vec2::new(2.2, -0.4),
                    &format!("{reason:?}"),
                    "start",
                );
            }
            None => sketch.spider(&run.state(), scale.r, 1.0),
        }
        sketches.push((sketch, caption));
    }
    save(dir, "grip-limit.svg", &panels(sketches));
}

/// A line let go when a solid comes between, and when its anchor leaves.
fn let_go(dir: &Path) {
    let scale = CLOCK;
    let mut sketches = Vec::new();
    for removed in [false, true] {
        let mut world = course(scale);
        if removed {
            block(
                &mut world,
                scale,
                SHELF,
                Vec2::new(0.0, 30.0),
                Vec2::new(6.0, 1.0),
            );
        }
        let mut run = Run::new(world, scale, Vec2::new(0.0, 20.0));
        let hung = run.hang_from_ceiling();
        let line = hung.line.unwrap();
        let mut sketch = Sketch::new(Vec2::new(-16.0, -1.0), Vec2::new(16.0, 42.5), 8.0);
        sketch.course();
        let middle = (hung.position + line.anchor) / 2.0 / scale.r;
        if removed {
            assert!(run.world.remove_entity(SHELF));
            sketch.block(Vec2::new(0.0, 30.0), Vec2::new(6.0, 1.0), SOLID, true);
        } else {
            block(&mut run.world, scale, PANE, middle, Vec2::new(3.0, 0.5));
            sketch.block(middle, Vec2::new(3.0, 0.5), FOREIGN_SOLID, false);
        }
        sketch.path(
            &[hung.position / scale.r, line.anchor / scale.r],
            SILK,
            2.0,
            true,
        );
        let released = run
            .until(5, SpiderIntent::default(), |s| s.line.is_none())
            .map(|_| run.state())
            .unwrap();
        let mut fall = vec![released.position / scale.r];
        for _ in 0..30 {
            fall.push(run.tick(SpiderIntent::default()).position / scale.r);
        }
        sketch.path(&fall, FALL, 2.0, true);
        sketch.spider(&released, scale.r, 1.0);
        let reason = released.last_release.unwrap().reason;
        sketch.label(
            released.position / scale.r + Vec2::new(1.8, 0.0),
            &format!("{reason:?}"),
            "start",
        );
        let caption = if removed {
            "its anchor leaves"
        } else {
            "a pane comes between"
        };
        sketches.push((sketch, caption));
    }
    save(dir, "let-go.svg", &panels(sketches));
}
