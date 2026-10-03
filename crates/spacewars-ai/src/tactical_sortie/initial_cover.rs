//! Qualify the first airborne capture choice using current cover, and request
//! its native route before the ordinary shortlist publishes an exposed one.
use super::*;
use scenario_spacewars::surface_sortie::LandingPhase;

pub const INITIAL_COVER_PROFILE: &str = "initial_qualified_cover_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct InitialCoverRequest {
    pub tick: u64,
    pub site: LandingSiteId,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct InitialCover {
    pub armed_tick: Option<u64>,
    pub finished_tick: Option<u64>,
    pub outcome: Option<&'static str>,
    pub selected_site: Option<LandingSiteId>,
    pub selected_exposed: Option<bool>,
    pub last_seed_tick: Option<u64>,
    pub requests: u32,
    pub last_request: Option<InitialCoverRequest>,
}

impl InitialCover {
    pub(super) fn requested(&mut self, tick: u64, site: LandingSiteId) {
        self.requests += 1;
        self.last_request = Some(InitialCoverRequest { tick, site });
    }
}

impl TacticalSortiePilot {
    pub(crate) fn enable_initial_cover(&mut self, enabled: bool) {
        assert!(!enabled || (self.commit_descent && self.telemetry.cover_response.is_some()));
        self.telemetry.initial_cover = enabled.then(InitialCover::default);
    }

    pub(super) fn initial_cover_active(&self) -> bool {
        self.telemetry
            .initial_cover
            .as_ref()
            .is_some_and(|s| s.armed_tick.is_some() && s.finished_tick.is_none())
    }

    pub(super) fn finish_initial_cover(
        &mut self,
        tick: u64,
        outcome: &'static str,
        site: Option<LandingSiteId>,
        exposed: Option<bool>,
    ) {
        if let Some(initial) = &mut self.telemetry.initial_cover
            && initial.finished_tick.is_none()
        {
            initial.finished_tick = Some(tick);
            initial.outcome = Some(outcome);
            initial.selected_site = site;
            initial.selected_exposed = exposed;
        }
    }

    pub(super) fn prepare_initial_cover(&mut self, o: &TacticalSortieObservationV1) {
        let p = &o.combat.recovery.flight.pilot;
        let Some(initial) = &mut self.telemetry.initial_cover else {
            return;
        };
        if initial.finished_tick.is_some()
            || self.site.is_some()
            || self.telemetry.failed_tick.is_some()
            || self.telemetry.completed_tick.is_some()
            || !p.controls_armed
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || !o.combat.recovery.flight.flight.enabled
        {
            return;
        }
        // This is an airborne entry gate. Physical contact, hatch access and
        // on-foot recovery retain their existing controller priorities.
        if p.location == PilotLocation::OnFoot
            || p.landing.supported_feet > 0
            || p.landing.phase == LandingPhase::Landed
        {
            let armed = initial.armed_tick.is_some();
            self.finish_initial_cover(p.tick, "physical_surface", None, None);
            if armed {
                self.finish_cover_search(p.tick, "physical_surface");
                self.telemetry
                    .cover_response
                    .as_mut()
                    .unwrap()
                    .required_since = None;
            }
            return;
        }
        if !p.queries_ready || !selection::exposed(o) {
            return;
        }
        let started = *initial.armed_tick.get_or_insert(p.tick);
        let state = self.telemetry.cover_response.as_mut().unwrap();
        // Initial admission is not a witnessed rejection. Do not increment
        // failures or erase the original deadline when exposure returns.
        state.required_since = Some(started);
        if let Some(search) = &mut state.search
            && search.outcome == Some("exposure_cleared")
        {
            search.finished_tick = None;
            search.outcome = None;
            search.guidance = None;
            search.seeded = false;
            search.pending.clear();
            search.walk_deferred.clear();
            search.omitted = 0;
        }
    }
}
