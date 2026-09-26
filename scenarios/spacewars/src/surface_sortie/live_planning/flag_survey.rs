//! Observational remote flag trips. Hosts dispatch these last, with the work
//! left after local planning and mission evaluation. No result reaches controls.
use super::*;
use destination_cover::{CoverFinding, CoverMeasurement};
use engine_core::planning::{JobAllocation, JobPhase};
use engine_rapier::world::CollisionGroups;
use query_budget::{QueryFuel, SITE_QUERY_CAP};

pub const MAX_FLAG_SURVEY_AGE: u64 = 30 * 60;
const PATCH_HALF_WIDTH: u16 = 8;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FlagSurveyRequest {
    pub generation: u64,
    pub objective: LandingObjective,
    pub candidates: [LandingSiteId; 2],
}
impl FlagSurveyRequest {
    pub fn matches_objective(&self, objective: LandingObjective) -> bool {
        LiveObjectivePlanner::same_objective(self.objective, objective)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlagSurveySample {
    pub actor: PlayerId,
    pub generation: u64,
    pub site: LandingSiteId,
    pub source_tick: u64,
    pub completed_tick: u64,
    /// A one-time publication check, never a renewed source or live permission.
    pub validated_tick: Option<u64>,
    pub objective: LandingObjective,
    pub measurement: CoverMeasurement,
    pub route: Option<LandingObjectiveRoute>,
    pub reason: Option<&'static str>,
    pub graph: u64,
    pub physics_queries: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FlagSurveyTelemetry {
    pub started: u64,
    pub completed: u64,
    pub walking_round_trips: u64,
    pub cancelled: u64,
    pub cancelled_jobs: u64,
    pub cancelled_graph: u64,
    pub cancelled_queries: u64,
    pub cancellation_reasons: BTreeMap<&'static str, u64>,
    pub graph: u64,
    pub physics_queries: u64,
    pub snapshot_builds: u64,
    pub snapshot_ms: f64,
    pub geometry_checks: u64,
    pub geometry_area_tests: u64,
    pub geometry_ms: f64,
    pub max_completion_ticks: u64,
    pub deferred: BTreeMap<&'static str, u64>,
    pub unknown: BTreeMap<&'static str, u64>,
}

#[derive(Clone)]
struct Pending {
    token: RequestToken,
    snapshot: Arc<QuerySnapshot>,
    sample: FlagSurveySample,
}

#[derive(Clone)]
struct Actor {
    request: FlagSurveyRequest,
    pilot: PilotObservationV1,
    opponent: Option<combat::CombatTarget>,
    seen: u64,
    next: usize,
    pending: Option<Pending>,
    samples: Vec<FlagSurveySample>,
}

/// Separate token namespace from LiveObjectivePlanner and MissionEvaluator.
/// Capacity limits actors/snapshots; each actor retains at most two samples.
#[derive(Clone)]
pub struct FlagSurveyPlanner {
    queue: PlanningQueue<(), ObjectiveSurveyJob>,
    actors: BTreeMap<usize, Actor>,
    capacity: usize,
    last_advance: Option<u64>,
    telemetry: FlagSurveyTelemetry,
}
impl FlagSurveyPlanner {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: PlanningQueue::new(capacity),
            actors: BTreeMap::new(),
            capacity,
            last_advance: None,
            telemetry: Default::default(),
        }
    }
    pub fn telemetry(&self) -> &FlagSurveyTelemetry {
        &self.telemetry
    }
    pub fn samples(&self) -> Vec<&FlagSurveySample> {
        self.actors.values().flat_map(|a| &a.samples).collect()
    }
    fn remove(&mut self, actor: usize, reason: &'static str) {
        if let Some(old) = self.actors.remove(&actor) {
            if let Some(pending) = old.pending {
                self.queue.cancel(pending.token);
                self.telemetry.cancelled_jobs += 1;
                self.telemetry.cancelled_graph += pending.sample.graph;
                self.telemetry.cancelled_queries += pending.sample.physics_queries;
                *self
                    .telemetry
                    .cancellation_reasons
                    .entry(reason)
                    .or_default() += 1;
            }
            self.telemetry.cancelled += 1;
        }
    }
    /// Requests are chosen by the AI after controls. No local sensors are
    /// replaced, and observations/measurements do not enter the mission report.
    pub fn observe(
        &mut self,
        state: &SurfaceSortieState,
        player: usize,
        o: &mission::MissionObservationV1,
        request: Option<FlagSurveyRequest>,
    ) {
        let p = &o.local.combat.recovery.flight.pilot;
        assert_eq!(p.tick, state.world.tick);
        assert_eq!(p.owner, state.pilots[player].owner);
        let Some(request) = request.filter(|r| {
            r.generation <= p.tick
                && r.candidates[0] != r.candidates[1]
                && r.candidates.iter().all(|id| {
                    id.planet == r.objective.planet && id.bearing < pilot::LANDING_SITE_COUNT
                })
                && p.ship_available
                && p.ship_form == ShipForm::Ship
                && matches!(p.location, PilotLocation::Aboard(_))
                && p.landing.phase != LandingPhase::Landed
                && p.queries_ready
                && !o.match_context.as_ref().is_some_and(|m| m.finished)
        }) else {
            self.remove(player, "inactive or absent demand");
            return;
        };
        let Some(planet) = o
            .planets
            .iter()
            .take(8)
            .find(|v| v.index == request.objective.planet)
        else {
            self.remove(player, "planet absent");
            return;
        };
        let mut remote = p.clone();
        remote.planet = planet.clone();
        let Some(body) = state.world.planets.get(planet.index) else {
            self.remove(player, "planet absent");
            return;
        };
        // Site, graph and claim inputs all describe the same authoritative frame.
        let frame = motion::SurfaceFrame::read(&state.world.physics, planet.index);
        remote.planet.motion = pilot::PilotMotion {
            position: frame.position,
            velocity: frame.linear_velocity,
            angle: frame.angle,
            spin: frame.angular_velocity,
        };
        remote.planet.radius = body.radius;
        remote.planet.revision = state
            .world
            .terrain
            .planets
            .get(&planet.index)
            .map_or(0, |p| p.field.revision());
        remote.planet.claim = state.claim_observation(planet.index, player);
        remote.sites.clear();
        if LandingObjective::read(&remote).is_none_or(|v| !request.matches_objective(v))
            || remote.planet.claim.as_ref().is_none_or(|c| {
                c.owner != Some(request.objective.owner)
                    || (c.stage_required_seconds - 3.0).abs() > 0.001
            })
        {
            self.remove(player, "objective changed");
            return;
        }
        if self
            .actors
            .get(&player)
            .is_some_and(|a| a.request != request || a.seen > p.tick)
        {
            self.remove(player, "request changed");
        }
        if !self.actors.contains_key(&player) && self.actors.len() >= self.capacity {
            *self.telemetry.deferred.entry("capacity").or_default() += 1;
            return;
        }
        let actor = self.actors.entry(player).or_insert_with(|| Actor {
            request,
            pilot: remote.clone(),
            opponent: None,
            seen: p.tick,
            next: 0,
            pending: None,
            samples: Vec::new(),
        });
        actor.seen = p.tick;
        actor.pilot = remote;
        actor.opponent = o.local.combat.target;
        actor
            .samples
            .retain(|s| p.tick - s.source_tick <= MAX_FLAG_SURVEY_AGE);
    }

    /// `remaining` must exclude every earlier stage's charge. `busy` contains
    /// actors that already performed a physical check in those stages, so they
    /// cannot also begin an atomic landing check here. Incremental job queries
    /// still use the remaining shared fuel. Repeated calls never spend twice.
    pub fn advance(
        &mut self,
        state: &SurfaceSortieState,
        remaining: Work,
        busy: &[usize],
    ) -> Option<PlanningReport> {
        let tick = state.world.tick;
        if self.last_advance == Some(tick) {
            return None;
        }
        let absent: Vec<_> = self
            .actors
            .iter()
            .filter(|(_, a)| a.seen != tick)
            .map(|(&id, _)| id)
            .collect();
        for actor in absent {
            self.remove(actor, "actor unobserved");
        }
        let allowance = Work {
            graph: remaining.graph.min(2),
            ..remaining
        };
        let mut atomic = Vec::new();
        let mut queries = allowance.physics_queries;
        for (&player, actor) in &mut self.actors {
            if actor
                .pending
                .as_ref()
                .is_some_and(|j| tick - j.sample.source_tick > MAX_FLAG_SURVEY_AGE)
            {
                let pending = actor.pending.take().unwrap();
                self.queue.cancel(pending.token);
                Self::finish(
                    actor,
                    pending.sample,
                    tick,
                    Some("source expired before completion"),
                    &mut self.telemetry,
                );
            }
            if actor.pending.is_some() {
                continue;
            }
            let id = actor.request.candidates[actor.next];
            if actor.samples.iter().any(|s| s.site == id) {
                continue;
            }
            let reason = if actor.pilot.site_query != LandingSiteQuery::NotRequested
                || actor.pilot.landing.supported_feet > 0
            {
                Some("local landing demand")
            } else if busy.contains(&player) {
                Some("earlier physical work")
            } else if queries < SITE_QUERY_CAP || allowance.graph == 0 {
                Some("remaining budget")
            } else if state.world.physics.material_queries_dirty {
                Some("material queries unavailable")
            } else if state.world.physics.world.collider_count() > MAX_SNAPSHOT_COLLIDERS
                || state.world.physics.world.body_count() > MAX_SNAPSHOT_BODIES
            {
                Some("snapshot capacity")
            } else {
                None
            };
            if let Some(reason) = reason {
                *self.telemetry.deferred.entry(reason).or_default() += 1;
                continue;
            }
            let fuel = QueryFuel::default();
            let measurement = destinations::measure(state, player, id, actor.opponent, true, &fuel);
            queries -= fuel.used();
            let sample = FlagSurveySample {
                actor: actor.pilot.owner,
                generation: actor.request.generation,
                site: id,
                source_tick: tick,
                completed_tick: tick,
                validated_tick: None,
                objective: actor.request.objective,
                measurement,
                route: None,
                reason: None,
                graph: 0,
                physics_queries: u64::from(fuel.used()),
            };
            self.telemetry.started += 1;
            let usable = sample.measurement.finding == CoverFinding::Measured
                && sample.measurement.climb_clear == Some(true)
                && sample
                    .measurement
                    .site
                    .is_some_and(|s| s.boarding_hatches.iter().any(Option::is_some));
            let token = if usable {
                let clock = Instant::now();
                let snapshot = Arc::new(state.world.physics.world.query_snapshot());
                self.telemetry.snapshot_builds += 1;
                self.telemetry.snapshot_ms += clock.elapsed().as_secs_f64() * 1000.0;
                let mut p = actor.pilot.clone();
                p.sites = vec![sample.measurement.site.unwrap()];
                let center = ((-actor.request.objective.position.x)
                    .atan2(actor.request.objective.position.y)
                    .rem_euclid(std::f32::consts::TAU)
                    * ground_navigation::GROUND_SAMPLES as f32
                    / std::f32::consts::TAU)
                    .round() as u16
                    % ground_navigation::GROUND_SAMPLES as u16;
                let job = state
                    .objective_job_with_planning(
                        player,
                        &p,
                        &[],
                        Arc::clone(&snapshot),
                        None,
                        false,
                        ObjectivePlanning::JointRoundTrip,
                    )
                    .unwrap()
                    .with_walk_patch(center, PATCH_HALF_WIDTH);
                let token = self
                    .queue
                    .submit(
                        player as u64,
                        (),
                        JobLimits {
                            per_tick: Work {
                                graph: 1,
                                physics_queries: SITE_QUERY_CAP,
                            },
                            ..Default::default()
                        },
                        job,
                    )
                    .unwrap();
                actor.pending = Some(Pending {
                    token,
                    snapshot,
                    sample,
                });
                token
            } else {
                Self::finish(
                    actor,
                    sample,
                    tick,
                    Some("landing, boarding or climb unmeasured"),
                    &mut self.telemetry,
                );
                self.queue.reserve_token(player as u64)
            };
            atomic.push(JobAllocation {
                request: token,
                age_ticks: 0,
                limits: JobLimits::default(),
                charged: Work {
                    graph: 0,
                    physics_queries: fuel.used(),
                },
                phase: JobPhase::Pending,
            });
        }
        let mut report = self.queue.advance(Work {
            physics_queries: queries,
            ..allowance
        });
        report.allowance = allowance;
        for row in &report.jobs {
            let actor = self.actors.get_mut(&(row.request.actor as usize)).unwrap();
            let pending = actor.pending.as_mut().unwrap();
            pending.sample.graph += u64::from(row.charged.graph);
            pending.sample.physics_queries += u64::from(row.charged.physics_queries);
            if row.phase != JobPhase::Ready {
                continue;
            }
            let mut pending = actor.pending.take().unwrap();
            let job = self.queue.take(pending.token).unwrap();
            let route = job.output().unwrap().sites.first().cloned();
            let complete_walk = route.as_ref().is_some_and(|r| {
                r.cost().is_some()
                    && r.crossing.is_none()
                    && r.outbound.jumps == 0
                    && r.outbound.flights == 0
                    && r.returning
                        .as_ref()
                        .is_some_and(|back| back.jumps == 0 && back.flights == 0)
            });
            let reason = if !complete_walk {
                Some("no walking round trip within sampled patch")
            } else if state.world.physics.world.collider_count() > MAX_SNAPSHOT_COLLIDERS
                || state.world.physics.world.body_count() > MAX_SNAPSHOT_BODIES
            {
                Some("publication geometry exceeds capacity")
            } else {
                let clock = Instant::now();
                self.telemetry.geometry_checks += 1;
                let old = pending.sample.measurement.planet;
                let now = actor.pilot.planet.motion;
                // Includes the initial landing rays, hull settling, both hatch
                // corridors and the highest (+60) climb sample with hull margin.
                let validation = pending.snapshot.validate_region(
                    &state.world.physics.world,
                    QueryRegion {
                        previous_position: old.position,
                        previous_angle: old.angle,
                        current_position: now.position,
                        current_angle: now.angle,
                        radius: actor.pilot.planet.radius + 80.0,
                        groups: CollisionGroups::ALL,
                        excluded: &[
                            pilot_physics_id(actor.pilot.owner),
                            state.world.physics.surface_vehicle_entity(
                                state.pilots[row.request.actor as usize].vehicle.0,
                            ),
                        ],
                    },
                );
                self.telemetry.geometry_area_tests += validation.area_tests;
                let valid = !state.world.physics.material_queries_dirty && validation.valid;
                self.telemetry.geometry_ms += clock.elapsed().as_secs_f64() * 1000.0;
                if valid {
                    pending.sample.validated_tick = Some(tick);
                    None
                } else {
                    Some("geometry changed since source measurement")
                }
            };
            pending.sample.route = route;
            Self::finish(actor, pending.sample, tick, reason, &mut self.telemetry);
        }
        for row in atomic {
            report.charged.physics_queries += row.charged.physics_queries;
            if let Some(existing) = report.jobs.iter_mut().find(|r| r.request == row.request) {
                existing.charged.physics_queries += row.charged.physics_queries;
            } else {
                report.jobs.push(row);
            }
        }
        assert!(
            report.charged.graph <= remaining.graph
                && report.charged.physics_queries <= remaining.physics_queries
        );
        self.telemetry.graph += u64::from(report.charged.graph);
        self.telemetry.physics_queries += u64::from(report.charged.physics_queries);
        self.last_advance = Some(tick);
        Some(report)
    }
    fn finish(
        actor: &mut Actor,
        mut sample: FlagSurveySample,
        tick: u64,
        reason: Option<&'static str>,
        telemetry: &mut FlagSurveyTelemetry,
    ) {
        sample.completed_tick = tick;
        sample.reason = reason;
        telemetry.completed += 1;
        telemetry.max_completion_ticks = telemetry
            .max_completion_ticks
            .max(tick - sample.source_tick);
        telemetry.walking_round_trips += u64::from(reason.is_none());
        if let Some(reason) = reason {
            *telemetry.unknown.entry(reason).or_default() += 1;
        }
        actor.samples.retain(|s| s.site != sample.site);
        actor.samples.push(sample);
        actor.next = (actor.next + 1) % actor.request.candidates.len();
    }
}
