use super::*;
use pilot::LandingSiteQuery;

mod flight_dependencies;
mod route_dependencies;

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
        for (allowance, local) in [
            (
                Work {
                    graph: 17,
                    physics_queries: 11,
                },
                true,
            ),
            (Work::UNLIMITED, true),
        ]
        .into_iter()
        .chain(
            [
                Work {
                    graph: 17,
                    physics_queries: 11,
                },
                Work {
                    graph: 1024,
                    physics_queries: 1024,
                },
                Work::UNLIMITED,
            ]
            .into_iter()
            .map(|work| (work, false)),
        ) {
            let snapshot = Arc::new(state.world.physics.world.query_snapshot());
            let job = state
                .objective_job(seat, p, &o.cover, snapshot, None, local)
                .unwrap();
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
        let mut planner = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_ground_reuse();
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
        assert_eq!(planner.telemetry.reused_requests, 0);
        assert_eq!(
            planner.telemetry.invalidations.get("objective_changed"),
            Some(&1)
        );
    }
}

#[test]
fn gravity_and_own_hatch_changes_salvage_partial_or_complete_ground() {
    for complete in [false, true] {
        let mut state = state();
        let mut planner = LiveObjectivePlanner::new(
            1,
            if complete {
                Work::UNLIMITED
            } else {
                Work {
                    graph: 16384,
                    physics_queries: 512,
                }
            },
        )
        .with_ground_reuse();
        let mut o = target(&state, 0);
        planner.observe(&state, 0, &mut o);
        let source_tick = state.world.tick;
        let old_token = planner.requests[&0].token;
        let weak = Arc::downgrade(&planner.requests[&0].snapshot);
        planner.advance(state.world.tick);
        // Dependency fixture advances the observation clock with frozen geometry,
        // changing only gravity. Physical moving-world runs cover normal stepping.
        state.world.tick += REFRESH_TICKS;
        let planet = o.combat.recovery.flight.pilot.planet.index;
        state.world.planets[planet].mass *= 2.0;
        let mut o = target(&state, 0);
        planner.observe(&state, 0, &mut o);
        assert_eq!(o.objective_work, Some(ObjectiveWorkState::Stale));
        assert!(o.landing_objective.is_none());
        assert_eq!(
            planner.telemetry.invalidations.get("gravity_changed"),
            Some(&1)
        );
        assert_eq!(planner.telemetry.reused_requests, 1);
        assert_eq!(planner.requests[&0].measurement_tick, source_tick);
        assert!(Weak::ptr_eq(
            &weak,
            &Arc::downgrade(&planner.requests[&0].snapshot)
        ));
        assert_eq!(planner.queue.poll(old_token, &source_tick), JobPoll::Stale);
        planner.allowance = Work::UNLIMITED;
        planner.advance(state.world.tick);
        planner.observe(&state, 0, &mut o);
        let mut expected = state
            .landing_objective_survey(
                0,
                &o.combat.recovery.flight.pilot,
                &o.cover,
                ObjectivePlanning::JointRoundTrip,
            )
            .unwrap();
        expected.tick = source_tick;
        expected.validated_tick = Some(state.world.tick);
        assert_eq!(o.landing_objective, Some(expected));
        assert!(planner.telemetry.reused_ground.nodes > 0);
        if complete {
            assert!(planner.telemetry.reused_ground.walks > 0);
        }

        for moved in [false, true] {
            state.world.tick += REFRESH_TICKS;
            let p = &mut o.combat.recovery.flight.pilot;
            p.tick = state.world.tick;
            p.landing.phase = LandingPhase::Landed;
            p.hatch =
                Some(p.sites[0].hatch_position + if moved { Vec2::X * 0.2 } else { Vec2::ZERO });
            planner.observe(&state, 0, &mut o);
            assert!(o.landing_objective.is_none());
            assert_eq!(planner.requests[&0].measurement_tick, source_tick);
            planner.advance(state.world.tick);
            planner.observe(&state, 0, &mut o);
            let mut expected = state
                .landing_objective_survey(
                    0,
                    &o.combat.recovery.flight.pilot,
                    &o.cover,
                    ObjectivePlanning::JointRoundTrip,
                )
                .unwrap();
            expected.tick = source_tick;
            expected.validated_tick = Some(state.world.tick);
            assert_eq!(o.landing_objective, Some(expected));
        }
        assert_eq!(planner.telemetry.reused_requests, 3);
        assert_eq!(planner.telemetry.snapshot_builds, 1);
        planner.reset();
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn reuse_cannot_hide_geometry_changes_behind_gravity_invalidation() {
    let mut state = state();
    let mut planner = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_ground_reuse();
    let mut o = target(&state, 0);
    let p = &o.combat.recovery.flight.pilot;
    let planet = p.planet.index;
    let near = p.planet.motion.position + Vec2::Y * (p.planet.radius + 5.0);
    planner.observe(&state, 0, &mut o);
    let weak = Arc::downgrade(&planner.requests[&0].snapshot);
    planner.advance(state.world.tick);
    state.world.ships[1].position = near;
    state.world.ships[1].velocity = Vec2::ZERO;
    state.world.planets[planet].mass *= 2.0;
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut o = target(&state, 0);
    planner.observe(&state, 0, &mut o);
    assert!(o.landing_objective.is_none());
    assert_eq!(
        planner.telemetry.invalidations.get("gravity_changed"),
        Some(&1)
    );
    assert_eq!(
        planner.telemetry.reuse_rejections.get("obstacles_changed"),
        Some(&1)
    );
    assert_eq!(planner.telemetry.reused_requests, 0);
    assert_eq!(planner.telemetry.snapshot_builds, 2);
    assert!(weak.upgrade().is_none());
}

#[test]
fn repeated_reuse_keeps_original_age_and_clone_reset_releases_it() {
    let mut state = state();
    let mut planner = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_ground_reuse();
    let mut o = target(&state, 0);
    planner.observe(&state, 0, &mut o);
    let source_tick = state.world.tick;
    let weak = Arc::downgrade(&planner.requests[&0].snapshot);
    planner.advance(state.world.tick);
    planner.observe(&state, 0, &mut o);
    for age in [30, 60, 90, 120] {
        // Keep physics frozen to isolate refresh/expiry from moving obstacles.
        state.world.tick = source_tick + age;
        let mut o = target(&state, 0);
        planner.observe(&state, 0, &mut o);
        assert_eq!(planner.requests[&0].measurement_tick, source_tick);
        assert_eq!(planner.requests[&0].tick, state.world.tick);
        let mut clone = planner.clone();
        assert_eq!(
            clone.advance(state.world.tick),
            planner.advance(state.world.tick)
        );
        let mut copy = o.clone();
        clone.observe(&state, 0, &mut copy);
        planner.observe(&state, 0, &mut o);
        assert_eq!(copy, o);
        let survey = o.landing_objective.unwrap();
        assert_eq!(survey.tick, source_tick);
        assert_eq!(survey.validated_tick, Some(state.world.tick));
        clone.reset();
    }
    assert_eq!(planner.telemetry.snapshot_builds, 1);
    assert_eq!(planner.telemetry.reused_requests, 4);
    assert_eq!(planner.telemetry.max_ready_age_ticks, 120);
    state.world.tick += 1;
    let mut o = target(&state, 0);
    planner.observe(&state, 0, &mut o);
    assert!(o.landing_objective.is_none());
    assert_eq!(planner.telemetry.invalidations.get("expired"), Some(&1));
    assert_eq!(planner.requests[&0].measurement_tick, state.world.tick);
    assert_eq!(planner.telemetry.snapshot_builds, 2);
    assert!(weak.upgrade().is_none());
    let new = Arc::downgrade(&planner.requests[&0].snapshot);
    planner.remove(0);
    assert!(new.upgrade().is_none());
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
fn changing_either_boarding_entrance_invalidates_a_landed_request() {
    let state = state();
    for side in 0..2 {
        for removed in [false, true] {
            let mut planner =
                LiveObjectivePlanner::new(1, Work::UNLIMITED).with_route_dependencies();
            let mut o = target(&state, 0);
            let p = &mut o.combat.recovery.flight.pilot;
            let site = p.sites[0];
            p.landing.phase = LandingPhase::Landed;
            p.hatch = Some(site.hatch_position);
            p.boarding_hatches = [
                Some(site.hatch_position),
                Some(site.hatch_position + Vec2::X * 12.0),
            ];
            planner.observe(&state, 0, &mut o);
            planner.advance(state.world.tick);
            let hatch = &mut o.combat.recovery.flight.pilot.boarding_hatches[side];
            *hatch = if removed {
                None
            } else {
                hatch.map(|h| h + Vec2::X)
            };
            planner.observe(&state, 0, &mut o);
            assert!(o.landing_objective.is_none());
            assert_eq!(planner.telemetry.invalidations.get("hatch_moved"), Some(&1));
        }
    }
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
        LiveObjectivePlanner::valid(&state, 0, &p, &old, old.objective, false),
        Err("expired")
    );
    // Reset scopes the new episode even when it starts at the same physics tick.
    planner.reset();
    planner.observe(&state, 0, &mut o);
    assert_eq!(planner.queue.poll(old.token, &old.tick), JobPoll::Stale);
    assert_ne!(planner.requests[&0].token, old.token);
}

#[test]
fn powered_objective_requires_flight_evidence_and_respects_the_same_dispatch_quota() {
    use jetpack::forecast::FlightEnvironment;
    let mut state = SurfaceSortieScenario::init_material_jetpack(42, 2);
    for _ in 0..120 {
        SurfaceSortieScenario::step(&mut state, &[], DT);
    }
    let mut o = state.tactical_sortie_observation_for_live_planning(0, LandingSiteQuery::Survey);
    let p = &mut o.combat.recovery.flight.pilot;
    let map = state
        .survey_ground_with_gravity(
            0,
            p.planet.index,
            0..ground_navigation::GROUND_SAMPLES as u16,
            false,
            18.2,
        )
        .unwrap();
    let center =
        (p.ship.position - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
    let crossing = state
        .forecast_vehicle_crossing(0, p, &map, center, p.ship.angle - p.planet.motion.angle)
        .unwrap();
    p.sites.clear(); // Isolate this actual/proposed pose, not a different landing.
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: PlayerId::PLAYER_2,
        position: p.planet.motion.position
            + crossing
                .plan
                .destination
                .rotate_radians(p.planet.motion.angle),
        normal: crossing
            .plan
            .destination
            .normalized()
            .rotate_radians(p.planet.motion.angle),
        raised_fraction: 1.0,
    });
    let p = &o.combat.recovery.flight.pilot;
    let legacy = state
        .landing_objective_survey(0, p, &o.cover, ObjectivePlanning::JointRoundTrip)
        .unwrap();
    assert!(legacy.actual.as_ref().unwrap().cost().is_none());
    let before = state.world.physics.world.snapshot_bytes().unwrap();
    let expected = state
        .landing_objective_survey(0, p, &o.cover, ObjectivePlanning::JetpackRoundTrip)
        .unwrap();
    let route = expected.actual.as_ref().unwrap();
    assert!(route.cost().is_some(), "{route:?}");
    assert_eq!(route.outbound.flights, 1);
    assert_eq!(
        route.returning.as_ref().unwrap().flights,
        0,
        "opposite entrance is reachable without another flight"
    );
    assert!(route.crossing.is_some());
    for allowance in [
        Work {
            graph: 17,
            physics_queries: 11,
        },
        Work::UNLIMITED,
    ] {
        let job = state
            .objective_job_with_planning(
                0,
                p,
                &o.cover,
                Arc::new(state.world.physics.world.query_snapshot()),
                None,
                true,
                ObjectivePlanning::JetpackRoundTrip,
            )
            .unwrap();
        let mut queue = PlanningQueue::new(1);
        let token = queue.submit(1, (), JobLimits::default(), job).unwrap();
        for _ in 0..100_000 {
            let r = queue.advance(allowance);
            assert!(
                r.charged.graph <= allowance.graph
                    && r.charged.physics_queries <= allowance.physics_queries
            );
            if matches!(queue.poll(token, &()), JobPoll::Ready(_)) {
                break;
            }
        }
        assert_eq!(queue.poll(token, &()), JobPoll::Ready(&expected));
        let job = queue.job(token).unwrap();
        assert!(
            job.dependencies()
                .iter()
                .any(|(site, areas)| site.is_none() && areas.len() > 4)
        );
    }
    assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
    let mut live = LiveObjectivePlanner::new(
        1,
        Work {
            graph: 0,
            physics_queries: 0,
        },
    );
    let mut observation = o.clone();
    live.observe_with_planning(
        &state,
        0,
        &mut observation,
        ObjectivePlanning::JetpackRoundTrip,
    );
    let mut request = live.requests[&0].clone();
    let mut changed_motion = p.clone();
    changed_motion.planet.motion.spin += 0.001;
    assert!(
        LiveObjectivePlanner::valid(
            &state,
            0,
            &changed_motion,
            &request,
            request.objective,
            false
        )
        .is_ok(),
        "walk-only work does not depend on a not-yet-used flight model"
    );
    request.flight_dependent = true;
    assert_eq!(
        LiveObjectivePlanner::valid(
            &state,
            0,
            &changed_motion,
            &request,
            request.objective,
            false
        ),
        Err("flight_environment_changed")
    );
    let field = FlightEnvironment::read(&state, p).unwrap();
    let mut moved = p.clone();
    moved.planet.motion.spin += 0.001;
    assert!(!field.compatible(&FlightEnvironment::read(&state, &moved).unwrap()));
    state.pilots[0].jetpack_charge = None;
    assert_eq!(
        LiveObjectivePlanner::valid(&state, 0, p, &request, request.objective, false),
        Err("jetpack_changed")
    );
    let unavailable = state
        .landing_objective_survey(0, p, &o.cover, ObjectivePlanning::JetpackRoundTrip)
        .unwrap();
    assert!(unavailable.actual.as_ref().unwrap().cost().is_none());
    state.pilots[0].jetpack_charge = Some(1.0);
    state.world.planets[0].mass *= 1.5;
    let heavy = state
        .landing_objective_survey(0, p, &o.cover, ObjectivePlanning::JetpackRoundTrip)
        .unwrap();
    assert!(
        heavy.actual.as_ref().unwrap().cost().is_none(),
        "a clear hull crossing cannot waive the fuel limit"
    );
}
