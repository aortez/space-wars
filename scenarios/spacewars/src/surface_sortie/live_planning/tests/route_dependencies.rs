use super::*;
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, CollisionGroups,
    PhysicsId,
};

fn fixture() -> (
    SurfaceSortieState,
    combat::TacticalSortieObservationV1,
    BodyId,
) {
    let mut state = state();
    // This dependency fixture freezes the other bodies and moves one real
    // collider. End-to-end runners retain the ordinary shared physics step.
    let bodies: Vec<_> = state.world.physics.world.motions().map(|r| r.id).collect();
    for id in bodies {
        state
            .world
            .physics
            .world
            .set_body_kind(id, BodyKind::Fixed, true);
    }
    let p = target(&state, 0).combat.recovery.flight.pilot;
    let entity = PhysicsId::new(terrain::FRAGMENT_ID_BASE + 777);
    let id = BodyId::new(entity, BodyRole::PRIMARY);
    let collider = ColliderId::new(entity, ColliderRole::PRIMARY, 0);
    let mut shape = ColliderSpec::ball(collider, 0.3);
    shape.collision_groups = CollisionGroups::new(1 << 12, u32::MAX);
    assert!(state.world.physics.world.insert_body(
        id,
        BodySpec {
            kind: BodyKind::Fixed,
            position: p.planet.motion.position + Vec2::X * 10.0,
            ..Default::default()
        },
        &[shape]
    ));
    state.world.physics.world.step(DT.as_secs_f32());
    let o = target(&state, 0);
    (state, o, id)
}
fn move_body(state: &mut SurfaceSortieState, id: BodyId, position: Vec2) {
    state.world.physics.world.set_pose(id, position, 0.0, true);
    state.world.physics.world.step(DT.as_secs_f32());
    state.world.tick += 1;
}

#[test]
fn a_path_obstructed_while_waiting_for_landing_sites_is_not_handed_off() {
    let (mut state, mut source, body) = fixture();
    source.combat.recovery.flight.pilot.landing.phase = LandingPhase::Flying;
    let mut live = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_route_dependencies();
    let mut o = source.clone();
    live.observe(&state, 0, &mut o);
    live.advance(state.world.tick);
    let request = live.requests[&0].clone();
    let job = live.queue.job(request.token).unwrap();
    let (site, areas) = job.dependencies().first().unwrap();
    let site = *site;
    assert!(
        job.output()
            .unwrap()
            .sites
            .iter()
            .any(|r| r.site == site && r.cost().is_some())
    );
    let area = *areas
        .iter()
        .find(|a| a.groups == SurfaceSortieState::spec().collision_groups)
        .unwrap();
    let frame = source.combat.recovery.flight.pilot.planet.motion;
    for age in [1, REFRESH_TICKS + 5] {
        state.world.tick = request.measurement_tick + age;
        o = source.clone();
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick = state.world.tick;
        p.site_query = LandingSiteQuery::Deferred {
            next_tick: state.world.tick + 1,
        };
        p.sites.clear();
        live.observe(&state, 0, &mut o);
        assert!(
            o.landing_objective
                .as_ref()
                .unwrap()
                .sites
                .iter()
                .any(|r| r.site == site)
        );
        assert_eq!(live.requests[&0].token, request.token);
    }
    let point = (area.minimum + area.maximum) * 0.5;
    move_body(
        &mut state,
        body,
        frame.position + point.rotate_radians(frame.angle),
    );
    let mut o = target(&state, 0);
    let p = &mut o.combat.recovery.flight.pilot;
    p.landing.phase = LandingPhase::Flying;
    p.planet.claim.as_mut().unwrap().flag = source
        .combat
        .recovery
        .flight
        .pilot
        .planet
        .claim
        .as_ref()
        .unwrap()
        .flag;
    live.observe(&state, 0, &mut o);
    assert!(
        o.landing_objective
            .as_ref()
            .is_none_or(|s| !s.sites.iter().any(|r| r.site == site))
    );
    assert!(live.telemetry.withheld_route_entries > 0);
    assert_eq!(live.telemetry.held_for_site_refresh, 1);
}

