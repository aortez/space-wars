//! Controller-owned search history and a bounded walking step toward a preview.
use super::*;
use ground_navigation::{GroundEdgeKind, GroundNode, GroundRoute};

pub const MAX_STAGING_WALK: f32 = 4.0;
const MIN_STAGING_WALK: f32 = 2.0;
const SEARCH_LIFETIME: u64 = 5 * 60;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RebuildSearchProgress {
    pub planet: usize,
    pub revision: u64,
    pub origin: Vec2,
    pub started_tick: u64,
    pub visited: Vec<u16>,
    /// Recheck this measured target after a real staging arrival. It grants no
    /// placement or route validity in the new survey.
    pub preferred: Option<u16>,
    /// Explicit current request to remeasure a bearing even if history expires
    /// or terrain changes. Consumed by one bounded survey; not a valid site.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub recheck_preferred: bool,
    /// Include the measured walking map when a staging move is proposed.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub include_staging_map: bool,
}

impl RebuildSearchProgress {
    fn refreshed(&self, map: &GroundMap, foot: Vec2) -> Self {
        let current = self.planet == map.planet
            && self.revision == map.revision
            && map.tick >= self.started_tick
            && map.tick - self.started_tick <= SEARCH_LIFETIME;
        let valid = self.visited.len() <= GROUND_SAMPLES
            && self
                .visited
                .iter()
                .all(|&id| usize::from(id) < GROUND_SAMPLES);
        if current && valid && self.origin.distance_to(foot) <= 0.5 {
            return self.clone();
        }
        Self {
            planet: map.planet,
            revision: map.revision,
            origin: foot,
            started_tick: map.tick,
            visited: Vec::new(),
            preferred: self.preferred.filter(|&id| {
                usize::from(id) < GROUND_SAMPLES
                    && (current
                        || (self.recheck_preferred
                            && self.planet == map.planet
                            && map.tick >= self.started_tick))
            }),
            recheck_preferred: self.recheck_preferred,
            include_staging_map: self.include_staging_map,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RebuildStagingProposal {
    pub planet: usize,
    pub revision: u64,
    pub position: Vec2,
    pub walk_length: f32,
    pub target_bearing: u16,
    pub target_position: Vec2,
    pub remaining_length: f32,
    pub hatch_walk_length: f32,
}

pub(super) fn staging_step(
    map: &GroundMap,
    foot: Vec2,
    target: &GroundNode,
    route: &GroundRoute,
    checks: &mut usize,
) -> Option<RebuildStagingProposal> {
    let length = route.diagnostics.length;
    if route.diagnostics.failure.is_some()
        || !(MAX_REBUILD_WALK..=MAX_REBUILD_WALK + MAX_STAGING_WALK).contains(&length)
    {
        return None;
    }
    // Leave some margin on the second leg rather than depending on exact
    // endpoint rounding. Each proposed leg is then queried independently.
    let required = MIN_STAGING_WALK.max(length - MAX_REBUILD_WALK + 0.25);
    let mut walked = 0.0;
    for pair in route.path.windows(2) {
        let edge = map
            .edges
            .iter()
            .find(|e| e.from == pair[0] && e.to == pair[1])?;
        if edge.kind != GroundEdgeKind::Walk {
            return None;
        }
        walked += edge.length;
        if walked > MAX_STAGING_WALK {
            return None;
        }
        if walked < required {
            continue;
        }
        let node = map.nodes.iter().find(|n| n.id == pair[1])?;
        *checks += 2;
        let first = map.route(foot, node.position, 0.01);
        let second = map.route(node.position, target.position, 0.01);
        if first.diagnostics.failure.is_some()
            || first.diagnostics.jumps != 0
            || first.diagnostics.flights != 0
            || !(MIN_STAGING_WALK..=MAX_STAGING_WALK).contains(&first.diagnostics.length)
            || second.diagnostics.failure.is_some()
            || second.diagnostics.length > MAX_REBUILD_WALK
        {
            return None;
        }
        return Some(RebuildStagingProposal {
            planet: map.planet,
            revision: map.revision,
            position: node.position,
            walk_length: first.diagnostics.length,
            target_bearing: target.id,
            target_position: target.position,
            remaining_length: second.diagnostics.length,
            hatch_walk_length: 0.0,
        });
    }
    None
}

impl SurfaceSortieState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn progressive_rebuild_survey(
        &self,
        player: usize,
        map: &GroundMap,
        base: &GroundMap,
        seeds: &[&GroundNode],
        previous: &RebuildSearchProgress,
        survey: &mut RebuildRelocationSurvey,
    ) {
        let actor = self.spaceling_snapshot(player).unwrap();
        let frame = motion::SurfaceFrame::read(&self.world.physics, map.planet);
        let foot = (actor.motion.position - actor.up * Self::spec().half_height() - frame.position)
            .rotate_radians(-frame.angle);
        let mut search = previous.refreshed(map, foot);
        search.recheck_preferred = false;
        let preferred = search.preferred.take().and_then(|id| {
            map.nodes
                .iter()
                .find(|n| n.id == id && n.position.distance_to(foot) <= MAX_REBUILD_WALK)
        });
        let mut candidates = preferred.into_iter().collect::<Vec<_>>();
        for node in refinement::neighbors(map, foot, seeds, &search.visited) {
            if candidates.iter().all(|old| old.id != node.id) {
                candidates.push(node);
            }
            if candidates.len() == 8 {
                break;
            }
        }
        for node in candidates {
            if !search.visited.contains(&node.id) {
                search.visited.push(node.id);
            }
            survey.refinement.as_mut().unwrap().refined_candidates += 1;
            let route = map.route(foot, node.position, 0.01);
            survey.attempts.push(RebuildRelocationAttempt {
                bearing: node.id,
                route: Some(route.diagnostics.clone()),
                placement: None,
            });
            if route.diagnostics.failure.is_some()
                || route.diagnostics.length > MAX_REBUILD_WALK + MAX_STAGING_WALK
            {
                continue;
            }
            survey.checked += 1;
            let (pose, report) = self.find_rebuild_placement(
                player,
                map.planet,
                frame.position + node.position.rotate_radians(frame.angle),
                node.normal.rotate_radians(frame.angle),
                Some(base),
            );
            survey.refinement.as_mut().unwrap().offset_checks += report.attempts.len();
            survey.attempts.last_mut().unwrap().placement = Some(report.clone());
            if pose.is_none() {
                continue;
            }
            let selected = report
                .attempts
                .iter()
                .find(|a| Some(a.offset) == report.selected_offset)
                .unwrap();
            let hatch_length = selected.route.as_ref().unwrap().length;
            if route.diagnostics.length > MAX_REBUILD_WALK {
                if survey.staging.is_none() {
                    survey.staging = staging_step(
                        map,
                        foot,
                        node,
                        &route,
                        &mut survey.refinement.as_mut().unwrap().staging_route_checks,
                    );
                    if let Some(stage) = &mut survey.staging {
                        stage.hatch_walk_length = hatch_length;
                    }
                }
                // A directly reachable placement later in this bounded batch
                // takes precedence over the staged proposal.
                continue;
            }
            let flight_length = route
                .path
                .windows(2)
                .filter_map(|pair| {
                    map.edges.iter().find(|e| {
                        e.from == pair[0] && e.to == pair[1] && e.kind == GroundEdgeKind::Jetpack
                    })
                })
                .map(|e| e.length)
                .sum::<f32>();
            survey.site = Some(RebuildStandingSite {
                precise: true,
                planet: map.planet,
                revision: map.revision,
                position: node.position,
                walk_length: (route.diagnostics.length - flight_length).max(0.0),
                flight_length,
                jetpack_flights: route.diagnostics.flights,
                hatch_walk_length: hatch_length,
            });
            survey.staging = None;
            break;
        }
        survey.search = Some(search);
        let work = survey.refinement.as_ref().unwrap();
        assert!(
            work.coarse_candidates <= 8
                && work.refined_candidates <= 8
                && work.offset_checks <= 240
                && work.staging_route_checks <= 16
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ground_navigation::GroundEdge;

    fn map() -> GroundMap {
        GroundMap {
            version: 1,
            actor: PlayerId::PLAYER_1,
            planet: 3,
            revision: 4,
            tick: 100,
            nodes: (0..=26)
                .map(|id| GroundNode {
                    id,
                    position: Vec2::new(f32::from(id), 20.0),
                    normal: Vec2::Y,
                })
                .collect(),
            edges: (0..26)
                .map(|from| GroundEdge {
                    from,
                    to: from + 1,
                    kind: GroundEdgeKind::Walk,
                    length: 1.0,
                })
                .collect(),
            rejected: Vec::new(),
        }
    }

    #[test]
    fn search_advances_without_changing_measurements_and_restarts_on_new_geometry() {
        let mut map = map();
        let foot = map.nodes[0].position;
        let seeds = [&map.nodes[4], &map.nodes[12]];
        let first = refinement::neighbors(&map, foot, &seeds, &[]);
        let previous = RebuildSearchProgress {
            visited: first.iter().map(|n| n.id).collect(),
            ..RebuildSearchProgress::default().refreshed(&map, foot)
        };
        let next = refinement::neighbors(&map, foot, &seeds, &previous.visited);
        assert!(!next.is_empty());
        assert!(next.iter().all(|n| !previous.visited.contains(&n.id)));
        assert_eq!(previous.refreshed(&map, foot), previous);
        assert_eq!(previous.clone().refreshed(&map, foot), previous);
        assert!(previous.refreshed(&map, foot + Vec2::X).visited.is_empty());
        let mut preferred = previous.clone();
        preferred.preferred = Some(25);
        preferred.include_staging_map = true;
        assert!(
            preferred
                .refreshed(&map, foot + Vec2::X)
                .include_staging_map
        );
        assert_eq!(
            preferred.refreshed(&map, foot + Vec2::X).preferred,
            Some(25)
        );
        map.revision += 1;
        assert!(previous.refreshed(&map, foot).visited.is_empty());
        assert_eq!(preferred.refreshed(&map, foot).preferred, None);
        assert!(preferred.refreshed(&map, foot).include_staging_map);
        map.revision -= 1;
        map.tick += SEARCH_LIFETIME + 1;
        assert!(previous.refreshed(&map, foot).visited.is_empty());
    }

    #[test]
    fn explicit_footing_recheck_discards_old_geometry_but_keeps_a_bounded_query_hint() {
        let mut map = map();
        let foot = map.nodes[0].position;
        let prior = RebuildSearchProgress {
            preferred: Some(25),
            recheck_preferred: true,
            visited: vec![25, 24],
            ..RebuildSearchProgress::default().refreshed(&map, foot)
        };
        map.revision += 1;
        map.tick += SEARCH_LIFETIME + 1;
        let fresh = prior.refreshed(&map, foot + Vec2::X);
        assert_eq!(fresh.preferred, Some(25));
        assert!(fresh.visited.is_empty());
        assert_eq!(fresh.started_tick, map.tick);
        assert_eq!(fresh.origin, foot + Vec2::X);
        assert_eq!(fresh.revision, map.revision);
        // The same id can only request a new route and placement; the previous
        // history remains unmodified and carries no validity into this map.
        assert_eq!(prior.visited, vec![25, 24]);
        let mut invalid = prior.clone();
        invalid.preferred = Some(GROUND_SAMPLES as u16);
        assert_eq!(invalid.refreshed(&map, foot).preferred, None);
        invalid = prior.clone();
        invalid.planet += 1;
        assert_eq!(invalid.refreshed(&map, foot).preferred, None);
        invalid = prior.clone();
        invalid.started_tick = map.tick + 1;
        assert_eq!(invalid.refreshed(&map, foot).preferred, None);
        invalid = prior;
        invalid.recheck_preferred = false;
        assert_eq!(invalid.refreshed(&map, foot).preferred, None);
    }

    #[test]
    fn staging_requires_two_bounded_routes_and_a_walk_only_first_leg() {
        let mut map = map();
        let foot = map.nodes[0].position;
        let target = map.nodes[25];
        let route = map.route(foot, target.position, 0.01);
        let mut checks = 0;
        let stage = staging_step(&map, foot, &target, &route, &mut checks).unwrap();
        assert_eq!(checks, 2);
        assert_eq!(stage.walk_length, 2.0);
        assert_eq!(stage.remaining_length, 23.0);
        assert_eq!(stage.position, map.nodes[2].position);
        map.edges[0].kind = GroundEdgeKind::Jump;
        assert!(staging_step(&map, foot, &target, &route, &mut checks).is_none());
        map.edges[0].kind = GroundEdgeKind::Walk;
        map.edges.retain(|e| e.from != 5);
        assert!(staging_step(&map, foot, &target, &route, &mut checks).is_none());
    }
}
