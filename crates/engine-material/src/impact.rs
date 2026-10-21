//! Transient impacts: two regions meet with a closing speed and a
//! contact-effective mass. The available energy loads two springs in series
//! until the weaker safe limit caps the force; work beyond that plateau is
//! irreversible and charged to the region that yielded.

use crate::{ContactGeometry, FailureMode, MaterialError, Outcome, Region, WorkLedger};
use serde::Serialize;

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

fn series_stiffness(a: f32, b: f32) -> f32 {
    1.0 / (1.0 / a + 1.0 / b)
}

/// Closing speed at which the weaker region reaches its safe limit for this
/// contact: `min(F_y) / sqrt(m_eff * k_series)`. Slower impacts are fully
/// recoverable.
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
    let series = series_stiffness(first.stiffness, second.stiffness);
    let speed = first.yield_force.min(second.yield_force) / (effective_mass * series).sqrt();
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
    let series = series_stiffness(side[0].stiffness, side[1].stiffness);
    let elastic_force = (2.0 * energy * series).sqrt();
    let plateau = side[0].yield_force.min(side[1].yield_force);
    let ledger = &mut outcome.ledger;
    if elastic_force <= plateau {
        // Both regions stay below their safe limits: nothing is permanent.
        ledger.peak_force = elastic_force;
        for (index, side) in side.iter().enumerate() {
            store(
                ledger,
                index,
                side,
                0.5 * elastic_force * elastic_force / side.stiffness,
            );
            ledger.peak_stress[index] = elastic_force / area;
            ledger.deformation[index] = elastic_force / side.stiffness;
        }
        return outcome.checked();
    }
    // The weaker safe limit caps the force. The stronger region never carries
    // more than that force, so it stays elastic however much energy arrives;
    // a weaker material cannot damage a stronger one in a single contact.
    let weak = [
        side[0].yield_force <= plateau,
        side[1].yield_force <= plateau,
    ];
    let stored = side.map(|side| 0.5 * plateau * plateau / side.stiffness);
    let plateau_work = (energy - stored[0] - stored[1]).max(0.0);
    let share = if weak[0] && weak[1] { 0.5 } else { 1.0 };
    ledger.peak_force = plateau;
    for (index, side) in side.iter().enumerate() {
        ledger.peak_stress[index] = plateau / area;
        ledger.deformation[index] = plateau / side.stiffness;
        if !weak[index] {
            store(ledger, index, side, stored[index]);
            continue;
        }
        outcome.yielded[index] = true;
        let work = plateau_work * share;
        let charge = match side.mode {
            FailureMode::Brittle => work + stored[index],
            FailureMode::Ductile => {
                store(ledger, index, side, stored[index]);
                work
            }
        };
        let consumed = charge.min(side.remaining);
        outcome.consumed[index] = consumed;
        outcome.failed[index] = side.remaining > 0.0 && consumed >= side.remaining;
        ledger.irreversible[index] = consumed;
        ledger.excess += charge - consumed;
        ledger.deformation[index] += work / plateau;
    }
    outcome.checked()
}

fn store(ledger: &mut WorkLedger, index: usize, side: &Side, stored: f32) {
    ledger.recoverable[index] = (1.0 - side.damping) * stored;
    ledger.dissipated[index] = side.damping * stored;
}
