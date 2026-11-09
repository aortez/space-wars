//! Two material layers in series, coupled by a discrete work reaction.
use super::*;
use model::{Segment, add};

/// Retained state of one layer, in incoming-body/support order in the pair API.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialCompressionSummary {
    pub material_id: u64,
    /// Signed layer displacement. Negative elastic displacement represents its
    /// share of the open gap and stores no energy.
    pub indentation: f64,
    pub plastic_indentation: f64,
    pub elastic_energy: f64,
    pub damage_work: f64,
    pub viscous_loss: f64,
    pub remaining: f64,
    pub charge_error: f64,
    pub absolute_charge_error: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Layer {
    x: f64,
    p: f64,
    loss: f64,
    remaining: f64,
    absolute_charge_error: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct PairedHistory {
    initial: [StoredRegion; 2],
    layers: [Layer; 2],
}

struct Allocation {
    x: [f64; 2],
    layers: [Segment; 2],
    total: Segment,
}

impl PairedHistory {
    pub fn new(world: &PhysicsWorld, scene: &Scene) -> Result<Option<Self>, R> {
        let materials = &world
            .loading
            .as_ref()
            .unwrap()
            .response
            .as_ref()
            .unwrap()
            .materials;
        let Some(driver) = materials.get(&scene.plates.sources[0].region) else {
            return Ok(None);
        };
        if world.body_kind(scene.plates.bodies[1]) != Some(BodyKind::Dynamic) {
            return Err(R::UnsupportedScene);
        }
        if driver.remaining <= 0.0 {
            return Err(R::FailedRegion);
        }
        let initial = [driver.clone(), scene.plates.material.clone()];
        let curves = initial.each_ref().map(MaterialCurve::new);
        let fraction = curves[1].k / (curves[0].k + curves[1].k);
        let x0 = scene.plates.x * fraction;
        Ok(Some(Self {
            layers: std::array::from_fn(|i| Layer {
                x: if i == 0 { x0 } else { scene.plates.x - x0 },
                p: 0.0,
                loss: 0.0,
                remaining: initial[i].remaining,
                absolute_charge_error: 0.0,
            }),
            initial,
        }))
    }

    /// Equivalent elastic stiffness and conservative fixed scales. The reaction
    /// itself is solved from both curves, never from this equivalent curve.
    pub fn curve(&self) -> MaterialCurve {
        let [a, b] = self.initial.each_ref().map(MaterialCurve::new);
        let k = a.k * b.k / (a.k + b.k);
        let y = a.y.min(b.y);
        MaterialCurve {
            k,
            y,
            ey: y / k,
            eta: a.eta.max(b.eta),
            depth: a.depth.min(b.depth),
        }
    }

    pub fn material(&self, i: usize) -> StoredRegion {
        let mut m = self.initial[i].clone();
        m.remaining = self.layers[i].remaining;
        m
    }

    pub fn summaries(&self) -> [MaterialCompressionSummary; 2] {
        std::array::from_fn(|i| {
            let c = MaterialCurve::new(&self.initial[i]);
            let l = &self.layers[i];
            let damage = self.initial[i].remaining - l.remaining;
            MaterialCompressionSummary {
                material_id: self.initial[i].region().unwrap().material().identity(),
                indentation: l.x,
                plastic_indentation: l.p,
                elastic_energy: c.energy(l.x, l.p),
                damage_work: damage,
                viscous_loss: l.loss,
                remaining: l.remaining,
                charge_error: c.y * l.p - damage,
                absolute_charge_error: l.absolute_charge_error,
            }
        })
    }

    pub fn plastic_work(&self) -> f64 {
        (0..2)
            .map(|i| MaterialCurve::new(&self.initial[i]).y * self.layers[i].p)
            .sum()
    }

    fn allocation(&self, next: f64, h: f64) -> Result<Allocation, R> {
        let c = self.initial.each_ref().map(MaterialCurve::new);
        let evaluate = |x: [f64; 2]| {
            let q =
                std::array::from_fn(|i| c[i].segment(self.layers[i].x, self.layers[i].p, x[i], h));
            (x, q)
        };
        let p: f64 = self.layers.iter().map(|l| l.p).sum();
        let old: f64 = self.layers.iter().map(|l| l.x).sum();
        // At separation both layers must release their stored energy. The
        // terminal interval uses their total work over the measured relative
        // displacement; extrapolating two independent secants into the gap
        // could otherwise leave one spring compressed after contact ends.
        let (x, q) = if next <= p {
            let fraction = c[1].k / (c[0].k + c[1].k);
            evaluate([
                self.layers[0].p + (next - p) * fraction,
                self.layers[1].p + (next - p) * (1.0 - fraction),
            ])
        } else {
            // Each layer's mean work force is monotone in its displacement.
            // Equal forces and complementary displacements share one finite
            // work budget, including elastic work transferred between layers.
            let bound = 4.0 * c[0].depth.max(c[1].depth);
            let (mut lo, mut hi) = (-bound, bound);
            let residual = |x0| {
                let (_, q): (_, [Segment; 2]) = evaluate([x0, next - x0]);
                q[0].mean - q[1].mean
            };
            if !(residual(lo) <= 0.0 && residual(hi) >= 0.0) {
                return Err(R::Numerical);
            }
            for _ in 0..64 {
                let mid = 0.5 * (lo + hi);
                if residual(mid) > 0.0 {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let x0 = 0.5 * (lo + hi);
            evaluate([x0, next - x0])
        };
        let work = q[0].work + q[1].work;
        let mean = if next <= p && next != old {
            work / (next - old)
        } else {
            0.5 * (q[0].mean + q[1].mean)
        };
        if !mean.is_finite()
            || mean < 0.0
            || (work - mean * (next - old)).abs() > 1e-10 * self.curve().y * self.curve().depth
        {
            return Err(R::Numerical);
        }
        Ok(Allocation {
            x,
            total: Segment {
                p: q[0].p + q[1].p,
                dp: q[0].dp + q[1].dp,
                elastic: q[0].elastic + q[1].elastic,
                loss: q[0].loss + q[1].loss,
                work,
                force: mean,
                mean,
            },
            layers: q,
        })
    }

    pub fn segment(&self, next: f64, h: f64) -> Result<Segment, R> {
        Ok(self.allocation(next, h)?.total)
    }

    pub fn finish(
        &mut self,
        scene: &Scene,
        x: f64,
        h: f32,
        owner: Option<EpisodeId>,
    ) -> Result<([CompressionLoad; 2], Segment), R> {
        let a = self.allocation(x, f64::from(h))?;
        let mut loads = Vec::with_capacity(2);
        for i in 0..2 {
            let c = MaterialCurve::new(&self.initial[i]);
            let before = self.material(i);
            let q = &a.layers[i];
            let (remaining, charge, charge_error) = model::charge(&before, q, h)?;
            loads.push(CompressionLoad {
                law: engine_material::LAW_VERSION,
                material_id: before
                    .region()
                    .map_err(|_| R::UnsupportedScene)?
                    .material()
                    .identity(),
                episode: owner,
                sources: scene.plates.sources,
                normal: Vec2::new(scene.plates.normal[0], scene.plates.normal[1]),
                duration: h,
                width: 2.0,
                section: before.section(),
                displacement: a.x[i] - self.layers[i].x,
                plastic_displacement: q.dp,
                force: q.force,
                impulse: a.total.mean * f64::from(h),
                elastic_before: c.energy(self.layers[i].x, self.layers[i].p),
                elastic_after: q.elastic,
                driver_work: q.work,
                damage_work: charge,
                viscous_loss: q.loss,
                charge_error,
            });
            self.layers[i] = Layer {
                x: a.x[i],
                p: q.p,
                remaining,
                loss: add(self.layers[i].loss, q.loss)?,
                absolute_charge_error: add(
                    self.layers[i].absolute_charge_error,
                    charge_error.abs(),
                )?,
            };
        }
        Ok((loads.try_into().unwrap(), a.total))
    }

    pub fn valid(&self, h: &History) -> bool {
        let near = |a: f64, b: f64, scale: f64| (a - b).abs() <= 1e-10 * scale;
        let c = self.curve();
        if self.initial[1] != h.initial
            || h.moving.is_none()
            || h.loads.is_some()
            || h.incoming.is_some()
        {
            return false;
        }
        for i in 0..2 {
            let m = &self.initial[i];
            let l = &self.layers[i];
            let c = MaterialCurve::new(m);
            if m.region().is_err()
                || m.remaining <= 0.0
                || !(0.125..=8.0).contains(&m.thickness)
                || !(0.125..=1.0).contains(&m.depth)
                || !c.k.is_finite()
                || !c.y.is_finite()
                || c.y <= 0.0
                || c.ey <= 0.0
                || c.ey >= c.depth / 4.0
                || !l.x.is_finite()
                || l.x.abs() > c.depth / 4.0
                || !(0.0..=c.depth / 4.0 - c.ey).contains(&l.p)
                || l.x - l.p > c.ey + 1e-14 * c.depth
                || (h.summary.indentation >= h.summary.plastic_indentation
                    && l.x < l.p - 1e-14 * c.depth)
                || !l.loss.is_finite()
                || l.loss < 0.0
                || !(0.0..=m.remaining).contains(&l.remaining)
                || l.remaining == 0.0
                || !(0.0..=1e-5 * c.y * c.depth).contains(&l.absolute_charge_error)
                || (c.y * l.p - (m.remaining - l.remaining)).abs()
                    > l.absolute_charge_error + 1e-10 * c.y * c.depth
            {
                return false;
            }
        }
        let s = self.summaries();
        near(
            s[0].indentation + s[1].indentation,
            h.summary.indentation,
            c.depth,
        ) && near(
            s[0].plastic_indentation + s[1].plastic_indentation,
            h.summary.plastic_indentation,
            c.depth,
        ) && near(
            s[0].elastic_energy + s[1].elastic_energy,
            h.summary.elastic_energy,
            c.y * c.depth,
        ) && near(
            s[0].damage_work + s[1].damage_work,
            h.summary.damage_work,
            c.y * c.depth,
        ) && near(
            s[0].viscous_loss + s[1].viscous_loss,
            h.summary.viscous_loss,
            c.y * c.depth,
        ) && near(
            s[0].absolute_charge_error + s[1].absolute_charge_error,
            h.summary.absolute_charge_error,
            c.y * c.depth,
        ) && (h.summary.indentation > h.summary.plastic_indentation
            || h.summary.elastic_energy == 0.0)
            && (h.summary.solves != 0
                || self
                    .layers
                    .iter()
                    .all(|l| l.p == 0.0 && l.loss == 0.0 && l.absolute_charge_error == 0.0))
    }
}

impl PhysicsWorld {
    /// Per-material histories in incoming-body/support order, retained even
    /// after invalidation. Motion and total work use the existing summaries.
    pub fn material_paired_compression_summary(&self) -> Option<[MaterialCompressionSummary; 2]> {
        Some(
            self.loading
                .as_ref()?
                .dynamic_compression
                .as_ref()?
                .history
                .paired
                .as_ref()?
                .summaries(),
        )
    }
}

#[cfg(test)]
#[path = "tests/paired.rs"]
mod tests;
