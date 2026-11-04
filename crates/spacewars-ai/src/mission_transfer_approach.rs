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

/// The entries a learned flight host was given for the pilot's transfer
/// legs, by tick: the pilot's own entry while it was clear, a clear bearing
/// around the destination while the pilot's lay inside another planet's
/// margin, the sun's or the boundary's, or the pilot's entry though blocked,
/// when no bearing was clear.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct LearnedEntries {
    pub ordinary_ticks: u64,
    pub alternate_ticks: u64,
    pub blocked_ticks: u64,
}

/// A clear bearing a learned leg keeps while it stays clear, for one
/// selection of one destination.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LearnedBearing {
    selected_tick: u64,
    target: usize,
    bearing: Vec2,
}

/// Whether an entry point is clear of the other planets' margins, the sun's
/// and the boundary: the mission's screen for an entry worth flying to.
pub(super) fn clear_entry(o: &MissionObservationV1, target: usize, entry: Vec2) -> bool {
    o.planets
        .iter()
        .filter(|v| v.index != target)
        .all(|v| entry.distance_to(v.motion.position) >= v.radius + OTHER_CLEARANCE)
        && o.sun
            .is_none_or(|sun| entry.distance_to(sun.position) >= sun.radius + 110.0)
        && entry.distance_to(o.boundary.center) <= o.boundary.radius - 20.0
}

/// The clear entry nearest the ship among the other sampled bearings around
/// the target, with its bearing, or none when every one is blocked.
pub(super) fn nearest_clear_entry(
    o: &MissionObservationV1,
    target: &PilotPlanetObservation,
    ship: Vec2,
    radial: Vec2,
) -> Option<(Vec2, Vec2)> {
    let mut best: Option<(Vec2, Vec2, f32)> = None;
    for i in 1..SAMPLES {
        let bearing = radial.rotate_radians(std::f32::consts::TAU * i as f32 / SAMPLES as f32);
        let point = target.motion.position + bearing * (target.radius + ENTRY_ALTITUDE);
        if clear_entry(o, target.index, point) {
            let distance = ship.distance_to(point);
            if best.is_none_or(|(_, _, d)| distance < d) {
                best = Some((bearing, point, distance));
            }
        }
    }
    best.map(|(bearing, point, _)| (bearing, point))
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
                sample.candidates_checked += SAMPLES - 1;
                if let Some((bearing, point)) =
                    nearest_clear_entry(o, target, p.ship.position, radial)
                {
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

    /// The entry a learned flight host is given for the leg. The stack flies
    /// to the point it is given, so the leg takes a clear one: the pilot's
    /// own entry while it is clear of the other planets' margins, the sun's
    /// and the boundary, otherwise the clear entry nearest the ship among
    /// the bearings around the destination, kept while it stays clear, and
    /// the pilot's entry when no bearing is clear. The pilot's own flight
    /// keeps its entry, which its detours and the arrival rule at the
    /// destination's radius do not need clear.
    pub(super) fn learned_entry(
        &mut self,
        o: &MissionObservationV1,
        target: &PilotPlanetObservation,
        pilot_entry: Vec2,
    ) -> Vec2 {
        let ship = o.local.combat.recovery.flight.pilot.ship.position;
        let retained = self
            .learned_bearing
            .filter(|b| b.selected_tick == self.selected_tick && b.target == target.index)
            .map(|b| target.motion.position + b.bearing * (target.radius + ENTRY_ALTITUDE))
            .filter(|&point| clear_entry(o, target.index, point));
        let counts = self.telemetry.learned_entries.get_or_insert_default();
        if let Some(point) = retained {
            counts.alternate_ticks += 1;
            return point;
        }
        if clear_entry(o, target.index, pilot_entry) {
            counts.ordinary_ticks += 1;
            self.learned_bearing = None;
            return pilot_entry;
        }
        let radial = (ship - target.motion.position).normalized();
        match nearest_clear_entry(o, target, ship, radial) {
            Some((bearing, point)) => {
                counts.alternate_ticks += 1;
                self.learned_bearing = Some(LearnedBearing {
                    selected_tick: self.selected_tick,
                    target: target.index,
                    bearing,
                });
                point
            }
            None => {
                counts.blocked_ticks += 1;
                self.learned_bearing = None;
                pilot_entry
            }
        }
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
