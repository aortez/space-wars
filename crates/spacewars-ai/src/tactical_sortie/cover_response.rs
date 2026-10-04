//! Bounded, opt-in evidence acquisition after a witnessed cover failure or
//! an explicitly enabled initial approach qualification.
//! A requested site is a sensor request, never a retained landing permission.
use super::*;
use scenario_spacewars::surface_sortie::{
    landing_objective::LandingObjectiveSurvey, pilot::LandingSiteQuery,
};

pub const COVER_RESPONSE_PROFILE: &str = "qualified_cover_response_v1";
pub const COVER_SEARCH_TICKS: u64 = 10 * 60;
pub const MAX_COVER_PROBES: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoverSearch {
    pub planet: usize,
    pub revision: u64,
    pub objective: Option<LandingObjective>,
    pub started_tick: u64,
    pub deadline_tick: u64,
    pub hold_altitude: f32,
    pub seeded: bool,
    pub pending: Vec<LandingSiteId>,
    /// Unsuccessful or unsupported walking hypotheses remain unknown and
    /// eligible for later positive routes. They are not rejected sites.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub walk_deferred: Vec<LandingSiteId>,
    pub probes: usize,
    pub omitted: usize,
    pub finished_tick: Option<u64>,
    pub outcome: Option<&'static str>,
    pub guidance: Option<&'static str>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CoverResponse {
    pub required_since: Option<u64>,
    pub failures: u32,
    pub searches: u32,
    pub first_effect_tick: Option<u64>,
    pub filtered_directions: u64,
    pub requested_sites: u64,
    pub measured_sites: u64,
    pub selected_routes: u32,
    pub exhausted_searches: u32,
    pub budget_exhaustions: u32,
    pub deadlines: u32,
    pub exposure_releases: u32,
    pub search: Option<CoverSearch>,
}

/// The low-height exception is local to the actual native descent gate. A
/// distant site's negative projected height is not a sheltered approach.
pub(super) fn usable_cover(o: &TacticalSortieObservationV1, site: &PilotLandingSite) -> bool {
    let p = &o.combat.recovery.flight.pilot;
    let up = (p.ship.position - p.planet.motion.position).normalized();
    let direction = (site.vehicle_position - p.planet.motion.position).normalized();
    let tangent = Vec2::new(-up.y, up.x);
    let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
    o.cover.iter().any(|cover| {
        cover.site == site.id
            && cover.grounded
            && (cover.approach
                || (angle_between(up, direction).abs() < 0.2
                    && relative.dot(tangent).abs() < 18.0
                    && (p.ship.position - site.vehicle_position).dot(site.normal) < 40.0))
    })
}

impl TacticalSortiePilot {
    pub(crate) fn enable_cover_response(&mut self, enabled: bool) {
        assert!(enabled || self.telemetry.initial_cover.is_none());
        self.telemetry.cover_response = enabled.then(CoverResponse::default);
    }

    pub(super) fn arm_cover_response(&mut self, tick: u64) {
        if self.commit_descent
            && self.site.is_some()
            && let Some(state) = &mut self.telemetry.cover_response
        {
            state.failures += 1;
            state.required_since = Some(tick);
            state.search = None;
        }
    }

    pub(super) fn cover_required(&self, o: &TacticalSortieObservationV1, exposed: bool) -> bool {
        self.commit_descent
            && exposed
            && self.telemetry.cover_response.as_ref().is_some_and(|s| {
                s.required_since
                    .is_some_and(|tick| tick <= o.combat.recovery.flight.pilot.tick)
            })
    }

    pub(super) fn cover_probe_request(&self) -> Option<LandingSiteId> {
        let search = self.telemetry.cover_response.as_ref()?.search.as_ref()?;
        search
            .finished_tick
            .is_none()
            .then(|| {
                search
                    .pending
                    .first()
                    .or(search.walk_deferred.last())
                    .copied()
            })
            .flatten()
    }

