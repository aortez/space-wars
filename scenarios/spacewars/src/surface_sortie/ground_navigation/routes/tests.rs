use super::*;
use reference::reference_route;

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
    }
}
