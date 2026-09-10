//! Decision contracts use synthetic observations; the flag soak separately
//! verifies traversal, capture, destruction and recovery in shared physics.
use engine_common::Scenario;
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        PilotLocation, PlanetFlagObservation, SurfaceSortieAction, SurfaceSortieScenario,
        ground_navigation::{GroundEdge, GroundEdgeKind, GroundMap, GroundNode},
        pilot::PilotMotion,
        recovery_sensors::RecoveryTaskObservationV1,
    },
};
use spacewars_ai::{
    BrainReset,
    ground_task::{GroundDestination, GroundGoal, GroundNavigationTask},
};
use std::time::Duration;

fn fixture() -> (BrainReset, RecoveryTaskObservationV1) {
    let mut state = SurfaceSortieScenario::init_material(42, 1);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let mut o = state.recovery_task_observation(0, None);
    let p = &mut o.flight.pilot;
    p.tick = 0;
    p.location = PilotLocation::OnFoot;
    p.controls_armed = true;
    p.queries_ready = true;
    p.actor_up = Vec2::Y;
    p.supported_planet = Some(p.planet.index);
    p.balanced = true;
    p.planet.motion = PilotMotion {
        position: Vec2::ZERO,
        velocity: Vec2::ZERO,
        angle: 0.0,
        spin: 0.0,
    };
    p.actor = Some(PilotMotion {
        position: Vec2::new(0.0, 60.9),
        velocity: Vec2::ZERO,
        angle: 0.0,
        spin: 0.0,
    });
    p.hatch = Some(Vec2::new(6.0, 60.0));
    // This fixture isolates traversal to an already landed ship. Tests for an
    // unsettled hatch explicitly replace these observed boarding conditions.
    p.landing.phase = scenario_spacewars::surface_sortie::LandingPhase::Landed;
    p.landing.supported_feet = 2;
    p.transfer = scenario_spacewars::surface_sortie::TransferResult::TooFar;
    let context = BrainReset {
        actor: p.owner,
        episode_seed: 42,
    };
    o.ground = Some(GroundMap {
        version: 1,
        actor: p.owner,
        planet: p.planet.index,
        revision: p.planet.revision,
        tick: 0,
        nodes: (0..4)
            .map(|id| GroundNode {
                id,
                position: Vec2::new(f32::from(id) * 2.0, 60.0),
                normal: Vec2::Y,
            })
            .collect(),
        rejected: Vec::new(),
        edges: (0..3)
            .map(|id| GroundEdge {
                from: id,
                to: id + 1,
                kind: GroundEdgeKind::Walk,
                length: 2.0,
            })
            .collect(),
    });
    (context, o)
}
fn advance(o: &mut RecoveryTaskObservationV1, tick: u64) {
    o.flight.pilot.tick = tick;
    if let Some(posture) = &mut o.posture {
        posture.tick = tick;
    }
    if let Some(map) = &mut o.ground {
        map.tick = tick;
    }
}

fn blocked_posture(o: &mut RecoveryTaskObservationV1) {
    use scenario_spacewars::surface_sortie::ground_posture::{
        CrawlStep, GroundPostureObservation, SpacelingBalance, SpacelingGetUpResult,
    };
    let p = &mut o.flight.pilot;
    p.balanced = false;
    o.posture = Some(GroundPostureObservation {
        version: 1,
        owner: p.owner,
        planet: p.planet.index,
        revision: p.planet.revision,
        tick: p.tick,
        balance: SpacelingBalance::Recovering,
        get_up_result: SpacelingGetUpResult::Blocked,
        get_up_attempts: 1,
        stable: true,
        crawl: [
            Some(CrawlStep {
                direction: -1.0,
                position: Vec2::new(-0.6, 60.9),
            }),
            None,
        ],
    });
}

#[test]
fn blocked_get_up_backs_out_then_resurveys_before_resuming_the_hatch_route() {
    use scenario_spacewars::surface_sortie::ground_posture::SpacelingGetUpResult;
    let (context, mut o) = fixture();
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    blocked_posture(&mut o);
    advance(&mut o, 1);
    let unchanged = o.clone();
    let crawl = task.step(&o);
    assert_eq!(
        crawl.horizontal, -1.0,
        "only the route away from the hatch is clear"
    );
    assert!(!crawl.interact_held);
    assert_eq!(task.telemetry().goal, GroundGoal::Crawl);
    assert_eq!(task.telemetry().get_up_repositions, 1);
    assert_eq!(
        task.step(&o),
        crawl,
        "repeated observations keep the button edge"
    );
    assert_eq!(o, unchanged);
    let mut replay = task.clone();
    o.flight.pilot.actor.as_mut().unwrap().position.x = -0.4;
    o.posture.as_mut().unwrap().get_up_result = SpacelingGetUpResult::Started;
    advance(&mut o, 62);
    let lifting = task.step(&o);
    assert_eq!(lifting, replay.step(&o));
    assert_eq!(
        lifting.horizontal, 0.0,
        "let the ordinary get-up lift finish"
    );
    o.flight.pilot.balanced = true;
    advance(&mut o, 90);
    task.step(&o);
    assert!(
        task.telemetry().replans >= 2,
        "standing requires a new measured route"
    );
    assert_ne!(task.telemetry().goal, GroundGoal::Arrived);
    assert!(task.telemetry().crawl_direction.is_none());
    o.flight.pilot.actor.as_mut().unwrap().position = Vec2::new(5.0, 60.9);
    advance(&mut o, 91);
    assert!(!task.step(&o).interact_held);
    assert_eq!(
        task.telemetry().goal,
        GroundGoal::Arrived,
        "actual hatch range ends traversal"
    );
}

