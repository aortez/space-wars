//! Body records and an arc ring around the ship, from circle geometry, for
//! models that read `arrival_perception_v1`. See
//! docs/design/guidance-perception.md. Readings use the ship's local frame,
//! like the nine features. Every encoded value is bounded by its transform
//! rather than clamped, so no reading saturates.

use super::*;
use crate::model::{
    Activation, DenseGenome, DenseLayer, ModelError,
    shared::{Pool, SetEncoder, SharedGenome},
};
use scenario_spacewars::surface_sortie::{
    free_flight::{CelestialBody, CelestialKind},
    pilot::PilotMotion,
};
use sha2::{Digest, Sha256};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

pub const BODY_SLOTS: usize = 8;
pub const BODY_VALUES: usize = 11;
pub const ARCS: usize = 16;
pub const ARC_VALUES: usize = 8;
/// Values that follow the nine features: every body slot, then every arc.
pub const INPUTS: usize = BODY_SLOTS * BODY_VALUES + ARCS * ARC_VALUES;
/// An arc's fill counts surface no farther than this along its directions.
pub const FILL_RANGE: f32 = 400.0;
/// Bodies compete for slots by their clearance after this many seconds at
/// the current closing speed.
pub const THREAT_SECONDS: f32 = 2.0;
const ARC: f32 = TAU / ARCS as f32;