    pub(super) fn record_cover_filter(&mut self, tick: u64, count: usize) {
        if count > 0
            && let Some(state) = &mut self.telemetry.cover_response
        {
            state.filtered_directions += count as u64;
            state.first_effect_tick.get_or_insert(tick);
        }
    }

    pub(super) fn finish_cover_search(&mut self, tick: u64, outcome: &'static str) {
        let Some(state) = &mut self.telemetry.cover_response else {
            return;
        };
        let Some(search) = state.search.as_mut().filter(|s| s.finished_tick.is_none()) else {
            return;
        };
        search.finished_tick = Some(tick);
        search.outcome = Some(outcome);
        search.guidance = None;
        match outcome {
            "selected_covered_route" => state.selected_routes += 1,
            "observed_candidates_exhausted" => state.exhausted_searches += 1,
            "evidence_budget_exhausted" => state.budget_exhaustions += 1,
            "evidence_deadline" => state.deadlines += 1,
            "exposure_cleared" => state.exposure_releases += 1,
            _ => {}
        }
    }

    pub(super) fn finish_selected_cover_search(
        &mut self,
        o: &TacticalSortieObservationV1,
        site: PilotLandingSite,
        survey: Option<&LandingObjectiveSurvey>,
    ) {
        if self.cover_probe_request() == Some(site.id)
            && survey.is_some_and(|s| s.sites.iter().any(|r| r.site == Some(site.id)))
        {
            self.telemetry
                .cover_response
                .as_mut()
                .unwrap()
                .measured_sites += 1;
        }
        self.finish_cover_search(
            o.combat.recovery.flight.pilot.tick,
            "selected_covered_route",
        );
    }

    pub(super) fn check_cover_search_deadline(&mut self, o: &TacticalSortieObservationV1) -> bool {
        if self.telemetry.failed_tick.is_some() || self.telemetry.completed_tick.is_some() {
            return false;
        }
        let p = &o.combat.recovery.flight.pilot;
        let expired =
            self.telemetry
                .cover_response
                .as_ref()
                .is_some_and(|state| match &state.search {
                    Some(s) => s.finished_tick.is_none() && p.tick >= s.deadline_tick,
                    None => state
                        .required_since
                        .is_some_and(|tick| p.tick >= tick.saturating_add(COVER_SEARCH_TICKS)),
                });
        if !expired
            || !p.controls_armed
            || !o.combat.recovery.flight.flight.enabled
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || p.location == PilotLocation::OnFoot
            || (p.queries_ready && !selection::exposed(o))
        {
            return false;
        }
        if self
            .telemetry
            .cover_response
            .as_ref()
            .unwrap()
            .search
            .is_none()
        {
            self.telemetry.cover_response.as_mut().unwrap().deadlines += 1;
        } else {
            self.finish_cover_search(p.tick, "evidence_deadline");
        }
        self.telemetry
            .cover_response
            .as_mut()
            .unwrap()
            .first_effect_tick
            .get_or_insert(p.tick);
        self.acquisition_reason("cover_evidence_deadline");
        self.abort(p.tick, "cover route evidence deadline exhausted");
        true
    }