#[test]
fn crawl_waits_for_settled_recovery_and_has_a_deadline_even_if_clearance_disappears() {
    use scenario_spacewars::surface_sortie::ground_posture::{
        SpacelingBalance, SpacelingGetUpResult,
    };
    for fault in 0..4 {
        let (context, mut o) = fixture();
        blocked_posture(&mut o);
        let posture = o.posture.as_mut().unwrap();
        match fault {
            0 => posture.stable = false,
            1 => posture.balance = SpacelingBalance::KnockedDown,
            2 => posture.get_up_result = SpacelingGetUpResult::Unsettled,
            _ => posture.crawl = [None, None],
        }
        let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
        assert_eq!(task.step(&o).horizontal, 0.0, "fault={fault}");
        assert_eq!(task.telemetry().get_up_repositions, 0);
    }
    let (context, mut o) = fixture();
    blocked_posture(&mut o);
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    assert_eq!(task.step(&o).horizontal, -1.0);
    o.posture.as_mut().unwrap().crawl = [None, None];
    advance(&mut o, 8 * 60 + 1);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(
        task.telemetry().reason,
        Some("unable to crawl clear for standing")
    );
    task.reset(context);
    assert_eq!(task.telemetry().get_up_repositions, 0);
}

#[test]
fn crawl_rejects_stale_identity_and_unbounded_or_invalid_corridors() {
    for fault in 0..8 {
        let (context, mut o) = fixture();
        blocked_posture(&mut o);
        let posture = o.posture.as_mut().unwrap();
        match fault {
            0 => posture.tick += 1,
            1 => posture.owner = PlayerId::PLAYER_2,
            2 => posture.planet += 1,
            3 => posture.revision += 1,
            4 => posture.version += 1,
            5 => posture.crawl[0].as_mut().unwrap().direction = 2.0,
            6 => posture.crawl[0].as_mut().unwrap().position.x = f32::NAN,
            _ => posture.crawl[0].as_mut().unwrap().position.x = -10.0,
        }
        let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
        assert_eq!(
            task.step(&o),
            SurfaceSortieAction::default(),
            "fault={fault}"
        );
        assert_eq!(task.telemetry().goal, GroundGoal::Blocked, "fault={fault}");
    }
}

#[test]
fn a_measured_crawl_finishes_despite_pose_jitter_but_loses_eligibility_on_world_changes() {
    for change in 0..4 {
        let (context, mut o) = fixture();
        blocked_posture(&mut o);
        let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
        assert_eq!(task.step(&o).horizontal, -1.0);
        o.posture.as_mut().unwrap().crawl = [None, None];
        advance(&mut o, 2);
        assert_eq!(
            task.step(&o).horizontal,
            -1.0,
            "finish the measured short move"
        );
        advance(&mut o, 3);
        match change {
            0 => o.flight.pilot.supported_planet = None,
            1 => o.flight.pilot.queries_ready = false,
            2 => {
                o.flight.pilot.planet.revision += 1;
                o.posture.as_mut().unwrap().revision += 1;
            }
            _ => advance(&mut o, 60),
        }
        assert_eq!(task.step(&o).horizontal, 0.0, "change={change}");
    }
}

#[test]
fn an_overhead_waypoint_needs_a_jump_even_inside_the_horizontal_dead_zone() {
    let (context, mut o) = fixture();
    let map = o.ground.as_mut().unwrap();
    map.nodes[1].position = Vec2::new(0.1, 62.0);
    map.edges[0].kind = GroundEdgeKind::Jump;
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    advance(&mut o, 1);
    task.step(&o);
    advance(&mut o, 48);
    let action = task.step(&o);
    assert!(
        action.primary_held,
        "the higher step requires ordinary jump input"
    );
    assert!(action.horizontal.abs() < 0.1);
    assert_eq!(task.telemetry().goal, GroundGoal::Jump);
    advance(&mut o, 49);
    assert!(!task.step(&o).primary_held, "no held or repeated jump edge");
}

