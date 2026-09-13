use super::*;
use pilot::LandingSiteQuery;

const DT: Duration = Duration::from_nanos(16_666_667);
fn state() -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    for _ in 0..180 {
        SurfaceSortieScenario::step(&mut state, &[], DT);
    }
    state
}
fn target(state: &SurfaceSortieState, seat: usize) -> combat::TacticalSortieObservationV1 {
    let mut o = state.tactical_sortie_observation_for_live_planning(seat, LandingSiteQuery::Survey);
    let p = &mut o.combat.recovery.flight.pilot;
    let site = p.sites[0];
    let enemy = PlayerId::from_index(1 - seat).unwrap();
    // Sensor fixture supplies only a target; real terrain/collider poses are
    // untouched. End-to-end runners establish flags through actual controls.
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(enemy);
    claim.flag = Some(PlanetFlagObservation {
        player: enemy,
        position: site.hatch_position,
        normal: site.normal,
        raised_fraction: 1.0,
    });
    o
}

#[test]
fn complete_incremental_forecast_matches_synchronous_v10() {
    let mut state = state();
    for seat in 0..2 {
        if seat == 1 {
            for _ in 0..15 {
                SurfaceSortieScenario::step(&mut state, &[], DT);
            }
        }
        let o = target(&state, seat);
        let p = &o.combat.recovery.flight.pilot;
        let expected = state
            .landing_objective_survey(seat, p, &o.cover, ObjectivePlanning::JointRoundTrip)
            .unwrap();
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        for allowance in [
            Work {
                graph: 17,
                physics_queries: 11,
            },
            Work {
                graph: 1024,
                physics_queries: 1024,
            },
            Work::UNLIMITED,
        ] {
            let snapshot = Arc::new(state.world.physics.world.query_snapshot());
            let job = state.objective_job(seat, p, &o.cover, snapshot).unwrap();
            let mut queue = PlanningQueue::new(2);
            let token = queue.submit(33, (), JobLimits::default(), job).unwrap();
            for _ in 0..100_000 {
                let report = queue.advance(allowance);
                assert!(
                    report.charged.graph <= allowance.graph
                        && report.charged.physics_queries <= allowance.physics_queries
                );
                if matches!(queue.poll(token, &()), JobPoll::Ready(_)) {
                    break;
                }
            }
            assert_eq!(queue.poll(token, &()), JobPoll::Ready(&expected));
        }
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
    }
}

#[test]
fn shared_live_queue_has_one_tick_allowance_and_releases_reset_snapshots() {
    let state = state();
    let mut planner = LiveObjectivePlanner::new(
        2,
        Work {
            graph: 5,
            physics_queries: 3,
        },
    );
    for seat in 0..2 {
        let mut o = target(&state, seat);
        planner.observe(&state, seat, &mut o);
        assert_eq!(o.objective_work, Some(ObjectiveWorkState::Pending));
        assert!(o.landing_objective.is_none());
    }
    assert_eq!(planner.telemetry.snapshot_builds, 1);
    let weak = Arc::downgrade(&planner.requests[&0].snapshot);
    let mut clone = planner.clone();
    let report = planner.advance(state.world.tick).unwrap();
    assert_eq!(clone.advance(state.world.tick).unwrap(), report);
    assert_eq!(report.charged.physics_queries, 3);
    assert_eq!(
        report
            .jobs
            .iter()
            .map(|j| j.charged.physics_queries)
            .sum::<u32>(),
        3
    );
    assert!(report.jobs.iter().all(|j| j.charged.physics_queries > 0));
    assert!(planner.advance(state.world.tick).is_none());
    clone.reset();
    planner.reset();
    assert!(weak.upgrade().is_none());
    assert!(planner.requests.is_empty());
}

#[test]
fn both_surveys_make_progress_across_real_physics_steps() {
    let mut state = state();
    let allowance = Work {
        graph: 16384,
        physics_queries: 1024,
    };
    let mut planner = LiveObjectivePlanner::new(2, allowance);
    let mut ready = [false; 2];
    let mut shared_dispatches = 0;
    for _ in 0..120 {
        for (seat, seen) in ready.iter_mut().enumerate() {
            let mut o = target(&state, seat);
            planner.observe(&state, seat, &mut o);
            *seen |= o.landing_objective.is_some();
        }
        let report = planner.advance(state.world.tick).unwrap();
        assert!(report.charged.graph <= allowance.graph);
        assert!(report.charged.physics_queries <= allowance.physics_queries);
        if report
            .jobs
            .iter()
            .filter(|j| j.charged != Work::default())
            .count()
            == 2
        {
            shared_dispatches += 1;
        }
        SurfaceSortieScenario::step(&mut state, &[], DT);
        if ready == [true; 2] {
            break;
        }
    }
    assert_eq!(ready, [true; 2], "{:?}", planner.telemetry);
    assert!(shared_dispatches > 1);
    assert_eq!(planner.telemetry.max_retained_requests, 2);
}

