use super::*;
use jetpack::forecast::{
    FlightForecastJob, FlightScene, Proposal, ProposalJob, VehicleCrossingForecast,
};

#[derive(Clone)]
enum Phase {
    Ground(Box<GroundSurveyJob>),
    Avoid(Box<AvoidingJob>),
    Trip(Box<GroundRoundTripJob<'static>>),
    Dependencies(Box<RouteDependenciesJob>),
    Proposal(Box<ProposalJob>),
    Flight(Box<FlightForecastJob>),
    Done,
}
#[derive(Clone)]
pub(crate) struct ObjectiveSurveyJob {
    phase: Phase,
    flight_scene: Option<FlightScene>,
    flight_dependent: bool,
    flight_work: FlightForecastWork,
    measurement_work: ObjectiveMeasurementWork,
    candidate_map: Option<Arc<GroundMap>>,
    proposal: Option<Proposal>,
    crossing: Option<VehicleCrossingForecast>,
    saved_trip: Option<Box<GroundRoundTripJob<'static>>>,
    walking_failure: Option<LandingObjectiveRoute>,
    flight_area: Option<QueryArea>,
    base: Option<Arc<GroundMap>>,
    measurements: Option<Box<GroundMeasurements>>,
    reused: ReusedGroundWork,
    local_dependencies: bool,
    radius: f32,
    dependencies: Vec<(Option<LandingSiteId>, Vec<QueryArea>)>,
    candidates: Vec<Candidate>,
    index: usize,
    position: Vec2,
    angle: f32,
    gravity: f32,
    clearance_radius: f32,
    preview: HullPreview,
    result: LandingObjectiveSurvey,
}
impl SurfaceSortieState {
    #[cfg(test)]
    pub(crate) fn objective_job(
        &self,
        player: usize,
        p: &PilotObservationV1,
        cover: &[combat::LandingCover],
        snapshot: Arc<QuerySnapshot>,
        measurements: Option<Box<GroundMeasurements>>,
        local_dependencies: bool,
    ) -> Option<ObjectiveSurveyJob> {
        self.objective_job_with_planning(
            player,
            p,
            cover,
            snapshot,
            measurements,
            local_dependencies,
            ObjectivePlanning::JointRoundTrip,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn objective_job_with_planning(
        &self,
        player: usize,
        p: &PilotObservationV1,
        cover: &[combat::LandingCover],
        snapshot: Arc<QuerySnapshot>,
        measurements: Option<Box<GroundMeasurements>>,
        local_dependencies: bool,
        planning: ObjectivePlanning,
    ) -> Option<ObjectiveSurveyJob> {
        let flight_scene = if planning == ObjectivePlanning::JetpackRoundTrip {
            let (position, angle) = measurements
                .as_ref()
                .map_or((p.planet.motion.position, p.planet.motion.angle), |m| {
                    (m.position, m.angle)
                });
            FlightScene::read(self, player, p, Arc::clone(&snapshot), position, angle)
        } else {
            None
        };
        let objective = LandingObjective::read(p)?;
        if !p.queries_ready
            || !matches!(p.location, PilotLocation::Aboard(_))
            || p.ship_form != ShipForm::Ship
            || !p.ship_available
        {
            return None;
        }
        let flag =
            p.planet.motion.position + objective.position.rotate_radians(p.planet.motion.angle);
        let gravity = self.objective_gravity(p);
        let ground = if let Some(measurements) = measurements {
            GroundSurveyJob::from_measurements(measurements, gravity)
        } else {
            self.ground_survey_job(player, p.planet.index, gravity, snapshot)?
        };
        let measurement_tick = ground.measurement_tick();
        let ship = self.replacement_ship(player);
        let spec = Self::spec();
        let mut ordered: Vec<_> = p.sites.iter().collect();
        ordered.sort_by(|a, b| {
            a.hatch_position
                .distance_to(flag)
                .total_cmp(&b.hatch_position.distance_to(flag))
        });
        let mut shortlisted: Vec<_> = ordered
            .iter()
            .copied()
            .take(landing_objective::MAX_OBJECTIVE_SITES / 2)
            .collect();
        for candidate in ordered
            .iter()
            .copied()
            .filter(|site| {
                cover
                    .iter()
                    .any(|c| c.site == site.id && c.grounded && c.approach && c.departure)
            })
            .chain(ordered.iter().copied())
        {
            if shortlisted.len() == landing_objective::MAX_OBJECTIVE_SITES {
                break;
            }
            if !shortlisted.iter().any(|s| s.id == candidate.id) {
                shortlisted.push(candidate);
            }
        }
        let local =
            |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let mut candidates: Vec<_> = shortlisted
            .into_iter()
            .map(|site| Candidate {
                site: Some(site.id),
                vehicle: site.vehicle_position,
                angle: rotation_for_direction(site.normal),
                hatch: local(site.hatch_position),
                boarding_hatches: site.boarding_hatches.map(|h| h.map(local)),
            })
            .collect();
        if p.landing.phase == LandingPhase::Landed
            && let Some(hatch) = p.hatch
        {
            candidates.push(Candidate {
                site: None,
                vehicle: p.ship.position,
                angle: p.ship.angle,
                hatch: local(hatch),
                boarding_hatches: p.boarding_hatches.map(|h| h.map(local)),
            });
        }
        if candidates.is_empty() {
            return None;
        }
        Some(ObjectiveSurveyJob {
            phase: Phase::Ground(Box::new(ground)),
            flight_scene,
            flight_dependent: false,
            flight_work: FlightForecastWork::default(),
            measurement_work: ObjectiveMeasurementWork::default(),
            candidate_map: None,
            proposal: None,
            crossing: None,
            saved_trip: None,
            walking_failure: None,
            flight_area: None,
            base: None,
            measurements: None,
            reused: ReusedGroundWork::default(),
            local_dependencies,
            radius: p.planet.radius,
            dependencies: Vec::new(),
            candidates,
            index: 0,
            position: p.planet.motion.position,
            angle: p.planet.motion.angle,
            gravity,
            clearance_radius: physics::SpacewarsPhysics::surface_vehicle_clearance_radius(&ship)
                + spec.half_height()
                + 0.02,
            preview: Arc::new(self.world.physics.surface_vehicle_capsule_clearance(
                self.pilots[player].vehicle.0,
                &ship,
                spec.half_segment,
                spec.radius + 0.02,
            )),
            result: LandingObjectiveSurvey {
                planning,
                version: 1,
                actor: p.owner,
                tick: measurement_tick,
                validated_tick: None,
                validated_routes_only: false,
                objective,
                sites: Vec::new(),
                actual: None,
            },
        })
    }

    pub(crate) fn objective_gravity(&self, p: &PilotObservationV1) -> f32 {
        let Some(objective) = LandingObjective::read(p) else {
            return 0.0;
        };
        let flag =
            p.planet.motion.position + objective.position.rotate_radians(p.planet.motion.angle);
        self.world
            .planets
            .iter()
            .map(|body| compatibility::source_acceleration(body.position, body.mass, flag).length())
            .sum::<f32>()
            + self.world.sun.map_or(0.0, |sun| {
                compatibility::source_acceleration(sun.position, sun.mass, flag).length()
            })
    }
}
impl ObjectiveSurveyJob {
    pub(crate) fn measurement_work(&self) -> &ObjectiveMeasurementWork {
        &self.measurement_work
    }
    pub(crate) fn flight_work(&self) -> &FlightForecastWork {
        &self.flight_work
    }
    pub(crate) fn uses_flight_environment(&self) -> bool {
        self.flight_dependent
    }
    pub(crate) fn dependencies(&self) -> &[(Option<LandingSiteId>, Vec<QueryArea>)] {
        &self.dependencies
    }
    pub(crate) fn reused(&self) -> ReusedGroundWork {
        match &self.phase {
            Phase::Ground(job) => job.reused(),
            _ => self.reused,
        }
    }
    pub(crate) fn into_measurements(self) -> Box<GroundMeasurements> {
        match self.phase {
            Phase::Ground(job) => job.into_measurements(),
            _ => self.measurements.unwrap(),
        }
    }
    fn avoid(&self) -> Phase {
        Phase::Avoid(Box::new(AvoidingJob::new(
            Arc::clone(self.base.as_ref().unwrap()),
            self.candidates[self.index],
            self.position,
            self.angle,
            self.clearance_radius,
            self.gravity,
            Arc::clone(&self.preview),
        )))
    }
    fn finish_route(&mut self, route: LandingObjectiveRoute) {
        self.measurement_work.record(&route);
        if route.site.is_some() {
            self.result.sites.push(route);
        } else {
            self.result.actual = Some(route);
        }
        self.candidate_map = None;
        self.proposal = None;
        self.crossing = None;
        self.saved_trip = None;
        self.walking_failure = None;
        self.flight_area = None;
        self.index += 1;
        self.phase = if self.index < self.candidates.len() {
            self.avoid()
        } else {
            self.measurement_work.finished_surveys += 1;
            Phase::Done
        };
    }

    // Boarding floor/clearance probes are distinct from the last path node:
    // a route may stop within boarding range rather than at the probe itself.
    // Include both bounded probe corridors in positive-result validation.
    fn entrance_dependencies(&self) -> [QueryArea; 2] {
        let c = self.candidates[self.index];
        let local = |point: Vec2| (point - self.position).rotate_radians(-self.angle);
        let up = (c.vehicle - self.position).normalized();
        let right = Vec2::new(up.y, -up.x);
        let margin = SurfaceSortieState::spec().half_height() * 2.0 + 0.2;
        [0, 1].map(|side| {
            let hatch = c.vehicle + hatch_offset(ShipForm::Ship, side).rotate_radians(c.angle);
            let corners = [-1.0, 1.0].map(|offset| {
                [
                    local(hatch + right * offset + up * 2.0),
                    local(hatch + right * offset - up * 3.0),
                ]
            });
            let mut minimum = corners[0][0];
            let mut maximum = minimum;
            for point in corners.into_iter().flatten() {
                minimum.x = minimum.x.min(point.x);
                minimum.y = minimum.y.min(point.y);
                maximum.x = maximum.x.max(point.x);
                maximum.y = maximum.y.max(point.y);
            }
            QueryArea {
                minimum: minimum - Vec2::new(margin, margin),
                maximum: maximum + Vec2::new(margin, margin),
                groups: SurfaceSortieState::spec().collision_groups,
            }
        })
    }
}
impl PlanningJob for ObjectiveSurveyJob {
    type Output = LandingObjectiveSurvey;
    fn next_work(&self) -> Option<WorkKind> {
        match &self.phase {
            Phase::Ground(j) => j.next_work().or(Some(WorkKind::Graph)),
            Phase::Avoid(j) => j.next_work().or(Some(WorkKind::Graph)),
            Phase::Trip(j) => j.next_work().or(Some(WorkKind::Graph)),
            Phase::Dependencies(j) => j.next_work().or(Some(WorkKind::Graph)),
            Phase::Proposal(j) => j.next_work().or(Some(WorkKind::Graph)),
            Phase::Flight(j) => j.next_work().or(Some(WorkKind::Graph)),
            Phase::Done => None,
        }
    }
    fn output(&self) -> Option<&LandingObjectiveSurvey> {
        matches!(self.phase, Phase::Done).then_some(&self.result)
    }
    fn step(&mut self) {
        match &mut self.phase {
            Phase::Ground(j) => {
                if j.next_work().is_some() {
                    j.step();
                } else {
                    self.base = Some(Arc::new(j.take_map()));
                    self.reused = j.reused();
                    if let Phase::Ground(job) = std::mem::replace(&mut self.phase, Phase::Done) {
                        self.measurements = Some(job.into_measurements());
                    }
                    self.phase = self.avoid();
                }
            }
            Phase::Avoid(j) => {
                if j.next_work().is_some() {
                    j.step();
                } else {
                    let map = Arc::new(j.take_map());
                    self.candidate_map = Some(Arc::clone(&map));
                    let hatch = self.candidates[self.index].hatch;
                    self.phase = Phase::Trip(Box::new(GroundRoundTripJob::with_hatches(
                        map,
                        hatch,
                        self.result.objective.position,
                        self.result.objective.range,
                        self.candidates[self.index].boarding_hatches,
                    )));
                }
            }
            Phase::Trip(j) => {
                if j.next_work().is_some() {
                    j.step();
                } else {
                    let result = j.output().unwrap();
                    if result.endpoint.is_none()
                        && self.flight_scene.is_some()
                        && self.proposal.is_none()
                    {
                        self.walking_failure = Some(LandingObjectiveRoute {
                            site: self.candidates[self.index].site,
                            outbound: result.outbound.diagnostics.clone(),
                            returning: result.returning.as_ref().map(|r| r.diagnostics.clone()),
                            endpoint: None,
                            crossing: None,
                        });
                        let candidate = self.candidates[self.index];
                        self.phase = Phase::Proposal(Box::new(
                            self.flight_scene.as_ref().unwrap().proposal(
                                Arc::clone(self.candidate_map.as_ref().unwrap()),
                                (candidate.vehicle - self.position).rotate_radians(-self.angle),
                                candidate.angle - self.angle,
                                self.radius,
                            ),
                        ));
                        return;
                    }
                    if self.proposal.is_some() && self.crossing.is_none() {
                        if result.endpoint.is_none() {
                            let failure = self.walking_failure.take().unwrap();
                            self.finish_route(failure);
                        } else {
                            if let Phase::Trip(trip) =
                                std::mem::replace(&mut self.phase, Phase::Done)
                            {
                                self.saved_trip = Some(trip);
                            }
                            self.flight_dependent = true;
                            self.flight_work.started += 1;
                            self.phase = Phase::Flight(Box::new(FlightForecastJob::new(
                                self.flight_scene.as_ref().unwrap(),
                                self.proposal.unwrap(),
                            )));
                        }
                        return;
                    }
                    if self.local_dependencies && result.endpoint.is_some() {
                        if let Phase::Trip(trip) = std::mem::replace(&mut self.phase, Phase::Done) {
                            self.phase = Phase::Dependencies(Box::new(RouteDependenciesJob::new(
                                trip,
                                self.radius,
                                self.gravity,
                            )));
                        }
                        return;
                    }
                    let route = LandingObjectiveRoute {
                        crossing: self.crossing,
                        site: self.candidates[self.index].site,
                        outbound: result.outbound.diagnostics.clone(),
                        returning: result.returning.as_ref().map(|r| r.diagnostics.clone()),
                        endpoint: result.endpoint,
                    };
                    self.finish_route(route);
                }
            }
            Phase::Dependencies(j) => {
                if j.next_work().is_some() {
                    j.step();
                } else {
                    let result = j.trip.output().unwrap();
                    let route = LandingObjectiveRoute {
                        crossing: self.crossing,
                        site: self.candidates[self.index].site,
                        outbound: result.outbound.diagnostics.clone(),
                        returning: result.returning.as_ref().map(|r| r.diagnostics.clone()),
                        endpoint: result.endpoint,
                    };
                    let mut areas = j.take_areas();
                    areas.extend(self.entrance_dependencies());
                    areas.extend(self.flight_area);
                    self.dependencies.push((route.site, areas));
                    self.finish_route(route);
                }
            }
            Phase::Proposal(j) => {
                if j.next_work().is_some() {
                    j.step();
                } else if let Some(proposal) = *j.output().unwrap() {
                    self.proposal = Some(proposal);
                    let c = self.candidates[self.index];
                    self.phase = Phase::Trip(Box::new(
                        GroundRoundTripJob::with_hatches(
                            Arc::clone(self.candidate_map.as_ref().unwrap()),
                            c.hatch,
                            self.result.objective.position,
                            self.result.objective.range,
                            c.boarding_hatches,
                        )
                        .with_crossing(proposal.edges()),
                    ));
                } else {
                    let failure = self.walking_failure.take().unwrap();
                    self.finish_route(failure);
                }
            }
            Phase::Flight(j) => {
                if j.next_work().is_some() {
                    j.step();
                } else if let Some(crossing) = *j.output().unwrap() {
                    self.flight_work.approved += 1;
                    self.crossing = Some(crossing);
                    self.flight_area = Some(j.area());
                    self.phase = Phase::Trip(self.saved_trip.take().unwrap());
                } else {
                    *self
                        .flight_work
                        .rejected
                        .entry(j.rejection().expect("failed forecast has a reason"))
                        .or_default() += 1;
                    let failure = self.walking_failure.take().unwrap();
                    self.finish_route(failure);
                }
            }
            Phase::Done => {}
        }
    }
}