#[test]
fn a_return_jump_depends_on_its_apex_even_when_both_endpoints_and_outward_walk_are_clear() {
    use ground_navigation::{GroundEdge, GroundEdgeKind, GroundNode, standing_height};

    let nodes: Vec<_> = [0, 6]
        .into_iter()
        .map(|id| {
            let normal = Vec2::Y.rotate_radians(id as f32 * std::f32::consts::TAU / 512.0);
            GroundNode {
                id,
                position: normal * 60.0,
                normal,
            }
        })
        .collect();
    let start = nodes[0].position;
    let end = nodes[1].position;
    let map = Arc::new(GroundMap {
        version: 1,
        actor: PlayerId::PLAYER_1,
        planet: 0,
        revision: 1,
        tick: 0,
        nodes,
        edges: vec![
            GroundEdge {
                from: 0,
                to: 6,
                kind: GroundEdgeKind::Walk,
                length: start.distance_to(end),
            },
            GroundEdge {
                from: 6,
                to: 0,
                kind: GroundEdgeKind::Jump,
                length: start.distance_to(end),
            },
        ],
        rejected: vec![],
    });
    let height = SurfaceSortieState::spec().half_height();
    let mut trip = Box::new(GroundRoundTripJob::new(
        map,
        start,
        end + end.normalized() * height,
        0.1,
        start + start.normalized() * height,
    ));
    while trip.next_work().is_some() {
        trip.step();
    }
    assert_eq!(trip.output().unwrap().outbound.path, [0, 6]);
    assert_eq!(
        trip.output().unwrap().returning.as_ref().unwrap().path,
        [6, 0]
    );
    let mut dependencies = RouteDependenciesJob::new(trip, 60.0, 1.0);
    while dependencies.next_work().is_some() {
        assert_eq!(dependencies.next_work(), Some(WorkKind::Graph));
        dependencies.step();
    }
    let areas = dependencies.take_areas();
    assert_eq!(areas.len(), 8);

    let (mut state, _, body) = fixture();
    let snapshot = state.world.physics.world.query_snapshot();
    let middle = (start + end) * 0.5;
    let apex = middle
        + middle.normalized()
            * (standing_height() + SurfaceSortieState::spec().jump_speed.powi(2) / 2.0 * 0.85);
    move_body(&mut state, body, apex);
    let frame = QueryFrame {
        previous_position: Vec2::ZERO,
        previous_angle: 0.0,
        current_position: Vec2::ZERO,
        current_angle: 0.0,
        excluded: &[],
    };
    let validate = |areas: &[QueryArea]| {
        snapshot
            .validate_areas(&state.world.physics.world, frame, areas)
            .valid
    };
    assert!(
        validate(&[areas[1], areas[5]]),
        "both endpoint capsules stay clear"
    );
    assert!(validate(&areas[..4]), "the outward walk stays clear");
    assert!(!validate(&areas), "the directed return jump is obstructed");
}

