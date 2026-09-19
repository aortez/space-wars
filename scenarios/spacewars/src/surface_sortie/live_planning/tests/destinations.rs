use super::*;
use crate::surface_sortie::destination_cover::*;
use crate::surface_sortie::mission::MissionObservationV1;

fn fixture() -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    state
}
fn flying_observation(state: &SurfaceSortieState, player: usize) -> MissionObservationV1 {
    let mut o = state.mission_observation(
        player,
        Some(pilot::LandingSiteId {
            planet: 0,
            bearing: pilot::LANDING_SITE_COUNT,
        }),
    );
    // Only the sensing demand is synthetic; every measurement uses the real
    // world and remote planet. No physical action or landing is authorized.
    o.local.combat.recovery.flight.pilot.landing.supported_feet = 0;
    o.local.combat.recovery.flight.pilot.landing.phase = LandingPhase::Flying;
    o
}
fn request(state: &SurfaceSortieState, player: usize) -> DestinationCoverRequest {
    let ids: Vec<_> = (0..pilot::LANDING_SITE_COUNT)
        .filter_map(|bearing| {
            let id = pilot::LandingSiteId { planet: 1, bearing };
            state.vehicle_landing_site(player, id, false).map(|_| id)
        })
        .take(2)
        .collect();
    assert_eq!(ids.len(), 2);
    DestinationCoverRequest {
        generation: 7,
        candidates: [Some(ids[0]), Some(ids[1]), None, None],
    }
}

#[test]
fn both_actors_share_remaining_quota_read_only_and_repeated_calls_do_not_spend_again() {
    let state = fixture();
    let allowance = Work {
        graph: 16384,
        physics_queries: 384,
    };
    let mut planner = LiveObjectivePlanner::new(2, allowance);
    let before = state.world.physics.world.snapshot_bytes().unwrap();
    for player in [1, 0] {
        let mut o = flying_observation(&state, player);
        let request = request(&state, player);
        planner.observe_destination_cover(&state, player, &mut o, Some(request));
        planner.observe_destination_cover(&state, player, &mut o, Some(request));
        assert!(
            o.destination_cover
                .unwrap()
                .candidates
                .iter()
                .all(|c| c.status == CoverStatus::Pending)
        );
    }
    let mut clone = planner.clone();
    let report = planner.advance_with_state(&state).unwrap();
    assert_eq!(report, clone.advance_with_state(&state).unwrap());
    assert!(report.charged.physics_queries > 0 && report.charged.physics_queries <= 384);
    assert_eq!(
        report.charged.physics_queries,
        report
            .jobs
            .iter()
            .map(|j| j.charged.physics_queries)
            .sum::<u32>()
    );
    assert_eq!(report.jobs.len(), 2);
    assert_eq!(planner.destination_cover_telemetry().checks, 2);
    assert_eq!(
        planner.telemetry.physics_queries,
        u64::from(report.charged.physics_queries)
    );
    assert_eq!(
        planner.destination_cover_telemetry().physics_queries,
        planner.telemetry.physics_queries
    );
    assert_eq!(before, state.world.physics.world.snapshot_bytes().unwrap());
    let results = planner.destination_cover_observations(state.world.tick);
    assert_eq!(results.len(), 2);
    assert!(
        results
            .iter()
            .all(|(_, r)| r.candidates[0].status == CoverStatus::Measured)
    );
    assert!(results.iter().all(|(_, r)| {
        r.candidates[0]
            .measurement
            .as_ref()
            .unwrap()
            .site
            .unwrap()
            .id
            .planet
            == 1
    }));
    planner.remove(0);
    assert!(planner.advance_with_state(&state).is_none());
    assert_eq!(
        planner.telemetry.physics_queries,
        u64::from(report.charged.physics_queries)
    );
    planner.reset();
    assert!(
        planner
            .destination_cover_observations(state.world.tick)
            .is_empty()
    );
    assert_eq!(planner.destination_cover_telemetry().checks, 0);
}

#[test]
fn zero_small_and_exhausted_allowances_defer_without_negative_evidence() {
    let state = fixture();
    for budget in [0, 191, 383] {
        let mut planner = LiveObjectivePlanner::new(
            2,
            Work {
                graph: 0,
                physics_queries: budget,
            },
        );
        let mut o = flying_observation(&state, 0);
        planner.observe_destination_cover(&state, 0, &mut o, Some(request(&state, 0)));
        let report = planner.advance_with_state(&state).unwrap();
        assert_eq!(report.charged.physics_queries, 0);
        let results = planner.destination_cover_observations(state.world.tick);
        assert_eq!(results[0].1.candidates[0].status, CoverStatus::Deferred);
        assert_eq!(results[0].1.candidates[0].measurement, None);
    }
}

