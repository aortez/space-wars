//! Where a weapon's energy goes (#329): how a blast's front and a beam's
//! cross-section divide among the recipients they reach, which recipients a
//! shot's sweep crosses, and which lie under a footprint. A recipient is one
//! or more circles in the plane, each a [`Target`] with the recipient's key
//! and its represented thickness into the screen.
//!
//! Energy travels in straight lines, outward from a blast's center or along a
//! beam. Where it meets a target, the target takes the fraction of what
//! arrives that its thickness covers of the front's height, and the rest
//! carries on behind it. So a target as thick as the front shadows everything
//! behind it, a thin blade lets most of the energy through, and the shares
//! never sum past the whole. The results give each recipient's
//! [`Share`](crate::Share) its coupling and its face's width. See
//! `docs/design/material-sources.md`.

use crate::MaterialError;
use crate::geometry::{MAX_LENGTH, MIN_LENGTH, length_ok};
use engine_core::Vec2;
use serde::Serialize;
use std::collections::BTreeMap;
use std::f64::consts::TAU;

/// One circle of a recipient, as a source sees it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Target {
    /// The recipient's identity to the host, as in a [`Share`](crate::Share).
    /// The results combine a recipient's circles, which take from each line
    /// of energy once, as one body. Circles of different recipients should
    /// not overlap, so that energy meets them in one order across each part
    /// of a front. Under a footprint they may.
    pub key: u64,
    pub center: Vec2,
    /// `wu`.
    pub radius: f32,
    /// Represented depth into the screen, `wu`.
    pub thickness: f32,
}

impl Target {
    pub fn validate(self) -> Result<(), MaterialError> {
        if point_ok(self.center) && length_ok(self.radius) && length_ok(self.thickness) {
            Ok(())
        } else {
            Err(MaterialError::InvalidGeometry)
        }
    }
}

/// One recipient's part of a blast or a beam.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Reach {
    pub key: u64,
    /// The fraction of the source's energy the recipient takes: its share's
    /// coupling.
    pub coupling: f32,
    /// Width of the face the energy reaches, `wu`: its share's face width,
    /// within the lengths a [`Face`](crate::Face) accepts.
    pub width: f32,
}

/// What a beam's path takes, and what reaches its end.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PathReach {
    /// In key order.
    pub reaches: Vec<Reach>,
    /// The fraction of the beam that reaches the end of the path: the coupling
    /// of the hard surface where the beam ends, or what goes on past it.
    pub through: f32,
}

/// A recipient a shot's sweep crosses.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Crossing {
    pub key: u64,
    /// Distance along the sweep where the shot first touches the recipient,
    /// `wu`. Zero for one it touches at the start.
    pub distance: f32,
}

/// A recipient under a footprint.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Overlap {
    pub key: u64,
    /// The fraction of the footprint's load the recipient carries.
    pub share: f32,
    /// Length of the footprint over the recipient, `wu`: its load's width,
    /// within the lengths a [`SustainedLoad`](crate::SustainedLoad) accepts.
    pub width: f32,
}

