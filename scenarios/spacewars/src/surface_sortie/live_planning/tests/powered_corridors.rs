use super::*;

const WORK: Work = Work {
    graph: 4,
    physics_queries: 384,
};

fn planner() -> LiveObjectivePlanner {
    LiveObjectivePlanner::new(2, WORK)
        .with_route_dependencies()
        .with_early_candidates()
        .with_focused_candidates()
        .with_requested_corridors()
        .with_extended_corridors()
        .with_walk_feedback()
        .with_walk_bounds_feedback()
        .with_powered_corridors()
}

pub(super) fn fixture(
    moving: bool,
    seat: usize,
) -> (SurfaceSortieState, combat::TacticalSortieObservationV1) {
    let mut state = if moving {
        SurfaceSortieScenario::init_material_moving_crossing_trial(42, seat, 60.0, 0.015, 0.065)
    } else {
        SurfaceSortieScenario::init_material_jetpack(42, 2)
    };
    if !moving {
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
    }
    while !(state.world.tick + seat as u64 * 15).is_multiple_of(30) {
        SurfaceSortieScenario::step(&mut state, &[], DT);
    }
    let mut o = state.tactical_sortie_observation_for_live_planning(seat, LandingSiteQuery::Survey);
    let p = &mut o.combat.recovery.flight.pilot;
    p.queries_ready = true;
    let map = state
        .survey_ground_with_gravity(seat, p.planet.index, 0..512, false, 18.2)
        .unwrap();
    let center =
        (p.ship.position - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
    let crossing = state
        .forecast_vehicle_crossing(seat, p, &map, center, p.ship.angle - p.planet.motion.angle)
        .unwrap();
    let hatch =
        (p.hatch.unwrap() - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
    let far = [crossing.plan.start, crossing.plan.destination]
        .into_iter()
        .max_by(|a, b| a.distance_to(hatch).total_cmp(&b.distance_to(hatch)))
        .unwrap();
    let enemy = PlayerId::from_index(1 - seat).unwrap();
    p.sites.clear();
    p.planet.claim.as_mut().unwrap().owner = Some(enemy);
    p.planet.claim.as_mut().unwrap().flag = Some(PlanetFlagObservation {
        player: enemy,
        position: p.planet.motion.position + far.rotate_radians(p.planet.motion.angle),
        normal: far.normalized().rotate_radians(p.planet.motion.angle),
        raised_fraction: 1.0,
    });
    let legacy = state
        .landing_objective_survey(seat, p, &o.cover, ObjectivePlanning::JointRoundTrip)
        .unwrap();
    assert!(
        legacy.actual.unwrap().cost().is_none(),
        "fixture must require a flight"
    );
    (state, o)
}

fn job(
    state: &SurfaceSortieState,
    seat: usize,
    o: &combat::TacticalSortieObservationV1,
) -> ObjectiveSurveyJob {
    let p = &o.combat.recovery.flight.pilot;
    state
        .objective_job_with_planning(
            seat,
            p,
            &o.cover,
            Arc::new(state.world.physics.world.query_snapshot()),
            None,
            true,
            ObjectivePlanning::JetpackRoundTrip,
        )
        .unwrap()
        .with_focused_candidate(0)
        .with_extended_corridor(p.site_query)
        .with_powered_corridor(p.site_query)
}

#[test]
fn powered_local_routes_join_exact_endpoints_and_finish_for_two_actors_before_expiry() {
    for moving in [false, true] {
        let mut queue = PlanningQueue::new(2);
        let fixtures = [fixture(moving, 0), fixture(moving, 1)];
        let before = fixtures
            .each_ref()
            .map(|(s, _)| s.world.physics.world.snapshot_bytes().unwrap());
        let tokens = [0, 1].map(|seat| {
            queue
                .submit(
                    seat as u64,
                    (),
                    JobLimits::default(),
                    job(&fixtures[seat].0, seat, &fixtures[seat].1),
                )
                .unwrap()
        });
        let mut ages = [None; 2];
        for age in 0..=120 {
            let r = queue.advance(WORK);
            assert!(r.charged.graph <= 4 && r.charged.physics_queries <= 384);
            for seat in 0..2 {
                let job = queue.job(tokens[seat]).unwrap();
                if let Some(survey) = job.positive_candidates() {
                    assert!(job.actual_local_failure().is_none());
                    let route = survey.actual.unwrap();
                    let c = route
                        .crossing
                        .expect("walking alone cannot complete this fixture");
                    assert_eq!(survey.tick, fixtures[seat].0.world.tick);
                    assert_eq!(c.measured_tick, survey.tick);
                    assert!(c.valid_at(survey.tick + age));
                    assert_eq!(route.outbound.flights, 1);
                    assert_eq!(route.returning.as_ref().unwrap().flights, 1);
                    assert!(route.cost().is_some());
                    assert_eq!(job.flight_work().approved, 1);
                    ages[seat].get_or_insert(age);
                }
            }
            if ages.iter().all(Option::is_some) {
                break;
            }
        }
        for seat in 0..2 {
            let j = queue.job(tokens[seat]).unwrap();
            assert!(
                ages[seat].is_some(),
                "moving={moving} seat={seat} {:?} {:?}",
                j.measurement_work(),
                j.flight_work()
            );
            assert_eq!(
                fixtures[seat]
                    .0
                    .world
                    .physics
                    .world
                    .snapshot_bytes()
                    .unwrap(),
                before[seat]
            );
        }
        eprintln!("powered route moving={moving} first ages={ages:?}");
    }
}

#[test]
fn live_powered_actual_return_is_published_with_its_original_clock() {
    for moving in [false, true] {
        check_live_powered_return(moving);
    }
}

fn check_live_powered_return(moving: bool) {
    let (mut state, source) = fixture(moving, 0);
    let start = state.world.tick;
    let target = LandingObjective::read(&source.combat.recovery.flight.pilot).unwrap();
    let mut live = planner();
    let mut published_source = None;
    let mut source_clocks = BTreeMap::new();
    for age in 0..=120 {
        let mut o =
            state.tactical_sortie_observation_for_live_planning(0, LandingSiteQuery::Survey);
        let p = &mut o.combat.recovery.flight.pilot;
        p.sites.clear();
        p.planet.claim.as_mut().unwrap().owner = source
            .combat
            .recovery
            .flight
            .pilot
            .planet
            .claim
            .as_ref()
            .unwrap()
            .owner;
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
        p.planet
            .claim
            .as_mut()
            .unwrap()
            .flag
            .as_mut()
            .unwrap()
            .position =
            p.planet.motion.position + target.position.rotate_radians(p.planet.motion.angle);
        assert_eq!(state.world.tick, start + age);
        live.observe_with_planning(&state, 0, &mut o, ObjectivePlanning::JetpackRoundTrip);
        if let Some(r) = live.requests.get(&0) {
            assert_eq!(
                *source_clocks
                    .entry(r.token.generation)
                    .or_insert(r.measurement_tick),
                r.measurement_tick
            );
            assert_eq!(r.measurement_tick, r.tick);
        }
        if let Some(s) = o.landing_objective {
            let request = &live.requests[&0];
            assert_eq!(s.tick, request.measurement_tick);
            assert_eq!(
                o.objective_evidence.unwrap().generation,
                Some(request.token.generation)
            );
            assert_eq!(s.validated_tick, Some(state.world.tick));
            assert!(
                s.actual
                    .unwrap()
                    .crossing
                    .unwrap()
                    .valid_at(state.world.tick)
            );
            published_source = Some(s.tick);
            break;
        }
        let r = live.advance(state.world.tick).unwrap();
        assert!(r.charged.graph <= 4 && r.charged.physics_queries <= 384);
        SurfaceSortieScenario::step(&mut state, &[], DT);
    }
    let published_source = published_source.unwrap_or_else(|| panic!("{:?}", live.telemetry));
    // Real moving-hatch changes may require a new snapshot. Each generation
    // keeps its original clock; expiry is relative to the published request.
    state.world.tick = published_source + MAX_SURVEY_AGE_TICKS + 1;
    let mut expired = source.clone();
    expired.combat.recovery.flight.pilot.tick = state.world.tick;
    live.observe_with_planning(&state, 0, &mut expired, ObjectivePlanning::JetpackRoundTrip);
    assert!(expired.landing_objective.is_none());
    assert_eq!(
        expired.objective_evidence.unwrap().invalidated_by,
        Some("expired")
    );
}

#[test]
fn missing_boarding_support_never_becomes_a_powered_permission() {
    let (state, mut o) = fixture(false, 0);
    o.combat.recovery.flight.pilot.boarding_hatches = [None; 2];
    let mut j = job(&state, 0, &o);
    while j.measurement_work().powered_corridor_completed == 0 {
        j.step();
    }
    assert!(j.positive_candidates().is_none() && j.exhausted_walk().is_none());
    assert_eq!(j.actual_local_failure(), Some("hatch_to_crossing"));
    assert_eq!(j.flight_work().started, 0);
    assert_eq!(
        j.measurement_work()
            .powered_corridor_failures
            .get("hatch_to_crossing"),
        Some(&1)
    );
    let expected = state
        .landing_objective_survey(
            0,
            &o.combat.recovery.flight.pilot,
            &o.cover,
            ObjectivePlanning::JetpackRoundTrip,
        )
        .unwrap();
    while j.next_work().is_some() {
        j.step();
    }
    assert_eq!(j.output(), Some(&expected));
}

#[test]
fn powered_requests_take_a_fresh_epoch_instead_of_relabeling_warm_geometry() {
    let (mut state, mut source) = fixture(false, 0);
    let old = job(&state, 0, &source).into_measurements();
    let snapshot = Arc::clone(&old.snapshot);
    let objective = LandingObjective::read(&source.combat.recovery.flight.pilot).unwrap();
    let mut live = planner();
    live.parked.insert(
        0,
        Parked {
            objective,
            measurements: old,
            seen: state.world.tick,
        },
    );
    state.world.tick += 1;
    source.combat.recovery.flight.pilot.tick = state.world.tick;
    live.observe_with_planning(&state, 0, &mut source, ObjectivePlanning::JetpackRoundTrip);
    let request = &live.requests[&0];
    assert_eq!(request.measurement_tick, state.world.tick);
    assert!(!Arc::ptr_eq(&request.snapshot, &snapshot));
    assert_eq!(live.telemetry.reused_requests, 0);
    assert!(source.landing_objective.is_none());

    // A survey without an actual hatch uses the full fallback. Its geometry
    // must also originate at the flight environment's source epoch.
    live.reset();
    let old = job(&state, 0, &source).into_measurements();
    let snapshot = Arc::clone(&old.snapshot);
    live.parked.insert(
        0,
        Parked {
            objective,
            measurements: old,
            seen: state.world.tick,
        },
    );
    let mut survey = source.clone();
    let p = &mut survey.combat.recovery.flight.pilot;
    p.sites = state
        .tactical_sortie_observation_for_live_planning(0, LandingSiteQuery::Survey)
        .combat
        .recovery
        .flight
        .pilot
        .sites;
    assert!(!p.sites.is_empty());
    p.hatch = None;
    p.site_query = LandingSiteQuery::Survey;
    state.world.tick += 1;
    p.tick = state.world.tick;
    live.observe_with_planning(&state, 0, &mut survey, ObjectivePlanning::JetpackRoundTrip);
    let request = &live.requests[&0];
    assert_eq!(request.measurement_tick, state.world.tick);
    assert!(!Arc::ptr_eq(&request.snapshot, &snapshot));
    assert!(request.actual.is_none());
    assert_eq!(live.telemetry.reused_requests, 0);

    live.reset();
    assert!(live.uses_powered_corridors());
    state.pilots[0].jetpack_charge = None;
    let mut without_pack = source.clone();
    without_pack.combat.recovery.flight.pilot.tick = state.world.tick;
    live.observe_with_planning(
        &state,
        0,
        &mut without_pack,
        ObjectivePlanning::JetpackRoundTrip,
    );
    for age in 0..20 {
        let r = live.advance(state.world.tick + age).unwrap();
        assert!(r.charged.graph <= 4 && r.charged.physics_queries <= 384);
    }
    assert_eq!(live.telemetry.flight_forecasts.started, 0);
    assert_eq!(
        live.telemetry.measurements_by_actor[&0].powered_corridor_started,
        0
    );
}
