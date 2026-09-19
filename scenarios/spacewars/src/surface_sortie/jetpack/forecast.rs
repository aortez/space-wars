//! A bounded, read-only prediction of one parked-ship crossing in both directions.
//! This is a controller model, not a second world simulation or a landing permission.
use super::*;
use engine_core::planning::{PlanningJob, WorkKind};
use engine_rapier::world::{CapsuleQuery, QueryArea, QuerySnapshot};
use flight::{FlightPhase, FlightSample, LANDING_RESERVE, LAUNCH_CHARGE};
use ground_navigation::{GroundEdge, GroundEdgeKind, GroundNode};
use std::sync::Arc;

const DT: f32 = 1.0 / 60.0;
// Proposals top out at R+18. Prediction is capped another 20 units above
// cruise; the remaining margin encloses the capsule and transform tolerance.
pub(crate) const FORECAST_REGION_HEIGHT: f32 = 40.0;
const MAX_STEPS: usize = 12 * 60;
mod environment;
pub(crate) use environment::FlightEnvironment;
type Preview = Arc<dyn Fn(Vec2, f32, Vec2, f32) -> bool + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FlightEstimate {
    pub seconds: f32,
    pub burn_seconds: f32,
    pub arrival_speed: f32,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct VehicleCrossingForecast {
    pub version: u32,
    pub measured_tick: u64,
    pub launch_until_tick: u64,
    pub plan: CrossingPlan,
    pub nodes: [u16; 2],
    /// Worst estimates across sampled launch times, independently recharged.
    pub flights: [FlightEstimate; 2],
}
impl VehicleCrossingForecast {
    pub fn edges(self) -> [GroundEdge; 2] {
        crossing_edges(self.nodes, self.plan)
    }
    pub fn is_valid(self) -> bool {
        let plan = self.plan;
        let finite = |p: Vec2| p.x.is_finite() && p.y.is_finite();
        self.version == 2
            && self
                .launch_until_tick
                .checked_sub(self.measured_tick)
                .is_some_and(|age| age <= environment::LAUNCH_WINDOW_TICKS)
            && finite(plan.start)
            && finite(plan.destination)
            && plan.start.distance_to(plan.destination) > 1.0
            && plan.start.distance_to(plan.destination) < 30.0
            && plan.cruise_radius.is_finite()
            && plan.cruise_radius > plan.start.length().max(plan.destination.length())
            && plan.cruise_radius < plan.start.length().min(plan.destination.length()) + 20.0
            && matches!(plan.anchor, CrossingAnchor::Vehicle { position, angle, form: ShipForm::Ship, .. } if finite(position) && angle.is_finite())
            && self.nodes[0] != self.nodes[1]
            && self
                .nodes
                .iter()
                .all(|id| usize::from(*id) < ground_navigation::GROUND_SAMPLES)
            && self.flights.iter().all(|f| {
                f.seconds.is_finite()
                    && f.seconds > 0.0
                    && f.seconds <= MAX_STEPS as f32 * DT
                    && f.burn_seconds.is_finite()
                    && f.burn_seconds >= 0.0
                    && f.burn_seconds <= (LAUNCH_CHARGE - LANDING_RESERVE) * motor::BURN_SECONDS
                    && f.arrival_speed.is_finite()
                    && (0.0..=7.0).contains(&f.arrival_speed)
            })
    }
    pub fn valid_at(self, tick: u64) -> bool {
        (self.measured_tick..=self.launch_until_tick).contains(&tick) && self.is_valid()
    }
    pub fn valid_for(self, map: &GroundMap) -> bool {
        self.is_valid()
            && self.plan.planet == map.planet
            && self.plan.revision == map.revision
            && self
                .nodes
                .into_iter()
                .zip([self.plan.start, self.plan.destination])
                .all(|(id, point)| {
                    map.nodes
                        .iter()
                        .any(|n| n.id == id && n.position.distance_to(point) < 0.01)
                })
    }
}
fn crossing_edges(nodes: [u16; 2], plan: CrossingPlan) -> [GroundEdge; 2] {
    [(nodes[0], nodes[1]), (nodes[1], nodes[0])].map(|(from, to)| GroundEdge {
        from,
        to,
        kind: GroundEdgeKind::Jetpack,
        length: plan.start.distance_to(plan.destination),
    })
}
#[derive(Clone, Copy)]
pub(crate) struct Proposal {
    pub plan: CrossingPlan,
    pub nodes: [u16; 2],
}
impl Proposal {
    pub fn edges(self) -> [GroundEdge; 2] {
        crossing_edges(self.nodes, self.plan)
    }
}

/// One node inspection per step. Use actual surveyed nodes as endpoints; no
/// nearest-node snap may bridge an unmeasured gap between landing and walking.
#[derive(Clone)]
pub(crate) struct ProposalJob {
    map: Arc<GroundMap>,
    index: usize,
    center: Vec2,
    angle: f32,
    vehicle: usize,
    width: f32,
    cruise: f32,
    nearest: [Option<GroundNode>; 2],
    result: Option<Proposal>,
    done: bool,
}
impl ProposalJob {
    pub fn new(
        map: Arc<GroundMap>,
        center: Vec2,
        angle: f32,
        vehicle: usize,
        hull: &[Vec2],
        radius: f32,
    ) -> Self {
        let right = Vec2::new(center.normalized().y, -center.normalized().x);
        let width = hull
            .iter()
            .map(|p| p.rotate_radians(angle).dot(right).abs())
            .fold(0.0, f32::max);
        let cruise = hull
            .iter()
            .map(|p| (center + p.rotate_radians(angle)).length())
            .fold(radius, f32::max)
            + 2.5;
        Self {
            map,
            index: 0,
            center,
            angle,
            vehicle,
            width,
            cruise,
            nearest: [None; 2],
            result: None,
            done: !cruise.is_finite() || cruise > radius + 18.0,
        }
    }
}
impl PlanningJob for ProposalJob {
    type Output = Option<Proposal>;
    fn next_work(&self) -> Option<WorkKind> {
        (!self.done).then_some(WorkKind::Graph)
    }
    fn output(&self) -> Option<&Self::Output> {
        self.done.then_some(&self.result)
    }
    fn step(&mut self) {
        if self.done {
            return;
        }
        let up = self.center.normalized();
        let right = Vec2::new(up.y, -up.x);
        if let Some(&node) = self.map.nodes.get(self.index) {
            self.index += 1;
            let along = (node.position - self.center).dot(right);
            let side = usize::from(along > 0.0);
            if along.abs() >= self.width + 2.0
                && along.abs() <= self.width + 6.0
                && node.position.normalized().dot(up) > 0.95
                && node.normal.dot(node.position.normalized())
                    >= SurfaceSortieState::spec().min_support_alignment
                && self.nearest[side].is_none_or(|old| {
                    node.position.distance_to(self.center) < old.position.distance_to(self.center)
                })
            {
                self.nearest[side] = Some(node);
            }
        } else {
            self.done = true;
            if let [Some(left), Some(right)] = self.nearest {
                let angle = left
                    .position
                    .normalized()
                    .dot(right.position.normalized())
                    .clamp(-1.0, 1.0)
                    .acos();
                if angle <= 0.55 && left.position.distance_to(right.position) < 30.0 {
                    self.result = Some(Proposal {
                        nodes: [right.id, left.id],
                        plan: CrossingPlan {
                            planet: self.map.planet,
                            revision: self.map.revision,
                            direction: CrossingDirection::Left,
                            start: right.position,
                            destination: left.position,
                            cruise_radius: self.cruise,
                            anchor: CrossingAnchor::Vehicle {
                                index: self.vehicle,
                                form: ShipForm::Ship,
                                position: self.center,
                                angle: self.angle,
                            },
                        },
                    });
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Phase {
    Warmup,
    Integrate,
    WorldQuery,
    HullQuery,
    Done,
}
#[derive(Clone)]
pub(crate) struct FlightForecastJob {
    proposal: Proposal,
    initial_environment: FlightEnvironment,
    environment: FlightEnvironment,
    snapshot: Arc<QuerySnapshot>,
    frame_position: Vec2,
    frame_angle: f32,
    capsules: [CapsuleQuery; 2],
    previews: [Preview; 2],
    vehicle: Vec2,
    angle: f32,
    position: Vec2,
    velocity: Vec2,
    reference: Vec2,
    phase: Phase,
    flight_phase: FlightPhase,
    step: usize,
    direction: usize,
    sample: usize,
    samples: usize,
    warmup_left: u64,
    burn: f32,
    query: Vec2,
    query_size: usize,
    estimates: Vec<FlightEstimate>,
    minimum: Vec2,
    maximum: Vec2,
    result: Option<VehicleCrossingForecast>,
    rejection: Option<&'static str>,
}
#[derive(Clone)]
pub(crate) struct FlightScene {
    environment: FlightEnvironment,
    snapshot: Arc<QuerySnapshot>,
    frame_position: Vec2,
    frame_angle: f32,
    capsules: [CapsuleQuery; 2],
    previews: [Preview; 2],
    pub hull: Arc<Vec<Vec2>>,
    pub vehicle: usize,
}
impl FlightScene {
    pub(crate) fn read(
        state: &SurfaceSortieState,
        player: usize,
        p: &pilot::PilotObservationV1,
        snapshot: Arc<QuerySnapshot>,
        frame_position: Vec2,
        frame_angle: f32,
    ) -> Option<Self> {
        let environment = FlightEnvironment::read(state, p)?;
        state.pilots[player].jetpack_charge?;
        let ship = state.replacement_ship(player);
        let spec = SurfaceSortieState::spec();
        let excluded = vec![
            pilot_physics_id(p.owner),
            state
                .world
                .physics
                .surface_vehicle_entity(state.pilots[player].vehicle.0),
        ];
        let previews = [0.02, 0.20].map(|margin| {
            Arc::new(state.world.physics.surface_vehicle_capsule_clearance(
                state.pilots[player].vehicle.0,
                &ship,
                spec.half_segment,
                spec.radius + margin,
            )) as Preview
        });
        Some(Self {
            environment,
            snapshot,
            frame_position,
            frame_angle,
            capsules: [0.02, 0.20].map(|margin| {
                CapsuleQuery::new(
                    spec.half_segment,
                    spec.radius + margin,
                    spec.collision_groups,
                    excluded.clone(),
                )
            }),
            previews,
            hull: Arc::new(physics::ship_collision_hull(&ship)),
            vehicle: state.pilots[player].vehicle.0,
        })
    }
    pub fn proposal(
        &self,
        map: Arc<GroundMap>,
        center: Vec2,
        angle: f32,
        radius: f32,
    ) -> ProposalJob {
        ProposalJob::new(map, center, angle, self.vehicle, &self.hull, radius)
    }
}
impl FlightForecastJob {
    pub(crate) fn new(scene: &FlightScene, proposal: Proposal) -> Self {
        let FlightScene {
            environment,
            snapshot,
            frame_position,
            frame_angle,
            capsules,
            previews,
            ..
        } = scene.clone();
        let CrossingAnchor::Vehicle {
            position: center,
            angle,
            ..
        } = proposal.plan.anchor
        else {
            unreachable!()
        };
        let samples = if environment.time_dependent() { 3 } else { 1 };
        let mut job = Self {
            proposal,
            initial_environment: environment.clone(),
            environment,
            snapshot,
            frame_position,
            frame_angle,
            capsules,
            previews,
            vehicle: frame_position + center.rotate_radians(frame_angle),
            angle: angle + frame_angle,
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            reference: Vec2::ZERO,
            phase: Phase::Integrate,
            flight_phase: FlightPhase::Lift,
            step: 0,
            direction: 0,
            sample: 0,
            samples,
            warmup_left: 0,
            burn: 0.0,
            query: Vec2::ZERO,
            query_size: 0,
            estimates: Vec::new(),
            minimum: proposal.plan.start,
            maximum: proposal.plan.start,
            result: None,
            rejection: None,
        };
        job.launch();
        job
    }
    fn plan(&self) -> CrossingPlan {
        if self.direction == 0 {
            self.proposal.plan
        } else {
            self.proposal.plan.reversed()
        }
    }
    fn launch(&mut self) {
        self.environment.clone_from(&self.initial_environment);
        self.warmup_left = self.launch_delay_ticks();
        self.phase = Phase::Warmup;
    }
    fn begin_flight(&mut self) {
        let start = self.plan().start;
        let time = self.launch_delay();
        let offset = (start + start.normalized() * (spaceling_geometry::HALF_HEIGHT + 0.03))
            .rotate_radians(self.environment.spin * time);
        self.position = self.environment.center() + offset;
        self.reference = self.environment.surface_velocity(offset);
        self.velocity = self.reference
            - self.environment.gravity(self.position).normalized()
                * SurfaceSortieState::spec().jump_speed;
        self.step = 0;
        self.burn = 0.0;
        self.flight_phase = FlightPhase::Lift;
        self.phase = Phase::Integrate;
    }
    fn launch_delay_ticks(&self) -> u64 {
        if self.samples == 1 {
            0
        } else {
            self.sample as u64 * environment::LAUNCH_WINDOW_TICKS / (self.samples - 1) as u64
        }
    }
    fn launch_delay(&self) -> f32 {
        self.launch_delay_ticks() as f32 * DT
    }
    pub(crate) fn area(&self) -> QueryArea {
        let margin = Vec2::new(
            SurfaceSortieState::spec().half_height() + 0.21,
            SurfaceSortieState::spec().half_height() + 0.21,
        );
        QueryArea {
            minimum: self.minimum - margin,
            maximum: self.maximum + margin,
            groups: SurfaceSortieState::spec().collision_groups,
        }
    }
    fn stop(&mut self) {
        self.phase = Phase::Done;
    }
    fn reject(&mut self, reason: &'static str) {
        self.rejection = Some(reason);
        self.stop();
    }
    pub(crate) fn rejection(&self) -> Option<&'static str> {
        self.rejection
    }
}
impl PlanningJob for FlightForecastJob {
    type Output = Option<VehicleCrossingForecast>;
    fn next_work(&self) -> Option<WorkKind> {
        match self.phase {
            Phase::Done => None,
            Phase::Warmup | Phase::Integrate => Some(WorkKind::Graph),
            _ => Some(WorkKind::PhysicsQuery),
        }
    }
    fn output(&self) -> Option<&Self::Output> {
        matches!(self.phase, Phase::Done).then_some(&self.result)
    }
    fn step(&mut self) {
        match self.phase {
            Phase::Done => {}
            Phase::Warmup => {
                if self.warmup_left > 0 {
                    self.environment.advance();
                    self.warmup_left -= 1;
                } else {
                    self.begin_flight();
                }
            }
            Phase::Integrate => {
                let plan = self.plan();
                let elapsed = self.step as f32 * DT;
                let time = self.launch_delay() + elapsed;
                let offset = self.position - self.environment.center();
                let gravity = self.environment.gravity(self.position);
                if self.step >= MAX_STEPS {
                    self.reject("time_limit");
                    return;
                }
                if !gravity.length().is_finite() || gravity.length() < 0.1 {
                    self.reject("invalid_gravity");
                    return;
                }
                let up = -gravity.normalized();
                // This first primitive assumes gravity approximately normal to the retained footing.
                if up.dot(offset.normalized()) < 0.98 {
                    self.reject("gravity_direction");
                    return;
                }
                let right = Vec2::new(up.y, -up.x);
                let surface_velocity = self.environment.surface_velocity(offset);
                let relative = self.velocity - surface_velocity;
                let target = if self.flight_phase == FlightPhase::Lift {
                    plan.start
                } else {
                    plan.destination
                };
                let error =
                    (target.rotate_radians(self.environment.spin * time) - offset).dot(right);
                if self.flight_phase == FlightPhase::Descend
                    && offset.length()
                        <= plan.destination.length() + ground_navigation::standing_height() + 0.12
                {
                    if error.abs() < 0.5
                        && relative.dot(right).abs() < 1.0
                        && relative.length() <= 7.0
                    {
                        let estimate = FlightEstimate {
                            seconds: elapsed,
                            burn_seconds: self.burn,
                            arrival_speed: relative.length(),
                        };
                        if self.estimates.len() <= self.direction {
                            self.estimates.push(estimate);
                        } else {
                            let worst = &mut self.estimates[self.direction];
                            worst.seconds = worst.seconds.max(estimate.seconds);
                            worst.burn_seconds = worst.burn_seconds.max(estimate.burn_seconds);
                            worst.arrival_speed = worst.arrival_speed.max(estimate.arrival_speed);
                        }
                        if self.direction == 0 {
                            self.direction = 1;
                            self.launch();
                        } else if self.sample + 1 < self.samples {
                            self.sample += 1;
                            self.direction = 0;
                            self.launch();
                        } else {
                            self.result = Some(VehicleCrossingForecast {
                                version: 2,
                                measured_tick: self.environment.tick,
                                launch_until_tick: self.environment.tick
                                    + environment::LAUNCH_WINDOW_TICKS,
                                plan: self.proposal.plan,
                                nodes: self.proposal.nodes,
                                flights: [self.estimates[0], self.estimates[1]],
                            });
                            self.stop();
                        }
                    } else {
                        self.reject("arrival_window");
                    }
                    return;
                }
                let sample = FlightSample {
                    radius: offset.length(),
                    radial_speed: relative.dot(up),
                    error,
                    lateral_speed: relative.dot(right),
                    frame_speed: (surface_velocity - self.reference).dot(right),
                    air_speed: motor::AIR_SPEED,
                    supported: false,
                    previous_jump: false,
                };
                flight::advance_phase(&plan, &mut self.flight_phase, sample);
                let command = flight::flight_command(&plan, &mut self.flight_phase, sample);
                let side_delta = (command.horizontal * motor::AIR_SPEED
                    - (self.velocity - self.reference).dot(right))
                .clamp(-motor::AIR_ACCELERATION * DT, motor::AIR_ACCELERATION * DT);
                self.velocity += right * side_delta;
                if command.primary_held && self.step > 0 {
                    let impulse = (motor::RISE_SPEED + gravity.length() * DT
                        - (self.velocity - self.reference).dot(up))
                    .clamp(0.0, motor::THRUST * DT);
                    self.burn += impulse / motor::THRUST;
                    self.velocity += up * impulse;
                }
                if self.burn > (LAUNCH_CHARGE - LANDING_RESERVE) * motor::BURN_SECONDS {
                    self.reject("fuel_reserve");
                    return;
                }
                self.velocity += gravity * DT;
                self.position += self.velocity * DT;
                self.step += 1;
                self.environment.advance();
                let offset = self.position - self.environment.center();
                if offset.length() > plan.cruise_radius + 20.0 {
                    self.reject("height_limit");
                    return;
                }
                self.query = offset.rotate_radians(-self.environment.spin * (time + DT));
                self.query_size = usize::from(
                    offset.length()
                        > plan.start.length().max(plan.destination.length())
                            + corridor_endpoint_height(),
                );
                self.minimum.x = self.minimum.x.min(self.query.x);
                self.minimum.y = self.minimum.y.min(self.query.y);
                self.maximum.x = self.maximum.x.max(self.query.x);
                self.maximum.y = self.maximum.y.max(self.query.y);
                self.phase = Phase::WorldQuery;
            }
            Phase::WorldQuery => {
                if self.capsules[self.query_size].is_clear(
                    &self.snapshot,
                    self.frame_position + self.query.rotate_radians(self.frame_angle),
                    rotation_for_direction(self.query) + self.frame_angle,
                ) {
                    self.phase = Phase::HullQuery;
                } else {
                    self.reject("world_clearance");
                }
            }
            Phase::HullQuery => {
                if (self.previews[self.query_size])(
                    self.frame_position + self.query.rotate_radians(self.frame_angle),
                    rotation_for_direction(self.query) + self.frame_angle,
                    self.vehicle,
                    self.angle,
                ) {
                    self.phase = Phase::Integrate;
                } else {
                    self.reject("hull_clearance");
                }
            }
        }
    }
}

impl SurfaceSortieState {
    /// Experimental prospective sensor. Uses a proposed pose even while aboard;
    /// it never moves the real ship or grants landing/capture permissions.
    pub fn forecast_vehicle_crossing(
        &self,
        player: usize,
        p: &pilot::PilotObservationV1,
        map: &GroundMap,
        center: Vec2,
        angle: f32,
    ) -> Option<VehicleCrossingForecast> {
        if p.tick != self.world.tick
            || !p.queries_ready
            || self.world.physics.material_queries_dirty
            || map.planet != p.planet.index
            || map.revision != p.planet.revision
            || map.tick > p.tick
            || p.tick - map.tick > 30
            || self.pilots[player].owner != p.owner
            || p.ship_form != ShipForm::Ship
        {
            return None;
        }
        let snapshot = Arc::new(self.world.physics.world.query_snapshot());
        let scene = FlightScene::read(
            self,
            player,
            p,
            snapshot,
            p.planet.motion.position,
            p.planet.motion.angle,
        )?;
        let mut proposal = scene.proposal(Arc::new(map.clone()), center, angle, p.planet.radius);
        while proposal.next_work().is_some() {
            proposal.step();
        }
        let mut flight = FlightForecastJob::new(&scene, (*proposal.output()?)?);
        while flight.next_work().is_some() {
            flight.step();
        }
        *flight.output()?
    }
}

impl SurfaceSortieState {
    pub(crate) fn add_vehicle_forecast(
        &self,
        player: usize,
        o: &mut recovery_sensors::RecoveryTaskObservationV1,
    ) {
        let p = &o.flight.pilot;
        if let (Some(jetpack), Some(map)) = (&mut o.jetpack, &o.ground)
            && jetpack.surveyed
            && p.ship_available
            && p.ship_form == ShipForm::Ship
            && self.vehicle_settled(player)
        {
            jetpack.vehicle_forecast = self.forecast_vehicle_crossing(
                player,
                p,
                map,
                (p.ship.position - p.planet.motion.position).rotate_radians(-p.planet.motion.angle),
                p.ship.angle - p.planet.motion.angle,
            );
            jetpack.crossing = jetpack.vehicle_forecast.map(|f| f.plan);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_launch_window_preserves_incremental_work_and_expiry() {
        use engine_core::planning::{JobLimits, JobPoll, PlanningQueue, Work};
        let state =
            SurfaceSortieScenario::init_material_moving_crossing_trial(42, 0, 60.0, 0.015, 0.065);
        let p = state.pilot_observation(0, None);
        let f = p.planet.motion;
        let map = state
            .survey_ground_with_gravity(0, 0, 0..512, false, 18.2)
            .unwrap();
        let scene = FlightScene::read(
            &state,
            0,
            &p,
            Arc::new(state.world.physics.world.query_snapshot()),
            f.position,
            f.angle,
        )
        .unwrap();
        let mut proposal = scene.proposal(
            Arc::new(map),
            (p.ship.position - f.position).rotate_radians(-f.angle),
            p.ship.angle - f.angle,
            p.planet.radius,
        );
        while proposal.next_work().is_some() {
            proposal.step();
        }
        let input = proposal.output().unwrap().unwrap();
        let snapshot = state.world.physics.world.snapshot_bytes().unwrap();
        let mut outcomes = Vec::new();
        for allowance in [
            Work {
                graph: 17,
                physics_queries: 11,
            },
            Work::UNLIMITED,
        ] {
            let job = FlightForecastJob::new(&scene, input);
            let mut queue = PlanningQueue::new(1);
            let token = queue.submit(0, (), JobLimits::default(), job).unwrap();
            let mut charged = Work::default();
            for _ in 0..2000 {
                let report = queue.advance(allowance);
                assert!(
                    report.charged.graph <= allowance.graph
                        && report.charged.physics_queries <= allowance.physics_queries
                );
                charged.graph += report.charged.graph;
                charged.physics_queries += report.charged.physics_queries;
                if matches!(queue.poll(token, &()), JobPoll::Ready(_)) {
                    break;
                }
            }
            let job = queue.job(token).unwrap();
            let forecast = job.output().unwrap().unwrap();
            assert_eq!(job.sample, 2, "both directions at all three launch epochs");
            // Six flights, each with one launch and one terminal integration,
            // plus 0/60/120 warmup ticks in each direction.
            assert!(charged.graph <= 6 * (MAX_STEPS as u32 + 2) + 360);
            assert!(charged.physics_queries <= 12 * MAX_STEPS as u32);
            assert!(
                forecast.valid_at(p.tick)
                    && forecast.valid_at(p.tick + 60)
                    && forecast.valid_at(p.tick + 120)
            );
            assert!(!forecast.valid_at(p.tick - 1) && !forecast.valid_at(p.tick + 121));
            let mut invalid = forecast;
            invalid.launch_until_tick += 1;
            assert!(!invalid.is_valid());
            outcomes.push((forecast, charged, job.area().minimum, job.area().maximum));
        }
        assert_eq!(outcomes[0], outcomes[1]);
        assert_eq!(
            snapshot,
            state.world.physics.world.snapshot_bytes().unwrap()
        );
    }

    #[test]
    fn moving_forecasts_enforce_the_same_fuel_reserve() {
        for radius in [100.0, 128.0] {
            for seat in 0..2 {
                let state = SurfaceSortieScenario::init_material_moving_crossing_trial(
                    42, seat, radius, 0.02, 0.04,
                );
                let p = state.pilot_observation(seat, None);
                let map = state
                    .survey_ground_with_gravity(seat, 0, 0..512, false, 18.2)
                    .unwrap();
                let f = p.planet.motion;
                let scene = FlightScene::read(
                    &state,
                    seat,
                    &p,
                    Arc::new(state.world.physics.world.query_snapshot()),
                    f.position,
                    f.angle,
                )
                .unwrap();
                let mut proposal = scene.proposal(
                    Arc::new(map),
                    (p.ship.position - f.position).rotate_radians(-f.angle),
                    p.ship.angle - f.angle,
                    radius,
                );
                while proposal.next_work().is_some() {
                    proposal.step();
                }
                let p = proposal
                    .output()
                    .unwrap()
                    .expect("measured geometric proposal");
                let mut job = FlightForecastJob::new(&scene, p);
                while job.next_work().is_some() {
                    job.step();
                }
                assert_eq!(job.output(), Some(&None));
                assert_eq!(job.rejection(), Some("fuel_reserve"));
            }
        }
    }
    #[test]
    fn parked_ship_forecast_is_read_only_and_rejects_blocked_landings() {
        for seat in 0..2 {
            let mut state = SurfaceSortieScenario::init_material_jetpack(42, 2);
            for _ in 0..120 {
                SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            }
            let p = state.pilot_observation(seat, None);
            let gravity = FlightEnvironment::read(&state, &p)
                .unwrap()
                .gravity(Vec2::Y * p.planet.radius)
                .length();
            let map = state
                .survey_ground_with_gravity(
                    seat,
                    p.planet.index,
                    0..ground_navigation::GROUND_SAMPLES as u16,
                    false,
                    gravity,
                )
                .unwrap();
            let center =
                (p.ship.position - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
            let result = state.forecast_vehicle_crossing(
                seat,
                &p,
                &map,
                center,
                p.ship.angle - p.planet.motion.angle,
            );
            assert!(result.unwrap().valid_for(&map));
            let before = state.world.physics.world.snapshot_bytes().unwrap();
            assert_eq!(
                result,
                state.forecast_vehicle_crossing(
                    seat,
                    &p,
                    &map,
                    center,
                    p.ship.angle - p.planet.motion.angle
                )
            );
            assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
            // A roof in the measured ascent blocks this geometry even with full fuel.
            use engine_rapier::world::{
                BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
            };
            let plan = result.unwrap().plan;
            let entity = PhysicsId::new(9_000_000);
            let at = p.planet.motion.position
                + (plan.start.normalized() * (plan.cruise_radius - 1.0))
                    .rotate_radians(p.planet.motion.angle);
            assert!(state.world.physics.world.insert_body(
                BodyId::new(entity, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: at,
                    ..Default::default()
                },
                &[ColliderSpec::cuboid(
                    ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                    2.0,
                    2.0
                )]
            ));
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            let p = state.pilot_observation(seat, None);
            let center =
                (p.ship.position - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
            assert!(
                state
                    .forecast_vehicle_crossing(
                        seat,
                        &p,
                        &map,
                        center,
                        p.ship.angle - p.planet.motion.angle
                    )
                    .is_none()
            );
            state.world.physics.material_queries_dirty = true;
            assert!(
                state
                    .forecast_vehicle_crossing(
                        seat,
                        &p,
                        &map,
                        center,
                        p.ship.angle - p.planet.motion.angle
                    )
                    .is_none()
            );
        }
    }
}
