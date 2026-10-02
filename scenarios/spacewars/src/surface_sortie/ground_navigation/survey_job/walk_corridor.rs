//! Positive walking evidence along one short arc, without a general graph search.
//! A failed hypothesis says nothing about other walks, jumps or powered routes.
use super::*;
use engine_rapier::world::QueryArea;

const MAX_STEPS: u16 = 96;
type HullCheck = Arc<dyn Fn(Vec2) -> bool + Send + Sync>;

#[derive(Clone, Copy)]
enum Phase {
    Ray(u16),
    Capsule(GroundNode, Vec2),
    Hull(GroundNode, Vec2),
    Inspect(Option<GroundNode>),
    Begin,
    WalkCapsule(bool, usize),
    WalkHull(bool, usize, Vec2),
    Floor(bool, usize),
    Advance,
    Done,
}

#[derive(Clone)]
pub(crate) struct WalkCorridorResult {
    pub outbound: GroundRouteDiagnostics,
    pub returning: GroundRouteDiagnostics,
    pub endpoint: GroundNode,
    pub areas: Vec<QueryArea>,
    pub max_rise: f32,
}

/// One real physics query, node inspection or constant-size transition per
/// operation. Both directed walks use the ordinary nine capsule/three floor
/// samples. Hull checks are separate, charged queries. No graph scan is hidden
/// in a query operation. The five-node start window and 96-edge limit are fixed.
#[derive(Clone)]
pub(crate) struct WalkCorridorJob {
    ground: GroundSurveyJob,
    hull: HullCheck,
    phase: Phase,
    start: Vec2,
    target: Vec2,
    range: f32,
    hatches: [Option<Vec2>; 2],
    start_id: u16,
    start_cursor: u16,
    nearest: Option<GroundNode>,
    previous: Option<GroundNode>,
    next: Option<GroundNode>,
    direction: i32,
    remaining: u16,
    length: f32,
    nodes: usize,
    first: Option<GroundNode>,
    nearest_target: f32,
    nearest_hatch: f32,
    hatch_nodes: usize,
    max_rise: f32,
    areas: Vec<QueryArea>,
    result: Option<WalkCorridorResult>,
    #[cfg(test)]
    path: Vec<GroundNode>,
}

impl GroundSurveyJob {
    pub(crate) fn walk_corridor(
        &self,
        start: Vec2,
        target: Vec2,
        range: f32,
        hatches: [Option<Vec2>; 2],
        hull: HullCheck,
    ) -> Option<WalkCorridorJob> {
        let id = |point: Vec2| {
            ((-point.x).atan2(point.y).rem_euclid(std::f32::consts::TAU) * GROUND_SAMPLES as f32
                / std::f32::consts::TAU)
                .round() as u16
                % GROUND_SAMPLES as u16
        };
        let start_id = id(start);
        let delta = signed_span(start_id, id(target));
        // Include the start-window displacement and two samples past the flag
        // bearing. Longer arcs remain the full survey's responsibility.
        if delta.unsigned_abs() + 4 > u32::from(MAX_STEPS) {
            return None;
        }
        let mut ground = self.clone();
        ground.measurements.footprint = Some(RefCell::new(QueryFootprint::new(
            ground.measurements.position,
            ground.measurements.angle,
        )));
        Some(WalkCorridorJob {
            ground,
            hull,
            phase: Phase::Ray(offset(start_id, -2)),
            start,
            target,
            range,
            hatches,
            start_id,
            start_cursor: 0,
            nearest: None,
            previous: None,
            next: None,
            direction: delta.signum(),
            remaining: 0,
            length: 0.0,
            nodes: 0,
            first: None,
            nearest_target: f32::INFINITY,
            nearest_hatch: f32::INFINITY,
            hatch_nodes: 0,
            max_rise: 0.0,
            areas: Vec::new(),
            result: None,
            #[cfg(test)]
            path: Vec::new(),
        })
    }
}

fn offset(id: u16, delta: i32) -> u16 {
    (i32::from(id) + delta).rem_euclid(GROUND_SAMPLES as i32) as u16
}
fn signed_span(from: u16, to: u16) -> i32 {
    let half = GROUND_SAMPLES as i32 / 2;
    (i32::from(to) - i32::from(from) + half).rem_euclid(GROUND_SAMPLES as i32) - half
}

