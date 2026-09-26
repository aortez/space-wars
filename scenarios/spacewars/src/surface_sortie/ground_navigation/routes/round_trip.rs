//! Resumable joint walk/jump search over one immutable measurement.
use super::*;
use engine_core::planning::{PlanningJob, WorkKind};
use std::{cmp::Ordering, collections::BinaryHeap, sync::Arc};

#[derive(Debug, Clone, PartialEq)]
pub struct GroundRoundTrip {
    pub outbound: GroundRoute,
    pub returning: Option<GroundRoute>,
    pub endpoint: Option<GroundNode>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct GroundTripWork {
    pub operations: u64,
    pub indexed_edges: u64,
    pub queue_pops: u64,
    pub expanded_nodes: u64,
    pub scanned_edges: u64,
    pub peak_frontier: usize,
}

#[derive(Clone)]
enum Snapshot<'a> {
    Borrowed(&'a GroundMap),
    Shared(Arc<GroundMap>),
}
impl std::ops::Deref for Snapshot<'_> {
    type Target = GroundMap;
    fn deref(&self) -> &GroundMap {
        match self {
            Self::Borrowed(map) => map,
            Self::Shared(map) => map,
        }
    }
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

#[derive(Clone)]
struct Search {
    costs: [f32; GROUND_SAMPLES],
    parents: [Option<(u16, f32, GroundEdgeKind)>; GROUND_SAMPLES],
    queue: BinaryHeap<Entry>,
    active: Option<(Entry, usize, usize)>,
    visited: usize,
}
impl Search {
    fn new() -> Self {
        Self {
            costs: [f32::INFINITY; GROUND_SAMPLES],
            parents: [None; GROUND_SAMPLES],
            queue: BinaryHeap::new(),
            active: None,
            visited: 0,
        }
    }
    fn seed(&mut self, node: usize) {
        self.costs[node] = 0.0;
        self.queue.push(Entry { cost: 0.0, node });
    }
    // One queue pop or directed edge inspection. High-degree vertices and
    // obsolete heap entries cannot hide arbitrary work in one expansion.
    fn step(
        &mut self,
        map: &GroundMap,
        extra: &[Option<GroundEdge>; 2],
        offsets: &[usize; GROUND_SAMPLES + 1],
        edges: &[usize],
        reversed: bool,
        work: &mut GroundTripWork,
    ) -> bool {
        if let Some((entry, index, end)) = self.active {
            let id = edges[index];
            let edge = edge_at(map, extra, id).unwrap();
            work.scanned_edges += 1;
            self.active = (index + 1 < end).then_some((entry, index + 1, end));
            if edge.kind != GroundEdgeKind::Jetpack || id >= map.edges.len() {
                let next = usize::from(if reversed { edge.from } else { edge.to });
                let candidate = entry.cost
                    + edge.length
                    + if edge.kind == GroundEdgeKind::Jump {
                        2.0
                    } else if edge.kind == GroundEdgeKind::Jetpack {
                        30.0
                    } else {
                        0.0
                    };
                if candidate < self.costs[next] {
                    self.costs[next] = candidate;
                    self.parents[next] = Some((entry.node as u16, edge.length, edge.kind));
                    self.queue.push(Entry {
                        cost: candidate,
                        node: next,
                    });
                }
            }
        } else if let Some(entry) = self.queue.pop() {
            work.queue_pops += 1;
            if entry.cost == self.costs[entry.node] {
                self.visited += 1;
                work.expanded_nodes += 1;
                let (start, end) = (offsets[entry.node], offsets[entry.node + 1]);
                self.active = (start < end).then_some((entry, start, end));
            }
        } else {
            return true;
        }
        work.peak_frontier = work.peak_frontier.max(self.queue.len());
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Nodes(usize),
    CountEdges(usize),
    Prefix(usize),
    SparsePrefix(usize, usize, usize),
    IndexEdges(usize),
    Forward,
    ReturnSources(usize),
    Reverse,
    Endpoint(usize),
    ReturnDiagnostics(usize),
    TraceOut(usize),
    ReverseOut(usize),
    TraceBack(usize),
    Done,
}

/// Retains an immutable measurement, never live physics. Completion describes
/// that snapshot only; the adapter must revalidate dependencies and execution.
/// Each step charges a node/edge/index inspection, queue pop, path operation or
/// constant-size transition. Initial fixed 512-slot arrays/buffer reservations
/// are outside dispatch; variable index filling and path tracing are resumable.
/// Heap operations and buffer growth still depend on graph size; operation
/// quotas are not time limits and do not include allocator/scheduler overhead.
#[derive(Clone)]
pub struct GroundRoundTripJob<'a> {
    map: Snapshot<'a>,
    extra: [Option<GroundEdge>; 2],
    start: Vec2,
    target: Vec2,
    range: f32,
    hatches: [Option<Vec2>; 2],
    height: f32,
    nodes: [Option<GroundNode>; GROUND_SAMPLES],
    offsets: [usize; GROUND_SAMPLES + 1],
    reverse_offsets: [usize; GROUND_SAMPLES + 1],
    next: [usize; GROUND_SAMPLES + 1],
    reverse_next: [usize; GROUND_SAMPLES + 1],
    outgoing: Vec<usize>,
    incoming: Vec<usize>,
    forward: Search,
    backward: Search,
    best: Option<(GroundNode, f32)>,
    phase: Phase,
    result: GroundRoundTrip,
    work: GroundTripWork,
    sparse_prefix: Option<Vec<u16>>,
}
impl GroundRoundTripJob<'static> {
    pub fn new(map: Arc<GroundMap>, start: Vec2, target: Vec2, range: f32, hatch: Vec2) -> Self {
        Self::with_hatches(map, start, target, range, [Some(hatch), None])
    }

