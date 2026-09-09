//! Bounded measurements of surviving outer ground and spaceling clearance.
//! The AI chooses routes. Sensors never edit material or advance physics.
use super::*;

pub const GROUND_SAMPLES: usize = 512;
pub const GROUND_NEIGHBOR_SPAN: usize = 6;
pub const GROUND_REFRESH_TICKS: u64 = 30;

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
}

impl SurfaceSortieState {
    pub fn ground_navigation_map(&self, player: usize) -> Option<GroundMap> {
        if !(self.world.tick + player as u64 * 15).is_multiple_of(GROUND_REFRESH_TICKS)
            || self.location(player) != PilotLocation::OnFoot
            || self.world.physics.material_queries_dirty
        {
            return None;
        }
        let planet = self.motion_planet_index(player);
        let terrain = self.world.terrain.planets.get(&planet)?;
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let spec = Self::spec();
        // On a staircase the support normal and gravity-relative capsule axis
        // differ. Reserve the capsule's full projected foot radius on slopes.
        let standing_height =
            spec.half_segment + (spec.radius + 0.04) / spec.min_support_alignment + 0.08;
        let radius = self.world.planets[planet].radius;
        let world_point = |point: Vec2| frame.position + point.rotate_radians(frame.angle);
        let capsule_clear = self.world.physics.world.capsule_clearance_test(
            spec.half_segment,
            spec.radius + 0.02,
            spec.collision_groups,
            Some(pilot_physics_id(self.pilots[player].owner)),
        );
        let clear = |point: Vec2| {
            let up = point.normalized();
            capsule_clear(
                world_point(point),
                rotation_for_direction(up.rotate_radians(frame.angle)),
            )
        };
        let mut nodes = Vec::new();
        for id in 0..GROUND_SAMPLES {
            let up =
                Vec2::Y.rotate_radians(id as f32 * std::f32::consts::TAU / GROUND_SAMPLES as f32);
            let world_up = up.rotate_radians(frame.angle);
            let Some(hit) = self.world.physics.material_ground_ray(
                planet,
                frame.position + world_up * (radius + 8.0),
                -world_up,
                radius + 8.0,
            ) else {
                continue;
            };
            if hit.normal.dot(world_up) < spec.min_support_alignment {
                continue;
            }
            let position = (hit.point - frame.position).rotate_radians(-frame.angle);
            if !clear(position + up * standing_height) {
                continue;
            }
            nodes.push(GroundNode {
                id: id as u16,
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
