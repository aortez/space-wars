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
    let site = state
        .vehicle_landing_site(0, request.candidates[0], false)
        .unwrap();
    // The long collider starts between the +7/+30 climb poses, inside their
    // captured hull union but without obstructing either original query.
    let direction = Vec2::new(site.normal.y, -site.normal.x);
    let angle = rotation_for_direction(direction);
    let origin = site.vehicle_position + site.normal * 18.0 + direction * 10000.0;
    assert!(state.world.physics.world.insert_body(
        body,
        BodySpec {
            kind: BodyKind::Fixed,
            position: origin,
            angle,
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
        .set_pose(body, origin, angle + 0.00001, true);
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
    let detail = planner.samples()[0].geometry.as_ref().unwrap();
    assert!(detail.report.complete);
    assert!(!detail.acceptance_prefix.valid);
    assert!(detail.report.changes.iter().any(|c| {
        c.current
            .as_ref()
            .and_then(|s| s.collider)
            .is_some_and(|id| id.entity == entity)
            && c.motion_bound.is_some_and(|d| d > 0.002)
    }));
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
        assert!(
            sample
                .validation
                .as_ref()
                .is_some_and(|v| v.complete && v.predicates_valid && v.geometry.valid)
        );
        assert!(sample.geometry.is_none());
        assert_eq!(planner.telemetry().geometry_diagnostics, 0);
        assert!(sample.graph < MAX_FLAG_SURVEY_AGE);
        assert_eq!(sample.validated_tick, Some(sample.completed_tick));
        assert_eq!(sample.measurement.tick, sample.source_tick);
        assert_eq!(sample.route.as_ref().unwrap().outbound.jumps, 0);
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
    }
}

fn completed_source(
    empty_other_vehicle: bool,
) -> (
    SurfaceSortieState,
    PilotObservationV1,
    Pending,
    ObjectiveSurveyJob,
    LandingObjectiveRoute,
) {
    let (mut state, request) = fixture(0);
    if empty_other_vehicle {
        let other = state.pilots[1].vehicle.0;
        let entity = state.world.physics.surface_vehicle_entity(other);
        let roles: std::collections::BTreeSet<_> = state
            .world
            .physics
            .world
            .collider_ids()
            .filter(|c| c.entity == entity)
            .map(|c| c.role)
            .collect();
        for role in roles {
            assert!(state.world.physics.world.replace_colliders(
                state.world.physics.ship_body(other),
                role,
                &[]
            ));
        }
    }
    freeze(&mut state);
    let mut planner = FlagSurveyPlanner::new(2);
    tick(&mut state, &mut planner, 0, request, WORK);
    let actor = &planner.actors[&0];
    let mut pending = actor.pending.as_ref().unwrap().clone();
    let mut p = actor.pilot.clone();
    // A tighter interaction range prevents this controlled fixture's hatch
    // node from already being a destination. Runtime rules are unchanged.
    p.planet.claim.as_mut().unwrap().flag_interaction_range = 1.2;
    p.sites = vec![pending.sample.measurement.site.unwrap()];
    let objective = LandingObjective::read(&p).unwrap();
    let center = ((-objective.position.x)
        .atan2(objective.position.y)
        .rem_euclid(std::f32::consts::TAU)
        * ground_navigation::GROUND_SAMPLES as f32
        / std::f32::consts::TAU)
        .round() as u16
        % ground_navigation::GROUND_SAMPLES as u16;
    let mut job = state
        .objective_job_with_planning(
            0,
            &p,
            &[],
            Arc::clone(&pending.snapshot),
            None,
            false,
            ObjectivePlanning::JointRoundTrip,
        )
        .unwrap()
        .with_walk_patch(center, PATCH_HALF_WIDTH);
    pending.source = local::Source::new(&state, 0, &p, pending.source.footprint.clone());
    while job.next_work().is_some() {
        job.step();
    }
    let route = job.output().unwrap().sites[0].clone();
    assert!(route.cost().is_some());
    assert!(
        route.outbound.length > 2.0,
        "exercise several outward edges (the endpoint is already in boarding range): {route:?}"
    );
    (state, p, pending, job, route)
}

fn freeze(state: &mut SurfaceSortieState) {
    let bodies: Vec<_> = state.world.physics.world.motions().map(|b| b.id).collect();
    for body in bodies {
        state
            .world
            .physics
            .world
            .set_body_kind(body, BodyKind::Fixed, true);
    }
    state.world.physics.world.step(1.0 / 60.0);
}

fn blocker(state: &mut SurfaceSortieState, point: Vec2) -> (RapierBodyId, ColliderId) {
    let entity = PhysicsId::new(987654323);
    let body = RapierBodyId::new(entity, BodyRole::PRIMARY);
    let collider = ColliderId::new(entity, ColliderRole::PRIMARY, 0);
    assert!(state.world.physics.world.insert_body(
        body,
        BodySpec {
            kind: BodyKind::Fixed,
            position: point,
            ..Default::default()
        },
        &[ColliderSpec::ball(collider, 0.05)]
    ));
    (body, collider)
}

#[test]
fn local_capture_covers_nonzero_walk_and_every_query_class() {
    let (state, p, pending, job, route) = completed_source(false);
    let base = pending
        .source
        .validate(&state, 0, &p, &pending, &job, &route);
    assert!(base.complete && base.predicates_valid && base.geometry.valid);
    assert_eq!(base.source_areas.len(), 5);
    assert!(base.captured_queries > 0 && base.walking_queries > 0);
    for area in &base.source_areas {
        let mut changed = state.clone();
        let point = p.planet.motion.position
            + ((area.minimum + area.maximum) * 0.5).rotate_radians(p.planet.motion.angle);
        blocker(&mut changed, point);
        let check = pending
            .source
            .validate(&changed, 0, &p, &pending, &job, &route);
        assert!(check.predicates_valid, "{:?}", check.predicate_failure);
        assert!(
            !check.geometry.valid,
            "new obstacle inside captured area {area:?}"
        );
    }
}

#[test]
fn unrelated_motion_inside_old_circle_can_publish_with_identical_work() {
    let (mut state, request) = fixture(0);
    freeze(&mut state);
    let frame = observation(&state, 0).planets[1].motion;
    // The opposite hemisphere is in the old planetary gate but outside the
    // actual landing and flag-patch queries.
    let up = request.objective.position.normalized();
    let (body, _) = blocker(&mut state, frame.position - up * 65.0);
    state.world.physics.world.step(1.0 / 60.0);
    let mut planner = FlagSurveyPlanner::new(2);
    tick(&mut state, &mut planner, 0, request, WORK);
    let mut baseline = planner.clone();
    let mut original = state.clone();
    state
        .world
        .physics
        .world
        .set_pose(body, frame.position - up * 66.0, 0.0, true);
    state.world.physics.world.step(1.0 / 60.0);
    for _ in 0..MAX_FLAG_SURVEY_AGE {
        assert_eq!(
            tick(&mut state, &mut planner, 0, request, WORK),
            tick(&mut original, &mut baseline, 0, request, WORK)
        );
        if !planner.samples().is_empty() {
            break;
        }
    }
    let sample = planner.samples()[0];
    assert_eq!(sample.reason, None);
    assert!(!sample.geometry.as_ref().unwrap().acceptance_prefix.valid);
    assert!(sample.validation.as_ref().unwrap().geometry.valid);
    assert_eq!(planner.telemetry().local_rescued, 1);
    assert_eq!(sample.source_tick, baseline.samples()[0].source_tick);
}

#[test]
fn nearby_vehicle_predicate_can_fail_without_a_local_collider_change() {
    let (mut state, p, pending, job, route) = completed_source(true);
    let site = pending.sample.measurement.site.unwrap();
    let other = state.pilots[1].vehicle.0;
    let body = state.world.physics.ship_body(other);
    // A live body without colliders isolates the body-center predicate,
    // which cannot be inferred from collider intersections.
    let right = Vec2::new(site.normal.y, -site.normal.x);
    let target = site.vehicle_position - right * 15.9;
    state
        .world
        .physics
        .world
        .set_pose(body, target, rotation_for_direction(site.normal), true);
    state.world.physics.world.step(1.0 / 60.0);
    let check = pending
        .source
        .validate(&state, 0, &p, &pending, &job, &route);
    assert_eq!(
        check.predicate_failure,
        Some("landing vehicle neighborhood occupied")
    );
    assert!(
        check.geometry.valid,
        "fixture must isolate the non-query predicate"
    );
}

#[test]
fn raw_hull_replacement_and_preview_changes_withhold_publication() {
    let (state, p, pending, job, route) = completed_source(false);
    for change in 0..3 {
        let mut changed = state.clone();
        if change == 0 {
            let hull = changed.world.physics.surface_hull_id(0);
            assert!(changed.world.physics.world.replace_colliders(
                changed.world.physics.ship_body(0),
                hull.role,
                &[ColliderSpec::ball(hull, 1.0)]
            ));
        } else if change == 1 {
            changed.world.ships[0].wing_theta += 0.1;
        } else {
            // The replacement's collision groups follow the pilot owner,
            // independently of the retained live ship's assembly.
            changed.pilots[0].owner = PlayerId::PLAYER_2;
        }
        let check = pending
            .source
            .validate(&changed, 0, &p, &pending, &job, &route);
        assert_eq!(
            check.predicate_failure,
            Some("source vehicle geometry changed"),
            "change {change}"
        );
    }
}

#[test]
fn dirty_queries_radius_gravity_and_incomplete_capture_remain_unknown() {
    let (state, p, pending, job, route) = completed_source(false);
    for change in 0..4 {
        let mut changed = state.clone();
        let mut p = p.clone();
        let mut pending = pending.clone();
        let expected = match change {
            0 => {
                changed.world.physics.material_queries_dirty = true;
                "material queries unavailable"
            }
            1 => {
                p.planet.radius += 0.01;
                "source objective or radius changed"
            }
            2 => {
                changed.world.planets[p.planet.index].mass = f32::NAN;
                "source scalar gravity changed"
            }
            _ => {
                pending.source.footprint.complete = false;
                "query dependency capture incomplete"
            }
        };
        let check = pending
            .source
            .validate(&changed, 0, &p, &pending, &job, &route);
        assert_eq!(check.predicate_failure, Some(expected));
        assert!(!check.predicates_valid);
    }
}

#[test]
fn endpoint_range_is_rechecked_inside_objective_matching_tolerance() {
    let (state, mut p, mut pending, job, route) = completed_source(false);
    let node = route.endpoint.unwrap();
    let center =
        node.position + node.position.normalized() * SurfaceSortieState::spec().half_height();
    let distance = center.distance_to(LandingObjective::read(&p).unwrap().position);
    p.planet.claim.as_mut().unwrap().flag_interaction_range = distance + 0.2 + 0.00004;
    pending.source = local::Source::new(&state, 0, &p, pending.source.footprint.clone());
    let old = LandingObjective::read(&p).unwrap();
    assert!(
        pending
            .source
            .validate(&state, 0, &p, &pending, &job, &route)
            .predicates_valid
    );
    p.planet.claim.as_mut().unwrap().flag_interaction_range -= 0.00008;
    assert!(LiveObjectivePlanner::same_objective(
        old,
        LandingObjective::read(&p).unwrap()
    ));
    assert_eq!(
        pending
            .source
            .validate(&state, 0, &p, &pending, &job, &route)
            .predicate_failure,
        Some("source objective or radius changed")
    );
}

#[test]
fn recording_both_hatches_and_settling_queries_preserves_atomic_measurement() {
    let (state, request) = fixture(0);
    let ordinary_fuel = QueryFuel::default();
    let ordinary =
        destinations::measure(&state, 0, request.candidates[0], None, true, &ordinary_fuel);
    let fuel = QueryFuel::default();
    let events = std::cell::RefCell::new(Vec::new());
    let recorded = destinations::measure_with_query_observer(
        &state,
        0,
        request.candidates[0],
        None,
        true,
        &fuel,
        |q| events.borrow_mut().push(q),
    );
    assert_eq!(ordinary, recorded);
    assert_eq!(ordinary_fuel.used(), fuel.used());
    assert_eq!(
        events.borrow().len(),
        fuel.used() as usize,
        "no armed opponent/cover queries in this fixture"
    );
    let site = recorded.site.unwrap();
    assert!(site.boarding_hatches.iter().all(Option::is_some));
    assert!(site.hatch_has_settling_margin);
    let frame = recorded.planet;
    let fp = std::cell::RefCell::new(query_footprint::QueryFootprint::new(
        frame.position,
        frame.angle,
    ));
    for &event in events.borrow().iter() {
        local::record(&state, 0, &fp, event, frame);
    }
    let fp = fp.into_inner();
    assert!(fp.complete);
    let local = |p: Vec2| (p - frame.position).rotate_radians(-frame.angle);
    let contains = |area: QueryArea, point: Vec2| {
        point.x >= area.minimum.x
            && point.x <= area.maximum.x
            && point.y >= area.minimum.y
            && point.y <= area.maximum.y
    };
    for h in site.boarding_hatches.into_iter().flatten() {
        assert!(contains(fp.areas[0].unwrap(), local(h)));
        assert!(contains(
            fp.areas[1].unwrap(),
            local(h + site.normal * SurfaceSortieState::spec().half_height())
        ));
    }
    assert!(fp.queries[0] > 5 && fp.queries[1] > 5 && fp.queries[2] == 6);
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
    let detail = sample.geometry.as_ref().unwrap();
    assert!(detail.report.complete);
    assert_eq!(detail.report.omitted_changes, 0);
    let climb = detail
        .envelopes
        .iter()
        .position(|a| a.name == "climb_60")
        .unwrap();
    let change = detail
        .report
        .changes
        .iter()
        .find(|c| {
            c.current
                .as_ref()
                .and_then(|s| s.collider)
                .is_some_and(|id| id.entity == entity)
        })
        .unwrap();
    assert!(change.previous.is_none());
    assert!(change.areas.contains(&climb));
    assert_eq!(detail.report.area_changes[climb], 1);
    assert_eq!(planner.telemetry().geometry_diagnostics, 1);
    assert_eq!(
        planner.telemetry().diagnostic_area_tests,
        detail.report.area_tests
    );
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
