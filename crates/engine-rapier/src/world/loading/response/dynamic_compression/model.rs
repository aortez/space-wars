//! Discrete-work reaction; no continuous-solution oracle is used here.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct History {
    initial: StoredRegion,
    pub(super) mass: f32,
    pub(super) acceleration: f32,
    initial_x: f64,
    pub(super) summary: DynamicCompressionSummary,
}

pub(super) struct MaterialCurve {
    pub k: f64,
    pub y: f64,
    pub ey: f64,
    pub eta: f64,
    pub depth: f64,
}
pub(super) struct Segment {
    pub p: f64,
    pub dp: f64,
    pub elastic: f64,
    pub loss: f64,
    pub work: f64,
    pub force: f64,
    pub mean: f64,
}
impl MaterialCurve {
    pub(super) fn new(material: &StoredRegion) -> Self {
        let area = 2.0 * material.thickness;
        let k = f64::from(
            material
                .section()
                .contact_stiffness(material.parameters[1], area),
        );
        let y = f64::from(material.parameters[2] * area);
        let depth = f64::from(material.depth);
        Self {
            k,
            y,
            ey: y / k,
            eta: y * f64::from(CRUSH_TIME_CONSTANT) / depth,
            depth,
        }
    }
    fn energy(&self, x: f64, p: f64) -> f64 {
        let e = (x - p).max(0.0);
        0.5 * self.k * e * e
    }
    pub(super) fn segment(&self, x: f64, p: f64, next: f64, h: f64) -> Segment {
        let dx = next - x;
        let new_p = p.max(next - self.ey);
        let dp = new_p - p;
        let loss = if dp > 0.0 {
            self.eta * (dx / h) * dp
        } else {
            0.0
        };
        let e0 = (x - p).max(0.0);
        let e1 = (next - new_p).max(0.0);
        // Factored difference avoids subtracting nearly equal stored energies.
        let work = 0.5 * self.k * (e1 + e0) * (e1 - e0) + self.y * dp + loss;
        let force = if dp > 0.0 {
            self.y + self.eta * dx / h
        } else {
            self.k * e1
        };
        Segment {
            p: new_p,
            dp,
            elastic: self.energy(next, new_p),
            loss,
            work,
            force,
            mean: if dx != 0.0 { work / dx } else { force },
        }
    }
}
impl History {
    pub(super) fn new(scene: &Scene) -> Result<Self, R> {
        let mut h = Self {
            initial: scene.plates.material.clone(),
            mass: scene.mass,
            acceleration: scene.acceleration,
            initial_x: scene.plates.x,
            summary: DynamicCompressionSummary::default(),
        };
        h.summary = h.initial_summary();
        if scene.plates.speed != 0.0 || !h.valid() {
            return Err(R::UnsupportedScene);
        }
        Ok(h)
    }
    // Enrollment selects the immutable physical contract. Old overload and exact
    // safe-equilibrium histories retain revision 1, including their v15 bytes.
    pub(super) fn revision(&self) -> u32 {
        let c = MaterialCurve::new(&self.initial);
        if (self.initial_x == 0.0 && self.weight() > c.y)
            || (self.initial_x > 0.0 && self.weight() == c.k * self.initial_x)
        {
            1
        } else {
            2
        }
    }
    fn weight(&self) -> f64 {
        f64::from(self.mass) * f64::from(self.acceleration)
    }
    fn initial_summary(&self) -> DynamicCompressionSummary {
        let c = MaterialCurve::new(&self.initial);
        DynamicCompressionSummary {
            indentation: self.initial_x,
            elastic_energy: c.energy(self.initial_x, 0.0),
            scales: [
                c.y * c.depth,
                libm::sqrt(2.0 * f64::from(self.mass) * c.y * c.depth),
                c.depth,
            ],
            ..Default::default()
        }
    }
    pub(super) fn current_material(&self) -> StoredRegion {
        let mut m = self.initial.clone();
        m.remaining -= self.summary.damage_work;
        m
    }
    pub(super) fn valid(&self) -> bool {
        let c = MaterialCurve::new(&self.initial);
        let s = &self.summary;
        let start = self.initial_summary();
        let weight = self.weight();
        let rebound = self.revision() == 2;
        let near = |a: f64, b: f64, scale: f64| (a - b).abs() <= 1e-10 * scale;
        self.initial.region().is_ok()
            && self.initial.remaining > 0.0
            && (0.125..=8.0).contains(&self.initial.thickness)
            && (0.125..=1.0).contains(&self.initial.depth)
            && (5.0..=80.0).contains(&self.mass)
            && self.acceleration.is_finite()
            && (0.0..=1e6).contains(&self.acceleration)
            && (rebound || self.acceleration > 0.0)
            && c.k.is_finite()
            && c.k > 0.0
            && c.y.is_finite()
            && c.y > 0.0
            && c.ey > 0.0
            && c.ey < c.depth / 4.0
            && ((self.initial_x == 0.0 && weight > c.y && weight <= 2.0 * c.y)
                || (self.initial_x > 0.0
                    && self.initial_x <= c.ey
                    && weight == c.k * self.initial_x)
                || (rebound
                    && weight <= c.y
                    && (0.0..=c.ey).contains(&self.initial_x)
                    && (self.initial_x > 0.0 || weight > 0.0)))
            && [
                s.indentation,
                s.velocity,
                s.plastic_indentation,
                s.kinetic_energy,
                s.elastic_energy,
                s.gravity_work,
                s.damage_work,
                s.viscous_loss,
                s.charge_error,
                s.absolute_charge_error,
                s.reaction_impulse,
                s.integrated_velocity,
                s.elapsed,
            ]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1e12)
            && s.indentation
                >= if rebound {
                    -c.depth / 4.0
                } else {
                    self.initial_x
                }
            && s.indentation <= c.depth / 4.0
            && (if rebound { -64.0 } else { 0.0 }..=64.0).contains(&s.velocity)
            && f64::from(s.velocity as f32) == s.velocity
            && (0.0..=c.depth / 4.0 - c.ey).contains(&s.plastic_indentation)
            && s.indentation - s.plastic_indentation <= c.ey + 1e-14 * c.depth
            && s.damage_work >= 0.0
            && s.damage_work < self.initial.remaining
            && s.viscous_loss >= 0.0
            && s.reaction_impulse >= 0.0
            && s.scales == start.scales
            && s.scales.iter().all(|v| v.is_finite() && *v > 0.0)
            && near(
                s.kinetic_energy,
                0.5 * f64::from(self.mass) * s.velocity * s.velocity,
                s.scales[0],
            )
            && near(
                s.elastic_energy,
                c.energy(s.indentation, s.plastic_indentation),
                s.scales[0],
            )
            && near(
                s.gravity_work,
                weight * (s.indentation - self.initial_x),
                s.scales[0],
            )
            && near(
                s.charge_error,
                c.y * s.plastic_indentation - s.damage_work,
                s.scales[0],
            )
            && (0.0..=1e-5 * s.scales[0]).contains(&s.absolute_charge_error)
            && s.absolute_charge_error + 1e-10 * s.scales[0] >= s.charge_error.abs()
            && s.errors
                .iter()
                .zip(s.absolute_errors)
                .zip(s.scales)
                .all(|((e, a), scale)| {
                    e.is_finite()
                        && a.is_finite()
                        && a >= 0.0
                        && a <= 1e-5 * scale
                        && e.abs() <= a + 1e-10 * scale
                })
            && near(
                s.errors[0],
                s.gravity_work + start.elastic_energy
                    - s.kinetic_energy
                    - s.elastic_energy
                    - c.y * s.plastic_indentation
                    - s.viscous_loss,
                s.scales[0],
            )
            && near(
                s.errors[1],
                weight * s.elapsed - s.reaction_impulse - f64::from(self.mass) * s.velocity,
                s.scales[1],
            )
            && near(
                s.errors[2],
                s.indentation - self.initial_x - s.integrated_velocity,
                s.scales[2],
            )
            && (0.0..=3600.0).contains(&s.elapsed)
            && s.solves <= 1_000_000
            && ((s.solves == 0 && *s == start) || (s.solves > 0 && s.elapsed > 0.0))
            && (rebound
                || self.initial_x == 0.0
                || (s.indentation == self.initial_x
                    && s.velocity == 0.0
                    && s.plastic_indentation == 0.0))
    }
    pub(super) fn subdivision(&self, dt: f32) -> Result<(usize, f32), R> {
        let c = MaterialCurve::new(&self.initial);
        let bound = (1.0_f64 / 4096.0)
            .min(1.0 / (32.0 * libm::sqrt(c.k / f64::from(self.mass))))
            .min(f64::from(self.mass) / (32.0 * c.eta));
        let mut count = 1;
        while f64::from(dt) / (count as f64) > bound {
            count *= 2;
            if count > MAX_DYNAMIC_COMPRESSION_STEPS {
                return Err(R::Limit);
            }
        }
        let h = dt / (count as f32);
        if h <= 0.0 || f64::from(h) * (count as f64) != f64::from(dt) {
            return Err(R::Numerical);
        }
        Ok((count, h))
    }
    pub(super) fn displacement(&self, h: f64) -> Result<f64, R> {
        let s = &self.summary;
        let c = MaterialCurve::new(&self.initial);
        if self.revision() == 2 {
            return self.signed_displacement(h);
        }
        if self.initial_x > 0.0 {
            return Ok(0.0);
        }
        let mut lo = 0.0;
        let mut hi = s.velocity * h + 0.5 * f64::from(self.acceleration) * h * h;
        // Monotone reaction makes this a unique scalar root. Fixed iteration
        // count bounds work; no tolerance derived from remaining capacity.
        for _ in 0..64 {
            let dx = 0.5 * (lo + hi);
            let r = c
                .segment(s.indentation, s.plastic_indentation, s.indentation + dx, h)
                .mean;
            let residual =
                2.0 * f64::from(self.mass) * (dx / h - s.velocity) / h - self.weight() + r;
            if residual > 0.0 {
                hi = dx;
            } else {
                lo = dx;
            }
        }
        let dx = 0.5 * (lo + hi);
        if !dx.is_finite() || dx <= 0.0 {
            Err(R::Numerical)
        } else {
            Ok(dx)
        }
    }
    fn signed_displacement(&self, h: f64) -> Result<f64, R> {
        let s = &self.summary;
        let c = MaterialCurve::new(&self.initial);
        let residual = |dx: f64| {
            let reaction = c
                .segment(s.indentation, s.plastic_indentation, s.indentation + dx, h)
                .mean;
            2.0 * f64::from(self.mass) * (dx / h - s.velocity) / h - self.weight() + reaction
        };
        if residual(0.0) == 0.0 {
            return Ok(0.0);
        }
        let (mut lo, mut hi) = (-c.depth, c.depth);
        if !residual(lo).is_finite()
            || !residual(hi).is_finite()
            || residual(lo) >= 0.0
            || residual(hi) <= 0.0
        {
            return Err(R::Numerical);
        }
        for _ in 0..64 {
            let dx = 0.5 * (lo + hi);
            if residual(dx) > 0.0 {
                hi = dx;
            } else {
                lo = dx;
            }
        }
        let dx = 0.5 * (lo + hi);
        if dx.is_finite() {
            Ok(dx)
        } else {
            Err(R::Numerical)
        }
    }
    pub(super) fn finish(
        &mut self,
        scene: &Scene,
        x: f64,
        v: f32,
        h: f32,
        at: f64,
        owner: Option<EpisodeId>,
    ) -> Result<DynamicCompressionInterval, R> {
        let old = self.summary.clone();
        let c = MaterialCurve::new(&self.initial);
        let dt = f64::from(h);
        let dx = x - old.indentation;
        if !x.is_finite() || x.abs() > c.depth / 4.0 {
            return Err(R::Limit);
        }
        if !v.is_finite()
            || (self.revision() == 1
                && (v < 0.0 || dx < 0.0 || (self.initial_x == 0.0 && dx == 0.0)))
        {
            return Err(R::Numerical);
        }
        let q = c.segment(old.indentation, old.plastic_indentation, x, dt);
        let mut region = scene
            .plates
            .material
            .region()
            .map_err(|_| R::UnsupportedScene)?;
        let outcome = sustained(
            &region,
            SustainedLoad {
                force: q.force as f32,
                width: 2.0,
                section: self.initial.section(),
                duration: h,
                measured_plastic_displacement: Some(q.dp as f32),
            },
        )
        .map_err(|_| R::Numerical)?;
        if outcome.failed[0]
            || outcome.ledger.excess != 0.0
            || region.absorb(outcome.consumed[0]) != Ok(0.0)
            || region.failed()
        {
            return Err(R::FailedRegion);
        }
        let charge = scene.plates.material.remaining - region.remaining_precise();
        let charge_error = c.y * q.dp - charge;
        if (q.dp > 0.0 && (charge <= 0.0 || charge_error.abs() > 1e-5 * c.y * q.dp))
            || (q.dp == 0.0 && charge != 0.0)
        {
            return Err(R::Numerical);
        }
        let kinetic = 0.5 * f64::from(self.mass) * f64::from(v) * f64::from(v);
        let kinetic_change = kinetic - old.kinetic_energy;
        let impulse = q.mean * dt;
        let gravity_work = self.weight() * dx;
        let integrated = 0.5 * (old.velocity + f64::from(v)) * dt;
        let errors = [
            gravity_work - kinetic_change - q.work,
            self.weight() * dt - impulse - f64::from(self.mass) * (f64::from(v) - old.velocity),
            dx - integrated,
        ];
        let mut next = old.clone();
        next.indentation = x;
        next.velocity = f64::from(v);
        next.plastic_indentation = q.p;
        next.kinetic_energy = kinetic;
        next.elastic_energy = q.elastic;
        next.gravity_work = self.weight() * (x - self.initial_x);
        next.damage_work = self.initial.remaining - region.remaining_precise();
        next.viscous_loss = add(old.viscous_loss, q.loss)?;
        next.charge_error = c.y * q.p - next.damage_work;
        next.absolute_charge_error = add(old.absolute_charge_error, charge_error.abs())?;
        next.reaction_impulse = add(old.reaction_impulse, impulse)?;
        next.integrated_velocity = add(old.integrated_velocity, integrated)?;
        for (i, error) in errors.into_iter().enumerate() {
            next.errors[i] = add(old.errors[i], error)?;
            next.absolute_errors[i] = add(old.absolute_errors[i], error.abs())?;
        }
        next.elapsed = exact_time(old.elapsed, dt)?;
        next.solves = old.solves.checked_add(1).ok_or(R::Limit)?;
        self.summary = next;
        if !self.valid() {
            return Err(R::Numerical);
        }
        Ok(DynamicCompressionInterval {
            at,
            gravity_work,
            kinetic_change,
            incoming: old.velocity as f32,
            outgoing: v,
            errors,
            load: CompressionLoad {
                law: engine_material::LAW_VERSION,
                material_id: region.material().identity(),
                episode: owner,
                sources: scene.plates.sources,
                normal: Vec2::new(scene.plates.normal[0], scene.plates.normal[1]),
                duration: h,
                width: 2.0,
                section: self.initial.section(),
                displacement: dx,
                plastic_displacement: q.dp,
                force: q.force,
                impulse,
                elastic_before: old.elastic_energy,
                elastic_after: q.elastic,
                driver_work: q.work,
                damage_work: charge,
                viscous_loss: q.loss,
                charge_error,
            },
        })
    }
}
fn add(old: f64, change: f64) -> Result<f64, R> {
    let next = old + change;
    if !next.is_finite() || (change != 0.0 && next == old) {
        Err(R::Numerical)
    } else {
        Ok(next)
    }
}
fn exact_time(a: f64, b: f64) -> Result<f64, R> {
    let sum = add(a, b)?;
    let residual = if a >= b { (a - sum) + b } else { (b - sum) + a };
    if residual != 0.0 {
        Err(R::Numerical)
    } else {
        Ok(sum)
    }
}

#[cfg(test)]
pub(super) fn frozen_v15_history(h: &History) -> Vec<u8> {
    let m = &h.initial;
    let s = &h.summary;
    let mut bytes = bincode::serialize(&(
        (
            m.parameters,
            m.brittle,
            m.volume,
            m.remaining,
            m.thickness,
            m.depth,
        ),
        h.mass,
        h.acceleration,
        h.initial_x,
    ))
    .unwrap();
    // Field order is independent of the current History/Summary serializers.
    bytes.extend(
        bincode::serialize(&(
            (
                s.indentation,
                s.velocity,
                s.plastic_indentation,
                s.kinetic_energy,
                s.elastic_energy,
                s.gravity_work,
                s.damage_work,
                s.viscous_loss,
                s.charge_error,
                s.absolute_charge_error,
                s.reaction_impulse,
                s.integrated_velocity,
            ),
            s.errors,
            s.absolute_errors,
            s.scales,
            s.elapsed,
            s.solves,
        ))
        .unwrap(),
    );
    bytes
}
