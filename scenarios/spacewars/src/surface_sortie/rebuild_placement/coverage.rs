//! Diagnostic coverage only. No result is supplied to a playing controller.
use super::*;
use serde_json::{Value, json};

fn offsets() -> Vec<f32> {
    let mut result = REBUILD_OFFSETS.to_vec();
    for sign in [-1.0, 1.0] {
        for half_units in 17..28 {
            result.push(sign * half_units as f32 * 0.5);
        }
    }
    result
}

impl SurfaceSortieState {
    /// Inspect every measured footing in the existing local patch and offset
    /// envelope. A valid proposal is still not a native build or boarding.
    pub fn rebuild_coverage_diagnostics(&self, player: usize) -> Value {
        if player >= self.pilots.len() || self.world.physics.material_queries_dirty {
            return json!({"unavailable":"invalid seat or queries pending"});
        }
        let Some(actor) = self.spaceling_snapshot(player) else {
            return json!({"unavailable":"no on-foot actor"});
        };
        let before = self.world.physics.world.snapshot_bytes().unwrap();
        let planet = self.motion_planet_index(player);
        let Some(mut map) = self.local_ground_map(player, planet, actor.motion.position, false)
        else {
            return json!({"unavailable":"no measured ground map"});
        };
        let Some(base) = self.rebuild_ground_map(player, planet, actor.motion.position) else {
            return json!({"unavailable":"no replacement ground map"});
        };
        if self.pilots[player].jetpack_charge.is_some() {
            let plans = self.terrain_crossings(player, &map);
            for plan in self
                .crossing_plan(player, jetpack::CrossingDirection::Left)
                .into_iter()
                .chain(plans)
            {
                map.connect_jetpack(plan.start, plan.destination);
                map.connect_jetpack(plan.destination, plan.start);
            }
        }
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let foot = (actor.motion.position - actor.up * Self::spec().half_height() - frame.position)
            .rotate_radians(-frame.angle);
        let mut nodes = map
            .nodes
            .iter()
            .filter(|n| n.position.distance_to(foot) <= MAX_REBUILD_WALK)
            .collect::<Vec<_>>();
        nodes.sort_by(|a, b| {
            a.position
                .distance_to(foot)
                .total_cmp(&b.position.distance_to(foot))
                .then(a.id.cmp(&b.id))
        });
        assert!(nodes.len() <= (2 * LOCAL_HALF_SPAN + 1) as usize);
        let mut sparse: Vec<&ground_navigation::GroundNode> = Vec::new();
        for node in &nodes {
            if node.position.distance_to(foot) >= 2.0
                && sparse
                    .iter()
                    .all(|old| old.position.distance_to(node.position) >= 2.0)
            {
                sparse.push(node);
                if sparse.len() == 32 {
                    break;
                }
            }
        }
        let sparse_bearings: Vec<_> = sparse.iter().map(|n| n.id).collect();
        let offsets = offsets();
        let mut attempts = Vec::new();
        for node in nodes {
            let route = map.route(foot, node.position, 0.8);
            let eligible =
                route.diagnostics.failure.is_none() && route.diagnostics.length <= MAX_REBUILD_WALK;
            let reports = eligible.then(|| {
                let point = frame.position + node.position.rotate_radians(frame.angle);
                let up = node.normal.rotate_radians(frame.angle);
                let (_, legacy) = self.find_rebuild_placement_offsets(
                    player,
                    planet,
                    point,
                    up,
                    Some(&base),
                    &REBUILD_OFFSETS,
                );
                let (_, dense) = self.find_rebuild_placement_offsets(
                    player,
                    planet,
                    point,
                    up,
                    Some(&base),
                    &offsets,
                );
                assert_eq!(legacy.attempts, dense.attempts[..REBUILD_OFFSETS.len()]);
                json!({"legacy":legacy,"dense":dense})
            });
            attempts.push(
                json!({"node":node,"distance":node.position.distance_to(foot),
                "route":route,"eligible":eligible,"placement":reports}),
            );
        }
        let native = self.rebuild_relocation_survey(player);
        assert_eq!(self.world.physics.world.snapshot_bytes().unwrap(), before);
        json!({"schema":1,"tick":self.tick(),"seat":player,"planet":planet,
            "revision":map.revision,"foot":foot,"offsets":offsets,
            "native_survey":native,"sparse_bearings":sparse_bearings,"map":map,"replacement_map":base,
            "attempts":attempts,"physics_unchanged":true,
            "scope":"Read-only proposals. Same local patch and route limit; no candidate thinning. No placement, movement, task reset, forecast authorization or physics step."})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_preserves_native_results_and_physics_at_measured_footing() {
        let mut state = SurfaceSortieScenario::init_material(42, 1);
        let dt = Duration::from_nanos(16_666_667);
        assert!(state.rebuild_coverage_diagnostics(0)["unavailable"].is_string());
        assert!(state.rebuild_coverage_diagnostics(99)["unavailable"].is_string());
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
        let before = state.world.physics.snapshot_bytes();
        let actor = state.spaceling_snapshot(0).unwrap();
        let report = state.rebuild_coverage_diagnostics(0);
        assert_eq!(report, state.rebuild_coverage_diagnostics(0));
        assert!(state.set_rebuild_refinement(0, true));
        assert_eq!(report, state.rebuild_coverage_diagnostics(0));
        assert_eq!(state.world.physics.snapshot_bytes(), before);
        assert_eq!(state.spaceling_snapshot(0), Some(actor));
        let attempts = report["attempts"].as_array().unwrap();
        assert!(!attempts.is_empty() && attempts.len() <= 113);
        assert!(attempts.iter().any(|a| a["eligible"] == true));
        for attempt in attempts {
            if attempt["eligible"] != true {
                assert!(attempt["placement"].is_null());
                continue;
            }
            let old = attempt["placement"]["legacy"]["attempts"]
                .as_array()
                .unwrap();
            let dense = attempt["placement"]["dense"]["attempts"]
                .as_array()
                .unwrap();
            assert_eq!(old, &dense[..4]);
            assert_eq!(dense.len(), 26);
            for pose in dense {
                if pose["rejection"].is_null() {
                    assert!(pose["settling_angle_degrees"].as_f64().unwrap() < 20.0);
                    assert!(pose["route"]["failure"].is_null());
                    assert!(pose["route"]["length"].as_f64().unwrap() <= 24.0);
                }
            }
        }
        state.world.physics.material_queries_dirty = true;
        assert!(state.rebuild_coverage_diagnostics(0)["unavailable"].is_string());
        assert_eq!(state.world.physics.snapshot_bytes(), before);
    }
}
