//! Discrete-work reaction; no continuous-solution oracle is used here.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct History {
    pub(super) initial: StoredRegion,
    pub(super) mass: f32,
    pub(super) acceleration: f32,
    pub(super) initial_x: f64,
    // Written in tagged v17/v18 extensions; v15/v16 history bytes stay fixed.
    #[serde(skip)]
    pub(super) incoming: Option<IncomingMotion>,
    // Required only in the tagged v18 extension; earlier history bytes stay fixed.
    #[serde(skip)]
    pub(super) loads: Option<Vec<loads::LoadChange>>,
    // Required fixed-size v19 extension. Earlier history layouts are unchanged.
    #[serde(skip)]
    pub(super) moving: Option<moving::MovingHistory>,
    #[serde(skip)]
    pub(super) paired: Option<paired::PairedHistory>,
    // Required v21 extension, including the explicit collapse policy.
    #[serde(skip)]
    pub(super) failure: Option<failure::FailureHistory>,
    pub(super) summary: DynamicCompressionSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct IncomingMotion {
    pub(super) velocity: f32,
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
    pub(super) fn energy(&self, x: f64, p: f64) -> f64 {
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
    pub(super) fn new(scene: &Scene, world: &PhysicsWorld) -> Result<Self, R> {
        let moving = moving::MovingHistory::new(world, scene)?;
        let mut h = Self {
            initial: scene.plates.material.clone(),
            mass: scene.mass,
            acceleration: scene.acceleration,
            initial_x: scene.plates.x,
            incoming: (moving.is_none() && (scene.plates.x < 0.0 || scene.plates.speed != 0.0))
                .then_some(IncomingMotion {
                    velocity: scene.plates.speed,
                }),
            loads: None,
            moving,
            paired: paired::PairedHistory::new(world, scene)?,
            failure: None,
            summary: DynamicCompressionSummary::default(),
        };
        h.summary = h.initial_summary();
        if !h.valid() {
            return Err(R::UnsupportedScene);
        }
        Ok(h)
    }
    // Retained policy selects the format. Legacy overload and exact
    // safe-equilibrium histories retain revision 1/v15 bytes.
    pub(super) fn revision(&self) -> u32 {
        if self.failure.is_some() {
            return 7;
        }
        if self.paired.is_some() {
            return 6;
        }
        if self.moving.is_some() {
            return 5;
        }
        if self.loads.is_some() {
            return 4;
        }
        if self.incoming.is_some() {
            return 3;
        }
        let c = self.curve();
        if (self.initial_x == 0.0 && self.weight() > c.y)
            || (self.initial_x > 0.0 && self.weight() == c.k * self.initial_x)
        {
            1
        } else {
            2
        }
    }
    fn initial_velocity(&self) -> f64 {
        if let Some(moving) = &self.moving {
            return moving.initial.relative_velocity();
        }
        self.incoming
            .as_ref()
            .map_or(0.0, |v| f64::from(v.velocity))
    }
    fn weight(&self) -> f64 {
        f64::from(self.mass) * f64::from(self.current_acceleration())
    }
    pub(super) fn effective_mass(&self) -> f64 {
        self.moving
            .as_ref()
            .map_or(f64::from(self.mass), |m| m.effective_mass())
    }
    pub(super) fn curve(&self) -> MaterialCurve {
        self.paired
            .as_ref()
            .map_or_else(|| MaterialCurve::new(&self.initial), |p| p.curve())
    }
    pub(super) fn plastic_work(&self) -> f64 {
        self.paired.as_ref().map_or_else(
            || self.curve().y * self.summary.plastic_indentation,
            |p| p.plastic_work(),
        )
    }
    pub(super) fn segment(&self, x: f64, h: f64) -> Result<Segment, R> {
        if self.collapsed() {
            return Ok(Segment {
                p: self.summary.plastic_indentation,
                dp: 0.0,
                elastic: 0.0,
                loss: 0.0,
                work: 0.0,
                force: 0.0,
                mean: 0.0,
            });
        }
        if let Some(p) = &self.paired {
            return p.segment(x, h);
        }
        Ok(self.curve().segment(
            self.summary.indentation,
            self.summary.plastic_indentation,
            x,
            h,
        ))
    }
    fn initial_summary(&self) -> DynamicCompressionSummary {
        let c = self.curve();
        DynamicCompressionSummary {
            indentation: self.initial_x,
            velocity: self.initial_velocity(),
            kinetic_energy: 0.5
                * self.effective_mass()
                * self.initial_velocity()
                * self.initial_velocity(),
            elastic_energy: c.energy(self.initial_x, 0.0),
            scales: [
                c.y * c.depth,
                libm::sqrt(2.0 * self.effective_mass() * c.y * c.depth),
                c.depth,
            ],
            ..Default::default()
        }
    }
    pub(super) fn current_material(&self) -> StoredRegion {
        if let Some(p) = &self.paired {
            return p.material(1);
        }
        let mut m = self.initial.clone();
        m.remaining -= self.summary.damage_work;
        m
    }
    pub(super) fn valid(&self) -> bool {
        let c = self.curve();
        let s = &self.summary;
        let start = self.initial_summary();
        let weight = f64::from(self.mass) * f64::from(self.acceleration);
        let rebound = self.revision() >= 2;
        let falling = self.incoming.is_some() || self.moving.is_some();
        let v0 = self.initial_velocity();
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
            && (self.paired.is_some() || c.ey < c.depth / 4.0)
            && self.valid_loads()
            && self.failure.as_ref().is_none_or(|f| f.valid(self))
            && self.paired.as_ref().is_none_or(|p| p.valid(self))
            && self.moving.as_ref().is_none_or(|m| m.valid(self))
            && (if falling {
                (-c.depth / 4.0..=0.0).contains(&self.initial_x)
                    && (0.0..=64.0).contains(&v0)
                    && weight <= 2.0 * c.y
                    && ((self.initial_x < 0.0 && (v0 > 0.0 || weight > 0.0))
                        || (self.initial_x == 0.0 && v0 > 0.0))
            } else {
                (self.initial_x == 0.0 && weight > c.y && weight <= 2.0 * c.y)
                    || (self.initial_x > 0.0
                        && self.initial_x <= c.ey
                        && weight == c.k * self.initial_x)
                    || (rebound
                        && weight <= c.y
                        && (0.0..=c.ey).contains(&self.initial_x)
                        && (self.initial_x > 0.0 || weight > 0.0))
            })
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
            && (self.moving.is_some() || f64::from(s.velocity as f32) == s.velocity)
            && (if self.paired.is_some() {
                (0.0..=c.depth / 4.0).contains(&s.plastic_indentation)
            } else {
                (0.0..=c.depth / 4.0 - c.ey).contains(&s.plastic_indentation)
                    && (self.collapsed()
                        || s.indentation - s.plastic_indentation <= c.ey + 1e-14 * c.depth)
            })
            && s.damage_work >= 0.0
            && (self.paired.is_some() || self.collapsed() || s.damage_work < self.initial.remaining)
            && s.viscous_loss >= 0.0
            && s.reaction_impulse >= 0.0
            && s.scales == start.scales
            && s.scales.iter().all(|v| v.is_finite() && *v > 0.0)
            && near(
                s.kinetic_energy,
                0.5 * self.effective_mass() * s.velocity * s.velocity,
                s.scales[0],
            )
            && (self.paired.is_some()
                || self.collapsed()
                || near(
                    s.elastic_energy,
                    c.energy(s.indentation, s.plastic_indentation),
                    s.scales[0],
                ))
            && near(
                s.gravity_work,
                self.gravity_totals(s.indentation, s.elapsed)[0],
                s.scales[0],
            )
            && near(
                s.charge_error,
                self.plastic_work() - s.damage_work - self.failure_excess(),
                s.scales[0],
            )
            && (0.0..=1e-5 * s.scales[0]).contains(&s.absolute_charge_error)
            && s.absolute_charge_error + 1e-10 * s.scales[0] >= s.charge_error.abs()
            && s.errors
                .iter()
                .zip(s.absolute_errors)
                .zip(s.scales)
                .enumerate()
                .all(|(i, ((e, a), scale))| {
                    // A free pair has two measured position trajectories, each
                    // checked against its own fixed allowance in MovingHistory.
                    let allowance = if i == 2 && self.moving.is_some() {
                        2e-5
                    } else {
                        1e-5
                    };
                    e.is_finite()
                        && a.is_finite()
                        && a >= 0.0
                        && a <= allowance * scale
                        && e.abs() <= a + 1e-10 * scale
                })
            && near(
                s.errors[0],
                s.gravity_work + start.elastic_energy + start.kinetic_energy
                    - s.kinetic_energy
                    - s.elastic_energy
                    - self.plastic_work()
                    - s.viscous_loss
                    - self.released_elastic(),
                s.scales[0],
            )
            && near(
                s.errors[1],
                self.gravity_totals(s.indentation, s.elapsed)[1]
                    - s.reaction_impulse
                    - self.effective_mass() * (s.velocity - v0),
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
        let c = self.curve();
        let bound = (1.0_f64 / 4096.0)
            .min(1.0 / (32.0 * libm::sqrt(c.k / self.effective_mass())))
            .min(self.effective_mass() / (32.0 * c.eta));
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
        let c = self.curve();
        if self.revision() >= 2 {
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
                2.0 * self.effective_mass() * (dx / h - s.velocity) / h - self.weight() + r;
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
        let c = self.curve();
        let residual = |dx: f64| {
            let reaction = self.segment(s.indentation + dx, h)?.mean;
            Ok::<_, R>(
                2.0 * self.effective_mass() * (dx / h - s.velocity) / h - self.weight() + reaction,
            )
        };
        if residual(0.0)? == 0.0 {
            return Ok(0.0);
        }
        let (mut lo, mut hi) = (-c.depth, c.depth);
        if !residual(lo)?.is_finite()
            || !residual(hi)?.is_finite()
            || residual(lo)? >= 0.0
            || residual(hi)? <= 0.0
        {
            return Err(R::Numerical);
        }
        for _ in 0..64 {
            let dx = 0.5 * (lo + hi);
            if residual(dx)? > 0.0 {
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
        v: f64,
        h: f32,
        at: f64,
        owner: Option<EpisodeId>,
    ) -> Result<DynamicCompressionInterval, R> {
        let old = self.summary.clone();
        let c = self.curve();
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
        let was_collapsed = self.collapsed();
        let (mut q, loads) = if let Some(paired) = &mut self.paired {
            let (loads, q) = paired.finish(scene, x, h, owner)?;
            (q, Some(loads))
        } else {
            (self.segment(x, dt)?, None)
        };
        let (remaining, charge, charge_error, excess) = if let Some(loads) = &loads {
            (
                0.0,
                loads[0].damage_work + loads[1].damage_work,
                loads[0].charge_error + loads[1].charge_error,
                0.0,
            )
        } else if was_collapsed {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            charge_with_policy(&scene.plates.material, &q, h, self.failure.is_some())?
        };
        let mut failure_dissipated = 0.0;
        if !was_collapsed
            && remaining == 0.0
            && let Some(f) = &mut self.failure
        {
            let event = CompressionFailure {
                elapsed: exact_time(old.elapsed, dt)?,
                source: f.source,
                episode: owner.ok_or(R::Numerical)?,
                indentation: x,
                velocity: v,
                plastic_indentation: q.p,
                released_elastic: q.elastic,
                excess_work: excess,
            };
            failure_dissipated = event.dissipated();
            f.event = Some(event);
            q.elastic = 0.0;
            q.force = 0.0;
        }
        let kinetic = 0.5 * self.effective_mass() * v * v;
        let kinetic_change = kinetic - old.kinetic_energy;
        let impulse = q.mean * dt;
        let gravity_work = self.weight() * dx;
        let integrated = 0.5 * (old.velocity + v) * dt;
        let errors = [
            gravity_work - kinetic_change - q.work,
            self.weight() * dt - impulse - self.effective_mass() * (v - old.velocity),
            dx - integrated,
        ];
        let mut next = old.clone();
        next.indentation = x;
        next.velocity = v;
        next.plastic_indentation = q.p;
        next.kinetic_energy = kinetic;
        next.elastic_energy = q.elastic;
        next.gravity_work = self.gravity_totals(x, old.elapsed)[0];
        next.damage_work = if let Some(p) = &self.paired {
            p.summaries().iter().map(|s| s.damage_work).sum()
        } else {
            self.initial.remaining - remaining
        };
        next.viscous_loss = add(old.viscous_loss, q.loss)?;
        next.charge_error = self.paired.as_ref().map_or(c.y * q.p, |p| p.plastic_work())
            - next.damage_work
            - self.failure_excess();
        next.absolute_charge_error = if let Some(p) = &self.paired {
            p.summaries().iter().map(|s| s.absolute_charge_error).sum()
        } else {
            add(old.absolute_charge_error, charge_error.abs())?
        };
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
        let (other_load, load) = if let Some([other, load]) = loads {
            (Some(other), load)
        } else {
            (
                None,
                CompressionLoad {
                    law: engine_material::LAW_VERSION,
                    material_id: scene
                        .plates
                        .material
                        .region()
                        .map_err(|_| R::UnsupportedScene)?
                        .material()
                        .identity(),
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
            )
        };
        Ok(DynamicCompressionInterval {
            other_load,
            at,
            gravity_work,
            kinetic_change,
            incoming: old.velocity as f32,
            outgoing: v as f32,
            errors,
            load,
            failure_dissipated,
        })
    }
}
pub(super) fn charge(material: &StoredRegion, q: &Segment, h: f32) -> Result<(f64, f64, f64), R> {
    let (remaining, charge, error, _) = charge_with_policy(material, q, h, false)?;
    Ok((remaining, charge, error))
}
fn charge_with_policy(
    material: &StoredRegion,
    q: &Segment,
    h: f32,
    collapse: bool,
) -> Result<(f64, f64, f64, f64), R> {
    let c = MaterialCurve::new(material);
    let mut region = material.region().map_err(|_| R::UnsupportedScene)?;
    let outcome = sustained(
        &region,
        SustainedLoad {
            force: q.force as f32,
            width: 2.0,
            section: material.section(),
            duration: h,
            measured_plastic_displacement: Some(q.dp as f32),
        },
    )
    .map_err(|_| R::Numerical)?;
    if !collapse && (outcome.failed[0] || outcome.ledger.excess != 0.0) {
        return Err(R::FailedRegion);
    }
    let remainder = region
        .absorb(outcome.consumed[0])
        .map_err(|_| R::Numerical)?;
    if !collapse && (remainder != 0.0 || region.failed()) {
        return Err(R::FailedRegion);
    }
    let charge = material.remaining - region.remaining_precise();
    let excess = if region.failed() {
        (c.y * q.dp - charge).max(0.0)
    } else {
        0.0
    };
    let error = c.y * q.dp - charge - excess;
    if (q.dp > 0.0 && (charge <= 0.0 || error.abs() > 1e-5 * c.y * q.dp))
        || (q.dp == 0.0 && charge != 0.0)
    {
        return Err(R::Numerical);
    }
    Ok((region.remaining_precise(), charge, error, excess))
}
pub(super) fn add(old: f64, change: f64) -> Result<f64, R> {
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
