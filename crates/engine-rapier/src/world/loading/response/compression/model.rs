//! Closed-form work integral for one monotone, measured platen segment.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct History {
    initial: StoredRegion,
    pub(super) summary: CompressionSummary,
}

impl History {
    pub(super) fn new(initial: StoredRegion) -> Result<Self, R> {
        let value = Self {
            initial,
            summary: CompressionSummary::default(),
        };
        if !value.valid() {
            return Err(R::UnsupportedScene);
        }
        Ok(value)
    }
    fn constants(&self) -> (f64, f64, f64, f64) {
        let area = 2.0 * self.initial.thickness;
        let k = f64::from(
            self.initial
                .section()
                .contact_stiffness(self.initial.parameters[1], area),
        );
        let y = f64::from(self.initial.parameters[2] * area);
        let depth = f64::from(self.initial.depth);
        (k, y, y / k, y * f64::from(CRUSH_TIME_CONSTANT) / depth)
    }
    pub(super) fn current_material(&self) -> StoredRegion {
        let mut material = self.initial.clone();
        material.remaining -= self.summary.damage_work;
        material
    }
    pub(super) fn valid(&self) -> bool {
        let h = &self.summary;
        let (k, y, ey, _) = self.constants();
        let depth = f64::from(self.initial.depth);
        let energy_scale = y * depth;
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-10 * energy_scale;
        self.initial.region().is_ok()
            && self.initial.remaining > 0.0
            && (0.125..=8.0).contains(&self.initial.thickness)
            && (0.125..=1.0).contains(&self.initial.depth)
            && k.is_finite()
            && k > 0.0
            && y.is_finite()
            && y > 0.0
            && ey > 0.0
            && ey < depth / 4.0
            && [
                h.indentation,
                h.plastic_indentation,
                h.elastic_energy,
                h.driver_work,
                h.damage_work,
                h.viscous_loss,
                h.charge_error,
                h.absolute_charge_error,
                h.absolute_motion_error,
                h.elapsed,
            ]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1e12)
            && h.indentation.abs() <= depth / 4.0
            && (0.0..=depth / 4.0 - ey).contains(&h.plastic_indentation)
            && h.indentation - h.plastic_indentation <= ey + 1e-14 * depth
            && h.damage_work >= 0.0
            && h.damage_work < self.initial.remaining
            && h.viscous_loss >= 0.0
            && h.elastic_energy >= 0.0
            && close(
                h.elastic_energy,
                elastic_energy(k, h.indentation, h.plastic_indentation),
            )
            && close(h.charge_error, y * h.plastic_indentation - h.damage_work)
            && close(
                h.driver_work,
                h.elastic_energy + h.damage_work + h.charge_error + h.viscous_loss,
            )
            && h.absolute_charge_error >= h.charge_error.abs() - 1e-10 * energy_scale
            && (0.0..=1e-5 * energy_scale).contains(&h.absolute_charge_error)
            && (0.0..=1e-5 * depth).contains(&h.absolute_motion_error)
            && (0.0..=3600.0).contains(&h.elapsed)
            && h.steps <= 1_000_000
            && ((h.steps == 0 && *h == CompressionSummary::default())
                || (h.steps > 0 && h.elapsed > 0.0))
    }

    pub(super) fn advance(
        &mut self,
        scene: &Scene,
        x: f64,
        dt: f32,
        speed: f32,
    ) -> Result<CompressionLoad, R> {
        let (k, y, ey, eta) = self.constants();
        let old = self.summary.clone();
        let dx = x - old.indentation;
        let command = f64::from(speed) * f64::from(dt);
        if !x.is_finite()
            || (speed != 0.0 && (dx == 0.0 || dx.signum() != command.signum()))
            || (speed == 0.0 && dx != 0.0)
        {
            return Err(R::Numerical);
        }
        if x.abs() > f64::from(self.initial.depth) / 4.0 {
            return Err(R::Limit);
        }
        let p = old.plastic_indentation.max(x - ey);
        let dp = p - old.plastic_indentation;
        let elastic = elastic_energy(k, x, p);
        let force = if dp > 0.0 {
            y + eta * dx / f64::from(dt)
        } else {
            k * (x - p).max(0.0)
        };
        let loss = if dp > 0.0 {
            eta * dx / f64::from(dt) * dp
        } else {
            0.0
        };
        let physical_damage = y * dp;
        let work = elastic - old.elastic_energy + physical_damage + loss;
        let impulse = if dx != 0.0 {
            work / dx * f64::from(dt)
        } else {
            force * f64::from(dt)
        };
        let mut material = scene.material.region().map_err(|_| R::UnsupportedScene)?;
        if material.failed() {
            return Err(R::FailedRegion);
        }
        let outcome = sustained(
            &material,
            SustainedLoad {
                force: force as f32,
                width: 2.0,
                section: scene.material.section(),
                duration: dt,
                measured_plastic_displacement: Some(dp as f32),
            },
        )
        .map_err(|_| R::Numerical)?;
        if outcome.failed[0] || outcome.ledger.excess != 0.0 {
            return Err(R::FailedRegion);
        }
        if material.absorb(outcome.consumed[0]) != Ok(0.0) || material.failed() {
            return Err(R::FailedRegion);
        }
        let charge = scene.material.remaining - material.remaining_precise();
        let error = physical_damage - charge;
        // Catch below-ulp motion/force/capacity charges even when the cumulative
        // absolute allowance is still large. Never accept a silently lost charge.
        if (dp > 0.0 && (charge <= 0.0 || error.abs() > 1e-5 * physical_damage))
            || (dp == 0.0 && charge != 0.0)
            || !impulse.is_finite()
            || impulse < 0.0
        {
            return Err(R::Numerical);
        }
        self.summary = CompressionSummary {
            indentation: x,
            plastic_indentation: p,
            elastic_energy: elastic,
            driver_work: add(old.driver_work, work)?,
            damage_work: self.initial.remaining - material.remaining_precise(),
            viscous_loss: add(old.viscous_loss, loss)?,
            charge_error: y * p - (self.initial.remaining - material.remaining_precise()),
            absolute_charge_error: add(old.absolute_charge_error, error.abs())?,
            absolute_motion_error: add(old.absolute_motion_error, (dx - command).abs())?,
            elapsed: add(old.elapsed, f64::from(dt))?,
            steps: old.steps.checked_add(1).ok_or(R::Limit)?,
        };
        if !self.valid() {
            return Err(R::Numerical);
        }
        Ok(CompressionLoad {
            law: engine_material::LAW_VERSION,
            material_id: material.material().identity(),
            episode: None,
            sources: scene.sources,
            normal: Vec2::new(scene.normal[0], scene.normal[1]),
            duration: dt,
            width: 2.0,
            section: scene.material.section(),
            displacement: dx,
            plastic_displacement: dp,
            force,
            impulse,
            elastic_before: old.elastic_energy,
            elastic_after: elastic,
            driver_work: work,
            damage_work: charge,
            viscous_loss: loss,
            charge_error: error,
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

fn elastic_energy(stiffness: f64, x: f64, plastic: f64) -> f64 {
    let strain = (x - plastic).max(0.0);
    0.5 * stiffness * strain * strain
}

#[cfg(test)]
pub(super) fn frozen_v14_history(h: &History) -> Vec<u8> {
    let m = &h.initial;
    let s = &h.summary;
    bincode::serialize(&(
        (
            m.parameters,
            m.brittle,
            m.volume,
            m.remaining,
            m.thickness,
            m.depth,
        ),
        (
            s.indentation,
            s.plastic_indentation,
            s.elastic_energy,
            s.driver_work,
            s.damage_work,
            s.viscous_loss,
            s.charge_error,
            s.absolute_charge_error,
            s.absolute_motion_error,
            s.elapsed,
            s.steps,
        ),
    ))
    .unwrap()
}
