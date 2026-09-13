//! Pure route queries over one immutable measured map.
use super::*;

/// Reusable node and outgoing-edge lookup for a single map snapshot.
/// The borrow prevents editing the graph while its indexes are in use. This
/// does not extend the lifetime of the physical measurements in the map.
pub struct GroundRoutes<'a> {
    map: &'a GroundMap,
    nodes: [Option<&'a GroundNode>; GROUND_SAMPLES],
    offsets: [usize; GROUND_SAMPLES + 1],
    outgoing: Vec<usize>,
}

impl<'a> GroundRoutes<'a> {
    pub(super) fn new(map: &'a GroundMap) -> Self {
        #[cfg(feature = "sensor-profile")]
        let _profile = super::super::sensor_profile::Scope::new("ground_route_index");
        #[cfg(feature = "sensor-profile")]
        let scanned_edges =
            super::super::sensor_profile::Counter::new("ground_route_index_scanned_edges");
        let mut nodes = [None; GROUND_SAMPLES];
        for node in &map.nodes {
            if let Some(slot) = nodes.get_mut(usize::from(node.id)) {
                // Preserve the former linear lookup's first matching node.
                slot.get_or_insert(node);
            }
        }
        let mut offsets = [0; GROUND_SAMPLES + 1];
        for edge in &map.edges {
            let from = usize::from(edge.from);
            if from < GROUND_SAMPLES {
                offsets[from + 1] += 1;
            }
        }
        for i in 1..=GROUND_SAMPLES {
            offsets[i] += offsets[i - 1];
        }
        let mut outgoing = vec![0; offsets[GROUND_SAMPLES]];
        let mut next = offsets;
        for (index, edge) in map.edges.iter().enumerate() {
            let from = usize::from(edge.from);
            if from < GROUND_SAMPLES {
                outgoing[next[from]] = index;
                next[from] += 1;
            }
        }
        #[cfg(feature = "sensor-profile")]
        scanned_edges.add(2 * map.edges.len());
        Self {
            map,
            nodes,
            offsets,
            outgoing,
        }
    }

    /// Bounded shortest measured route. Absence is evidence about this survey,
    /// not proof that a human cannot traverse the physical terrain.
    pub fn route(&self, start: Vec2, target: Vec2, range: f32) -> GroundRoute {
        self.route_with_height(start, target, range, 0.0, false)
    }

    /// Boarding measures the supported actor's center against the hatch. This
    /// includes nearby lower footing; climbing onto the hatch ray's hit is not
    /// required by the human transfer rule.
    pub fn route_to_hatch(&self, start: Vec2, target: Vec2) -> GroundRoute {
        self.route_to_actor_target(start, target, HATCH_APPROACH_RANGE)
    }

    /// Route using a standing-center envelope. Hatch transfer measures this
    /// center; claims still independently check the real supported flag anchor.
    pub fn route_to_actor_target(&self, start: Vec2, target: Vec2, range: f32) -> GroundRoute {
        self.route_with_height(
            start,
            target,
            range,
            SurfaceSortieState::spec().half_height(),
            false,
        )
    }

    /// Advance through measured ground when distant connections are unknown.
    /// A partial route never establishes arrival or authorizes an unmeasured edge.
    pub fn route_toward_actor_target(&self, start: Vec2, target: Vec2, range: f32) -> GroundRoute {
        self.route_with_height(
            start,
            target,
            range,
            SurfaceSortieState::spec().half_height(),
            true,
        )
    }

    fn route_with_height(
        &self,
        start: Vec2,
        target: Vec2,
        range: f32,
        height: f32,
        allow_partial: bool,
    ) -> GroundRoute {
        #[cfg(feature = "sensor-profile")]
        let _profile = super::super::sensor_profile::Scope::new("ground_route");
        #[cfg(feature = "sensor-profile")]
        let visited_nodes =
            super::super::sensor_profile::Counter::new("ground_route_visited_nodes");
        #[cfg(feature = "sensor-profile")]
        let scanned_edges =
            super::super::sensor_profile::Counter::new("ground_route_scanned_edges");
        let destination_distance = |node: &GroundNode| {
            (node.position + node.position.normalized() * height).distance_to(target)
        };
        let nearest = self.map.nodes.iter().min_by(|a, b| {
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
                destination_nodes: self
                    .map
                    .nodes
                    .iter()
                    .filter(|n| destination_distance(n) < range)
                    .count(),
                nearest_destination_distance: self
                    .map
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
        let mut parent: [Option<(u16, f32, GroundEdgeKind)>; GROUND_SAMPLES] =
            [None; GROUND_SAMPLES];
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
            #[cfg(feature = "sensor-profile")]
            visited_nodes.add(1);
            result.diagnostics.reachable_nodes += 1;
            let node = self.nodes[index].unwrap();
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
            let outgoing = &self.outgoing[self.offsets[index]..self.offsets[index + 1]];
            #[cfg(feature = "sensor-profile")]
            scanned_edges.add(outgoing.len());
            for &edge_index in outgoing {
                let edge = &self.map.edges[edge_index];
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
}

fn trace_route(
    route: &mut GroundRoute,
    parents: &[Option<(u16, f32, GroundEdgeKind)>; GROUND_SAMPLES],
    end: usize,
) {
    route.path.push(end as u16);
    let mut cursor = end;
    while let Some((previous, length, kind)) = parents[cursor] {
        route.path.push(previous);
        route.diagnostics.length += length;
        route.diagnostics.jumps += usize::from(kind == GroundEdgeKind::Jump);
        route.diagnostics.flights += usize::from(kind == GroundEdgeKind::Jetpack);
        cursor = usize::from(previous);
    }
    route.path.reverse();
}

#[cfg(test)]
mod reference;
#[cfg(test)]
mod tests;
