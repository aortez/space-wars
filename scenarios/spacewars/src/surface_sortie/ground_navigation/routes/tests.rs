use super::*;
use engine_core::planning::{JobLimits, JobPoll, PlanningJob, PlanningQueue, Work};
use reference::reference_route;
use std::sync::Arc;

fn map(nodes: Vec<GroundNode>, edges: Vec<GroundEdge>) -> GroundMap {
    GroundMap {
        version: 1,
        actor: PlayerId::PLAYER_1,
        planet: 0,
        revision: 1,
        tick: 0,
        nodes,
        edges,
        rejected: vec![],
    }
}

fn node(id: u16, x: f32, y: f32) -> GroundNode {
    GroundNode {
        id,
        position: Vec2::new(x, y),
        normal: Vec2::new(x, y).normalized(),
    }
}

fn edge(from: u16, to: u16, length: f32, kind: GroundEdgeKind) -> GroundEdge {
    GroundEdge {
        from,
        to,
        length,
        kind,
    }
}

#[test]
fn ties_keep_node_id_and_original_directed_edge_order() {
    use GroundEdgeKind::{Jump, Walk};
    // Storage order differs from node IDs. Parallel edges tie in cost but
    // differ in the route diagnostics; the first edge must still win.
    let mut map = map(
        vec![
            node(3, 0.0, 64.0),
            node(2, -2.0, 62.0),
            node(0, 0.0, 60.0),
            node(1, 2.0, 62.0),
        ],
        vec![
            edge(2, 3, 1.0, Walk),
            edge(0, 2, 3.0, Walk),
            edge(0, 1, 1.0, Jump),
            edge(1, 3, 1.0, Walk),
            edge(0, 1, 3.0, Walk),
        ],
    );
    let start = Vec2::new(0.0, 60.0);
    let target = Vec2::new(0.0, 64.0);
    let routes = map.routes();
    let route = routes.route(start, target, 0.1);
    assert_eq!(route, reference_route(&map, start, target, 0.1, 0.0, false));
    assert_eq!(route.path, [0, 1, 3]);
    assert_eq!(route.diagnostics.jumps, 1);
    let returning = routes.route(target, start, 0.1);
    assert_eq!(
        returning,
        reference_route(&map, target, start, 0.1, 0.0, false)
    );
    assert_eq!(
        returning.diagnostics.failure,
        Some(GroundRouteFailure::Disconnected)
    );

    // New graph, new index: an appended measured corridor is immediately usable.
    map.edges.push(edge(3, 0, 4.0, GroundEdgeKind::Jetpack));
    let returning = map.routes().route(target, start, 0.1);
    assert_eq!(returning.path, [3, 0]);
    assert_eq!(returning.diagnostics.flights, 1);
}

#[test]
fn empty_maps_and_missing_endpoints_keep_failure_diagnostics() {
    let maps = [map(vec![], vec![]), map(vec![node(511, 0.0, 60.0)], vec![])];
    for map in maps {
        let routes = map.routes();
        for (start, target, range) in [
            (Vec2::new(0.0, 60.0), Vec2::new(2.0, 60.0), 0.1),
            (Vec2::ZERO, Vec2::new(0.0, 60.0), 0.1),
            (Vec2::new(0.0, 60.0), Vec2::new(0.0, 60.0), 0.1),
        ] {
            for partial in [false, true] {
                assert_eq!(
                    routes.route_with_height(start, target, range, 0.0, partial),
                    reference_route(&map, start, target, range, 0.0, partial),
                );
            }
        }
    }
}

