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
        }
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
                if self.world(point).distance_to(self.candidate.vehicle) > self.clearance_radius {
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
