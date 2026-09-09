//! A measured ship crossing is an optional leg in the ordinary ground route.
use super::*;
use crate::jetpack_crossing::CrossingGoal;
use scenario_spacewars::surface_sortie::ground_navigation::GroundRoute;

impl GroundNavigationTask {
    pub(super) fn route_with_jetpack(
        &self,
        map: &GroundMap,
        foot: Vec2,
        target: Vec2,
        range: f32,
        o: &RecoveryTaskObservationV1,
    ) -> (GroundRoute, Option<CrossingPlan>) {
        let route = |start| {
            if self.telemetry.destination == GroundDestination::Hatch {
                map.route_to_hatch(start, target)
            } else {
                map.route(start, target, range - 0.6)
            }
        };
        let cost = |r: &GroundRoute| {
            if r.path.is_empty() {
                f32::INFINITY
            } else {
                r.diagnostics.length + r.diagnostics.jumps as f32 * 2.0
            }
        };
        let mut best = route(foot);
        let mut best_cost = cost(&best);
        let mut crossing = None;
        if let Some(jetpack) = &o.jetpack
            && jetpack.surveyed
            && jetpack.charge.is_finite()
            && (0.0..=1.0).contains(&jetpack.charge)
            && let Some(plan) = &jetpack.crossing
            && valid_plan(plan, map)
        {
            for plan in [plan.clone(), plan.reversed()] {
                let approach = map.route(foot, plan.start, 1.4);
                let finish = route(plan.destination);
                // Flight needs lift, descent and possibly four seconds of
                // charging. Prefer the existing short walking route when usable.
                let flight_cost = plan.start.distance_to(plan.destination) + 30.0;
                let candidate_cost = cost(&approach) + flight_cost + cost(&finish);
                if candidate_cost + 2.0 < best_cost {
                    best_cost = candidate_cost;
                    best = approach;
                    crossing = Some(plan);
                }
            }
        }
        (best, crossing)
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
        let observation = jetpack.for_crossing(p, task.direction());
        let old = task.telemetry().plan.as_ref().unwrap();
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
        && [plan.start, plan.destination, plan.ship_position]
            .iter()
            .all(|v| v.x.is_finite() && v.y.is_finite())
        && plan.ship_angle.is_finite()
        && plan.cruise_radius.is_finite()
        && plan.start.distance_to(plan.destination) > 1.0
        && plan.start.distance_to(plan.destination) < 30.0
        && plan.cruise_radius > plan.start.length()
        && plan.cruise_radius > plan.destination.length()
        && plan.cruise_radius < plan.start.length().min(plan.destination.length()) + 20.0
}