#[test]
fn joint_trip_chooses_a_returnable_endpoint_and_preserves_jump_direction() {
    use GroundEdgeKind::{Jump, Walk};
    let start = Vec2::new(0.0, 60.0);
    let hatch = Vec2::new(0.0, 60.9);
    let flag = Vec2::new(0.0, 75.9);
    let mut map = map(
        vec![node(0, 0.0, 60.0), node(1, -1.0, 75.0), node(2, 1.0, 75.0)],
        vec![
            edge(0, 1, 1.0, Walk),
            edge(0, 2, 2.0, Walk),
            edge(2, 0, 2.0, Jump),
        ],
    );
    let routes = map.routes();
    let outbound = routes.route_to_actor_target(start, flag, 1.5);
    assert_eq!(outbound.path, [0, 1]);
    assert_eq!(
        routes
            .route_to_hatch(map.nodes[1].position, hatch)
            .diagnostics
            .failure,
        Some(GroundRouteFailure::Disconnected)
    );
    let joint = routes.round_trip_to_actor_target(start, flag, 1.5, hatch);
    assert_eq!(joint.endpoint.unwrap().id, 2);
    assert_eq!(joint.outbound.path, [0, 2]);
    let returning = joint.returning.unwrap();
    assert_eq!(returning.path, [2, 0]);
    assert_eq!(returning.diagnostics.jumps, 1);

    // A long but valid return from the cheapest outbound point still loses.
    map.edges.push(edge(1, 0, 100.0, Walk));
    assert_eq!(
        map.routes()
            .round_trip_to_actor_target(start, flag, 1.5, hatch)
            .endpoint
            .unwrap()
            .id,
        2
    );
    map.edges.retain(|e| e.to != 0);
    let blocked = map
        .routes()
        .round_trip_to_actor_target(start, flag, 1.5, hatch);
    assert!(blocked.endpoint.is_none() && blocked.returning.is_none());
    assert_eq!(
        blocked.outbound.diagnostics.failure,
        Some(GroundRouteFailure::Disconnected)
    );
}

#[test]
fn joint_trip_returns_to_any_boarding_node_without_using_jetpack_edges() {
    use GroundEdgeKind::{Jetpack, Walk};
    let start = Vec2::new(0.0, 60.0);
    let hatch = Vec2::new(0.0, 60.9);
    let flag = Vec2::new(0.0, 75.9);
    let map = map(
        vec![node(0, 0.0, 60.0), node(1, 0.0, 75.0), node(2, 1.5, 60.0)],
        vec![
            edge(0, 1, 15.0, Walk),
            edge(1, 2, 15.0, Walk),
            edge(1, 0, 1.0, Jetpack),
        ],
    );
    let routes = map.routes();
    let joint = routes.round_trip_to_actor_target(start, flag, 1.5, hatch);
    assert_eq!(joint.outbound.path, [0, 1]);
    assert_eq!(joint.returning.unwrap().path, [1, 2]);
    assert!(
        routes
            .round_trip_to_actor_target(start, flag, 1.5, Vec2::ZERO)
            .endpoint
            .is_none()
    );
    assert_eq!(
        routes
            .round_trip_to_actor_target(Vec2::ZERO, flag, 1.5, hatch)
            .outbound
            .diagnostics
            .failure,
        Some(GroundRouteFailure::NoStartFooting)
    );
    assert_eq!(
        routes
            .round_trip_to_actor_target(start, Vec2::ZERO, 1.5, hatch)
            .outbound
            .diagnostics
            .failure,
        Some(GroundRouteFailure::NoDestinationFooting)
    );
}