fn add_jetpack(o: &mut RecoveryTaskObservationV1, charge: f32) {
    use scenario_spacewars::surface_sortie::jetpack::{
        CrossingAnchor, CrossingDirection, CrossingPlan, JetpackNavigationObservation,
    };
    o.jetpack = Some(JetpackNavigationObservation {
        charge,
        burning: false,
        burn_seconds: 0.0,
        gravity: -Vec2::Y * 18.0,
        surveyed: true,
        terrain_crossings: Vec::new(),
        crossing: Some(CrossingPlan {
            planet: o.flight.pilot.planet.index,
            revision: o.flight.pilot.planet.revision,
            direction: CrossingDirection::Left,
            start: Vec2::new(0.0, 60.0),
            destination: Vec2::new(6.0, 60.0),
            cruise_radius: 72.0,
            anchor: CrossingAnchor::Vehicle {
                index: 0,
                form: scenario_spacewars::ShipForm::Ship,
                position: Vec2::new(3.0, 60.0),
                angle: 0.0,
            },
        }),
    });
}

#[test]
fn local_route_advances_then_resurveys_without_claiming_arrival() {
    let (context, mut o) = fixture();
    add_jetpack(&mut o, 1.0);
    o.jetpack.as_mut().unwrap().crossing = None;
    o.ground.as_mut().unwrap().edges.truncate(1);
    let original = o.clone();
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    assert!(task.telemetry().route.as_ref().unwrap().partial);
    assert_eq!(task.telemetry().partial_routes, 1);
    assert_eq!(o, original, "planning must not mutate the observation");
    advance(&mut o, 1);
    assert!(task.step(&o).horizontal > 0.0);
    let mut replay = task.clone();
    o.flight.pilot.actor.as_mut().unwrap().position.x = 2.0;
    advance(&mut o, 60);
    assert_eq!(task.step(&o), replay.step(&o));
    assert_eq!(task.telemetry().goal, GroundGoal::Survey);
    assert!(task.telemetry().path.is_empty());
    o.ground.as_mut().unwrap().edges.push(GroundEdge {
        from: 1,
        to: 2,
        kind: GroundEdgeKind::Walk,
        length: 2.0,
    });
    advance(&mut o, 61);
    task.step(&o);
    assert!(!task.telemetry().route.as_ref().unwrap().partial);
    assert_ne!(task.telemetry().goal, GroundGoal::Arrived);
    task.reset(context);
    let mut fresh = GroundNavigationTask::new(context, GroundDestination::Hatch);
    assert_eq!(task.step(&original), fresh.step(&original));
    assert_eq!(task.telemetry(), fresh.telemetry());
}

#[test]
fn knockback_waits_for_contact_then_replans_with_the_original_deadline() {
    let (context, mut o) = fixture();
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    o.flight.pilot.actor.as_mut().unwrap().position = Vec2::new(0.0, 80.0);
    o.flight.pilot.supported_planet = None;
    advance(&mut o, 30);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(task.telemetry().displacements, 1);
    assert_eq!(task.telemetry().goal, GroundGoal::Settle);
    advance(&mut o, 600); // Airborne for longer than the normal progress timeout.
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(task.telemetry().goal, GroundGoal::Settle);
    let mut replay = task.clone();
    o.flight.pilot.actor.as_mut().unwrap().position = Vec2::new(2.0, 60.9);
    o.flight.pilot.supported_planet = Some(o.flight.pilot.planet.index);
    advance(&mut o, 601);
    assert_eq!(task.step(&o), replay.step(&o));
    assert_eq!(task.telemetry().started_tick, Some(0));
    assert_eq!(task.telemetry().replans, 2);
    advance(&mut o, 90 * 60 + 1);
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::Blocked);
    assert_eq!(
        task.telemetry().reason,
        Some("ground traversal exceeded ninety seconds")
    );
}

#[test]
fn route_can_chain_two_measured_flights_without_changing_the_sensor_map() {
    use scenario_spacewars::surface_sortie::jetpack::CrossingAnchor;
    let (context, mut o) = fixture();
    add_jetpack(&mut o, 1.0);
    let map = o.ground.as_mut().unwrap();
    map.nodes = (0..9)
        .map(|id| GroundNode {
            id,
            position: Vec2::new(f32::from(id) * 2.0, 60.0),
            normal: Vec2::Y,
        })
        .collect();
    map.edges = (0..8)
        .filter(|id| *id != 1 && *id != 5)
        .map(|id| GroundEdge {
            from: id,
            to: id + 1,
            kind: GroundEdgeKind::Walk,
            length: 2.0,
        })
        .collect();
    let mut second = o.jetpack.as_ref().unwrap().crossing.clone().unwrap();
    second.start = Vec2::new(8.0, 60.0);
    second.destination = Vec2::new(16.0, 60.0);
    second.anchor = CrossingAnchor::GroundGap { from: 5, to: 6 };
    o.jetpack.as_mut().unwrap().terrain_crossings.push(second);
    let original = o.ground.clone();
    let mut task = GroundNavigationTask::new(
        context,
        GroundDestination::Rebuild {
            planet: o.flight.pilot.planet.index,
            position: Vec2::new(16.0, 60.0),
        },
    );
    task.step(&o);
    assert!(
        task.is_crossing(),
        "neither flight alone reaches the objective"
    );
    assert_eq!(task.telemetry().route.as_ref().unwrap().flights, 2);
    assert_eq!(o.ground, original);
    assert_eq!(
        task.telemetry()
            .crossing
            .as_ref()
            .unwrap()
            .plan
            .as_ref()
            .unwrap()
            .start,
        Vec2::new(0.0, 60.0)
    );
}

