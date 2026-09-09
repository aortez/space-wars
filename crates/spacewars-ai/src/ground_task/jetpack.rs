//! Measured flights join the ordinary walk/jump graph; execute its first flight.
use super::*;
use crate::jetpack_crossing::CrossingGoal;
use scenario_spacewars::surface_sortie::{
    ground_navigation::GroundRoute,
    jetpack::{CrossingAnchor, MAX_TERRAIN_CROSSINGS},
};

impl GroundNavigationTask {
    pub(super) fn route_with_jetpack(
        &self,
        map: &GroundMap,
        foot: Vec2,
        target: Vec2,
        range: f32,
        o: &RecoveryTaskObservationV1,
    ) -> (GroundRoute, Option<CrossingPlan>) {
        let route = |map: &GroundMap| {
            if self.telemetry.destination == GroundDestination::Hatch {
                map.route_to_hatch(foot, target)
            } else if self.telemetry.destination == GroundDestination::Flag {
                map.route_to_actor_target(foot, target, range)
            } else {
                map.route(foot, target, range - 0.6)
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
        let direct = route(map);
        let Some(jetpack) = &o.jetpack else {
            return (direct, None);
        };
        if !jetpack.surveyed
            || !jetpack.charge.is_finite()
            || !(0.0..=1.0).contains(&jetpack.charge)
            || jetpack.terrain_crossings.len() > MAX_TERRAIN_CROSSINGS
        {
            return (direct, None);
        }
        let mut graph = map.clone();
        let mut flights = Vec::new();
        for plan in jetpack
            .crossing
            .iter()
            .chain(&jetpack.terrain_crossings)
            .filter(|plan| valid_plan(plan, map))
            .flat_map(|plan| [plan.clone(), plan.reversed()])
        {
            // A measured direct ground edge is cheaper. Keep one unambiguous
            // flight per node pair when a vehicle and gap survey overlap.
            if let Some((from, to)) = graph.connect_jetpack(plan.start, plan.destination) {
                flights.push((from, to, plan));
            }
        }
        let mut combined = route(&graph);
        if combined.path.is_empty()
            && matches!(
                self.telemetry.destination,
                GroundDestination::Flag | GroundDestination::Hatch
            )
        {
            // Only nearby powered crossings are surveyed. Walk/fly through the
            // known portion, then survey again from its end before continuing.
            combined = graph.route_toward_actor_target(foot, target, range);
        }
        if cost(&combined) + 2.0 < cost(&direct) {
            for (i, pair) in combined.path.windows(2).enumerate() {
                if let Some((_, _, plan)) = flights
                    .iter()
                    .find(|(a, b, _)| *a == pair[0] && *b == pair[1])
                {
                    let plan = plan.clone();
                    combined.path.truncate(i + 1);
                    return (combined, Some(plan));
                }
            }
            return (combined, None);
        }
        (direct, None)
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
        let old = task.telemetry().plan.as_ref().unwrap();
        let observation = jetpack.for_crossing(p, old);
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
