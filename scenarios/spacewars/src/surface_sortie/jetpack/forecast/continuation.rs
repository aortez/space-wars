//! Recheck one already launched maneuver from its actual state, never a new launch.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct VehicleFlightRequest {
    pub launch: VehicleCrossingForecast,
    pub launched_tick: u64,
    pub plan: CrossingPlan,
    pub phase: FlightPhase,
}
impl VehicleFlightRequest {
    pub fn valid_at(self, tick: u64) -> bool {
        self.launch.valid_at(self.launched_tick)
            && tick
                .checked_sub(self.launched_tick)
                .is_some_and(|age| age < MAX_STEPS as u64)
            && self.plan.revision == self.launch.plan.revision
            && (self.plan.same_corridor(&self.launch.plan)
                || self.plan.same_corridor(&self.launch.plan.reversed()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct VehicleFlightContinuation {
    pub version: u32,
    pub tick: u64,
    pub request: VehicleFlightRequest,
    pub charge: f32,
    pub plan: Option<CrossingPlan>,
    pub remaining: Option<FlightEstimate>,
    pub rejection: Option<&'static str>,
}
impl VehicleFlightContinuation {
    pub fn valid_for(self, request: VehicleFlightRequest, tick: u64, charge: f32) -> bool {
        self.version == 1
            && self.tick == tick
            && self.request == request
            && request.valid_at(tick)
            && self.charge == charge
            && self.charge.is_finite()
            && (LANDING_RESERVE..=1.0).contains(&self.charge)
            && self.rejection.is_none()
            && self.plan.is_some_and(|p| {
                p.revision == request.plan.revision && request.plan.same_corridor(&p)
            })
            && self.remaining.is_some_and(|f| {
                f.seconds.is_finite()
                    && f.seconds >= 0.0
                    && f.seconds <= (MAX_STEPS as u64 - (tick - request.launched_tick)) as f32 * DT
                    && f.burn_seconds.is_finite()
                    && f.burn_seconds >= 0.0
                    && f.burn_seconds <= (charge - LANDING_RESERVE) * motor::BURN_SECONDS
                    && f.arrival_speed.is_finite()
                    && (0.0..=7.0).contains(&f.arrival_speed)
            })
    }
}

#[derive(Clone, Copy)]
pub(super) struct ContinuationLimits {
    pub steps: usize,
    pub burn_seconds: f32,
}

impl SurfaceSortieState {
    /// Current physics and remaining charge validate a single active direction.
    /// This result cannot authorize launch, recharge, reversal or another flight.
    pub fn vehicle_flight_continuation(
        &self,
        player: usize,
        o: &recovery_sensors::RecoveryTaskObservationV1,
        request: VehicleFlightRequest,
    ) -> VehicleFlightContinuation {
        #[cfg(feature = "sensor-profile")]
        let _profile =
            super::super::super::sensor_profile::Scope::new("vehicle_flight_continuation");
        let p = &o.flight.pilot;
        let charge = o.jetpack.as_ref().map_or(f32::NAN, |j| j.charge);
        let mut result = VehicleFlightContinuation {
            version: 1,
            tick: self.world.tick,
            request,
            charge,
            plan: None,
            remaining: None,
            rejection: None,
        };
        let prediction = || -> Result<_, &'static str> {
            if !request.valid_at(p.tick) {
                return Err("flight_identity_or_deadline");
            }
            if p.tick != self.world.tick
                || self.pilots[player].owner != p.owner
                || p.location != PilotLocation::OnFoot
                || !p.queries_ready
                || self.world.physics.material_queries_dirty
                || request.plan.planet != p.planet.index
                || request.plan.revision != p.planet.revision
                || !p.ship_available
                || p.ship_form != ShipForm::Ship
                || !self.vehicle_settled(player)
            {
                return Err("current_context");
            }
            let jetpack = o.jetpack.as_ref().ok_or("equipment")?;
            if !jetpack.surveyed
                || !charge.is_finite()
                || !(LANDING_RESERVE..=1.0).contains(&charge)
            {
                return Err("equipment_or_survey");
            }
            let actor = p.actor.ok_or("actor")?;
            let finite = |v: Vec2| v.x.is_finite() && v.y.is_finite();
            if !finite(actor.position)
                || !finite(actor.velocity)
                || !finite(jetpack.reference_velocity)
            {
                return Err("actor_motion");
            }
            let map = o.ground.as_ref().ok_or("ground_unavailable")?;
            if map.tick != p.tick
                || map.planet != p.planet.index
                || map.revision != p.planet.revision
            {
                return Err("ground_identity");
            }
            let reverse = request.plan.direction != request.launch.plan.direction;
            let ids = if reverse {
                [request.launch.nodes[1], request.launch.nodes[0]]
            } else {
                request.launch.nodes
            };
            let nodes = [0, 1].map(|i| map.nodes.iter().find(|n| n.id == ids[i]).copied());
            let [Some(start), Some(destination)] = nodes else {
                return Err("endpoints_unavailable");
            };
            if start.position.distance_to(request.plan.start) >= 0.01
                || destination.position.distance_to(request.plan.destination) >= 0.01
            {
                return Err("endpoints_changed");
            }
            let frame = p.planet.motion;
            let plan = CrossingPlan {
                start: start.position,
                destination: destination.position,
                anchor: CrossingAnchor::Vehicle {
                    index: self.pilots[player].vehicle.0,
                    form: ShipForm::Ship,
                    position: (p.ship.position - frame.position).rotate_radians(-frame.angle),
                    angle: p.ship.angle - frame.angle,
                },
                ..request.plan
            };
            if !request.plan.same_corridor(&plan) {
                return Err("vehicle_changed");
            }
            let snapshot = Arc::new(self.world.physics.world.query_snapshot());
            let scene = FlightScene::read(self, player, p, snapshot, frame.position, frame.angle)
                .ok_or("flight_environment")?;
            let proposal = Proposal {
                plan,
                nodes: ids,
                measured_nodes: [start, destination],
            };
            let mut job = FlightForecastJob::new(&scene, proposal);
            job.continuation = Some(ContinuationLimits {
                steps: MAX_STEPS - (p.tick - request.launched_tick) as usize,
                burn_seconds: (charge - LANDING_RESERVE) * motor::BURN_SECONDS,
            });
            job.position = (actor.position - frame.position).rotate_radians(-frame.angle);
            job.velocity = actor.velocity.rotate_radians(-frame.angle);
            job.reference = jetpack.reference_velocity.rotate_radians(-frame.angle);
            job.flight_phase = request.phase;
            job.phase = Phase::Integrate;
            job.minimum = job.position;
            job.maximum = job.position;
            // As with launch prediction, every integrated position gets both
            // clearance queries. Check the existing arrival window first: the
            // current actor may already be touching its measured landing floor.
            while job.next_work().is_some() {
                job.step();
            }
            job.continuation_result
                .map(|f| (plan, f))
                .ok_or(job.rejection().unwrap_or("unfinished"))
        };
        match prediction() {
            Ok((plan, remaining)) => {
                result.plan = Some(plan);
                result.remaining = Some(remaining);
            }
            Err(reason) => result.rejection = Some(reason),
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use engine_rapier::world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
    };

    #[test]
    fn continuation_is_read_only_and_rejects_new_geometry_and_invalid_source() {
        let dt = std::time::Duration::from_nanos(16_666_667);
        let mut state = SurfaceSortieScenario::init_material_jetpack(42, 2);
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        let p = state.pilot_observation(0, None);
        let map = state
            .survey_ground_with_gravity(0, p.planet.index, 0..512, false, 18.2)
            .unwrap();
        let launch = state
            .forecast_vehicle_crossing(
                0,
                &p,
                &map,
                (p.ship.position - p.planet.motion.position).rotate_radians(-p.planet.motion.angle),
                p.ship.angle - p.planet.motion.angle,
            )
            .unwrap();
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(p.owner)],
            dt,
        );
        // Release transfer controls and let the real actor settle before launch.
        for _ in 0..29 {
            SurfaceSortieScenario::step(
                &mut state,
                &[SurfaceSortieAction::default().encode(p.owner)],
                dt,
            );
        }
        let first = state.pilot_observation(0, None);
        assert!(first.controls_armed && first.balanced);
        let local = (first.actor.unwrap().position - first.planet.motion.position)
            .rotate_radians(-first.planet.motion.angle);
        let plan =
            if local.distance_to(launch.plan.start) < local.distance_to(launch.plan.destination) {
                launch.plan
            } else {
                launch.plan.reversed()
            };
        let mut previous_jump = false;
        for _ in 0..60 {
            let p = state.pilot_observation(0, None);
            let actor = p.actor.unwrap();
            let pack = state.jetpack_navigation_observation(0).unwrap();
            let right = Vec2::new(p.actor_up.y, -p.actor_up.x);
            let offset = actor.position - p.planet.motion.position;
            let relative = actor.velocity - p.planet.velocity_at(actor.position);
            let action = flight::flight_command(
                &plan,
                &mut FlightPhase::Lift,
                FlightSample {
                    radius: offset.length(),
                    radial_speed: relative.dot(p.actor_up),
                    error: (plan.start.rotate_radians(p.planet.motion.angle) - offset).dot(right),
                    lateral_speed: relative.dot(right),
                    frame_speed: (p.planet.velocity_at(actor.position) - pack.reference_velocity)
                        .dot(right),
                    air_speed: motor::AIR_SPEED,
                    supported: p.supported_planet.is_some(),
                    previous_jump,
                },
            );
            previous_jump = action.primary_held;
            SurfaceSortieScenario::step(&mut state, &[action.encode(p.owner)], dt);
        }
        let o = state.recovery_task_observation(0, None);
        let request = VehicleFlightRequest {
            launch,
            launched_tick: 150,
            plan,
            phase: FlightPhase::Lift,
        };
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let good = state.vehicle_flight_continuation(0, &o, request);
        assert!(
            good.valid_for(
                request,
                state.world.tick,
                o.jetpack.as_ref().unwrap().charge
            ),
            "{good:?}"
        );
        assert_eq!(state.vehicle_flight_continuation(0, &o, request), good);
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
        for bad in [
            VehicleFlightRequest {
                launched_tick: state.world.tick + 1,
                ..request
            },
            VehicleFlightRequest {
                launched_tick: launch.launch_until_tick + 1,
                ..request
            },
            VehicleFlightRequest {
                plan: CrossingPlan {
                    revision: request.plan.revision + 1,
                    ..request.plan
                },
                ..request
            },
        ] {
            assert_eq!(
                state.vehicle_flight_continuation(0, &o, bad).rejection,
                Some("flight_identity_or_deadline")
            );
        }
        assert!(!request.valid_at(request.launched_tick + MAX_STEPS as u64));
        let mut moved = o.clone();
        moved.flight.pilot.ship.position.x += 2.0;
        assert_eq!(
            state
                .vehicle_flight_continuation(0, &moved, request)
                .rejection,
            Some("vehicle_changed")
        );
        let entity = PhysicsId::new(9_000_000);
        assert!(
            state.world.physics.world.insert_body(
                BodyId::new(entity, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: o.flight.pilot.planet.motion.position
                        + ((plan.start + plan.destination).normalized() * plan.cruise_radius)
                            .rotate_radians(o.flight.pilot.planet.motion.angle),
                    ..Default::default()
                },
                &[ColliderSpec::cuboid(
                    ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                    2.0,
                    2.0
                )]
            )
        );
        // Flush the new collider through the real physics step, then explicitly
        // complete a fresh survey at this tick for the collision regression.
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction::default().encode(p.owner)],
            dt,
        );
        let mut blocked = state.recovery_task_observation(0, None);
        blocked.ground = state.survey_ground(0, blocked.flight.pilot.planet.index, 0..512, false);
        blocked.jetpack.as_mut().unwrap().surveyed = true;
        assert_eq!(
            state
                .vehicle_flight_continuation(0, &blocked, request)
                .rejection,
            Some("world_clearance")
        );
    }
}
