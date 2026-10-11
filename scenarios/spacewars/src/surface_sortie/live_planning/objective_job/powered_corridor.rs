//! One positive round trip: exact walking endpoints and one measured hull flight.
use super::*;
use ground_navigation::{GroundNode, GroundNodeWindowJob};

#[derive(Clone)]
enum Stage {
    Direct(Box<WalkCorridorJob>),
    Nodes(Box<GroundNodeWindowJob>),
    Proposal(Box<ProposalJob>),
    Walk(Box<WalkCorridorJob>),
    Flight(Box<FlightForecastJob>),
    Done,
}

#[derive(Clone)]
pub(super) struct PoweredCorridorJob {
    stage: Stage,
    ground: Box<GroundSurveyJob>,
    scene: FlightScene,
    candidate: Candidate,
    objective: LandingObjective,
    center: Vec2,
    angle: f32,
    radius: f32,
    hull: Arc<dyn Fn(Vec2) -> bool + Send + Sync>,
    nodes: Option<[GroundNode; 2]>,
    proposal: Option<Proposal>,
    walks: Vec<WalkCorridorResult>,
    areas: Vec<QueryArea>,
    pub flight_work: FlightForecastWork,
    pub direct_walk: Option<bool>,
    pub extended_walk: bool,
    pub failure: Option<&'static str>,
    pub result: Option<(LandingObjectiveRoute, Vec<QueryArea>, f32)>,
}
impl PoweredCorridorJob {
    pub(super) fn diagnostic_phase(&self) -> &'static str {
        match self.stage {
            Stage::Direct(_) => "direct_walk",
            Stage::Nodes(_) => "crossing_nodes",
            Stage::Proposal(_) => "crossing_proposal",
            Stage::Walk(_) => "connector_walk",
            Stage::Flight(_) => "flight_forecast",
            Stage::Done => "done",
        }
    }

    pub fn new(parent: &ObjectiveSurveyJob, candidate: Candidate) -> Option<Self> {
        let scene = parent.flight_scene.clone()?;
        if scene.measurement_tick() != parent.result.tick {
            return None;
        }
        let Phase::Ground(ground) = &parent.phase else {
            return None;
        };
        GroundSurveyJob::corridor_geometry(candidate.hatch, parent.result.objective.position, true)
            .ok()?;
        let preview = Arc::clone(&parent.preview);
        let (position, angle) = (parent.position, parent.angle);
        let hull = Arc::new(move |point: Vec2| {
            preview(
                position + point.rotate_radians(angle),
                rotation_for_direction(point) + angle,
                candidate.vehicle,
                candidate.angle,
            )
        });
        let center = (candidate.vehicle - position).rotate_radians(-angle);
        let direct = ground
            .walk_corridor(
                candidate.hatch,
                parent.result.objective.position,
                parent.result.objective.range,
                candidate.boarding_hatches,
                hull.clone(),
                true,
            )
            .ok()?;
        let extended_walk = direct.is_extended();
        Some(Self {
            stage: Stage::Direct(Box::new(direct.with_exact_endpoints(None, None))),
            ground: ground.clone(),
            scene,
            candidate,
            objective: parent.result.objective,
            center,
            angle: candidate.angle - angle,
            radius: parent.radius,
            hull,
            nodes: None,
            proposal: None,
            walks: Vec::new(),
            areas: Vec::new(),
            flight_work: FlightForecastWork::default(),
            direct_walk: None,
            extended_walk,
            failure: None,
            result: None,
        })
    }
    fn center(node: GroundNode) -> Vec2 {
        node.position + node.position.normalized() * SurfaceSortieState::spec().half_height()
    }
    fn fail(&mut self, reason: &'static str) {
        self.failure = Some(reason);
        self.stage = Stage::Done;
    }
    fn first_walk(&self) -> Option<WalkCorridorJob> {
        let entry = self.nodes?[0];
        self.ground
            .walk_corridor(
                self.candidate.hatch,
                Self::center(entry),
                0.002,
                self.candidate.boarding_hatches,
                Arc::clone(&self.hull),
                true,
            )
            .ok()
            .map(|j| j.with_exact_endpoints(None, Some(entry)))
    }
    fn second_walk(&self) -> Option<WalkCorridorJob> {
        let exit = self.nodes?[1];
        self.ground
            .walk_corridor(
                exit.position,
                self.objective.position,
                self.objective.range,
                [None; 2],
                Arc::clone(&self.hull),
                true,
            )
            .ok()
            .map(|j| j.with_exact_endpoints(Some(exit), None))
    }
    fn finish(&mut self, crossing: VehicleCrossingForecast) {
        let first = &self.walks[0];
        let second = &self.walks[1];
        let length = first.outbound.length
            + second.outbound.length
            + crossing.plan.start.distance_to(crossing.plan.destination);
        let reachable_nodes = first.outbound.reachable_nodes + second.outbound.reachable_nodes;
        let mut outbound = second.outbound.clone();
        outbound.start_node = first.outbound.start_node;
        outbound.start_distance = first.outbound.start_distance;
        outbound.length = length;
        outbound.reachable_nodes = reachable_nodes;
        outbound.flights = 1;
        let mut returning = first.returning.clone();
        returning.start_node = Some(second.endpoint.id);
        returning.start_distance = Some(0.0);
        returning.length = length;
        returning.reachable_nodes = reachable_nodes;
        returning.flights = 1;
        self.result = Some((
            LandingObjectiveRoute {
                site: self.candidate.site,
                outbound,
                returning: Some(returning),
                endpoint: Some(second.endpoint),
                crossing: Some(crossing),
            },
            std::mem::take(&mut self.areas),
            first.max_rise.max(second.max_rise),
        ));
        self.stage = Stage::Done;
    }
}
impl PlanningJob for PoweredCorridorJob {
    type Output = ();
    fn next_work(&self) -> Option<WorkKind> {
        match &self.stage {
            Stage::Direct(j) => j.next_work().or(Some(WorkKind::Graph)),
            Stage::Nodes(j) => j.next_work().or(Some(WorkKind::Graph)),
            Stage::Proposal(j) => j.next_work().or(Some(WorkKind::Graph)),
            Stage::Walk(j) => j.next_work().or(Some(WorkKind::Graph)),
            Stage::Flight(j) => j.next_work().or(Some(WorkKind::Graph)),
            Stage::Done => None,
        }
    }
    fn output(&self) -> Option<&()> {
        matches!(self.stage, Stage::Done).then_some(&())
    }
    fn step(&mut self) {
        match &mut self.stage {
            Stage::Direct(j) => {
                if j.next_work().is_some() {
                    j.step();
                    return;
                }
                if let Some(walk) = j.take_result() {
                    self.direct_walk = Some(true);
                    self.result = Some((
                        LandingObjectiveRoute {
                            site: self.candidate.site,
                            outbound: walk.outbound,
                            returning: Some(walk.returning),
                            endpoint: Some(walk.endpoint),
                            crossing: None,
                        },
                        walk.areas,
                        walk.max_rise,
                    ));
                    self.stage = Stage::Done;
                } else {
                    self.direct_walk = Some(false);
                    self.stage = Stage::Nodes(Box::new(
                        self.ground.node_window(self.center, Arc::clone(&self.hull)),
                    ));
                }
            }
            Stage::Nodes(j) => {
                if j.next_work().is_some() {
                    j.step();
                } else if let Some((map, areas)) = j.take_map() {
                    self.areas.extend(areas);
                    let map = Arc::new(map);
                    self.stage = Stage::Proposal(Box::new(self.scene.proposal(
                        Arc::clone(&map),
                        self.center,
                        self.angle,
                        self.radius,
                    )));
                } else {
                    self.fail("node_footprint");
                }
            }
            Stage::Proposal(j) => {
                if j.next_work().is_some() {
                    j.step();
                    return;
                }
                let Some(proposal) = *j.output().unwrap() else {
                    self.fail("no_crossing_proposal");
                    return;
                };
                let nodes = proposal.measured_nodes;
                // Two constant-size alternatives; never snap a route to a nearby
                // crossing node or infer missing edges from this local map.
                let selected = [nodes, [nodes[1], nodes[0]]]
                    .into_iter()
                    .filter_map(|nodes| {
                        let a = GroundSurveyJob::corridor_geometry(
                            self.candidate.hatch,
                            nodes[0].position,
                            true,
                        )
                        .ok()?
                        .1
                        .unsigned_abs();
                        let b = GroundSurveyJob::corridor_geometry(
                            nodes[1].position,
                            self.objective.position,
                            true,
                        )
                        .ok()?
                        .1
                        .unsigned_abs();
                        (a + b + 8 <= 224).then_some((a + b, nodes))
                    })
                    .min_by_key(|(steps, _)| *steps);
                let Some((_, nodes)) = selected else {
                    self.fail("walking_span");
                    return;
                };
                self.nodes = Some(nodes);
                self.proposal = Some(proposal);
                if let Some(walk) = self.first_walk() {
                    self.stage = Stage::Walk(Box::new(walk));
                } else {
                    self.fail("walking_span");
                }
            }
            Stage::Walk(j) => {
                if j.next_work().is_some() {
                    j.step();
                    return;
                }
                let Some(mut result) = j.take_result() else {
                    self.fail(if self.walks.is_empty() {
                        "hatch_to_crossing"
                    } else {
                        "crossing_to_flag"
                    });
                    return;
                };
                self.areas.append(&mut result.areas);
                self.walks.push(result);
                if self.walks.len() == 1 {
                    if let Some(walk) = self.second_walk() {
                        self.stage = Stage::Walk(Box::new(walk));
                    } else {
                        self.fail("walking_span");
                    }
                } else {
                    self.flight_work.started += 1;
                    self.stage = Stage::Flight(Box::new(
                        FlightForecastJob::new(&self.scene, self.proposal.unwrap())
                            .with_streamed_steps(),
                    ));
                }
            }
            Stage::Flight(j) => {
                if j.next_work().is_some() {
                    j.step();
                    return;
                }
                if let Some(crossing) = *j.output().unwrap() {
                    self.flight_work.approved += 1;
                    self.areas.push(j.area());
                    self.finish(crossing);
                } else {
                    let reason = j.rejection().unwrap();
                    *self.flight_work.rejected.entry(reason).or_default() += 1;
                    self.fail(reason);
                }
            }
            Stage::Done => {}
        }
    }
}
