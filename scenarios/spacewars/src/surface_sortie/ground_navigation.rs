//! Bounded measurements of surviving outer ground and spaceling clearance.
//! The AI chooses routes. Sensors never edit material or advance physics.
use super::*;

pub const GROUND_SAMPLES: usize = 512;
pub const GROUND_NEIGHBOR_SPAN: usize = 6;
pub const GROUND_REFRESH_TICKS: u64 = 30;
pub const HATCH_APPROACH_RANGE: f32 = BOARDING_RANGE - 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GroundNode {
    pub id: u16,
    pub position: Vec2,
    pub normal: Vec2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundEdgeKind {
    Walk,
    Jump,
    /// Added only to the AI's route graph from a measured flight corridor.
    Jetpack,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GroundEdge {
    pub from: u16,
    pub to: u16,
    pub kind: GroundEdgeKind,
    pub length: f32,
}
/// Positions and normals are in the retained planet body's local frame.
/// This profile samples its outer contour, not caves or arbitrary mining routes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroundMap {
    pub version: u32,
    pub actor: PlayerId,
    pub planet: usize,
    pub revision: u64,
    pub tick: u64,
    pub nodes: Vec<GroundNode>,
    pub edges: Vec<GroundEdge>,
    pub rejected: Vec<GroundRejectedNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundNodeRejection {
    NoRetainedFloor,
    SteepFloor,
    CapsuleObstructed,
    ReplacementObstructed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GroundRejectedNode {
    pub id: u16,
    pub reason: GroundNodeRejection,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundRouteFailure {
    NoStartFooting,
    NoDestinationFooting,
    Disconnected,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroundRouteDiagnostics {
    pub failure: Option<GroundRouteFailure>,
    /// A measured advance toward a destination beyond the current connections.
    pub partial: bool,
    pub start_node: Option<u16>,
    pub start_distance: Option<f32>,
    pub destination_nodes: usize,
    pub nearest_destination_distance: Option<f32>,
    pub reachable_nodes: usize,
    pub closest_reachable_distance: Option<f32>,
    pub length: f32,
    pub jumps: usize,
    pub flights: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroundRoute {
    pub path: Vec<u16>,
    pub diagnostics: GroundRouteDiagnostics,
}

pub(super) fn standing_height() -> f32 {
    let spec = SurfaceSortieState::spec();
    spec.half_segment + (spec.radius + 0.04) / spec.min_support_alignment + 0.08
}

fn trace_route(
    route: &mut GroundRoute,
    parents: &[Option<(u16, f32, GroundEdgeKind)>; GROUND_SAMPLES],
    end: usize,
) {
    route.path.push(end as u16);
    let mut cursor = end;
    while let Some((previous, length, kind)) = parents[cursor] {
        route.path.push(previous);
        route.diagnostics.length += length;
        route.diagnostics.jumps += usize::from(kind == GroundEdgeKind::Jump);
        route.diagnostics.flights += usize::from(kind == GroundEdgeKind::Jetpack);
        cursor = usize::from(previous);
    }
    route.path.reverse();
}

impl GroundMap {
    /// Join matching surveyed endpoints. The caller must supply a physically
    /// measured corridor; this graph operation performs no world query or move.
    pub fn connect_jetpack(&mut self, start: Vec2, destination: Vec2) -> Option<(u16, u16)> {
        let nearest = |point: Vec2| {
            self.nodes
                .iter()
                .filter(|n| n.position.distance_to(point) < 1.4)
                .min_by(|a, b| {
                    a.position
                        .distance_to(point)
                        .total_cmp(&b.position.distance_to(point))
                })
                .map(|n| n.id)
        };
        let (from, to) = (nearest(start)?, nearest(destination)?);
        if from == to || self.edges.iter().any(|e| e.from == from && e.to == to) {
            return None;
        }
        self.edges.push(GroundEdge {
            from,
            to,
            kind: GroundEdgeKind::Jetpack,
            length: start.distance_to(destination),
        });
        Some((from, to))
    }

    /// Bounded shortest measured route. Absence is evidence about this survey,
    /// not proof that a human cannot traverse the physical terrain.
    pub fn route(&self, start: Vec2, target: Vec2, range: f32) -> GroundRoute {
        self.route_with_height(start, target, range, 0.0, false)
    }

    /// Boarding measures the supported actor's center against the hatch. This
    /// includes nearby lower footing; climbing onto the hatch ray's hit is not
    /// required by the human transfer rule.
    pub fn route_to_hatch(&self, start: Vec2, target: Vec2) -> GroundRoute {
        self.route_to_actor_target(start, target, HATCH_APPROACH_RANGE)
    }

    /// Route using a standing-center envelope. Hatch transfer measures this
    /// center; claims still independently check the real supported flag anchor.
    pub fn route_to_actor_target(&self, start: Vec2, target: Vec2, range: f32) -> GroundRoute {
        self.route_with_height(
            start,
            target,
            range,
            SurfaceSortieState::spec().half_height(),
            false,
        )
    }

    /// Advance through measured ground when distant connections are unknown.
    /// A partial route never establishes arrival or authorizes an unmeasured edge.
    pub fn route_toward_actor_target(&self, start: Vec2, target: Vec2, range: f32) -> GroundRoute {
        self.route_with_height(
            start,
            target,
            range,
            SurfaceSortieState::spec().half_height(),
            true,
        )
    }

    fn route_with_height(
        &self,
        start: Vec2,
        target: Vec2,
        range: f32,
        height: f32,
        allow_partial: bool,
    ) -> GroundRoute {
        let destination_distance = |node: &GroundNode| {
            (node.position + node.position.normalized() * height).distance_to(target)
        };
        let nearest = self.nodes.iter().min_by(|a, b| {
            a.position
                .distance_to(start)
                .total_cmp(&b.position.distance_to(start))
        });
        let mut result = GroundRoute {
            path: Vec::new(),
            diagnostics: GroundRouteDiagnostics {
                failure: None,
                partial: false,
                start_node: nearest.map(|n| n.id),
                start_distance: nearest.map(|n| n.position.distance_to(start)),
                destination_nodes: self
                    .nodes
                    .iter()
                    .filter(|n| destination_distance(n) < range)
                    .count(),
                nearest_destination_distance: self
                    .nodes
                    .iter()
                    .map(destination_distance)
                    .min_by(f32::total_cmp),
                reachable_nodes: 0,
                closest_reachable_distance: None,
                length: 0.0,
                jumps: 0,
                flights: 0,
            },
        };
        let Some(initial) = nearest.filter(|n| n.position.distance_to(start) < 3.0) else {
            result.diagnostics.failure = Some(GroundRouteFailure::NoStartFooting);
            return result;
        };
        if result.diagnostics.destination_nodes == 0 && !allow_partial {
            result.diagnostics.failure = Some(GroundRouteFailure::NoDestinationFooting);
            return result;
        }
        let mut costs = [f32::INFINITY; GROUND_SAMPLES];
        let mut parent: [Option<(u16, f32, GroundEdgeKind)>; GROUND_SAMPLES] =
            [None; GROUND_SAMPLES];
        let mut visited = [false; GROUND_SAMPLES];
        // Surface arc distance still measures progress near the opposite side
        // of a planet, where a useful walk barely changes straight-line distance.
        let remaining = |point: Vec2| {
            (point.x * target.y - point.y * target.x)
                .atan2(point.dot(target))
                .abs()
                * target.length()
        };
        let mut frontier = usize::from(initial.id);
        let mut frontier_distance = remaining(initial.position);
        let initial_distance = frontier_distance;
        costs[usize::from(initial.id)] = 0.0;
        for _ in 0..GROUND_SAMPLES {
            let Some(index) = (0..GROUND_SAMPLES)
                .filter(|&i| !visited[i] && costs[i].is_finite())
                .min_by(|&a, &b| costs[a].total_cmp(&costs[b]))
            else {
                break;
            };
            visited[index] = true;
            result.diagnostics.reachable_nodes += 1;
            let node = self
                .nodes
                .iter()
                .find(|n| usize::from(n.id) == index)
                .unwrap();
            let distance = destination_distance(node);
            if remaining(node.position) < frontier_distance {
                frontier = index;
                frontier_distance = remaining(node.position);
            }
            result.diagnostics.closest_reachable_distance = Some(
                result
                    .diagnostics
                    .closest_reachable_distance
                    .map_or(distance, |old| old.min(distance)),
            );
            if distance < range {
                trace_route(&mut result, &parent, index);
                return result;
            }
            for edge in self.edges.iter().filter(|e| usize::from(e.from) == index) {
                let next = usize::from(edge.to);
                let cost = costs[index]
                    + edge.length
                    + match edge.kind {
                        GroundEdgeKind::Walk => 0.0,
                        GroundEdgeKind::Jump => 2.0,
                        GroundEdgeKind::Jetpack => 30.0,
                    };
                if cost < costs[next] {
                    costs[next] = cost;
                    parent[next] = Some((index as u16, edge.length, edge.kind));
                }
            }
        }
        if allow_partial && frontier_distance < initial_distance - 1.5 {
            result.diagnostics.partial = true;
            trace_route(&mut result, &parent, frontier);
        } else {
            result.diagnostics.failure = Some(if result.diagnostics.destination_nodes == 0 {
                GroundRouteFailure::NoDestinationFooting
            } else {
                GroundRouteFailure::Disconnected
            });
        }
        result
    }

    /// Filter a measured route against the proposed ship's real hull and feet.
    pub(super) fn avoiding(&self, gravity: f32, clear: impl Fn(Vec2) -> bool) -> Self {
        let mut map = self.clone();
        map.nodes.retain(|node| {
            let ok = clear(node.position + node.position.normalized() * standing_height());
            if !ok {
                map.rejected.push(GroundRejectedNode {
                    id: node.id,
                    reason: GroundNodeRejection::ReplacementObstructed,
                });
            }
            ok
        });
        let mut nodes = [None; GROUND_SAMPLES];
        for node in &map.nodes {
            nodes[usize::from(node.id)] = Some(*node);
        }
        let jump_height = SurfaceSortieState::spec().jump_speed.powi(2) / (2.0 * gravity.max(1.0));
        map.edges.retain(|edge| {
            let (Some(a), Some(b)) = (nodes[usize::from(edge.from)], nodes[usize::from(edge.to)])
            else {
                return false;
            };
            (0..=8).all(|sample| {
                let t = sample as f32 / 8.0;
                let foot = a.position + (b.position - a.position) * t;
                clear(
                    foot + foot.normalized()
                        * (standing_height()
                            + if edge.kind == GroundEdgeKind::Jump {
                                4.0 * t * (1.0 - t) * jump_height * 0.85
                            } else {
                                0.0
                            }),
                )
            })
        });
        map
    }
}

impl SurfaceSortieState {
    pub fn ground_navigation_map(&self, player: usize) -> Option<GroundMap> {
        if !(self.world.tick + player as u64 * 15).is_multiple_of(GROUND_REFRESH_TICKS)
            || self.location(player) != PilotLocation::OnFoot
            || self.world.physics.material_queries_dirty
        {
            return None;
        }
        self.survey_ground(
            player,
            self.motion_planet_index(player),
            0..GROUND_SAMPLES as u16,
            false,
        )
    }

    /// Local rebuild previews use the same measurements at a bounded subset of
    /// bearings. The old pod is absent only in the proposed replacement world.
    pub(super) fn survey_ground(
        &self,
        player: usize,
        planet: usize,
        bearings: impl Iterator<Item = u16>,
        replacing: bool,
    ) -> Option<GroundMap> {
        if self.world.physics.material_queries_dirty {
            return None;
        }
        let terrain = self.world.terrain.planets.get(&planet)?;
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let spec = Self::spec();
        // On a staircase the support normal and gravity-relative capsule axis
        // differ. Reserve the capsule's full projected foot radius on slopes.
        let standing_height = standing_height();
        let radius = self.world.planets[planet].radius;
        let world_point = |point: Vec2| frame.position + point.rotate_radians(frame.angle);
        let mut excluded = vec![pilot_physics_id(self.pilots[player].owner)];
        if replacing {
            excluded.push(
                self.world
                    .physics
                    .surface_vehicle_entity(self.pilots[player].vehicle.0),
            );
        }
        let capsule_clear = self.world.physics.world.capsule_clearance_test_excluding(
            spec.half_segment,
            spec.radius + 0.02,
            spec.collision_groups,
            excluded,
        );
        let clear = |point: Vec2| {
            let up = point.normalized();
            capsule_clear(
                world_point(point),
                rotation_for_direction(up.rotate_radians(frame.angle)),
            )
        };
        let mut nodes = Vec::new();
        let mut rejected = Vec::new();
        for id in bearings {
            let up =
                Vec2::Y.rotate_radians(id as f32 * std::f32::consts::TAU / GROUND_SAMPLES as f32);
            let world_up = up.rotate_radians(frame.angle);
            let Some(hit) = self.world.physics.material_ground_ray(
                planet,
                frame.position + world_up * (radius + 8.0),
                -world_up,
                radius + 8.0,
            ) else {
                rejected.push(GroundRejectedNode {
                    id,
                    reason: GroundNodeRejection::NoRetainedFloor,
                });
                continue;
            };
            if hit.normal.dot(world_up) < spec.min_support_alignment {
                rejected.push(GroundRejectedNode {
                    id,
                    reason: GroundNodeRejection::SteepFloor,
                });
                continue;
            }
            let position = (hit.point - frame.position).rotate_radians(-frame.angle);
            if !clear(position + up * standing_height) {
                rejected.push(GroundRejectedNode {
                    id,
                    reason: GroundNodeRejection::CapsuleObstructed,
                });
                continue;
            }
            nodes.push(GroundNode {
                id,
                position,
                normal: hit.normal.rotate_radians(-frame.angle),
            });
        }
        let gravity = self.pilots[player].gravity.length().max(1.0);
        let jump_height = spec.jump_speed.powi(2) / (2.0 * gravity);
        let mut nodes_by_id = [None; GROUND_SAMPLES];
        for node in &nodes {
            nodes_by_id[usize::from(node.id)] = Some(node);
        }
        let mut edges = Vec::new();
        for a in &nodes {
            for direction in [-1_i32, 1] {
                for span in 1..=GROUND_NEIGHBOR_SPAN {
                    let id = (i32::from(a.id) + direction * span as i32)
                        .rem_euclid(GROUND_SAMPLES as i32) as u16;
                    let Some(b) = nodes_by_id[usize::from(id)] else {
                        continue;
                    };
                    let offset = b.position - a.position;
                    let length = offset.length();
                    let up = (a.position + b.position).normalized();
                    let rise = offset.dot(up);
                    if rise > jump_height * 0.75 || rise < -2.5 {
                        continue;
                    }
                    let path_clear = |jump: bool| {
                        (0..=8).all(|sample| {
                            let t = sample as f32 / 8.0;
                            let foot = a.position + offset * t;
                            clear(
                                foot + foot.normalized()
                                    * (standing_height
                                        + if jump {
                                            4.0 * t * (1.0 - t) * jump_height * 0.85
                                        } else {
                                            0.0
                                        }),
                            )
                        })
                    };
                    let continuous_floor = || {
                        (1..4).all(|sample| {
                            let point = a.position + offset * (sample as f32 / 4.0);
                            let up = point.normalized().rotate_radians(frame.angle);
                            self.world
                                .physics
                                .material_ground_ray(
                                    planet,
                                    world_point(point) + up * 0.4,
                                    -up,
                                    0.75,
                                )
                                .is_some_and(|hit| hit.normal.dot(up) >= spec.min_support_alignment)
                        })
                    };
                    let walk =
                        rise.abs() < 0.3 && span == 1 && path_clear(false) && continuous_floor();
                    let kind = if walk {
                        GroundEdgeKind::Walk
                    } else if length <= spec.walk_speed * (2.0 * spec.jump_speed / gravity) * 0.9
                        && path_clear(true)
                    {
                        GroundEdgeKind::Jump
                    } else {
                        continue;
                    };
                    edges.push(GroundEdge {
                        from: a.id,
                        to: b.id,
                        kind,
                        length,
                    });
                    // Walk to the next footing before considering a jump.
                    // Testing every longer shortcut along clear floor multiplies
                    // capsule queries without adding a necessary connection.
                    if walk {
                        break;
                    }
                }
            }
        }
        Some(GroundMap {
            version: 1,
            actor: self.pilots[player].owner,
            planet,
            revision: terrain.field.revision(),
            tick: self.world.tick,
            nodes,
            edges,
            rejected,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::impact::RecoveryDisruption;
    use super::*;

    #[test]
    fn ground_map_measures_retained_material_without_writing_or_flushing_queries() {
        let mut state = SurfaceSortieScenario::init_material(42, 1);
        let dt = Duration::from_nanos(16_666_667);
        assert!(
            state.ground_navigation_map(0).is_none(),
            "no survey while aboard"
        );
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1)],
            dt,
        );
        for _ in 0..59 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        assert_eq!(state.location(0), PilotLocation::OnFoot);
        let before = state.observation(0);
        let bodies = state.world.physics.world.body_count();
        let map = state.ground_navigation_map(0).unwrap();
        assert_eq!(state.ground_navigation_map(0), Some(map.clone()));
        assert_eq!(state.observation(0), before);
        assert_eq!(state.world.physics.world.body_count(), bodies);
        assert_eq!(map.tick, state.world.tick);
        assert_eq!(map.actor, PlayerId::PLAYER_1);
        assert!(!map.nodes.is_empty() && map.nodes.len() <= GROUND_SAMPLES);
        assert!(
            !map.edges.is_empty() && map.edges.len() <= GROUND_SAMPLES * GROUND_NEIGHBOR_SPAN * 2
        );
        let frame = motion::SurfaceFrame::read(&state.world.physics, map.planet);
        for node in &map.nodes {
            let point = frame.position + node.position.rotate_radians(frame.angle);
            // A normal ray through an exact staircase vertex may graze its
            // neighbor. Probe inward along the sample bearing at that corner.
            let normal = node.position.normalized().rotate_radians(frame.angle);
            let hit = state
                .world
                .physics
                .material_ground_ray(map.planet, point + normal * 0.1, -normal, 0.2)
                .unwrap_or_else(|| panic!("waypoint {}: {point:?}, normal {normal:?}", node.id));
            assert!(hit.point.distance_to(point) < 0.01);
        }
        for edge in &map.edges {
            assert!(map.nodes.iter().any(|n| n.id == edge.from));
            assert!(map.nodes.iter().any(|n| n.id == edge.to));
        }
        state.world.physics.material_queries_dirty = true;
        assert!(state.ground_navigation_map(0).is_none());
        assert!(
            state.world.physics.material_queries_dirty,
            "observation must not flush"
        );
        SurfaceSortieScenario::step(&mut state, &[], dt);
        assert!(
            state.ground_navigation_map(0).is_none(),
            "surveys have a bounded cadence"
        );
        assert!(state.queue_recovery_disruption(
            0,
            RecoveryDisruption::GroundRouteNode {
                node: map.nodes[0].id,
                radius: 1,
            }
        ));
        for _ in 0..29 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        let after = state.ground_navigation_map(0).unwrap();
        assert!(after.revision > map.revision);
        assert_ne!(after.nodes, map.nodes);
    }
}
