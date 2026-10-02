use super::*;

const WORK: Work = Work {
    graph: 4,
    physics_queries: 384,
};

fn smooth_state() -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_capture_destination_trial(42, 0, false, 0.8);
    for _ in 0..179 {
        SurfaceSortieScenario::step(&mut state, &[], DT);
    }
    state
}

fn long_target(state: &SurfaceSortieState, seat: usize) -> combat::TacticalSortieObservationV1 {
    let source = target(state, seat);
    let p = &source.combat.recovery.flight.pilot;
    let mut ground = state
        .ground_survey_job(
            seat,
            p.planet.index,
            state.objective_gravity(p),
            Arc::new(state.world.physics.world.query_snapshot()),
        )
        .unwrap();
    while ground.next_work().is_some() {
        ground.step();
    }
    let map = ground.take_map();
    // Use a real selected site and retained footing 0.65 radians away. This
    // sensor fixture changes only the target; mission trials use real flags.
    for site in &p.sites {
        for direction in [-1.0, 1.0] {
            let local = (site.hatch_position - p.planet.motion.position)
                .rotate_radians(-p.planet.motion.angle);
            let aim = local.rotate_radians(0.65 * direction);
            let node = *map
                .nodes
                .iter()
                .min_by(|a, b| {
                    a.position
                        .distance_to(aim)
                        .total_cmp(&b.position.distance_to(aim))
                })
                .unwrap();
            let mut o = source.clone();
            let p = &mut o.combat.recovery.flight.pilot;
            p.landing.phase = LandingPhase::Flying;
            p.site_query = LandingSiteQuery::Selected(site.id);
            p.sites = vec![*site];
            p.planet
                .claim
                .as_mut()
                .unwrap()
                .flag
                .as_mut()
                .unwrap()
                .position = p.planet.motion.position
                + (node.position
                    + node.position.normalized() * SurfaceSortieState::spec().half_height())
                .rotate_radians(p.planet.motion.angle);
            let mut job = make_job(state, seat, &o);
            while job.measurement_work().corridor_started > 0
                && job.measurement_work().corridor_completed == 0
            {
                job.step();
            }
            if job.measurement_work().corridor_successes == 1 {
                return o;
            }
        }
    }
    panic!("no long walking fixture for seat {seat}");
}

fn make_job(
    state: &SurfaceSortieState,
    seat: usize,
    o: &combat::TacticalSortieObservationV1,
) -> ObjectiveSurveyJob {
    let p = &o.combat.recovery.flight.pilot;
    state
        .objective_job(
            seat,
            p,
            &o.cover,
            Arc::new(state.world.physics.world.query_snapshot()),
            None,
            true,
        )
        .unwrap()
        .with_focused_candidate(0)
        .with_requested_corridor(p.site_query)
}

fn planner(work: Work) -> LiveObjectivePlanner {
    LiveObjectivePlanner::new(2, work)
        .with_route_dependencies()
        .with_early_candidates()
        .with_focused_candidates()
        .with_requested_corridors()
}

#[test]
fn requested_long_routes_deliver_to_both_actors_before_expiry_with_no_extra_allowance() {
    for planning in [
        ObjectivePlanning::JointRoundTrip,
        ObjectivePlanning::JetpackRoundTrip,
    ] {
        let mut state = smooth_state();
        let source = [long_target(&state, 0), long_target(&state, 1)];
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let start = state.world.tick;
        let mut live = planner(WORK);
        let mut delivered = [None; 2];
        for age in 0..=MAX_SURVEY_AGE_TICKS {
            state.world.tick = start + age;
            for seat in 0..2 {
                let mut o = source[seat].clone();
                o.combat.recovery.flight.pilot.tick = state.world.tick;
                live.observe_with_planning(&state, seat, &mut o, planning);
                if let Some(survey) = o.landing_objective {
                    assert_eq!(survey.tick, start);
                    assert_eq!(survey.validated_tick, Some(start + age));
                    assert!(survey.validated_routes_only);
                    assert_eq!(survey.sites.len(), 1);
                    let r = &survey.sites[0];
                    assert_eq!(
                        r.site,
                        Some(source[seat].combat.recovery.flight.pilot.sites[0].id)
                    );
                    assert!(r.cost().is_some() && r.outbound.length > 25.0 && r.crossing.is_none());
                    delivered[seat].get_or_insert(age);
                }
            }
            let report = live.advance(state.world.tick).unwrap();
            assert!(
                report.charged.graph <= WORK.graph
                    && report.charged.physics_queries <= WORK.physics_queries
            );
            if delivered.iter().all(Option::is_some) {
                break;
            }
        }
        assert!(
            delivered.iter().all(Option::is_some),
            "{planning:?}: {delivered:?}"
        );
        assert_eq!(
            live.telemetry
                .measurements_by_actor
                .values()
                .map(|m| m.corridor_successes)
                .sum::<u64>(),
            2
        );
        assert_eq!(live.telemetry.completed, 0);
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
    }
}