    /// Drop obsolete requests without refreshing the deadline or probe budget.
    /// A full current observation must seed every new material/flag context.
    pub(super) fn prepare_cover_search(
        &mut self,
        o: &TacticalSortieObservationV1,
        objective: Option<LandingObjective>,
    ) -> bool {
        let p = &o.combat.recovery.flight.pilot;
        let pending = self.cover_probe_request().is_some();
        if !selection::exposed(o) {
            self.finish_cover_search(p.tick, "exposure_cleared");
            if let Some(state) = &mut self.telemetry.cover_response {
                state.required_since = None;
            }
            return pending;
        }
        if self.site.is_some() || !self.cover_required(o, true) {
            return false;
        }
        let state = self.telemetry.cover_response.as_mut().unwrap();
        if state
            .search
            .as_ref()
            .is_none_or(|s| s.finished_tick.is_some())
        {
            let started_tick = if state.search.is_none() {
                state.required_since.unwrap()
            } else {
                p.tick
            };
            state.searches += 1;
            state.search = Some(CoverSearch {
                planet: p.planet.index,
                revision: p.planet.revision,
                objective,
                started_tick,
                deadline_tick: started_tick.saturating_add(COVER_SEARCH_TICKS),
                hold_altitude: (p.ship.position.distance_to(p.planet.motion.position)
                    - p.planet.radius)
                    .clamp(60.0, 85.0),
                seeded: false,
                pending: Vec::new(),
                walk_deferred: Vec::new(),
                probes: 0,
                omitted: 0,
                finished_tick: None,
                outcome: None,
                guidance: None,
            });
        }
        let search = state.search.as_mut().unwrap();
        let same_objective = match (search.objective, objective) {
            (Some(a), Some(b)) => a.matches(b),
            (None, None) => true,
            _ => false,
        };
        if search.planet != p.planet.index
            || search.revision != p.planet.revision
            || !same_objective
        {
            search.planet = p.planet.index;
            search.revision = p.planet.revision;
            search.objective = objective;
            search.seeded = false;
            search.pending.clear();
            search.walk_deferred.clear();
            search.omitted = 0;
            return pending;
        }
        false
    }

    pub(super) fn search_cover_routes(
        &mut self,
        o: &TacticalSortieObservationV1,
        objective: Option<LandingObjective>,
        survey: Option<&LandingObjectiveSurvey>,
        unknown: &[LandingSiteId],
    ) -> Option<CombatIntent> {
        let p = &o.combat.recovery.flight.pilot;
        if !self.cover_required(o, selection::exposed(o)) {
            return None;
        }
        let initial_active = self.initial_cover_active();
        let state = self.telemetry.cover_response.as_mut().unwrap();
        let search = state.search.as_mut()?;
        state.first_effect_tick.get_or_insert(p.tick);
        if !search.seeded {
            let complete_scan = p.site_query == LandingSiteQuery::Survey
                || self
                    .required_site
                    .is_some_and(|id| p.site_query == LandingSiteQuery::Selected(id));
            if !complete_scan || (objective.is_some() && survey.is_none() && !initial_active) {
                self.acquisition_reason("cover_evidence_pending");
                return Some(self.wait_for_site(o, 12.0));
            }
            search.seeded = true;
            search.pending = unknown
                .iter()
                .copied()
                .take(MAX_COVER_PROBES - search.probes)
                .collect();
            search.omitted = unknown.len() - search.pending.len();
            if initial_active {
                self.telemetry
                    .initial_cover
                    .as_mut()
                    .unwrap()
                    .last_seed_tick = Some(p.tick);
            }
            if !search.pending.is_empty() {
                search.probes += 1;
                state.requested_sites += 1;
                if initial_active {
                    self.telemetry
                        .initial_cover
                        .as_mut()
                        .unwrap()
                        .requested(p.tick, search.pending[0]);
                }
            }
        } else if let Some(id) = search.pending.first().copied() {
            let deferred_walk = unknown.contains(&id) && walking_method_deferred(o, search, id);
            if p.site_query != LandingSiteQuery::Selected(id)
                || (unknown.contains(&id) && !deferred_walk)
            {
                self.acquisition_reason("cover_evidence_pending");
                return Some(self.wait_for_site(o, 12.0));
            }
            // Current cover/solar/material gates can disqualify a probe before
            // its route arrives. Missing or rejected survey data stays unknown.
            state.measured_sites +=
                u64::from(survey.is_some_and(|s| s.sites.iter().any(|r| r.site == Some(id))));
            if deferred_walk {
                search.walk_deferred.push(id);
            }
            search.pending.remove(0);
            if !search.pending.is_empty() {
                search.probes += 1;
                state.requested_sites += 1;
                if initial_active {
                    self.telemetry
                        .initial_cover
                        .as_mut()
                        .unwrap()
                        .requested(p.tick, search.pending[0]);
                }
            }
        }
        if search.pending.is_empty() {
            if !search.walk_deferred.is_empty() {
                // Leave the last requested site's full fallback running while
                // waiting for other route types. Do not spend another probe,
                // renew the deadline, or turn these unknowns into rejections.
                self.acquisition_reason("cover_walks_exhausted_waiting");
                return Some(self.wait_for_site(o, 12.0));
            }
            let (outcome, failure) = if search.omitted > 0 {
                (
                    "evidence_budget_exhausted",
                    "cover search probe budget exhausted with unmeasured candidates",
                )
            } else {
                (
                    "observed_candidates_exhausted",
                    "cover search exhausted its observed candidates",
                )
            };
            self.finish_cover_search(p.tick, outcome);
            self.acquisition_reason(outcome);
            self.abort(p.tick, failure);
            return Some(self.combat.intent(&o.combat));
        }
        self.acquisition_reason("cover_evidence_pending");
        Some(self.wait_for_site(o, 12.0))
    }

