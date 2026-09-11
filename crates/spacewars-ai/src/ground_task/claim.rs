//! Relocate only when real planet contact cannot supply a valid flag anchor.
use super::*;
use scenario_spacewars::surface_sortie::{
    PlanetClaimStatus,
    claim_footing::{CLAIM_SEARCH_RADIUS, MAX_CLAIM_FOOTINGS},
};

#[derive(Debug, Clone, Default)]
pub(super) struct ClaimRelocation {
    invalid_since: Option<u64>,
    started: Option<u64>,
    site: Option<(u64, Vec2)>,
    settled_since: Option<u64>,
    tried: Vec<Vec2>,
}

impl GroundNavigationTask {
    /// None means wait (or blocked); Some preserves or replaces the world target.
    pub(super) fn claim_target(
        &mut self,
        o: &RecoveryTaskObservationV1,
        original: Option<Vec2>,
        foot: Vec2,
    ) -> Option<Option<Vec2>> {
        let p = &o.flight.pilot;
        let Some(claim) = &p.planet.claim else {
            return Some(original);
        };
        let local =
            |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let actor = p.actor.expect("ground task checked actor availability");
        let active = self.claim_relocation.started.is_some();
        if matches!(
            claim.status,
            PlanetClaimStatus::Ready
                | PlanetClaimStatus::Lowering
                | PlanetClaimStatus::Raising
                | PlanetClaimStatus::Contested
                | PlanetClaimStatus::Secured
        ) {
            if active {
                self.clear_route();
                self.telemetry.claim_target = None;
            }
            self.claim_relocation = ClaimRelocation::default();
            return Some(original);
        }
        if self
            .telemetry
            .started_tick
            .is_some_and(|start| p.tick - start > 90 * 60)
        {
            self.block("ground traversal exceeded ninety seconds");
            return None;
        }
        if self
            .claim_relocation
            .started
            .is_some_and(|tick| p.tick - tick > 12 * 60)
        {
            self.block("unable to find valid claim footing within twelve seconds");
            return None;
        }
        if p.supported_planet != Some(p.planet.index) {
            self.claim_relocation.invalid_since = None;
            self.claim_relocation.site = None;
            self.telemetry.claim_target = None;
            if active || original.is_none() {
                self.telemetry.goal = GroundGoal::Settle;
                return None;
            }
            return Some(original);
        }
        if !active {
            if claim.status != PlanetClaimStatus::NeedSupport
                || original.is_some_and(|target| actor.position.distance_to(target) > 2.8)
            {
                self.claim_relocation.invalid_since = None;
                return Some(original);
            }
            let since = *self.claim_relocation.invalid_since.get_or_insert(p.tick);
            self.telemetry.goal = GroundGoal::Settle;
            if p.tick - since < 30 {
                return None;
            }
            self.claim_relocation.started = Some(p.tick);
            self.clear_route();
        }
        if self
            .claim_relocation
            .site
            .is_some_and(|(revision, _)| revision != p.planet.revision)
        {
            self.claim_relocation.site = None;
            self.claim_relocation.settled_since = None;
            self.telemetry.claim_target = None;
            self.clear_route();
        }
        if let Some((_, position)) = self.claim_relocation.site {
            if local(actor.position).distance_to(position) < 0.45 {
                let since = *self.claim_relocation.settled_since.get_or_insert(p.tick);
                if p.tick - since < 2 * 60 {
                    self.telemetry.goal = GroundGoal::Settle;
                    return None;
                }
                self.claim_relocation.site = None;
                self.claim_relocation.settled_since = None;
                self.telemetry.claim_target = None;
                self.clear_route();
            } else {
                self.claim_relocation.settled_since = None;
            }
        }
        if self.claim_relocation.site.is_none() {
            self.telemetry.goal = GroundGoal::Survey;
            if self.claim_relocation.tried.len() >= 4 {
                self.block("four claim footing proposals failed actual support checks");
                return None;
            }
            let (Some(survey), Some(map)) = (&o.claim_footing, &self.map) else {
                return None;
            };
            if survey.version != 1
                || survey.owner != p.owner
                || survey.planet != p.planet.index
                || survey.revision != p.planet.revision
                || survey.tick != p.tick
                || map.tick != p.tick
                || survey.positions.len() > MAX_CLAIM_FOOTINGS
                || survey.positions.iter().any(|position| {
                    !position.x.is_finite()
                        || !position.y.is_finite()
                        || position.distance_to(local(actor.position)) > CLAIM_SEARCH_RADIUS + 0.1
                        || !map.nodes.iter().any(|node| {
                            (node.position + node.position.normalized() * 0.9)
                                .distance_to(*position)
                                < 0.01
                        })
                })
            {
                self.block("claim footing identity/version or geometry mismatch");
                return None;
            }
            let choice = survey
                .positions
                .iter()
                .filter(|position| {
                    self.claim_relocation
                        .tried
                        .iter()
                        .all(|old| old.distance_to(**position) > 0.75)
                })
                .filter_map(|position| {
                    let route = map.route_to_actor_target(foot, *position, 0.45);
                    (!route.path.is_empty() && route.diagnostics.length <= 12.0).then_some((
                        *position,
                        route.diagnostics.length + route.diagnostics.jumps as f32 * 2.0,
                    ))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let (position, _) = choice?;
            self.claim_relocation.site = Some((p.planet.revision, position));
            self.claim_relocation.tried.push(position);
            self.telemetry.claim_target = Some(position);
            self.telemetry.claim_relocations += 1;
            self.telemetry.last_progress_tick = p.tick;
        }
        let (_, position) = self.claim_relocation.site?;
        Some(Some(
            p.planet.motion.position + position.rotate_radians(p.planet.motion.angle),
        ))
    }
}