#[test]
fn corridor_handoff_preserves_full_native_result_and_failed_corridors_publish_nothing() {
    let state = smooth_state();
    let source = long_target(&state, 0);
    for blocked in [false, true] {
        let mut o = source.clone();
        if blocked {
            o.combat.recovery.flight.pilot.sites[0].boarding_hatches = [None; 2];
        }
        let p = &o.combat.recovery.flight.pilot;
        let expected = state
            .landing_objective_survey(0, p, &o.cover, ObjectivePlanning::JointRoundTrip)
            .unwrap();
        let mut job = make_job(&state, 0, &o);
        while job.measurement_work().corridor_completed == 0 {
            job.step();
        }
        assert_eq!(job.positive_candidates().is_none(), blocked);
        assert_eq!(
            job.measurement_work().corridor_successes,
            u64::from(!blocked)
        );
        while job.next_work().is_some() {
            job.step();
        }
        assert_eq!(job.output(), Some(&expected));
    }
}

#[test]
fn warm_snapshot_keeps_its_original_age_and_corridor_queries_are_not_counted_as_reuse() {
    let state = smooth_state();
    let mut source = long_target(&state, 0);
    let p = &mut source.combat.recovery.flight.pilot;
    let snapshot = Arc::new(state.world.physics.world.query_snapshot());
    let mut ground = state
        .ground_survey_job(
            0,
            p.planet.index,
            state.objective_gravity(p),
            Arc::clone(&snapshot),
        )
        .unwrap();
    while ground.next_work().is_some() {
        ground.step();
    }
    let original = ground.measurement_tick();
    p.tick += 20;
    let mut job = state
        .objective_job(
            0,
            p,
            &source.cover,
            snapshot,
            Some(ground.into_measurements()),
            true,
        )
        .unwrap()
        .with_focused_candidate(0)
        .with_requested_corridor(p.site_query);
    while job.measurement_work().corridor_completed == 0 {
        job.step();
    }
    assert_eq!(job.positive_candidates().unwrap().tick, original);
    assert_eq!(job.reused(), ReusedGroundWork::default());
    while job.next_work().is_some() {
        job.step();
    }
    assert!(job.reused().nodes > 0 && job.reused().walks > 0);
}

#[test]
fn zero_work_expiry_clone_and_removal_do_not_renew_or_publish_a_corridor() {
    let mut state = smooth_state();
    let mut source = long_target(&state, 0);
    let mut live = planner(Work::default());
    let start = state.world.tick;
    live.observe(&state, 0, &mut source);
    assert_eq!(live.advance(start).unwrap().charged, Work::default());
    assert!(source.landing_objective.is_none());
    let mut clone = live.clone();
    clone.remove(0);
    assert!(clone.requests.is_empty() && !live.requests.is_empty());
    state.world.tick = start + MAX_SURVEY_AGE_TICKS + 1;
    source.combat.recovery.flight.pilot.tick = state.world.tick;
    live.observe(&state, 0, &mut source);
    assert_eq!(
        source.objective_evidence.unwrap().invalidated_by,
        Some("expired")
    );
    assert!(source.landing_objective.is_none());
    live.reset();
    assert!(live.requests.is_empty() && live.uses_requested_corridors());
}

