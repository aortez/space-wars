//! Transient impacts: two regions meet with a closing speed and a
//! contact-effective mass. The available energy loads two springs in series
//! with a positive post-yield compression slope. A weaker material yields
//! first, but cannot cap transmitted force forever. Both sides share one force
//! and one energy budget; capacity limits damage, not contact resistance.

use crate::{ContactGeometry, FailureMode, MaterialError, Outcome, Region, WorkLedger};
use serde::Serialize;

/// Transient post-yield compression stiffness relative to elastic stiffness.
/// This fixed, versioned approximation permits energetic weak-on-strong hits.
/// It is an event-local loading curve, not persistent strength hardening.
pub const POST_YIELD_STIFFNESS_FRACTION: f32 = 0.01;

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
            && (1.0e-9..=1.0e12).contains(&self.effective_mass)
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

fn sides(a: &Region, b: &Region, contact: ContactGeometry) -> [Side; 2] {
    let area = contact.area();
    [(a, contact.a), (b, contact.b)].map(|(region, section)| Side {
        stiffness: section.contact_stiffness(region.material().stiffness, area),
        yield_force: region.effective_strength() * area,
        remaining: region.remaining(),
        damping: region.material().damping,
        mode: region.material().mode,
    })
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
    let force = libm::sqrt(force_squared);
    let plastic_ratio = 1.0 / f64::from(POST_YIELD_STIFFNESS_FRACTION) - 1.0;
    let ledger = &mut outcome.ledger;
    ledger.peak_force = force as f32;
    for (index, side) in side.iter().enumerate() {
        let stiffness = f64::from(side.stiffness);
        let yield_force = f64::from(side.yield_force);
        let stored = (0.5 * force_squared / stiffness) as f32;
        let work = (0.5 * (force_squared - yield_force * yield_force).max(0.0) * plastic_ratio
            / stiffness) as f32;
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
    outcome.checked()
}

fn store(ledger: &mut WorkLedger, index: usize, side: &Side, stored: f32) {
    ledger.recoverable[index] = (1.0 - side.damping) * stored;
    ledger.dissipated[index] = side.damping * stored;
}