#[test]
fn buried_fragment_motion_allows_route_publication_but_route_obstruction_revokes_it() {
    for changed_gravity in [false, true] {
        let (mut state, mut o, body) = fixture();
        let flag = o
            .combat
            .recovery
            .flight
            .pilot
            .planet
            .claim
            .as_ref()
            .unwrap()
            .flag;
        let mut planner = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_route_dependencies();
        planner.observe(&state, 0, &mut o);
        let source = state.world.tick;
        let weak = Arc::downgrade(&planner.requests[&0].snapshot);
        planner.advance(state.world.tick);
        let job = planner.queue.job(planner.requests[&0].token).unwrap();
        assert!(!job.dependencies().is_empty());
        let (site, areas) = &job.dependencies()[0];
        let site = *site;
        let area = *areas
            .iter()
            .find(|a| a.groups == SurfaceSortieState::spec().collision_groups)
            .unwrap();
        let source_frame = o.combat.recovery.flight.pilot.planet.motion;
        if changed_gravity {
            let planet = o.combat.recovery.flight.pilot.planet.index;
            state.world.planets[planet].mass *= 2.0;
        }
        move_body(&mut state, body, source_frame.position + Vec2::X * 11.0);
        let mut o = target(&state, 0);
        o.combat
            .recovery
            .flight
            .pilot
            .planet
            .claim
            .as_mut()
            .unwrap()
            .flag = flag;
        let p = &o.combat.recovery.flight.pilot;
        assert_eq!(
            LiveObjectivePlanner::geometry_valid(
                &state,
                0,
                p,
                &planner.requests[&0],
                planner.requests[&0].gravity
            ),
            Err("obstacles_changed")
        );
        planner.observe(&state, 0, &mut o);
        let survey = o.landing_objective.as_ref().unwrap();
        assert!(survey.validated_routes_only);
        assert_eq!(survey.tick, source);
        assert_eq!(survey.validated_tick, Some(state.world.tick));
        assert!(survey.sites.iter().all(|r| r.cost().is_some()));
        assert!(survey.sites.iter().any(|r| r.site == site));
        assert!(planner.telemetry.route_unrelated_changes > 0);
        assert_eq!(
            planner.telemetry.gravity_independent_validations,
            u64::from(changed_gravity)
        );
        let point = (area.minimum + area.maximum) * 0.5;
        move_body(
            &mut state,
            body,
            source_frame.position + point.rotate_radians(source_frame.angle),
        );
        let mut o = target(&state, 0);
        o.combat
            .recovery
            .flight
            .pilot
            .planet
            .claim
            .as_mut()
            .unwrap()
            .flag = flag;
        planner.observe(&state, 0, &mut o);
        assert!(
            o.landing_objective
                .as_ref()
                .is_none_or(|s| !s.sites.iter().any(|r| r.site == site))
        );
        assert!(planner.telemetry.route_area_tests > 0);
        assert!(planner.telemetry.withheld_route_entries > 0);
        planner.reset();
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn changed_gravity_keeps_negative_work_pending_then_withholds_the_stale_answer() {
    let (mut state, mut o, body) = fixture();
    o.combat
        .recovery
        .flight
        .pilot
        .planet
        .claim
        .as_mut()
        .unwrap()
        .flag
        .as_mut()
        .unwrap()
        .position = o.combat.recovery.flight.pilot.planet.motion.position;
    let mut planner = LiveObjectivePlanner::new(
        1,
        Work {
            graph: 16384,
            physics_queries: 512,
        },
    )
    .with_route_dependencies();
    planner.observe(&state, 0, &mut o);
    let source = state.world.tick;
    let token = planner.requests[&0].token;
    let weak = Arc::downgrade(&planner.requests[&0].snapshot);
    planner.advance(state.world.tick);
    let planet = o.combat.recovery.flight.pilot.planet.index;
    let center = o.combat.recovery.flight.pilot.planet.motion.position;
    move_body(&mut state, body, center + Vec2::X * 11.0);
    state.world.planets[planet].mass *= 2.0;
    o.combat.recovery.flight.pilot.tick = state.world.tick;
    // At the center this planet's acceleration is zero. Move the source mass
    // to change gravity without altering the frozen query geometry.
    state.world.planets[planet].position += Vec2::X * 3.0;
    planner.observe(&state, 0, &mut o);
    assert_eq!(planner.queue.poll(token, &source), JobPoll::Pending);
    assert_eq!(planner.requests[&0].token, token);
    assert_eq!(planner.requests[&0].measurement_tick, source);
    assert!(Weak::ptr_eq(
        &weak,
        &Arc::downgrade(&planner.requests[&0].snapshot)
    ));
    assert!(o.landing_objective.is_none());
    planner.allowance = Work::UNLIMITED;
    planner.advance(state.world.tick);
    let result = planner
        .queue
        .job(planner.requests[&0].token)
        .unwrap()
        .output()
        .unwrap();
    assert!(result.sites.iter().all(|r| r.cost().is_none()));
    let measured = planner.telemetry.measurements_by_actor[&0].clone();
    assert_eq!(measured.finished_surveys, 1);
    assert_eq!(measured.successful_candidates, 0);
    assert_eq!(
        measured.finished_candidates,
        measured.failures.values().sum::<u64>()
    );
    assert!(measured.finished_candidates > 0);
    assert_eq!(planner.telemetry.completed, 0);
    planner.observe(&state, 0, &mut o);
    assert_eq!(planner.telemetry.measurements_by_actor[&0], measured);
    assert_eq!(planner.telemetry.completed, 0);
    assert!(
        o.landing_objective.is_none(),
        "stale failed paths must not be reported as impossibility"
    );
    assert_eq!(o.objective_work, Some(ObjectiveWorkState::Stale));
    assert_eq!(planner.telemetry.reused_requests, 0);
    assert_eq!(planner.telemetry.jump_gravity_mismatches, 1);
    assert!(weak.upgrade().is_none());
}

#[test]
fn deferred_candidate_refresh_keeps_a_bounded_cache_with_normal_lifetime_rules() {
    for outcome in ["resume", "expired", "goal", "loss", "missing", "reset"] {
        let (mut state, mut o, _) = fixture();
        let mut planner = LiveObjectivePlanner::new(
            1,
            Work {
                graph: 16384,
                physics_queries: 512,
            },
        )
        .with_route_dependencies();
        // A changed actual touchdown still retires and salvages the source.
        let p = &mut o.combat.recovery.flight.pilot;
        p.landing.phase = LandingPhase::Landed;
        p.hatch = Some(p.sites[0].hatch_position);
        planner.observe(&state, 0, &mut o);
        let source = state.world.tick;
        let token = planner.requests[&0].token;
        let weak = Arc::downgrade(&planner.requests[&0].snapshot);
        planner.advance(source);
        state.world.tick += 1;
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick = state.world.tick;
        p.landing.phase = LandingPhase::Flying;
        p.hatch = None;
        p.sites.clear();
        planner.observe(&state, 0, &mut o);
        assert!(planner.requests.is_empty());
        assert_eq!(planner.parked.len(), 1);
        assert!(weak.upgrade().is_some());
        assert!(o.landing_objective.is_none());
        let mut clone = planner.clone();
        let report = planner.advance(state.world.tick).unwrap();
        assert_eq!(report, clone.advance(state.world.tick).unwrap());
        assert_eq!(report.charged, Work::default());
        clone.reset();
        let mut other = target(&state, 1);
        planner.observe(&state, 1, &mut other);
        assert_eq!(planner.telemetry.deferred_capacity, 1);
        assert!(other.landing_objective.is_none());
        state.world.tick += 1;
        o.combat.recovery.flight.pilot.tick = state.world.tick;
        match outcome {
            "resume" => {
                let mut fresh = target(&state, 0);
                planner.observe(&state, 0, &mut fresh);
                assert!(planner.parked.is_empty());
                assert_eq!(planner.requests[&0].measurement_tick, source);
                assert_eq!(planner.telemetry.reused_requests, 1);
                planner.advance(state.world.tick);
                assert!(planner.telemetry.reused_ground.nodes > 0);
                planner.reset();
            }
            "expired" => {
                state.world.tick = source + MAX_SURVEY_AGE_TICKS + 1;
                o.combat.recovery.flight.pilot.tick = state.world.tick;
                planner.observe(&state, 0, &mut o);
                assert_eq!(planner.telemetry.invalidations.get("expired"), Some(&1));
            }
            "goal" => {
                o.combat
                    .recovery
                    .flight
                    .pilot
                    .planet
                    .claim
                    .as_mut()
                    .unwrap()
                    .flag
                    .as_mut()
                    .unwrap()
                    .position += Vec2::X * 4.0;
                planner.observe(&state, 0, &mut o);
                assert_eq!(
                    planner.telemetry.invalidations.get("objective_changed"),
                    Some(&1)
                );
            }
            "loss" => {
                o.combat.recovery.flight.pilot.ship_available = false;
                planner.observe(&state, 0, &mut o);
            }
            "missing" => {
                planner.advance(state.world.tick);
            }
            _ => planner.reset(),
        }
        assert_eq!(planner.queue.poll(token, &source), JobPoll::Stale);
        assert!(planner.parked.is_empty());
        assert!(weak.upgrade().is_none(), "{outcome}");
    }
}
