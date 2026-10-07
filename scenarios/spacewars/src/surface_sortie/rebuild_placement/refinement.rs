//! Opt-in local refinement after the ordinary sparse survey finds no site.
use super::*;
use ground_navigation::{GroundEdgeKind, GroundNode};

pub(super) const EXTRA_OFFSETS: [f32; 22] = [
    -8.5, -9.0, -9.5, -10.0, -10.5, -11.0, -11.5, -12.0, -12.5, -13.0, -13.5, 8.5, 9.0, 9.5, 10.0,
    10.5, 11.0, 11.5, 12.0, 12.5, 13.0, 13.5,
];
const MAX_REFINED_CANDIDATES: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RebuildRefinementWork {
    pub coarse_candidates: usize,
    pub refined_candidates: usize,
    pub offset_checks: usize,
}

pub(super) fn promising(report: &RebuildPlacementReport) -> bool {
    report.attempts.iter().any(|a| {
        matches!(
            a.rejection,
            Some(
                RebuildRejection::NoHatchFooting
                    | RebuildRejection::NoHatchRoute
                    | RebuildRejection::HatchRouteTooLong
            )
        )
    })
}

fn neighbors<'a>(map: &'a GroundMap, foot: Vec2, seeds: &[&GroundNode]) -> Vec<&'a GroundNode> {
    let mut queues = seeds
        .iter()
        .map(|seed| {
            let mut nearby = map
                .nodes
                .iter()
                .filter(|node| {
                    node.position.distance_to(seed.position) <= 2.0
                        && node.position.distance_to(foot) <= MAX_REBUILD_WALK
                })
                .collect::<Vec<_>>();
            nearby.sort_by(|a, b| {
                a.position
                    .distance_to(seed.position)
                    .total_cmp(&b.position.distance_to(seed.position))
                    .then(a.id.cmp(&b.id))
            });
            nearby.into_iter()
        })
        .collect::<Vec<_>>();
    let mut result: Vec<&GroundNode> = Vec::new();
    loop {
        let before = result.len();
        for queue in &mut queues {
            if let Some(node) = queue.find(|node| result.iter().all(|old| old.id != node.id)) {
                result.push(node);
                if result.len() == MAX_REFINED_CANDIDATES {
                    return result;
                }
            }
        }
        if result.len() == before {
            return result;
        }
    }
}

impl SurfaceSortieState {
    /// Experimental search density only. Native placement/landing guards and
    /// recovery timers remain authoritative, including for actual builds.
    #[cfg(feature = "sensor-profile")]
    pub fn set_rebuild_refinement(&mut self, player: usize, enabled: bool) -> bool {
        let Some(pilot) = self.pilots.get_mut(player) else {
            return false;
        };
        pilot.rebuild_refinement = enabled;
        true
    }

    pub(super) fn refine_rebuild_survey(
        &self,
        player: usize,
        map: &GroundMap,
        base: &GroundMap,
        seeds: &[&GroundNode],
        survey: &mut RebuildRelocationSurvey,
    ) {
        let actor = self.spaceling_snapshot(player).unwrap();
        let frame = motion::SurfaceFrame::read(&self.world.physics, map.planet);
        let foot = (actor.motion.position - actor.up * Self::spec().half_height() - frame.position)
            .rotate_radians(-frame.angle);
        for node in neighbors(map, foot, seeds) {
            survey.refinement.as_mut().unwrap().refined_candidates += 1;
            let route = map.route(foot, node.position, 0.01);
            survey.attempts.push(RebuildRelocationAttempt {
                bearing: node.id,
                route: Some(route.diagnostics.clone()),
                placement: None,
            });
            if route.diagnostics.failure.is_some() || route.diagnostics.length > MAX_REBUILD_WALK {
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
                hatch_walk_length: selected.route.as_ref().unwrap().length,
            });
            break;
        }
        let work = survey.refinement.as_ref().unwrap();
        assert!(
            work.coarse_candidates <= 8
                && work.refined_candidates <= 8
                && work.offset_checks <= 240
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighborhood_work_is_bounded_unique_and_distributed_between_seeds() {
        let map = GroundMap {
            version: 1,
            actor: PlayerId::PLAYER_1,
            planet: 0,
            revision: 0,
            tick: 0,
            nodes: (0..40)
                .map(|id| GroundNode {
                    id,
                    position: Vec2::new(id as f32 * 0.2, 0.0),
                    normal: Vec2::Y,
                })
                .collect(),
            edges: Vec::new(),
            rejected: Vec::new(),
        };
        let selected = neighbors(&map, Vec2::ZERO, &[&map.nodes[5], &map.nodes[30]]);
        assert_eq!(selected.len(), 8);
        assert_eq!(selected[0].id, 5);
        assert_eq!(selected[1].id, 30);
        for (index, node) in selected.iter().enumerate() {
            assert!(selected[..index].iter().all(|old| old.id != node.id));
            assert!(
                [5, 30]
                    .iter()
                    .any(|&seed| node.position.distance_to(map.nodes[seed].position) <= 2.0)
            );
        }
        assert!(neighbors(&map, Vec2::new(100.0, 0.0), &[&map.nodes[5]]).is_empty());
    }
}
