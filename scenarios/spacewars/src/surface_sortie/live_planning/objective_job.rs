use super::*;

#[derive(Clone)]
enum Phase {
    Ground(Box<GroundSurveyJob>),
    Avoid(Box<AvoidingJob>),
    Trip(Box<GroundRoundTripJob<'static>>),
    Done,
}
#[derive(Clone)]
pub(crate) struct ObjectiveSurveyJob {
    phase: Phase,
    base: Option<Arc<GroundMap>>,
    measurements: Option<Box<GroundMeasurements>>,
    reused: ReusedGroundWork,
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
    pub(crate) fn objective_job(
        &self,
        player: usize,
        p: &PilotObservationV1,
        cover: &[combat::LandingCover],
        snapshot: Arc<QuerySnapshot>,
        measurements: Option<Box<GroundMeasurements>>,
    ) -> Option<ObjectiveSurveyJob> {
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
            });
        }
        if candidates.is_empty() {
            return None;
        }
        Some(ObjectiveSurveyJob {
            phase: Phase::Ground(Box::new(ground)),
            base: None,
            measurements: None,
            reused: ReusedGroundWork::default(),
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
                planning: ObjectivePlanning::JointRoundTrip,
                version: 1,
                actor: p.owner,
                tick: measurement_tick,
                validated_tick: None,
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
}
impl PlanningJob for ObjectiveSurveyJob {
    type Output = LandingObjectiveSurvey;
    fn next_work(&self) -> Option<WorkKind> {
        match &self.phase {
            Phase::Ground(j) => j.next_work().or(Some(WorkKind::Graph)),
            Phase::Avoid(j) => j.next_work().or(Some(WorkKind::Graph)),
            Phase::Trip(j) => j.next_work().or(Some(WorkKind::Graph)),
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
                    let hatch = self.candidates[self.index].hatch;
                    self.phase = Phase::Trip(Box::new(GroundRoundTripJob::new(
                        map,
                        hatch,
                        self.result.objective.position,
                        self.result.objective.range,
                        hatch,
                    )));
                }
            }
            Phase::Trip(j) => {
                if j.next_work().is_some() {
                    j.step();
                } else {
                    let result = j.output().unwrap();
                    let route = LandingObjectiveRoute {
                        site: self.candidates[self.index].site,
                        outbound: result.outbound.diagnostics.clone(),
                        returning: result.returning.as_ref().map(|r| r.diagnostics.clone()),
                        endpoint: result.endpoint,
                    };
                    if route.site.is_some() {
                        self.result.sites.push(route);
                    } else {
                        self.result.actual = Some(route);
                    }
                    self.index += 1;
                    self.phase = if self.index < self.candidates.len() {
                        self.avoid()
                    } else {
                        Phase::Done
                    };
                }
            }
            Phase::Done => {}
        }
    }
}