#[test]
fn flag_route_uses_the_supported_actor_center_and_the_real_interaction_range() {
    let (context, mut o) = fixture();
    let p = &mut o.flight.pilot;
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: PlayerId::PLAYER_2,
        position: Vec2::new(8.5, 60.0),
        normal: Vec2::Y,
        raised_fraction: 1.0,
    });
    assert_eq!(claim.flag_interaction_range, 3.0);
    let mut task = GroundNavigationTask::new(context, GroundDestination::Flag);
    task.step(&o);
    assert!(!task.telemetry().path.is_empty());
    assert_eq!(task.telemetry().route.as_ref().unwrap().failure, None);
    o.flight.pilot.actor.as_mut().unwrap().position = Vec2::new(6.0, 60.9);
    advance(&mut o, 1);
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::Arrived);
    o.flight.pilot.supported_planet = None;
    advance(&mut o, 2);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(
        task.telemetry().goal,
        GroundGoal::Settle,
        "contact loss near the flag must settle without restarting a jump"
    );
    advance(&mut o, 90 * 60 + 1);
    task.step(&o);
    assert_eq!(
        task.telemetry().goal,
        GroundGoal::Blocked,
        "waiting near a destination cannot reset the task deadline"
    );
}

#[test]
fn walking_wins_on_clear_ground_and_disconnected_ground_waits_for_real_charge() {
    let (context, mut o) = fixture();
    add_jetpack(&mut o, 0.2);
    let mut walking = GroundNavigationTask::new(context, GroundDestination::Hatch);
    walking.step(&o);
    assert!(!walking.is_crossing());
    o.ground.as_mut().unwrap().edges.retain(|e| e.from != 1);
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    o.jetpack.as_mut().unwrap().surveyed = false;
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::Survey);
    assert!(!task.is_crossing());
    advance(&mut o, 1);
    o.jetpack.as_mut().unwrap().surveyed = true;
    let first = task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::Recharge);
    assert!(!first.primary_held);
    assert_eq!(task.step(&o), first);
    let mut copy = task.clone();
    advance(&mut o, 90);
    assert_eq!(task.step(&o), copy.step(&o));
    assert_eq!(task.telemetry(), copy.telemetry());
    assert_eq!(
        task.telemetry().goal,
        GroundGoal::Recharge,
        "elapsed time is not a refill"
    );
    o.jetpack.as_mut().unwrap().charge = 0.99;
    advance(&mut o, 91);
    task.step(&o);
    advance(&mut o, 92);
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::JetpackLift);
    task.reset(context);
    assert!(!task.is_crossing());
}

#[test]
fn objective_changes_preserve_landing_and_missing_corridors_interrupt_without_resetting_deadline() {
    let (context, mut o) = fixture();
    add_jetpack(&mut o, 1.0);
    o.ground.as_mut().unwrap().edges.retain(|e| e.from != 1);
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    for tick in 0..3 {
        advance(&mut o, tick);
        task.step(&o);
    }
    assert!(task.is_crossing());
    o.flight.pilot.supported_planet = None;
    o.flight.pilot.actor.as_mut().unwrap().position.y += 5.0;
    task.retarget(GroundDestination::Flag);
    advance(&mut o, 3);
    task.step(&o);
    assert!(
        task.is_crossing(),
        "a vanished flag must not abandon an airborne actor"
    );
    o.jetpack.as_mut().unwrap().crossing = None;
    advance(&mut o, 4);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(task.telemetry().flight_interruptions, 1);
    assert_eq!(task.telemetry().goal, GroundGoal::Settle);
    let mut landed = task.clone();
    let mut footing = o.clone();
    footing.flight.pilot.supported_planet = Some(footing.flight.pilot.planet.index);
    advance(&mut footing, 5);
    landed.step(&footing);
    assert_eq!(landed.telemetry().goal, GroundGoal::Arrived);
    advance(&mut o, 90 * 60 + 1);
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("ground traversal exceeded ninety seconds")
    );
}

#[test]
fn traversal_replay_clone_reset_and_local_frame_contract() {
    let (context, mut o) = fixture();
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    assert_eq!(task.telemetry().path, vec![0, 1, 2]);
    advance(&mut o, 1);
    let action = task.step(&o);
    assert!(action.horizontal > 0.0);
    let before = task.telemetry().clone();
    assert_eq!(task.step(&o), action);
    assert_eq!(task.telemetry(), &before);
    let mut copy = task.clone();
    // Rigid movement of the planet does not invalidate local route coordinates.
    let shift = Vec2::new(30.0, -80.0);
    let p = &mut o.flight.pilot;
    p.planet.motion.position = shift;
    p.planet.motion.angle = 0.7;
    p.actor.as_mut().unwrap().position = shift + p.actor.unwrap().position.rotate_radians(0.7);
    p.actor_up = Vec2::Y.rotate_radians(0.7);
    p.hatch = p.hatch.map(|v| shift + v.rotate_radians(0.7));
    advance(&mut o, 2);
    assert_eq!(task.step(&o), copy.step(&o));
    assert_eq!(task.telemetry(), copy.telemetry());
    assert_eq!(task.telemetry().replans, 1);
    assert_eq!(task.telemetry().invalidations, 0);
    task.reset(context);
    assert_eq!(
        task.telemetry(),
        GroundNavigationTask::new(context, GroundDestination::Hatch).telemetry()
    );
}

