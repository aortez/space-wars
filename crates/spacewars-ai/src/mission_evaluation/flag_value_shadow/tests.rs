use super::*;
use crate::mission_policy::MissionPolicy;
use scenario_spacewars::surface_sortie::{
    PlanetFlagObservation,
    destination_cover::CoverMeasurement,
    ground_navigation::{GroundNode, GroundRouteDiagnostics},
    landing_objective::LandingObjectiveRoute,
    live_planning::FlagSurveyValidation,
    pilot::{PilotLandingSite, PilotMotion},
};

fn fixture() -> (
    MissionObservationV1,
    MissionEvaluator,
    FlagSurveyRequest,
    FlagSurveySample,
) {
    let (_, mut o, bot) = super::super::tests::fixture();
    o.sun = None;
    o.boundary.center = Vec2::ZERO;
    o.boundary.radius = 2000.0;
    o.planets.truncate(3);
    for (i, planet) in o.planets.iter_mut().enumerate() {
        planet.motion.position = [Vec2::ZERO, Vec2::new(300.0, 150.0), Vec2::new(-500.0, 0.0)][i];
        planet.motion.velocity = Vec2::ZERO;
        planet.motion.angle = 0.0;
        planet.radius = 50.0;
        let claim = planet.claim.as_mut().unwrap();
        claim.owner = [None, Some(PlayerId::PLAYER_2), Some(PlayerId::PLAYER_1)][i];
        claim.flag = (i == 1).then_some(PlanetFlagObservation {
            player: PlayerId::PLAYER_2,
            position: planet.motion.position + Vec2::Y * 50.0,
            normal: Vec2::Y,
            raised_fraction: 1.0,
        });
    }
    o.match_context.as_mut().unwrap().owned_planets = [1, 1];
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick = 100;
    p.ship.position = Vec2::new(0.0, 150.0);
    p.ship.velocity = Vec2::ZERO;
    p.ship.angle = 0.0;
    p.ship.spin = 0.0;
    p.gravity = -Vec2::Y * 10.0;
    p.planet = o.planets[0].clone();
    p.sites.clear();
    let mut mission = bot.telemetry().clone();
    mission.policy = MissionPolicy::ValuePlanner.id();
    mission.target = Some(0);
    let mut evaluator = MissionEvaluator::new(2);
    let mut known = super::super::tests::known(&o.planets[0], 100, 70.0);
    known.remote = true;
    evaluator.actors.insert(
        0,
        ActorState {
            evidence: vec![known],
            ..Default::default()
        },
    );
    for tick in 100..=102 {
        o.local.combat.recovery.flight.pilot.tick = tick;
        evaluator.observe(&o, &mission);
        evaluator.advance(tick, DEFAULT_WORK);
    }
    assert_eq!(
        evaluator.latest(PlayerId::PLAYER_1).unwrap().source_tick,
        100
    );
    let planet = &o.planets[1];
    let site = LandingSiteId {
        planet: 1,
        bearing: 0,
    };
    let objective = LandingObjective {
        planet: 1,
        revision: planet.revision,
        owner: PlayerId::PLAYER_2,
        position: Vec2::Y * 50.0,
        range: planet.claim.as_ref().unwrap().flag_interaction_range - 0.2,
    };
    let request = FlagSurveyRequest {
        generation: 50,
        objective,
        candidates: [site, LandingSiteId { bearing: 1, ..site }],
    };
    let diagnostics = GroundRouteDiagnostics {
        failure: None,
        partial: false,
        start_node: Some(0),
        start_distance: Some(0.0),
        destination_nodes: 1,
        nearest_destination_distance: Some(0.0),
        reachable_nodes: 2,
        closest_reachable_distance: Some(0.0),
        length: 5.0,
        jumps: 0,
        flights: 0,
    };
    let mut sample = FlagSurveySample {
        actor: PlayerId::PLAYER_1,
        generation: 50,
        site,
        source_tick: 60,
        completed_tick: 80,
        validated_tick: Some(80),
        objective,
        measurement: CoverMeasurement {
            tick: 60,
            revision: planet.revision,
            planet: planet.motion,
            ship_form: ShipForm::Ship,
            opponent: None,
            queries: 40,
            finding: CoverFinding::Measured,
            cover: None,
            climb_clear: Some(true),
            site: Some(PilotLandingSite {
                hatch_has_settling_margin: true,
                id: site,
                revision: planet.revision,
                local_position: Vec2::Y * 50.0,
                position: planet.motion.position + Vec2::Y * 50.0,
                normal: Vec2::Y,
                velocity: Vec2::ZERO,
                vehicle_position: planet.motion.position + Vec2::Y * 55.0,
                hatch_position: planet.motion.position + Vec2::Y * 50.0,
                boarding_hatches: [Some(planet.motion.position + Vec2::Y * 50.0), None],
            }),
        },
        route: Some(LandingObjectiveRoute {
            crossing: None,
            site: Some(site),
            outbound: diagnostics.clone(),
            returning: Some(diagnostics),
            endpoint: Some(GroundNode {
                id: 1,
                position: Vec2::Y * 50.0,
                normal: Vec2::Y,
            }),
        }),
        reason: None,
        geometry: None,
        validation: Some(FlagSurveyValidation {
            model: "captured_query_unions_v1",
            source_objective: objective,
            source_radius: 50.0,
            source_gravity: 10.0,
            current_gravity: 10.0,
            source_areas: Vec::new(),
            captured_queries: 40,
            walking_queries: 100,
            complete: true,
            predicates_valid: true,
            predicate_failure: None,
            geometry: Default::default(),
        }),
        graph: 100,
        physics_queries: 140,
    };
    sample.validation.as_mut().unwrap().geometry.valid = true;
    (o, evaluator, request, sample)
}