impl WalkCorridorJob {
    pub(crate) fn take_result(&mut self) -> Option<WalkCorridorResult> {
        assert!(matches!(self.phase, Phase::Done));
        self.result.take()
    }
    fn center(node: GroundNode) -> Vec2 {
        node.position + node.position.normalized() * SurfaceSortieState::spec().half_height()
    }
    fn hatch_distance(&self, node: GroundNode) -> f32 {
        self.hatches
            .into_iter()
            .flatten()
            .map(|h| Self::center(node).distance_to(h))
            .min_by(f32::total_cmp)
            .unwrap_or(f32::INFINITY)
    }
    fn take_footprint(&mut self) -> bool {
        let measurement = &self.ground.measurements;
        let footprint = measurement.footprint.as_ref().unwrap();
        let old = footprint.replace(QueryFootprint::new(measurement.position, measurement.angle));
        self.areas.extend(old.areas.into_iter().flatten());
        old.complete
    }
    fn endpoints(&self, returning: bool) -> (GroundNode, GroundNode) {
        let pair = (self.previous.unwrap(), self.next.unwrap());
        if returning { (pair.1, pair.0) } else { pair }
    }
    fn foot(&self, returning: bool, t: f32) -> Vec2 {
        let (from, to) = self.endpoints(returning);
        from.position + (to.position - from.position) * t
    }
    fn visit(&mut self, node: GroundNode) {
        self.nodes += 1;
        self.nearest_target = self
            .nearest_target
            .min(Self::center(node).distance_to(self.target));
        self.nearest_hatch = self.nearest_hatch.min(self.hatch_distance(node));
        self.hatch_nodes += usize::from(self.hatch_distance(node) < HATCH_APPROACH_RANGE);
        #[cfg(test)]
        self.path.push(node);
        self.previous = Some(node);
        if Self::center(node).distance_to(self.target) < self.range {
            let first = self.first.unwrap();
            self.result = Some(WalkCorridorResult {
                outbound: GroundRouteDiagnostics {
                    failure: None,
                    partial: false,
                    start_node: Some(first.id),
                    start_distance: Some(first.position.distance_to(self.start)),
                    destination_nodes: 1,
                    nearest_destination_distance: Some(self.nearest_target),
                    reachable_nodes: self.nodes,
                    closest_reachable_distance: Some(self.nearest_target),
                    length: self.length,
                    jumps: 0,
                    flights: 0,
                },
                returning: GroundRouteDiagnostics {
                    failure: None,
                    partial: false,
                    start_node: Some(node.id),
                    start_distance: Some(0.0),
                    destination_nodes: self.hatch_nodes,
                    nearest_destination_distance: Some(self.nearest_hatch),
                    reachable_nodes: self.nodes,
                    closest_reachable_distance: Some(self.nearest_hatch),
                    length: self.length,
                    jumps: 0,
                    flights: 0,
                },
                endpoint: node,
                areas: std::mem::take(&mut self.areas),
                max_rise: self.max_rise,
            });
            self.phase = Phase::Done;
        } else if self.remaining == 0 {
            self.phase = Phase::Done;
        } else {
            self.remaining -= 1;
            self.phase = Phase::Ray(offset(node.id, self.direction));
        }
    }
}

