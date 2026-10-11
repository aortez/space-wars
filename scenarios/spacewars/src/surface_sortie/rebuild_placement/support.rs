//! A necessary support condition for the proposed resting ship, not a landing
//! guarantee. Actual round-foot contacts and settled boarding remain native.
use super::*;

pub(super) fn alignments(normals: [Vec2; 2], settled: Vec2, planet_center: Vec2) -> [f32; 2] {
    let up = (settled - planet_center).normalized();
    normals.map(|normal| normal.dot(up))
}

pub(super) fn supports_landing(alignments: [f32; 2]) -> bool {
    alignments.into_iter().all(|alignment| {
        alignment.is_finite() && alignment >= physics::LANDING_MIN_SUPPORT_ALIGNMENT
    })
}

#[cfg(feature = "sensor-profile")]
impl SurfaceSortieState {
    /// Apply the native per-foot alignment requirement to placement previews
    /// and actual rebuild attempts. Activated only after the retained prefix.
    pub fn set_rebuild_support_alignment(&mut self, player: usize, enabled: bool) -> bool {
        let Some(pilot) = self.pilots.get_mut(player) else {
            return false;
        };
        pilot.rebuild_support_alignment = enabled;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_support_uses_the_ships_frame_for_the_retained_bad_foot() {
        // Frozen native build 25373: both old ray checks and the overall ship
        // angle pass, but the first sampled foot cannot earn native support.
        let query = Vec2::new(0.7570638, -0.653341);
        let normals = [
            Vec2::new(0.17360905, -0.9848147),
            Vec2::new(0.8559741, -0.5170187),
        ];
        let normal = Vec2::new(0.70255333, -0.71163106);
        let settled = Vec2::new(33.60344, -19.006557);
        assert!(normals.into_iter().all(|n| n.dot(query) >= 0.65));
        assert!(settling_angle(normal, settled, Vec2::ZERO) < landing::LANDED_ANGLE);
        for angle in [0.0, -0.7, 1.5] {
            let center = Vec2::new(1000.0, -700.0);
            let result = alignments(
                normals.map(|n| n.rotate_radians(angle)),
                center + settled.rotate_radians(angle),
                center,
            );
            assert!((result[0] - 0.6359545).abs() < 0.00001);
            assert!(result[1] > 0.9995);
            assert!(!supports_landing(result));
        }
        // The already-successful handoff remains well inside the same gate.
        assert!(supports_landing(alignments(
            [
                Vec2::new(0.995322, -0.0966136),
                Vec2::new(0.9653762, -0.26086184)
            ],
            Vec2::new(40.934494, -7.0092626),
            Vec2::ZERO
        )));
    }

    #[test]
    fn rebuild_support_shares_the_native_threshold_for_both_feet() {
        let min = physics::LANDING_MIN_SUPPORT_ALIGNMENT;
        assert_eq!(min, 0.7);
        assert!(supports_landing([min, min]));
        for invalid in [min - 0.00001, f32::NAN, f32::NEG_INFINITY, f32::INFINITY] {
            assert!(!supports_landing([invalid, 1.0]));
            assert!(!supports_landing([1.0, invalid]));
        }
    }

    #[cfg(feature = "sensor-profile")]
    #[test]
    fn rebuild_support_gate_preserves_valid_queries_and_clones_without_mutation() {
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
        let actor = state.spaceling_snapshot(0).unwrap();
        let support = actor.support.unwrap();
        let planet = physics::planet_surface_support_index(support.collider).unwrap();
        let map = state
            .rebuild_ground_map(0, planet, support.position)
            .unwrap();
        let (_, original) =
            state.find_rebuild_placement(0, planet, support.position, support.normal, Some(&map));
        assert!(
            original
                .attempts
                .iter()
                .all(|a| a.support_alignments.is_none())
        );
        assert!(!state.set_rebuild_support_alignment(99, true));
        state.set_rebuild_support_alignment(0, true);
        let before = state.world.physics.snapshot_bytes();
        let (pose, report) =
            state.find_rebuild_placement(0, planet, support.position, support.normal, Some(&map));
        assert!(pose.is_some(), "{report:?}");
        assert_eq!(report.selected_offset, original.selected_offset);
        for attempt in &report.attempts {
            if attempt.rejection.is_none() {
                assert!(supports_landing(attempt.support_alignments.unwrap()));
            }
        }
        let (_, cloned) = state.clone().find_rebuild_placement(
            0,
            planet,
            support.position,
            support.normal,
            Some(&map),
        );
        assert_eq!(report, cloned);
        assert_eq!(state.world.physics.snapshot_bytes(), before);
        assert_eq!(state.spaceling_snapshot(0), Some(actor));
        state.set_rebuild_support_alignment(0, false);
        let (_, restored) =
            state.find_rebuild_placement(0, planet, support.position, support.normal, Some(&map));
        assert_eq!(original, restored);
        state.set_rebuild_support_alignment(0, true);
        state.world.physics.material_queries_dirty = true;
        let (pose, blocked) =
            state.find_rebuild_placement(0, planet, support.position, support.normal, Some(&map));
        assert!(pose.is_none());
        assert_eq!(
            blocked.attempts[0].rejection,
            Some(RebuildRejection::QueriesPending)
        );
    }
}