#[test]
fn historical_flag_fills_only_missing_costs_and_leaves_baseline_unchanged() {
    let (o, evaluator, request, sample) = fixture();
    let before = evaluator.latest(PlayerId::PLAYER_1).unwrap().clone();
    let mut shadow = FlagValueShadow::new(2);
    shadow.observe(&o, &evaluator, Some(request), &[&sample]);
    assert!(shadow.pending(PlayerId::PLAYER_1));
    assert_eq!(shadow.advance(102, Work::default()), Work::default());
    assert_eq!(shadow.advance(102, DEFAULT_WORK), Work::default());
    for tick in 103..=105 {
        assert_eq!(
            shadow
                .advance(
                    tick,
                    Work {
                        graph: 1,
                        physics_queries: 384
                    }
                )
                .graph,
            1
        );
    }
    let report = shadow.latest(PlayerId::PLAYER_1).unwrap();
    assert_eq!(report.baseline, before);
    assert_eq!(evaluator.latest(PlayerId::PLAYER_1).unwrap(), &before);
    assert_eq!(preferred(&report.baseline), None);
    assert_eq!(preferred(&report.augmented), Some(1));
    assert!(report.preference_changed && report.admissions[0].used);
    assert_eq!(report.augmented.candidates[1].evidence_tick, Some(60));
    assert_eq!(
        report.augmented.candidates[1].route_validated_tick,
        Some(80)
    );
    assert_eq!(report.augmented.transfer_source, before.transfer_source);
    assert_eq!(report.augmented.match_context, before.match_context);
    assert_eq!(report.augmented.charged_work, 3);
    assert_eq!(shadow.charged_total, 3);
    assert_eq!(shadow.clone().latest(PlayerId::PLAYER_1), Some(report));
}

