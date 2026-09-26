//! Historical, paired value comparisons. This type exposes no selection API.
//! Hosts dispatch it last; neither admission nor results enter the playing bot.
use super::*;
use scenario_spacewars::surface_sortie::{
    destination_cover::CoverFinding,
    landing_objective::LandingObjective,
    live_planning::{FlagSurveyRequest, FlagSurveySample},
};

pub const MODEL: &str = "capture_flag_value_shadow_v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlagShadowAdmission {
    pub site: LandingSiteId,
    pub generation: u64,
    pub source_tick: u64,
    pub completed_tick: u64,
    pub validated_tick: Option<u64>,
    pub source_age_ticks: Option<u64>,
    pub source_objective: Option<LandingObjective>,
    pub reason: Option<&'static str>,
    pub used: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlagValueShadowReport {
    pub model: &'static str,
    pub observational: bool,
    pub actor: PlayerId,
    /// Current observation at admission; transfer/other costs keep baseline time.
    pub admitted_tick: u64,
    pub completed_tick: Option<u64>,
    pub admissions: Vec<FlagShadowAdmission>,
    pub baseline: MissionEvaluation,
    pub augmented: MissionEvaluation,
    /// A historical ranking difference, never authority to change destination.
    pub preference_changed: bool,
    pub completion_reason: Option<&'static str>,
}

#[derive(Clone)]
struct Job {
    report: FlagValueShadowReport,
    evaluation: EvaluationJob,
}
impl PlanningJob for Job {
    type Output = FlagValueShadowReport;
    fn next_work(&self) -> Option<WorkKind> {
        self.evaluation.next_work()
    }
    fn step(&mut self) {
        self.evaluation.step();
        if let Some(result) = self.evaluation.output() {
            self.report.augmented = result.clone();
        }
    }
    fn output(&self) -> Option<&Self::Output> {
        self.evaluation.output().map(|_| &self.report)
    }
}

#[derive(Clone, Default)]
struct Actor {
    seen: Option<u64>,
    submitted: Option<u64>,
    source: Option<u64>,
    pending: Option<RequestToken>,
    latest: Option<FlagValueShadowReport>,
}