    pub(super) fn wait_for_cover_evidence(
        &mut self,
        o: &TacticalSortieObservationV1,
        speed: f32,
    ) -> Option<CombatIntent> {
        let p = &o.combat.recovery.flight.pilot;
        let state = self.telemetry.cover_response.as_mut()?;
        // Preserve the native wait for the first ordinary survey, including
        // successful covered retries. Hold locally only while probing omissions.
        let search = state
            .search
            .as_mut()
            .filter(|s| s.finished_tick.is_none() && s.seeded && s.planet == p.planet.index)?;
        state.first_effect_tick.get_or_insert(p.tick);
        let (desired, guidance) =
            acquisition_wait::waiting_velocity(o, search.hold_altitude, false, speed);
        search.guidance = Some(guidance);
        Some(self.guide(o, desired, Vec2::ZERO))
    }
}

fn walking_method_deferred(
    o: &TacticalSortieObservationV1,
    search: &CoverSearch,
    site: LandingSiteId,
) -> bool {
    use scenario_spacewars::surface_sortie::live_planning::{
        MAX_SURVEY_AGE_TICKS, ObjectiveWorkState,
    };
    let p = &o.combat.recovery.flight.pilot;
    let Some(e) = o.objective_evidence else {
        return false;
    };
    e.tick == p.tick
        && e.invalidated_by.is_none()
        && e.submission_deferred_by.is_none()
        && e.generation.is_some()
        && e.request_tick
            .is_some_and(|t| t >= search.started_tick && t <= p.tick)
        && e.measurement_tick.is_some_and(|t| {
            t >= search.started_tick
                && t <= p.tick
                && p.tick - t <= MAX_SURVEY_AGE_TICKS
                && e.request_tick.is_some_and(|requested| t <= requested)
        })
        && matches!(
            o.objective_work,
            Some(ObjectiveWorkState::Pending | ObjectiveWorkState::Ready)
        )
        && match (e.exhausted_walk, e.unsupported_walk) {
            (Some(a), None) => a.actor == p.owner && a.site == site,
            (None, Some(a)) => {
                a.actor == p.owner && a.site == site && a.required_steps > a.max_steps
            }
            _ => false,
        }
        && search.planet == p.planet.index
        && site.planet == search.planet
        && search.revision == p.planet.revision
        && search.objective.is_some_and(|target| {
            target.matches(e.objective)
                && e.source_objective.is_some_and(|old| target.matches(old))
                && LandingObjective::read(p).is_some_and(|now| target.matches(now))
        })
        && p.queries_ready
        && p.sites
            .iter()
            .any(|s| s.id == site && s.revision == p.planet.revision)
}

#[cfg(test)]
mod tests;