fn point_ok(point: Vec2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn validate(targets: &[Target]) -> Result<(), MaterialError> {
    targets.iter().try_for_each(|target| target.validate())
}

/// A width the laws accept: a sliver of overlap or a vast target still makes
/// a valid face.
fn face_width(width: f64) -> f32 {
    (width as f32).clamp(MIN_LENGTH, MAX_LENGTH)
}

/// A line from `start` to `end`, in `f64`.
struct Line {
    start: (f64, f64),
    along: (f64, f64),
    length: f64,
}

impl Line {
    fn new(start: Vec2, end: Vec2) -> Self {
        let start = (f64::from(start.x), f64::from(start.y));
        let (dx, dy) = (f64::from(end.x) - start.0, f64::from(end.y) - start.1);
        let length = libm::sqrt(dx * dx + dy * dy);
        let along = if length > 0.0 {
            (dx / length, dy / length)
        } else {
            (0.0, 0.0)
        };
        Self {
            start,
            along,
            length,
        }
    }

    /// A point's distance along the line from its start, and its signed offset
    /// to the left of it.
    fn coordinates(&self, point: Vec2) -> (f64, f64) {
        let (x, y) = (
            f64::from(point.x) - self.start.0,
            f64::from(point.y) - self.start.1,
        );
        (
            x * self.along.0 + y * self.along.1,
            y * self.along.0 - x * self.along.1,
        )
    }
}

/// Part of a target's extent across a front, from `lo` to `hi`. `along` and
/// `offset` place the target's center: its distance and direction from a
/// blast, or its distance along a beam and its position across it.
struct Span {
    index: usize,
    key: u64,
    lo: f64,
    hi: f64,
    take: f64,
    along: f64,
    offset: f64,
    radius: f64,
}

/// How energy crosses a front: outward from a point, or along parallel lines.
#[derive(Clone, Copy)]
enum Front {
    Radial,
    Parallel,
}

impl Front {
    /// How far the energy at `coordinate` across the front travels before it
    /// meets the span's target: negative where it starts inside the target.
    fn distance(self, span: &Span, coordinate: f64) -> f64 {
        let (along, across) = match self {
            Front::Radial => {
                let turn = coordinate - span.offset;
                (span.along * libm::cos(turn), span.along * libm::sin(turn))
            }
            Front::Parallel => (span.along, coordinate - span.offset),
        };
        along - libm::sqrt((span.radius * span.radius - across * across).max(0.0))
    }
}

/// A unit of energy spread evenly over a front of `measure`, as each target
/// took it.
struct Spread {
    shares: Vec<f64>,
    /// The extent over which each target took anything.
    covered: Vec<f64>,
    through: f64,
}

/// The pieces of `[0, measure)` between the edges of every item's extent,
/// `lo` to `hi`, each with its length, its middle and the items that cover it.
fn pieces<T>(
    items: &[T],
    extent: impl Fn(&T) -> (f64, f64),
    measure: f64,
) -> Vec<(f64, f64, Vec<&T>)> {
    let mut edges: Vec<f64> = items
        .iter()
        .flat_map(|item| {
            let (lo, hi) = extent(item);
            [lo, hi]
        })
        .chain([0.0, measure])
        .collect();
    edges.sort_by(f64::total_cmp);
    edges.dedup();
    edges
        .windows(2)
        .map(|pair| {
            let middle = 0.5 * (pair[0] + pair[1]);
            let covering = items
                .iter()
                .filter(|item| {
                    let (lo, hi) = extent(item);
                    lo <= middle && middle < hi
                })
                .collect();
            (pair[1] - pair[0], middle, covering)
        })
        .collect()
}

/// In each piece of the front, each covering span takes its fraction of what
/// is left, in the order the energy meets them, and what passes them all goes
/// through. A recipient takes once, at the first of its circles the energy
/// meets. Circles of different recipients that do not overlap meet the
/// energy in one order across a whole piece, so the order along its middle
/// holds for all of it.
fn spread(spans: &[Span], front: Front, measure: f64, count: usize) -> Spread {
    let mut spread = Spread {
        shares: vec![0.0; count],
        covered: vec![0.0; count],
        through: 0.0,
    };
    let mut order = Vec::new();
    let mut met = Vec::new();
    for (extent, middle, covering) in pieces(spans, |span| (span.lo, span.hi), measure) {
        order.clear();
        order.extend(
            covering
                .into_iter()
                .map(|span| (front.distance(span, middle), span)),
        );
        order.sort_by(|(a_distance, a), (b_distance, b)| {
            a_distance.total_cmp(b_distance).then(a.index.cmp(&b.index))
        });
        let fraction = extent / measure;
        let mut left = 1.0;
        met.clear();
        for (_, span) in &order {
            if left <= 0.0 {
                break;
            }
            if met.contains(&span.key) {
                continue;
            }
            met.push(span.key);
            let taken = left * span.take;
            spread.shares[span.index] += taken * fraction;
            spread.covered[span.index] += extent;
            left -= taken;
        }
        spread.through += left * fraction;
    }
    spread
}

/// The spread's shares, combined per key in key order, with each target's
/// face width.
fn reaches(targets: &[Target], spread: &Spread, width: impl Fn(usize) -> f64) -> Vec<Reach> {
    let mut merged = BTreeMap::<u64, (f64, f64)>::new();
    for (index, target) in targets.iter().enumerate() {
        if spread.shares[index] > 0.0 {
            let entry = merged.entry(target.key).or_default();
            entry.0 += spread.shares[index];
            entry.1 += width(index);
        }
    }
    merged
        .into_iter()
        .map(|(key, (coupling, width))| Reach {
            key,
            coupling: coupling as f32,
            width: face_width(width),
        })
        .collect()
}

fn take(target: &Target, height: f32) -> f64 {
    (f64::from(target.thickness) / f64::from(height)).min(1.0)
}

/// A blast at `center` spreads its energy evenly over every direction in the
/// plane, out to `radius`, with a front `height` deep into the screen. A target
/// takes the directions in which the energy meets it within `radius`: every
/// direction it subtends, unless it straddles the blast's edge, where the
/// directions that would meet it beyond `radius` are left out. Its face is the
/// part of its diameter that the energy reaches. What meets no target, or
/// passes every target within `radius`, reaches no recipient. A blast inside
/// a target meets it first in every direction. The results are in key order.
pub fn blast_reach(
    center: Vec2,
    radius: f32,
    height: f32,
    targets: &[Target],
) -> Result<Vec<Reach>, MaterialError> {
    if !(point_ok(center) && length_ok(radius) && length_ok(height)) {
        return Err(MaterialError::InvalidGeometry);
    }
    validate(targets)?;
    let mut spans = Vec::new();
    let mut extent = vec![0.0; targets.len()];
    for (index, target) in targets.iter().enumerate() {
        let (dx, dy) = (
            f64::from(target.center.x) - f64::from(center.x),
            f64::from(target.center.y) - f64::from(center.y),
        );
        let distance = libm::sqrt(dx * dx + dy * dy);
        let r = f64::from(target.radius);
        let reach = f64::from(radius);
        if distance - r >= reach {
            continue;
        }
        let take = take(target, height);
        let middle = libm::atan2(dy, dx).rem_euclid(TAU);
        let span = |lo, hi| Span {
            index,
            key: target.key,
            lo,
            hi,
            take,
            along: distance,
            offset: middle,
            radius: r,
        };
        if distance <= r {
            extent[index] = TAU;
            spans.push(span(0.0, TAU));
            continue;
        }
        let subtended = libm::asin(r / distance);
        extent[index] = 2.0 * subtended;
        // Past the point where a line grazes the target, at `sqrt(d^2 - r^2)`,
        // the lines that still meet it within reach are those within the
        // angle where the blast's edge crosses its circle.
        let half = if distance * distance - r * r <= reach * reach {
            subtended
        } else {
            let cos = (distance * distance + reach * reach - r * r) / (2.0 * distance * reach);
            libm::acos(cos.min(1.0))
        };
        let (lo, hi) = (middle - half, middle + half);
        if lo < 0.0 {
            spans.extend([span(lo + TAU, TAU), span(0.0, hi)]);
        } else if hi > TAU {
            spans.extend([span(lo, TAU), span(0.0, hi - TAU)]);
        } else {
            spans.push(span(lo, hi));
        }
    }
    let spread = spread(&spans, Front::Radial, TAU, targets.len());
    Ok(reaches(targets, &spread, |index| {
        2.0 * f64::from(targets[index].radius) * spread.covered[index] / extent[index]
    }))
}

/// A beam from `start` to `end`, where its trace ends on a hard surface or
/// runs out, spreads its energy evenly across its `width`, with a cross-section
/// `height` deep into the screen. A target takes the part of the width its
/// diameter covers, if any of it lies between the ends, and its face is the
/// part of the width that the energy reaches.
pub fn beam_reach(
    start: Vec2,
    end: Vec2,
    width: f32,
    height: f32,
    targets: &[Target],
) -> Result<PathReach, MaterialError> {
    if !(point_ok(start) && point_ok(end) && length_ok(width) && length_ok(height)) {
        return Err(MaterialError::InvalidGeometry);
    }
    validate(targets)?;
    let line = Line::new(start, end);
    let half = 0.5 * f64::from(width);
    let mut spans = Vec::new();
    if line.length > 0.0 {
        for (index, target) in targets.iter().enumerate() {
            let (along, across) = line.coordinates(target.center);
            let r = f64::from(target.radius);
            if along + r <= 0.0 || along - r >= line.length {
                continue;
            }
            let (lo, hi) = ((across - r).max(-half), (across + r).min(half));
            if hi > lo {
                spans.push(Span {
                    index,
                    key: target.key,
                    lo: lo + half,
                    hi: hi + half,
                    take: take(target, height),
                    along,
                    offset: across + half,
                    radius: r,
                });
            }
        }
    }
    let spread = spread(&spans, Front::Parallel, f64::from(width), targets.len());
    Ok(PathReach {
        reaches: reaches(targets, &spread, |index| spread.covered[index]),
        through: spread.through as f32,
    })
}

/// The recipients that a shot of `radius` touches as it sweeps from `start` to
/// `end`, in the order it reaches them, with ties in key order. A recipient
/// already touching the shot at the start comes first, at distance zero.
pub fn sweep_crossings(
    start: Vec2,
    end: Vec2,
    radius: f32,
    targets: &[Target],
) -> Result<Vec<Crossing>, MaterialError> {
    if !(point_ok(start) && point_ok(end) && length_ok(radius)) {
        return Err(MaterialError::InvalidGeometry);
    }
    validate(targets)?;
    let line = Line::new(start, end);
    let mut first = BTreeMap::<u64, f64>::new();
    for target in targets {
        let reach = f64::from(target.radius) + f64::from(radius);
        let (along, across) = line.coordinates(target.center);
        // With no motion, the coordinates are zero and only the gap counts.
        let gap = if line.length > 0.0 {
            across * across
        } else {
            let (x, y) = (
                f64::from(target.center.x) - line.start.0,
                f64::from(target.center.y) - line.start.1,
            );
            x * x + y * y
        };
        if gap >= reach * reach {
            continue;
        }
        let half = libm::sqrt(reach * reach - gap);
        if along + half < 0.0 || along - half > line.length {
            continue;
        }
        let distance = (along - half).max(0.0);
        first
            .entry(target.key)
            .and_modify(|first| *first = first.min(distance))
            .or_insert(distance);
    }
    let mut crossings: Vec<Crossing> = first
        .into_iter()
        .map(|(key, distance)| Crossing {
            key,
            distance: distance as f32,
        })
        .collect();
    crossings.sort_by(|a, b| a.distance.total_cmp(&b.distance).then(a.key.cmp(&b.key)));
    Ok(crossings)
}

/// The recipients under a footprint from `start` to `end`, such as the patch
/// where a body rests on the ground, and the share of its load each carries,
/// in key order. The load spreads evenly along the footprint. Recipients side
/// by side under the same stretch share it equally, however many of their
/// circles lie there, and a stretch over no recipient rests on the ground. A
/// recipient's width is the length of footprint over it.
pub fn footprint_overlaps(
    start: Vec2,
    end: Vec2,
    targets: &[Target],
) -> Result<Vec<Overlap>, MaterialError> {
    validate(targets)?;
    let line = Line::new(start, end);
    // This also refuses ends that are not finite.
    if !length_ok(line.length as f32) {
        return Err(MaterialError::InvalidGeometry);
    }
    // Each circle's key and its stretch of the footprint, `lo` to `hi`.
    let mut under = Vec::new();
    for target in targets {
        let (along, across) = line.coordinates(target.center);
        let r = f64::from(target.radius);
        if across.abs() >= r {
            continue;
        }
        let half = libm::sqrt(r * r - across * across);
        let (lo, hi) = ((along - half).max(0.0), (along + half).min(line.length));
        if hi > lo {
            under.push((target.key, lo, hi));
        }
    }
    let mut merged = BTreeMap::<u64, (f64, f64)>::new();
    let mut keys = Vec::new();
    for (extent, _, covering) in pieces(&under, |&(_, lo, hi)| (lo, hi), line.length) {
        keys.clear();
        keys.extend(covering.iter().map(|(key, _, _)| *key));
        keys.sort_unstable();
        keys.dedup();
        for key in &keys {
            let entry = merged.entry(*key).or_default();
            entry.0 += extent / line.length / keys.len() as f64;
            entry.1 += extent;
        }
    }
    Ok(merged
        .into_iter()
        .map(|(key, (share, width))| Overlap {
            key,
            share: share as f32,
            width: face_width(width),
        })
        .collect())
}
