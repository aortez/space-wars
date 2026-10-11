use super::*;
use crate::{
    BrainReset,
    mission_pilot::MissionEvent,
    mission_policy::{MissionBot, MissionPolicy},
};

fn fixture() -> (
    MissionObservationV1,
    MissionTelemetry,
    MissionEvaluator,
    FlagSurveyRequest,
    FlagSurveySample,
) {
    let (o, old, request, sample) = flag_value_shadow::tests::fixture();
    let mut mission = MissionBot::new(
        MissionPolicy::SurveyValuePlanner,
        BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        },
        Default::default(),
    )
    .telemetry()
    .clone();
    mission.target = Some(0);
    mission.events.push(MissionEvent {
        tick: 90,
        planet: Some(0),
        kind: "selected",
        reason: None,
    });
    let mut evaluator = MissionEvaluator::new(2);
    evaluator.actors.insert(
        0,
        ActorState {
            evidence: old.actors[&0].latest_evidence.clone(),
            ..Default::default()
        },
    );
    (o, mission, evaluator, request, sample)
}

fn publish(
    o: &mut MissionObservationV1,
    mission: &MissionTelemetry,
    evaluator: &mut MissionEvaluator,
    request: Option<FlagSurveyRequest>,
    sample: &FlagSurveySample,
) {
    for _ in 0..2 {
        evaluator.observe_with_flag_surveys(o, mission, request, &[sample]);
        let tick = o.local.combat.recovery.flight.pilot.tick;
        assert!(evaluator.advance(tick, DEFAULT_WORK).graph <= 2);
        o.local.combat.recovery.flight.pilot.tick += 1;
    }
}

#[test]
fn survey_value_successors_consume_published_costs_and_retain_the_original_source() {
    for policy in [
        MissionPolicy::ValuePlanner,
        MissionPolicy::SurveyValuePlanner,
        MissionPolicy::LandingPlanPlanner,
        MissionPolicy::ApproachSurveyPlanner,
    ] {
        let (mut o, mut mission, mut evaluator, request, sample) = fixture();
        mission.policy = policy.id();
        let mut ordinary = evaluator.clone();
        let mut same_o = o.clone();
        publish(&mut same_o, &mission, &mut ordinary, None, &sample);
        publish(&mut o, &mission, &mut evaluator, Some(request), &sample);
        let report = evaluator.latest(PlayerId::PLAYER_1).unwrap();
        if policy == MissionPolicy::ValuePlanner {
            assert_eq!(Some(report), ordinary.latest(PlayerId::PLAYER_1));
            assert!(evaluator.selection(&o, &mission).is_none());
        } else {
            assert_eq!(report.model, MODEL);
            assert_eq!(report.value_comparison.as_ref().unwrap().preferred, Some(1));
            let candidate = &report.candidates[1];
            assert_eq!(candidate.evidence_tick, Some(60));
            assert_eq!(candidate.evidence_age_ticks, Some(42));
            assert_eq!(candidate.route_validated_tick, Some(80));
            assert_eq!(
                candidate.local,
                Some(model::walking_costs(sample.route.as_ref().unwrap(), 3.0).unwrap())
            );
            assert_eq!(report.charged_work, 3);
            let selection = evaluator.selection(&o, &mission).unwrap();
            assert_eq!(selection.destination, 1);
            assert_eq!(selection.value.unwrap().destination.ownership_swing, 2);
        }
    }
}