#[test]
fn revisions_and_obstacles_invalidate_routes_but_dirty_queries_only_hold() {
    let (context, mut o) = fixture();
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    let route = task.telemetry().path.clone();
    advance(&mut o, 1);
    o.flight.pilot.queries_ready = false;
    o.ground = None;
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(task.telemetry().path, route);
    assert_eq!(task.telemetry().invalidations, 0);
    advance(&mut o, 2);
    o.flight.pilot.queries_ready = true;
    o.flight.pilot.planet.revision += 1;
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert!(task.telemetry().path.is_empty());
    assert_eq!(task.telemetry().invalidations, 1);
    let (_, fresh) = fixture();
    o.ground = fresh.ground;
    o.ground.as_mut().unwrap().revision = o.flight.pilot.planet.revision;
    advance(&mut o, 30);
    task.step(&o);
    assert_eq!(task.telemetry().replans, 2);
    // Another actor can obstruct a route without editing the terrain revision.
    o.ground.as_mut().unwrap().edges.clear();
    advance(&mut o, 60);
    task.step(&o);
    assert_eq!(task.telemetry().invalidations, 2);
    assert!(task.telemetry().path.is_empty());
    advance(&mut o, 361);
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::Blocked);
    assert_eq!(
        task.telemetry().reason,
        Some("no measured walk/jump route to destination")
    );
    o.ground = fixture().1.ground;
    advance(&mut o, 400);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(
        task.telemetry().goal,
        GroundGoal::Blocked,
        "failure requires caller reset"
    );
}

#[test]
fn jump_edges_pulse_once_and_arrival_requires_planet_support() {
    let (context, mut o) = fixture();
    o.ground.as_mut().unwrap().edges[0].kind = GroundEdgeKind::Jump;
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    advance(&mut o, 1);
    assert!(task.step(&o).primary_held);
    advance(&mut o, 2);
    assert!(!task.step(&o).primary_held);
    assert_eq!(task.telemetry().jumps, 1);
    o.flight.pilot.actor.as_mut().unwrap().position = Vec2::new(6.0, 60.9);
    o.flight.pilot.supported_planet = None;
    advance(&mut o, 3);
    task.step(&o);
    assert_ne!(task.telemetry().goal, GroundGoal::Arrived);
    o.flight.pilot.supported_planet = Some(o.flight.pilot.planet.index);
    advance(&mut o, 4);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(task.telemetry().goal, GroundGoal::Arrived);
    advance(&mut o, 10000);
    task.step(&o);
    assert_eq!(
        task.telemetry().goal,
        GroundGoal::Arrived,
        "finished arrival must not time out"
    );
}

#[test]
fn missing_map_dirty_queries_and_stalled_motion_have_bounded_deadlines() {
    for dirty in [false, true] {
        let (context, mut o) = fixture();
        o.ground = None;
        o.flight.pilot.queries_ready = !dirty;
        let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
        task.step(&o);
        advance(&mut o, 90 * 60 + 1);
        task.step(&o);
        assert_eq!(
            task.telemetry().reason,
            Some("ground traversal exceeded ninety seconds")
        );
    }
    let (context, mut o) = fixture();
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    advance(&mut o, 1);
    task.step(&o);
    advance(&mut o, 8 * 60 + 2);
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("spaceling stopped making progress on ground route")
    );
}

#[test]
fn identity_versions_and_malformed_maps_fail_closed() {
    for fault in 0..9 {
        let (context, mut o) = fixture();
        match fault {
            0 => o.flight.pilot.owner = PlayerId::PLAYER_2,
            1 => o.flight.version = 99,
            2 => o.flight.flight.version = 99,
            3 => o.ground.as_mut().unwrap().actor = PlayerId::PLAYER_2,
            4 => o.ground.as_mut().unwrap().nodes[1].id = 0,
            5 => o.ground.as_mut().unwrap().nodes[0].position.x = f32::NAN,
            6 => o.ground.as_mut().unwrap().edges[0].to = 255,
            7 => o.ground.as_mut().unwrap().edges[0].length = -1.0,
            _ => o.ground.as_mut().unwrap().tick = 1,
        }
        let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
        assert_eq!(
            task.step(&o),
            SurfaceSortieAction::default(),
            "fault={fault}"
        );
        assert_eq!(task.telemetry().goal, GroundGoal::Blocked, "fault={fault}");
    }
}

