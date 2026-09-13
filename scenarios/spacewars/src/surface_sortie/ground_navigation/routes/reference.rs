//! Frozen route lookup from 1e8ce10, used only as a differential test oracle.
use super::*;

pub(super) fn reference_route(
    map: &GroundMap,
    start: Vec2,
    target: Vec2,
    range: f32,
    height: f32,
    allow_partial: bool,
) -> GroundRoute {
    let destination_distance = |node: &GroundNode| {
        (node.position + node.position.normalized() * height).distance_to(target)
    };
    let nearest = map.nodes.iter().min_by(|a, b| {
        a.position
            .distance_to(start)
            .total_cmp(&b.position.distance_to(start))
    });
    let mut result = GroundRoute {
        path: Vec::new(),
        diagnostics: GroundRouteDiagnostics {
            failure: None,
            partial: false,
            start_node: nearest.map(|n| n.id),
            start_distance: nearest.map(|n| n.position.distance_to(start)),
            destination_nodes: map
                .nodes
                .iter()
                .filter(|n| destination_distance(n) < range)
                .count(),
            nearest_destination_distance: map
                .nodes
                .iter()
                .map(destination_distance)
                .min_by(f32::total_cmp),
            reachable_nodes: 0,
            closest_reachable_distance: None,
            length: 0.0,
            jumps: 0,
            flights: 0,
        },
    };
    let Some(initial) = nearest.filter(|n| n.position.distance_to(start) < 3.0) else {
        result.diagnostics.failure = Some(GroundRouteFailure::NoStartFooting);
        return result;
    };
    if result.diagnostics.destination_nodes == 0 && !allow_partial {
        result.diagnostics.failure = Some(GroundRouteFailure::NoDestinationFooting);
        return result;
    }
    let mut costs = [f32::INFINITY; GROUND_SAMPLES];
    let mut parent: [Option<(u16, f32, GroundEdgeKind)>; GROUND_SAMPLES] = [None; GROUND_SAMPLES];
    let mut visited = [false; GROUND_SAMPLES];
    // Surface arc distance still measures progress near the opposite side
    // of a planet, where a useful walk barely changes straight-line distance.
    let remaining = |point: Vec2| {
        (point.x * target.y - point.y * target.x)
            .atan2(point.dot(target))
            .abs()
            * target.length()
    };
    let mut frontier = usize::from(initial.id);
    let mut frontier_distance = remaining(initial.position);
    let initial_distance = frontier_distance;
    costs[usize::from(initial.id)] = 0.0;
    for _ in 0..GROUND_SAMPLES {
        let Some(index) = (0..GROUND_SAMPLES)
            .filter(|&i| !visited[i] && costs[i].is_finite())
            .min_by(|&a, &b| costs[a].total_cmp(&costs[b]))
        else {
            break;
        };
        visited[index] = true;
        result.diagnostics.reachable_nodes += 1;
        let node = map
            .nodes
            .iter()
            .find(|n| usize::from(n.id) == index)
            .unwrap();
        let distance = destination_distance(node);
        if remaining(node.position) < frontier_distance {
            frontier = index;
            frontier_distance = remaining(node.position);
        }
        result.diagnostics.closest_reachable_distance = Some(
            result
                .diagnostics
                .closest_reachable_distance
                .map_or(distance, |old| old.min(distance)),
        );
        if distance < range {
            trace_route(&mut result, &parent, index);
            return result;
        }
        for edge in map.edges.iter().filter(|e| usize::from(e.from) == index) {
            let next = usize::from(edge.to);
            let cost = costs[index]
                + edge.length
                + match edge.kind {
                    GroundEdgeKind::Walk => 0.0,
                    GroundEdgeKind::Jump => 2.0,
                    GroundEdgeKind::Jetpack => 30.0,
                };
            if cost < costs[next] {
                costs[next] = cost;
                parent[next] = Some((index as u16, edge.length, edge.kind));
            }
        }
    }
    if allow_partial && frontier_distance < initial_distance - 1.5 {
        result.diagnostics.partial = true;
        trace_route(&mut result, &parent, frontier);
    } else {
        result.diagnostics.failure = Some(if result.diagnostics.destination_nodes == 0 {
            GroundRouteFailure::NoDestinationFooting
        } else {
            GroundRouteFailure::Disconnected
        });
    }
    result
}
