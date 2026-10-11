//! One opt-in, measured attempt to regain standing ground before rebuild search.
use super::*;
use scenario_spacewars::surface_sortie::{
    ground_navigation::GroundMap,
    ground_posture::{GroundPostureObservation, SpacelingBalance},
};

const ATTEMPT_TICKS: u64 = 8 * 60;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RebuildGroundRejoinTelemetry {
    pub planet: usize,
    pub origin: Vec2,
    pub target: Vec2,
    pub started_tick: u64,
    pub deadline_tick: u64,
    pub search_since: u64,
    pub finished_tick: Option<u64>,
    pub rejoined_tick: Option<u64>,
    pub reason: Option<&'static str>,
}

fn current_map(o: &RecoveryTaskObservationV1) -> Option<&GroundMap> {
    let p = &o.flight.pilot;
    o.ground.as_ref().filter(|map| {
        map.version == 1
            && map.actor == p.owner
            && map.planet == p.planet.index
            && map.revision == p.planet.revision
            && map.tick == p.tick
    })
}

fn current_posture(o: &RecoveryTaskObservationV1) -> Option<&GroundPostureObservation> {
    let p = &o.flight.pilot;
    o.posture.as_ref().filter(|posture| {
        posture.version == 1
            && posture.owner == p.owner
            && posture.planet == p.planet.index
            && posture.revision == p.planet.revision
            && posture.tick == p.tick
            && posture.stable
            && posture.balance == SpacelingBalance::Balanced
    })
}

impl RecoverShipTask {
    /// Compare endpoint distance against direction persistence without granting
    /// a new attempt, changing its bounds or resetting controller memory.
    #[cfg(feature = "sensor-profile")]
    pub fn set_rebuild_rejoin_target_directed(&mut self, enabled: bool) {
        self.rebuild_rejoin_target_directed = enabled;
    }

    /// Continue the ordinary short corridor probes after upright clearance
    /// returns, only within the independently selected bounded rejoin attempt.
    #[cfg(feature = "sensor-profile")]
    pub fn set_rebuild_rejoin_corridors(&mut self, enabled: bool) {
        self.rebuild_rejoin_corridors = enabled;
    }

    #[cfg(feature = "terrain-recovery")]
    pub fn rebuild_rejoin_corridors_requested(&self, p: &PilotObservationV1) -> bool {
        self.rebuild_rejoin_corridors
            && self.rebuild_ground_rejoin_enabled
            && self.telemetry.status == TaskStatus::Running
            && !self.task_budget_exceeded(p.tick)
            && p.owner == self.context.actor
            && p.location == PilotLocation::OnFoot
            && p.controls_armed
            && p.queries_ready
            && !p.ship_available
            && p.balanced
            && p.supported_planet == Some(p.planet.index)
            && p.planet
                .claim
                .as_ref()
                .is_some_and(|claim| claim.owner == Some(p.owner))
            && self
                .telemetry
                .rebuild_ground_rejoin
                .as_ref()
                .is_some_and(|attempt| {
                    attempt.finished_tick.is_none()
                        && (attempt.started_tick..=attempt.deadline_tick).contains(&p.tick)
                        && p.planet.index == attempt.planet
                        && p.actor.is_some_and(|actor| {
                            (actor.position - p.planet.motion.position)
                                .rotate_radians(-p.planet.motion.angle)
                                .distance_to(attempt.origin)
                                <= 6.0
                        })
                })
    }

    /// Independently selected for retained-state experiments; never resets a
    /// blocked task or the original overall recovery clock.
    #[cfg(feature = "sensor-profile")]
    pub fn set_rebuild_ground_rejoin(&mut self, enabled: bool) {
        self.rebuild_ground_rejoin_enabled = enabled;
        if !enabled
            && self.finish_rebuild_ground_rejoin(self.previous_tick.unwrap_or(0), "disabled")
        {
            self.telemetry.ground = None;
        }
    }

