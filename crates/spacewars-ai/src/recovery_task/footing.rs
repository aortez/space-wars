//! Keep an arrived, measured build footing while native construction proceeds.
use super::*;

const HOLD_TICKS: u64 = 20 * 60;
const MAX_HOLD_DISPLACEMENT: f32 = 2.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RebuildFootingTelemetry {
    pub site: RebuildStandingSite,
    pub bearing: u16,
    pub started_tick: u64,
    pub ended_tick: Option<u64>,
    pub reason: Option<&'static str>,
}

impl RebuildFootingTelemetry {
    pub(super) fn active(&self) -> bool {
        self.ended_tick.is_none()
    }
}

impl RecoverShipTask {
    pub(super) fn remember_footing_bearing(
        &mut self,
        survey: &scenario_spacewars::surface_sortie::rebuild_placement::RebuildRelocationSurvey,
        site: RebuildStandingSite,
    ) {
        self.selected_footing_bearing = if self.rebuild_footing_hold && site.precise {
            survey
                .attempts
                .iter()
                .find(|a| {
                    usize::from(a.bearing)
                        < scenario_spacewars::surface_sortie::ground_navigation::GROUND_SAMPLES
                        && a.placement.as_ref().is_some_and(|report| {
                            report.tick == survey.tick
                                && report.planet == site.planet
                                && report.revision == Some(site.revision)
                                && report.selected_offset.is_some()
                                && report.standing.distance_to(site.position) <= 0.01
                        })
                })
                .map(|a| a.bearing)
        } else {
            None
        };
    }

    #[cfg(feature = "sensor-profile")]
    pub fn set_rebuild_footing_hold(&mut self, enabled: bool) {
        self.rebuild_footing_hold = enabled;
        if !enabled {
            self.release_rebuild_footing("holding disabled", self.previous_tick.unwrap_or(0));
            self.footing_recheck = None;
        }
    }

    pub(super) fn retain_rebuild_footing(
        &mut self,
        site: RebuildStandingSite,
        o: &RecoveryTaskObservationV1,
    ) -> bool {
        if !self.rebuild_footing_hold || !site.precise {
            return false;
        }
        let bearing = self
            .ground_task
            .as_mut()
            .and_then(|task| task.retain_arrived_rebuild())
            .or(self.selected_footing_bearing);
        let Some(bearing) = bearing else {
            return false;
        };
        self.telemetry.rebuild_footing = Some(RebuildFootingTelemetry {
            site,
            bearing,
            started_tick: o.flight.pilot.tick,
            ended_tick: None,
            reason: None,
        });
        self.telemetry.ground = self
            .ground_task
            .as_ref()
            .map(|task| task.telemetry().clone());
        true
    }

    pub(super) fn release_rebuild_footing(&mut self, reason: &'static str, tick: u64) {
        if let Some(footing) = &mut self.telemetry.rebuild_footing
            && footing.active()
        {
            footing.ended_tick = Some(tick);
            footing.reason = Some(reason);
            self.ground_task = None;
        }
    }

    pub(super) fn check_footing_deadline(&mut self, tick: u64) {
        if self.telemetry.status == TaskStatus::Running
            && self
                .telemetry
                .rebuild_footing
                .as_ref()
                .is_some_and(|f| f.active() && tick.saturating_sub(f.started_tick) > HOLD_TICKS)
        {
            self.release_rebuild_footing("holding deadline exceeded", tick);
            self.block("rebuild footing exceeded twenty seconds", tick);
        }
    }

    pub(super) fn hold_rebuild_footing(
        &mut self,
        o: &RecoveryTaskObservationV1,
    ) -> Option<SurfaceSortieAction> {
        let held = self
            .telemetry
            .rebuild_footing
            .as_ref()
            .filter(|f| f.active())?;
        let site = held.site;
        let bearing = held.bearing;
        let p = &o.flight.pilot;
        if site.planet != p.planet.index {
            self.release_rebuild_footing("planet changed", p.tick);
            return None;
        }
        if matches!(
            p.recovery.as_ref().unwrap().status,
            SurfaceRecoveryStatus::ClearanceBlocked | SurfaceRecoveryStatus::HatchBlocked
        ) {
            self.release_rebuild_footing("native placement rejected", p.tick);
            return None;
        }
        let distance = p.actor.map(|actor| {
            let foot = (actor.position - p.actor_up * HALF_HEIGHT - p.planet.motion.position)
                .rotate_radians(-p.planet.motion.angle);
            foot.distance_to(site.position)
        });
        let invalidation = if site.revision != p.planet.revision {
            Some("terrain changed")
        } else if distance.is_none_or(|d| !d.is_finite() || d > MAX_HOLD_DISPLACEMENT) {
            Some("footing displaced")
        } else {
            None
        };
        if let Some(reason) = invalidation {
            self.release_rebuild_footing(reason, p.tick);
            // The old bearing is only a query hint. A current native preview
            // and another counted relocation are required, even for this spot.
            self.footing_recheck = Some((site.planet, bearing));
            self.relocation_missing_since.get_or_insert(p.tick);
            self.telemetry.invalidations += 1;
            return None;
        }
        self.goal(RecoveryGoal::Rebuild, p.tick);
        if p.supported_planet != Some(site.planet) {
            return Some(SurfaceSortieAction::default());
        }
        // Keep the same ground task and its clocks, measured route, precise
        // arrival and posture handling. This cannot authorize a native build.
        Some(self.traverse(
            o,
            GroundDestination::Rebuild {
                planet: site.planet,
                position: site.position,
            },
        ))
    }
}
