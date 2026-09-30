//! Conditional capture references if the first native scan selects a retained
//! usable site. Neither scan scheduling nor this sum predicts selection success.
use super::{
    ArrivalLocalReference, RemoteArrivalScreen, TransferForecastEnd, TransferForecastReport,
    TransferScanClock, arrival_local,
};
use crate::mission_evaluation::MAX_EVIDENCE_AGE;
use scenario_spacewars::{PlayerId, surface_sortie::pilot::LandingSiteId};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FirstScanSuccessReference {
    pub site: LandingSiteId,
    pub measurement_tick: Option<u64>,
    /// Geometry remains at its original projected arrival, not the scan tick.
    pub geometry_tick: u64,
    pub eligible_sides: Vec<f32>,
    pub local_seconds: Option<f32>,
    pub conditional_total_seconds: Option<f32>,
    pub exceeds_match_time: Option<bool>,
    pub unknown: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FirstScanSuccessReport {
    pub model: &'static str,
    pub actor: PlayerId,
    pub source_tick: u64,
    pub destination: usize,
    pub revision: u64,
    pub handoff_tick: Option<u64>,
    pub scan_tick: Option<u64>,
    pub travel_seconds: Option<f32>,
    pub scan_wait_seconds: Option<f32>,
    pub references: Vec<FirstScanSuccessReference>,
    pub unknown: Option<&'static str>,
    pub conditions: &'static str,
    /// Deliberately unknown: none of the conditional references establishes
    /// that a future scan will select a usable site or the trip will succeed.
    pub remaining_trip_seconds: Option<f32>,
}

impl FirstScanSuccessReport {
    pub(super) fn compose(
        forecast: &TransferForecastReport,
        clock: &TransferScanClock,
        screen: &RemoteArrivalScreen,
    ) -> Self {
        let local = screen.local_reference.as_ref();
        let mut unknown = clock.unknown.or(screen.unknown).or_else(|| {
            if !clock.complete
                || forecast.end != Some(TransferForecastEnd::KinematicHandoff)
                || !screen.complete
                || local.is_none_or(|r| !r.complete)
            {
                Some("scan or arrival-local reference incomplete")
            } else if clock.source_tick != forecast.source_tick
                || clock.destination != forecast.destination
                || clock.handoff_tick != clock.source_tick.checked_add(forecast.ticks)
                || screen.source_tick != clock.source_tick
                || screen.destination != clock.destination
                || local.is_some_and(|r| r.source_tick != clock.source_tick)
                || screen.source_planet.as_ref().is_none_or(|p| {
                    p.index != clock.destination
                        || p.revision != clock.revision
                        || !arrival_local::neutral_claim(p)
                })
                || screen.arrival.as_ref().is_none_or(|a| {
                    Some(a.tick) != clock.handoff_tick
                        || a.planet.index != clock.destination
                        || a.planet.revision != clock.revision
                        || !arrival_local::neutral_claim(&a.planet)
                })
            {
                Some("scan and arrival evidence identity or epoch mismatch")
            } else {
                local.and_then(|r| r.unknown)
            }
        });
        let travel = forecast
            .handoff_seconds
            .filter(|s| s.is_finite() && *s >= 0.0);
        let wait_ticks = clock
            .opportunity_tick
            .zip(clock.handoff_tick)
            .and_then(|(scan, handoff)| scan.checked_sub(handoff))
            .filter(|ticks| *ticks > 0 && *ticks <= 15)
            .filter(|ticks| clock.handoff_to_scan_ticks == Some(*ticks));
        if travel.is_none() || wait_ticks.is_none() {
            unknown.get_or_insert("conditional handoff or scan delay unavailable");
        }
        let wait = wait_ticks.map(|ticks| ticks as f32 / 60.0);
        let references = local
            .into_iter()
            .flat_map(|r| &r.references)
            .map(|reference| Self::reference(reference, clock, unknown, travel.zip(wait)))
            .collect();
        Self {
            model: "conditional_first_scan_success_v1",
            actor: clock.actor,
            source_tick: clock.source_tick,
            destination: clock.destination,
            revision: clock.revision,
            handoff_tick: clock.handoff_tick,
            scan_tick: clock.opportunity_tick,
            travel_seconds: travel,
            scan_wait_seconds: wait,
            references,
            unknown,
            conditions: "conditional native handoff and uninterrupted requests; unchanged neutral claim/material/form and survey history; first scan selects this retained usable site and an eligible solar direction; arrival geometry remains applicable through that scan; successful unexposed local execution",
            remaining_trip_seconds: None,
        }
    }

    fn reference(
        reference: &ArrivalLocalReference,
        clock: &TransferScanClock,
        parent_unknown: Option<&'static str>,
        phases: Option<(f32, f32)>,
    ) -> FirstScanSuccessReference {
        let mut unknown = parent_unknown.or(reference.unknown);
        if reference.site.planet != clock.destination
            || Some(reference.arrival_tick) != clock.handoff_tick
            || reference.measurement_tick.is_none_or(|tick| {
                tick > clock.source_tick
                    || clock
                        .opportunity_tick
                        .is_none_or(|scan| scan < tick || scan - tick > MAX_EVIDENCE_AGE)
            })
            || reference
                .projected
                .is_none_or(|site| site.id != reference.site || site.revision != clock.revision)
            || reference.eligible_sides.is_empty()
            || reference
                .eligible_sides
                .iter()
                .any(|side| !matches!(*side, -1.0 | 1.0))
        {
            unknown.get_or_insert("usable retained site or direction unavailable");
        }
        let local = reference
            .conditional_seconds
            .filter(|s| s.is_finite() && *s >= 0.0);
        if local.is_none() || reference.phases.is_none() {
            unknown.get_or_insert("local duration reference unavailable");
        }
        let total = phases
            .zip(local)
            .map(|((travel, wait), local)| travel + wait + local)
            .filter(|s| s.is_finite());
        if total.is_none() {
            unknown.get_or_insert("conditional component sum unavailable");
        }
        let total = total.filter(|_| unknown.is_none());
        FirstScanSuccessReference {
            site: reference.site,
            measurement_tick: reference.measurement_tick,
            geometry_tick: reference.arrival_tick,
            eligible_sides: reference.eligible_sides.clone(),
            local_seconds: local,
            conditional_total_seconds: total,
            exceeds_match_time: total
                .zip(clock.match_remaining_ticks)
                .map(|(seconds, ticks)| f64::from(seconds) >= ticks as f64 / 60.0),
            unknown,
        }
    }
}
