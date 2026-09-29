//! Successful neutral-trip medians attached to a conditional arrival site.
//! These are not geometry-specific duration predictions or native choices.
use super::remote_arrival::RemoteArrivalScreen;
use crate::mission_evaluation::{PhaseCosts, neutral_phase_costs};
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{
        PilotLocation, PlanetClaimPhase,
        mission::MissionObservationV1,
        pilot::{LandingSiteId, PilotLandingSite, PilotPlanetObservation},
    },
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArrivalLocalReference {
    pub site: LandingSiteId,
    pub measurement_tick: Option<u64>,
    pub arrival_tick: u64,
    pub projected: Option<PilotLandingSite>,
    /// Conditional solar eligibility, not a prediction of native selection.
    pub eligible_sides: Vec<f32>,
    pub phases: Option<PhaseCosts>,
    pub conditional_seconds: Option<f32>,
    pub unknown: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArrivalLocalReport {
    pub model: &'static str,
    pub calibration: &'static str,
    pub policy: &'static str,
    pub source_tick: u64,
    pub references: Vec<ArrivalLocalReference>,
    pub charged_graph: u64,
    pub complete: bool,
    pub unknown: Option<&'static str>,
    pub phase_origin: &'static str,
    pub conditions: &'static str,
    pub solar_scope: &'static str,
    pub acquisition_seconds: Option<f32>,
    pub future_threat: &'static str,
    pub remaining_trip_seconds: Option<f32>,
}

impl ArrivalLocalReport {
    pub(super) fn new(policy: &'static str, o: &MissionObservationV1) -> Self {
        let p = &o.local.combat.recovery.flight.pilot;
        let unknown = if policy != "material_mission_v13" {
            Some("policy outside arrival-local reference domain")
        } else if !p.controls_armed
            || !p.queries_ready
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || !p.ship_health.is_finite()
            || p.ship_health <= 0.0
            || p.location != PilotLocation::Aboard(p.vehicle)
        {
            Some("source ship unavailable for arrival-local reference")
        } else {
            None
        };
        Self {
            model: "arrival_local_reference_v1",
            calibration: "neutral_successful_trip_medians_v1",
            policy,
            source_tick: p.tick,
            references: Vec::new(),
            charged_graph: 0,
            complete: unknown.is_some(),
            unknown,
            phase_origin: "future native site choice, whose tick is unknown; no elapsed-time subtraction",
            conditions: "native selects this site and an eligible direction; neutral rules and usable material/hatches persist; successful unexposed v13/tactical-v11 execution",
            solar_scope: "geometric approach and fixed parking screen only; not a solar safety certificate over the empirical phase durations",
            acquisition_seconds: None,
            future_threat: "unknown; historical opponent and cover do not establish future exposure",
            remaining_trip_seconds: None,
        }
    }

    pub(super) fn reject(&mut self, reason: &'static str) {
        self.unknown.get_or_insert(reason);
        self.complete = true;
    }

    /// One charged step per recorded site, after both solar screens finish.
    /// At most four records. No physical query or future controller is run.
    pub(super) fn step(&mut self, screen: &RemoteArrivalScreen) {
        let site = &screen.sites[self.references.len()];
        let arrival = screen.arrival.as_ref().unwrap();
        let eligible_sides: Vec<_> = site
            .directions
            .iter()
            .filter(|d| d.solar_clear)
            .map(|d| d.side)
            .collect();
        let unknown = if site.unknown.is_some() {
            site.unknown
        } else if site.projected.is_none() || site.source.measurement.is_none() {
            Some("arrival material site unavailable")
        } else if !neutral_claim(&arrival.planet) {
            Some("claim state outside arrival-local reference domain")
        } else if site.directions.len() != 2 || eligible_sides.is_empty() {
            Some("no eligible arrival solar direction")
        } else {
            None
        };
        let phases = unknown.is_none().then(neutral_phase_costs);
        self.references.push(ArrivalLocalReference {
            site: site.source.id,
            measurement_tick: site.source.measurement.as_ref().map(|m| m.tick),
            arrival_tick: arrival.tick,
            projected: site.projected,
            eligible_sides,
            conditional_seconds: phases.as_ref().map(PhaseCosts::total),
            phases,
            unknown,
        });
        self.charged_graph += 1;
        self.complete = self.references.len() == screen.sites.len();
    }
}

pub(super) fn neutral_claim(planet: &PilotPlanetObservation) -> bool {
    planet.claim.as_ref().is_some_and(|c| {
        c.planet == planet.index
            && c.owner.is_none()
            && c.flag.is_none()
            && c.claimant.is_none()
            && c.phase == PlanetClaimPhase::Idle
            && c.progress == 0.0
            && c.stage_required_seconds.is_finite()
            && (c.stage_required_seconds - 3.0).abs() <= 0.001
            && c.flag_interaction_range.is_finite()
            && c.flag_interaction_range > 0.0
    })
}