#[test]
fn two_entrances_share_one_search_and_do_not_change_the_exit() {
    let exit = Vec2::new(0.0, 60.0);
    let opposite = Vec2::new(12.0, 60.0);
    let flag = Vec2::new(6.0, 70.0);
    let map = Arc::new(map(
        vec![
            node(0, exit.x, exit.y),
            node(1, flag.x, flag.y),
            node(2, opposite.x, opposite.y),
        ],
        vec![
            edge(0, 1, 12.0, GroundEdgeKind::Walk),
            edge(1, 2, 12.0, GroundEdgeKind::Jump),
        ],
    ));
    let routes = map.routes();
    assert!(
        routes
            .round_trip_to_actor_target(exit, flag, 1.0, exit)
            .endpoint
            .is_none()
    );
    let hatches = [Some(exit), Some(opposite)];
    let expected = routes.round_trip_to_hatches(exit, flag, 1.0, hatches);
    assert_eq!(expected.outbound.path, [0, 1]);
    assert_eq!(expected.returning.as_ref().unwrap().path, [1, 2]);
    assert_eq!(routes.route_to_hatches(flag, hatches).path, [1, 2]);
    assert!(
        routes
            .route_to_hatches(flag, [None; 2])
            .diagnostics
            .failure
            .is_some()
    );
    assert!(
        routes
            .round_trip_to_hatches(opposite, flag, 1.0, hatches)
            .endpoint
            .is_none(),
        "entry does not supply an outbound edge"
    );
    for batch in [1, 7, 64] {
        let mut job = GroundRoundTripJob::with_hatches(Arc::clone(&map), exit, flag, 1.0, hatches);
        while job.next_work().is_some() {
            let before = job.work().operations;
            for _ in 0..batch {
                if job.next_work().is_none() {
                    break;
                }
                job.step();
            }
            assert!(job.work().operations - before <= batch);
        }
        assert_eq!(job.output(), Some(&expected));
        assert_eq!(job.work().indexed_edges, 2 * map.edges.len() as u64);
    }
}

#[test]
fn sparse_directed_surveys_match_complete_reference_routes() {
    let height = SurfaceSortieState::spec().half_height();
    for variant in 0..4_u16 {
        let mut nodes: Vec<_> = (0..GROUND_SAMPLES as u16)
            .filter(|&id| variant == 0 || !(id + variant).is_multiple_of(11))
            .map(|id| {
                let angle = f32::from(id) * std::f32::consts::TAU / GROUND_SAMPLES as f32;
                node(id, angle.sin() * 60.0, angle.cos() * 60.0)
            })
            .collect();
        let mut edges = vec![];
        for a in &nodes {
            for offset in [1, 6, 506, 511] {
                let id = (a.id + offset) % GROUND_SAMPLES as u16;
                let Some(b) = nodes.iter().find(|n| n.id == id) else {
                    continue;
                };
                if variant > 0 && (a.id + id + variant).is_multiple_of(7) {
                    continue;
                }
                if variant == 3 && a.id / 128 != id / 128 {
                    continue;
                }
                let kind = match (a.id + id + variant) % 13 {
                    0 => GroundEdgeKind::Jetpack,
                    2 | 4 => GroundEdgeKind::Jump,
                    _ => GroundEdgeKind::Walk,
                };
                edges.push(edge(a.id, id, a.position.distance_to(b.position), kind));
            }
        }
        nodes.rotate_left(17);
        edges.reverse();
        let map = map(nodes, edges);
        let routes = map.routes();
        let before = map.clone();
        for start in [
            map.nodes[0].position,
            map.nodes[map.nodes.len() / 3].position,
            map.nodes.last().unwrap().position,
        ] {
            for target in [start, -start, Vec2::new(120.0, 0.0), Vec2::ZERO] {
                for (range, height, partial) in [
                    (0.1, 0.0, false),
                    (4.0, height, false),
                    (0.1, height, true),
                    (4.0, height, true),
                ] {
                    assert_eq!(
                        routes.route_with_height(start, target, range, height, partial),
                        reference_route(&map, start, target, range, height, partial),
                        "variant={variant}, start={start:?}, target={target:?}, range={range}, height={height}, partial={partial}",
                    );
                }
            }
        }
        assert_eq!(map, before);
        let start = map.nodes[0].position;
        let target = -start;
        assert_eq!(
            routes.route(start, target, 4.0),
            map.route(start, target, 4.0)
        );
        assert_eq!(
            routes.route_to_hatch(start, target),
            map.route_to_hatch(start, target)
        );
        assert_eq!(
            routes.route_to_actor_target(start, target, 4.0),
            map.route_to_actor_target(start, target, 4.0)
        );
        assert_eq!(
            routes.route_toward_actor_target(start, target, 4.0),
            map.route_toward_actor_target(start, target, 4.0)
        );

        // Independent exhaustive endpoint oracle over these sparse, directed
        // graphs. The joint solver must agree on minimum *combined* cost.
        let mut walking = map.clone();
        walking.edges.retain(|e| e.kind != GroundEdgeKind::Jetpack);
        let cost = |r: &GroundRoute| r.diagnostics.length + r.diagnostics.jumps as f32 * 2.0;
        let hatch = start + start.normalized() * height;
        for target in [hatch, -hatch, Vec2::new(120.0, 0.0)] {
            let expected = walking
                .nodes
                .iter()
                .filter(|n| {
                    (n.position + n.position.normalized() * height).distance_to(target) < 4.0
                })
                .filter_map(|node| {
                    let out = reference_route(&walking, start, node.position, 0.01, 0.0, false);
                    let back = reference_route(
                        &walking,
                        node.position,
                        hatch,
                        HATCH_APPROACH_RANGE,
                        height,
                        false,
                    );
                    (out.diagnostics.failure.is_none() && back.diagnostics.failure.is_none())
                        .then(|| cost(&out) + cost(&back))
                })
                .min_by(f32::total_cmp);
            let actual = routes.round_trip_to_actor_target(start, target, 4.0, hatch);
            assert_eq!(
                actual,
                routes.reference_round_trip_to_actor_target(start, target, 4.0, hatch)
            );
            assert_eq!(expected.is_some(), actual.endpoint.is_some());
            if let Some(expected) = expected {
                let back = actual.returning.as_ref().unwrap();
                assert!((cost(&actual.outbound) + cost(back) - expected).abs() < 0.002);
                assert_eq!(actual.outbound.path.last(), back.path.first());
                for route in [&actual.outbound, back] {
                    for pair in route.path.windows(2) {
                        assert!(
                            walking
                                .edges
                                .iter()
                                .any(|e| e.from == pair[0] && e.to == pair[1])
                        );
                    }
                }
            }
        }
    }
}

