//! Opt-in common placement direction for previews and actual construction.
use super::*;

impl SurfaceSortieState {
    /// Use the standing point's current radial frame for offset queries only.
    /// Native support, construction and landing remain authoritative.
    pub fn set_rebuild_radial_placement(&mut self, player: usize, enabled: bool) -> bool {
        let Some(pilot) = self.pilots.get_mut(player) else {
            return false;
        };
        pilot.rebuild_radial_placement = enabled;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on_foot() -> SurfaceSortieState {
        let mut state = SurfaceSortieScenario::init_material(42, 1);
        let dt = Duration::from_nanos(16_666_667);
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
        for _ in 0..30 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        state
    }

    #[test]
    fn rebuild_radial_queries_ignore_facet_normals_without_changing_physics() {
        let mut state = on_foot();
        let actor = state.spaceling_snapshot(0).unwrap();
        let support = actor.support.unwrap();
        let planet = physics::planet_surface_support_index(support.collider).unwrap();
        let frame = motion::SurfaceFrame::read(&state.world.physics, planet);
        let point = frame.position + support.local_surface.position.rotate_radians(frame.angle);
        let up = (point - frame.position).normalized();
        let map = state.rebuild_ground_map(0, planet, point).unwrap();
        let (_, original) = state.find_rebuild_placement(0, planet, point, up, Some(&map));
        assert!(original.radial_up.is_none());
        assert!(
            serde_json::to_value(&original)
                .unwrap()
                .get("radial_up")
                .is_none()
        );
        let (_, tilted) = state.find_rebuild_placement(
            0,
            planet,
            point,
            up.rotate_radians(42.0_f32.to_radians()),
            Some(&map),
        );
        assert_ne!(original.attempts, tilted.attempts);
        assert!(!state.set_rebuild_radial_placement(99, true));
        assert!(state.set_rebuild_radial_placement(0, true));
        let before = state.world.physics.snapshot_bytes();
        for refined in [false, true] {
            state.set_rebuild_refinement(0, refined);
            let (pose, report) = state.find_rebuild_placement(0, planet, point, up, Some(&map));
            assert!(pose.is_some(), "{report:?}");
            let local_up = report.radial_up.unwrap();
            assert!(local_up.distance_to(report.standing.normalized()) < 0.00001);
            for degrees in [-90.0_f32, -42.0, 42.0, 90.0] {
                let (other, repeated) = state.find_rebuild_placement(
                    0,
                    planet,
                    point,
                    up.rotate_radians(degrees.to_radians()),
                    Some(&map),
                );
                assert_eq!(repeated, report);
                assert_eq!(other.unwrap().center, pose.as_ref().unwrap().center);
            }
            let (_, cloned) =
                state
                    .clone()
                    .find_rebuild_placement(0, planet, point, -up, Some(&map));
            assert_eq!(cloned, report);
            let (_, offsets) = state.find_rebuild_placement_offsets(
                0,
                planet,
                point,
                -up,
                Some(&map),
                &refinement::EXTRA_OFFSETS,
            );
            let (_, direct) = state.find_rebuild_placement_offsets(
                0,
                planet,
                point,
                up,
                Some(&map),
                &refinement::EXTRA_OFFSETS,
            );
            assert_eq!(offsets, direct);
            for attempt in report.attempts.iter().chain(&offsets.attempts) {
                if attempt.rejection.is_none() {
                    assert!(attempt.settling_angle_degrees.unwrap() < landing::LANDED_ANGLE);
                    let route = attempt.route.as_ref().unwrap();
                    assert!(route.failure.is_none() && route.length <= MAX_REBUILD_WALK);
                }
            }
        }
        assert_eq!(state.world.physics.snapshot_bytes(), before);
        assert_eq!(state.spaceling_snapshot(0), Some(actor));
        state.set_rebuild_radial_placement(0, false);
        let (_, restored) = state.find_rebuild_placement(0, planet, point, up, Some(&map));
        assert_eq!(restored, original);
    }

    #[test]
    fn rebuild_radial_queries_still_require_current_material_queries_and_map() {
        let mut state = on_foot();
        let support = state.spaceling_snapshot(0).unwrap().support.unwrap();
        let planet = physics::planet_surface_support_index(support.collider).unwrap();
        let map = state
            .rebuild_ground_map(0, planet, support.position)
            .unwrap();
        state.set_rebuild_radial_placement(0, true);
        for (dirty, map) in [(false, None), (true, Some(&map))] {
            state.world.physics.material_queries_dirty = dirty;
            let (pose, report) =
                state.find_rebuild_placement(0, planet, support.position, support.normal, map);
            assert!(pose.is_none());
            assert_eq!(report.attempts.len(), 1);
            assert_eq!(
                report.attempts[0].rejection,
                Some(RebuildRejection::QueriesPending)
            );
        }
    }
}
