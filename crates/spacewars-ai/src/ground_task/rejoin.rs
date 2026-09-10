//! A supported actor may fit under a hull where the upright route graph cannot.
//! Use only fresh, short contact-tangent sweeps until it can rejoin that graph.
use super::*;

#[derive(Debug, Clone)]
pub(super) struct GroundRejoin {
    began: u64,
    origin: Vec2,
    target: Vec2,
    direction: Option<f32>,
    step: Option<(u64, u64, Vec2, CrawlStep)>,
}

impl GroundRejoin {
    pub(super) fn new(began: u64, origin: Vec2, target: Vec2) -> Self {
        Self {
            began,
            origin,
            target,
            direction: None,
            step: None,
        }
    }
}

impl GroundNavigationTask {
    pub(super) fn rejoin_ground(&mut self, o: &RecoveryTaskObservationV1) -> SurfaceSortieAction {
        let p = &o.flight.pilot;
        let actor = p.actor.unwrap();
        let position =
            (actor.position - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let attempt = self.rejoin.as_mut().unwrap();
        let mut action = SurfaceSortieAction::default();
        self.telemetry.goal = GroundGoal::Rejoin;
        if p.tick - attempt.began > 8 * 60 || position.distance_to(attempt.origin) > 6.0 {
            if self.telemetry.destination == GroundDestination::Hatch {
                self.telemetry.return_failure = Some(ShipReturnFailure::NoStandingRoute);
            }
            self.block("unable to rejoin measured ground within eight seconds or six units");
            return action;
        }
        let Some(posture) = &o.posture else {
            return action;
        };
        if posture.version != 1
            || posture.owner != p.owner
            || posture.planet != p.planet.index
            || posture.revision != p.planet.revision
            || posture.tick != p.tick
            || posture.crawl.iter().enumerate().any(|(i, step)| {
                step.is_some_and(|step| {
                    step.direction != if i == 0 { -1.0 } else { 1.0 }
                        || !step.position.x.is_finite()
                        || !step.position.y.is_finite()
                        || step.position.distance_to(position) > CRAWL_DISTANCE + 0.1
                })
            })
        {
            self.block("invalid ground rejoin corridor");
            return action;
        }
        if !p.queries_ready
            || p.supported_planet != Some(p.planet.index)
            || !posture.stable
            || posture.balance != SpacelingBalance::Balanced
        {
            return action;
        }
        let retained = attempt
            .step
            .filter(|(tick, revision, origin, step)| {
                let delta = step.position - *origin;
                let along = (position - *origin).dot(delta.normalized());
                p.tick - tick < 15
                    && *revision == p.planet.revision
                    && along >= -0.05
                    && along < delta.length() - 0.03
                    && (position - (*origin + delta.normalized() * along)).length() < 0.15
            })
            .map(|(_, _, _, step)| step);
        if let Some(step) = posture
            .crawl
            .iter()
            .flatten()
            .find(|step| Some(step.direction) == attempt.direction)
            .or_else(|| {
                posture.crawl.iter().flatten().min_by(|a, b| {
                    a.position
                        .distance_to(attempt.target)
                        .total_cmp(&b.position.distance_to(attempt.target))
                })
            })
            .copied()
            .or(retained)
        {
            if retained != Some(step) {
                attempt.step = Some((p.tick, p.planet.revision, position, step));
            }
            attempt.direction = Some(step.direction);
            action.horizontal = step.direction;
        }
        action
    }
}
