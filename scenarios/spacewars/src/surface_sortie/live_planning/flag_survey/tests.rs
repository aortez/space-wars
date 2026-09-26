use super::*;
use engine_rapier::world::{
    BodyId as RapierBodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
    PhysicsId,
};

fn fixture(player: usize) -> (SurfaceSortieState, FlagSurveyRequest) {
    let state = SurfaceSortieScenario::init_capture_destination_trial(42, player, false, 0.8);
    let mut p = observation(&state, player)
        .local
        .combat
        .recovery
        .flight
        .pilot;
    let o = observation(&state, player);
    p.planet = o.planets[1 - player].clone();
    let objective = LandingObjective::read(&p).unwrap();
    let b = (((-objective.position.x)
        .atan2(objective.position.y)
        .rem_euclid(std::f32::consts::TAU)
        * 64.0
        / std::f32::consts::TAU)
        .round() as u8)
        % 64;
    (
        state,
        FlagSurveyRequest {
            generation: p.tick,
            objective,
            candidates: [1, 63].map(|offset| LandingSiteId {
                planet: 1 - player,
                bearing: (b + offset) % 64,
            }),
        },
    )
}
fn observation(state: &SurfaceSortieState, player: usize) -> mission::MissionObservationV1 {
    let mut o = state.mission_observation(
        player,
        Some(LandingSiteId {
            planet: player,
            bearing: 64,
        }),
    );
    let p = &mut o.local.combat.recovery.flight.pilot;
    // Frozen geometry test: only the demand is airborne. No landing or control
    // assertion follows from this fixture; physical matched runs cover those.
    p.landing.supported_feet = 0;
    p.landing.phase = LandingPhase::Flying;
    o
}
fn tick(
    state: &mut SurfaceSortieState,
    planner: &mut FlagSurveyPlanner,
    player: usize,
    request: FlagSurveyRequest,
    work: Work,
) -> PlanningReport {
    let o = observation(state, player);
    planner.observe(state, player, &o, Some(request));
    let report = planner.advance(state, work, &[]).unwrap();
    assert_eq!(
        report.charged.graph,
        report.jobs.iter().map(|j| j.charged.graph).sum::<u32>()
    );
    assert_eq!(
        report.charged.physics_queries,
        report
            .jobs
            .iter()
            .map(|j| j.charged.physics_queries)
            .sum::<u32>()
    );
    assert!(
        report.charged.graph <= work.graph
            && report.charged.physics_queries <= work.physics_queries
    );
    assert!(planner.advance(state, work, &[]).is_none());
    // Dispatch advances while the physical snapshot remains frozen.
    state.world.tick += 1;
    report
}
const WORK: Work = Work {
    graph: 4,
    physics_queries: 384,
};

#[test]
fn simultaneous_actors_share_atomic_and_incremental_work_without_exceeding_quota() {
    let (mut state, a) = fixture(0);
    let (other, b) = fixture(1);
    // Compose the two controlled flag setups on the same initial geometry.
    state.claims[0] = other.claims[0].clone();
    state.world.planets[0].owner_id = Some(0);
    let mut planner = FlagSurveyPlanner::new(2);
    for _ in 0..MAX_FLAG_SURVEY_AGE {
        for player in [1, 0] {
            planner.observe(
                &state,
                player,
                &observation(&state, player),
                Some([a, b][player]),
            );
        }
        let mut copy = planner.clone();
        let report = planner.advance(&state, WORK, &[]).unwrap();
        assert_eq!(copy.advance(&state, WORK, &[]), Some(report.clone()));
        assert!(report.charged.graph <= 2 && report.charged.physics_queries <= 384);
        assert_eq!(
            report.charged.physics_queries,
            report
                .jobs
                .iter()
                .map(|j| j.charged.physics_queries)
                .sum::<u32>()
        );
        assert!(report.jobs.iter().all(|j| j.charged.graph <= 1));
        state.world.tick += 1;
        if planner.telemetry().walking_round_trips >= 2 {
            break;
        }
    }
    assert_eq!(planner.telemetry().walking_round_trips, 2);
    assert_eq!(
        planner
            .samples()
            .iter()
            .map(|s| s.actor)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        2
    );
}