#[test]
fn admission_rejects_incompatible_stale_future_and_unsupported_evidence() {
    for mutation in 0..28 {
        let (mut o, evaluator, mut request, mut s) = fixture();
        let mut base = evaluator.latest(PlayerId::PLAYER_1).unwrap().clone();
        let key = &evaluator.actors[&0]
            .latest_dependencies
            .as_ref()
            .unwrap()
            .planets[1];
        match mutation {
            0 => s.actor = PlayerId::PLAYER_2,
            1 => s.generation += 1,
            2 => s.site.bearing = 4,
            3 => s.source_tick = 49,
            4 => {
                s.completed_tick = 101;
                s.validated_tick = Some(101);
            }
            5 => s.validated_tick = Some(79),
            6 => s.measurement.tick = 61,
            7 => o.local.combat.recovery.flight.pilot.tick = 1861,
            8 => s.validation = None,
            9 => s.validation.as_mut().unwrap().complete = false,
            10 => s.validation.as_mut().unwrap().geometry.valid = false,
            11 => s.validation.as_mut().unwrap().predicates_valid = false,
            12 => s.reason = Some("local rejection"),
            13 => o.planets[1].revision += 1,
            14 => o.planets[1].radius += 0.1,
            15 => o.planets[1].claim.as_mut().unwrap().owner = None,
            16 => o.planets[1].claim.as_mut().unwrap().flag = None,
            17 => {
                o.planets[1]
                    .claim
                    .as_mut()
                    .unwrap()
                    .flag
                    .as_mut()
                    .unwrap()
                    .position
                    .x += 0.1
            }
            18 => s.measurement.climb_clear = Some(false),
            19 => s.measurement.site.as_mut().unwrap().boarding_hatches = [None; 2],
            20 => s.route.as_mut().unwrap().outbound.jumps = 1,
            21 => {
                s.route
                    .as_mut()
                    .unwrap()
                    .returning
                    .as_mut()
                    .unwrap()
                    .flights = 1
            }
            22 => s.route.as_mut().unwrap().outbound.length = f32::NAN,
            23 => s.route.as_mut().unwrap().returning = None,
            24 => {
                s.route
                    .as_mut()
                    .unwrap()
                    .endpoint
                    .as_mut()
                    .unwrap()
                    .position += Vec2::Y * 10.0
            }
            25 => {
                base.candidates[1].unknown_reason =
                    Some("transfer requires unmodelled boundary guidance")
            }
            26 => {
                request.objective.range += 0.001;
                s.objective = request.objective;
                s.validation.as_mut().unwrap().source_objective = request.objective;
            }
            27 => o.planets[1].claim.as_mut().unwrap().stage_required_seconds = f32::NAN,
            _ => unreachable!(),
        }
        assert!(
            admit(&o, &base, Some(request), &s, Some(key)).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn historical_cover_is_ignored_and_equal_source_age_is_allowed_without_renewal() {
    let (mut o, evaluator, request, mut s) = fixture();
    let base = evaluator.latest(PlayerId::PLAYER_1).unwrap();
    let key = &evaluator.actors[&0]
        .latest_dependencies
        .as_ref()
        .unwrap()
        .planets[1];
    let expected = admit(&o, base, Some(request), &s, Some(key)).unwrap();
    s.measurement.opponent = Some(
        scenario_spacewars::surface_sortie::destination_cover::CoverOpponent {
            owner: PlayerId::PLAYER_2,
            motion: PilotMotion {
                position: Vec2::new(f32::NAN, 0.0),
                ..s.measurement.planet
            },
            armed_ship: true,
        },
    );
    o.local.combat.recovery.flight.pilot.tick = 1860;
    assert_eq!(admit(&o, base, Some(request), &s, Some(key)), Ok(expected));
    o.local.combat.recovery.flight.pilot.tick += 1;
    assert_eq!(
        admit(&o, base, Some(request), &s, Some(key)),
        Err("flag source expired")
    );
}

#[test]
fn delayed_work_withholds_rankings_and_reset_drops_previous_episode() {
    let (mut o, evaluator, request, sample) = fixture();
    let mut shadow = FlagValueShadow::new(1);
    shadow.observe(&o, &evaluator, Some(request), &[&sample]);
    shadow.advance(220, DEFAULT_WORK);
    shadow.advance(221, DEFAULT_WORK);
    let report = shadow.latest(PlayerId::PLAYER_1).unwrap();
    assert!(report.completion_reason.is_some());
    assert_eq!(preferred(&report.augmented), None);
    assert_eq!(report.augmented.preferred_by_time, None);
    assert!(!report.preference_changed);
    o.local.combat.recovery.flight.pilot.tick = 1;
    shadow.observe(&o, &evaluator, Some(request), &[&sample]);
    assert!(shadow.latest(PlayerId::PLAYER_1).is_none());
}

#[test]
fn unsupported_transfer_and_short_clock_remain_unknown_or_ineligible() {
    for deadline in [false, true] {
        let (mut o, mut evaluator, request, sample) = fixture();
        let base = evaluator
            .actors
            .get_mut(&0)
            .unwrap()
            .latest
            .as_mut()
            .unwrap();
        if deadline {
            base.match_context.as_mut().unwrap().remaining_seconds = Some(0.01);
        } else {
            // Both the augmented route and the baseline use this frozen source.
            o.boundary.radius = 1.0;
            base.transfer_source = Some(TransferSource::read(&o));
        }
        let mut shadow = FlagValueShadow::new(1);
        shadow.observe(&o, &evaluator, Some(request), &[&sample]);
        shadow.advance(102, DEFAULT_WORK);
        shadow.advance(103, DEFAULT_WORK);
        let report = shadow.latest(PlayerId::PLAYER_1).unwrap();
        assert!(report.admissions[0].used);
        assert_eq!(preferred(&report.augmented), None);
        let alternative = &report.augmented.candidates[1];
        if deadline {
            assert_eq!(alternative.reference_exceeds_match_time, Some(true));
        } else {
            assert!(alternative.unknown_reason.is_some());
        }
    }
}

#[test]
fn pending_refresh_cannot_renew_the_baseline_transfer_source() {
    let (mut o, mut evaluator, request, sample) = fixture();
    let (_, _, bot) = super::super::tests::fixture();
    let mut mission = bot.telemetry().clone();
    mission.policy = MissionPolicy::ValuePlanner.id();
    mission.target = Some(0);
    o.local.combat.recovery.flight.pilot.tick = 160;
    o.local.combat.recovery.flight.pilot.ship.position.x = 1.9;
    evaluator.observe(&o, &mission);
    assert!(evaluator.pending(PlayerId::PLAYER_1));
    o.local.combat.recovery.flight.pilot.tick = 161;
    o.local.combat.recovery.flight.pilot.ship.position.x = 3.0;
    evaluator.observe(&o, &mission);
    let base = evaluator.latest(PlayerId::PLAYER_1).unwrap();
    assert_eq!(base.source_tick, 100);
    assert!(!base.transfer_source.as_ref().unwrap().is_current(&o));
    let mut shadow = FlagValueShadow::new(1);
    shadow.observe(&o, &evaluator, Some(request), &[&sample]);
    assert!(!shadow.pending(PlayerId::PLAYER_1));
    o.local.combat.recovery.flight.pilot.tick = 162;
    o.local.combat.recovery.flight.pilot.ship.position.x = 1.95;
    evaluator.observe(&o, &mission);
    shadow.observe(&o, &evaluator, Some(request), &[&sample]);
    assert!(shadow.pending(PlayerId::PLAYER_1));
}

#[test]
fn new_publication_waits_for_ordinary_refresh_without_consuming_cadence() {
    let (mut o, mut evaluator, request, mut sample) = fixture();
    sample.completed_tick = 101;
    sample.validated_tick = Some(101);
    let mut shadow = FlagValueShadow::new(1);
    shadow.observe(&o, &evaluator, Some(request), &[&sample]);
    assert_eq!(shadow.deferred_source_total, 1);
    assert!(!shadow.pending(PlayerId::PLAYER_1));
    assert_eq!(shadow.advance(102, DEFAULT_WORK), Work::default());
    assert!(shadow.actors[&0].submitted.is_none());
    o.local.combat.recovery.flight.pilot.tick = 103;
    let state = evaluator.actors.get_mut(&0).unwrap();
    state.last_tick = Some(103);
    state.latest.as_mut().unwrap().source_tick = 102;
    state.latest.as_mut().unwrap().completed_tick = Some(103);
    shadow.observe(&o, &evaluator, Some(request), &[&sample]);
    assert!(shadow.pending(PlayerId::PLAYER_1));
    shadow.advance(103, DEFAULT_WORK);
    shadow.advance(104, DEFAULT_WORK);
    let report = shadow.latest(PlayerId::PLAYER_1).unwrap();
    assert!(report.admissions[0].used);
    assert_eq!(report.admitted_tick, 103);
    assert_eq!(report.baseline.source_tick, 102);
    assert_eq!(report.admissions[0].source_tick, 60);
}

#[test]
fn survey_source_expiry_during_dispatch_is_independent_of_baseline_age() {
    let (mut o, mut evaluator, request, sample) = fixture();
    o.local.combat.recovery.flight.pilot.tick = 1859;
    let state = evaluator.actors.get_mut(&0).unwrap();
    state.last_tick = Some(1859);
    let base = state.latest.as_mut().unwrap();
    base.source_tick = 1858;
    base.completed_tick = Some(1859);
    let mut shadow = FlagValueShadow::new(1);
    shadow.observe(&o, &evaluator, Some(request), &[&sample]);
    shadow.advance(1859, DEFAULT_WORK);
    shadow.advance(1861, DEFAULT_WORK);
    let report = shadow.latest(PlayerId::PLAYER_1).unwrap();
    assert!(report.admissions[0].used);
    assert!(report.completion_reason.is_some());
    assert_eq!(preferred(&report.augmented), None);
    assert_eq!(report.augmented.preferred_by_time, None);
    assert_eq!(
        report.completed_tick.unwrap() - report.baseline.source_tick,
        3
    );
}

#[test]
fn two_actors_share_remaining_work_without_resubmitting_a_source() {
    let (mut o, evaluator, request, sample) = fixture();
    let mut other = o.clone();
    other.local.combat.recovery.flight.pilot.owner = PlayerId::PLAYER_2;
    let mut other_evaluator = evaluator.clone();
    let mut state = other_evaluator.actors.remove(&0).unwrap();
    state.latest.as_mut().unwrap().actor = PlayerId::PLAYER_2;
    other_evaluator.actors.insert(1, state);
    let mut other_sample = sample.clone();
    other_sample.actor = PlayerId::PLAYER_2;
    let mut shadow = FlagValueShadow::new(2);
    shadow.observe(&o, &evaluator, Some(request), &[&sample]);
    shadow.observe(&other, &other_evaluator, Some(request), &[&other_sample]);
    for tick in 102..108 {
        assert_eq!(
            shadow.advance(
                tick,
                Work {
                    graph: 1,
                    physics_queries: 384
                }
            ),
            Work {
                graph: 1,
                physics_queries: 0
            }
        );
    }
    assert_eq!(shadow.completed_total, 2);
    assert_eq!(shadow.charged_total, 6);
    assert!(shadow.latest(PlayerId::PLAYER_2).is_some());
    o.local.combat.recovery.flight.pilot.tick = 162;
    let mut evaluator = evaluator;
    evaluator.actors.get_mut(&0).unwrap().last_tick = Some(162);
    shadow.observe(&o, &evaluator, Some(request), &[&sample]);
    assert!(!shadow.pending(PlayerId::PLAYER_1));
    assert_eq!(shadow.advance(162, DEFAULT_WORK), Work::default());
}
