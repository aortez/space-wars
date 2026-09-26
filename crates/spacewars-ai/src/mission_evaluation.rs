//! Bounded capture comparisons. No controller or mutable world is held.
//! v12/v13 can consume validated reports; earlier policies remain observational.
//! One charged step evaluates one candidate; a final step compares at most three.
use crate::mission_pilot::MissionTelemetry;
use engine_core::{
    Vec2,
    planning::{JobLimits, JobPhase, PlanningJob, PlanningQueue, RequestToken, Work, WorkKind},
};
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        PilotLocation,
        mission::{MissionMatchContext, MissionObservationV1},
        pilot::{LandingSiteId, PilotPlanetObservation},
    },
};
use serde::Serialize;
use std::collections::BTreeMap;

mod flag_survey;
mod model;
mod selection;
mod survey;
mod transfer;
mod value;
use model::{LocalEvidence, PlanetKey};
pub(crate) use selection::CaptureSelection;
pub use transfer::{TransferReference, TransferSource};
pub use value::{CaptureValue, ValueComparison, ValueDecision};
#[cfg(test)]
mod survey_tests;
#[cfg(test)]
mod tests;

pub const MODEL: &str = "capture_mission_reference_v1";
pub fn model_for_policy(policy: &str) -> &'static str {
    if value::enabled(policy) {
        value::MODEL
    } else {
        MODEL
    }
}
pub const MAX_PLANETS: usize = 8;
pub const MAX_OPTIONS: usize = 3;
pub const REFRESH_TICKS: u64 = 60;
pub const MAX_EVIDENCE_AGE: u64 = 30 * 60;
pub const MAX_RESULT_AGE: u64 = 120;
pub const DEFAULT_WORK: Work = Work {
    graph: 4,
    physics_queries: 0,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PhaseCosts {
    pub landing: f32,
    pub exit: f32,
    pub outbound: f32,
    pub claim: f32,
    pub return_board: f32,
    pub departure: f32,
}
impl PhaseCosts {
    fn total(&self) -> f32 {
        self.landing + self.exit + self.outbound + self.claim + self.return_board + self.departure
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureCandidate {
    pub planet: usize,
    pub current: bool,
    pub revision: u64,
    pub observed_owner: Option<PlayerId>,
    pub ownership_known: bool,
    pub adds_ownership: bool,
    pub first_rebuild_foothold: bool,
    pub distance: f32,
    /// Nominal v12 travel or staged v13 reference; neither models opposition.
    pub travel_seconds: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer: Option<TransferReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<CaptureValue>,
    pub site: Option<LandingSiteId>,
    pub evidence_tick: Option<u64>,
    pub evidence_age_ticks: Option<u64>,
    pub route_source_tick: Option<u64>,
    pub route_validated_tick: Option<u64>,
    pub evidence_kind: &'static str,
    pub local: Option<PhaseCosts>,
    pub unknown_reason: Option<&'static str>,
    /// Conditional on successful execution, never a completion guarantee.
    pub total_seconds: Option<f32>,
    pub reference_exceeds_match_time: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissionEvaluation {
    pub model: &'static str,
    pub policy: &'static str,
    pub actor: PlayerId,
    pub source_tick: u64,
    pub completed_tick: Option<u64>,
    pub current_target: Option<usize>,
    pub selected_tick: Option<u64>,
    pub match_context: Option<MissionMatchContext>,
    pub ship_health: f32,
    pub ship_form: ShipForm,
    pub location: PilotLocation,
    pub opponent_distance: Option<f32>,
    pub combat_risk: &'static str,
    pub inactive_reason: Option<&'static str>,
    pub candidates_truncated: bool,
    pub candidates: Vec<CaptureCandidate>,
    pub fastest_supported: Option<usize>,
    /// Only a time-reference comparison with full shortlist coverage.
    /// This is not a survival/utility recommendation or execution permission.
    pub preferred_by_time: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_comparison: Option<ValueComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_source: Option<TransferSource>,
    pub comparison_reason: &'static str,
    pub charged_work: u32,
}

#[derive(Clone)]
struct EvaluationJob {
    report: MissionEvaluation,
    cursor: usize,
    complete: bool,
}
impl PlanningJob for EvaluationJob {
    type Output = MissionEvaluation;
    fn next_work(&self) -> Option<WorkKind> {
        (!self.complete).then_some(WorkKind::Graph)
    }
    fn step(&mut self) {
        if self.complete {
            return;
        }
        self.report.charged_work += 1;
        if let Some(candidate) = self.report.candidates.get_mut(self.cursor) {
            if let Some(source) = &self.report.transfer_source
                && candidate.unknown_reason.is_none()
            {
                let estimate = candidate
                    .transfer
                    .map_or_else(|| source.estimate(candidate.planet), Ok);
                match estimate {
                    Ok(transfer) => {
                        candidate.travel_seconds = transfer.total();
                        candidate.transfer = Some(transfer);
                    }
                    Err(reason) => candidate.unknown_reason = Some(reason),
                }
            }
            candidate.total_seconds = candidate
                .local
                .as_ref()
                .filter(|_| candidate.unknown_reason.is_none())
                .map(|local| local.total() + candidate.travel_seconds);
            candidate.reference_exceeds_match_time = candidate
                .total_seconds
                .zip(
                    self.report
                        .match_context
                        .as_ref()
                        .and_then(|m| m.remaining_seconds),
                )
                .map(|(cost, remaining)| f64::from(cost) > remaining);
            if let Some(value) = &mut candidate.value {
                value.seconds_per_unit = candidate
                    .total_seconds
                    .filter(|_| value.priority_units != 0)
                    .map(|seconds| seconds / f32::from(value.priority_units));
            }
            self.cursor += 1;
            return;
        }
        self.report.fastest_supported = self
            .report
            .candidates
            .iter()
            .filter(|c| c.total_seconds.is_some())
            .min_by(|a, b| {
                a.total_seconds
                    .unwrap()
                    .total_cmp(&b.total_seconds.unwrap())
                    .then_with(|| a.planet.cmp(&b.planet))
            })
            .map(|c| c.planet);
        self.report.comparison_reason = if self.report.inactive_reason.is_some() {
            "capture comparison inactive"
        } else if self.report.candidates.is_empty() {
            "no eligible capture destination"
        } else if self
            .report
            .candidates
            .iter()
            .any(|c| c.total_seconds.is_none())
        {
            "insufficient evidence for complete shortlist comparison"
        } else if self
            .report
            .candidates
            .iter()
            .all(|c| c.reference_exceeds_match_time == Some(true))
        {
            "all completion references exceed remaining match time"
        } else {
            self.report.preferred_by_time = self.report.fastest_supported;
            "lowest supported completion-time reference; combat risk unmodelled"
        };
        value::finish(&mut self.report);
        self.complete = true;
    }
    fn output(&self) -> Option<&Self::Output> {
        self.complete.then_some(&self.report)
    }
}

#[derive(Clone, PartialEq)]
struct EvidenceIdentity {
    remote: bool,
    site: LandingSiteId,
    costs: Option<PhaseCosts>,
    reason: Option<&'static str>,
    route_source_tick: Option<u64>,
}

#[derive(Clone, PartialEq)]
struct Dependencies {
    policy: &'static str,
    transfer: Option<TransferSource>,
    planets: Vec<PlanetKey>,
    target: Option<usize>,
    location: PilotLocation,
    form: ShipForm,
    available: bool,
    finished: bool,
    gravity: f32,
    selected_tick: Option<u64>,
    selected_site: Option<LandingSiteId>,
    landed: bool,
    evidence: Vec<EvidenceIdentity>,
}
#[derive(Clone, Default)]
struct ActorState {
    survey: Option<survey::AlternativeSurvey>,
    flag_survey: Option<flag_survey::RequestState>,
    last_tick: Option<u64>,
    submitted_tick: Option<u64>,
    dependencies: Option<Dependencies>,
    evidence: Vec<LocalEvidence>,
    pending: Option<RequestToken>,
    latest: Option<MissionEvaluation>,
    latest_dependencies: Option<Dependencies>,
    latest_evidence: Vec<LocalEvidence>,
    submitted_evidence: Vec<LocalEvidence>,
}

/// Shared, capacity-limited diagnostic queue. Construction visits at most eight
/// planets and 64 local sites; physical surveys remain owned by the host.
#[derive(Clone)]
pub struct MissionEvaluator {
    queue: PlanningQueue<(), EvaluationJob>,
    actors: BTreeMap<u64, ActorState>,
    capacity: usize,
    last_advance: Option<u64>,
    pub charged_total: u64,
    pub cancelled_total: u64,
    pub completed_total: u64,
}
impl MissionEvaluator {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: PlanningQueue::new(capacity),
            actors: BTreeMap::new(),
            capacity,
            last_advance: None,
            charged_total: 0,
            cancelled_total: 0,
            completed_total: 0,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::new(self.capacity);
    }
    pub fn latest(&self, actor: PlayerId) -> Option<&MissionEvaluation> {
        self.actors.get(&(actor.index() as u64))?.latest.as_ref()
    }
    pub fn pending(&self, actor: PlayerId) -> bool {
        self.actors
            .get(&(actor.index() as u64))
            .is_some_and(|s| s.pending.is_some())
    }
    /// Separate observational experiment. Its results are deliberately not
    /// admitted to evaluation/selection until coverage and timing are tested.
    pub fn flag_request(
        &mut self,
        o: &MissionObservationV1,
        mission: &MissionTelemetry,
    ) -> Option<scenario_spacewars::surface_sortie::live_planning::FlagSurveyRequest> {
        let actor = o.local.combat.recovery.flight.pilot.owner.index() as u64;
        if !self.actors.contains_key(&actor) && self.actors.len() >= self.capacity {
            return None;
        }
        flag_survey::request(
            &mut self.actors.entry(actor).or_default().flag_survey,
            o,
            mission,
        )
    }
    /// Optional demand for the host's existing remote-query dispatcher. Call
    /// after controls; this never changes the bot's own sensor request.
    pub fn alternative_request(
        &mut self,
        o: &MissionObservationV1,
        mission: &MissionTelemetry,
    ) -> Option<scenario_spacewars::surface_sortie::destination_cover::DestinationCoverRequest>
    {
        let actor = o.local.combat.recovery.flight.pilot.owner.index() as u64;
        if !self.actors.contains_key(&actor) && self.actors.len() >= self.capacity {
            return None;
        }
        survey::request(
            &mut self.actors.entry(actor).or_default().survey,
            o,
            mission,
        )
    }
    pub fn observe(&mut self, o: &MissionObservationV1, mission: &MissionTelemetry) {
        let p = &o.local.combat.recovery.flight.pilot;
        let actor = p.owner.index() as u64;
        if !self.actors.contains_key(&actor) && self.actors.len() >= self.capacity {
            return;
        }
        let state = self.actors.entry(actor).or_default();
        if state.last_tick == Some(p.tick) {
            return;
        }
        if state.last_tick.is_some_and(|tick| tick > p.tick) {
            if let Some(token) = state.pending.take() {
                self.queue.cancel(token);
                self.cancelled_total += 1;
            }
            *state = ActorState::default();
        }
        state.last_tick = Some(p.tick);
        let mut dependencies = Dependencies {
            policy: mission.policy,
            transfer: value::enabled(mission.policy).then(|| TransferSource::read(o)),
            planets: o
                .planets
                .iter()
                .take(MAX_PLANETS)
                .map(PlanetKey::read)
                .collect(),
            target: mission.target,
            location: p.location,
            form: p.ship_form,
            available: p.ship_available,
            finished: o
                .match_context
                .as_ref()
                .is_some_and(|m| m.finished || !m.pilots_alive[p.owner.index()]),
            gravity: o.local.objective_gravity,
            selected_tick: selection_tick(mission),
            selected_site: mission.capture.as_ref().and_then(|c| c.site),
            landed: mission
                .capture
                .as_ref()
                .is_some_and(|c| c.landing.landed_tick.is_some()),
            evidence: Vec::new(),
        };
        state.evidence.retain(|sample| {
            sample.tick <= p.tick
                && p.tick - sample.tick <= MAX_EVIDENCE_AGE
                && dependencies
                    .planets
                    .iter()
                    .any(|key| key.matches(&sample.key))
                && (!sample.remote || p.queries_ready)
                && (sample.remote
                    || sample.key.planet != p.planet.index
                    || (sample.gravity - dependencies.gravity).abs() <= 0.01)
        });
        if let Some(sample) = survey::evidence(&state.survey, o) {
            state
                .evidence
                .retain(|old| old.key.planet != sample.key.planet);
            if state.evidence.len() == MAX_PLANETS {
                state.evidence.remove(0);
            }
            state.evidence.push(sample);
        }
        if let Some(mut sample) = model::observe_local(o, mission) {
            // Native route surveys are cadenced. Missing work between surveys
            // is not a fresh negative measurement and cannot renew its age.
            if let Some(old) = state.evidence.iter().find(|old| old.site == sample.site)
                && sample.reason == Some("objective route unmeasured")
                && model::route_cadence_gap(o, old)
            {
                sample = old.clone();
            }
            if let Some(old) = state.evidence.iter().find(|old| old.site == sample.site)
                && let (Some((visit, tick)), Some((new_visit, _))) = (old.choice, sample.choice)
                && visit == new_visit
            {
                sample.choice = Some((visit, tick));
            }
            state
                .evidence
                .retain(|old| old.key.planet != sample.key.planet);
            if state.evidence.len() == MAX_PLANETS {
                state.evidence.remove(0);
            }
            state.evidence.push(sample);
        }
        dependencies.evidence = state
            .evidence
            .iter()
            .map(|s| EvidenceIdentity {
                remote: s.remote,
                site: s.site,
                costs: s.costs.clone(),
                reason: s.reason,
                route_source_tick: s.route_source_tick,
            })
            .collect();
        // Own flight gravity cannot invalidate an unknown remote ground cost.
        // When a local timing reference exists, compare against its pinned
        // source rather than allowing small per-tick changes to accumulate.
        if !state
            .evidence
            .iter()
            .any(|s| !s.remote && s.key.planet == p.planet.index && s.costs.is_some())
        {
            dependencies.gravity = 0.0;
        }
        let changed = state.dependencies.as_ref().is_none_or(|old| {
            old.policy != dependencies.policy
                || old
                    .transfer
                    .as_ref()
                    .is_some_and(|source| !source.is_current(o))
                || old.target != dependencies.target
                || old.location != dependencies.location
                || old.form != dependencies.form
                || old.available != dependencies.available
                || (old.gravity - dependencies.gravity).abs() > 0.01
                || old.selected_tick != dependencies.selected_tick
                || old.selected_site != dependencies.selected_site
                || old.landed != dependencies.landed
                || old.evidence != dependencies.evidence
                || old.finished != dependencies.finished
                || old.planets.len() != dependencies.planets.len()
                || old
                    .planets
                    .iter()
                    .zip(&dependencies.planets)
                    .any(|(a, b)| !a.matches(b))
        });
        let request_expired = state
            .submitted_tick
            .is_some_and(|tick| p.tick.saturating_sub(tick) > MAX_RESULT_AGE);
        let result_expired = state
            .latest
            .as_ref()
            .is_some_and(|report| p.tick.saturating_sub(report.source_tick) > MAX_RESULT_AGE);
        if (changed || request_expired)
            && let Some(token) = state.pending.take()
        {
            self.queue.cancel(token);
            self.cancelled_total += 1;
        }
        // Submitting a refresh does not renew the previously published result.
        if changed || result_expired {
            state.latest = None;
        }
        if state.pending.is_some()
            || (!changed
                && !request_expired
                && state
                    .submitted_tick
                    .is_some_and(|tick| p.tick.saturating_sub(tick) < REFRESH_TICKS))
        {
            return;
        }
        let report = snapshot(o, mission, &state.evidence);
        state.submitted_evidence = state.evidence.clone();
        state.dependencies = Some(dependencies);
        let token = self
            .queue
            .submit(
                actor,
                (),
                JobLimits {
                    per_tick: Work {
                        graph: 2,
                        physics_queries: 0,
                    },
                    ..Default::default()
                },
                EvaluationJob {
                    report,
                    cursor: 0,
                    complete: false,
                },
            )
            .unwrap();
        state.pending = Some(token);
        state.submitted_tick = Some(p.tick);
    }
    /// Caller passes work left after higher-priority jobs. Never adds an
    /// independent allowance to a live planner's already-consumed quota.
    pub fn advance(&mut self, tick: u64, remaining: Work) -> Work {
        if self.last_advance == Some(tick) {
            return Work::default();
        }
        self.last_advance = Some(tick);
        let allocation = self.queue.advance(Work {
            graph: remaining.graph.min(DEFAULT_WORK.graph),
            physics_queries: 0,
        });
        self.charged_total += u64::from(allocation.charged.graph);
        for row in allocation.jobs {
            if row.phase == JobPhase::Ready {
                let job = self.queue.take(row.request).unwrap();
                let mut report = job.output().unwrap().clone();
                report.completed_tick = Some(tick);
                let state = self.actors.get_mut(&row.request.actor).unwrap();
                state.pending = None;
                state.latest = Some(report);
                state.latest_dependencies = state.dependencies.clone();
                state.latest_evidence = state.submitted_evidence.clone();
                self.completed_total += 1;
            }
        }
        allocation.charged
    }
}

fn selection_tick(mission: &MissionTelemetry) -> Option<u64> {
    mission
        .events
        .iter()
        .rev()
        .find(|e| e.kind == "selected" && e.planet == mission.target)
        .map(|e| e.tick)
}

fn candidate_planets<'a>(
    o: &'a MissionObservationV1,
    mission: &MissionTelemetry,
) -> Vec<&'a PilotPlanetObservation> {
    let p = &o.local.combat.recovery.flight.pilot;
    let mut options: Vec<&PilotPlanetObservation> = o
        .planets
        .iter()
        .take(MAX_PLANETS)
        .filter(|planet| {
            Some(planet.index) == mission.target
                || planet
                    .claim
                    .as_ref()
                    .is_none_or(|c| c.owner != Some(p.owner))
        })
        .collect();
    options.sort_by(|a, b| {
        (Some(b.index) == mission.target)
            .cmp(&(Some(a.index) == mission.target))
            .then_with(|| {
                p.ship
                    .position
                    .distance_to(a.motion.position)
                    .total_cmp(&p.ship.position.distance_to(b.motion.position))
            })
            .then_with(|| a.index.cmp(&b.index))
    });
    options
}

fn snapshot(
    o: &MissionObservationV1,
    mission: &MissionTelemetry,
    evidence: &[LocalEvidence],
) -> MissionEvaluation {
    let p = &o.local.combat.recovery.flight.pilot;
    let inactive = if o.planets.len() > MAX_PLANETS {
        Some("planet list exceeds bounded snapshot capacity")
    } else if o
        .match_context
        .as_ref()
        .is_some_and(|m| m.finished || !m.pilots_alive[p.owner.index()])
    {
        Some("match finished or pilot dead")
    } else if !p.ship_available
        || p.ship_form != ShipForm::Ship
        || !matches!(p.location, PilotLocation::Aboard(_))
    {
        Some("ship or pilot recovery required")
    } else if mission
        .capture
        .as_ref()
        .is_some_and(|c| c.landing.landed_tick.is_some())
    {
        Some("capture already landed; retain its return task")
    } else {
        None
    };
    let mut options = candidate_planets(o, mission);
    let truncated = o.planets.len() > MAX_PLANETS || options.len() > MAX_OPTIONS;
    options.truncate(MAX_OPTIONS);
    let own_count = o.match_context.as_ref().map_or_else(
        || {
            o.planets
                .iter()
                .take(MAX_PLANETS)
                .filter(|v| v.claim.as_ref().is_some_and(|c| c.owner == Some(p.owner)))
                .count()
        },
        |m| m.owned_planets[p.owner.index()],
    );
    let candidates = options
        .into_iter()
        .map(|planet| {
            let sample = evidence.iter().find(|s| s.key.planet == planet.index);
            let distance = p.ship.position.distance_to(planet.motion.position);
            let owner = planet.claim.as_ref().and_then(|c| c.owner);
            let mut local = sample.and_then(|s| s.costs.clone());
            let active_choice = Some(planet.index) == mission.target
                && sample
                    .and_then(|s| s.choice)
                    .is_some_and(|(visit, _)| selection_tick(mission) == Some(visit))
                && mission.capture.as_ref().and_then(|c| c.site) == sample.map(|s| s.site);
            if active_choice
                && let Some((_, choice_tick)) = sample.and_then(|s| s.choice)
                && let Some(costs) = &mut local
            {
                // Remaining median reference for the in-flight current attempt;
                // elapsed time is never added to an alternative's new trip.
                costs.landing = (costs.landing - (p.tick - choice_tick) as f32 / 60.0).max(0.0);
            }
            let reason = inactive.or_else(|| {
                if planet.claim.is_none() {
                    Some("ownership unknown")
                } else if owner == Some(p.owner) {
                    Some("planet already owned")
                } else {
                    sample.map_or(Some("remote or local surface unmeasured"), |s| s.reason)
                }
            });
            CaptureCandidate {
                planet: planet.index,
                current: mission.target == Some(planet.index),
                revision: planet.revision,
                observed_owner: owner,
                ownership_known: planet.claim.is_some(),
                adds_ownership: planet.claim.is_some() && owner != Some(p.owner),
                first_rebuild_foothold: own_count == 0
                    && planet.claim.is_some()
                    && owner != Some(p.owner),
                distance,
                travel_seconds: if active_choice {
                    0.0
                } else {
                    (distance - planet.radius - 85.0).max(0.0) / 38.0
                },
                transfer: (value::enabled(mission.policy) && active_choice).then_some(
                    TransferReference {
                        continuing_approach: true,
                        ..Default::default()
                    },
                ),
                value: value::enabled(mission.policy).then(|| {
                    let swing = if planet.claim.is_none() || owner == Some(p.owner) {
                        0
                    } else if owner.is_some() {
                        2
                    } else {
                        1
                    };
                    CaptureValue {
                        ownership_swing: swing,
                        priority_units: if own_count == 0 { swing.min(1) } else { swing },
                        seconds_per_unit: None,
                    }
                }),
                site: sample.map(|s| s.site),
                evidence_tick: sample.map(|s| s.tick),
                evidence_age_ticks: sample.map(|s| p.tick - s.tick),
                route_source_tick: sample.and_then(|s| s.route_source_tick),
                route_validated_tick: sample.and_then(|s| s.route_validated_tick),
                evidence_kind: if sample.is_some_and(|s| s.remote) {
                    "remote landing, hatch and climb samples; live feasibility unknown"
                } else if sample.is_some_and(|s| s.tick == p.tick) {
                    "current local measurement"
                } else {
                    "historical timing reference; live feasibility unknown"
                },
                local,
                unknown_reason: reason,
                total_seconds: None,
                reference_exceeds_match_time: None,
            }
        })
        .collect();
    MissionEvaluation {
        model: model_for_policy(mission.policy),
        policy: mission.policy,
        actor: p.owner,
        source_tick: p.tick,
        completed_tick: None,
        current_target: mission.target,
        selected_tick: selection_tick(mission),
        match_context: o.match_context.clone(),
        ship_health: p.ship_health,
        ship_form: p.ship_form,
        location: p.location,
        opponent_distance: o
            .local
            .combat
            .target
            .map(|enemy| p.ship.position.distance_to(enemy.motion.position)),
        combat_risk: "unmodelled; timing is conditional on successful execution",
        inactive_reason: inactive,
        candidates_truncated: truncated,
        candidates,
        fastest_supported: None,
        preferred_by_time: None,
        value_comparison: value::enabled(mission.policy).then_some(ValueComparison {
            objective: if own_count == 0 {
                "first rebuild foothold"
            } else {
                "ownership swing per completion second"
            },
            preferred: None,
        }),
        transfer_source: value::enabled(mission.policy).then(|| TransferSource::read(o)),
        comparison_reason: "pending",
        charged_work: 0,
    }
}
