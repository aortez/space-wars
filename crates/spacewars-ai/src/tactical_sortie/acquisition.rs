//! Current native selection evidence, with no additional search or history.
use super::*;
use scenario_spacewars::surface_sortie::{
    live_planning::ObjectiveWorkState, pilot::LandingSiteQuery,
};
use std::cell::Cell;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct CandidateCheckCounts {
    /// Site-level exclusions, in the controller's existing check order.
    pub required_site: usize,
    pub previously_rejected: usize,
    pub solar_cooldown: usize,
    /// The following count directions; each surviving site has at most two.
    pub directions: usize,
    pub unsafe_solar: usize,
    /// These overlap when several solar clearances fail for one direction.
    pub unsafe_approach: usize,
    pub unsafe_parking: usize,
    pub unsafe_departure: usize,
    pub survey_unavailable: usize,
    pub route_absent: usize,
    pub route_unusable: usize,
    pub eligible: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AcquisitionTelemetry {
    pub tick: u64,
    pub planet: usize,
    pub revision: u64,
    pub objective: Option<LandingObjective>,
    pub measurement_tick: Option<u64>,
    pub generation: Option<u64>,
    pub objective_work: Option<ObjectiveWorkState>,
    pub site_query: LandingSiteQuery,
    pub sites_available: usize,
    pub required_site: Option<LandingSiteId>,
    pub selected_site: Option<LandingSiteId>,
    /// Set by the branch actually taken, not inferred from absent candidates.
    pub reason: &'static str,
    pub survey_rejected_by: Option<&'static str>,
    pub checks: CandidateCheckCounts,
}
impl AcquisitionTelemetry {
    pub(super) fn new(
        o: &TacticalSortieObservationV1,
        required_site: Option<LandingSiteId>,
        selected_site: Option<LandingSiteId>,
    ) -> Self {
        let p = &o.combat.recovery.flight.pilot;
        Self {
            tick: p.tick,
            planet: p.planet.index,
            revision: p.planet.revision,
            objective: LandingObjective::read(p),
            measurement_tick: o
                .landing_objective
                .as_ref()
                .map(|s| s.tick)
                .or_else(|| o.objective_evidence.and_then(|e| e.measurement_tick)),
            generation: o.objective_evidence.and_then(|e| e.generation),
            objective_work: o.objective_work,
            site_query: p.site_query,
            sites_available: p.sites.len(),
            required_site,
            selected_site,
            reason: if selected_site.is_some() {
                "retained_site"
            } else {
                "awaiting_selection"
            },
            survey_rejected_by: None,
            checks: CandidateCheckCounts::default(),
        }
    }
}

pub(super) fn count(
    counts: &Cell<CandidateCheckCounts>,
    change: impl FnOnce(&mut CandidateCheckCounts),
) {
    let mut value = counts.get();
    change(&mut value);
    counts.set(value);
}