#[test]
fn unarmed_actor_waits_and_get_up_is_an_ordinary_fresh_jump() {
    let (context, mut o) = fixture();
    o.flight.pilot.controls_armed = false;
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert!(task.telemetry().started_tick.is_none());
    o.flight.pilot.controls_armed = true;
    o.flight.pilot.balanced = false;
    advance(&mut o, 1);
    assert!(task.step(&o).primary_held);
    assert_eq!(task.telemetry().goal, GroundGoal::GetUp);
    advance(&mut o, 2);
    assert!(!task.step(&o).primary_held);
    advance(&mut o, 62);
    assert!(task.step(&o).primary_held);
}

#[test]
fn changed_destination_replans_and_destroyed_flag_returns_to_normal_claim_rules() {
    let (context, mut o) = fixture();
    let claim = o.flight.pilot.planet.claim.as_mut().unwrap();
    claim.owner = Some(PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: PlayerId::PLAYER_2,
        position: Vec2::new(6.0, 60.0),
        normal: Vec2::Y,
        raised_fraction: 1.0,
    });
    let mut task = GroundNavigationTask::new(context, GroundDestination::Flag);
    task.step(&o);
    assert!(!task.telemetry().path.is_empty());
    // Several small movements must accumulate against the planned destination.
    for tick in 1..=6 {
        o.flight
            .pilot
            .planet
            .claim
            .as_mut()
            .unwrap()
            .flag
            .as_mut()
            .unwrap()
            .position
            .x += 0.1;
        advance(&mut o, tick);
        task.step(&o);
    }
    assert_eq!(task.telemetry().replans, 2);
    let claim = o.flight.pilot.planet.claim.as_mut().unwrap();
    claim.owner = None;
    claim.flag = None;
    advance(&mut o, 7);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(task.telemetry().goal, GroundGoal::Arrived);
    assert!(task.telemetry().path.is_empty());
    assert_eq!(task.telemetry().target, None);
}

#[test]
fn a_cut_edge_can_replan_to_an_alternative_jump() {
    let (context, mut o) = fixture();
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    let map = o.ground.as_mut().unwrap();
    map.edges.retain(|e| e.from != 1);
    map.edges.push(GroundEdge {
        from: 0,
        to: 2,
        kind: GroundEdgeKind::Jump,
        length: 4.0,
    });
    advance(&mut o, 1);
    task.step(&o);
    assert_eq!(task.telemetry().path, vec![0, 2]);
    assert_eq!(task.telemetry().invalidations, 1);
    assert_eq!(task.telemetry().replans, 2);
}

#[test]
fn route_diagnostics_distinguish_missing_footing_from_disconnected_ground() {
    use scenario_spacewars::surface_sortie::ground_navigation::GroundRouteFailure;
    let (_, o) = fixture();
    let mut map = o.ground.unwrap();
    let start = Vec2::new(0.0, 60.0);
    let target = Vec2::new(6.0, 60.0);
    let route = map.route(start, target, 0.8);
    assert_eq!(route.path, vec![0, 1, 2, 3]);
    assert_eq!(route.diagnostics.failure, None);
    assert_eq!(route.diagnostics.length, 6.0);
    assert_eq!(
        map.route(start + Vec2::Y * 5.0, target, 0.8)
            .diagnostics
            .failure,
        Some(GroundRouteFailure::NoStartFooting)
    );
    assert_eq!(
        map.route(start, target + Vec2::Y * 5.0, 0.8)
            .diagnostics
            .failure,
        Some(GroundRouteFailure::NoDestinationFooting)
    );
    map.edges.retain(|e| e.from != 1);
    let failed = map.route(start, target, 0.8).diagnostics;
    assert_eq!(failed.failure, Some(GroundRouteFailure::Disconnected));
    assert_eq!(failed.destination_nodes, 1);
    assert_eq!(failed.reachable_nodes, 2);
    assert_eq!(failed.closest_reachable_distance, Some(4.0));
}

fn invalid_claim(o: &mut RecoveryTaskObservationV1) {
    use scenario_spacewars::surface_sortie::{
        PlanetClaimStatus, claim_footing::ClaimFootingSurvey,
    };
    let p = &mut o.flight.pilot;
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = None;
    claim.flag = None;
    claim.status = PlanetClaimStatus::NeedSupport;
    let node = &o.ground.as_ref().unwrap().nodes[1];
    o.claim_footing = Some(ClaimFootingSurvey {
        version: 1,
        owner: p.owner,
        planet: p.planet.index,
        revision: p.planet.revision,
        tick: p.tick,
        positions: vec![node.position + node.position.normalized() * 0.9],
    });
}

