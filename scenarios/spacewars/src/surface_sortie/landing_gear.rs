//! Automatic, presentation-only landing gear. Reuses completed landing
//! telemetry; it neither queries terrain nor changes colliders or controls.

use super::*;

pub mod fixture;
#[cfg(test)]
mod tests;

const EXTEND_HEIGHT: f32 = 18.0;
const RETRACT_HEIGHT: f32 = 24.0;
const EXTEND_ANGLE: f32 = 50.0;
const RETRACT_ANGLE: f32 = 65.0;
const EXTEND_SECONDS: f32 = 0.4;
const RETRACT_SECONDS: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LandingGear {
    progress: f32,
    deployed: bool,
}

impl Default for LandingGear {
    fn default() -> Self {
        // Existing setups may spawn parked. Free-flight starts fold away on
        // their first simulation ticks, without a misleading unsupported frame.
        Self {
            progress: 1.0,
            deployed: true,
        }
    }
}

impl LandingGear {
    pub(super) fn extension(self) -> f32 {
        self.progress * self.progress * (3.0 - 2.0 * self.progress)
    }

    pub(super) fn advance(&mut self, landing: &LandingTelemetry, ship: &ShipState, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        if ship.dead || ship.form != ShipForm::Ship {
            *self = Self::default();
            return;
        }
        if landing.supported_feet > 0
            || matches!(landing.phase, LandingPhase::Settling | LandingPhase::Landed)
        {
            // Fast touchdown must never show a supported ship hovering above
            // its drawn feet. This is not a new landing/boarding delay or gate.
            *self = Self::default();
            return;
        }
        let (height, angle) = if self.deployed {
            (RETRACT_HEIGHT, RETRACT_ANGLE)
        } else {
            (EXTEND_HEIGHT, EXTEND_ANGLE)
        };
        self.deployed = landing.planet.is_some()
            && landing.altitude.is_finite()
            && landing.altitude <= height
            && landing.angle_degrees <= angle
            && !ship.wings_closed
            && flight::sweep(ship) <= 0.01;
        let target = if self.deployed { 1.0 } else { 0.0 };
        let seconds = if self.deployed {
            EXTEND_SECONDS
        } else {
            RETRACT_SECONDS
        };
        self.progress += (target - self.progress).clamp(-dt / seconds, dt / seconds);
        self.progress = self.progress.clamp(0.0, 1.0);
    }
}
