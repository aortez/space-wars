//! One energy ledger per event. Every unit of available energy ends up as
//! recoverable elastic energy, harmless dissipation, irreversible work in a
//! region, or excess awaiting explicit failure-motion/dissipation allocation.

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
    /// Work no region could absorb. The host must resolve its disposition.
    pub excess: f32,
    /// Excess allocated to surviving bodies/fragments. The host must apply it
    /// with momentum accounting; it is not supplied by restitution.
    pub failure_motion: f32,
    /// Excess explicitly dissipated by the host's destruction policy.
    pub failure_dissipated: f32,
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
            + self.failure_motion
            + self.failure_dissipated
    }

    /// Whether available energy equals accounted energy within a relative
    /// tolerance, with a few `f32` ulps of absolute slack for tiny events.
    pub fn closes(&self, relative_tolerance: f32) -> bool {
        let difference = (self.available - self.accounted()).abs();
        difference <= relative_tolerance * self.available.abs() + 16.0 * f32::EPSILON
    }

    /// Resolve excess once, assigning `motion` to surviving bodies/fragments
    /// and the remainder to destruction dissipation. Zero means a blocking,
    /// dissipative failure; this is a host choice, not a penetration model.
    /// Invalid allocations leave the ledger unchanged.
    pub fn resolve_excess(&mut self, motion: f32) -> Result<(), MaterialError> {
        if !self.finite() || !motion.is_finite() || !(0.0..=self.excess).contains(&motion) {
            return Err(MaterialError::InvalidLoad);
        }
        let carried = self.failure_motion + motion;
        let dissipated = self.failure_dissipated + (self.excess - motion);
        if !carried.is_finite() || !dissipated.is_finite() {
            return Err(MaterialError::Overflow);
        }
        self.failure_motion = carried;
        self.failure_dissipated = dissipated;
        self.excess = 0.0;
        Ok(())
    }

    /// Coefficient of restitution targeting this isolated impact's
    /// recoverable energy, `sqrt(recoverable / available)`. The host sets the
    /// contact's rebound from it so the physics engine does not spend the
    /// bounce a second time. Excess must be resolved first; allocated failure
    /// motion needs a separate host response. It means nothing for sustained loads.
    pub fn restitution(&self) -> Result<f32, MaterialError> {
        if !self.finite() {
            return Err(MaterialError::InvalidLoad);
        }
        if self.excess != 0.0 {
            return Err(MaterialError::UnresolvedExcess);
        }
        if self.available > 0.0 {
            Ok(libm::sqrtf(self.recoverable.iter().sum::<f32>() / self.available).min(1.0))
        } else {
            Ok(0.0)
        }
    }

    pub fn finite(&self) -> bool {
        [
            self.available,
            self.excess,
            self.failure_motion,
            self.failure_dissipated,
            self.peak_force,
        ]
        .iter()
        .chain(&self.recoverable)
        .chain(&self.dissipated)
        .chain(&self.irreversible)
        .chain(&self.peak_stress)
        .chain(&self.deformation)
        .all(|value| value.is_finite() && *value >= 0.0)
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
