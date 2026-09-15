//! Opt-in incremental landing-objective measurements. Ordinary v9/v10 APIs stay synchronous.
use super::*;
use engine_core::planning::{
    JobLimits, JobPoll, PlanningJob, PlanningQueue, PlanningReport, RequestToken, Work, WorkKind,
};
use engine_rapier::world::{QueryArea, QueryFrame, QueryRegion, QuerySnapshot};
use ground_navigation::{
    GroundMap, GroundMeasurements, GroundRoundTripJob, GroundSurveyJob, ReusedGroundWork,
};
use landing_objective::{
    LandingObjective, LandingObjectiveRoute, LandingObjectiveSurvey, ObjectivePlanning,
};
use pilot::{LandingSiteId, PilotObservationV1};
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
    time::Instant,
};

mod avoiding;
mod objective_job;
mod route_dependencies;
use avoiding::{AvoidingJob, HullPreview};
use objective_job::ObjectiveSurveyJob;
use route_dependencies::RouteDependenciesJob;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
struct Candidate {
    site: Option<LandingSiteId>,
    vehicle: Vec2,
    angle: f32,
    hatch: Vec2,
    boarding_hatches: [Option<Vec2>; 2],
}

pub const MAX_SURVEY_AGE_TICKS: u64 = 120;
const REFRESH_TICKS: u64 = 30;
const MAX_SNAPSHOT_COLLIDERS: usize = 8192;
const MAX_SNAPSHOT_BODIES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveWorkState {
    Pending,
    Ready,
    Stale,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FlightForecastWork {
    pub started: u64,
    pub approved: u64,
    pub rejected: BTreeMap<&'static str, u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LivePlanningTelemetry {
    pub submitted: u64,
    pub completed: u64,
    pub published: u64,
    pub completed_by_actor: BTreeMap<usize, u64>,
    pub published_by_actor: BTreeMap<usize, u64>,
    pub flight_forecasts: FlightForecastWork,
    pub flight_environment_checks: u64,
    pub flight_environment_mismatches: u64,
    /// Counts entries withheld during validation, including repeated checks.
    pub withheld_flight_routes: u64,
    pub flight_independent_validations: u64,
    pub invalidations: BTreeMap<&'static str, u64>,
    pub deferred_capacity: u64,
    pub deferred_snapshot_limit: u64,
    pub graph: u64,
    pub physics_queries: u64,
    pub max_ready_age_ticks: u64,
    pub max_request_completion_ticks: u64,
    pub reused_requests: u64,
    pub reused_ground: ReusedGroundWork,
    pub reuse_rejections: BTreeMap<&'static str, u64>,
    /// Work in retired requests that never published. Some of their ground
    /// measurements may have been salvaged, so this is not all wasted work.
    pub retired_unpublished_graph: u64,
    pub retired_unpublished_queries: u64,
    pub snapshot_builds: u64,
    pub snapshot_max_bodies: usize,
    pub snapshot_max_colliders: usize,
    pub max_retained_requests: usize,
    pub snapshot_total_ms: f64,
    pub snapshot_max_ms: f64,
    pub validation_total_ms: f64,
    pub validation_max_ms: f64,
    pub region_area_tests: u64,
    pub route_checks: u64,
    pub route_area_tests: u64,
    pub route_unrelated_changes: u64,
    pub withheld_route_entries: u64,
    pub local_publications: u64,
    pub max_route_areas: usize,
    pub parked_restarts: u64,
    pub max_parked_requests: usize,
}

#[derive(Clone, Copy)]
struct ActualLanding {
    vehicle: Vec2,
    angle: f32,
    exit: Vec2,
    boarding_hatches: [Option<Vec2>; 2],
}

#[derive(Clone)]
struct Request {
    token: RequestToken,
    objective: LandingObjective,
    tick: u64,
    measurement_tick: u64,
    seen: u64,
    completed: bool,
    position: Vec2,
    angle: f32,
    gravity: f32,
    planning: ObjectivePlanning,
    jetpack_available: bool,
    flight_dependent: bool,
    flight_environment: Option<jetpack::forecast::FlightEnvironment>,
    flight_work: FlightForecastWork,
    actual: Option<ActualLanding>,
    snapshot: Arc<QuerySnapshot>,
    reused: ReusedGroundWork,
    graph: u64,
    physics_queries: u64,
}

#[derive(Clone)]
struct Parked {
    objective: LandingObjective,
    measurements: Box<GroundMeasurements>,
    seen: u64,
}

/// Observe each active planner, then call `advance` once before the shared
/// physics step. All jobs share one allowance. Reset on a new match/episode.
/// The budget covers only landing-objective work; immediate observations and
/// on-foot/recovery surveys retain their existing synchronous behavior.
#[derive(Clone)]
pub struct LiveObjectivePlanner {
    queue: PlanningQueue<u64, ObjectiveSurveyJob>,
    requests: BTreeMap<usize, Request>,
    parked: BTreeMap<usize, Parked>,
    capacity: usize,
    allowance: Work,
    reuse_ground: bool,
    local_dependencies: bool,
    last_observed: Option<u64>,
    last_advanced: Option<u64>,
    shared: Option<(u64, Weak<QuerySnapshot>)>,
    telemetry: LivePlanningTelemetry,
}
impl LiveObjectivePlanner {
    pub fn new(capacity: usize, allowance: Work) -> Self {
        Self {
            queue: PlanningQueue::new(capacity),
            requests: BTreeMap::new(),
            parked: BTreeMap::new(),
            capacity,
            allowance,
            reuse_ground: false,
            local_dependencies: false,
            last_observed: None,
            last_advanced: None,
            shared: None,
            telemetry: Default::default(),
        }
    }
    /// Opt in to retaining footings and walking clearance across compatible
    /// request restarts. Jump measurements and hull overlays are always rebuilt.
    pub fn with_ground_reuse(mut self) -> Self {
        self.reuse_ground = true;
        self
    }
    pub fn reuses_ground(&self) -> bool {
        self.reuse_ground
    }
    /// Allow a coherent survey to finish while obstacles move, then validate
    /// successful directed paths locally. Negative results still require the
    /// complete region to match. This also enables compatible ground reuse.
    pub fn with_route_dependencies(mut self) -> Self {
        self.reuse_ground = true;
        self.local_dependencies = true;
        self
    }
    pub fn uses_route_dependencies(&self) -> bool {
        self.local_dependencies
    }
    pub fn allowance(&self) -> Work {
        self.allowance
    }
    pub fn telemetry(&self) -> &LivePlanningTelemetry {
        &self.telemetry
    }
    pub fn reset(&mut self) {
        self.queue.reset();
        self.requests.clear();
        self.parked.clear();
        self.shared = None;
        self.last_observed = None;
        self.last_advanced = None;
        self.telemetry = Default::default();
    }
    pub fn remove(&mut self, player: usize) {
        self.parked.remove(&player);
        self.retire(player);
    }
    fn retire(&mut self, player: usize) -> Option<ObjectiveSurveyJob> {
        if let Some(request) = self.requests.remove(&player) {
            if !request.completed {
                self.telemetry.retired_unpublished_graph += request.graph;
                self.telemetry.retired_unpublished_queries += request.physics_queries;
            }
            return self.queue.take(request.token);
        }
        None
    }
    fn invalidate(&mut self, player: usize, reason: &'static str) {
        self.remove(player);
        *self.telemetry.invalidations.entry(reason).or_default() += 1;
    }
    fn actual(p: &PilotObservationV1) -> Option<ActualLanding> {
        if p.landing.phase != LandingPhase::Landed {
            return None;
        }
        let local =
            |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        Some(ActualLanding {
            vehicle: local(p.ship.position),
            angle: p.ship.angle - p.planet.motion.angle,
            exit: local(p.hatch?),
            boarding_hatches: p.boarding_hatches.map(|h| h.map(local)),
        })
    }
    fn same_objective(old: LandingObjective, new: LandingObjective) -> bool {
        old.matches(new)
            && old.position.distance_to(new.position) <= 0.002
            && (old.range - new.range).abs() <= 0.0001
    }
    fn valid(
        state: &SurfaceSortieState,
        player: usize,
        p: &PilotObservationV1,
        request: &Request,
        objective: LandingObjective,
        local_dependencies: bool,
    ) -> Result<(), &'static str> {
        if !Self::same_objective(request.objective, objective) {
            return Err("objective_changed");
        }
        if request.measurement_tick > p.tick
            || p.tick - request.measurement_tick > MAX_SURVEY_AGE_TICKS
        {
            return Err("expired");
        }
        let actual = Self::actual(p);
        if actual.is_some() != request.actual.is_some() {
            return Err("touchdown_changed");
        }
        if let (Some(old), Some(new)) = (request.actual, actual)
            && (old.vehicle.distance_to(new.vehicle) > 0.002
                || old.exit.distance_to(new.exit) > 0.002
                || old
                    .boarding_hatches
                    .into_iter()
                    .zip(new.boarding_hatches)
                    .any(|(old, new)| match (old, new) {
                        (Some(a), Some(b)) => a.distance_to(b) > 0.002,
                        (None, None) => false,
                        _ => true,
                    })
                || ((old.angle - new.angle) * 0.5).sin().abs() > 0.0001)
        {
            return Err("hatch_moved");
        }
        let gravity = state.objective_gravity(p);
        if (gravity - request.gravity).abs() > 0.01 {
            return Err("gravity_changed");
        }
        if request.planning == ObjectivePlanning::JetpackRoundTrip
            && request.jetpack_available != state.pilots[player].jetpack_charge.is_some()
        {
            return Err("jetpack_changed");
        }
        // Local validation can still deliver ground routes from this coherent
        // survey. Certify its flight-dependent answers at publication instead.
        if !local_dependencies && !Self::flight_environment_valid(state, p, request) {
            return Err("flight_environment_changed");
        }
        if local_dependencies {
            Ok(())
        } else {
            Self::geometry_valid(state, player, p, request, request.gravity)
        }
    }
    fn flight_environment_valid(
        state: &SurfaceSortieState,
        p: &PilotObservationV1,
        request: &Request,
    ) -> bool {
        !request.flight_dependent
            || request.flight_environment.as_ref().is_some_and(|old| {
                jetpack::forecast::FlightEnvironment::read(state, p)
                    .is_some_and(|new| old.compatible(&new))
            })
    }
    fn geometry_valid(
        state: &SurfaceSortieState,
        player: usize,
        p: &PilotObservationV1,
        request: &Request,
        gravity: f32,
    ) -> Result<(), &'static str> {
        let spec = SurfaceSortieState::spec();
        let radius =
            p.planet.radius + 12.0 + spec.jump_speed.powi(2) / (2.0 * gravity.max(1.0)) * 0.85;
        let radius = if request.planning == ObjectivePlanning::JetpackRoundTrip {
            radius.max(p.planet.radius + jetpack::forecast::FORECAST_REGION_HEIGHT)
        } else {
            radius
        };
        if !request.snapshot.matches_region(
            &state.world.physics.world,
            QueryRegion {
                previous_position: request.position,
                previous_angle: request.angle,
                current_position: p.planet.motion.position,
                current_angle: p.planet.motion.angle,
                radius,
                groups: spec.collision_groups,
                excluded: &[
                    pilot_physics_id(p.owner),
                    state
                        .world
                        .physics
                        .surface_vehicle_entity(state.pilots[player].vehicle.0),
                ],
            },
        ) {
            return Err("obstacles_changed");
        }
        Ok(())
    }

    fn salvage(
        &mut self,
        state: &SurfaceSortieState,
        player: usize,
        p: &PilotObservationV1,
    ) -> Option<Box<GroundMeasurements>> {
        if !self.reuse_ground {
            self.remove(player);
            return None;
        }
        if self.local_dependencies {
            // This remains one coherent, possibly stale, world hypothesis.
            // Publication must certify the selected paths in the live world;
            // negative answers still require the complete region to match.
            return self
                .retire(player)
                .map(ObjectiveSurveyJob::into_measurements);
        }
        let request = self.requests.get(&player)?;
        // Check geometry even when the earlier validity check stopped at a
        // gravity/hatch change. A lower gravity also expands the jump region.
        let clock = Instant::now();
        let valid = Self::geometry_valid(state, player, p, request, state.objective_gravity(p));
        let ms = clock.elapsed().as_secs_f64() * 1000.0;
        self.telemetry.validation_total_ms += ms;
        self.telemetry.validation_max_ms = self.telemetry.validation_max_ms.max(ms);
        if let Err(reason) = valid {
            *self.telemetry.reuse_rejections.entry(reason).or_default() += 1;
            self.remove(player);
            return None;
        }
        self.retire(player)
            .map(ObjectiveSurveyJob::into_measurements)
    }

    fn locally_validated(
        state: &SurfaceSortieState,
        player: usize,
        p: &PilotObservationV1,
        request: &Request,
        job: &ObjectiveSurveyJob,
        mut survey: LandingObjectiveSurvey,
        telemetry: &mut LivePlanningTelemetry,
    ) -> Option<LandingObjectiveSurvey> {
        let flight_valid = Self::flight_environment_valid(state, p, request);
        telemetry.flight_environment_checks += u64::from(request.flight_dependent);
        telemetry.flight_environment_mismatches += u64::from(!flight_valid);
        let excluded = [
            pilot_physics_id(p.owner),
            state
                .world
                .physics
                .surface_vehicle_entity(state.pilots[player].vehicle.0),
        ];
        let region = QueryFrame {
            previous_position: request.position,
            previous_angle: request.angle,
            current_position: p.planet.motion.position,
            current_angle: p.planet.motion.angle,
            excluded: &excluded,
        };
        // The full-survey shortcut must use the same conservative transform
        // bound as the selected paths, including long colliders with distant
        // origins. Keep the circular region to avoid unmeasured box corners.
        let spec = SurfaceSortieState::spec();
        let radius = p.planet.radius
            + 12.0
            + spec.jump_speed.powi(2) / (2.0 * request.gravity.max(1.0)) * 0.85;
        let radius = if request.planning == ObjectivePlanning::JetpackRoundTrip {
            radius.max(p.planet.radius + jetpack::forecast::FORECAST_REGION_HEIGHT)
        } else {
            radius
        };
        let whole = request.snapshot.validate_region(
            &state.world.physics.world,
            QueryRegion {
                previous_position: request.position,
                previous_angle: request.angle,
                current_position: p.planet.motion.position,
                current_angle: p.planet.motion.angle,
                radius,
                groups: spec.collision_groups,
                excluded: &excluded,
            },
        );
        telemetry.region_area_tests += whole.area_tests;
        if whole.valid && flight_valid {
            return Some(survey);
        }
        let mut valid = Vec::new();
        for (site, areas) in job.dependencies() {
            // Failed flight attempts must not poison independent walking
            // routes. Conversely, a changed field certifies neither a powered
            // route nor a negative answer that might now be flyable.
            let route = survey
                .sites
                .iter()
                .chain(survey.actual.iter())
                .find(|r| r.site == *site);
            if !route.is_some_and(|r| r.cost().is_some() && (flight_valid || r.crossing.is_none()))
            {
                continue;
            }
            if whole.valid {
                valid.push(*site);
                continue;
            }
            telemetry.route_checks += 1;
            telemetry.max_route_areas = telemetry.max_route_areas.max(areas.len());
            let check = request
                .snapshot
                .validate_areas(&state.world.physics.world, region, areas);
            telemetry.route_area_tests += check.area_tests;
            telemetry.route_unrelated_changes += check.unrelated_changes;
            if check.valid {
                valid.push(*site);
            }
        }
        let before = survey.sites.len() + usize::from(survey.actual.is_some());
        if !flight_valid {
            telemetry.withheld_flight_routes += survey
                .sites
                .iter()
                .chain(survey.actual.iter())
                .filter(|r| r.crossing.is_some())
                .count() as u64;
        }
        survey
            .sites
            .retain(|r| r.cost().is_some() && valid.contains(&r.site));
        survey.actual = survey
            .actual
            .filter(|r| r.cost().is_some() && valid.contains(&None));
        telemetry.withheld_route_entries +=
            (before - survey.sites.len() - usize::from(survey.actual.is_some())) as u64;
        if (survey.sites.is_empty() && survey.actual.is_none())
            || (request.actual.is_some() && survey.actual.is_none())
        {
            return None;
        }
        survey.validated_routes_only = true;
        telemetry.flight_independent_validations += u64::from(!flight_valid);
        Some(survey)
    }

    pub fn observe(
        &mut self,
        state: &SurfaceSortieState,
        player: usize,
        o: &mut combat::TacticalSortieObservationV1,
    ) {
        self.observe_with_planning(state, player, o, ObjectivePlanning::JointRoundTrip);
    }
    pub fn observe_with_planning(
        &mut self,
        state: &SurfaceSortieState,
        player: usize,
        o: &mut combat::TacticalSortieObservationV1,
        planning: ObjectivePlanning,
    ) {
        assert!(!planning.is_legacy());
        if self
            .requests
            .get(&player)
            .is_some_and(|r| r.planning != planning)
        {
            self.invalidate(player, "policy_changed");
        }
        let p = &o.combat.recovery.flight.pilot;
        assert!(
            player < state.pilots.len()
                && p.owner == state.pilots[player].owner
                && p.tick == state.world.tick
        );
        if self.last_observed.is_some_and(|tick| p.tick < tick) {
            self.reset();
        }
        self.last_observed = Some(p.tick);
        o.landing_objective = None;
        o.objective_work = None;
        let Some(objective) = LandingObjective::read(p).filter(|_| {
            p.queries_ready
                && p.ship_available
                && p.ship_form == ShipForm::Ship
                && matches!(p.location, PilotLocation::Aboard(_))
        }) else {
            self.remove(player);
            return;
        };
        let mut invalidation = None;
        let mut measurements = None;
        let was_parked = self.parked.contains_key(&player);
        if let Some(parked) = self.parked.remove(&player) {
            if !Self::same_objective(parked.objective, objective) {
                invalidation = Some("objective_changed");
            } else if parked.measurements.tick > p.tick
                || p.tick - parked.measurements.tick > MAX_SURVEY_AGE_TICKS
            {
                invalidation = Some("expired");
            } else {
                measurements = Some(parked.measurements);
            }
            if let Some(reason) = invalidation {
                *self.telemetry.invalidations.entry(reason).or_default() += 1;
            }
        }
        if let Some(request) = self.requests.get_mut(&player) {
            request.seen = p.tick;
            let clock = Instant::now();
            let valid = Self::valid(
                state,
                player,
                p,
                request,
                objective,
                self.local_dependencies,
            );
            let ms = clock.elapsed().as_secs_f64() * 1000.0;
            self.telemetry.validation_total_ms += ms;
            self.telemetry.validation_max_ms = self.telemetry.validation_max_ms.max(ms);
            match valid {
                Err(reason) => {
                    invalidation = Some(reason);
                    if matches!(
                        reason,
                        "gravity_changed" | "touchdown_changed" | "hatch_moved"
                    ) {
                        measurements = self.salvage(state, player, p);
                        *self.telemetry.invalidations.entry(reason).or_default() += 1;
                    } else {
                        self.invalidate(player, reason);
                    }
                }
                Ok(()) => match self.queue.poll(request.token, &request.tick) {
                    JobPoll::Ready(survey) => {
                        let survey = survey.clone();
                        let survey = if self.local_dependencies {
                            let clock = Instant::now();
                            let result = Self::locally_validated(
                                state,
                                player,
                                p,
                                request,
                                self.queue.job(request.token).unwrap(),
                                survey,
                                &mut self.telemetry,
                            );
                            let ms = clock.elapsed().as_secs_f64() * 1000.0;
                            self.telemetry.validation_total_ms += ms;
                            self.telemetry.validation_max_ms =
                                self.telemetry.validation_max_ms.max(ms);
                            result
                        } else {
                            Some(survey)
                        };
                        if let Some(mut survey) = survey {
                            let first = !request.completed;
                            if first {
                                request.completed = true;
                                self.telemetry.completed += 1;
                                *self.telemetry.completed_by_actor.entry(player).or_default() += 1;
                                self.telemetry.max_ready_age_ticks = self
                                    .telemetry
                                    .max_ready_age_ticks
                                    .max(p.tick - request.measurement_tick);
                                self.telemetry.max_request_completion_ticks = self
                                    .telemetry
                                    .max_request_completion_ticks
                                    .max(p.tick - request.tick);
                            }
                            if first || p.tick - request.tick < REFRESH_TICKS {
                                survey.validated_tick = Some(p.tick);
                                self.telemetry.local_publications +=
                                    u64::from(survey.validated_routes_only);
                                o.landing_objective = Some(survey);
                                o.objective_work = Some(ObjectiveWorkState::Ready);
                                self.telemetry.published += 1;
                                *self.telemetry.published_by_actor.entry(player).or_default() += 1;
                                return;
                            }
                            measurements = self.salvage(state, player, p);
                        } else {
                            invalidation = Some("routes_changed");
                            self.invalidate(player, "routes_changed");
                        }
                    }
                    JobPoll::Pending => {
                        o.objective_work = Some(ObjectiveWorkState::Pending);
                        return;
                    }
                    JobPoll::Stale => {
                        invalidation = Some("cancelled");
                        self.invalidate(player, "cancelled");
                    }
                },
            }
        }
        o.objective_work = Some(
            if invalidation.is_some_and(|r| r != "expired" && r != "touchdown_changed") {
                ObjectiveWorkState::Stale
            } else {
                ObjectiveWorkState::Pending
            },
        );
        if self.requests.len() + self.parked.len() == self.capacity {
            self.telemetry.deferred_capacity += 1;
            return;
        }
        if state.world.physics.world.collider_count() > MAX_SNAPSHOT_COLLIDERS
            || state.world.physics.world.body_count() > MAX_SNAPSHOT_BODIES
        {
            self.telemetry.deferred_snapshot_limit += 1;
            return;
        }
        if p.sites.is_empty() && Self::actual(p).is_none() {
            if self.local_dependencies
                && let Some(measurements) = measurements
            {
                self.parked.insert(
                    player,
                    Parked {
                        objective,
                        measurements,
                        seen: p.tick,
                    },
                );
                self.telemetry.parked_restarts += u64::from(!was_parked);
                self.telemetry.max_parked_requests =
                    self.telemetry.max_parked_requests.max(self.parked.len());
            }
            return;
        }
        let snapshot = measurements
            .as_ref()
            .map(|m| Arc::clone(&m.snapshot))
            .or_else(|| {
                self.shared
                    .as_ref()
                    .filter(|(tick, _)| *tick == p.tick)
                    .and_then(|(_, s)| s.upgrade())
            })
            .unwrap_or_else(|| {
                let clock = Instant::now();
                let snapshot = Arc::new(state.world.physics.world.query_snapshot());
                let ms = clock.elapsed().as_secs_f64() * 1000.0;
                self.telemetry.snapshot_builds += 1;
                self.telemetry.snapshot_total_ms += ms;
                self.telemetry.snapshot_max_ms = self.telemetry.snapshot_max_ms.max(ms);
                let (bodies, colliders) = snapshot.counts();
                self.telemetry.snapshot_max_bodies = self.telemetry.snapshot_max_bodies.max(bodies);
                self.telemetry.snapshot_max_colliders =
                    self.telemetry.snapshot_max_colliders.max(colliders);
                self.shared = Some((p.tick, Arc::downgrade(&snapshot)));
                snapshot
            });
        let measurement_tick = measurements.as_ref().map_or(p.tick, |m| m.tick);
        let position = measurements
            .as_ref()
            .map_or(p.planet.motion.position, |m| m.position);
        let angle = measurements
            .as_ref()
            .map_or(p.planet.motion.angle, |m| m.angle);
        let reused = measurements.is_some();
        if let Some(job) = state.objective_job_with_planning(
            player,
            p,
            &o.cover,
            Arc::clone(&snapshot),
            measurements,
            self.local_dependencies,
            planning,
        ) {
            let token = self
                .queue
                .submit(player as u64, p.tick, JobLimits::default(), job)
                .unwrap();
            self.requests.insert(
                player,
                Request {
                    token,
                    objective,
                    tick: p.tick,
                    measurement_tick,
                    seen: p.tick,
                    completed: false,
                    position,
                    angle,
                    gravity: state.objective_gravity(p),
                    planning,
                    jetpack_available: state.pilots[player].jetpack_charge.is_some(),
                    flight_dependent: false,
                    flight_environment: (planning == ObjectivePlanning::JetpackRoundTrip)
                        .then(|| jetpack::forecast::FlightEnvironment::read(state, p))
                        .flatten(),
                    flight_work: FlightForecastWork::default(),
                    actual: Self::actual(p),
                    snapshot,
                    reused: ReusedGroundWork::default(),
                    graph: 0,
                    physics_queries: 0,
                },
            );
            self.telemetry.submitted += 1;
            self.telemetry.reused_requests += u64::from(reused);
            self.telemetry.max_retained_requests = self
                .telemetry
                .max_retained_requests
                .max(self.requests.len() + self.parked.len());
        }
    }

    /// Repeated calls for the same physics tick cannot spend the allowance twice.
    /// Actors not observed this tick are removed before dispatch.
    pub fn advance(&mut self, tick: u64) -> Option<PlanningReport> {
        if self.last_advanced == Some(tick) {
            return None;
        }
        let removed: Vec<_> = self
            .requests
            .iter()
            .filter(|(_, r)| r.seen != tick)
            .map(|(&p, _)| p)
            .chain(
                self.parked
                    .iter()
                    .filter(|(_, p)| p.seen != tick)
                    .map(|(&p, _)| p),
            )
            .collect();
        for player in removed {
            self.remove(player);
        }
        let report = self.queue.advance(self.allowance);
        for allocation in &report.jobs {
            let request = self
                .requests
                .get_mut(&(allocation.request.actor as usize))
                .unwrap();
            request.graph += u64::from(allocation.charged.graph);
            request.physics_queries += u64::from(allocation.charged.physics_queries);
            let job = self.queue.job(request.token).unwrap();
            request.flight_dependent = job.uses_flight_environment();
            let flight = job.flight_work();
            self.telemetry.flight_forecasts.started += flight.started - request.flight_work.started;
            self.telemetry.flight_forecasts.approved +=
                flight.approved - request.flight_work.approved;
            for (&reason, &count) in &flight.rejected {
                *self
                    .telemetry
                    .flight_forecasts
                    .rejected
                    .entry(reason)
                    .or_default() += count
                    - request
                        .flight_work
                        .rejected
                        .get(reason)
                        .copied()
                        .unwrap_or(0);
            }
            request.flight_work = flight.clone();
            let reused = job.reused();
            self.telemetry.reused_ground.nodes += reused.nodes - request.reused.nodes;
            self.telemetry.reused_ground.walks += reused.walks - request.reused.walks;
            self.telemetry.reused_ground.physics_queries +=
                reused.physics_queries - request.reused.physics_queries;
            request.reused = reused;
        }
        self.telemetry.graph += u64::from(report.charged.graph);
        self.telemetry.physics_queries += u64::from(report.charged.physics_queries);
        self.last_advanced = Some(tick);
        Some(report)
    }
}
