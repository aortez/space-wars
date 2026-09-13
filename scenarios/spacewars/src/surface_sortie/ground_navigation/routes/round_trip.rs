//! Joint walk/jump access to an interaction region and back to the hatch.
use super::*;
use std::{cmp::Ordering, collections::BinaryHeap};

#[derive(Debug, Clone, PartialEq)]
pub struct GroundRoundTrip {
    pub outbound: GroundRoute,
    pub returning: Option<GroundRoute>,
    pub endpoint: Option<GroundNode>,
}

#[derive(Clone, Copy)]
struct Entry {
    cost: f32,
    node: usize,
}
impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.node.cmp(&self.node))
    }
}

struct Distances {
    costs: [f32; GROUND_SAMPLES],
    parents: [Option<(u16, f32, GroundEdgeKind)>; GROUND_SAMPLES],
    visited: usize,
}

impl GroundRoutes<'_> {
    /// Optimize both legs together, retaining the original direction of every
    /// measured edge. Jetpack resource planning is outside this walk/jump query.
    pub fn round_trip_to_actor_target(
        &self,
        start: Vec2,
        target: Vec2,
        range: f32,
        hatch: Vec2,
    ) -> GroundRoundTrip {
        #[cfg(feature = "sensor-profile")]
        let _profile = super::super::super::sensor_profile::Scope::new("ground_round_trip");
        let height = SurfaceSortieState::spec().half_height();
        let distance = |node: &GroundNode, target: Vec2| {
            (node.position + node.position.normalized() * height).distance_to(target)
        };
        let diagnostics = |start: Vec2, target: Vec2, range: f32| {
            let nearest = self.map.nodes.iter().min_by(|a, b| {
                a.position
                    .distance_to(start)
                    .total_cmp(&b.position.distance_to(start))
            });
            GroundRouteDiagnostics {
                failure: None,
                partial: false,
                start_node: nearest.map(|n| n.id),
                start_distance: nearest.map(|n| n.position.distance_to(start)),
                destination_nodes: self
                    .map
                    .nodes
                    .iter()
                    .filter(|n| distance(n, target) < range)
                    .count(),
                nearest_destination_distance: self
                    .map
                    .nodes
                    .iter()
                    .map(|n| distance(n, target))
                    .min_by(f32::total_cmp),
                reachable_nodes: 0,
                closest_reachable_distance: None,
                length: 0.0,
                jumps: 0,
                flights: 0,
            }
        };
        let mut result = GroundRoundTrip {
            outbound: GroundRoute {
                path: vec![],
                diagnostics: diagnostics(start, target, range),
            },
            returning: None,
            endpoint: None,
        };
        if !result
            .outbound
            .diagnostics
            .start_distance
            .is_some_and(|d| d < 3.0)
        {
            result.outbound.diagnostics.failure = Some(GroundRouteFailure::NoStartFooting);
            return result;
        }
        if result.outbound.diagnostics.destination_nodes == 0 {
            result.outbound.diagnostics.failure = Some(GroundRouteFailure::NoDestinationFooting);
            return result;
        }
        let initial = usize::from(result.outbound.diagnostics.start_node.unwrap());
        let forward = self.distances([initial], &self.offsets, &self.outgoing, false);
        result.outbound.diagnostics.reachable_nodes = forward.visited;
        result.outbound.diagnostics.closest_reachable_distance = self
            .map
            .nodes
            .iter()
            .filter(|n| forward.costs[usize::from(n.id)].is_finite())
            .map(|n| distance(n, target))
            .min_by(f32::total_cmp);

        // Incoming entries preserve original edge order. A reversed search
        // follows an edge from its destination to its source, never vice versa
        // during physical execution.
        let mut offsets = [0; GROUND_SAMPLES + 1];
        for edge in &self.map.edges {
            offsets[usize::from(edge.to) + 1] += 1;
        }
        for i in 1..=GROUND_SAMPLES {
            offsets[i] += offsets[i - 1];
        }
        let mut incoming = vec![0; self.map.edges.len()];
        let mut next = offsets;
        for (i, edge) in self.map.edges.iter().enumerate() {
            let to = usize::from(edge.to);
            incoming[next[to]] = i;
            next[to] += 1;
        }
        #[cfg(feature = "sensor-profile")]
        super::super::super::sensor_profile::Counter::new("ground_round_trip_index_scanned_edges")
            .add(2 * self.map.edges.len());
        let returning = self.distances(
            self.map
                .nodes
                .iter()
                .filter(|n| distance(n, hatch) < HATCH_APPROACH_RANGE)
                .map(|n| usize::from(n.id)),
            &offsets,
            &incoming,
            true,
        );
        let endpoint = self
            .map
            .nodes
            .iter()
            .filter(|n| distance(n, target) < range)
            .filter_map(|n| {
                let id = usize::from(n.id);
                let cost = forward.costs[id] + returning.costs[id];
                cost.is_finite().then_some((n, cost))
            })
            .min_by(|(a, ac), (b, bc)| ac.total_cmp(bc).then_with(|| a.id.cmp(&b.id)))
            .map(|(node, _)| *node);
        let Some(endpoint) = endpoint else {
            result.outbound.diagnostics.failure = Some(GroundRouteFailure::Disconnected);
            return result;
        };
        trace_route(
            &mut result.outbound,
            &forward.parents,
            usize::from(endpoint.id),
        );
        let mut back = GroundRoute {
            path: vec![endpoint.id],
            diagnostics: diagnostics(endpoint.position, hatch, HATCH_APPROACH_RANGE),
        };
        back.diagnostics.reachable_nodes = returning.visited;
        back.diagnostics.closest_reachable_distance = Some(distance(&endpoint, hatch));
        let mut cursor = usize::from(endpoint.id);
        while let Some((next, length, kind)) = returning.parents[cursor] {
            back.path.push(next);
            back.diagnostics.length += length;
            back.diagnostics.jumps += usize::from(kind == GroundEdgeKind::Jump);
            cursor = usize::from(next);
            if let Some(node) = self.nodes[cursor] {
                back.diagnostics.closest_reachable_distance = Some(
                    back.diagnostics
                        .closest_reachable_distance
                        .unwrap()
                        .min(distance(node, hatch)),
                );
            }
        }
        result.endpoint = Some(endpoint);
        result.returning = Some(back);
        result
    }

    fn distances(
        &self,
        sources: impl IntoIterator<Item = usize>,
        offsets: &[usize; GROUND_SAMPLES + 1],
        edges: &[usize],
        reversed: bool,
    ) -> Distances {
        #[cfg(feature = "sensor-profile")]
        let scanned =
            super::super::super::sensor_profile::Counter::new("ground_round_trip_scanned_edges");
        #[cfg(feature = "sensor-profile")]
        let visited =
            super::super::super::sensor_profile::Counter::new("ground_round_trip_visited_nodes");
        let mut result = Distances {
            costs: [f32::INFINITY; GROUND_SAMPLES],
            parents: [None; GROUND_SAMPLES],
            visited: 0,
        };
        let mut queue = BinaryHeap::new();
        for node in sources {
            result.costs[node] = 0.0;
            queue.push(Entry { cost: 0.0, node });
        }
        while let Some(Entry { cost, node }) = queue.pop() {
            if cost != result.costs[node] {
                continue;
            }
            result.visited += 1;
            #[cfg(feature = "sensor-profile")]
            visited.add(1);
            for &index in &edges[offsets[node]..offsets[node + 1]] {
                #[cfg(feature = "sensor-profile")]
                scanned.add(1);
                let edge = &self.map.edges[index];
                if edge.kind == GroundEdgeKind::Jetpack {
                    continue;
                }
                let next = usize::from(if reversed { edge.from } else { edge.to });
                let candidate = cost
                    + edge.length
                    + if edge.kind == GroundEdgeKind::Jump {
                        2.0
                    } else {
                        0.0
                    };
                if candidate < result.costs[next] {
                    result.costs[next] = candidate;
                    result.parents[next] = Some((node as u16, edge.length, edge.kind));
                    queue.push(Entry {
                        cost: candidate,
                        node: next,
                    });
                }
            }
        }
        result
    }
}