#[test]
fn missing_flag_anchor_relocates_but_only_real_claim_progress_finishes_it() {
    use scenario_spacewars::surface_sortie::PlanetClaimStatus;
    let (context, mut o) = fixture();
    invalid_claim(&mut o);
    let mut task = GroundNavigationTask::new(context, GroundDestination::Flag);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(task.telemetry().goal, GroundGoal::Settle);
    advance(&mut o, 30);
    o.claim_footing.as_mut().unwrap().tick = 30;
    task.step(&o);
    assert_eq!(task.telemetry().claim_relocations, 1);
    let mut copy = task.clone();
    advance(&mut o, 31);
    let walk = task.step(&o);
    assert!(walk.horizontal > 0.0);
    assert_eq!(walk, copy.step(&o));
    assert_eq!(walk, task.step(&o));
    o.flight.pilot.actor.as_mut().unwrap().position = task.telemetry().claim_target.unwrap();
    advance(&mut o, 32);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(
        task.telemetry().goal,
        GroundGoal::Settle,
        "a planned endpoint is not proof of a claim"
    );
    o.flight.pilot.planet.claim.as_mut().unwrap().status = PlanetClaimStatus::Raising;
    advance(&mut o, 33);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(task.telemetry().goal, GroundGoal::Arrived);
    assert!(task.telemetry().claim_target.is_none());
}

#[test]
fn claim_relocation_waits_for_real_support_and_invalidates_destroyed_proposals() {
    let (context, mut o) = fixture();
    invalid_claim(&mut o);
    let mut task = GroundNavigationTask::new(context, GroundDestination::Flag);
    o.flight.pilot.supported_planet = None;
    task.step(&o);
    advance(&mut o, 90);
    task.step(&o);
    assert_eq!(task.telemetry().claim_relocations, 0);
    o.flight.pilot.supported_planet = Some(o.flight.pilot.planet.index);
    advance(&mut o, 91);
    task.step(&o);
    advance(&mut o, 121);
    o.claim_footing.as_mut().unwrap().tick = 121;
    task.step(&o);
    assert!(task.telemetry().claim_target.is_some());
    o.flight.pilot.queries_ready = false;
    advance(&mut o, 122);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    o.flight.pilot.queries_ready = true;
    o.flight.pilot.planet.revision += 1;
    o.ground = None;
    o.claim_footing = None;
    advance(&mut o, 123);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert!(task.telemetry().claim_target.is_none());
    advance(&mut o, 121 + 12 * 60 + 1);
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::Blocked);
}

#[test]
fn claim_footing_rejects_stale_or_unmeasured_proposals_and_leaves_contests_alone() {
    use scenario_spacewars::surface_sortie::PlanetClaimStatus;
    for fault in 0..5 {
        let (context, mut o) = fixture();
        invalid_claim(&mut o);
        let mut task = GroundNavigationTask::new(context, GroundDestination::Flag);
        task.step(&o);
        advance(&mut o, 30);
        let s = o.claim_footing.as_mut().unwrap();
        s.tick = 30;
        match fault {
            0 => s.owner = PlayerId::PLAYER_2,
            1 => s.revision += 1,
            2 => s.tick = 0,
            3 => s.positions[0].x = f32::NAN,
            _ => s.positions[0].x += 0.1,
        }
        assert_eq!(task.step(&o), SurfaceSortieAction::default());
        assert_eq!(task.telemetry().goal, GroundGoal::Blocked);
    }
    for status in [
        PlanetClaimStatus::Ready,
        PlanetClaimStatus::Raising,
        PlanetClaimStatus::Contested,
    ] {
        let (context, mut o) = fixture();
        invalid_claim(&mut o);
        o.flight.pilot.planet.claim.as_mut().unwrap().status = status;
        let mut task = GroundNavigationTask::new(context, GroundDestination::Flag);
        task.step(&o);
        advance(&mut o, 120);
        assert_eq!(task.step(&o), SurfaceSortieAction::default());
        assert_eq!(task.telemetry().claim_relocations, 0);
    }
}

#[test]
fn claim_relocation_exhausts_four_proposals_and_airborne_wait_keeps_task_deadline() {
    let (context, mut o) = fixture();
    invalid_claim(&mut o);
    let positions: Vec<_> = o
        .ground
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .map(|n| n.position + n.position.normalized() * 0.9)
        .collect();
    o.claim_footing.as_mut().unwrap().positions = positions.clone();
    let mut task = GroundNavigationTask::new(context, GroundDestination::Flag);
    task.step(&o);
    advance(&mut o, 30);
    o.claim_footing.as_mut().unwrap().tick = 30;
    task.step(&o);
    for attempt in 0..4 {
        assert_eq!(task.telemetry().claim_relocations, attempt + 1);
        o.flight.pilot.actor.as_mut().unwrap().position = task.telemetry().claim_target.unwrap();
        let tick = 31 + u64::from(attempt) * 121;
        advance(&mut o, tick);
        o.claim_footing.as_mut().unwrap().tick = tick;
        task.step(&o);
        advance(&mut o, tick + 120);
        o.claim_footing.as_mut().unwrap().tick = tick + 120;
        task.step(&o);
    }
    assert_eq!(
        task.telemetry().reason,
        Some("four claim footing proposals failed actual support checks")
    );
    task.reset(context);
    advance(&mut o, 1000);
    o.flight.pilot.supported_planet = None;
    task.step(&o);
    advance(&mut o, 1000 + 90 * 60 + 1);
    task.step(&o);
    assert_eq!(
        task.telemetry().reason,
        Some("ground traversal exceeded ninety seconds")
    );
}