pub fn contract() -> ModelContract {
    ModelContract {
        observation: "arrival_perception_v1".into(),
        inputs: 9 + INPUTS,
        ..super::contract()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Sun,
    Planet,
    Edge,
}

impl Kind {
    fn one_hot(self) -> [f32; 3] {
        match self {
            Self::Sun => [1.0, 0.0, 0.0],
            Self::Planet => [0.0, 1.0, 0.0],
            Self::Edge => [0.0, 0.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BodyRecord {
    pub kind: Kind,
    pub destination: bool,
    /// Unit vector to the nearest surface point; zero when that is undefined.
    pub direction: Vec2,
    pub clearance: f32,
    /// Ship velocity relative to the body along `direction`, positive while
    /// the gap closes.
    pub closing_speed: f32,
    /// The same velocity along `direction` turned a quarter counterclockwise.
    pub sideways_speed: f32,
    /// Half the angle the body subtends, over a right angle; 1 for the edge.
    pub size: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ArcNearest {
    pub distance: f32,
    pub kind: Kind,
    /// Ship velocity relative to that body along the direction to the point.
    pub closing_speed: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ArcReading {
    /// The closest surface inside the arc, at any distance.
    pub nearest: Option<ArcNearest>,
    /// Fraction of the arc's angle in which surface lies within `FILL_RANGE`.
    pub fill: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Perception {
    /// The destination first when present, then the others by threat.
    pub bodies: Vec<BodyRecord>,
    /// Bodies that did not fit in the slots.
    pub omitted: usize,
    /// Arc `i` spans local bearings from `i` to `i + 1` times 22.5°,
    /// counterclockwise from the ship's local +x axis.
    pub arcs: [ArcReading; ARCS],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EncodedPerception {
    pub inputs: Vec<f32>,
}

/// One body around the ship, in the ship's frame with the ship at the origin.
struct Circle {
    kind: Kind,
    destination: bool,
    center: Vec2,
    radius: f32,
    enclosing: bool,
    /// Ship velocity relative to the body.
    relative: Vec2,
}

impl Circle {
    fn read(body: &CelestialBody, ship: &PilotMotion, destination: Option<usize>) -> Self {
        let local = |v: Vec2| v.rotate_radians(-ship.angle);
        Self {
            kind: match body.kind {
                CelestialKind::Sun => Kind::Sun,
                CelestialKind::Planet(_) => Kind::Planet,
                CelestialKind::Edge => Kind::Edge,
            },
            destination: matches!(body.kind, CelestialKind::Planet(i) if Some(i) == destination),
            center: local(body.position - ship.position),
            radius: body.radius,
            enclosing: body.kind == CelestialKind::Edge,
            relative: local(ship.velocity - body.velocity),
        }
    }

    /// The ship is inside the solid, or outside the edge. Damaged terrain can
    /// allow it; every direction then reads as touching.
    fn engulfed(&self) -> bool {
        let d = self.center.length();
        if self.enclosing {
            d >= self.radius
        } else {
            d <= self.radius
        }
    }

    fn clearance(&self) -> f32 {
        let d = self.center.length();
        if self.enclosing {
            (self.radius - d).max(0.0)
        } else {
            (d - self.radius).max(0.0)
        }
    }

    /// Toward the nearest surface point: the center for a body, away from it
    /// for the edge.
    fn direction(&self) -> Vec2 {
        let d = self.center.length();
        if d <= f32::EPSILON * self.radius {
            Vec2::ZERO
        } else if self.enclosing {
            -self.center / d
        } else {
            self.center / d
        }
    }

    fn record(&self) -> BodyRecord {
        let direction = self.direction();
        BodyRecord {
            kind: self.kind,
            destination: self.destination,
            direction,
            clearance: self.clearance(),
            closing_speed: self.relative.dot(direction),
            sideways_speed: self.relative.dot(Vec2::new(-direction.y, direction.x)),
            size: if self.enclosing || self.engulfed() {
                1.0
            } else {
                (self.radius / self.center.length()).asin() / FRAC_PI_2
            },
        }
    }

    /// Distance along a unit direction to the surface, if it is struck.
    fn ray(&self, direction: Vec2) -> Option<f32> {
        let along = self.center.dot(direction);
        let discriminant =
            along * along - (self.center.length_squared() - self.radius * self.radius);
        if discriminant < 0.0 {
            return None;
        }
        let root = discriminant.sqrt();
        let distance = if self.enclosing {
            along + root
        } else {
            along - root
        };
        (distance >= 0.0).then_some(distance)
    }

    /// Bearing of the nearest surface point, the half-width with surface
    /// within `range`, and the half-width the surface spans at all.
    fn cover(&self, range: f32) -> (f32, f32, f32) {
        let d = self.center.length();
        let r = self.radius;
        if self.enclosing {
            let within = if d <= f32::EPSILON * r {
                if r <= range { PI } else { 0.0 }
            } else {
                ((r * r - d * d - range * range) / (2.0 * d * range))
                    .clamp(-1.0, 1.0)
                    .acos()
            };
            (bearing(-self.center), within, PI)
        } else {
            let full = (r / d).asin();
            let within = if d - r > range {
                0.0
            } else if d * d - r * r <= range * range {
                full
            } else {
                ((d * d + range * range - r * r) / (2.0 * d * range))
                    .clamp(-1.0, 1.0)
                    .acos()
                    .min(full)
            };
            (bearing(self.center), within, full)
        }
    }
}

fn wrap(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

/// Counterclockwise from +x, matching `unit`.
fn bearing(vector: Vec2) -> f32 {
    vector.y.atan2(vector.x)
}

fn unit(angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(cos, sin)
}

/// 1 at contact, 0.5 at `DISTANCE_SCALE`, toward 0 far away.
fn proximity(distance: f32) -> f32 {
    DISTANCE_SCALE / (DISTANCE_SCALE + distance.max(0.0))
}

/// Signed, ±0.5 at `SPEED_SCALE`, toward ±1 at high speed.
fn speed(value: f32) -> f32 {
    value / (value.abs() + SPEED_SCALE)
}

/// Exact nearest surface and covered fraction in each arc.
fn ring(circles: &[Circle]) -> [ArcReading; ARCS] {
    let covers: Vec<_> = circles.iter().map(|c| c.cover(FILL_RANGE)).collect();
    let mut spans = Vec::with_capacity(circles.len() * 3);
    std::array::from_fn(|arc| {
        let center = (arc as f32 + 0.5) * ARC;
        let mut nearest: Option<ArcNearest> = None;
        let mut closer = |distance: f32, circle: &Circle, toward: Vec2| {
            if nearest.is_none_or(|n| distance < n.distance) {
                nearest = Some(ArcNearest {
                    distance,
                    kind: circle.kind,
                    closing_speed: circle.relative.dot(toward),
                });
            }
        };
        spans.clear();
        for (circle, &(bearing, within, full)) in circles.iter().zip(&covers) {
            if circle.engulfed() {
                closer(0.0, circle, circle.direction());
                spans.push((-ARC / 2.0, ARC / 2.0));
                continue;
            }
            // The body's own nearest point when it lies in the arc, otherwise
            // the arc boundary closest to it.
            let offset = wrap(bearing - center);
            if offset.abs() <= ARC / 2.0 {
                closer(circle.clearance(), circle, unit(bearing));
            } else if offset.abs() - ARC / 2.0 <= full {
                let side = unit(center + (ARC / 2.0).copysign(offset));
                if let Some(distance) = circle.ray(side) {
                    closer(distance, circle, side);
                }
            }
            for shift in [-TAU, 0.0, TAU] {
                let low = (offset + shift - within).max(-ARC / 2.0);
                let high = (offset + shift + within).min(ARC / 2.0);
                if high > low {
                    spans.push((low, high));
                }
            }
        }
        spans.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let (mut covered, mut end) = (0.0, -ARC);
        for &(low, high) in &spans {
            covered += (high - low.max(end)).max(0.0);
            end = end.max(high);
        }
        ArcReading {
            nearest,
            fill: (covered / ARC).clamp(0.0, 1.0),
        }
    })
}

impl Perception {
    /// `destination` is the planet being approached, which keeps the first
    /// slot.
    pub fn read(
        observation: &FreeFlightObservation,
        bodies: &[CelestialBody],
        destination: Option<usize>,
    ) -> Self {
        let circles: Vec<_> = bodies
            .iter()
            .map(|body| Circle::read(body, &observation.ship, destination))
            .collect();
        let mut records: Vec<_> = circles.iter().map(Circle::record).collect();
        let threat = |r: &BodyRecord| r.clearance - THREAT_SECONDS * r.closing_speed.max(0.0);
        // Stable, so equal threats keep the scenario's body order.
        records.sort_by(|a, b| {
            b.destination
                .cmp(&a.destination)
                .then(threat(a).total_cmp(&threat(b)))
                .then(a.clearance.total_cmp(&b.clearance))
        });
        let omitted = records.len().saturating_sub(BODY_SLOTS);
        records.truncate(BODY_SLOTS);
        Self {
            bodies: records,
            omitted,
            arcs: ring(&circles),
        }
    }

    pub fn encode(&self) -> Result<EncodedPerception, GuidanceError> {
        let mut inputs = Vec::with_capacity(INPUTS);
        for slot in 0..BODY_SLOTS {
            let Some(body) = self.bodies.get(slot) else {
                inputs.extend([0.0; BODY_VALUES]);
                continue;
            };
            inputs.push(1.0);
            inputs.extend(body.kind.one_hot());
            inputs.extend([
                f32::from(u8::from(body.destination)),
                body.direction.x,
                body.direction.y,
                proximity(body.clearance),
                speed(body.closing_speed),
                speed(body.sideways_speed),
                body.size,
            ]);
        }
        for (index, arc) in self.arcs.iter().enumerate() {
            let (distance, kind, closing) = arc.nearest.map_or((0.0, [0.0; 3], 0.0), |n| {
                (
                    proximity(n.distance),
                    n.kind.one_hot(),
                    speed(n.closing_speed),
                )
            });
            let center = unit((index as f32 + 0.5) * ARC);
            inputs.extend([distance, arc.fill]);
            inputs.extend(kind);
            inputs.extend([closing, center.x, center.y]);
        }
        if inputs.iter().any(|v| !v.is_finite()) {
            return Err(GuidanceError("non-finite perception".into()));
        }
        Ok(EncodedPerception { inputs })
    }
}

/// A perception genome that computes exactly what `parent` computes. The
/// trunk is the parent network with zero weights for the pooled summaries;
/// each encoder of `width` tanh units holds deterministic uniform weights and
/// biases in ±`scale`. Nonzero encoders let a single mutation of a trunk
/// weight change behavior.
pub fn warm_start(
    parent: &DenseGenome,
    width: usize,
    scale: f32,
    seed: u64,
) -> Result<SharedGenome, ModelError> {
    if parent.contract != super::contract() {
        return Err(ModelError(
            "a warm start needs a nine-feature guidance genome".into(),
        ));
    }
    if !scale.is_finite() || scale < 0.0 {
        return Err(ModelError("invalid initial encoder scale".into()));
    }
    let mut counter = 0_u64;
    let mut draw = || {
        let mut hash = Sha256::new();
        hash.update(b"spacewars.perception.warm_start.v1\0");
        hash.update(seed.to_le_bytes());
        hash.update(counter.to_le_bytes());
        counter += 1;
        let bits = u64::from_le_bytes(hash.finalize()[..8].try_into().expect("eight bytes"));
        ((bits >> 40) as f32 / 16_777_216.0 * 2.0 - 1.0) * scale
    };
    let mut layer = |inputs: usize| DenseLayer {
        outputs: width,
        activation: Activation::Tanh,
        weights: (0..inputs * width).map(|_| draw()).collect(),
        biases: (0..width).map(|_| draw()).collect(),
    };
    let bodies = SetEncoder {
        items: BODY_SLOTS,
        item_width: BODY_VALUES,
        presence: Some(0),
        pool: Pool::Max,
        layer: layer(BODY_VALUES),
    };
    let arcs = SetEncoder {
        items: ARCS,
        item_width: ARC_VALUES,
        presence: None,
        pool: Pool::Max,
        layer: layer(ARC_VALUES),
    };
    SharedGenome::from_dense(parent, contract(), vec![bodies, arcs])
}

#[cfg(test)]
mod tests;
