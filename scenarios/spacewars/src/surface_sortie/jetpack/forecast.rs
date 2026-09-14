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
const MAX_SOURCES: usize = 32;
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
    pub plan: CrossingPlan,
    pub nodes: [u16; 2],
    /// Both directions start recharged on stable retained ground.
    pub flights: [FlightEstimate; 2],
}
impl VehicleCrossingForecast {
    pub fn edges(self) -> [GroundEdge; 2] {
        crossing_edges(self.nodes, self.plan)
    }
    pub fn is_valid(self) -> bool {
        let plan = self.plan;
        let finite = |p: Vec2| p.x.is_finite() && p.y.is_finite();
        self.version == 1
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

#[derive(Debug, Clone, Copy, PartialEq)]
struct Source {
    position: Vec2,
    velocity: Vec2,
    scale: f32,
    radius: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FlightEnvironment {
    sources: Vec<Source>,
    spin: f32,
}
impl FlightEnvironment {
    pub(crate) fn read(state: &SurfaceSortieState, p: &pilot::PilotObservationV1) -> Option<Self> {
        if state.world.planets.len() + usize::from(state.world.sun.is_some()) > MAX_SOURCES {
            return None;
        }
        let frame = p.planet.motion;
        let mut sources = Vec::new();
        for (i, body) in state.world.planets.iter().enumerate() {
            let motion = motion::SurfaceFrame::read(&state.world.physics, i);
            sources.push(Source {
                position: (motion.position - frame.position).rotate_radians(-frame.angle),
                velocity: (motion.linear_velocity - frame.velocity).rotate_radians(-frame.angle),
                scale: 60.0 * GRAVITY * body.mass,
                radius: if state.world.terrain.planets.contains_key(&i) {
                    body.radius
                } else {
                    0.0
                },
            });
        }
        if let Some(sun) = state.world.sun {
            sources.push(Source {
                position: (sun.position - frame.position).rotate_radians(-frame.angle),
                velocity: -frame.velocity.rotate_radians(-frame.angle),
                scale: 60.0 * GRAVITY * sun.mass,
                radius: 0.0,
            });
        }
        Some(Self {
            sources,
            spin: frame.spin,
        })
    }
    pub(crate) fn compatible(&self, new: &Self) -> bool {
        (self.spin - new.spin).abs() <= 0.0001
            && self.sources.len() == new.sources.len()
            && self.sources.iter().zip(&new.sources).all(|(a, b)| {
                a.position.distance_to(b.position) <= 0.01
                    && a.velocity.distance_to(b.velocity) <= 0.01
                    && a.scale == b.scale
                    && a.radius == b.radius
            })
    }
    fn gravity(&self, point: Vec2, seconds: f32) -> Vec2 {
        self.sources.iter().fold(Vec2::ZERO, |g, source| {
            let d = source.position + source.velocity * seconds - point;
            let r2 = d.length_squared().max(source.radius.powi(2)).max(0.01);
            g + d * (source.scale / (r2 * r2.sqrt()))
        })
    }
}

#[derive(Clone, Copy)]
enum Phase {
    Integrate,
    WorldQuery,
    HullQuery,
    Done,
}
#[derive(Clone)]
pub(crate) struct FlightForecastJob {
    proposal: Proposal,
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
    burn: f32,
    query: Vec2,
    query_size: usize,
    estimates: Vec<FlightEstimate>,
    minimum: Vec2,
    maximum: Vec2,
    result: Option<VehicleCrossingForecast>,
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
        let mut job = Self {
            proposal,
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
            burn: 0.0,
            query: Vec2::ZERO,
            query_size: 0,
            estimates: Vec::new(),
            minimum: proposal.plan.start,
            maximum: proposal.plan.start,
            result: None,
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
        let start = self.plan().start;
        self.position = start + start.normalized() * (spaceling_geometry::HALF_HEIGHT + 0.03);
        self.reference = Vec2::new(-self.position.y, self.position.x) * self.environment.spin;
        self.velocity =
            self.reference + self.position.normalized() * SurfaceSortieState::spec().jump_speed;
        self.step = 0;
        self.burn = 0.0;
        self.flight_phase = FlightPhase::Lift;
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
}
impl PlanningJob for FlightForecastJob {
    type Output = Option<VehicleCrossingForecast>;
    fn next_work(&self) -> Option<WorkKind> {
        match self.phase {
            Phase::Done => None,
            Phase::Integrate => Some(WorkKind::Graph),
            _ => Some(WorkKind::PhysicsQuery),
        }
    }
    fn output(&self) -> Option<&Self::Output> {
        matches!(self.phase, Phase::Done).then_some(&self.result)
    }
    fn step(&mut self) {
        match self.phase {
            Phase::Done => {}
            Phase::Integrate => {
                let plan = self.plan();
                let time = self.step as f32 * DT;
                let gravity = self.environment.gravity(self.position, time);
                if self.step >= MAX_STEPS || !gravity.length().is_finite() || gravity.length() < 0.1
                {
                    self.stop();
                    return;
                }
                let up = -gravity.normalized();
                // This first primitive assumes gravity approximately normal to the retained footing.
                if up.dot(self.position.normalized()) < 0.98 {
                    self.stop();
                    return;
                }
                let right = Vec2::new(up.y, -up.x);
                let surface_velocity =
                    Vec2::new(-self.position.y, self.position.x) * self.environment.spin;
                let relative = self.velocity - surface_velocity;
                let target = if self.flight_phase == FlightPhase::Lift {
                    plan.start
                } else {
                    plan.destination
                };
                let error = (target.rotate_radians(self.environment.spin * time) - self.position)
                    .dot(right);
                if self.flight_phase == FlightPhase::Descend
                    && self.position.length()
                        <= plan.destination.length() + ground_navigation::standing_height() + 0.12
                {
                    if error.abs() < 0.5
                        && relative.dot(right).abs() < 1.0
                        && relative.length() <= 7.0
                    {
                        self.estimates.push(FlightEstimate {
                            seconds: time,
                            burn_seconds: self.burn,
                            arrival_speed: relative.length(),
                        });
                        if self.direction == 0 {
                            self.direction = 1;
                            self.launch();
                        } else {
                            self.result = Some(VehicleCrossingForecast {
                                version: 1,
                                plan: self.proposal.plan,
                                nodes: self.proposal.nodes,
                                flights: [self.estimates[0], self.estimates[1]],
                            });
                            self.stop();
                        }
                    } else {
                        self.stop();
                    }
                    return;
                }
                let sample = FlightSample {
                    radius: self.position.length(),
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
                    self.stop();
                    return;
                }
                self.velocity += gravity * DT;
                self.position += self.velocity * DT;
                self.step += 1;
                if self.position.length() > plan.cruise_radius + 20.0 {
                    self.stop();
                    return;
                }
                self.query = self
                    .position
                    .rotate_radians(-self.environment.spin * self.step as f32 * DT);
                self.query_size = usize::from(
                    self.position.length()
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
                    self.stop();
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
                    self.stop();
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
    fn parked_ship_forecast_is_read_only_and_rejects_blocked_landings() {
        for seat in 0..2 {
            let mut state = SurfaceSortieScenario::init_material_jetpack(42, 2);
            for _ in 0..120 {
                SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            }
            let p = state.pilot_observation(seat, None);
            let gravity = FlightEnvironment::read(&state, &p)
                .unwrap()
                .gravity(Vec2::Y * p.planet.radius, 0.0)
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