/// One pending and one completed historical report per actor. Each comparison
/// costs at most four graph steps and zero queries, after all ordinary work.
#[derive(Clone)]
pub struct FlagValueShadow {
    actors: BTreeMap<usize, Actor>,
    queue: PlanningQueue<(), Job>,
    capacity: usize,
    last_advance: Option<u64>,
    pub charged_total: u64,
    pub completed_total: u64,
}
impl FlagValueShadow {
    pub fn new(capacity: usize) -> Self {
        Self {
            actors: BTreeMap::new(),
            queue: PlanningQueue::new(capacity),
            capacity,
            last_advance: None,
            charged_total: 0,
            completed_total: 0,
        }
    }
    pub fn latest(&self, actor: PlayerId) -> Option<&FlagValueShadowReport> {
        self.actors.get(&actor.index())?.latest.as_ref()
    }
    pub fn pending(&self, actor: PlayerId) -> bool {
        self.actors
            .get(&actor.index())
            .is_some_and(|s| s.pending.is_some())
    }
    /// Call after the ordinary evaluator observes this tick. A pending refresh
    /// can retain an older report, so check its pinned transfer source directly.
    /// Surface costs remain historical. No observation is mutated.
    pub fn observe(
        &mut self,
        o: &MissionObservationV1,
        evaluator: &MissionEvaluator,
        request: Option<FlagSurveyRequest>,
        samples: &[&FlagSurveySample],
    ) {
        let p = &o.local.combat.recovery.flight.pilot;
        let seat = p.owner.index();
        if !self.actors.contains_key(&seat) && self.actors.len() >= self.capacity {
            return;
        }
        let actor = self.actors.entry(seat).or_default();
        if actor.seen.is_some_and(|tick| tick > p.tick) {
            if let Some(token) = actor.pending.take() {
                self.queue.cancel(token);
            }
            *actor = Actor::default();
        }
        if actor.seen == Some(p.tick) {
            return;
        }
        actor.seen = Some(p.tick);
        if actor.pending.is_some()
            || actor
                .submitted
                .is_some_and(|t| p.tick.saturating_sub(t) < REFRESH_TICKS)
        {
            return;
        }
        let Some(base) = evaluator.latest(p.owner).filter(|r| {
            value::enabled(r.policy)
                && r.model == value::MODEL
                && r.actor == p.owner
                && r.inactive_reason.is_none()
                && r.completed_tick
                    .is_some_and(|t| r.source_tick <= t && t <= p.tick)
                && r.source_tick <= p.tick
                && p.tick - r.source_tick <= MAX_RESULT_AGE
                && actor.source != Some(r.source_tick)
                && r.candidates.len() <= MAX_OPTIONS
                && r.transfer_source
                    .as_ref()
                    .is_some_and(|source| source.is_current(o))
                && r.candidates.iter().all(|c| {
                    c.evidence_tick
                        .is_none_or(|tick| tick <= p.tick && p.tick - tick <= MAX_EVIDENCE_AGE)
                })
                && p.queries_ready
        }) else {
            return;
        };
        let source = &evaluator.actors[&(seat as u64)];
        if source.last_tick != Some(p.tick) {
            return;
        }
        let Some(dependencies) = &source.latest_dependencies else {
            return;
        };
        // The planner holds at most two samples per actor. Bound even malformed
        // input before filtering; do not copy snapshots or captured area lists.
        let samples: Vec<_> = samples
            .iter()
            .take(4)
            .copied()
            .filter(|s| s.actor == p.owner)
            .take(2)
            .collect();
        if samples.is_empty() {
            return;
        }
        let mut augmented = base.clone();
        augmented.model = MODEL;
        augmented.completed_tick = None;
        augmented.charged_work = 0;
        augmented.fastest_supported = None;
        augmented.preferred_by_time = None;
        augmented.comparison_reason = "pending";
        if let Some(value) = &mut augmented.value_comparison {
            value.preferred = None;
        }
        let mut admissions = Vec::new();
        let mut accepted = Vec::new();
        for sample in samples {
            let key = dependencies
                .planets
                .iter()
                .find(|k| k.planet == sample.site.planet);
            let costs = admit(o, base, request, sample, key);
            let index = admissions.len();
            admissions.push(FlagShadowAdmission {
                site: sample.site,
                generation: sample.generation,
                source_tick: sample.source_tick,
                completed_tick: sample.completed_tick,
                validated_tick: sample.validated_tick,
                source_age_ticks: p.tick.checked_sub(sample.source_tick),
                source_objective: sample.validation.as_ref().map(|v| v.source_objective),
                reason: costs.as_ref().err().copied(),
                used: false,
            });
            if let Ok(costs) = costs {
                accepted.push((index, sample, costs));
            }
        }
        // One alternative planet, at most two sites: smallest calibrated walking
        // cost, then newest source and bearing. Existing local evidence wins.
        accepted.sort_by(|a, b| {
            a.2.total()
                .total_cmp(&b.2.total())
                .then(b.1.source_tick.cmp(&a.1.source_tick))
                .then(a.1.site.bearing.cmp(&b.1.site.bearing))
        });
        for (index, sample, costs) in accepted {
            let candidate = augmented
                .candidates
                .iter_mut()
                .find(|c| c.planet == sample.site.planet)
                .unwrap();
            if candidate.local.is_some() {
                continue;
            }
            admissions[index].used = true;
            candidate.site = Some(sample.site);
            candidate.local = Some(costs);
            candidate.unknown_reason = None;
            candidate.evidence_tick = Some(sample.source_tick);
            candidate.evidence_age_ticks = Some(base.source_tick - sample.source_tick);
            candidate.route_source_tick = Some(sample.source_tick);
            candidate.route_validated_tick = sample.validated_tick;
            candidate.evidence_kind =
                "historical published flag walk; cover and live feasibility unknown";
        }
        let report = FlagValueShadowReport {
            model: MODEL,
            observational: true,
            actor: p.owner,
            admitted_tick: p.tick,
            completed_tick: None,
            admissions,
            baseline: base.clone(),
            augmented: augmented.clone(),
            preference_changed: false,
            completion_reason: None,
        };
        actor.pending = Some(
            self.queue
                .submit(
                    seat as u64,
                    (),
                    JobLimits {
                        per_tick: Work {
                            graph: 2,
                            physics_queries: 0,
                        },
                        ..Default::default()
                    },
                    Job {
                        report,
                        evaluation: EvaluationJob {
                            report: augmented,
                            cursor: 0,
                            complete: false,
                        },
                    },
                )
                .unwrap(),
        );
        actor.source = Some(base.source_tick);
        actor.submitted = Some(p.tick);
    }
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
            if row.phase != JobPhase::Ready {
                continue;
            }
            let mut report = self
                .queue
                .take(row.request)
                .unwrap()
                .output()
                .unwrap()
                .clone();
            report.completed_tick = Some(tick);
            report.augmented.completed_tick = Some(tick);
            if tick < report.admitted_tick
                || tick.saturating_sub(report.baseline.source_tick) > MAX_RESULT_AGE
                || report
                    .admissions
                    .iter()
                    .filter(|a| a.used)
                    .any(|a| tick.saturating_sub(a.source_tick) > MAX_EVIDENCE_AGE)
            {
                report.completion_reason = Some("historical comparison expired before completion");
                report.augmented.preferred_by_time = None;
                if let Some(value) = &mut report.augmented.value_comparison {
                    value.preferred = None;
                }
            } else {
                report.preference_changed =
                    preferred(&report.baseline) != preferred(&report.augmented);
            }
            let actor = self.actors.get_mut(&(row.request.actor as usize)).unwrap();
            actor.pending = None;
            actor.latest = Some(report);
            self.completed_total += 1;
        }
        allocation.charged
    }
}

