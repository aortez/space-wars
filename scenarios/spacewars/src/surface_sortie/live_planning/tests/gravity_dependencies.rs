use super::*;

#[test]
fn scalar_gravity_changes_do_not_restart_independent_pending_routes() {
    let mut state = state();
    let mut source = target(&state, 0);
    source.combat.recovery.flight.pilot.landing.phase = LandingPhase::Flying;
    let mut live = LiveObjectivePlanner::new(
        1,
        Work {
            graph: 16384,
            physics_queries: 1024,
        },
    )
    .with_route_dependencies();
    let mut o = source.clone();
    live.observe(&state, 0, &mut o);
    let token = live.requests[&0].token;
    let tick = live.requests[&0].measurement_tick;
    let before = state.world.physics.world.snapshot_bytes().unwrap();
    let planet = o.combat.recovery.flight.pilot.planet.index;
    let mut published = None;
    for age in 0..MAX_SURVEY_AGE_TICKS {
        // Freeze physical poses, change only the source's scalar jump gravity.
        if age == 1 {
            state.world.planets[planet].mass *= 2.0;
        }
        o = source.clone();
        o.combat.recovery.flight.pilot.tick += age;
        state.world.tick = o.combat.recovery.flight.pilot.tick;
        live.observe(&state, 0, &mut o);
        assert_eq!(live.requests[&0].token, token);
        assert_eq!(live.requests[&0].measurement_tick, tick);
        if o.landing_objective.is_some() {
            published = o.landing_objective;
            break;
        }
        let r = live.advance(state.world.tick).unwrap();
        assert!(r.charged.graph <= 16384 && r.charged.physics_queries <= 1024);
    }
    let published = published.expect("a walking candidate survives the changed hypothesis");
    assert!(published.validated_routes_only && !published.sites.is_empty());
    assert!(published.sites.iter().all(|r| r.cost().is_some()
        && r.outbound.jumps == 0
        && r.returning.as_ref().unwrap().jumps == 0));
    assert_eq!(live.telemetry.gravity_independent_validations, 1);
    assert!(!live.telemetry.invalidations.contains_key("gravity_changed"));
    let request = &live.requests[&0];
    let p = &o.combat.recovery.flight.pilot;
    assert_eq!(
        LiveObjectivePlanner::valid(&state, 0, p, request, request.objective, false),
        Err("gravity_changed"),
        "whole-region mode retains the strict scalar contract"
    );
    let mut expired = p.clone();
    expired.tick = tick + MAX_SURVEY_AGE_TICKS + 1;
    assert_eq!(
        LiveObjectivePlanner::valid(&state, 0, &expired, request, request.objective, true),
        Err("expired")
    );
    assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
}

// Obtain both leg diagnostics from the real directed solver. This contract
// fixture substitutes an ordinary outward OR return jump into one candidate;
// physical geometry/jump execution remain covered by the route and flight tests.
fn trip_with_jump(returning: bool) -> ground_navigation::GroundRoundTrip {
    use ground_navigation::{GroundEdge, GroundEdgeKind, GroundNode};
    let nodes = [Vec2::new(-4.0, 60.0), Vec2::new(4.0, 60.0)].map(|position| GroundNode {
        id: if position.x < 0.0 { 0 } else { 1 },
        position,
        normal: position.normalized(),
    });
    let map = GroundMap {
        version: 1,
        actor: PlayerId::PLAYER_1,
        planet: 0,
        revision: 1,
        tick: 0,
        nodes: nodes.to_vec(),
        rejected: vec![],
        edges: [(0, 1), (1, 0)]
            .into_iter()
            .enumerate()
            .map(|(i, (from, to))| GroundEdge {
                from,
                to,
                length: 8.0,
                kind: if (i == 1) == returning {
                    GroundEdgeKind::Jump
                } else {
                    GroundEdgeKind::Walk
                },
            })
            .collect(),
    };
    let center = |n: GroundNode| n.position + n.normal * SurfaceSortieState::spec().half_height();
    map.routes().round_trip_to_actor_target(
        nodes[0].position,
        center(nodes[1]),
        0.1,
        center(nodes[0]),
    )
}