#[test]
fn real_terrain_edits_revoke_pending_and_ready_forecasts() {
    for complete in [false, true] {
        let mut state = state();
        let mut planner = LiveObjectivePlanner::new(1, Work::UNLIMITED);
        let mut o = target(&state, 0);
        let planet = o.combat.recovery.flight.pilot.planet.index;
        planner.observe(&state, 0, &mut o);
        let token = planner.requests[&0].token;
        let tick = planner.requests[&0].tick;
        if complete {
            planner.advance(state.world.tick);
        }
        let field = &state.world.terrain.planets[&planet].field;
        let center = field
            .local_to_cell(Vec2::Y * state.world.planets[planet].radius)
            .unwrap();
        state
            .world
            .queue_planet_edit(
                planet,
                engine_terrain::TerrainEdit {
                    brush: engine_terrain::Brush::Circle { center, radius: 6 },
                    mode: engine_terrain::EditMode::Remove,
                },
            )
            .unwrap();
        SurfaceSortieScenario::step(&mut state, &[], DT);
        let mut o = target(&state, 0);
        planner.observe(&state, 0, &mut o);
        assert_eq!(o.objective_work, Some(ObjectiveWorkState::Stale));
        assert!(o.landing_objective.is_none());
        assert_eq!(planner.queue.poll(token, &tick), JobPoll::Stale);
        assert_eq!(
            planner.telemetry.invalidations.get("objective_changed"),
            Some(&1)
        );
    }
}

#[test]
fn moved_hull_and_changed_actual_hatch_revoke_ready_results() {
    let mut state = state();
    let mut planner = LiveObjectivePlanner::new(1, Work::UNLIMITED);
    let mut o = target(&state, 0);
    let p = &o.combat.recovery.flight.pilot;
    let near = p.planet.motion.position + Vec2::Y * (p.planet.radius + 5.0);
    planner.observe(&state, 0, &mut o);
    planner.advance(state.world.tick);
    // The normal step synchronizes model poses into physics before solving.
    state.world.ships[1].position = near;
    state.world.ships[1].velocity = Vec2::ZERO;
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut o = target(&state, 0);
    planner.observe(&state, 0, &mut o);
    assert!(o.landing_objective.is_none());
    assert_eq!(o.objective_work, Some(ObjectiveWorkState::Stale));
    assert!(
        planner
            .telemetry
            .invalidations
            .get("obstacles_changed")
            .is_some_and(|&n| n > 0)
    );

    let state = self::state();
    let mut planner = LiveObjectivePlanner::new(1, Work::UNLIMITED);
    let mut o = target(&state, 0);
    o.combat.recovery.flight.pilot.landing.phase = LandingPhase::Landed;
    o.combat.recovery.flight.pilot.hatch =
        Some(o.combat.recovery.flight.pilot.sites[0].hatch_position);
    planner.observe(&state, 0, &mut o);
    planner.advance(state.world.tick);
    let p = &mut o.combat.recovery.flight.pilot;
    assert!(p.hatch.is_some(), "fixture needs a grounded hatch");
    p.hatch = p.hatch.map(|h| h + Vec2::X);
    planner.observe(&state, 0, &mut o);
    assert!(o.landing_objective.is_none());
    assert_eq!(planner.telemetry.invalidations.get("hatch_moved"), Some(&1));
}