impl PlanningJob for WalkCorridorJob {
    type Output = Option<WalkCorridorResult>;
    fn next_work(&self) -> Option<WorkKind> {
        match self.phase {
            Phase::Done => None,
            Phase::Inspect(_) | Phase::Begin | Phase::Advance => Some(WorkKind::Graph),
            _ => Some(WorkKind::PhysicsQuery),
        }
    }
    fn output(&self) -> Option<&Self::Output> {
        matches!(self.phase, Phase::Done).then_some(&self.result)
    }
    fn step(&mut self) {
        let spec = SurfaceSortieState::spec();
        match self.phase {
            Phase::Ray(id) => {
                let up = Vec2::Y
                    .rotate_radians(id as f32 * std::f32::consts::TAU / GROUND_SAMPLES as f32);
                let m = &self.ground.measurements;
                let world_up = up.rotate_radians(m.angle);
                self.phase = match self.ground.ray(
                    m.position + world_up * (m.radius + 8.0),
                    -world_up,
                    m.radius + 8.0,
                ) {
                    Some(hit) if hit.normal.dot(world_up) >= spec.min_support_alignment => {
                        let node = GroundNode {
                            id,
                            position: (hit.point - m.position).rotate_radians(-m.angle),
                            normal: hit.normal.rotate_radians(-m.angle),
                        };
                        Phase::Capsule(node, node.position + up * standing_height())
                    }
                    _ => Phase::Inspect(None),
                };
            }
            Phase::Capsule(node, center) => {
                self.phase = if self.ground.clear(center) {
                    Phase::Hull(node, center)
                } else {
                    Phase::Inspect(None)
                };
            }
            Phase::Hull(node, center) => {
                self.phase = Phase::Inspect((self.hull)(center).then_some(node));
            }
            Phase::Inspect(node) => {
                if self.first.is_none() {
                    if !self.take_footprint() {
                        self.phase = Phase::Done;
                        return;
                    }
                    if let Some(node) = node
                        && self.nearest.is_none_or(|old| {
                            node.position.distance_to(self.start)
                                < old.position.distance_to(self.start)
                        })
                    {
                        self.nearest = Some(node);
                    }
                    self.start_cursor += 1;
                    self.phase = if self.start_cursor == 5 {
                        Phase::Begin
                    } else {
                        Phase::Ray(offset(self.start_id, i32::from(self.start_cursor) - 2))
                    };
                } else if let Some(node) = node {
                    let previous = self.previous.unwrap();
                    let rise = (node.position - previous.position)
                        .dot((node.position + previous.position).normalized())
                        .abs();
                    let height = spec.jump_speed.powi(2) / (2.0 * self.ground.gravity);
                    if rise >= 0.3 || rise > height * 0.75 {
                        self.phase = Phase::Done;
                    } else {
                        self.max_rise = self.max_rise.max(rise);
                        self.next = Some(node);
                        self.phase = Phase::WalkCapsule(false, 0);
                    }
                } else {
                    self.phase = Phase::Done;
                }
            }
            Phase::Begin => {
                let Some(first) = self.nearest.filter(|n| {
                    n.position.distance_to(self.start) < 3.0
                        && self.hatch_distance(*n) < HATCH_APPROACH_RANGE
                }) else {
                    self.phase = Phase::Done;
                    return;
                };
                self.first = Some(first);
                // The initial angular bound already includes this displacement.
                let target_id = ((-self.target.x)
                    .atan2(self.target.y)
                    .rem_euclid(std::f32::consts::TAU)
                    * GROUND_SAMPLES as f32
                    / std::f32::consts::TAU)
                    .round() as u16
                    % GROUND_SAMPLES as u16;
                let delta = signed_span(first.id, target_id);
                self.direction = delta.signum();
                self.remaining = delta.unsigned_abs() as u16 + 2;
                assert!(self.remaining <= MAX_STEPS);
                self.visit(first);
            }
            Phase::WalkCapsule(returning, sample) => {
                let foot = self.foot(returning, sample as f32 / 8.0);
                let center = foot + foot.normalized() * standing_height();
                self.phase = if self.ground.clear(center) {
                    Phase::WalkHull(returning, sample, center)
                } else {
                    Phase::Done
                };
            }
            Phase::WalkHull(returning, sample, center) => {
                self.phase = if !(self.hull)(center) {
                    Phase::Done
                } else if sample < 8 {
                    Phase::WalkCapsule(returning, sample + 1)
                } else {
                    Phase::Floor(returning, 1)
                };
            }
            Phase::Floor(returning, sample) => {
                let foot = self.foot(returning, sample as f32 / 4.0);
                let up = foot
                    .normalized()
                    .rotate_radians(self.ground.measurements.angle);
                self.phase = if !self
                    .ground
                    .ray(self.ground.world(foot) + up * 0.4, -up, 0.75)
                    .is_some_and(|hit| hit.normal.dot(up) >= spec.min_support_alignment)
                {
                    Phase::Done
                } else if sample < 3 {
                    Phase::Floor(returning, sample + 1)
                } else if returning {
                    Phase::Advance
                } else {
                    Phase::WalkCapsule(true, 0)
                };
            }
            Phase::Advance => {
                if !self.take_footprint() {
                    self.phase = Phase::Done;
                    return;
                }
                let node = self.next.take().unwrap();
                self.length += self.previous.unwrap().position.distance_to(node.position);
                self.visit(node);
            }
            Phase::Done => {}
        }
    }
}

#[cfg(test)]
mod tests;