#[test]
fn earlier_samples_are_stale_after_real_motion_and_local_tasks_cancel_demand() {
    let mut state = fixture();
    let mut planner = LiveObjectivePlanner::new(1, Work::UNLIMITED);
    let req = request(&state, 0);
    let mut o = flying_observation(&state, 0);
    planner.observe_destination_cover(&state, 0, &mut o, Some(req));
    planner.advance_with_state(&state).unwrap();
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut o = flying_observation(&state, 0);
    planner.observe_destination_cover(&state, 0, &mut o, Some(req));
    let sample = &o.destination_cover.as_ref().unwrap().candidates[0];
    assert_eq!(sample.status, CoverStatus::Stale);
    assert_eq!(
        sample.measurement.as_ref().unwrap().tick,
        state.world.tick - 1
    );
    o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Survey;
    planner.observe_destination_cover(&state, 0, &mut o, Some(req));
    assert!(
        o.destination_cover
            .unwrap()
            .candidates
            .iter()
            .all(|c| c.status == CoverStatus::Deferred)
    );
    assert!(
        planner
            .destination_cover_observations(state.world.tick)
            .is_empty()
    );
    let report = planner.advance_with_state(&state).unwrap();
    assert_eq!(report.charged, Work::default());
}

#[test]
fn local_jobs_get_the_same_allocation_before_remote_checks() {
    let mut state = state();
    let allowance = Work {
        graph: 17,
        physics_queries: 384,
    };
    let mut control = LiveObjectivePlanner::new(2, allowance);
    let mut local = target(&state, 1);
    control.observe(&state, 1, &mut local);
    let mut probe = control.clone();
    let mut remote = flying_observation(&state, 0);
    let req = DestinationCoverRequest {
        generation: 7,
        candidates: [
            Some(pilot::LandingSiteId {
                planet: 0,
                bearing: 10,
            }),
            None,
            None,
            None,
        ],
    };
    probe.observe_destination_cover(&state, 0, &mut remote, Some(req));
    let a = control.advance_with_state(&state).unwrap();
    let b = probe.advance_with_state(&state).unwrap();
    for row in &a.jobs {
        assert_eq!(Some(row), b.jobs.iter().find(|b| b.request == row.request));
    }
    assert_eq!(a.charged, b.charged);
    assert_eq!(probe.destination_cover_telemetry().checks, 0);
    assert!(
        probe
            .destination_cover_telemetry()
            .deferred
            .contains_key("query budget")
    );
    // Unobserved remote work cannot survive into another physics tick.
    SurfaceSortieScenario::step(&mut state, &[], DT);
    probe.advance_with_state(&state).unwrap();
    assert!(
        probe
            .destination_cover_observations(state.world.tick)
            .is_empty()
    );
}

#[test]
fn exhaustion_discards_partial_site_and_cover_instead_of_reporting_no_landing() {
    use crate::surface_sortie::live_planning::{
        destinations::measure,
        query_budget::{QueryFuel, SITE_QUERY_CAP},
    };
    let state = fixture();
    let id = request(&state, 0).candidates[0].unwrap();
    let fuel = QueryFuel::default();
    for _ in 0..SITE_QUERY_CAP - 1 {
        assert!(fuel.charge());
    }
    let enemy = flying_observation(&state, 0).local.combat.target;
    let sample = measure(&state, 0, id, enemy, &fuel);
    assert_eq!(sample.finding, CoverFinding::Incomplete);
    assert_eq!(sample.queries, SITE_QUERY_CAP);
    assert_eq!(sample.site, None);
    assert_eq!(sample.cover, None);
}

#[test]
fn mining_the_remote_footing_revokes_old_evidence_and_remeasures_material() {
    use engine_terrain::{Brush, EditMode, TerrainEdit};
    let mut state = fixture();
    let mut planner = LiveObjectivePlanner::new(1, Work::UNLIMITED);
    let mut req = request(&state, 0);
    req.candidates[1] = None;
    let mut o = flying_observation(&state, 0);
    planner.observe_destination_cover(&state, 0, &mut o, Some(req));
    planner.advance_with_state(&state).unwrap();
    let sample = planner.destination_cover_observations(state.world.tick)[0]
        .1
        .candidates[0]
        .measurement
        .clone()
        .unwrap();
    let site = sample.site.unwrap();
    let cell = state.world.terrain.planets[&site.id.planet]
        .field
        .local_to_cell(site.local_position - site.local_position.normalized() * 0.1)
        .unwrap();
    state
        .world
        .queue_planet_edit(
            site.id.planet,
            TerrainEdit {
                brush: Brush::Circle {
                    center: cell,
                    radius: 10,
                },
                mode: EditMode::Remove,
            },
        )
        .unwrap();
    SpacewarsScenario::prepare_terrain(&mut state.world, &[]);
    let mut o = flying_observation(&state, 0);
    planner.observe_destination_cover(&state, 0, &mut o, Some(req));
    assert_eq!(
        o.destination_cover.unwrap().candidates[0].status,
        CoverStatus::Stale
    );
    SurfaceSortieScenario::step(&mut state, &[], DT);
    req.generation += 1;
    let mut o = flying_observation(&state, 0);
    planner.observe_destination_cover(&state, 0, &mut o, Some(req));
    planner.advance_with_state(&state).unwrap();
    let sample = planner.destination_cover_observations(state.world.tick)[0]
        .1
        .candidates[0]
        .measurement
        .clone()
        .unwrap();
    assert_eq!(sample.tick, state.world.tick);
    assert!(sample.revision > site.revision);
    assert!(
        sample
            .site
            .is_none_or(|s| s.local_position.distance_to(site.local_position) > 2.0)
    );
}