    /// A single reverse search seeded from either entrance's usable footings.
    /// Two fixed slots keep each budgeted node inspection bounded.
    pub fn with_hatches(
        map: Arc<GroundMap>,
        start: Vec2,
        target: Vec2,
        range: f32,
        hatches: [Option<Vec2>; 2],
    ) -> Self {
        Self::create(Snapshot::Shared(map), start, target, range, hatches)
    }
}
impl<'a> GroundRoundTripJob<'a> {
    fn create(
        map: Snapshot<'a>,
        start: Vec2,
        target: Vec2,
        range: f32,
        hatches: [Option<Vec2>; 2],
    ) -> Self {
        assert!(map.nodes.len() <= GROUND_SAMPLES);
        let edges = map.edges.len();
        Self {
            map,
            extra: [None; 2],
            start,
            target,
            range,
            hatches,
            height: SurfaceSortieState::spec().half_height(),
            nodes: [None; GROUND_SAMPLES],
            offsets: [0; GROUND_SAMPLES + 1],
            reverse_offsets: [0; GROUND_SAMPLES + 1],
            next: [0; GROUND_SAMPLES + 1],
            reverse_next: [0; GROUND_SAMPLES + 1],
            outgoing: Vec::with_capacity(edges),
            incoming: Vec::with_capacity(edges),
            forward: Search::new(),
            backward: Search::new(),
            best: None,
            phase: Phase::Nodes(0),
            result: GroundRoundTrip {
                outbound: GroundRoute {
                    path: vec![],
                    diagnostics: empty_diagnostics(),
                },
                returning: None,
                endpoint: None,
            },
            work: GroundTripWork::default(),
            sparse_prefix: None,
        }
    }
    /// Opt-in tiny walking patches. Preparation is bounded by 33 nodes/66
    /// edges, like the constructor's fixed arrays, outside charged indexing.
    /// Dispatch then visits each present node once instead of all 512 IDs.
    /// Existing profiles retain their original prefix work and scheduling.
    pub(crate) fn with_sparse_prefix(mut self) -> Self {
        assert!(self.map.nodes.len() <= 33 && self.map.edges.len() <= 66);
        assert!(self.extra.iter().all(Option::is_none));
        let mut ids: Vec<_> = self.map.nodes.iter().map(|n| n.id).collect();
        ids.sort_unstable();
        assert!(ids.windows(2).all(|pair| pair[0] != pair[1]));
        assert!(self.map.edges.iter().all(|e| e.kind == GroundEdgeKind::Walk
            && ids.binary_search(&e.from).is_ok()
            && ids.binary_search(&e.to).is_ok()));
        self.sparse_prefix = Some(ids);
        self
    }
    /// Exactly one explicit crossing pair; arbitrary Jetpack edges in the map
    /// remain excluded. Positive edge costs prevent repeated flights per leg.
    pub fn with_crossing(mut self, edges: [GroundEdge; 2]) -> Self {
        assert!(edges.iter().all(|e| e.kind == GroundEdgeKind::Jetpack));
        assert_eq!((edges[0].from, edges[0].to), (edges[1].to, edges[1].from));
        self.extra = edges.map(Some);
        self.outgoing.reserve(2);
        self.incoming.reserve(2);
        self
    }
    pub fn work(&self) -> GroundTripWork {
        self.work
    }
    /// One selected path node and its preceding directed edge. Constant-time
    /// access reuses the search's indexes/parents for dependency construction.
    pub(crate) fn measured_path_step(
        &self,
        returning: bool,
        index: usize,
    ) -> Option<(GroundNode, Option<(GroundNode, GroundEdgeKind)>)> {
        assert_eq!(self.phase, Phase::Done);
        let path = if returning {
            &self.result.returning.as_ref()?.path
        } else {
            &self.result.outbound.path
        };
        let id = usize::from(*path.get(index)?);
        let node = self.nodes[id]?;
        let previous = if index == 0 {
            None
        } else {
            let previous = usize::from(path[index - 1]);
            let parent = if returning {
                self.backward.parents[previous]?
            } else {
                self.forward.parents[id]?
            };
            assert_eq!(usize::from(parent.0), if returning { id } else { previous });
            Some((self.nodes[previous]?, parent.2))
        };
        Some((node, previous))
    }
    /// Compatibility path. Existing v10 callers still finish synchronously;
    /// hosts that spread work over updates must use the explicit queue API.
    pub fn finish(mut self) -> GroundRoundTrip {
        while self.next_work().is_some() {
            self.step();
        }
        // Aggregate once for synchronous observation profiling. Touching the
        // thread-local profile on every edge would distort the measured cost.
        #[cfg(feature = "sensor-profile")]
        if self.work.queue_pops > 0 {
            use super::super::super::sensor_profile::Counter;
            Counter::new("ground_round_trip_scanned_edges").add(self.work.scanned_edges as usize);
            Counter::new("ground_round_trip_visited_nodes").add(self.work.expanded_nodes as usize);
            Counter::new("ground_round_trip_index_scanned_edges")
                .add(self.work.indexed_edges as usize);
        }
        self.result
    }
    fn fail(&mut self, failure: GroundRouteFailure) {
        self.result.outbound.diagnostics.failure = Some(failure);
        self.phase = Phase::Done;
    }
    fn hatch_distance(&self, node: GroundNode) -> f32 {
        self.hatches
            .into_iter()
            .flatten()
            .map(|h| self.distance(node, h))
            .min_by(f32::total_cmp)
            .unwrap_or(f32::INFINITY)
    }
    fn distance(&self, node: GroundNode, target: Vec2) -> f32 {
        (node.position + node.position.normalized() * self.height).distance_to(target)
    }
}

