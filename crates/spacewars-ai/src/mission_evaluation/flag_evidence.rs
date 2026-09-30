//! V14/V15's narrow admission of historical remote flag walking evidence.
//! These costs cannot replace fresh native landing or surface-route evidence.
use super::*;
use scenario_spacewars::surface_sortie::live_planning::{FlagSurveyRequest, FlagSurveySample};

pub(super) const MODEL: &str = "capture_mission_survey_value_v1";

pub(super) fn enabled(policy: &str) -> bool {
    policy == crate::mission_policy::MissionPolicy::SurveyValuePlanner.id()
        || policy == crate::mission_policy::MissionPolicy::LandingPlanPlanner.id()
}

pub(super) fn is_flag(sample: &LocalEvidence) -> bool {
    sample.remote && sample.route_objective.is_some()
}

pub(super) fn read(
    o: &MissionObservationV1,
    base: &MissionEvaluation,
    request: Option<FlagSurveyRequest>,
    samples: &[&FlagSurveySample],
) -> Option<LocalEvidence> {
    let actor = o.local.combat.recovery.flight.pilot.owner;
    // The shared planner retains two sites per actor. Bound malformed input
    // before filtering, just as the historical shadow does.
    samples
        .iter()
        .take(4)
        .copied()
        .filter(|s| s.actor == actor)
        .take(2)
        .filter_map(|sample| {
            let planet = o
                .planets
                .iter()
                .take(MAX_PLANETS)
                .find(|p| p.index == sample.site.planet)?;
            let key = PlanetKey::read(planet);
            let costs =
                flag_value_shadow::admit(o, base, request, sample, Some(&key), true).ok()?;
            Some(LocalEvidence {
                remote: true,
                key,
                site: sample.site,
                tick: sample.source_tick,
                gravity: 0.0,
                costs: Some(costs),
                reason: None,
                choice: None,
                route_source_tick: Some(sample.source_tick),
                route_validated_tick: sample.validated_tick,
                route_objective: Some(sample.validation.as_ref()?.source_objective),
            })
        })
        .min_by(|a, b| {
            a.costs
                .as_ref()
                .unwrap()
                .total()
                .total_cmp(&b.costs.as_ref().unwrap().total())
                .then(b.tick.cmp(&a.tick))
                .then(a.site.bearing.cmp(&b.site.bearing))
        })
}

#[cfg(test)]
mod tests;
