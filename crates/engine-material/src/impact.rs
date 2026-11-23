//! Transient impacts: two regions meet with a closing speed and a
//! contact-effective mass. The available energy loads two springs in series
//! with a positive post-yield compression slope. A weaker material yields
//! first, but cannot cap transmitted force forever. Both sides share one force
//! and one energy budget; capacity limits damage, not contact resistance.

use crate::{
    ContactGeometry, Face, FailureMode, MaterialError, Outcome, Region, Section, WorkLedger,
};
use serde::Serialize;

/// Transient post-yield compression stiffness relative to elastic stiffness.
/// This fixed, versioned approximation permits energetic weak-on-strong hits.
/// It is an event-local loading curve, not persistent strength hardening.
pub const POST_YIELD_STIFFNESS_FRACTION: f32 = 0.01;

/// The smallest effective mass the impact law prices, `mu`.
pub const MIN_EFFECTIVE_MASS: f32 = 1.0e-9;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Impact {
    /// Relative normal approach speed at the contact, `wu / s`.
    pub closing_speed: f32,
    /// Contact-effective mass of the two bodies, `mu`. See `effective_mass`.
    pub effective_mass: f32,
}

impl Impact {
    pub fn validate(self) -> Result<(), MaterialError> {
        if self.closing_speed.is_finite()
            && (0.0..=1.0e6).contains(&self.closing_speed)
            && self.effective_mass.is_finite()
            && (MIN_EFFECTIVE_MASS..=1.0e12).contains(&self.effective_mass)
        {
            Ok(())
        } else {
            Err(MaterialError::InvalidLoad)
        }
    }

    /// Available energy `0.5 * m_eff * v^2`. It is not health loss.
    pub fn energy(self) -> f32 {
        0.5 * self.effective_mass * self.closing_speed * self.closing_speed
    }
}

#[derive(Debug, Clone, Copy)]
struct Side {
    stiffness: f32,
    yield_force: f32,
    remaining: f32,
    damping: f32,
    mode: FailureMode,
}

fn side(region: &Region, section: Section, area: f32) -> Side {
    Side {
        stiffness: section.contact_stiffness(region.material().stiffness, area),
        yield_force: region.effective_strength() * area,
        remaining: region.remaining(),
        damping: region.material().damping,
        mode: region.material().mode,
    }
}

fn sides(a: &Region, b: &Region, contact: ContactGeometry) -> [Side; 2] {
    let area = contact.area();
    [side(a, contact.a, area), side(b, contact.b, area)]
}

/// Invert the sum of two bilinear loading-work curves. Each yield point adds
/// compliance in force-squared space; at most two boundaries are crossed.
/// f64 intermediates avoid overflow/cancellation for validated f32 inputs.
fn peak_force_squared(side: &[Side; 2], energy: f32) -> f64 {
    let mut ordered = *side;
    if ordered[0].yield_force > ordered[1].yield_force {
        ordered.swap(0, 1);
    }
    let mut compliance = ordered
        .iter()
        .map(|s| 0.5 / f64::from(s.stiffness))
        .sum::<f64>();
    let plastic_ratio = 1.0 / f64::from(POST_YIELD_STIFFNESS_FRACTION) - 1.0;
    let mut remaining = f64::from(energy);
    let mut force_squared = 0.0;
    for s in ordered {
        let next = f64::from(s.yield_force).powi(2);
        let work = (next - force_squared) * compliance;
        if remaining <= work {
            return force_squared + remaining / compliance;
        }
        remaining -= work;
        force_squared = next;
        compliance += 0.5 * plastic_ratio / f64::from(s.stiffness);
    }
    force_squared + remaining / compliance
}

/// Closing speed at which the weaker region reaches its safe limit for this
/// contact: `min(F_y) / sqrt(m_eff * k_series)`. Slower impacts are fully
/// elastic (with harmless damping).
pub fn yield_speed(
    a: &Region,
    b: &Region,
    contact: ContactGeometry,
    effective_mass: f32,
) -> Result<f32, MaterialError> {
    contact.validate()?;
    if !(effective_mass.is_finite() && effective_mass > 0.0) {
        return Err(MaterialError::InvalidLoad);
    }
    let [first, second] = sides(a, b, contact);
    let compliance = 1.0 / f64::from(first.stiffness) + 1.0 / f64::from(second.stiffness);
    let speed = (f64::from(first.yield_force.min(second.yield_force))
        * libm::sqrt(compliance / f64::from(effective_mass))) as f32;
    if speed.is_finite() {
        Ok(speed)
    } else {
        Err(MaterialError::Overflow)
    }
}

