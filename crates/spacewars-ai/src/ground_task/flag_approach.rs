//! Bind a joint landing forecast to live on-foot execution.
use super::*;
use scenario_spacewars::surface_sortie::{
    ground_navigation::GroundNode, landing_objective::LandingObjective,
};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FlagApproach {
    pub objective: LandingObjective,
    pub endpoint: GroundNode,
    /// Planet-local entrances at the measurement, not a boarding permission.
    pub boarding_hatches: [Option<Vec2>; 2],
    pub tick: u64,
    pub reached: bool,
}
impl FlagApproach {
    pub fn actor_position(self) -> Vec2 {
        self.endpoint.position + self.endpoint.position.normalized() * HALF_HEIGHT
    }
}

impl GroundNavigationTask {
    pub fn with_flag_approach(context: BrainReset, approach: Option<FlagApproach>) -> Self {
        let mut task = Self::new(context, GroundDestination::Flag);
        task.joint_flag = true;
        task.telemetry.policy = "ground_navigation_v11";
        task.telemetry.flag_approach = approach;
        task
    }

    /// Revalidate on each fresh physical survey. Changing terrain, a moved
    /// hatch or obstruction can replace the endpoint; an old plan never grants
    /// support, capture or transfer permission.
    pub(super) fn planned_flag_target(
        &mut self,
        o: &RecoveryTaskObservationV1,
        foot: Vec2,
    ) -> Option<Option<Vec2>> {
        let p = &o.flight.pilot;
        let Some(objective) = LandingObjective::read(p) else {
            self.telemetry.flag_approach = None;
            return Some(None);
        };
        let local =
            |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let hatches = p.boarding_hatches.map(|h| h.map(local));
        if hatches.iter().all(Option::is_none) {
            self.block("joint flag trip has no grounded hatch");
            return None;
        }
        let Some(map) = &self.map else {
            self.telemetry.goal = GroundGoal::Survey;
            return None;
        };
        let map_tick = map.tick;
        if self.telemetry.flag_approach.is_some_and(|plan| {
            !plan.objective.matches(objective)
                || plan
                    .boarding_hatches
                    .into_iter()
                    .zip(hatches)
                    .any(|(old, new)| match (old, new) {
                        (Some(a), Some(b)) => a.distance_to(b) > 0.5,
                        (None, None) => false,
                        _ => true,
                    })
                || plan.tick > p.tick
                || !map.nodes.iter().any(|n| {
                    n.id == plan.endpoint.id && n.position.distance_to(plan.endpoint.position) < 0.1
                })
                || (map.tick > plan.tick
                    && map
                        .routes()
                        .route_to_hatches(plan.endpoint.position, hatches)
                        .diagnostics
                        .failure
                        .is_some())
        }) {
            self.telemetry.flag_approach = None;
            self.telemetry.invalidations += 1;
            self.clear_route();
        }
        if self.telemetry.flag_approach.is_none() {
            // A blocked trip waits for new measurements, with the ordinary
            // task deadline. Never fall back to an outward-only destination.
            if self
                .missing_since
                .is_some_and(|since| p.tick.saturating_sub(since) > 5 * 60)
            {
                self.block("no complete flag round trip in fresh surveys");
                return None;
            }
            if self.flag_survey_tick == Some(map_tick) {
                return None;
            }
            self.flag_survey_tick = Some(map_tick);
            let trip = self.map.as_ref().unwrap().routes().round_trip_to_hatches(
                foot,
                objective.position,
                objective.range,
                hatches,
            );
            let Some(endpoint) = trip.endpoint else {
                self.telemetry.goal = GroundGoal::Survey;
                self.telemetry.reason = Some("waiting for a complete flag round trip");
                self.telemetry.route = Some(trip.outbound.diagnostics);
                let since = *self.missing_since.get_or_insert(p.tick);
                if p.tick.saturating_sub(since) > 5 * 60 {
                    self.block("no complete flag round trip in fresh surveys");
                }
                return None;
            };
            self.telemetry.flag_approach = Some(FlagApproach {
                objective,
                endpoint,
                boarding_hatches: hatches,
                tick: map_tick,
                reached: false,
            });
            self.clear_route();
            self.missing_since = None;
            self.telemetry.reason = None;
        }
        let plan = self.telemetry.flag_approach.as_mut().unwrap();
        plan.tick = map_tick;
        let target = plan.actor_position();
        if p.supported_planet == Some(p.planet.index)
            && p.actor
                .is_some_and(|actor| local(actor.position).distance_to(target) < 0.45)
        {
            plan.reached = true;
        }
        Some(
            (!plan.reached)
                .then(|| p.planet.motion.position + target.rotate_radians(p.planet.motion.angle)),
        )
    }
}