#[cfg(test)]
mod sparse_tests {
    use super::*;

    #[test]
    fn sparse_prefix_preserves_complete_and_failed_trips_with_wraparound_ids() {
        for count in [0, 1, 2, 17, 33] {
            for one_way in [false, true] {
                for gap in [false, true] {
                    let mut nodes: Vec<_> = (0..count)
                        .map(|i| GroundNode {
                            id: (504 + i) % GROUND_SAMPLES as u16,
                            position: Vec2::new(f32::from(i), 60.0),
                            normal: Vec2::Y,
                        })
                        .collect();
                    let mut edges = Vec::new();
                    for (i, pair) in nodes.windows(2).enumerate() {
                        if gap && i == usize::from(count / 2) {
                            continue;
                        }
                        edges.push(GroundEdge {
                            from: pair[0].id,
                            to: pair[1].id,
                            kind: GroundEdgeKind::Walk,
                            length: 1.0,
                        });
                        if !one_way {
                            edges.push(GroundEdge {
                                from: pair[1].id,
                                to: pair[0].id,
                                kind: GroundEdgeKind::Walk,
                                length: 1.0,
                            });
                        }
                    }
                    nodes.reverse();
                    let map = Arc::new(GroundMap {
                        version: 1,
                        actor: PlayerId::PLAYER_1,
                        planet: 0,
                        revision: 0,
                        tick: 0,
                        nodes,
                        edges,
                        rejected: vec![],
                    });
                    let start = Vec2::new(0.0, 60.0);
                    let end = Vec2::new(f32::from(count.saturating_sub(1)), 60.5);
                    let mut dense = GroundRoundTripJob::new(map, start, end, 1.0, start);
                    let mut sparse = dense.clone().with_sparse_prefix();
                    for job in [&mut dense, &mut sparse] {
                        while job.next_work().is_some() {
                            job.step();
                        }
                    }
                    assert_eq!(
                        dense.output(),
                        sparse.output(),
                        "nodes={count}, directed={one_way}, gap={gap}"
                    );
                    assert!(sparse.work().operations <= dense.work().operations);
                    if dense.output().unwrap().endpoint.is_some() {
                        assert_eq!(
                            dense.work().operations - sparse.work().operations,
                            512 - u64::from(count)
                        );
                    }
                }
            }
        }
    }
}
fn empty_diagnostics() -> GroundRouteDiagnostics {
    GroundRouteDiagnostics {
        failure: None,
        partial: false,
        start_node: None,
        start_distance: None,
        destination_nodes: 0,
        nearest_destination_distance: None,
        reachable_nodes: 0,
        closest_reachable_distance: None,
        length: 0.0,
        jumps: 0,
        flights: 0,
    }
}
fn measure_node(
    d: &mut GroundRouteDiagnostics,
    node: GroundNode,
    start: Vec2,
    distance: f32,
    range: f32,
) {
    let from_start = node.position.distance_to(start);
    if d.start_distance
        .is_none_or(|old| from_start.total_cmp(&old).is_lt())
    {
        d.start_node = Some(node.id);
        d.start_distance = Some(from_start);
    }
    d.destination_nodes += usize::from(distance < range);
    if d.nearest_destination_distance
        .is_none_or(|old| distance.total_cmp(&old).is_lt())
    {
        d.nearest_destination_distance = Some(distance);
    }
}
impl PlanningJob for GroundRoundTripJob<'_> {
    type Output = GroundRoundTrip;
    fn next_work(&self) -> Option<WorkKind> {
        (self.phase != Phase::Done).then_some(WorkKind::Graph)
    }
    fn output(&self) -> Option<&GroundRoundTrip> {
        (self.phase == Phase::Done).then_some(&self.result)
    }
    fn step(&mut self) {
        if self.phase == Phase::Done {
            return;
        }
        self.work.operations += 1;
        match self.phase {
            Phase::Nodes(i) => {
                if let Some(&node) = self.map.nodes.get(i) {
                    self.nodes[usize::from(node.id)].get_or_insert(node);
                    let distance = self.distance(node, self.target);
                    measure_node(
                        &mut self.result.outbound.diagnostics,
                        node,
                        self.start,
                        distance,
                        self.range,
                    );
                    self.phase = Phase::Nodes(i + 1);
                } else if !self
                    .result
                    .outbound
                    .diagnostics
                    .start_distance
                    .is_some_and(|d| d < 3.0)
                {
                    self.fail(GroundRouteFailure::NoStartFooting);
                } else if self.result.outbound.diagnostics.destination_nodes == 0 {
                    self.fail(GroundRouteFailure::NoDestinationFooting);
                } else {
                    self.forward.seed(usize::from(
                        self.result.outbound.diagnostics.start_node.unwrap(),
                    ));
                    self.work.peak_frontier = 1;
                    self.phase = Phase::CountEdges(0);
                }
            }
            Phase::CountEdges(i) => {
                if let Some(edge) = edge_at(&self.map, &self.extra, i) {
                    assert!(edge.length.is_finite() && edge.length >= 0.0);
                    self.offsets[usize::from(edge.from) + 1] += 1;
                    self.reverse_offsets[usize::from(edge.to) + 1] += 1;
                    self.outgoing.push(0);
                    self.incoming.push(0);
                    self.work.indexed_edges += 1;
                    self.phase = Phase::CountEdges(i + 1);
                } else {
                    self.phase = if self.sparse_prefix.is_some() {
                        Phase::SparsePrefix(0, 0, 0)
                    } else {
                        Phase::Prefix(1)
                    };
                }
            }
            Phase::Prefix(i) => {
                if i <= GROUND_SAMPLES {
                    self.offsets[i] += self.offsets[i - 1];
                    self.reverse_offsets[i] += self.reverse_offsets[i - 1];
                    self.next[i] = self.offsets[i];
                    self.reverse_next[i] = self.reverse_offsets[i];
                    self.phase = Phase::Prefix(i + 1);
                } else {
                    self.phase = Phase::IndexEdges(0);
                }
            }
            Phase::SparsePrefix(i, forward, backward) => {
                if let Some(&id) = self.sparse_prefix.as_ref().unwrap().get(i) {
                    let id = usize::from(id);
                    let end = forward + self.offsets[id + 1];
                    let reverse_end = backward + self.reverse_offsets[id + 1];
                    self.offsets[id] = forward;
                    self.offsets[id + 1] = end;
                    self.reverse_offsets[id] = backward;
                    self.reverse_offsets[id + 1] = reverse_end;
                    self.next[id] = forward;
                    self.reverse_next[id] = backward;
                    self.phase = Phase::SparsePrefix(i + 1, end, reverse_end);
                } else {
                    self.phase = Phase::IndexEdges(0);
                }
            }
            Phase::IndexEdges(i) => {
                if let Some(edge) = edge_at(&self.map, &self.extra, i) {
                    let from = usize::from(edge.from);
                    let to = usize::from(edge.to);
                    self.outgoing[self.next[from]] = i;
                    self.next[from] += 1;
                    self.incoming[self.reverse_next[to]] = i;
                    self.reverse_next[to] += 1;
                    self.work.indexed_edges += 1;
                    self.phase = Phase::IndexEdges(i + 1);
                } else {
                    self.phase = Phase::Forward;
                }
            }
            Phase::Forward => {
                if self.forward.step(
                    &self.map,
                    &self.extra,
                    &self.offsets,
                    &self.outgoing,
                    false,
                    &mut self.work,
                ) {
                    self.result.outbound.diagnostics.reachable_nodes = self.forward.visited;
                    self.phase = Phase::ReturnSources(0);
                }
            }
            Phase::ReturnSources(i) => {
                if let Some(&node) = self.map.nodes.get(i) {
                    if self.hatch_distance(node) < HATCH_APPROACH_RANGE {
                        self.backward.seed(usize::from(node.id));
                        self.work.peak_frontier =
                            self.work.peak_frontier.max(self.backward.queue.len());
                    }
                    self.phase = Phase::ReturnSources(i + 1);
                } else {
                    self.phase = Phase::Reverse;
                }
            }
            Phase::Reverse => {
                if self.backward.step(
                    &self.map,
                    &self.extra,
                    &self.reverse_offsets,
                    &self.incoming,
                    true,
                    &mut self.work,
                ) {
                    self.phase = Phase::Endpoint(0);
                }
            }
            Phase::Endpoint(i) => {
                if let Some(&node) = self.map.nodes.get(i) {
                    let id = usize::from(node.id);
                    let distance = self.distance(node, self.target);
                    if self.forward.costs[id].is_finite() {
                        let d = &mut self.result.outbound.diagnostics.closest_reachable_distance;
                        if d.is_none_or(|old| distance.total_cmp(&old).is_lt()) {
                            *d = Some(distance);
                        }
                    }
                    let cost = self.forward.costs[id] + self.backward.costs[id];
                    if distance < self.range
                        && cost.is_finite()
                        && self.best.is_none_or(|(best, old)| {
                            cost.total_cmp(&old)
                                .then_with(|| node.id.cmp(&best.id))
                                .is_lt()
                        })
                    {
                        self.best = Some((node, cost));
                    }
                    self.phase = Phase::Endpoint(i + 1);
                } else if let Some((node, _)) = self.best {
                    self.result.endpoint = Some(node);
                    self.result.returning = Some(GroundRoute {
                        path: vec![node.id],
                        diagnostics: empty_diagnostics(),
                    });
                    self.phase = Phase::ReturnDiagnostics(0);
                } else {
                    self.fail(GroundRouteFailure::Disconnected);
                }
            }
            Phase::ReturnDiagnostics(i) => {
                let endpoint = self.result.endpoint.unwrap();
                if let Some(&node) = self.map.nodes.get(i) {
                    let distance = self.hatch_distance(node);
                    measure_node(
                        &mut self.result.returning.as_mut().unwrap().diagnostics,
                        node,
                        endpoint.position,
                        distance,
                        HATCH_APPROACH_RANGE,
                    );
                    self.phase = Phase::ReturnDiagnostics(i + 1);
                } else {
                    let distance = self.hatch_distance(endpoint);
                    let d = &mut self.result.returning.as_mut().unwrap().diagnostics;
                    d.reachable_nodes = self.backward.visited;
                    d.closest_reachable_distance = Some(distance);
                    self.result.outbound.path.push(endpoint.id);
                    self.phase = Phase::TraceOut(usize::from(endpoint.id));
                }
            }
            Phase::TraceOut(cursor) => {
                if let Some((previous, length, kind)) = self.forward.parents[cursor] {
                    self.result.outbound.path.push(previous);
                    self.result.outbound.diagnostics.length += length;
                    self.result.outbound.diagnostics.jumps +=
                        usize::from(kind == GroundEdgeKind::Jump);
                    self.result.outbound.diagnostics.flights +=
                        usize::from(kind == GroundEdgeKind::Jetpack);
                    self.phase = Phase::TraceOut(usize::from(previous));
                } else {
                    self.phase = Phase::ReverseOut(0);
                }
            }
            Phase::ReverseOut(i) => {
                let path = &mut self.result.outbound.path;
                if i < path.len() / 2 {
                    let end = path.len() - 1 - i;
                    path.swap(i, end);
                    self.phase = Phase::ReverseOut(i + 1);
                } else {
                    self.phase = Phase::TraceBack(usize::from(self.result.endpoint.unwrap().id));
                }
            }
            Phase::TraceBack(cursor) => {
                if let Some((next, length, kind)) = self.backward.parents[cursor] {
                    let distance =
                        self.nodes[usize::from(next)].map(|node| self.hatch_distance(node));
                    let back = self.result.returning.as_mut().unwrap();
                    back.path.push(next);
                    back.diagnostics.length += length;
                    back.diagnostics.jumps += usize::from(kind == GroundEdgeKind::Jump);
                    back.diagnostics.flights += usize::from(kind == GroundEdgeKind::Jetpack);
                    if let Some(distance) = distance {
                        back.diagnostics.closest_reachable_distance = Some(
                            back.diagnostics
                                .closest_reachable_distance
                                .unwrap()
                                .min(distance),
                        );
                    }
                    self.phase = Phase::TraceBack(usize::from(next));
                } else {
                    self.phase = Phase::Done;
                }
            }
            Phase::Done => {}
        }
    }
}