#[test]
fn actual_touchdown_uses_its_own_long_return_and_keeps_the_original_measurement_clock() {
    let mut state = smooth_state();
    let mut source = long_target(&state, 0);
    let p = &mut source.combat.recovery.flight.pilot;
    let site = p.sites[0];
    p.landing.phase = LandingPhase::Landed;
    p.ship.position = site.vehicle_position;
    p.ship.angle = rotation_for_direction(site.normal);
    p.hatch = Some(site.hatch_position);
    p.boarding_hatches = site.boarding_hatches;
    p.sites.clear();
    let start = state.world.tick;
    let mut live = planner(WORK);
    for age in 0..=MAX_SURVEY_AGE_TICKS {
        state.world.tick = start + age;
        let mut o = source.clone();
        o.combat.recovery.flight.pilot.tick = state.world.tick;
        live.observe(&state, 0, &mut o);
        if let Some(survey) = o.landing_objective {
            assert_eq!(survey.tick, start);
            assert!(survey.sites.is_empty());
            let actual = survey.actual.unwrap();
            assert!(actual.site.is_none() && actual.cost().is_some());
            assert!(actual.returning.unwrap().length > 25.0);
            let mut changed = o.combat.recovery.flight.pilot.clone();
            changed.hatch.as_mut().unwrap().x += 0.02;
            let request = &live.requests[&0];
            assert_eq!(
                LiveObjectivePlanner::valid(&state, 0, &changed, request, request.objective, true),
                Err("hatch_moved")
            );
            return;
        }
        live.advance(state.world.tick);
    }
    panic!("actual return did not arrive");
}

#[test]
fn new_obstacle_or_invalid_walking_rise_withholds_a_previously_positive_corridor() {
    use engine_rapier::world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
        CollisionGroups, PhysicsId,
    };
    let mut state = smooth_state();
    let bodies: Vec<_> = state.world.physics.world.motions().map(|r| r.id).collect();
    for id in bodies {
        state
            .world
            .physics
            .world
            .set_body_kind(id, BodyKind::Fixed, true);
    }
    let entity = PhysicsId::new(terrain::FRAGMENT_ID_BASE + 778);
    let id = BodyId::new(entity, BodyRole::PRIMARY);
    let mut shape = ColliderSpec::ball(ColliderId::new(entity, ColliderRole::PRIMARY, 0), 0.3);
    shape.collision_groups = CollisionGroups::new(1 << 12, u32::MAX);
    assert!(state.world.physics.world.insert_body(
        id,
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::ZERO,
            ..Default::default()
        },
        &[shape]
    ));
    state.world.physics.world.step(DT.as_secs_f32());
    let source = long_target(&state, 0);
    let mut live = planner(WORK);
    let start = state.world.tick;
    let mut published = None;
    for age in 0..=MAX_SURVEY_AGE_TICKS {
        state.world.tick = start + age;
        let mut o = source.clone();
        o.combat.recovery.flight.pilot.tick = state.world.tick;
        live.observe(&state, 0, &mut o);
        if let Some(survey) = o.landing_objective {
            published = Some(survey);
            break;
        }
        live.advance(state.world.tick);
    }
    let survey = published.unwrap();
    let request = &live.requests[&0];
    let job = live.queue.job(request.token).unwrap();
    let p = &source.combat.recovery.flight.pilot;
    assert!(!job.corridor_rise_valid(survey.sites[0].site, || f32::MAX));
    let mass = state.world.planets[p.planet.index].mass;
    state.world.planets[p.planet.index].mass *= 1.0e12;
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
        .is_none()
    );
    state.world.planets[p.planet.index].mass = mass;
    let node = survey.sites[0].endpoint.unwrap();
    let center = node.position + node.position.normalized() * ground_navigation::standing_height();
    route_dependencies::move_body(
        &mut state,
        id,
        p.planet.motion.position + center.rotate_radians(p.planet.motion.angle),
    );
    assert!(
        LiveObjectivePlanner::locally_validated(
            &state,
            0,
            p,
            request,
            job,
            survey,
            &mut LivePlanningTelemetry::default()
        )
        .is_none()
    );
}
