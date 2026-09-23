//! Opt-in first-site deadline and local waiting guidance. No landing permissions.
use super::*;

pub const ACQUISITION_DEADLINE_TICKS: u64 = 30 * 60;
const CLEARANCE_GRACE_TICKS: u64 = 2 * 60;
pub const ACQUISITION_WAIT_PROFILE: &str = "bounded_site_acquisition_v1";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AcquisitionWait {
    pub planet: usize,
    pub started_tick: u64,
    pub deadline_tick: u64,
    pub hold_altitude: f32,
    pub finished_tick: Option<u64>,
    pub outcome: Option<&'static str>,
    pub last_wait_reason: Option<&'static str>,
    pub guidance: Option<&'static str>,
}

impl TacticalSortiePilot {
    pub(crate) fn enable_bounded_acquisition(&mut self, enabled: bool) {
        self.bounded_acquisition = enabled;
    }

    pub(crate) fn start_acquisition(&mut self, o: &TacticalSortieObservationV1) {
        if !self.bounded_acquisition || self.telemetry.acquisition_wait.is_some() {
            return;
        }
        let p = &o.combat.recovery.flight.pilot;
        let altitude = p.ship.position.distance_to(p.planet.motion.position) - p.planet.radius;
        self.telemetry.acquisition_wait = Some(AcquisitionWait {
            planet: p.planet.index,
            started_tick: p.tick,
            deadline_tick: p.tick.saturating_add(ACQUISITION_DEADLINE_TICKS),
            hold_altitude: altitude.clamp(60.0, 85.0),
            finished_tick: None,
            outcome: None,
            last_wait_reason: None,
            guidance: None,
        });
    }

    pub(super) fn finish_acquisition(&mut self, tick: u64, outcome: &'static str) {
        if let Some(wait) = &mut self.telemetry.acquisition_wait
            && wait.finished_tick.is_none()
        {
            wait.finished_tick = Some(tick);
            wait.outcome = Some(outcome);
            wait.guidance = None;
        }
    }

    pub(super) fn acquisition_expired(&self, tick: u64) -> bool {
        self.telemetry
            .acquisition_wait
            .is_some_and(|w| w.finished_tick.is_none() && tick >= w.deadline_tick)
    }

    pub(super) fn wait_for_site(
        &mut self,
        o: &TacticalSortieObservationV1,
        clearance_speed: f32,
    ) -> CombatIntent {
        let p = &o.combat.recovery.flight.pilot;
        let offset = p.ship.position - p.planet.motion.position;
        let up = offset.normalized();
        let legacy = up * clearance_speed;
        let Some(wait) = self.telemetry.acquisition_wait.filter(|w| {
            w.finished_tick.is_none() && self.site.is_none() && w.planet == p.planet.index
        }) else {
            return self.guide(o, legacy, Vec2::ZERO);
        };
        let altitude = offset.length() - p.planet.radius;
        let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
        let falling = (-relative.dot(up)).max(0.0);
        // Foot rays saturate at 26: that is no hit in the short ray, not a
        // measured high-altitude clearance. Preserve the clearance command
        // around observed ground and while stopping a substantial inward fall.
        let near_ground = p.landing.supported_feet > 0
            || p.landing
                .foot_clearances
                .iter()
                .any(|h| !h.is_finite() || *h < 25.0)
            || altitude < 40.0 + falling * falling / 50.0;
        let desired = up * ((wait.hold_altitude - altitude) * 0.5).clamp(-6.0, 12.0);
        let solar_near = o.sun.is_some_and(|sun| {
            let next = p.ship.position + (p.planet.velocity_at(p.ship.position) + desired) * 2.0;
            crate::landing_safety::distance_to_segment(sun.position, p.ship.position, next)
                < sun.heat_radius + 10.0
        });
        let guidance = if p.tick.saturating_sub(wait.started_tick) < CLEARANCE_GRACE_TICKS {
            "initial_clearance"
        } else if near_ground {
            "ground_clearance"
        } else if solar_near {
            "solar_clearance"
        } else {
            "hold_altitude"
        };
        let state = self.telemetry.acquisition_wait.as_mut().unwrap();
        state.last_wait_reason = self.telemetry.acquisition.map(|a| a.reason);
        state.guidance = Some(guidance);
        self.guide(
            o,
            if guidance == "hold_altitude" {
                desired
            } else {
                legacy
            },
            Vec2::ZERO,
        )
    }
}

#[cfg(test)]
mod tests;
