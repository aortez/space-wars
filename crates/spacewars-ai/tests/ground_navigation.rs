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
    if let Some(map) = &mut o.ground {
        map.tick = tick;
    }
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
