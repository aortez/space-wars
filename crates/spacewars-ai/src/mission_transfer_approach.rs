//! Geometric entry selection for one committed post-escape transfer.
use super::*;
use scenario_spacewars::surface_sortie::VehicleId;

pub const TRANSFER_APPROACH_PROFILE: &str = "clear_transfer_approach_v1";
const ENTRY_ALTITUDE: f32 = 85.0;
const OTHER_CLEARANCE: f32 = 105.0;
const SAMPLES: u32 = 32;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TransferApproach {
    pub evaluated_ticks: u64,
    pub alternate_ticks: u64,
    pub guided_ticks: u64,
    pub last: Option<TransferApproachSample>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TransferApproachSample {
    pub tick: u64,
    pub started_tick: u64,
    pub deadline_tick: u64,
    pub selected_tick: u64,
    pub vehicle: VehicleId,
    pub destination: usize,
    pub ordinary_entry: Vec2,
    pub entry: Vec2,
    pub bearing: Vec2,
    pub alternate: bool,
    pub used_for_transfer: bool,
    pub candidates_checked: u32,
    pub reason: &'static str,
}

fn clear_entry(o: &MissionObservationV1, target: usize, entry: Vec2) -> bool {
    o.planets
        .iter()
        .filter(|v| v.index != target)
        .all(|v| entry.distance_to(v.motion.position) >= v.radius + OTHER_CLEARANCE)
        && o.sun
            .is_none_or(|sun| entry.distance_to(sun.position) >= sun.radius + 110.0)
        && entry.distance_to(o.boundary.center) <= o.boundary.radius - 20.0
}

impl MaterialMissionPilot {
    pub(crate) fn configure_transfer_approach(&mut self, enabled: bool) {
        assert!(enabled || self.telemetry.transfer_speed.is_none());
        assert!(
            self.previous_tick.is_none(),
            "configure before the first intent"
        );
        assert!(!enabled || self.telemetry.escape_travel.is_some());
        self.telemetry.transfer_approach = enabled.then(TransferApproach::default);
    }

    pub(super) fn transfer_entry(
        &mut self,
        o: &MissionObservationV1,
        target: &PilotPlanetObservation,
    ) -> Vec2 {
        let p = &o.local.combat.recovery.flight.pilot;
        let radial = (p.ship.position - target.motion.position).normalized();
        let entry =
            |bearing: Vec2| target.motion.position + bearing * (target.radius + ENTRY_ALTITUDE);
        let ordinary = entry(radial);
        let Some(travel) = self
            .telemetry
            .escape_travel
            .as_ref()
            .and_then(|s| s.last)
            .filter(|a| a.finished_tick.is_none())
        else {
            return ordinary;
        };
        let Some(state) = self.telemetry.transfer_approach.as_mut() else {
            return ordinary;
        };
        if !p.controls_armed
            || !p.queries_ready
            || !o.local.combat.recovery.flight.flight.enabled
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || p.vehicle != travel.vehicle
            || !matches!(p.location, PilotLocation::Aboard(_))
            || p.landing.supported_feet != 0
            || self.capture.is_some()
            || self.recovery.is_some()
            || p.tick >= travel.deadline_tick
            || travel
                .destination
                .is_some_and(|index| index != target.index)
            || travel.selected_tick.unwrap_or(travel.started_tick) != self.selected_tick
        {
            return ordinary;
        }

        // This is an endpoint screen, not a physical flight certificate. Keep
        // the ordinary local climb, obstacle routing and native arrival checks.
        let mut sample = TransferApproachSample {
            tick: p.tick,
            started_tick: travel.started_tick,
            deadline_tick: travel.deadline_tick,
            selected_tick: self.selected_tick,
            vehicle: p.vehicle,
            destination: target.index,
            ordinary_entry: ordinary,
            entry: ordinary,
            bearing: radial,
            alternate: false,
            used_for_transfer: false,
            candidates_checked: 0,
            reason: "ordinary entry clear",
        };
        let previous = state.last.filter(|s| {
            s.alternate
                && s.started_tick == travel.started_tick
                && s.selected_tick == self.selected_tick
                && s.vehicle == p.vehicle
                && s.destination == target.index
        });
        if let Some(previous) = previous {
            sample.candidates_checked += 1;
            if clear_entry(o, target.index, entry(previous.bearing)) {
                sample.entry = entry(previous.bearing);
                sample.bearing = previous.bearing;
                sample.alternate = true;
                sample.reason = "retaining clear approach bearing";
            }
        }
        if !sample.alternate {
            sample.candidates_checked += 1;
            if !clear_entry(o, target.index, ordinary) {
                let mut best: Option<(Vec2, Vec2, f32)> = None;
                for i in 1..SAMPLES {
                    let bearing =
                        radial.rotate_radians(std::f32::consts::TAU * i as f32 / SAMPLES as f32);
                    let point = entry(bearing);
                    sample.candidates_checked += 1;
                    if clear_entry(o, target.index, point) {
                        let distance = p.ship.position.distance_to(point);
                        if best.is_none_or(|(_, _, d)| distance < d) {
                            best = Some((bearing, point, distance));
                        }
                    }
                }
                if let Some((bearing, point, _)) = best {
                    sample.entry = point;
                    sample.bearing = bearing;
                    sample.alternate = true;
                    sample.reason = "selected clear approach bearing";
                } else {
                    sample.reason = "no clear entry; ordinary guidance retained";
                }
            }
        }
        state.evaluated_ticks += 1;
        state.alternate_ticks += u64::from(sample.alternate);
        state.last = Some(sample);
        sample.entry
    }

    pub(super) fn record_transfer_approach_guidance(&mut self, tick: u64) {
        if let Some(state) = &mut self.telemetry.transfer_approach
            && let Some(sample) = &mut state.last
            && sample.tick == tick
        {
            sample.used_for_transfer = true;
            state.guided_ticks += u64::from(sample.alternate);
        }
    }
}

#[cfg(test)]
#[path = "mission_transfer_approach_tests.rs"]
mod tests;