#[test]
fn publication_preserves_measurement_tick_and_pending_is_not_a_failed_route() {
    let mut state = state();
    let mut planner = LiveObjectivePlanner::new(1, Work::default());
    let mut o = target(&state, 0);
    planner.observe(&state, 0, &mut o);
    let report = planner.advance(state.world.tick).unwrap();
    assert_eq!(report.charged, Work::default());
    assert!(o.landing_objective.is_none());
    assert_eq!(o.objective_work, Some(ObjectiveWorkState::Pending));
    planner.reset();
    planner.allowance = Work::UNLIMITED;
    planner.observe(&state, 0, &mut o);
    let measured = state.world.tick;
    planner.advance(measured);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut o = target(&state, 0);
    planner.observe(&state, 0, &mut o);
    assert_eq!(
        o.objective_work,
        Some(ObjectiveWorkState::Ready),
        "{:?}",
        planner.telemetry()
    );
    let survey = o.landing_objective.as_ref().unwrap();
    assert_eq!(survey.tick, measured);
    assert_eq!(survey.validated_tick, Some(state.world.tick));
    assert!(survey.is_current(state.world.tick));
    assert!(!survey.is_current(state.world.tick + 1));
    o.combat.recovery.flight.pilot.ship_available = false;
    planner.observe(&state, 0, &mut o);
    assert!(planner.requests.is_empty());
    assert!(o.landing_objective.is_none());
}

#[test]
fn clone_resume_death_goal_change_and_missing_actor_release_work() {
    let mut state = state();
    let mut planner = LiveObjectivePlanner::new(
        2,
        Work {
            graph: 1024,
            physics_queries: 4096,
        },
    );
    for seat in 0..2 {
        let mut o = target(&state, seat);
        planner.observe(&state, seat, &mut o);
    }
    planner.advance(state.world.tick);
    let mut copy = planner.clone();
    let mut cloned_world = state.clone();
    for _ in 0..20 {
        SurfaceSortieScenario::step(&mut state, &[], DT);
        SurfaceSortieScenario::step(&mut cloned_world, &[], DT);
        for seat in 0..2 {
            let mut a = target(&state, seat);
            let mut b = target(&cloned_world, seat);
            planner.observe(&state, seat, &mut a);
            copy.observe(&cloned_world, seat, &mut b);
            assert_eq!(a, b);
        }
        assert_eq!(
            planner.advance(state.world.tick),
            copy.advance(cloned_world.world.tick)
        );
    }
    copy.reset();
    let mut o = target(&state, 0);
    let old = planner.requests[&0].token;
    let tick = planner.requests[&0].tick;
    o.combat
        .recovery
        .flight
        .pilot
        .planet
        .claim
        .as_mut()
        .unwrap()
        .owner = Some(PlayerId::PLAYER_1);
    planner.observe(&state, 0, &mut o);
    assert_eq!(planner.queue.poll(old, &tick), JobPoll::Stale);
    assert!(o.landing_objective.is_none());
    // Once the actor ceases to be observed, the next dispatch cancels its job.
    let weak = Arc::downgrade(&planner.requests[&1].snapshot);
    planner.advance(state.world.tick + 1);
    assert!(weak.upgrade().is_none());
    assert!(planner.requests.is_empty());

    planner.reset();
    let mut o = target(&state, 0);
    planner.observe(&state, 0, &mut o);
    let weak = Arc::downgrade(&planner.requests[&0].snapshot);
    state.world.ships[0].dead = true;
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut o = state.tactical_sortie_observation_for_live_planning(0, LandingSiteQuery::Survey);
    planner.observe(&state, 0, &mut o);
    assert!(o.landing_objective.is_none());
    assert!(!planner.requests.contains_key(&0));
    assert!(weak.upgrade().is_none());
}

#[test]
fn capacity_and_expiry_defer_without_reporting_an_impossible_route() {
    let state = state();
    let mut o = target(&state, 0);
    let mut planner = LiveObjectivePlanner::new(0, Work::UNLIMITED);
    planner.observe(&state, 0, &mut o);
    assert_eq!(o.objective_work, Some(ObjectiveWorkState::Pending));
    assert_eq!(planner.telemetry.deferred_capacity, 1);
    assert!(planner.requests.is_empty());

    let mut planner = LiveObjectivePlanner::new(1, Work::default());
    planner.observe(&state, 0, &mut o);
    let old = planner.requests[&0].clone();
    let mut p = o.combat.recovery.flight.pilot.clone();
    p.tick = old.tick + MAX_SURVEY_AGE_TICKS + 1;
    assert_eq!(
        LiveObjectivePlanner::valid(&state, 0, &p, &old, old.objective),
        Err("expired")
    );
    // Reset scopes the new episode even when it starts at the same physics tick.
    planner.reset();
    planner.observe(&state, 0, &mut o);
    assert_eq!(planner.queue.poll(old.token, &old.tick), JobPoll::Stale);
    assert_ne!(planner.requests[&0].token, old.token);
}