    pub(super) fn start_rebuild_ground_rejoin(
        &mut self,
        o: &RecoveryTaskObservationV1,
        search_since: u64,
    ) -> bool {
        let p = &o.flight.pilot;
        if !self.rebuild_ground_rejoin_enabled
            || self.telemetry.rebuild_ground_rejoin.is_some()
            || self.telemetry.relocation_site.is_some()
            || self.staging_destination(p).is_some()
            || p.tick.saturating_sub(search_since) > 5 * 60
            || !p.queries_ready
            || !p.balanced
            || p.supported_planet != Some(p.planet.index)
        {
            return false;
        }
        let (Some(actor), Some(map), Some(posture)) = (p.actor, current_map(o), current_posture(o))
        else {
            return false;
        };
        if posture.standing_clear || posture.crawl.iter().all(Option::is_none) {
            return false;
        }
        let local =
            |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let foot = local(actor.position - p.actor_up * HALF_HEIGHT);
        let Some(node) = map.nodes.iter().min_by(|a, b| {
            a.position
                .distance_to(foot)
                .total_cmp(&b.position.distance_to(foot))
        }) else {
            return false;
        };
        if !(3.0..=8.0).contains(&node.position.distance_to(foot)) {
            return false;
        }
        self.telemetry.rebuild_ground_rejoin = Some(RebuildGroundRejoinTelemetry {
            planet: p.planet.index,
            origin: local(actor.position),
            target: node.position,
            started_tick: p.tick,
            deadline_tick: p.tick.saturating_add(ATTEMPT_TICKS),
            search_since,
            finished_tick: None,
            rejoined_tick: None,
            reason: None,
        });
        self.ground_task = None;
        true
    }

    pub(super) fn finish_rebuild_ground_rejoin(&mut self, tick: u64, reason: &'static str) -> bool {
        let Some(attempt) = &mut self.telemetry.rebuild_ground_rejoin else {
            return false;
        };
        if attempt.finished_tick.is_some() {
            return false;
        }
        attempt.finished_tick = Some(tick);
        attempt.reason = Some(reason);
        self.ground_task = None;
        true
    }

    pub(super) fn check_rebuild_ground_rejoin_limits(&mut self, p: &PilotObservationV1) {
        let Some(attempt) = &self.telemetry.rebuild_ground_rejoin else {
            return;
        };
        if attempt.finished_tick.is_some() {
            return;
        }
        if p.planet.index != attempt.planet || p.actor.is_none() {
            self.block(
                "rebuild ground rejoin lost its retained planet or actor",
                p.tick,
            );
        } else if p.tick > attempt.deadline_tick
            || (p.actor.unwrap().position - p.planet.motion.position)
                .rotate_radians(-p.planet.motion.angle)
                .distance_to(attempt.origin)
                > 6.0
        {
            self.block(
                "rebuild ground rejoin exceeded eight seconds or six units",
                p.tick,
            );
        }
    }

    pub(super) fn advance_rebuild_ground_rejoin(
        &mut self,
        o: &RecoveryTaskObservationV1,
    ) -> Option<SurfaceSortieAction> {
        let attempt = self.telemetry.rebuild_ground_rejoin.as_ref()?;
        if attempt.finished_tick.is_some() {
            return None;
        }
        let p = &o.flight.pilot;
        let local =
            |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let actor = p.actor.unwrap();
        let foot = local(actor.position - p.actor_up * HALF_HEIGHT);
        let destination = GroundDestination::Rebuild {
            planet: attempt.planet,
            position: attempt.target,
        };
        let search_since = attempt.search_since;
        let started_tick = attempt.started_tick;
        self.goal(RecoveryGoal::FindBuildSpace, p.tick);
        // Let the existing ground controller validate the complete map and
        // posture before accepting a successful rejoin from these observations.
        let action = self.traverse(o, destination);
        if self.telemetry.status == TaskStatus::Blocked {
            return Some(SurfaceSortieAction::default());
        }
        if p.queries_ready
            && p.balanced
            && p.supported_planet == Some(p.planet.index)
            && current_posture(o).is_some_and(|posture| posture.standing_clear)
            && current_map(o).is_some_and(|map| {
                map.nodes
                    .iter()
                    .any(|node| node.position.distance_to(foot) < 3.0)
            })
        {
            // Pause only the missing-site timer for this single bounded move.
            // Preserve its remaining time, every retry count and the task clock.
            self.relocation_missing_since =
                Some(search_since.saturating_add(p.tick - started_tick));
            self.telemetry
                .rebuild_ground_rejoin
                .as_mut()
                .unwrap()
                .rejoined_tick = Some(p.tick);
            self.finish_rebuild_ground_rejoin(p.tick, "supported standing ground regained");
            self.telemetry.ground = None;
            self.telemetry.last_progress_tick = p.tick;
            return Some(SurfaceSortieAction::default());
        }
        // This attempt buys no jump, jetpack or trip to a speculative build
        // site. Reuse only the existing short measured ground-rejoin controls.
        Some(
            if self
                .telemetry
                .ground
                .as_ref()
                .is_some_and(|g| g.goal == GroundGoal::Rejoin)
            {
                action
            } else {
                SurfaceSortieAction::default()
            },
        )
    }
}

#[cfg(all(test, feature = "sensor-profile"))]
mod tests;