#[test]
fn pending_source_continues_after_local_sensors_but_a_new_site_waits() {
    let (mut state, request) = fixture(0);
    let mut planner = FlagSurveyPlanner::new(2);
    tick(&mut state, &mut planner, 0, request, WORK);
    let original = planner.actors[&0].pending.as_ref().unwrap().clone();
    for _ in 0..MAX_FLAG_SURVEY_AGE {
        let mut o = observation(&state, 0);
        o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Survey;
        planner.observe(&state, 0, &o, Some(request));
        if let Some(pending) = &planner.actors[&0].pending {
            assert_eq!(pending.token, original.token);
            assert_eq!(pending.sample.source_tick, original.sample.source_tick);
        }
        planner.advance(
            &state,
            Work {
                graph: 1,
                physics_queries: 192,
            },
            &[0],
        );
        state.world.tick += 1;
        if !planner.samples().is_empty() {
            break;
        }
    }
    assert_eq!(planner.samples()[0].reason, None);
    let mut o = observation(&state, 0);
    o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Survey;
    planner.observe(&state, 0, &o, Some(request));
    assert_eq!(
        planner.advance(&state, WORK, &[]).unwrap().charged,
        Work::default()
    );
    assert_eq!(planner.telemetry().started, 1);
}

#[test]
fn tiny_rotation_of_a_long_collider_revokes_publication() {
    let (mut state, request) = fixture(0);
    let bodies: Vec<_> = state.world.physics.world.motions().map(|m| m.id).collect();
    for body in bodies {
        state
            .world
            .physics
            .world
            .set_body_kind(body, BodyKind::Fixed, true);
    }
    let p = observation(&state, 0).planets[1].clone();
    let entity = PhysicsId::new(987654322);
    let body = RapierBodyId::new(entity, BodyRole::PRIMARY);
    let origin = p.motion.position + Vec2::new(70.0, 10000.0);
    assert!(state.world.physics.world.insert_body(
        body,
        BodySpec {
            kind: BodyKind::Fixed,
            position: origin,
            ..Default::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            0.1,
            10000.0
        )]
    ));
    state.world.physics.world.step(1.0 / 60.0);
    let mut planner = FlagSurveyPlanner::new(2);
    tick(&mut state, &mut planner, 0, request, WORK);
    let pending = planner.actors[&0].pending.as_ref().unwrap().clone();
    state
        .world
        .physics
        .world
        .set_pose(body, origin, 0.00001, true);
    state.world.physics.world.step(1.0 / 60.0);
    let region = || QueryRegion {
        previous_position: p.motion.position,
        previous_angle: p.motion.angle,
        current_position: p.motion.position,
        current_angle: p.motion.angle,
        radius: p.radius + 80.0,
        groups: CollisionGroups::ALL,
        excluded: &[],
    };
    assert!(
        pending
            .snapshot
            .matches_region(&state.world.physics.world, region()),
        "exercise the old radius-based tolerance"
    );
    assert!(
        !pending
            .snapshot
            .validate_region(&state.world.physics.world, region())
            .valid
    );
    for _ in 0..MAX_FLAG_SURVEY_AGE {
        tick(&mut state, &mut planner, 0, request, WORK);
        if !planner.samples().is_empty() {
            break;
        }
    }
    assert_eq!(
        planner.samples()[0].reason,
        Some("geometry changed since source measurement")
    );
    assert!(planner.telemetry().geometry_area_tests > 0);
}

#[test]
fn starvation_does_not_renew_a_sources_age() {
    let (mut state, request) = fixture(0);
    let mut planner = FlagSurveyPlanner::new(2);
    tick(&mut state, &mut planner, 0, request, WORK);
    let source = planner.actors[&0]
        .pending
        .as_ref()
        .unwrap()
        .sample
        .source_tick;
    state.world.tick = source + MAX_FLAG_SURVEY_AGE + 1;
    tick(&mut state, &mut planner, 0, request, Work::default());
    let sample = planner.samples()[0];
    assert_eq!(sample.source_tick, source);
    assert_eq!(sample.validated_tick, None);
    assert_eq!(sample.reason, Some("source expired before completion"));
}