#[test]
fn changed_scalar_filters_both_jump_legs_but_keeps_current_powered_routes() {
    let state = SurfaceSortieScenario::init_material_flag_crossing_trial(42, 0);
    let mut o = state.tactical_sortie_observation_for_live_planning(0, LandingSiteQuery::Survey);
    o.combat.recovery.flight.pilot.landing.phase = LandingPhase::Flying;
    let mut live = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_route_dependencies();
    live.observe_with_planning(&state, 0, &mut o, ObjectivePlanning::JetpackRoundTrip);
    live.advance(state.world.tick);
    let request = &live.requests[&0];
    let job = live.queue.job(request.token).unwrap();
    let original = job.output().unwrap().clone();
    let walk = original
        .sites
        .iter()
        .position(|r| r.cost().is_some() && r.crossing.is_none())
        .unwrap();
    assert!(original.sites.iter().any(|r| r.crossing.is_some()));
    let p = &o.combat.recovery.flight.pilot;
    // Isolate the scalar hypothesis from the flight environment. Moving-world
    // replays exercise the same divergence with real orbital updates.
    let mut changed = request.clone();
    changed.gravity -= 0.02;
    for returning in [false, true] {
        let trip = trip_with_jump(returning);
        let mut survey = original.clone();
        survey.sites[walk].outbound = trip.outbound.diagnostics;
        survey.sites[walk].returning = trip.returning.map(|r| r.diagnostics);
        assert!(survey.sites[walk].cost().is_some());
        let mut telemetry = LivePlanningTelemetry::default();
        let result = LiveObjectivePlanner::locally_validated(
            &state,
            0,
            p,
            &changed,
            job,
            survey,
            &mut telemetry,
        )
        .unwrap();
        assert!(result.validated_routes_only);
        assert!(
            !result
                .sites
                .iter()
                .any(|r| r.site == original.sites[walk].site)
        );
        assert!(result.sites.iter().all(|r| r.cost().is_some()
            && r.outbound.jumps == 0
            && r.returning.as_ref().unwrap().jumps == 0));
        assert!(result.sites.iter().any(|r| r.crossing.is_some()));
        assert_eq!(telemetry.jump_gravity_mismatches, 1);
        assert!(telemetry.withheld_jump_routes >= 1);
    }
    // A valid scalar cannot rescue an expired launch window, even in unchanged
    // geometry. Keep the forecast structurally valid to isolate age checking.
    let mut survey = original.clone();
    for r in &mut survey.sites {
        if let Some(c) = &mut r.crossing {
            c.measured_tick = p.tick - 2;
            c.launch_until_tick = p.tick - 1;
            assert!(c.is_valid());
        }
    }
    let result = LiveObjectivePlanner::locally_validated(
        &state,
        0,
        p,
        request,
        job,
        survey,
        &mut LivePlanningTelemetry::default(),
    )
    .unwrap();
    assert!(result.validated_routes_only);
    assert!(
        result
            .sites
            .iter()
            .all(|r| r.crossing.is_none() && r.cost().is_some())
    );
    assert_eq!(
        job.output().unwrap(),
        &original,
        "validation never mutates retained output"
    );
}

#[test]
fn a_stale_jump_on_the_actual_return_cannot_be_replaced_by_a_proposed_landing() {
    let state = SurfaceSortieScenario::init_material_flag_crossing_trial(42, 0);
    let mut o = state.tactical_sortie_observation_for_live_planning(0, LandingSiteQuery::Survey);
    let mut live = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_route_dependencies();
    live.observe_with_planning(&state, 0, &mut o, ObjectivePlanning::JetpackRoundTrip);
    live.advance(state.world.tick);
    let mut request = live.requests[&0].clone();
    let job = live.queue.job(request.token).unwrap();
    let mut survey = job.output().unwrap().clone();
    assert!(request.actual.is_some());
    assert!(
        survey
            .sites
            .iter()
            .any(|r| r.cost().is_some() && r.crossing.is_none())
    );
    survey.actual.as_mut().unwrap().returning =
        trip_with_jump(true).returning.map(|r| r.diagnostics);
    assert!(survey.actual.as_ref().unwrap().cost().is_some());
    request.gravity -= 0.02;
    assert!(
        LiveObjectivePlanner::locally_validated(
            &state,
            0,
            &o.combat.recovery.flight.pilot,
            &request,
            job,
            survey,
            &mut LivePlanningTelemetry::default()
        )
        .is_none()
    );
}

#[test]
fn changed_jump_hypothesis_cannot_publish_negative_answers_even_in_unchanged_geometry() {
    let state = state();
    let mut o = target(&state, 0);
    let p = &mut o.combat.recovery.flight.pilot;
    p.planet
        .claim
        .as_mut()
        .unwrap()
        .flag
        .as_mut()
        .unwrap()
        .position = p.planet.motion.position;
    let mut live = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_route_dependencies();
    live.observe(&state, 0, &mut o);
    live.advance(state.world.tick);
    let request = &live.requests[&0];
    let job = live.queue.job(request.token).unwrap();
    let survey = job.output().unwrap();
    assert!(survey.sites.iter().all(|r| r.cost().is_none()));
    let p = &o.combat.recovery.flight.pilot;
    assert!(
        LiveObjectivePlanner::locally_validated(
            &state,
            0,
            p,
            request,
            job,
            survey.clone(),
            &mut LivePlanningTelemetry::default()
        )
        .is_some()
    );
    let mut changed = request.clone();
    changed.gravity += 0.02;
    assert!(
        LiveObjectivePlanner::locally_validated(
            &state,
            0,
            p,
            &changed,
            job,
            survey.clone(),
            &mut LivePlanningTelemetry::default()
        )
        .is_none()
    );
}
