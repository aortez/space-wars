//! Measured flights join the ordinary walk/jump graph; execute its first flight.
use super::*;
use crate::jetpack_crossing::CrossingGoal;
use scenario_spacewars::surface_sortie::{
    ground_navigation::{GroundRoute, GroundRoutes},
    jetpack::{
        CrossingAnchor, MAX_TERRAIN_CROSSINGS, flight::FlightPhase, forecast::VehicleFlightRequest,
    },
};

impl GroundNavigationTask {
    pub fn set_active_flight_checks(&mut self, enabled: bool) {
        self.active_flight_checks = enabled && self.powered_flag;
    }
    pub fn vehicle_flight_request(&self) -> Option<VehicleFlightRequest> {
        self.active_flight_checks
            .then_some(self.active_flight)
            .flatten()
    }
    pub(super) fn route_with_jetpack(
        &self,
        map: &GroundMap,
        foot: Vec2,
        target: Vec2,
        range: f32,
        o: &RecoveryTaskObservationV1,
    ) -> (GroundRoute, Option<CrossingPlan>, Vec2) {
        let p = &o.flight.pilot;
        let hatches = p.boarding_hatches.map(|h| {
            h.map(|point| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle))
        });
        let selected_target = |r: &GroundRoute| {
            if self.telemetry.destination != GroundDestination::Hatch {
                return target;
            }
            r.path
                .last()
                .and_then(|id| map.nodes.iter().find(|n| n.id == *id))
                .and_then(|node| {
                    let center = node.position + node.position.normalized() * HALF_HEIGHT;
                    hatches
                        .into_iter()
                        .flatten()
                        .min_by(|a, b| a.distance_to(center).total_cmp(&b.distance_to(center)))
                })
                .unwrap_or(target)
        };
        let route = |routes: &GroundRoutes<'_>| {
            if self.telemetry.precise_rebuild {
                routes.route(foot, target, 0.01)
            } else if let Some(plan) = self.telemetry.flag_approach.filter(|plan| !plan.reached) {
                // A narrow footing target preserves the selected node even if
                // another footing is already in the flag's interaction radius.
                routes.route(foot, plan.endpoint.position, 0.01)
            } else if self.telemetry.destination == GroundDestination::Hatch {
                routes.route_to_hatches(foot, hatches)
            } else if self.telemetry.destination == GroundDestination::Flag {
                routes.route_to_actor_target(foot, target, range)
            } else {
                routes.route(foot, target, range - 0.6)
            }
        };
        let cost = |r: &GroundRoute| {
            if r.path.is_empty() {
                f32::INFINITY
            } else {
                r.diagnostics.length
                    + r.diagnostics.jumps as f32 * 2.0
                    + r.diagnostics.flights as f32 * 30.0
            }
        };
        let direct = route(&map.routes());
        let direct_target = selected_target(&direct);
        let Some(jetpack) = &o.jetpack else {
            return (direct, None, direct_target);
        };
        if !jetpack.surveyed
            || !jetpack.charge.is_finite()
            || !(0.0..=1.0).contains(&jetpack.charge)
            || jetpack.terrain_crossings.len() > MAX_TERRAIN_CROSSINGS
        {
            return (direct, None, direct_target);
        }
        let mut graph = map.clone();
        let mut flights = Vec::new();
        let approved = jetpack
            .vehicle_forecast
            .filter(|c| c.valid_for(map) && c.valid_at(p.tick));
        let candidates: Vec<_> = if self.powered_flag {
            approved.iter().map(|c| c.plan).collect()
        } else {
            jetpack
                .crossing
                .iter()
                .chain(&jetpack.terrain_crossings)
                .copied()
                .collect()
        };
        for plan in candidates
            .iter()
            .filter(|plan| valid_plan(plan, map))
            .flat_map(|plan| [*plan, plan.reversed()])
        {
            // A measured direct ground edge is cheaper. Keep one unambiguous
            // flight per node pair when a vehicle and gap survey overlap.
            let connected = if self.powered_flag {
                approved.map(|c| {
                    let edges = c.edges();
                    let edge = if plan.direction == c.plan.direction {
                        edges[0]
                    } else {
                        edges[1]
                    };
                    graph.edges.push(edge);
                    (edge.from, edge.to)
                })
            } else {
                graph.connect_jetpack(plan.start, plan.destination)
            };
            if let Some((from, to)) = connected {
                flights.push((from, to, plan));
            }
        }
        if !self.powered_flag
            && let Some(forecast) = jetpack
                .terrain_flight
                .as_ref()
                .and_then(|s| s.forecast)
                .filter(|f| f.valid_for(map, p.tick))
        {
            for (edge, plan) in forecast
                .edges()
                .into_iter()
                .zip([forecast.plan, forecast.plan.reversed()])
            {
                if !graph
                    .edges
                    .iter()
                    .any(|e| e.from == edge.from && e.to == edge.to)
                {
                    graph.edges.push(edge);
                    flights.push((edge.from, edge.to, plan));
                }
            }
        }
        let routes = graph.routes();
        let mut combined = route(&routes);
        if combined.path.is_empty()
            && matches!(
                self.telemetry.destination,
                GroundDestination::Flag | GroundDestination::Hatch
            )
        {
            // Only nearby powered crossings are surveyed. Walk/fly through the
            // known portion, then survey again from its end before continuing.
            combined = if self.telemetry.destination == GroundDestination::Hatch {
                routes.route_toward_hatches(foot, hatches)
            } else {
                routes.route_toward_actor_target(foot, target, range)
            };
        }
        if cost(&combined) + 2.0 < cost(&direct) {
            let combined_target = selected_target(&combined);
            for (i, pair) in combined.path.windows(2).enumerate() {
                if let Some((_, _, plan)) = flights
                    .iter()
                    .find(|(a, b, _)| *a == pair[0] && *b == pair[1])
                {
                    let plan = *plan;
                    combined.path.truncate(i + 1);
                    return (combined, Some(plan), combined_target);
                }
            }
            return (combined, None, combined_target);
        }
        (direct, None, direct_target)
    }

    pub(super) fn follow_crossing(&mut self, o: &RecoveryTaskObservationV1) -> SurfaceSortieAction {
        let p = &o.flight.pilot;
        let task = self.crossing_task.as_mut().unwrap();
        let Some(jetpack) = &o.jetpack else {
            return self.interrupt_crossing(p.tick);
        };
        if !jetpack.charge.is_finite() || !(0.0..=1.0).contains(&jetpack.charge) {
            return self.interrupt_crossing(p.tick);
        }
        let old = *task.telemetry().plan.as_ref().unwrap();
        // This permission belongs to the opt-in forecast edge. Historical
        // low corridors and the offline physical-probe injection keep their
        // existing execution contract.
        let high_terrain = self.terrain_flight_required;
        if high_terrain {
            if jetpack.charge < scenario_spacewars::surface_sortie::jetpack::flight::LANDING_RESERVE
                || self
                    .terrain_launch_tick
                    .is_some_and(|t| p.tick < t || p.tick - t >= 720)
            {
                return self.interrupt_crossing(p.tick);
            }
            if jetpack.surveyed
                && jetpack
                    .terrain_flight
                    .as_ref()
                    .and_then(|s| s.forecast)
                    .is_none_or(|f| {
                        !f.valid_at(p.tick)
                            || f.plan.revision != p.planet.revision
                            || !(old.same_corridor(&f.plan)
                                || old.same_corridor(&f.plan.reversed()))
                    })
            {
                return self.interrupt_crossing(p.tick);
            }
            if self.terrain_launch_tick.is_none()
                && self.terrain_launch_forecast.is_none_or(|f| {
                    !f.valid_at(p.tick)
                        || f.plan.revision != old.revision
                        || !(old.same_corridor(&f.plan) || old.same_corridor(&f.plan.reversed()))
                })
            {
                // A route is not launch permission. Await a current survey at
                // takeoff without extending the ground task's original timer.
                if jetpack.surveyed {
                    return self.interrupt_crossing(p.tick);
                }
                self.telemetry.goal = GroundGoal::Survey;
                return SurfaceSortieAction::default();
            }
        }
        let active = self.vehicle_flight_request();
        if let Some(request) = active {
            if !request.valid_at(p.tick)
                || self
                    .last_flight_check
                    .is_none_or(|tick| p.tick < tick || p.tick - tick > 30)
                || (jetpack.surveyed
                    && jetpack
                        .vehicle_continuation
                        .is_none_or(|c| !c.valid_for(request, p.tick, jetpack.charge)))
            {
                return self.interrupt_crossing(p.tick);
            }
            if jetpack.surveyed {
                self.last_flight_check = Some(p.tick);
            }
        } else if self.powered_flag
            && jetpack.surveyed
            && jetpack.vehicle_forecast.is_none_or(|f| !f.valid_at(p.tick))
        {
            return self.interrupt_crossing(p.tick);
        }
        let mut observation = jetpack.for_crossing(p, &old);
        if active.is_some() && jetpack.surveyed {
            observation.plan = jetpack.vehicle_continuation.and_then(|c| c.plan);
        }
        let task = self.crossing_task.as_mut().unwrap();
        if jetpack.surveyed {
            if observation
                .plan
                .as_ref()
                .is_none_or(|plan| !task.revalidate(plan))
            {
                return self.interrupt_crossing(p.tick);
            }
        } else if old.revision != p.planet.revision {
            // Await a completed survey after destruction, at most half a second.
            // The old corridor no longer authorizes powered travel.
            self.telemetry.goal = GroundGoal::Survey;
            return SurfaceSortieAction::default();
        }
        let action = task.step(&observation);
        let t = task.telemetry().clone();
        if high_terrain && t.goal == CrossingGoal::Lift && action.primary_held {
            self.terrain_launch_tick.get_or_insert(p.tick);
        }
        if self.active_flight_checks {
            let phase = match t.goal {
                CrossingGoal::Lift => Some(FlightPhase::Lift),
                CrossingGoal::Cross => Some(FlightPhase::Cross),
                CrossingGoal::Descend => Some(FlightPhase::Descend),
                _ => None,
            };
            if let Some(phase) = phase {
                if self.active_flight.is_none() && action.primary_held {
                    // Entering Lift releases the approach control first. Start
                    // continuation only with the actual launch press, after the
                    // ordinary launch gate has accepted this tick.
                    let Some(launch) = self.launch_forecast.filter(|f| {
                        f.valid_at(p.tick)
                            && f.plan.revision == old.revision
                            && (old.same_corridor(&f.plan) || old.same_corridor(&f.plan.reversed()))
                    }) else {
                        return self.interrupt_crossing(p.tick);
                    };
                    self.active_flight = Some(VehicleFlightRequest {
                        launch,
                        launched_tick: p.tick,
                        plan: t.plan.unwrap(),
                        phase,
                    });
                    self.last_flight_check = Some(p.tick);
                } else if let Some(request) = self.active_flight.as_mut() {
                    request.phase = phase;
                    request.plan = t.plan.unwrap();
                }
            }
        }
        self.telemetry.crossing = Some(t.clone());
        self.telemetry.last_progress_tick = p.tick;
        self.telemetry.goal = match t.goal {
            CrossingGoal::Recharge => GroundGoal::Recharge,
            CrossingGoal::Approach => GroundGoal::Walk,
            CrossingGoal::Lift => GroundGoal::JetpackLift,
            CrossingGoal::Cross => GroundGoal::JetpackCross,
            CrossingGoal::Descend => GroundGoal::JetpackLand,
            CrossingGoal::Complete => {
                self.telemetry.jetpack_crossings += 1;
                self.crossing_task = None;
                self.terrain_launch_tick = None;
                self.terrain_launch_forecast = None;
                self.active_flight = None;
                self.launch_forecast = None;
                self.last_flight_check = None;
                self.clear_route();
                GroundGoal::Survey
            }
            CrossingGoal::Blocked => return self.interrupt_crossing(p.tick),
            _ => GroundGoal::Survey,
        };
        action
    }

    fn interrupt_crossing(&mut self, tick: u64) -> SurfaceSortieAction {
        self.telemetry.flight_interruptions += 1;
        self.telemetry.invalidations += 1;
        self.crossing_task = None;
        self.terrain_launch_tick = None;
        self.terrain_launch_forecast = None;
        self.active_flight = None;
        self.launch_forecast = None;
        self.last_flight_check = None;
        self.clear_route();
        self.map = None;
        self.settling_after_interrupt = true;
        self.telemetry.goal = GroundGoal::Settle;
        self.telemetry.last_progress_tick = tick;
        if self.telemetry.flight_interruptions >= 3 {
            self.block("three interrupted jetpack routes");
        }
        // Invalid geometry cannot authorize another powered flight. Gravity
        // and contacts resolve the interrupted motion before ground replanning.
        SurfaceSortieAction::default()
    }
}

fn valid_plan(plan: &CrossingPlan, map: &GroundMap) -> bool {
    plan.planet == map.planet
        && plan.revision == map.revision
        && [plan.start, plan.destination]
            .iter()
            .all(|v| v.x.is_finite() && v.y.is_finite())
        && match &plan.anchor {
            CrossingAnchor::Vehicle {
                position, angle, ..
            } => position.x.is_finite() && position.y.is_finite() && angle.is_finite(),
            CrossingAnchor::GroundGap { from, to } => {
                usize::from(*from) < GROUND_SAMPLES && usize::from(*to) < GROUND_SAMPLES
            }
        }
        && plan.cruise_radius.is_finite()
        && plan.start.distance_to(plan.destination) > 1.0
        && plan.start.distance_to(plan.destination) < 30.0
        && plan.cruise_radius > plan.start.length()
        && plan.cruise_radius > plan.destination.length()
        && plan.cruise_radius < plan.start.length().min(plan.destination.length()) + 20.0
}