#[test]
fn patch_finishes_under_real_quota_in_both_seats_and_is_read_only() {
    for player in 0..2 {
        let (mut state, request) = fixture(player);
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let mut planner = FlagSurveyPlanner::new(2);
        for _ in 0..MAX_FLAG_SURVEY_AGE {
            tick(&mut state, &mut planner, player, request, WORK);
            if !planner.samples().is_empty() {
                break;
            }
        }
        let samples = planner.samples();
        let sample = samples.first().expect("bounded walk survey must finish");
        eprintln!(
            "seat {player}: reason={:?}, graph={}, queries={}, age={}",
            sample.reason,
            sample.graph,
            sample.physics_queries,
            sample.completed_tick - sample.source_tick
        );
        assert_eq!(sample.reason, None);
        assert!(sample.graph < MAX_FLAG_SURVEY_AGE);
        assert_eq!(sample.validated_tick, Some(sample.completed_tick));
        assert_eq!(sample.measurement.tick, sample.source_tick);
        assert_eq!(sample.route.as_ref().unwrap().outbound.jumps, 0);
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
    }
}

#[test]
fn zero_budget_busy_actor_cancellation_clone_and_reset_do_not_leak_work() {
    let (mut state, request) = fixture(0);
    let mut planner = FlagSurveyPlanner::new(2);
    assert_eq!(
        tick(&mut state, &mut planner, 0, request, Work::default()).charged,
        Work::default()
    );
    let o = observation(&state, 0);
    planner.observe(&state, 0, &o, Some(request));
    assert_eq!(
        planner.advance(&state, WORK, &[0]).unwrap().charged,
        Work::default()
    );
    state.world.tick += 1;
    tick(&mut state, &mut planner, 0, request, WORK);
    let mut copy = planner.clone();
    let mut copy_state = state.clone();
    for _ in 0..50 {
        assert_eq!(
            tick(&mut state, &mut planner, 0, request, WORK),
            tick(&mut copy_state, &mut copy, 0, request, WORK)
        );
    }
    let mut o = observation(&state, 0);
    o.local.combat.recovery.flight.pilot.landing.phase = LandingPhase::Landed;
    planner.observe(&state, 0, &o, Some(request));
    assert_eq!(
        planner.advance(&state, WORK, &[]).unwrap().charged,
        Work::default()
    );
    assert!(planner.actors.is_empty());
    assert!(FlagSurveyPlanner::new(2).samples().is_empty());
}

#[test]
fn changed_climb_geometry_withholds_an_otherwise_complete_walk() {
    let (mut state, request) = fixture(0);
    let mut planner = FlagSurveyPlanner::new(2);
    tick(&mut state, &mut planner, 0, request, WORK);
    let measured = planner.actors[&0]
        .pending
        .as_ref()
        .unwrap()
        .sample
        .measurement
        .site
        .unwrap();
    let entity = PhysicsId::new(987654321);
    assert!(state.world.physics.world.insert_body(
        RapierBodyId::new(entity, BodyRole::PRIMARY),
        BodySpec {
            position: measured.vehicle_position + measured.normal * 60.0,
            ..Default::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            2.0
        )],
    ));
    for _ in 0..MAX_FLAG_SURVEY_AGE {
        tick(&mut state, &mut planner, 0, request, WORK);
        if !planner.samples().is_empty() {
            break;
        }
    }
    let sample = planner.samples()[0];
    assert_eq!(
        sample.reason,
        Some("geometry changed since source measurement")
    );
    assert!(sample.route.as_ref().unwrap().cost().is_some());
    assert_eq!(sample.validated_tick, None);
}

#[test]
fn ownership_dirty_queries_and_unseen_requests_cancel_pending_sources() {
    for mutation in 0..3 {
        let (mut state, request) = fixture(0);
        let mut planner = FlagSurveyPlanner::new(2);
        tick(&mut state, &mut planner, 0, request, WORK);
        let mut o = observation(&state, 0);
        if mutation == 0 {
            state.world.planets[request.objective.planet].owner_id = None;
        }
        if mutation == 1 {
            o.local.combat.recovery.flight.pilot.queries_ready = false;
        }
        if mutation != 2 {
            planner.observe(&state, 0, &o, Some(request));
        }
        assert_eq!(
            planner.advance(&state, WORK, &[]).unwrap().charged,
            Work::default()
        );
        assert!(planner.samples().is_empty());
        assert!(planner.actors.is_empty());
    }
}