/// Evaluate one impact. The returned `consumed` work is for the host to
/// `absorb`; this function does not mutate the regions.
pub fn impact(
    a: &Region,
    b: &Region,
    contact: ContactGeometry,
    load: Impact,
) -> Result<Outcome, MaterialError> {
    contact.validate()?;
    load.validate()?;
    let area = contact.area();
    let side = sides(a, b, contact);
    let energy = load.energy();
    let mut outcome = Outcome {
        ledger: WorkLedger {
            available: energy,
            ..WorkLedger::default()
        },
        ..Outcome::default()
    };
    if energy <= 0.0 {
        return outcome.checked();
    }
    let force_squared = peak_force_squared(&side, energy);
    outcome.ledger.peak_force = libm::sqrt(force_squared) as f32;
    for (index, side) in side.iter().enumerate() {
        charge_side(&mut outcome, index, side, force_squared, area);
    }
    outcome.checked()
}

/// One pulse of `energy` arriving on a region's face, such as a blast's share
/// or a beam's pulse. It is the impact law against a rigid, unbreakable
/// counterpart: energy within the face's safe limit is recoverable or
/// harmlessly damped, and the region yields past it. The region is slot 0 of
/// the outcome, and the host absorbs its `consumed` work.
pub fn deposit(region: &Region, face: Face, energy: f32) -> Result<Outcome, MaterialError> {
    face.validate()?;
    if !(energy.is_finite() && energy >= 0.0) {
        return Err(MaterialError::InvalidLoad);
    }
    let area = face.area();
    let side = side(region, face.section, area);
    let mut outcome = Outcome {
        ledger: WorkLedger {
            available: energy,
            ..WorkLedger::default()
        },
        ..Outcome::default()
    };
    if energy <= 0.0 {
        return outcome.checked();
    }
    // The bilinear loading-work curve of one side, inverted as
    // `peak_force_squared` does with the counterpart's compliance at zero.
    let compliance = 0.5 / f64::from(side.stiffness);
    let yield_squared = f64::from(side.yield_force).powi(2);
    let elastic = yield_squared * compliance;
    let energy = f64::from(energy);
    let force_squared = if energy <= elastic {
        energy / compliance
    } else {
        yield_squared + (energy - elastic) * f64::from(POST_YIELD_STIFFNESS_FRACTION) / compliance
    };
    outcome.ledger.peak_force = libm::sqrt(force_squared) as f32;
    charge_side(&mut outcome, 0, &side, force_squared, area);
    outcome.checked()
}

/// Split one side's loading at the peak force into stored, damped and
/// irreversible work, capped by its remaining capacity.
fn charge_side(outcome: &mut Outcome, index: usize, side: &Side, force_squared: f64, area: f32) {
    let plastic_ratio = 1.0 / f64::from(POST_YIELD_STIFFNESS_FRACTION) - 1.0;
    let force = libm::sqrt(force_squared);
    let stiffness = f64::from(side.stiffness);
    let yield_force = f64::from(side.yield_force);
    let stored = (0.5 * force_squared / stiffness) as f32;
    let work = (0.5 * (force_squared - yield_force * yield_force).max(0.0) * plastic_ratio
        / stiffness) as f32;
    let ledger = &mut outcome.ledger;
    outcome.yielded[index] = force_squared > yield_force * yield_force;
    ledger.peak_stress[index] = (force / f64::from(area)) as f32;
    ledger.deformation[index] =
        ((force + (force - yield_force).max(0.0) * plastic_ratio) / stiffness) as f32;
    let charge = if outcome.yielded[index] && side.mode == FailureMode::Brittle {
        work + stored
    } else {
        store(ledger, index, side, stored);
        work
    };
    let consumed = charge.min(side.remaining);
    outcome.consumed[index] = consumed;
    outcome.failed[index] = side.remaining > 0.0 && consumed >= side.remaining;
    ledger.irreversible[index] = consumed;
    ledger.excess += charge - consumed;
}

fn store(ledger: &mut WorkLedger, index: usize, side: &Side, stored: f32) {
    ledger.recoverable[index] = (1.0 - side.damping) * stored;
    ledger.dissipated[index] = side.damping * stored;
}