#[test]
fn hatch_route_cannot_cross_between_retained_planets() {
    use scenario_spacewars::surface_sortie::LandingPhase;
    use spacewars_ai::ground_task::ShipReturnFailure;
    let (context, mut o) = fixture();
    o.flight.pilot.landing.phase = LandingPhase::Landed;
    o.flight.pilot.landing.supported_feet = 2;
    o.flight.pilot.landing.planet = Some(o.flight.pilot.planet.index + 1);
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    o.flight.pilot.supported_planet = None;
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(task.telemetry().goal, GroundGoal::Settle);
    assert!(task.telemetry().return_failure.is_none());
    advance(&mut o, 1);
    o.flight.pilot.supported_planet = Some(o.flight.pilot.planet.index);
    assert_eq!(task.step(&o), SurfaceSortieAction::default());
    assert_eq!(
        task.telemetry().return_failure,
        Some(ShipReturnFailure::OtherPlanet)
    );
    assert_eq!(task.telemetry().goal, GroundGoal::Blocked);
    assert!(task.telemetry().path.is_empty());
    assert!(task.telemetry().target.is_none());
}

#[test]
fn a_displaced_hatch_gets_a_fresh_settling_wait_within_the_original_task_budget() {
    use spacewars_ai::ground_task::ShipReturnFailure;
    let (context, mut o) = fixture();
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    advance(&mut o, 300);
    o.flight.pilot.hatch = None;
    task.step(&o);
    advance(&mut o, 901);
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::WaitForShip);
    advance(&mut o, 1201);
    task.step(&o);
    assert_eq!(
        task.telemetry().return_failure,
        Some(ShipReturnFailure::NoGroundedHatch)
    );
    assert_eq!(task.telemetry().goal, GroundGoal::Blocked);
}

#[test]
fn a_visible_unsettled_hatch_waits_without_renewing_progress_then_reports_return_failure() {
    use scenario_spacewars::surface_sortie::{LandingPhase, TransferResult};
    use spacewars_ai::ground_task::ShipReturnFailure;
    let (context, mut o) = fixture();
    let p = &mut o.flight.pilot;
    p.actor.as_mut().unwrap().position = p.hatch.unwrap() + Vec2::Y * 0.9;
    p.landing.phase = LandingPhase::Settling;
    p.landing.supported_feet = 1;
    p.transfer = TransferResult::ShipNotSettled;
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::WaitForShip);
    let progress = task.telemetry().last_progress_tick;
    advance(&mut o, 1);
    task.step(&o);
    assert_eq!(task.telemetry().last_progress_tick, progress);
    let mut copy = task.clone();
    // World motion, a changing hatch target and brief actor contact loss must
    // not grant a new wait. No crossing is active in this return fixture.
    for tick in [300, 600, 900] {
        advance(&mut o, tick);
        o.flight.pilot.planet.motion.position.x += 10.0;
        o.flight.pilot.hatch.as_mut().unwrap().x += 11.0;
        o.flight.pilot.actor.as_mut().unwrap().position.x += 11.0;
        o.flight.pilot.supported_planet = (tick != 600).then_some(o.flight.pilot.planet.index);
        assert_eq!(task.step(&o), copy.step(&o));
        assert_eq!(task.telemetry().goal, GroundGoal::WaitForShip);
        assert_eq!(task.step(&o), SurfaceSortieAction::default());
    }
    advance(&mut o, 901);
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::Blocked);
    assert_eq!(
        task.telemetry().return_failure,
        Some(ShipReturnFailure::UnsettledShip)
    );
    assert!(task.telemetry().last_progress_tick < 901);
    assert_eq!(progress, 0);
    task.reset(context);
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::WaitForShip);
    assert!(task.telemetry().return_failure.is_none());
}

#[test]
fn a_hatch_that_settles_in_time_remains_an_ordinary_return() {
    use scenario_spacewars::surface_sortie::{LandingPhase, TransferResult};
    let (context, mut o) = fixture();
    let p = &mut o.flight.pilot;
    p.actor.as_mut().unwrap().position = p.hatch.unwrap() + Vec2::Y * 0.9;
    p.landing.phase = LandingPhase::Settling;
    p.transfer = TransferResult::ShipNotSettled;
    let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
    task.step(&o);
    advance(&mut o, 600);
    o.flight.pilot.transfer = TransferResult::Ready;
    o.flight.pilot.landing.phase = LandingPhase::Landed;
    o.flight.pilot.landing.supported_feet = 2;
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::Arrived);
    advance(&mut o, 901);
    task.step(&o);
    assert_eq!(task.telemetry().goal, GroundGoal::Arrived);
    assert!(task.telemetry().return_failure.is_none());
}
