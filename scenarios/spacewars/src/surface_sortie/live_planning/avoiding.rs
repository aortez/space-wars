use super::*;
use ground_navigation::{
    GROUND_SAMPLES, GroundEdgeKind, GroundNode, GroundNodeRejection, GroundRejectedNode,
    standing_height,
};

pub(super) type HullPreview = Arc<dyn Fn(Vec2, f32, Vec2, f32) -> bool + Send + Sync>;
#[derive(Clone, Copy)]
enum Phase {
    Rejected(usize),
    Node(usize),
    NodeQuery(usize, Vec2),
    Edge(usize),
    Sample(usize, usize),
    EdgeQuery(usize, usize, Vec2),
    Done,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ground_navigation::GroundEdge;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn direct_samples_preserve_geometry_and_charge_every_preview_call() {
        for clearance in [1.0, 100.0] {
            let mut outputs = Vec::new();
            let mut charges = Vec::new();
            for direct in [false, true] {
                let queries = Arc::new(AtomicU64::new(0));
                let count = Arc::clone(&queries);
                let base = Arc::new(GroundMap {
                    version: 1,
                    actor: PlayerId::PLAYER_1,
                    planet: 0,
                    revision: 0,
                    tick: 0,
                    nodes: (0..2)
                        .map(|id| GroundNode {
                            id,
                            position: Vec2::new(f32::from(id), 60.0),
                            normal: Vec2::Y,
                        })
                        .collect(),
                    edges: [(0, 1), (1, 0)]
                        .map(|(from, to)| GroundEdge {
                            from,
                            to,
                            length: 1.0,
                            kind: GroundEdgeKind::Walk,
                        })
                        .to_vec(),
                    rejected: Vec::new(),
                });
                let candidate = Candidate {
                    site: None,
                    vehicle: Vec2::ZERO,
                    angle: 0.0,
                    hatch: Vec2::ZERO,
                    boarding_hatches: [None; 2],
                };
                let mut job = AvoidingJob::new(
                    base,
                    candidate,
                    Vec2::ZERO,
                    0.0,
                    clearance,
                    18.0,
                    Arc::new(move |_, _, _, _| {
                        count.fetch_add(1, Ordering::Relaxed);
                        true
                    }),
                );
                if direct {
                    job = job.with_direct_queries();
                }
                let mut work = Work::default();
                while let Some(kind) = job.next_work() {
                    match kind {
                        WorkKind::Graph => work.graph += 1,
                        WorkKind::PhysicsQuery => work.physics_queries += 1,
                    }
                    job.step();
                }
                assert_eq!(
                    u64::from(work.physics_queries),
                    queries.load(Ordering::Relaxed)
                );
                outputs.push(job.take_map());
                charges.push(work);
            }
            assert_eq!(outputs[0], outputs[1]);
            assert!(charges[1].graph < charges[0].graph);
            assert!(charges[1].physics_queries >= charges[0].physics_queries);
        }
    }
}
#[derive(Clone)]
pub(super) struct AvoidingJob {
    base: Arc<GroundMap>,
    output: GroundMap,
    nodes: [Option<GroundNode>; GROUND_SAMPLES],
    phase: Phase,
    candidate: Candidate,
    position: Vec2,
    angle: f32,
    clearance_radius: f32,
    jump_height: f32,
    preview: HullPreview,
    direct_queries: bool,
}
impl AvoidingJob {
    pub fn take_map(&mut self) -> GroundMap {
        assert!(matches!(self.phase, Phase::Done));
        self.output.take_contents()
    }
    pub fn new(
        base: Arc<GroundMap>,
        candidate: Candidate,
        position: Vec2,
        angle: f32,
        clearance_radius: f32,
        gravity: f32,
        preview: HullPreview,
    ) -> Self {
        Self {
            output: GroundMap {
                version: base.version,
                actor: base.actor,
                planet: base.planet,
                revision: base.revision,
                tick: base.tick,
                nodes: Vec::new(),
                edges: Vec::new(),
                rejected: Vec::new(),
            },
            base,
            candidate,
            position,
            angle,
            clearance_radius,
            preview,
            jump_height: SurfaceSortieState::spec().jump_speed.powi(2) / (2.0 * gravity.max(1.0)),
            nodes: [None; GROUND_SAMPLES],
            phase: Phase::Rejected(0),
            direct_queries: false,
        }
    }
    /// For a tiny patch, query each hull sample directly. This performs extra
    /// physics queries instead of a separate distance-test graph operation
    /// before each query; every preview call still consumes query allowance.
    pub fn with_direct_queries(mut self) -> Self {
        self.direct_queries = true;
        self
    }
    fn world(&self, point: Vec2) -> Vec2 {
        self.position + point.rotate_radians(self.angle)
    }
    fn clear(&self, point: Vec2) -> bool {
        (self.preview)(
            self.world(point),
            rotation_for_direction(point) + self.angle,
            self.candidate.vehicle,
            self.candidate.angle,
        )
    }
    fn keep_node(&mut self, index: usize) {
        let node = self.base.nodes[index];
        self.nodes[usize::from(node.id)] = Some(node);
        self.output.nodes.push(node);
        self.phase = Phase::Node(index + 1);
    }
    fn next_sample(&mut self, index: usize, sample: usize) {
        if sample == 8 {
            self.output.edges.push(self.base.edges[index]);
            self.phase = Phase::Edge(index + 1);
        } else {
            self.phase = Phase::Sample(index, sample + 1);
        }
    }
}
impl PlanningJob for AvoidingJob {
    type Output = GroundMap;
    fn next_work(&self) -> Option<WorkKind> {
        match self.phase {
            Phase::Done => None,
            Phase::NodeQuery(..) | Phase::EdgeQuery(..) => Some(WorkKind::PhysicsQuery),
            Phase::Sample(..) if self.direct_queries => Some(WorkKind::PhysicsQuery),
            _ => Some(WorkKind::Graph),
        }
    }
    fn output(&self) -> Option<&GroundMap> {
        matches!(self.phase, Phase::Done).then_some(&self.output)
    }
    fn step(&mut self) {
        match self.phase {
            Phase::Rejected(index) => {
                if let Some(&rejected) = self.base.rejected.get(index) {
                    self.output.rejected.push(rejected);
                    self.phase = Phase::Rejected(index + 1);
                } else {
                    self.phase = Phase::Node(0);
                }
            }
            Phase::Node(index) => {
                if let Some(node) = self.base.nodes.get(index) {
                    let point = node.position + node.position.normalized() * standing_height();
                    if self.world(point).distance_to(self.candidate.vehicle) > self.clearance_radius
                    {
                        self.keep_node(index);
                    } else {
                        self.phase = Phase::NodeQuery(index, point);
                    }
                } else {
                    self.phase = Phase::Edge(0);
                }
            }
            Phase::NodeQuery(index, point) => {
                if self.clear(point) {
                    self.keep_node(index);
                } else {
                    self.output.rejected.push(GroundRejectedNode {
                        id: self.base.nodes[index].id,
                        reason: GroundNodeRejection::ReplacementObstructed,
                    });
                    self.phase = Phase::Node(index + 1);
                }
            }
            Phase::Edge(index) => {
                if let Some(edge) = self.base.edges.get(index) {
                    self.phase = if self.nodes[usize::from(edge.from)].is_some()
                        && self.nodes[usize::from(edge.to)].is_some()
                    {
                        Phase::Sample(index, 0)
                    } else {
                        Phase::Edge(index + 1)
                    };
                } else {
                    self.phase = Phase::Done;
                }
            }
            Phase::Sample(index, sample) => {
                let edge = self.base.edges[index];
                let a = self.nodes[usize::from(edge.from)].unwrap();
                let b = self.nodes[usize::from(edge.to)].unwrap();
                let t = sample as f32 / 8.0;
                let foot = a.position + (b.position - a.position) * t;
                let point = foot
                    + foot.normalized()
                        * (standing_height()
                            + if edge.kind == GroundEdgeKind::Jump {
                                4.0 * t * (1.0 - t) * self.jump_height * 0.85
                            } else {
                                0.0
                            });
                if self.direct_queries {
                    if self.clear(point) {
                        self.next_sample(index, sample);
                    } else {
                        self.phase = Phase::Edge(index + 1);
                    }
                } else if self.world(point).distance_to(self.candidate.vehicle)
                    > self.clearance_radius
                {
                    self.next_sample(index, sample);
                } else {
                    self.phase = Phase::EdgeQuery(index, sample, point);
                }
            }
            Phase::EdgeQuery(index, sample, point) => {
                if self.clear(point) {
                    self.next_sample(index, sample);
                } else {
                    self.phase = Phase::Edge(index + 1);
                }
            }
            Phase::Done => {}
        }
    }
}