fn busy_round_trip_map() -> Arc<GroundMap> {
    let nodes: Vec<_> = (0..GROUND_SAMPLES as u16)
        .map(|id| {
            let angle = f32::from(id) * std::f32::consts::TAU / GROUND_SAMPLES as f32;
            node(id, angle.sin() * 60.0, angle.cos() * 60.0)
        })
        .collect();
    let mut edges = vec![];
    for a in &nodes {
        for id in [(a.id + 1) % 512, (a.id + 511) % 512] {
            let b = nodes[usize::from(id)];
            edges.push(edge(
                a.id,
                id,
                a.position.distance_to(b.position),
                GroundEdgeKind::Walk,
            ));
        }
        if a.id != 0 {
            let kind = if a.id.is_multiple_of(7) {
                GroundEdgeKind::Jetpack
            } else {
                GroundEdgeKind::Jump
            };
            let length = a.position.distance_to(nodes[0].position);
            edges.push(edge(0, a.id, length, kind));
            edges.push(edge(a.id, 0, length, kind));
        }
    }
    edges.reverse();
    Arc::new(map(nodes, edges))
}

#[test]
fn shared_budget_resumes_real_searches_without_changing_v10_results() {
    let map = busy_round_trip_map();
    let start = map.nodes[0].position;
    let hatch = start + start.normalized() * SurfaceSortieState::spec().half_height();
    let targets = [-hatch, Vec2::ZERO, hatch];
    let expected: Vec<_> = targets
        .iter()
        .map(|&target| {
            map.routes()
                .reference_round_trip_to_actor_target(start, target, 4.0, hatch)
        })
        .collect();
    assert!(expected[0].endpoint.is_some());
    assert!(expected[1].endpoint.is_none());
    for quota in [1, 7, 64, 4096] {
        let mut queue = PlanningQueue::new(3);
        let tokens: Vec<_> = targets
            .iter()
            .enumerate()
            .map(|(actor, &target)| {
                queue
                    .submit(
                        actor as u64,
                        (map.revision, map.tick),
                        JobLimits::default(),
                        GroundRoundTripJob::new(Arc::clone(&map), start, target, 4.0, hatch),
                    )
                    .unwrap()
            })
            .collect();
        assert_eq!(queue.advance(Work::default()).charged, Work::default());
        assert!(tokens.iter().all(|&token| matches!(
            queue.poll(token, &(map.revision, map.tick)),
            JobPoll::Pending
        )));
        let mut ticks = 0;
        loop {
            let report = queue.advance(Work {
                graph: quota,
                physics_queries: 11,
            });
            ticks += 1;
            assert!(ticks < 100_000);
            assert!(report.charged.graph <= quota);
            assert_eq!(report.charged.physics_queries, 0);
            assert_eq!(
                report.charged.graph,
                report.jobs.iter().map(|j| j.charged.graph).sum::<u32>()
            );
            let mut complete = 0;
            for (i, &token) in tokens.iter().enumerate() {
                match queue.poll(token, &(map.revision, map.tick)) {
                    JobPoll::Ready(result) => {
                        assert_eq!(result, &expected[i]);
                        complete += 1;
                    }
                    JobPoll::Pending => {}
                    JobPoll::Stale => panic!("unchanged snapshot became stale"),
                }
            }
            if complete == tokens.len() {
                break;
            }
        }
    }
}

