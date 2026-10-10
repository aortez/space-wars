//! Offline clearance measurements outside the ordinary terrain-flight envelope.
//! These results never enter a production observation or authorize a flight.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TerrainGapCandidate {
    pub margin: usize,
    pub height: f32,
    pub plan: CrossingPlan,
    pub ordinary_height_allowed: bool,
    /// None means the diagnostic geometry bound rejected it before querying.
    pub corridor_clear: Option<bool>,
}

impl SurfaceSortieState {
    /// At most 24 candidates, using the ordinary capsule and corridor query.
    /// The diagnostic allows 20 units above the lower footing. The normal
    /// survey's ten-unit limit is deliberately unchanged.
    pub fn diagnose_terrain_gap(
        &self,
        player: usize,
        map: &GroundMap,
        from: u16,
        to: u16,
    ) -> Option<Vec<TerrainGapCandidate>> {
        let pilot = self.pilots.get(player)?;
        if self.world.physics.material_queries_dirty
            || self.location(player) != PilotLocation::OnFoot
            || map.version != 1
            || map.actor != pilot.owner
            || map.tick != self.world.tick
            || map.planet != self.motion_planet_index(player)
            || map.revision
                != self
                    .world
                    .terrain
                    .planets
                    .get(&map.planet)?
                    .field
                    .revision()
            || !(2..=ground_navigation::GROUND_SAMPLES).contains(&map.nodes.len())
        {
            return None;
        }
        let n = map.nodes.len();
        let index = map.nodes.iter().position(|node| node.id == from)?;
        if map.nodes[(index + 1) % n].id != to {
            return None;
        }
        let frame = motion::SurfaceFrame::read(&self.world.physics, map.planet);
        let spec = Self::spec();
        let capsule = self.world.physics.world.capsule_clearance_test_excluding(
            spec.half_segment,
            spec.radius + 0.20,
            spec.collision_groups,
            vec![pilot_physics_id(pilot.owner)],
        );
        let clear = |point: Vec2| {
            capsule(
                frame.position + point.rotate_radians(frame.angle),
                rotation_for_direction(point.normalized().rotate_radians(frame.angle)),
            )
        };
        let mut candidates = Vec::new();
        for margin in 0..8.min(n / 2) {
            let a = map.nodes[(index + n - margin) % n].position;
            let b = map.nodes[(index + 1 + margin) % n].position;
            for height in [3.0, 5.0, 7.0] {
                let cruise = a.length().max(b.length()) + height;
                let geometry = [a, b].iter().all(|v| v.x.is_finite() && v.y.is_finite())
                    && (1.0..=24.0).contains(&a.distance_to(b))
                    && cruise <= a.length().min(b.length()) + 20.0;
                candidates.push(TerrainGapCandidate {
                    margin,
                    height,
                    plan: CrossingPlan {
                        planet: map.planet,
                        revision: map.revision,
                        direction: CrossingDirection::Left,
                        start: a,
                        destination: b,
                        cruise_radius: cruise,
                        anchor: CrossingAnchor::GroundGap { from, to },
                    },
                    ordinary_height_allowed: cruise <= a.length().min(b.length()) + 10.0,
                    corridor_clear: geometry.then(|| corridor_clear(a, b, cruise, &clear)),
                });
            }
        }
        Some(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: Duration = Duration::from_nanos(16_666_667);

    #[test]
    fn diagnostic_preserves_the_native_world_and_rejects_stale_or_wrong_identity() {
        let mut state = SurfaceSortieScenario::init_material_jetpack(42, 1);
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1)],
            DT,
        );
        for _ in 0..29 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
        let map = state.ground_navigation_map(0).unwrap();
        let (a, b) = (map.nodes[0].id, map.nodes[1].id);
        let before = state.world.physics.snapshot_bytes();
        let ordinary = state.jetpack_navigation_observation(0);
        let result = state.diagnose_terrain_gap(0, &map, a, b).unwrap();
        assert_eq!(result.len(), 24);
        assert!(result.iter().any(|c| c.corridor_clear.is_some()));
        assert_eq!(state.diagnose_terrain_gap(0, &map, a, b), Some(result));
        assert_eq!(state.world.physics.snapshot_bytes(), before);
        assert_eq!(state.jetpack_navigation_observation(0), ordinary);
        let mut changed = map.clone();
        changed.tick -= 1;
        assert!(state.diagnose_terrain_gap(0, &changed, a, b).is_none());
        changed = map.clone();
        changed.actor = PlayerId::PLAYER_2;
        assert!(state.diagnose_terrain_gap(0, &changed, a, b).is_none());
        changed = map.clone();
        changed.revision += 1;
        assert!(state.diagnose_terrain_gap(0, &changed, a, b).is_none());
        assert!(state.diagnose_terrain_gap(0, &map, a, a).is_none());
        assert!(state.diagnose_terrain_gap(2, &map, a, b).is_none());
    }
}
