//! Opt-in destination preference after a witnessed, unclaimed capture failure.
//! Memory orders retries; it never certifies a route or permits surface actions.
use super::*;
use crate::tactical_sortie::{AcquisitionTelemetry, CoverSearch};
use scenario_spacewars::surface_sortie::landing_objective::LandingObjective;

pub const DESTINATION_RETRY_PROFILE: &str = "destination_failure_context_v1";
pub(super) const RETRY_PREFERENCE: &str = "prefer another destination after capture failure";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationFailureKind {
    IncompleteSearch,
    ExecutionLimit,
    UnclassifiedFailure,
    ObservedCoverConstraints,
    ObservedGroundRouteFailure,
}

impl DestinationFailureKind {
    fn priority(self) -> u8 {
        match self {
            Self::IncompleteSearch | Self::ExecutionLimit | Self::UnclassifiedFailure => 1,
            Self::ObservedCoverConstraints | Self::ObservedGroundRouteFailure => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DestinationFailureContext {
    pub planet: usize,
    pub revision: u64,
    pub owner: Option<PlayerId>,
    pub flag: Option<LandingObjective>,
}

impl DestinationFailureContext {
    fn read(planet: &PilotPlanetObservation) -> Self {
        Self {
            planet: planet.index,
            revision: planet.revision,
            owner: planet.claim.as_ref().and_then(|c| c.owner),
            flag: planet.claim.as_ref().and_then(|c| {
                c.flag.map(|flag| LandingObjective {
                    planet: planet.index,
                    revision: planet.revision,
                    owner: flag.player,
                    position: (flag.position - planet.motion.position)
                        .rotate_radians(-planet.motion.angle),
                    range: c.flag_interaction_range - 0.2,
                })
            }),
        }
    }

    fn matches(&self, planet: &PilotPlanetObservation) -> bool {
        let current = Self::read(planet);
        self.planet == current.planet
            && self.revision == current.revision
            && self.owner == current.owner
            && match (self.flag, current.flag) {
                (None, None) => true,
                (Some(a), Some(b)) => a.matches(b),
                _ => false,
            }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DestinationFailure {
    pub context: DestinationFailureContext,
    pub selected_tick: u64,
    pub started_tick: u64,
    pub failed_tick: u64,
    pub reason: &'static str,
    pub kind: DestinationFailureKind,
    pub site: Option<LandingSiteId>,
    pub acquisition: Option<AcquisitionTelemetry>,
    pub cover_search: Option<CoverSearch>,
    pub cover_replans: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationSelectionPath {
    Initial,
    Switch,
    Probe,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DestinationRetryDecision {
    pub tick: u64,
    pub path: DestinationSelectionPath,
    pub rejected: usize,
    pub retained: Option<usize>,
    pub failure: DestinationFailure,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DestinationRetryAdmission {
    pub tick: u64,
    pub path: DestinationSelectionPath,
    pub failure: DestinationFailure,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DestinationRetryTelemetry {
    /// At most one current failure per observed planet. Old contexts expire
    /// on identity changes, not on a timer or a newly published cost forecast.
    pub failures: Vec<DestinationFailure>,
    pub recorded_failures: u32,
    pub context_invalidations: u32,
    pub initial_changes: u64,
    pub switch_rejections: u64,
    pub probe_rejections: u64,
    pub retry_selections: u64,
    pub first_effect_tick: Option<u64>,
    pub last_decision: Option<DestinationRetryDecision>,
    pub last_retry: Option<DestinationRetryAdmission>,
}

impl MaterialMissionPilot {
    pub(crate) fn enable_destination_retry(&mut self, enabled: bool) {
        self.telemetry.destination_retry = enabled.then(DestinationRetryTelemetry::default);
    }

    pub(super) fn refresh_destination_failures(&mut self, o: &MissionObservationV1) {
        let Some(memory) = &mut self.telemetry.destination_retry else {
            return;
        };
        let owner = o.local.combat.recovery.flight.pilot.owner;
        memory.failures.retain(|failure| {
            let keep = o.planets.iter().any(|planet| {
                failure.context.matches(planet)
                    && planet.claim.as_ref().is_none_or(|c| c.owner != Some(owner))
            });
            if !keep {
                memory.context_invalidations += 1;
            }
            keep
        });
    }

    pub(super) fn remember_capture_failure(
        &mut self,
        o: &MissionObservationV1,
        capture: &CaptureTelemetry,
    ) {
        let Some(memory) = &mut self.telemetry.destination_retry else {
            return;
        };
        let p = &o.local.combat.recovery.flight.pilot;
        let (Some(reason), Some(started), Some(failed)) =
            (capture.failure, capture.started_tick, capture.failed_tick)
        else {
            return;
        };
        // Bind the failure to its actual observation, before reconsideration
        // drops the task. Never attach a previous tick's failure to new terrain.
        if failed != p.tick
            || started < self.selected_tick
            || started > failed
            || self.telemetry.target != Some(p.planet.index)
            || !o.planets.iter().any(|planet| planet == &p.planet)
            || capture.landing.claimed_tick.is_some()
            || memory
                .failures
                .iter()
                .any(|f| f.context.planet == p.planet.index && f.failed_tick == failed)
        {
            return;
        }
        let kind = match reason {
            "cover search probe budget exhausted with unmeasured candidates"
            | "cover route evidence deadline exhausted" => DestinationFailureKind::IncompleteSearch,
            "cover search exhausted its observed candidates" => {
                DestinationFailureKind::ObservedCoverConstraints
            }
            "no complete flag round trip in fresh surveys" => {
                DestinationFailureKind::ObservedGroundRouteFailure
            }
            "capture approach exhausted its time or retry budget"
            | "ground traversal exceeded ninety seconds" => DestinationFailureKind::ExecutionLimit,
            _ => DestinationFailureKind::UnclassifiedFailure,
        };
        let failure = DestinationFailure {
            context: DestinationFailureContext::read(&p.planet),
            selected_tick: self.selected_tick,
            started_tick: started,
            failed_tick: failed,
            reason,
            kind,
            site: capture.site,
            acquisition: capture.acquisition,
            cover_search: capture
                .cover_response
                .as_ref()
                .and_then(|r| r.search.clone()),
            cover_replans: capture.cover_replans,
        };
        memory
            .failures
            .retain(|f| f.context.planet != p.planet.index);
        memory.failures.push(failure);
        memory.recorded_failures += 1;
    }

    fn destination_failure(
        &self,
        o: &MissionObservationV1,
        destination: usize,
    ) -> Option<&DestinationFailure> {
        let planet = o.planets.iter().find(|p| p.index == destination)?;
        self.telemetry
            .destination_retry
            .as_ref()?
            .failures
            .iter()
            .find(|f| f.context.matches(planet))
    }

    fn retry_priority(&self, o: &MissionObservationV1, destination: usize) -> (u8, u64) {
        self.destination_failure(o, destination)
            .map_or((0, 0), |f| (f.kind.priority(), f.failed_tick))
    }

    pub(super) fn destination_retry_admitted(
        &self,
        o: &MissionObservationV1,
        destination: usize,
    ) -> bool {
        if self.telemetry.destination_retry.is_none() {
            return true;
        }
        let p = &o.local.combat.recovery.flight.pilot;
        let priority = self.retry_priority(o, destination);
        // Try an eligible destination without this failure first. If all have
        // failed, incomplete evidence precedes observed constraints, oldest
        // failure first within each class. Native cooldowns still apply.
        !o.planets.iter().any(|planet| {
            planet
                .claim
                .as_ref()
                .is_none_or(|c| c.owner != Some(p.owner))
                && !self
                    .deferred
                    .iter()
                    .any(|(index, until)| *index == planet.index && p.tick < *until)
                && self.retry_priority(o, planet.index) < priority
        })
    }

    pub(super) fn record_destination_retry_rejection(
        &mut self,
        o: &MissionObservationV1,
        path: DestinationSelectionPath,
        rejected: usize,
        retained: Option<usize>,
    ) {
        let Some(failure) = self.destination_failure(o, rejected).cloned() else {
            return;
        };
        let tick = o.local.combat.recovery.flight.pilot.tick;
        let memory = self.telemetry.destination_retry.as_mut().unwrap();
        match path {
            DestinationSelectionPath::Initial => memory.initial_changes += 1,
            DestinationSelectionPath::Switch => memory.switch_rejections += 1,
            DestinationSelectionPath::Probe => memory.probe_rejections += 1,
        }
        memory.first_effect_tick.get_or_insert(tick);
        memory.last_decision = Some(DestinationRetryDecision {
            tick,
            path,
            rejected,
            retained,
            failure,
        });
    }

    pub(super) fn record_destination_retry_admission(
        &mut self,
        o: &MissionObservationV1,
        path: DestinationSelectionPath,
        destination: usize,
    ) {
        let Some(failure) = self.destination_failure(o, destination).cloned() else {
            return;
        };
        let memory = self.telemetry.destination_retry.as_mut().unwrap();
        memory.retry_selections += 1;
        memory.last_retry = Some(DestinationRetryAdmission {
            tick: o.local.combat.recovery.flight.pilot.tick,
            path,
            failure,
        });
    }
}

#[cfg(test)]
#[path = "mission_destination_retry_tests.rs"]
mod tests;
