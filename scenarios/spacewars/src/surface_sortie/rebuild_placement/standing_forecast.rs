//! Offline coverage of measured footing, outbound routes and conditional landing.
//! No proposal, forecast or additional query is supplied to a playing controller.
use super::*;
use serde_json::{Value, json};

const MAX_FORECASTS: usize = 64;

impl SurfaceSortieState {
    pub fn rebuild_standing_forecast_diagnostics(&self, player: usize) -> Value {
        if player >= self.player_count()
            || self.world.physics.material_queries_dirty
            || self.vehicle_available(player)
        {
            return json!({"unavailable":"invalid seat, queries pending or vehicle available"});
        }
        let Some(actor) = self.spaceling_snapshot(player) else {
            return json!({"unavailable":"no on-foot actor"});
        };
        let before = self.world.physics.world.snapshot_bytes().unwrap();
        let recovery = self.observation(player).recovery;
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
        let local = |p: Vec2| (p - frame.position).rotate_radians(-frame.angle);
        let foot = local(actor.motion.position - actor.up * Self::spec().half_height());
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
        let offsets = REBUILD_OFFSETS
            .iter()
            .chain(refinement::EXTRA_OFFSETS.iter())
            .copied()
            .collect::<Vec<_>>();
        let mut inputs = Vec::new();
        if let Ok((p, point, up)) = self.rebuild_candidate(player) {
            assert_eq!(p, planet);
            inputs.push((
                None,
                local(point),
                up.rotate_radians(-frame.angle),
                point,
                up,
            ));
        }
        inputs.extend(nodes.iter().map(|n| {
            (
                Some(n.id),
                n.position,
                n.normal,
                frame.position + n.position.rotate_radians(frame.angle),
                n.normal.rotate_radians(frame.angle),
            )
        }));
        let mut forecasts = 0;
        let mut offset_checks = 0;
        let mut attempts = Vec::new();
        for (bearing, position, normal, point, up) in inputs {
            let coarse = map.route(foot, position, 0.8);
            let precise = map.route(foot, position, 0.01);
            let mut staging_checks = 0;
            let stage = bearing.and_then(|id| {
                let node = map.nodes.iter().find(|n| n.id == id).unwrap();
                staging::staging_step(&map, foot, node, &precise, &mut staging_checks)
            });
            let staging_routes = stage.map(|s| {
                [
                    map.route(foot, s.position, 0.01),
                    map.route(s.position, position, 0.01),
                ]
            });
            let reachable = |route: &ground_navigation::GroundRoute| {
                route.diagnostics.failure.is_none() && route.diagnostics.length <= MAX_REBUILD_WALK
            };
            let eligible =
                bearing.is_none() || reachable(&coarse) || reachable(&precise) || stage.is_some();
            let mut predictions = Vec::new();
            let report = eligible.then(|| {
                let (_, report) = self.find_rebuild_placement_offsets(
                    player, planet, point, up, Some(&base), &offsets,
                );
                offset_checks += report.attempts.len();
                for attempt in &report.attempts {
                    if attempt.rejection.is_some() {
                        continue;
                    }
                    if forecasts == MAX_FORECASTS {
                        predictions.push(json!({"offset":attempt.offset,"forecast":null,"unavailable":"diagnostic forecast budget"}));
                        continue;
                    }
                    let (pose, single) = self.find_rebuild_placement_offsets(
                        player, planet, point, up, Some(&base), &[attempt.offset],
                    );
                    offset_checks += 1;
                    assert_eq!(single.attempts, vec![attempt.clone()]);
                    let mut job = RebuildLocalForecast::standing_preview(
                        self, player, &pose.unwrap(), single,
                    );
                    while !job.is_complete() {
                        job.advance(4);
                    }
                    forecasts += 1;
                    predictions.push(json!({"offset":attempt.offset,"forecast":job.diagnostics()}));
                }
                report
            });
            attempts.push(json!({"bearing":bearing,"position":position,"normal":normal,
                "distance":position.distance_to(foot),"already_here":bearing.is_none(),
                "sparse_candidate":bearing.is_some_and(|id|sparse.iter().any(|n|n.id==id)),
                "coarse_route":coarse,"precise_route":precise,"staging":stage,"staging_routes":staging_routes,"staging_route_checks":staging_checks,
                "eligible":eligible,"placement":report,"forecasts":predictions}));
        }
        assert_eq!(self.world.physics.world.snapshot_bytes().unwrap(), before);
        assert_eq!(self.spaceling_snapshot(player), Some(actor));
        assert_eq!(self.observation(player).recovery, recovery);
        json!({"schema":1,"tick":self.tick(),"seat":player,"planet":planet,"revision":map.revision,
            "foot":foot,"offsets":offsets,"map":map,"replacement_map":base,"attempts":attempts,
            "max_forecasts":MAX_FORECASTS,"forecasts":forecasts,"offset_checks":offset_checks,
            "physics_unchanged":true,"actor_unchanged":true,"recovery_unchanged":true,
            "preview_only":true,"controller_input":false,"travel_or_build_time_predicted":false,
            "scope":"Existing local patch and placement envelope. Snapshot-epoch conditional settling; no executed relocation or future arrival guarantee."})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_standing_forecast_preserves_world_and_checks_every_accepted_pose() {
        let mut state = native_forecast::tests::preparing_build(false);
        assert!(state.rebuild_standing_forecast_diagnostics(99)["unavailable"].is_string());
        for _ in 0..2 {
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        let before = state.world.physics.snapshot_bytes();
        let tick = state.tick();
        let value = state.rebuild_standing_forecast_diagnostics(0);
        assert_eq!(value["physics_unchanged"], true, "{value}");
        assert_eq!(state.world.physics.snapshot_bytes(), before);
        assert_eq!(state.tick(), tick);
        let cases = value["attempts"].as_array().unwrap();
        assert!(cases.len() <= 114);
        assert!(cases[0]["already_here"].as_bool().unwrap());
        let mut count = 0;
        let mut positive = false;
        for case in cases {
            let predictions = case["forecasts"].as_array().unwrap();
            if case["eligible"] == true {
                let attempts = case["placement"]["attempts"].as_array().unwrap();
                assert_eq!(attempts.len(), 26);
                assert_eq!(
                    attempts.iter().filter(|a| a["rejection"].is_null()).count(),
                    predictions.len()
                );
            } else {
                assert!(case["placement"].is_null() && predictions.is_empty());
            }
            for prediction in predictions {
                let f = &prediction["forecast"];
                if f.is_null() {
                    assert_eq!(prediction["unavailable"], "diagnostic forecast budget");
                    continue;
                }
                count += 1;
                assert_eq!(f["start_delay"], 0);
                assert_eq!(f["input"]["live_vehicle_available"], false);
                assert_eq!(f["steps"], 120);
                assert_eq!(f["samples"].as_array().unwrap().len(), 121);
                assert!(
                    f["chunks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|c| c["steps"].as_u64().unwrap() <= 4)
                );
                positive |= f["settles_within_horizon"] == true;
            }
        }
        assert!(positive && count <= MAX_FORECASTS);
        assert_eq!(value["forecasts"], count);
        state.world.physics.material_queries_dirty = true;
        assert!(state.rebuild_standing_forecast_diagnostics(0)["unavailable"].is_string());
    }
}