#[test]
fn high_degree_search_charges_individual_edges_and_clone_keeps_frontier() {
    let map = busy_round_trip_map();
    let start = map.nodes[0].position;
    let hatch = start + start.normalized() * SurfaceSortieState::spec().half_height();
    let mut job = GroundRoundTripJob::new(Arc::clone(&map), start, -hatch, 4.0, hatch);
    while job.work().scanned_edges < 17 {
        let before = job.work();
        job.step();
        let after = job.work();
        assert_eq!(after.operations, before.operations + 1);
        assert!(after.scanned_edges - before.scanned_edges <= 1);
        assert!(after.indexed_edges - before.indexed_edges <= 1);
        assert!(after.queue_pops - before.queue_pops <= 1);
    }
    let mut clone = job.clone();
    while job.next_work().is_some() {
        job.step();
    }
    while clone.next_work().is_some() {
        clone.step();
    }
    assert_eq!(job.output(), clone.output());
    assert_eq!(job.work(), clone.work());
    assert_eq!(
        job.output().unwrap(),
        &map.routes()
            .reference_round_trip_to_actor_target(start, -hatch, 4.0, hatch)
    );
    assert!(job.work().queue_pops >= job.work().expanded_nodes);
    let work = job.work();
    job.step();
    assert_eq!(job.work(), work);
}

#[test]
fn changed_measurements_cannot_publish_a_pending_or_ready_trip() {
    let mut map = busy_round_trip_map();
    let start = map.nodes[0].position;
    let hatch = start + start.normalized() * SurfaceSortieState::spec().half_height();
    let mut queue = PlanningQueue::new(1);
    // Include a separate overlay generation: terrain revision alone is not
    // enough when a ship moves across otherwise unchanged ground.
    let context = (map.actor, map.planet, map.revision, map.tick, 5_u64);
    let pending = queue
        .submit(
            0,
            context,
            JobLimits::default(),
            GroundRoundTripJob::new(Arc::clone(&map), start, -hatch, 4.0, hatch),
        )
        .unwrap();
    queue.advance(Work {
        graph: 17,
        physics_queries: 0,
    });
    assert_eq!(
        queue.poll(pending, &(context.0, context.1, context.2, context.3, 6)),
        JobPoll::Stale
    );
    assert_eq!(Arc::strong_count(&map), 1);
    let ready = queue
        .submit(
            0,
            context,
            JobLimits::default(),
            GroundRoundTripJob::new(Arc::clone(&map), start, -hatch, 4.0, hatch),
        )
        .unwrap();
    queue.advance(Work::UNLIMITED);
    assert!(matches!(queue.poll(ready, &context), JobPoll::Ready(_)));
    Arc::make_mut(&mut map).revision += 1;
    Arc::make_mut(&mut map).edges.clear();
    assert_eq!(
        queue.poll(
            ready,
            &(context.0, context.1, map.revision, context.3, context.4)
        ),
        JobPoll::Stale
    );
    assert_eq!(queue.advance(Work::UNLIMITED).charged, Work::default());
}
