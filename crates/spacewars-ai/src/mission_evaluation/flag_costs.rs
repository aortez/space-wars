//! Opt-in v13 evidence candidate. No coefficients or controller gates change.
use super::*;
use scenario_spacewars::surface_sortie::live_planning::FlagSurveySample;

pub(super) const MODEL: &str = "capture_value_published_flags_v1";
pub(super) const SCOPE: &str = "conditional historical landing/walk/return costs from published flag certificates; unchanged flag/material identity; native arrival, acquisition and exposure remain unmodelled; live controls reacquire their own site and route";

pub(super) fn enabled(policy: &str) -> bool {
    policy == crate::mission_policy::MissionPolicy::ValuePlanner.id()
}

pub(super) fn observe(
    state: &mut ActorState,
    o: &MissionObservationV1,
    mission: &MissionTelemetry,
    samples: &[&FlagSurveySample],
) -> Vec<FlagShadowAdmission> {
    let p = &o.local.combat.recovery.flight.pilot;
    let request = state.flag_survey.as_ref().map(|s| s.request);
    let base = snapshot(o, mission, &state.evidence);
    let mut admissions = Vec::new();
    let mut accepted = Vec::new();
    // The shared planner holds two sites for each of two actors. Bound the
    // input before filtering, including malformed callers.
    for sample in samples
        .iter()
        .take(4)
        .copied()
        .filter(|s| s.actor == p.owner)
        .take(2)
    {
        let key = o
            .planets
            .iter()
            .take(MAX_PLANETS)
            .find(|planet| planet.index == sample.site.planet)
            .map(PlanetKey::read);
        let costs = flag_value_shadow::admit(o, &base, request, sample, key.as_ref(), true);
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
        if let (Ok(costs), Some(key)) = (costs, key) {
            accepted.push((index, sample, key, costs));
        }
    }
    accepted.sort_by(|a, b| {
        a.3.total()
            .total_cmp(&b.3.total())
            .then(b.1.source_tick.cmp(&a.1.source_tick))
            .then(a.1.site.bearing.cmp(&b.1.site.bearing))
    });
    for (index, sample, key, costs) in accepted {
        if state
            .evidence
            .iter()
            .any(|old| old.key.planet == key.planet && old.costs.is_some())
        {
            continue;
        }
        state.evidence.retain(|old| old.key.planet != key.planet);
        if state.evidence.len() == MAX_PLANETS {
            state.evidence.remove(0);
        }
        state.evidence.push(LocalEvidence {
            remote: true,
            key,
            site: sample.site,
            tick: sample.source_tick,
            gravity: sample.validation.as_ref().unwrap().source_gravity,
            costs: Some(costs),
            reason: None,
            choice: None,
            route_source_tick: Some(sample.source_tick),
            route_validated_tick: sample.validated_tick,
            route_objective: Some(sample.objective),
        });
        admissions[index].used = true;
    }
    admissions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_pilot::MissionEvent;
    use scenario_spacewars::surface_sortie::live_planning::FlagSurveyRequest;

    fn fixture(
        enabled: bool,
    ) -> (
        MissionObservationV1,
        MissionTelemetry,
        MissionEvaluator,
        FlagSurveySample,
    ) {
        let (mut o, mut evaluator, request, sample) = flag_value_shadow::tests::fixture();
        let (_, _, bot) = super::super::tests::fixture();
        let mut mission = bot.telemetry().clone();
        mission.policy = "material_mission_v13";
        mission.target = Some(0);
        mission.events.push(MissionEvent {
            tick: 1,
            planet: Some(0),
            kind: "selected",
            reason: None,
        });
        evaluator.flag_cost_seats = [enabled, false];
        o.local.combat.recovery.flight.pilot.tick = 103;
        evaluator.flag_request(&o, &mission).unwrap();
        evaluator
            .actors
            .get_mut(&0)
            .unwrap()
            .flag_survey
            .as_mut()
            .unwrap()
            .request = request;
        (o, mission, evaluator, sample)
    }

    fn complete(e: &mut MissionEvaluator, o: &mut MissionObservationV1) {
        let tick = o.local.combat.recovery.flight.pilot.tick;
        e.advance(tick, DEFAULT_WORK);
        e.advance(tick + 1, DEFAULT_WORK);
        o.local.combat.recovery.flight.pilot.tick += 1;
    }

    #[test]
    fn opt_in_keeps_native_evidence_epochs_and_pins_source_before_selection() {
        let (mut o, m, mut e, sample) = fixture(true);
        e.observe_with_flag_surveys(&o, &m, None, &[&sample]);
        complete(&mut e, &mut o);
        let r = e.latest(PlayerId::PLAYER_1).unwrap();
        assert_eq!(r.model, MODEL);
        assert_eq!(r.flag_admissions.as_ref().unwrap().len(), 1);
        assert!(r.flag_admissions.as_ref().unwrap()[0].used);
        let c = r.candidates.iter().find(|c| c.planet == 1).unwrap();
        assert_eq!(c.evidence_tick, Some(60));
        assert_eq!(c.route_validated_tick, Some(80));
        assert!(c.total_seconds.is_some());
        assert!(e.selection(&o, &m).is_some());
        let mut changed = o.clone();
        changed.planets[1]
            .claim
            .as_mut()
            .unwrap()
            .flag
            .as_mut()
            .unwrap()
            .position
            .x += 0.01;
        assert!(e.selection(&changed, &m).is_none());
        e.reset();
        assert!(e.uses_flag_costs(PlayerId::PLAYER_1));
        assert!(!e.uses_flag_costs(PlayerId::PLAYER_2));
    }

    #[test]
    fn absence_negative_replacement_and_expiry_revoke_a_published_cost() {
        for mutation in 0..4 {
            let (mut o, m, mut e, mut sample) = fixture(true);
            e.observe_with_flag_surveys(&o, &m, None, &[&sample]);
            complete(&mut e, &mut o);
            assert!(e.selection(&o, &m).is_some());
            o.local.combat.recovery.flight.pilot.tick += 1;
            match mutation {
                0 => {}
                1 => sample.reason = Some("round trip incomplete"),
                2 => sample.validated_tick = None,
                3 => o.local.combat.recovery.flight.pilot.tick += MAX_EVIDENCE_AGE,
                _ => unreachable!(),
            }
            let samples = [&sample];
            e.observe_with_flag_surveys(&o, &m, None, if mutation == 0 { &[] } else { &samples });
            assert!(e.selection(&o, &m).is_none());
            complete(&mut e, &mut o);
            assert!(
                e.latest(PlayerId::PLAYER_1)
                    .unwrap()
                    .candidates
                    .iter()
                    .find(|c| c.planet == 1)
                    .unwrap()
                    .total_seconds
                    .is_none()
            );
        }
    }

    #[test]
    fn v13_opt_in_does_not_override_other_policy_models_or_evidence() {
        use crate::mission_policy::MissionPolicy;
        for policy in MissionPolicy::ALL {
            if policy == MissionPolicy::ValuePlanner {
                continue;
            }
            let reports = [false, true].map(|configured| {
                let (mut o, mut m, mut e, sample) = fixture(configured);
                m.policy = policy.id();
                let request = e.actors[&0].flag_survey.as_ref().unwrap().request;
                e.observe_with_flag_surveys(&o, &m, Some(request), &[&sample]);
                complete(&mut e, &mut o);
                let report = e.latest(PlayerId::PLAYER_1).unwrap();
                assert_eq!(report.model, model_for_policy(policy.id()));
                assert!(report.flag_admissions.is_none());
                assert!(report.flag_cost_scope.is_none());
                serde_json::to_value(report).unwrap()
            });
            assert_eq!(reports[0], reports[1], "{}", policy.id());
        }
    }

    #[test]
    fn predecessor_ignores_flag_costs_and_current_target_demand_is_opt_in() {
        let (mut o, mut m, mut e, sample) = fixture(false);
        e.observe_with_flag_surveys(&o, &m, None, &[&sample]);
        complete(&mut e, &mut o);
        assert_eq!(e.latest(PlayerId::PLAYER_1).unwrap().model, value::MODEL);
        assert!(
            e.latest(PlayerId::PLAYER_1)
                .unwrap()
                .flag_admissions
                .is_none()
        );
        assert!(e.selection(&o, &m).is_none());
        m.target = Some(1);
        assert!(e.flag_request(&o, &m).is_none());
        e.flag_cost_seats = [true, false];
        let request: FlagSurveyRequest = e.flag_request(&o, &m).unwrap();
        assert_eq!(request.objective.planet, 1);
        assert_ne!(request.candidates[0], request.candidates[1]);
    }
}
