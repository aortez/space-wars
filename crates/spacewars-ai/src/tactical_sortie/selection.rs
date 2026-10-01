//! The native landing ranker and an optional bounded same-observation diagnostic.
//! Scores retain their controller units; they are not capture-time estimates.
use super::*;
use scenario_spacewars::surface_sortie::landing_objective::LandingObjectiveSurvey;

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LandingDirectionAssessment {
    pub site: LandingSiteId,
    pub revision: u64,
    pub site_order: usize,
    pub direction_order: Option<usize>,
    /// None means a site-level gate rejected it before checking directions.
    pub side: Option<f32>,
    pub short_angle: Option<f32>,
    pub solar: Option<SolarLandingPlan>,
    pub rejection: Option<&'static str>,
    pub approach_score: Option<f32>,
    pub cover_penalty: Option<f32>,
    pub ground_score: Option<f32>,
    pub total_score: Option<f32>,
}

impl LandingDirectionAssessment {
    fn new(site: PilotLandingSite, site_order: usize) -> Self {
        Self {
            site: site.id,
            revision: site.revision,
            site_order,
            direction_order: None,
            side: None,
            short_angle: None,
            solar: None,
            rejection: None,
            approach_score: None,
            cover_penalty: None,
            ground_score: None,
            total_score: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LandingChoiceComparison {
    pub model: &'static str,
    pub tick: u64,
    pub planet: usize,
    pub revision: u64,
    pub reference: LandingSiteId,
    pub selected: LandingDirectionAssessment,
    pub reference_best: Option<LandingDirectionAssessment>,
    pub classification: &'static str,
    pub exposed: bool,
    pub commit_descent: bool,
    pub required_site: Option<LandingSiteId>,
    pub site_count: usize,
    pub objective: Option<LandingObjective>,
    pub checks: CandidateCheckCounts,
    pub assessments: Vec<LandingDirectionAssessment>,
}

pub(super) type Selected = (PilotLandingSite, f32, Option<SolarLandingPlan>, f32);

pub(crate) fn preferred_side(short: f32) -> f32 {
    if short < 0.0 { -1.0 } else { 1.0 }
}

pub(crate) fn approach_score(short: f32, side: f32, solar: bool, radius: f32) -> f32 {
    let angle = if solar {
        crate::landing_safety::directed_angle(short, side)
    } else {
        short
    };
    angle.abs() * (radius + 60.0)
}

pub(super) fn exposed(o: &TacticalSortieObservationV1) -> bool {
    let p = &o.combat.recovery.flight.pilot;
    o.combat.target.is_some_and(|t| {
        !t.ground_occluded && t.motion.position.distance_to(p.ship.position) < 300.0
    })
}

pub(super) fn survey_rejection(
    o: &TacticalSortieObservationV1,
    objective: Option<LandingObjective>,
    survey: &LandingObjectiveSurvey,
) -> Option<&'static str> {
    let p = &o.combat.recovery.flight.pilot;
    if survey.version != 1 {
        Some("survey_version")
    } else if survey.actor != p.owner {
        Some("survey_actor")
    } else if !survey.is_current(p.tick) {
        Some("survey_age")
    } else if !objective.is_some_and(|target| target.matches(survey.objective)) {
        Some("survey_objective")
    } else if survey.sites.len()
        > scenario_spacewars::surface_sortie::landing_objective::MAX_OBJECTIVE_SITES
    {
        Some("survey_size")
    } else {
        None
    }
}

/// Keep site order, preferred/opposite direction order, check precedence and
/// f32 arithmetic identical for the playing selector and its diagnostic. The
/// native call has a no-op sink, with no retained ledger or allocation.
pub(crate) fn select(
    pilot: &TacticalSortiePilot,
    o: &TacticalSortieObservationV1,
    objective: Option<LandingObjective>,
    survey: Option<&LandingObjectiveSurvey>,
    exposed: bool,
    mut record: impl FnMut(LandingDirectionAssessment),
) -> (Option<Selected>, CandidateCheckCounts) {
    let p = &o.combat.recovery.flight.pilot;
    let up = (p.ship.position - p.planet.motion.position).normalized();
    let mut checks = CandidateCheckCounts::default();
    let mut selected: Option<Selected> = None;
    for (site_order, site) in p.sites.iter().enumerate() {
        let mut assessment = LandingDirectionAssessment::new(*site, site_order);
        let rejected = if pilot
            .required_site
            .is_some_and(|id| site.id != id || site.revision != p.planet.revision)
        {
            checks.required_site += 1;
            Some("required_site")
        } else if pilot.rejected_sites.contains(&(site.id, site.revision)) {
            checks.previously_rejected += 1;
            Some("previously_rejected")
        } else if pilot.solar_rejected.iter().any(|(id, _)| *id == site.id) {
            checks.solar_cooldown += 1;
            Some("solar_cooldown")
        } else if pilot.cover_retry_blocked(o, site, exposed) {
            checks.cover_cooldown += 1;
            Some("cover_cooldown")
        } else {
            None
        };
        if rejected.is_some() {
            assessment.rejection = rejected;
            record(assessment);
            continue;
        }
        let direction = (site.vehicle_position - p.planet.motion.position).normalized();
        let short = angle_between(up, direction);
        let preferred = preferred_side(short);
        for (direction_order, side) in [preferred, -preferred].into_iter().enumerate() {
            if side != preferred && (!pilot.commit_descent || o.sun.is_none()) {
                continue;
            }
            checks.directions += 1;
            let solar = pilot
                .commit_descent
                .then(|| crate::landing_safety::assess(o, *site, side, true))
                .flatten();
            let mut assessment = LandingDirectionAssessment {
                side: Some(side),
                direction_order: Some(direction_order),
                short_angle: Some(short),
                solar,
                ..LandingDirectionAssessment::new(*site, site_order)
            };
            if let Some(plan) = solar.filter(|plan| !plan.safe()) {
                checks.unsafe_solar += 1;
                checks.unsafe_approach += usize::from(plan.approach_clearance < 0.0);
                checks.unsafe_parking += usize::from(plan.parked_clearance < 0.0);
                checks.unsafe_departure += usize::from(plan.departure_clearance < 0.0);
                assessment.rejection = Some("unsafe_solar");
                record(assessment);
                continue;
            }
            let cover = o.cover.iter().find(|s| s.site == site.id);
            if pilot.cover_required(o, exposed) && !super::cover_response::usable_cover(o, site) {
                checks.cover_required += 1;
                assessment.rejection = Some("cover_required");
                record(assessment);
                continue;
            }
            let ground_cost = if objective.is_some() {
                let Some(survey) = survey else {
                    checks.survey_unavailable += 1;
                    assessment.rejection = Some("survey_unavailable");
                    record(assessment);
                    continue;
                };
                let Some(route) = survey.sites.iter().find(|r| r.site == Some(site.id)) else {
                    checks.route_absent += 1;
                    assessment.rejection = Some("route_absent");
                    record(assessment);
                    continue;
                };
                let Some(cost) = route.cost() else {
                    checks.route_unusable += 1;
                    assessment.rejection = Some("route_unusable");
                    record(assessment);
                    continue;
                };
                cost
            } else {
                0.0
            };
            // Preserve the existing preference for a short approach when the
            // ship is already unexposed, including its objective multiplier.
            let penalty = if pilot.commit_descent && !exposed {
                0.0
            } else {
                cover.map_or(400.0, |s| {
                    if s.departure && s.approach && s.grounded {
                        0.0
                    } else {
                        400.0
                    }
                })
            };
            checks.eligible += 1;
            let approach = approach_score(short, side, solar.is_some(), p.planet.radius);
            let cover_penalty = penalty * if objective.is_some() { 10.0 } else { 1.0 };
            let score = approach + cover_penalty + ground_cost;
            assessment.approach_score = Some(approach);
            assessment.cover_penalty = Some(cover_penalty);
            assessment.ground_score = Some(ground_cost);
            assessment.total_score = Some(score);
            record(assessment);
            // Iterator::min_by keeps the first equal minimum. Preserve that
            // rule for both site order and preferred/opposite direction ties.
            if selected
                .as_ref()
                .is_none_or(|best| score.total_cmp(&best.3).is_lt())
            {
                selected = Some((*site, side, solar, score));
            }
        }
    }
    (selected, checks)
}

impl TacticalSortiePilot {
    /// Reassess the supplied current observation with the native ranker. This
    /// is available only at a witnessed fresh choice, never a retained site or
    /// a future landing authorization. The host binds the observation bytes.
    pub(crate) fn landing_choice_comparison(
        &self,
        o: &TacticalSortieObservationV1,
        reference: LandingSiteId,
    ) -> Result<LandingChoiceComparison, &'static str> {
        let p = &o.combat.recovery.flight.pilot;
        let a = self
            .telemetry
            .acquisition
            .ok_or("native acquisition unavailable")?;
        if self.previous_tick != Some(p.tick)
            || p.owner != self.context.actor
            || a.tick != p.tick
            || a.planet != p.planet.index
            || a.revision != p.planet.revision
            || a.reason != "selected_site"
            || self.telemetry.failed_tick.is_some()
            || a.selected_site != self.telemetry.site
            || self.site.map(|s| s.id) != a.selected_site
            || reference.planet != p.planet.index
            || reference.bearing >= scenario_spacewars::surface_sortie::pilot::LANDING_SITE_COUNT
        {
            return Err("not the current native site choice");
        }
        if o.cover.len() > 64
            || self.rejected_sites.len() > 128
            || self.solar_rejected.len() > 64
            || p.sites.len()
                > usize::from(scenario_spacewars::surface_sortie::pilot::LANDING_SITE_COUNT)
            || p.sites.iter().enumerate().any(|(i, site)| {
                site.id.planet != p.planet.index
                    || site.id.bearing
                        >= scenario_spacewars::surface_sortie::pilot::LANDING_SITE_COUNT
                    || p.sites[..i].iter().any(|s| s.id == site.id)
            })
        {
            return Err("site comparison exceeds the bounded unique survey");
        }
        let objective = self
            .commit_descent
            .then(|| LandingObjective::read(p))
            .flatten();
        let reason = o
            .landing_objective
            .as_ref()
            .and_then(|s| survey_rejection(o, objective, s));
        if reason != a.survey_rejected_by
            || objective != a.objective
            || self.required_site != a.required_site
            || p.site_query != a.site_query
            || p.sites.len() != a.sites_available
        {
            return Err("selection context differs from native telemetry");
        }
        let survey = o.landing_objective.as_ref().filter(|_| reason.is_none());
        let exposed = exposed(o);
        let mut assessments = Vec::with_capacity(p.sites.len() * 2);
        let (best, checks) = select(self, o, objective, survey, exposed, |v| assessments.push(v));
        let best = best.ok_or("native choice could not be reproduced")?;
        if Some(best.0) != self.site
            || best.1 != self.side
            || best.2 != self.telemetry.solar
            || checks != a.checks
        {
            return Err("native choice could not be reproduced");
        }
        if assessments.iter().any(|r| {
            [
                r.short_angle,
                r.approach_score,
                r.cover_penalty,
                r.ground_score,
                r.total_score,
            ]
            .into_iter()
            .flatten()
            .any(|v| !v.is_finite())
                || r.solar.is_some_and(|s| {
                    [
                        s.arrival_seconds,
                        s.surface_seconds,
                        s.side,
                        s.approach_clearance,
                        s.parked_clearance,
                        s.departure_clearance,
                        s.departure_side,
                    ]
                    .into_iter()
                    .any(|v| !v.is_finite())
                })
        }) {
            return Err("non-finite landing assessment");
        }
        let selected = *assessments
            .iter()
            .find(|v| v.site == best.0.id && v.side == Some(best.1) && v.rejection.is_none())
            .unwrap();
        let reference_best = assessments
            .iter()
            .filter(|v| v.site == reference && v.rejection.is_none())
            .min_by(|a, b| a.total_score.unwrap().total_cmp(&b.total_score.unwrap()))
            .copied();
        let classification = if selected.site == reference {
            "same_site"
        } else if let Some(r) = reference_best {
            if r.total_score
                .unwrap()
                .total_cmp(&selected.total_score.unwrap())
                .is_eq()
            {
                "native_order_tie"
            } else {
                "higher_reference_score"
            }
        } else if assessments.iter().any(|v| v.site == reference) {
            "reference_rejected"
        } else {
            "reference_absent"
        };
        Ok(LandingChoiceComparison {
            model: "native_landing_choice_comparison_v1",
            tick: p.tick,
            planet: p.planet.index,
            revision: p.planet.revision,
            reference,
            selected,
            reference_best,
            classification,
            exposed,
            commit_descent: self.commit_descent,
            required_site: self.required_site,
            site_count: p.sites.len(),
            objective,
            checks,
            assessments,
        })
    }
}