#[test]
fn consumption_rechecks_strict_flag_identity_age_recovery_and_commitment() {
    let (mut o, mission, mut evaluator, request, sample) = fixture();
    publish(&mut o, &mission, &mut evaluator, Some(request), &sample);
    assert!(evaluator.selection(&o, &mission).is_some());
    for mutation in 0..8 {
        let mut changed = o.clone();
        match mutation {
            0 => {
                changed.planets[1]
                    .claim
                    .as_mut()
                    .unwrap()
                    .flag
                    .as_mut()
                    .unwrap()
                    .position
                    .x += 0.01
            }
            1 => changed.planets[1].radius += 0.1,
            2 => {
                changed.planets[1]
                    .claim
                    .as_mut()
                    .unwrap()
                    .flag_interaction_range += 0.01
            }
            3 => changed.planets[1].revision += 1,
            4 => {
                changed.local.combat.recovery.flight.pilot.tick =
                    sample.source_tick + MAX_EVIDENCE_AGE + 1
            }
            5 => changed.local.combat.recovery.flight.pilot.location = PilotLocation::OnFoot,
            6 => changed.local.combat.recovery.flight.pilot.ship_form = ShipForm::EscapePod,
            7 => changed.match_context.as_mut().unwrap().remaining_seconds = Some(0.1),
            _ => unreachable!(),
        }
        assert!(
            evaluator.selection(&changed, &mission).is_none(),
            "mutation {mutation}"
        );
    }
    let mut other_visit = mission.clone();
    other_visit.events.last_mut().unwrap().tick += 1;
    assert!(evaluator.selection(&o, &other_visit).is_none());
    // A host that stops supplying demand cannot renew the earlier publication.
    evaluator.observe_with_flag_surveys(&o, &mission, None, &[&sample]);
    assert!(evaluator.latest(PlayerId::PLAYER_1).is_none());
    assert!(evaluator.selection(&o, &mission).is_none());
}

#[test]
fn source_expiry_while_queued_cannot_authorize_a_choice() {
    let (mut o, mission, mut evaluator, request, sample) = fixture();
    o.local.combat.recovery.flight.pilot.tick = sample.source_tick + MAX_EVIDENCE_AGE - 1;
    evaluator.observe_with_flag_surveys(&o, &mission, Some(request), &[&sample]);
    assert!(evaluator.latest(PlayerId::PLAYER_1).is_none());
    o.local.combat.recovery.flight.pilot.tick = 1861;
    for tick in 1859..=1861 {
        evaluator.advance(
            tick,
            Work {
                graph: 1,
                physics_queries: 0,
            },
        );
    }
    let report = evaluator.latest(PlayerId::PLAYER_1).unwrap();
    assert_eq!(report.source_tick, 1859);
    assert!(report.completed_tick.unwrap() - report.source_tick < MAX_RESULT_AGE);
    assert!(evaluator.selection(&o, &mission).is_none());
}

#[test]
fn current_flag_is_a_reference_too_but_existing_local_failures_are_not_overridden() {
    let (o, mut mission, evaluator, request, sample) = fixture();
    mission.target = Some(1);
    mission.events[0].planet = Some(1);
    let mut base = snapshot(&o, &mission, &evaluator.actors[&0].evidence);
    let admitted = read(&o, &base, Some(request), &[&sample]).unwrap();
    assert!(base.candidates[0].current);
    assert_eq!(admitted.site, sample.site);
    base.candidates[0].unknown_reason = Some("round trip incomplete");
    assert!(read(&o, &base, Some(request), &[&sample]).is_none());
    base.candidates[0].unknown_reason = Some("remote or local surface unmeasured");
    base.candidates[0].local = Some(model::no_flag_costs());
    assert!(read(&o, &base, Some(request), &[&sample]).is_none());
}

#[test]
fn survey_value_successors_request_current_destinations_without_changing_v13_requests() {
    let (mut o, mut mission, _, _, _) = fixture();
    o.local.combat.recovery.flight.pilot.site_query =
        scenario_spacewars::surface_sortie::pilot::LandingSiteQuery::NotRequested;
    o.local.combat.recovery.flight.pilot.landing.supported_feet = 0;
    for policy in [
        MissionPolicy::ValuePlanner,
        MissionPolicy::SurveyValuePlanner,
        MissionPolicy::LandingPlanPlanner,
        MissionPolicy::ApproachSurveyPlanner,
    ] {
        mission.policy = policy.id();
        let mut host = MissionEvaluator::new(1);
        mission.target = Some(0);
        let neutral = host.alternative_request(&o, &mission);
        assert_eq!(neutral.is_some(), policy.consumes_flag_surveys());
        mission.target = Some(1);
        let flag = host.flag_request(&o, &mission);
        assert_eq!(flag.is_some(), policy.consumes_flag_surveys());
        if let Some(request) = flag {
            assert_eq!(request.objective.planet, 1);
        }
    }
}
