//! Bounded recovery from a blocked standing attempt, using ordinary walking.
use super::*;

impl GroundNavigationTask {
    pub(super) fn recover_posture(
        &mut self,
        o: &RecoveryTaskObservationV1,
        target_local: Option<Vec2>,
    ) -> SurfaceSortieAction {
        let p = &o.flight.pilot;
        let actor = p.actor.expect("ground task checked actor availability");
        let local =
            |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let mut action = SurfaceSortieAction::default();
        self.telemetry.goal = GroundGoal::GetUp;
        self.telemetry.crawl_target = None;
        if self
            .crawl_started
            .is_some_and(|since| p.tick - since > 8 * 60)
            || self
                .crawl_origin
                .is_some_and(|origin| local(actor.position).distance_to(origin) > 6.0)
        {
            self.block("unable to crawl clear for standing");
            return action;
        }
        if let Some(posture) = &o.posture {
            if posture.version != 1
                || posture.owner != p.owner
                || posture.planet != p.planet.index
                || posture.revision != p.planet.revision
                || posture.tick != p.tick
            {
                self.block("ground posture identity/version mismatch");
                return action;
            }
            if posture.crawl.iter().enumerate().any(|(i, step)| {
                step.is_some_and(|step| {
                    step.direction != if i == 0 { -1.0 } else { 1.0 }
                        || !step.position.x.is_finite()
                        || !step.position.y.is_finite()
                        || step.position.distance_to(local(actor.position)) > CRAWL_DISTANCE + 0.1
                })
            }) {
                self.block("invalid crawl corridor geometry");
                return action;
            }
            let eligible = posture.stable
                && posture.balance == SpacelingBalance::Recovering
                && posture.get_up_result == SpacelingGetUpResult::Blocked;
            // Finish at most one second of measured travel while the body
            // turns against its contacts. Fresh support and terrain identity
            // remain mandatory; an unavailable next probe cannot renew it.
            let retained = self
                .crawl_step
                .filter(|(tick, revision, step)| {
                    eligible
                        && p.tick - tick < 60
                        && *revision == p.planet.revision
                        && local(actor.position).distance_to(step.position) > 0.15
                })
                .map(|(_, _, step)| step);
            let choice = retained.or_else(|| {
                posture
                    .crawl
                    .iter()
                    .flatten()
                    .filter(|_| eligible)
                    .find(|step| Some(step.direction) == self.telemetry.crawl_direction)
                    .or_else(|| {
                        posture
                            .crawl
                            .iter()
                            .flatten()
                            .filter(|_| eligible)
                            .min_by(|a, b| {
                                let target = target_local.unwrap_or(local(actor.position));
                                a.position
                                    .distance_to(target)
                                    .total_cmp(&b.position.distance_to(target))
                            })
                    })
                    .copied()
            });
            if let Some(step) = choice {
                if retained.is_none() {
                    self.crawl_step = Some((p.tick, p.planet.revision, step));
                }
                self.crawl_started.get_or_insert(p.tick);
                self.crawl_origin.get_or_insert(local(actor.position));
                if self.telemetry.crawl_direction != Some(step.direction) {
                    self.telemetry.get_up_repositions += 1;
                }
                self.telemetry.crawl_direction = Some(step.direction);
                self.telemetry.crawl_target = Some(step.position);
                self.telemetry.goal = GroundGoal::Crawl;
                action.horizontal = step.direction;
            }
        }
        action.primary_held = !self.was_jumping && self.jump_tick.is_none_or(|t| p.tick - t > 60);
        if action.primary_held {
            self.jump_tick = Some(p.tick);
        }
        action
    }
}