fn preferred(report: &MissionEvaluation) -> Option<usize> {
    report.value_comparison.as_ref().and_then(|v| v.preferred)
}

fn admit(
    o: &MissionObservationV1,
    base: &MissionEvaluation,
    request: Option<FlagSurveyRequest>,
    s: &FlagSurveySample,
    baseline_key: Option<&PlanetKey>,
) -> Result<PhaseCosts, &'static str> {
    let p = &o.local.combat.recovery.flight.pilot;
    let request = request.ok_or("flag demand absent")?;
    if s.actor != p.owner
        || s.generation != request.generation
        || !request.candidates.contains(&s.site)
        || !request.matches_objective(s.objective)
        || s.generation > s.source_tick
    {
        return Err("flag request identity changed");
    }
    if s.source_tick > s.completed_tick
        || s.completed_tick > base.source_tick
        || s.validated_tick != Some(s.completed_tick)
        || s.measurement.tick != s.source_tick
    {
        return Err("flag evidence unavailable at comparison source");
    }
    if s.source_tick > p.tick || p.tick - s.source_tick > MAX_EVIDENCE_AGE {
        return Err("flag source expired");
    }
    let v = s
        .validation
        .as_ref()
        .ok_or("local publication certificate absent")?;
    if s.reason.is_some()
        || v.model != "captured_query_unions_v1"
        || !v.complete
        || !v.predicates_valid
        || v.predicate_failure.is_some()
        || !v.geometry.valid
        || !request.matches_objective(v.source_objective)
    {
        return Err("local publication not validated");
    }
    if baseline_key
        .is_none_or(|key| !key.flag_identity_matches(v.source_objective, v.source_radius))
    {
        return Err("flag identity differs at comparison source");
    }
    let planet = o
        .planets
        .iter()
        .take(MAX_PLANETS)
        .find(|p| p.index == s.site.planet)
        .ok_or("flag planet absent")?;
    let claim = planet.claim.as_ref().ok_or("flag ownership unknown")?;
    let flag = claim.flag.ok_or("flag absent")?;
    let objective = LandingObjective {
        planet: planet.index,
        revision: planet.revision,
        owner: flag.player,
        position: (flag.position - planet.motion.position).rotate_radians(-planet.motion.angle),
        range: claim.flag_interaction_range - 0.2,
    };
    if claim.owner != Some(s.objective.owner)
        || claim.owner == Some(p.owner)
        || !request.matches_objective(objective)
        || !(FlagSurveyRequest {
            objective: v.source_objective,
            ..request
        })
        .matches_objective(objective)
        || planet.radius != v.source_radius
        || !v.source_radius.is_finite()
    {
        return Err("flag source objective or radius changed");
    }
    let candidate = base
        .candidates
        .iter()
        .find(|c| c.planet == planet.index)
        .ok_or("flag outside comparison shortlist")?;
    if candidate.current
        || candidate.revision != planet.revision
        || candidate.observed_owner != claim.owner
        || !candidate.ownership_known
    {
        return Err("flag alternative changed");
    }
    if candidate.local.is_some() {
        return Err("existing surface evidence retained");
    }
    if !matches!(
        candidate.unknown_reason,
        Some(
            "remote or local surface unmeasured"
                | "objective route unmeasured"
                | "site round trip unmeasured"
        )
    ) {
        return Err("unrelated comparison rejection retained");
    }
    let m = &s.measurement;
    if m.finding != CoverFinding::Measured
        || m.ship_form != ShipForm::Ship
        || m.revision != planet.revision
        || m.climb_clear != Some(true)
        || m.site.is_none_or(|site| {
            site.id != s.site
                || site.revision != m.revision
                || !site.hatch_has_settling_margin
                || !site.boarding_hatches.iter().any(Option::is_some)
        })
    {
        return Err("flag landing hatch or climb incomplete");
    }
    let route = s.route.as_ref().ok_or("flag round trip absent")?;
    if route.site != Some(s.site) || route.endpoint.is_none() {
        return Err("flag route identity incomplete");
    }
    let endpoint = route.endpoint.unwrap().position;
    let half_height = scenario_spacewars::spaceling_geometry::HALF_HEIGHT;
    if !(endpoint + endpoint.normalized() * half_height)
        .distance_to(objective.position)
        .lt(&objective.range)
    {
        return Err("flag endpoint no longer in interaction range");
    }
    model::walking_costs(route, claim.stage_required_seconds)
}

#[cfg(test)]
mod tests;
