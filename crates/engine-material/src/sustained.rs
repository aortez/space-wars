//! Sustained contact loads and structural connection loads. A load below the
//! safe limit charges nothing however long it rests. An overload yields and
//! charges plateau work over the plastic displacement the host measured, or
//! over a documented bounded fallback when it has no measurement. The
//! fallback is integrated exactly over each call, so the fixed step and the
//! substep count cannot change what a constant load does by a given time.
//! Sustained loads are evaluated per call, so only the plateau work
//! accumulates; the strain energy they report persists between calls and is
//! not a charge.

use crate::{Connection, MaterialError, Outcome, Region, Section};
use serde::Serialize;

/// Fallback crush progression, seconds. Without a host measurement, plastic
/// strain advances at `(r - 1) / CRUSH_TIME_CONSTANT` per second for an
/// overload ratio `r`, so intact material under twice its safe stress starts
/// to flow at one unit of strain per this time. The strain is multiplied by
/// the geometric scale of each load mode: the deformation depth, the
/// connection length, or `2 * length / height` for rotation.
pub const CRUSH_TIME_CONSTANT: f32 = 0.25;

fn duration_ok(duration: f32) -> bool {
    duration.is_finite() && (0.0..=3600.0).contains(&duration)
}

fn measured_ok(measured: Option<f32>) -> bool {
    measured.is_none_or(|value| value.is_finite() && value >= 0.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SustainedLoad {
    /// Normal force pressing on the region, `mu wu / s^2`.
    pub force: f32,
    /// In-plane width of the loaded patch, `wu`.
    pub width: f32,
    pub section: Section,
    /// Time the load acted, seconds.
    pub duration: f32,
    /// Plastic displacement the host measured during `duration`, `wu`.
    /// `None` uses the fallback progression.
    pub measured_plastic_displacement: Option<f32>,
}

impl SustainedLoad {
    pub fn validate(self) -> Result<(), MaterialError> {
        if self.force.is_finite()
            && (0.0..=1.0e12).contains(&self.force)
            && self.width.is_finite()
            && (1.0e-6..=1.0e6).contains(&self.width)
            && duration_ok(self.duration)
            && measured_ok(self.measured_plastic_displacement)
        {
            self.section.validate()
        } else {
            Err(MaterialError::InvalidLoad)
        }
    }
}

/// Constant-strength fallback: work accrues linearly with time and overstress.
/// Remaining capacity limits the charge, not the load or the damage threshold.
fn fallback_work(region: &Region, stress: f32, volume: f32, duration: f32) -> f32 {
    volume * (stress - region.effective_strength()) * (duration / CRUSH_TIME_CONSTANT)
}

/// Report strain energy with no charge. A sustained call is not a loading
/// cycle, so its strain energy persists as recoverable and none is damped.
fn safe_outcome(outcome: &mut Outcome, stored: f32, deformation: f32) {
    let ledger = &mut outcome.ledger;
    ledger.available = stored;
    ledger.recoverable[0] = stored;
    ledger.deformation[0] = deformation;
}

/// Shared overload accounting for one region: the strain energy at the safe
/// limit stays recoverable and `work` is charged against the capacity.
fn overload_outcome(
    outcome: &mut Outcome,
    region: &Region,
    stored: f32,
    work: f32,
    deformation: f32,
) {
    safe_outcome(outcome, stored, deformation);
    let remaining = region.remaining();
    let consumed = work.min(remaining);
    let ledger = &mut outcome.ledger;
    ledger.available = stored + work;
    ledger.irreversible[0] = consumed;
    ledger.excess = work - consumed;
    outcome.yielded[0] = true;
    outcome.consumed[0] = consumed;
    outcome.failed[0] = remaining > 0.0 && consumed >= remaining;
}

/// Evaluate a sustained normal load on one region. `consumed` is for the host
/// to `absorb`; the region is not mutated.
pub fn sustained(region: &Region, load: SustainedLoad) -> Result<Outcome, MaterialError> {
    load.validate()?;
    if load.duration == 0.0 {
        return Ok(Outcome::default());
    }
    let material = region.material();
    let area = load.width * load.section.thickness;
    let stiffness = load.section.contact_stiffness(material.stiffness, area);
    let stress = load.force / area;
    let strength = region.effective_strength();
    let mut outcome = Outcome::default();
    outcome.ledger.peak_force = load.force;
    outcome.ledger.peak_stress[0] = stress;
    if stress <= strength {
        let stored = 0.5 * load.force * load.force / stiffness;
        safe_outcome(&mut outcome, stored, load.force / stiffness);
        return outcome.checked();
    }
    let yield_force = strength * area;
    let yield_deflection = yield_force / stiffness;
    let (work, plastic) = match load.measured_plastic_displacement {
        Some(displacement) => (yield_force * displacement, displacement),
        None => {
            let volume = area * load.section.depth;
            let work = fallback_work(region, stress, volume, load.duration);
            // Work-equivalent displacement at the safe limit the call began at.
            (work, work / yield_force)
        }
    };
    overload_outcome(
        &mut outcome,
        region,
        0.5 * yield_force * yield_deflection,
        work,
        yield_deflection + plastic,
    );
    outcome.checked()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ConnectionLoad {
    /// Bending moment about the connection, `mu wu^2 / s^2`; the sign is ignored.
    pub moment: f32,
    /// Axial force along the connection, tension or compression; the sign is
    /// ignored. A finite root attachment is the tension case.
    pub axial: f32,
    /// Time the load acted, seconds.
    pub duration: f32,
    /// Plastic rotation the host measured, radians. `None` uses the fallback.
    pub measured_plastic_rotation: Option<f32>,
    /// Plastic extension the host measured, `wu`. `None` uses the fallback.
    pub measured_plastic_extension: Option<f32>,
}

impl ConnectionLoad {
    pub fn validate(self) -> Result<(), MaterialError> {
        if self.moment.is_finite()
            && self.moment.abs() <= 1.0e12
            && self.axial.is_finite()
            && self.axial.abs() <= 1.0e12
            && duration_ok(self.duration)
            && measured_ok(self.measured_plastic_rotation)
            && measured_ok(self.measured_plastic_extension)
        {
            Ok(())
        } else {
            Err(MaterialError::InvalidLoad)
        }
    }
}

/// Evaluate a bending and axial load on a connection. Stress is the
/// conservative superposition `|F| / A + |M| / S`; shear is not modeled.
pub fn connection(joint: &Connection, load: ConnectionLoad) -> Result<Outcome, MaterialError> {
    load.validate()?;
    if load.duration == 0.0 {
        return Ok(Outcome::default());
    }
    let region = joint.region();
    let material = region.material();
    let beam = joint.beam();
    let moment = load.moment.abs();
    let axial = load.axial.abs();
    let stress = axial / beam.area() + moment / beam.section_modulus();
    let strength = region.effective_strength();
    let rotation_stiffness = material.stiffness * beam.second_moment() / beam.length;
    let axial_stiffness = material.stiffness * beam.area() / beam.length;
    let rotation = moment / rotation_stiffness;
    let extension = axial / axial_stiffness;
    let stored = 0.5 * (moment * rotation + axial * extension);
    let mut outcome = Outcome::default();
    outcome.ledger.peak_force = axial;
    outcome.ledger.peak_stress[0] = stress;
    if stress <= strength {
        safe_outcome(&mut outcome, stored, rotation);
        return outcome.checked();
    }
    // Scale the applied loads back to the safe limit along the same load path.
    let scale = strength / stress;
    let rotation_arm = 2.0 * beam.length / beam.height;
    // Measured components are charged at the safe limit. The fallback covers
    // the rest over the volume each sets flowing: a third of the beam in pure
    // bending, all of it in pure tension.
    let mut work = 0.0;
    let mut volume = 0.0;
    match load.measured_plastic_rotation {
        Some(plastic) => work += scale * moment * plastic,
        None => volume += moment * rotation_arm / stress,
    }
    match load.measured_plastic_extension {
        Some(plastic) => work += scale * axial * plastic,
        None => volume += axial * beam.length / stress,
    }
    let mut strain = 0.0;
    if volume > 0.0 {
        let fallback = fallback_work(region, stress, volume, load.duration);
        work += fallback;
        // Work-equivalent plastic strain at the safe limit the call began at.
        strain = fallback / (strength * volume);
    }
    let plastic_rotation = load
        .measured_plastic_rotation
        .unwrap_or(strain * rotation_arm);
    overload_outcome(
        &mut outcome,
        region,
        scale * scale * stored,
        work,
        scale * rotation + plastic_rotation,
    );
    outcome.checked()
}
