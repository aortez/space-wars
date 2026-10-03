//! One ordinary-to-requested scheduling handoff, without extra dispatch work.
use super::*;

pub const COVERED_HANDOFF_PROFILE: &str = "covered_request_handoff_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CoveredRequestHandoff {
    pub tick: u64,
    pub site: LandingSiteId,
    pub previous_generation: u64,
    pub previous_request_tick: u64,
    pub previous_measurement_tick: u64,
    pub previous_graph: u64,
    pub previous_physics_queries: u64,
    /// Fresh measurements do not extend the replaced job's expiry boundary.
    pub deadline_tick: u64,
}

impl LiveObjectivePlanner {
    pub(super) fn covered_handoff_candidate(
        &self,
        state: &SurfaceSortieState,
        player: usize,
        o: &combat::TacticalSortieObservationV1,
        objective: LandingObjective,
    ) -> Option<CoveredRequestHandoff> {
        if !self.covered_handoff_players.contains(&player) {
            return None;
        }
        let p = &o.combat.recovery.flight.pilot;
        let LandingSiteQuery::Selected(site) = p.site_query else {
            return None;
        };
        let request = self.requests.get(&player)?;
        if request.query != LandingSiteQuery::Survey
            || p.tick <= request.tick
            || request.covered_handoff.is_some()
            || request.actual.is_some()
            || request.completed
            || request.published
            || request.partial_visible
            || p.landing.phase == LandingPhase::Landed
            || p.landing.supported_feet > 0
            || !p.controls_armed
            || !o.combat.recovery.flight.flight.enabled
            || p.sites.len() != 1
            || p.sites[0].id != site
            || site.planet != p.planet.index
            || p.sites[0].revision != p.planet.revision
            || !o
                .cover
                .iter()
                .any(|c| c.site == site && c.grounded && c.approach)
            || Self::valid(state, player, p, request, objective, true).is_err()
            || state.world.physics.world.collider_count() > MAX_SNAPSHOT_COLLIDERS
            || state.world.physics.world.body_count() > MAX_SNAPSHOT_BODIES
        {
            return None;
        }
        let job = self.queue.job(request.token)?;
        if job.output().is_some() || job.positive_candidates().is_some() || job.prioritizes(site) {
            return None;
        }
        Some(CoveredRequestHandoff {
            tick: p.tick,
            site,
            previous_generation: request.token.generation,
            previous_request_tick: request.tick,
            previous_measurement_tick: request.measurement_tick,
            previous_graph: request.graph,
            previous_physics_queries: request.physics_queries,
            deadline_tick: request
                .measurement_tick
                .saturating_add(MAX_SURVEY_AGE_TICKS),
        })
    }
}
