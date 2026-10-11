//! Explicit opt-in driver for the retained search/arrival recovery candidate.
//! Normal task creation and stepping own retries; no controller is replaced.
use super::*;
use scenario_spacewars::surface_sortie::SurfaceSortieState;

impl RulePilotV4 {
    pub fn set_terrain_recovery(&mut self, enabled: bool) {
        self.terrain_recovery = enabled;
        self.configure_terrain_recovery();
    }

    pub(super) fn configure_terrain_recovery(&mut self) {
        if let Some(task) = &mut self.task {
            task.set_pod_reconsideration(self.terrain_recovery);
            task.set_rebuild_search(self.terrain_recovery);
            task.set_staging_execution(self.terrain_recovery, self.terrain_recovery);
            task.set_rebuild_precise_arrival(self.terrain_recovery);
            self.telemetry.recovery = Some(task.telemetry().clone());
        }
    }

    /// Pair with `intent` for this explicitly selected profile. One recovery
    /// survey per decision; native refinement ends with the recovery task.
    pub fn observe_with_terrain_recovery(
        &self,
        state: &mut SurfaceSortieState,
    ) -> CombatObservationV2 {
        let seat = self.context.actor.index();
        let enabled = self.terrain_recovery && self.task.is_some();
        assert!(state.set_rebuild_refinement(seat, enabled));
        let search = self
            .task
            .as_ref()
            .and_then(RecoverShipTask::rebuild_search_request);
        let mut observation = if enabled && let Some(search) = search {
            state.combat_observation_with_rebuild_search(seat, self.site_request().into(), &search)
        } else {
            state.combat_observation(seat, self.site_request())
        };
        if enabled {
            state.add_terrain_flight_forecast(seat, &mut observation.recovery);
        }
        observation
    }
}
