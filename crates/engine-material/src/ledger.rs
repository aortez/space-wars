//! One energy ledger per event. Every unit of available energy ends up as
//! recoverable elastic energy, harmless dissipation, irreversible work in a
//! region, or excess that stays with the motion. Nothing is counted twice.

use crate::MaterialError;
use serde::Serialize;

/// Relative closure tolerance used by fixtures and tests.
pub const LEDGER_TOLERANCE: f32 = 1.0e-4;

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct WorkLedger {
    /// Energy the event made available, `mu wu^2 / s^2`.
    pub available: f32,
    /// Elastic energy each region returns.
    pub recoverable: [f32; 2],
    /// Elastic energy each region loses harmlessly through damping.
    pub dissipated: [f32; 2],
    /// Work charged against each region's capacity.
    pub irreversible: [f32; 2],
    /// Work no region could absorb; it remains with the motion.
    pub excess: f32,
    /// Largest force carried through the contact, or the axial force on a
    /// connection.
    pub peak_force: f32,
    /// Peak stress seen by each region.
    pub peak_stress: [f32; 2],
    /// Deformation of each region: `wu` for contacts, radians for connections.
    pub deformation: [f32; 2],
}

impl WorkLedger {
    pub fn accounted(&self) -> f32 {
        self.recoverable
            .iter()
            .chain(&self.dissipated)
            .chain(&self.irreversible)
            .sum::<f32>()
            + self.excess
    }

    /// Whether available energy equals accounted energy within a relative
    /// tolerance, with a few `f32` ulps of absolute slack for tiny events.
    pub fn closes(&self, relative_tolerance: f32) -> bool {
        let difference = (self.available - self.accounted()).abs();
        difference <= relative_tolerance * self.available.abs() + 16.0 * f32::EPSILON
    }

    /// Coefficient of restitution that returns exactly this impact's
    /// recoverable energy, `sqrt(recoverable / available)`. The host sets the
    /// contact's rebound from it so the physics engine does not spend the
    /// bounce a second time. It means nothing for sustained loads.
    pub fn restitution(&self) -> f32 {
        if self.available > 0.0 {
            (self.recoverable.iter().sum::<f32>() / self.available)
                .sqrt()
                .min(1.0)
        } else {
            0.0
        }
    }

    pub fn finite(&self) -> bool {
        [self.available, self.excess, self.peak_force]
            .iter()
            .chain(&self.recoverable)
            .chain(&self.dissipated)
            .chain(&self.irreversible)
            .chain(&self.peak_stress)
            .chain(&self.deformation)
            .all(|value| value.is_finite())
    }
}

/// Result of one loading event. Single-region loads fill slot 0.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Outcome {
    pub ledger: WorkLedger,
    /// Irreversible work for the host to `absorb` into each region.
    pub consumed: [f32; 2],
    /// Whether each region reached its safe limit.
    pub yielded: [bool; 2],
    /// Whether this event exhausts each region. Only the event that takes a
    /// region's last capacity sets it, so a host starts its failure handling
    /// once; a region that had already failed reports `false`.
    pub failed: [bool; 2],
}

impl Outcome {
    /// Neither region reached its safe limit, so nothing was charged.
    pub fn safe(&self) -> bool {
        !self.yielded[0] && !self.yielded[1]
    }

    /// Inputs inside their ranges can still overflow `f32` together. Every
    /// loading mode returns through here, so an `Ok` outcome is finite.
    pub(crate) fn checked(self) -> Result<Self, MaterialError> {
        if self.ledger.finite() {
            Ok(self)
        } else {
            Err(MaterialError::Overflow)
        }
    }
}
