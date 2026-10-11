//! Opt-in host adapter for the ordinary mission-owned recovery task.
use super::*;
use crate::mission_evaluation::MissionEvaluator;
use scenario_spacewars::surface_sortie::{SurfaceSortieState, mission::LandingSurveyCadence};

impl MaterialMissionPilot {
    pub(crate) fn configure_terrain_recovery(&mut self, enabled: bool) {
        assert!(
            self.previous_tick.is_none(),
            "configure before the first intent"
        );
        self.terrain_recovery = enabled;
        self.native_rebuild_refinement = enabled;
        self.configure_recovery_task();
    }

    pub(crate) fn configure_native_rebuild_refinement(&mut self, enabled: bool) {
        assert!(
            self.previous_tick.is_none(),
            "configure before the first intent"
        );
        assert!(
            !enabled || self.terrain_recovery,
            "native refinement requires terrain recovery"
        );
        self.native_rebuild_refinement = enabled;
    }

    pub(super) fn configure_recovery_task(&mut self) {
        if let Some(task) = &mut self.recovery {
            task.set_hatch_supported_arrival(self.hatch_supported_arrival);
            task.set_pod_reconsideration(self.terrain_recovery);
            task.set_rebuild_search(self.terrain_recovery);
            task.set_staging_execution(self.terrain_recovery, self.terrain_recovery);
            task.set_rebuild_precise_arrival(self.terrain_recovery);
        }
    }

    /// Independently selectable on the current and future recovery tasks.
    /// Preserves cached decisions, route memory and all existing task budgets.
    pub fn set_hatch_supported_arrival(&mut self, enabled: bool) {
        self.hatch_supported_arrival = enabled;
        if let Some(task) = &mut self.recovery {
            task.set_hatch_supported_arrival(enabled);
        }
    }

    pub fn terrain_recovery_active(&self) -> bool {
        self.terrain_recovery && self.recovery.is_some()
    }

    /// Physical-step selection; enhanced recovery observations remain independent.
    pub fn native_rebuild_refinement_active(&self) -> bool {
        self.native_rebuild_refinement && self.terrain_recovery_active()
    }

    /// Host-only pairing: one cadenced observation, the unchanged mission intent
    /// entry point, then native option synchronization before the physical step.
    /// Returned observations also feed the host's usual post-intent planners.
    pub fn decide_with_terrain_recovery(
        &mut self,
        state: &mut SurfaceSortieState,
        cadence: LandingSurveyCadence,
        evaluation: &MissionEvaluator,
    ) -> (MissionObservationV1, CombatIntent) {
        let seat = self.context.actor.index();
        let active = self.terrain_recovery_active();
        assert!(state.set_rebuild_refinement(seat, active));
        let search = self
            .recovery
            .as_ref()
            .and_then(RecoverShipTask::rebuild_search_request);
        let request = self.sensor_request();
        let mut observation = if active && let Some(search) = search {
            state.mission_observation_with_rebuild_search(seat, request, cadence, &search)
        } else {
            state.mission_observation_with_cadence(seat, request, cadence)
        };
        if active {
            state.add_terrain_flight_forecast(seat, &mut observation.local.combat.recovery);
            let recovery = &mut observation.local.combat.recovery;
            if self
                .recovery
                .as_ref()
                .is_some_and(|task| task.rebuild_rejoin_corridors_requested(&recovery.flight.pilot))
            {
                state.add_ground_rejoin_corridors(seat, recovery);
            }
        }
        let intent = self.intent_with_evaluation(&observation, evaluation);
        assert!(state.set_rebuild_refinement(seat, self.native_rebuild_refinement_active()));
        (observation, intent)
    }
}

#[cfg(test)]
mod qualification;
#[cfg(test)]
mod tests;
