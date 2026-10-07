//! Search scheduling belongs to the task; the native surveys remain read-only.
use super::*;
use scenario_spacewars::surface_sortie::{
    ground_navigation::GROUND_SAMPLES,
    rebuild_placement::{MAX_REBUILD_WALK, RebuildRelocationSurvey, RebuildStagingProposal},
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RebuildStagingTelemetry {
    pub proposal: RebuildStagingProposal,
    pub started_tick: u64,
    pub search_since: u64,
    pub arrived_tick: Option<u64>,
    pub invalidated_tick: Option<u64>,
}

impl RebuildStagingTelemetry {
    pub(super) fn active(&self) -> bool {
        self.arrived_tick.is_none() && self.invalidated_tick.is_none()
    }
}

impl RecoverShipTask {
    /// Independently opt in to faster staging interiors and measured-map reuse.
    #[cfg(feature = "sensor-profile")]
    pub fn set_staging_execution(&mut self, continuous_walk: bool, route_handoff: bool) {
        self.continuous_staging = continuous_walk;
        self.staging_route_handoff = route_handoff;
    }

    /// Opt-in search scheduling for controlled native experiments.
    #[cfg(feature = "sensor-profile")]
    pub fn set_rebuild_search(&mut self, enabled: bool) {
        self.rebuild_search_enabled = enabled;
        if !enabled {
            self.telemetry.rebuild_search = None;
            self.invalidate_staging(self.previous_tick.unwrap_or(0));
        }
    }

    #[cfg(feature = "sensor-profile")]
    pub fn rebuild_search_request(&self) -> Option<RebuildSearchProgress> {
        self.rebuild_search_enabled.then(|| {
            let mut request = self.telemetry.rebuild_search.clone().unwrap_or_default();
            request.include_staging_map = self.staging_route_handoff;
            request.preferred = self
                .footing_recheck
                .map(|(_, bearing)| bearing)
                .or_else(|| {
                    self.staged_search_pending().then(|| {
                        self.telemetry
                            .rebuild_staging
                            .as_ref()
                            .unwrap()
                            .proposal
                            .target_bearing
                    })
                });
            request
        })
    }

    pub(super) fn staged_search_pending(&self) -> bool {
        self.rebuild_search_enabled
            && self.telemetry.status == TaskStatus::Running
            && self.relocation_missing_since.is_some()
            && self
                .telemetry
                .rebuild_staging
                .as_ref()
                .is_some_and(|s| s.arrived_tick.is_some() && s.invalidated_tick.is_none())
    }

    pub(super) fn remember_rebuild_search(
        &mut self,
        survey: &RebuildRelocationSurvey,
        p: &PilotObservationV1,
    ) -> bool {
        if !self.rebuild_search_enabled {
            return true;
        }
        if let Some(search) = &survey.search {
            if search.planet != p.planet.index
                || search.revision != p.planet.revision
                || search.started_tick > p.tick
                || !search.origin.x.is_finite()
                || !search.origin.y.is_finite()
                || search.visited.len() > GROUND_SAMPLES
                || search
                    .visited
                    .iter()
                    .any(|&id| usize::from(id) >= GROUND_SAMPLES)
            {
                self.block("invalid rebuild search history", p.tick);
                return false;
            }
            self.telemetry.rebuild_search = Some(search.clone());
        }
        true
    }

    pub(super) fn accept_staging(
        &mut self,
        survey: &RebuildRelocationSurvey,
        o: &RecoveryTaskObservationV1,
        since: u64,
    ) {
        let p = &o.flight.pilot;
        // One staged move per recovery, counted in the existing four moves.
        if self.telemetry.rebuild_staging.is_some() || self.telemetry.relocations >= 4 {
            return;
        }
        let Some(stage) = survey.staging else {
            return;
        };
        if stage.planet != p.planet.index
            || stage.revision != p.planet.revision
            || !stage.position.x.is_finite()
            || !stage.position.y.is_finite()
            || !stage.target_position.x.is_finite()
            || !stage.target_position.y.is_finite()
            || !(2.0..=4.0).contains(&stage.walk_length)
            || !(0.0..=MAX_REBUILD_WALK).contains(&stage.remaining_length)
            || !(0.0..=MAX_REBUILD_WALK).contains(&stage.hatch_walk_length)
            || usize::from(stage.target_bearing) >= GROUND_SAMPLES
        {
            self.block("invalid rebuild staging proposal", p.tick);
            return;
        }
        self.telemetry.relocations += 1;
        self.telemetry.rebuild_staging = Some(RebuildStagingTelemetry {
            proposal: stage,
            started_tick: p.tick,
            search_since: since,
            arrived_tick: None,
            invalidated_tick: None,
        });
        self.ground_task = None;
        if self.staging_route_handoff
            && let Some(map) = &survey.staging_map
            && let Some(mut task) =
                GroundNavigationTask::from_staging_map(self.context, stage.position, o, map)
        {
            task.set_continuous_walk(self.continuous_staging);
            self.telemetry.ground = Some(task.telemetry().clone());
            self.ground_task = Some(task);
        }
        // The missing-site timer keeps running during this walk and after
        // arrival. A preview or staging proposal cannot buy another five seconds.
    }

    pub(super) fn staging_deadline_exceeded(&self, tick: u64) -> bool {
        self.telemetry
            .rebuild_staging
            .as_ref()
            .is_some_and(|s| s.active() && tick.saturating_sub(s.search_since) > 5 * 60)
    }

    pub(super) fn invalidate_staging(&mut self, tick: u64) {
        if let Some(stage) = &mut self.telemetry.rebuild_staging
            && stage.active()
        {
            stage.invalidated_tick = Some(tick);
        }
    }

    pub(super) fn staging_destination(&self, p: &PilotObservationV1) -> Option<GroundDestination> {
        let stage = self
            .telemetry
            .rebuild_staging
            .as_ref()
            .filter(|s| s.active())?;
        let proposal = stage.proposal;
        (proposal.planet == p.planet.index
            && proposal.revision == p.planet.revision
            && p.planet
                .claim
                .as_ref()
                .is_some_and(|c| c.owner == Some(p.owner)))
        .then_some(GroundDestination::Rebuild {
            planet: proposal.planet,
            position: proposal.position,
        })
    }

    pub(super) fn traverse_staging(
        &mut self,
        o: &RecoveryTaskObservationV1,
    ) -> Option<SurfaceSortieAction> {
        self.telemetry
            .rebuild_staging
            .as_ref()
            .filter(|s| s.active())?;
        let p = &o.flight.pilot;
        let Some(destination) = self.staging_destination(p) else {
            self.invalidate_staging(p.tick);
            self.ground_task = None;
            self.telemetry.invalidations += 1;
            return None;
        };
        self.goal(RecoveryGoal::FindBuildSpace, p.tick);
        let action = self.traverse(o, destination);
        if self
            .telemetry
            .ground
            .as_ref()
            .is_some_and(|g| g.goal == GroundGoal::Arrived)
        {
            let stage = self.telemetry.rebuild_staging.as_mut().unwrap();
            stage.arrived_tick = Some(p.tick);
            if let Some(search) = &mut self.telemetry.rebuild_search {
                search.preferred = Some(stage.proposal.target_bearing);
            }
            self.ground_task = None;
            self.telemetry.last_progress_tick = p.tick;
        }
        Some(action)
    }
}