impl GroundRoutes<'_> {
    /// Retains v10's synchronous behavior while sharing the resumable solver.
    pub fn round_trip_to_actor_target(
        &self,
        start: Vec2,
        target: Vec2,
        range: f32,
        hatch: Vec2,
    ) -> GroundRoundTrip {
        self.round_trip_to_hatches(start, target, range, [Some(hatch), None])
    }

    pub fn round_trip_to_hatches(
        &self,
        start: Vec2,
        target: Vec2,
        range: f32,
        hatches: [Option<Vec2>; 2],
    ) -> GroundRoundTrip {
        GroundRoundTripJob::create(Snapshot::Borrowed(self.map), start, target, range, hatches)
            .finish()
    }
}

fn edge_at(map: &GroundMap, extra: &[Option<GroundEdge>; 2], i: usize) -> Option<GroundEdge> {
    if i < map.edges.len() {
        Some(map.edges[i])
    } else {
        extra.get(i - map.edges.len()).copied().flatten()
    }
}
impl GroundRoutes<'_> {
    pub fn round_trip_with_crossing(
        &self,
        start: Vec2,
        target: Vec2,
        range: f32,
        hatches: [Option<Vec2>; 2],
        edges: [GroundEdge; 2],
    ) -> GroundRoundTrip {
        GroundRoundTripJob::create(Snapshot::Borrowed(self.map), start, target, range, hatches)
            .with_crossing(edges)
            .finish()
    }
}