#[test]
fn moving_obstacle_blocks_a_new_measurement_without_reusing_the_old_landing() {
    use engine_rapier::world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    };
    let mut state = fixture();
    let mut planner = LiveObjectivePlanner::new(1, Work::UNLIMITED);
    let mut req = request(&state, 0);
    req.candidates[1] = None;
    let mut o = flying_observation(&state, 0);
    planner.observe_destination_cover(&state, 0, &mut o, Some(req));
    planner.advance_with_state(&state).unwrap();
    let sample = planner.destination_cover_observations(state.world.tick)[0]
        .1
        .candidates[0]
        .measurement
        .clone()
        .unwrap();
    let site = sample.site.unwrap();
    let entity = PhysicsId::new(terrain::FRAGMENT_ID_BASE + 991);
    let body = BodyId::new(entity, BodyRole::PRIMARY);
    assert!(state.world.physics.world.insert_body(
        body,
        BodySpec {
            kind: BodyKind::Fixed,
            position: site.vehicle_position,
            ..Default::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            8.0
        )]
    ));
    state.world.physics.world.step(DT.as_secs_f32());
    state.world.tick += 1;
    req.generation += 1;
    let mut o = flying_observation(&state, 0);
    planner.observe_destination_cover(&state, 0, &mut o, Some(req));
    planner.advance_with_state(&state).unwrap();
    let sample = planner.destination_cover_observations(state.world.tick)[0]
        .1
        .candidates[0]
        .measurement
        .clone()
        .unwrap();
    assert_eq!(sample.finding, CoverFinding::NoLanding);
    assert_eq!(sample.site, None);
    assert_eq!(sample.cover, None);
}

#[test]
fn a_fragment_between_enemy_and_planet_is_not_reported_as_material_cover() {
    use engine_rapier::world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    };
    let mut state = state();
    let bodies: Vec<_> = state.world.physics.world.motions().map(|r| r.id).collect();
    for id in bodies {
        state
            .world
            .physics
            .world
            .set_body_kind(id, BodyKind::Fixed, true);
    }
    let o = state.tactical_sortie_observation(0, None);
    let enemy = o.combat.target.unwrap();
    let site = o
        .combat
        .recovery
        .flight
        .pilot
        .sites
        .iter()
        .find(|site| {
            state
                .landing_cover_with_queries(site, Some(enemy), || true)
                .grounded
        })
        .unwrap();
    let delta = site.vehicle_position + site.normal * 7.0 - enemy.motion.position;
    let shooter = state.pilots[enemy.owner.index()].vehicle.0;
    let first = state
        .world
        .physics
        .cast_laser(
            shooter,
            enemy.motion.position,
            delta.normalized(),
            delta.length(),
        )
        .unwrap();
    assert_eq!(
        first.target,
        Some(MechanicalEntity::Body(crate::BodyId::Planet(
            site.id.planet
        )))
    );
    let position = enemy.motion.position.midpoint(first.point);
    let radius = enemy.motion.position.distance_to(first.point) * 0.1;
    assert!(radius > 0.01);
    let entity = PhysicsId::new(terrain::FRAGMENT_ID_BASE + 992);
    assert!(state.world.physics.world.insert_body(
        BodyId::new(entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position,
            ..Default::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            radius
        )]
    ));
    state.world.physics.world.step(DT.as_secs_f32());
    let hit = state
        .world
        .physics
        .cast_laser(
            shooter,
            enemy.motion.position,
            delta.normalized(),
            delta.length(),
        )
        .unwrap();
    assert!(matches!(
        hit.target,
        Some(MechanicalEntity::TerrainFragment(_))
    ));
    let fuel = crate::surface_sortie::live_planning::query_budget::QueryFuel::default();
    let cover = state.landing_cover_with_queries(site, Some(enemy), || fuel.charge());
    assert!(!cover.grounded);
    assert_eq!(fuel.used(), 3);
}
