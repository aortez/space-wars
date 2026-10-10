//! Read-only comparisons on retained replay states, never controller inputs.
use super::*;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuildPlacementProbeRequest {
    pub tick: u64,
    pub preview_tick: u64,
    pub planet: usize,
    pub revision: u64,
    pub bearing: u16,
    pub expected_point: Vec2,
    pub expected_offset: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RebuildPlacementProbeAnchor {
    request: RebuildPlacementProbeRequest,
    point: Vec2,
    normal: Vec2,
    map_origin: Vec2,
    report: RebuildPlacementReport,
}

impl SurfaceSortieState {
    /// Capture the actual node and query direction when the retained preview
    /// is accepted. Later probes can refresh the same map extent independently.
    pub fn rebuild_placement_probe_anchor(
        &self,
        player: usize,
        request: &RebuildPlacementProbeRequest,
    ) -> Option<RebuildPlacementProbeAnchor> {
        if player >= self.pilots.len()
            || request.preview_tick != self.tick()
            || request.tick <= request.preview_tick
            || self.world.physics.material_queries_dirty
            || self
                .world
                .terrain
                .planets
                .get(&request.planet)?
                .field
                .revision()
                != request.revision
        {
            return None;
        }
        let actor = self.spaceling_snapshot(player)?;
        let frame = motion::SurfaceFrame::read(&self.world.physics, request.planet);
        let map = self.local_ground_map(player, request.planet, actor.motion.position, false)?;
        let node = map.nodes.iter().find(|n| n.id == request.bearing)?;
        if !request.expected_point.x.is_finite()
            || !request.expected_point.y.is_finite()
            || node.position.distance_to(request.expected_point) > 0.001
        {
            return None;
        }
        let point = frame.position + node.position.rotate_radians(frame.angle);
        let base = self.rebuild_ground_map(player, request.planet, actor.motion.position)?;
        let (_, report) = self.find_rebuild_placement(
            player,
            request.planet,
            point,
            node.normal.rotate_radians(frame.angle),
            Some(&base),
        );
        if report.selected_offset != Some(request.expected_offset) {
            return None;
        }
        Some(RebuildPlacementProbeAnchor {
            request: request.clone(),
            point: node.position,
            normal: report.radial_up.unwrap_or(node.normal),
            map_origin: (actor.motion.position - frame.position).rotate_radians(-frame.angle),
            report,
        })
    }

    /// Vary position, direction and map extent while keeping the world fixed.
    /// Hypothetical placements do not move the pilot or authorize construction.
    pub fn rebuild_placement_probe(
        &self,
        player: usize,
        anchor: &RebuildPlacementProbeAnchor,
    ) -> Value {
        let request = &anchor.request;
        if player >= self.pilots.len()
            || self.tick() != request.tick
            || self.world.physics.material_queries_dirty
        {
            return json!({"unavailable":"invalid seat, tick or queries"});
        }
        let Ok((planet, actual, actual_normal)) = self.rebuild_candidate(player) else {
            return json!({"unavailable":"native eligibility rejected"});
        };
        if planet != request.planet
            || self
                .world
                .terrain
                .planets
                .get(&planet)
                .map(|p| p.field.revision())
                != Some(request.revision)
        {
            return json!({"unavailable":"planet or revision changed"});
        }
        let before = self.world.physics.world.snapshot_bytes().unwrap();
        let contact = self.rebuild_contact_diagnostics(player);
        let recovery = self.observation(player).recovery;
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let local = |p: Vec2| (p - frame.position).rotate_radians(-frame.angle);
        let world = |p: Vec2| frame.position + p.rotate_radians(frame.angle);
        let preview = world(anchor.point);
        let normal = actual_normal.rotate_radians(-frame.angle);
        let Some(actual_map) = self.rebuild_ground_map(player, planet, actual) else {
            return json!({"unavailable":"no actual map"});
        };
        let Some(preview_map) = self.rebuild_ground_map(player, planet, world(anchor.map_origin))
        else {
            return json!({"unavailable":"no refreshed preview map"});
        };
        let (_, baseline) =
            self.find_rebuild_placement(player, planet, actual, actual_normal, Some(&actual_map));
        let mut query = self.clone();
        // Explicit directions below must reach the shared native query unchanged.
        // This flag change belongs only to the diagnostic clone, never the replay.
        query.pilots[player].rebuild_radial_placement = false;
        let mut cases = Vec::new();
        for (map_name, map) in [("actual", &actual_map), ("preview_extent", &preview_map)] {
            for (position_name, point) in [("actual", actual), ("preview", preview)] {
                for (direction_name, up) in [
                    ("contact", actual_normal),
                    ("preview", anchor.normal.rotate_radians(frame.angle)),
                    ("radial", (point - frame.position).normalized()),
                ] {
                    let (pose, report) =
                        query.find_rebuild_placement(player, planet, point, up, Some(map));
                    cases.push(json!({"map":map_name,"position":position_name,"direction":direction_name,
                        "query_point":local(point),"query_up":up.rotate_radians(-frame.angle),
                        "pose":pose.map(|p|json!({"center":local(p.center),"normal":p.normal.rotate_radians(-frame.angle)})),
                        "report":report}));
                }
            }
        }
        assert_eq!(before, query.world.physics.world.snapshot_bytes().unwrap());
        assert_eq!(before, self.world.physics.world.snapshot_bytes().unwrap());
        assert_eq!(contact, self.rebuild_contact_diagnostics(player));
        assert_eq!(recovery, self.observation(player).recovery);
        json!({"tick":self.tick(),"seat":player,"anchor":anchor,"actual_point":local(actual),
            "contact_normal":normal,"radial_enabled":self.pilots[player].rebuild_radial_placement,
            "contact":contact,"native_recovery":recovery,"baseline":baseline,
            "maps":{"actual":actual_map,"preview_extent":preview_map},"cases":cases,
            "physics_unchanged":true,"preview_only":true})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_placement_probe_varies_only_queries_and_rejects_invalid_context() {
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
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        state.enable_recovery();
        state.world.planets[0].owner_id = Some(0);
        state.world.ships[0].translate_life(-state.world.ships[0].life_max);
        for _ in 0..2 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        state.set_rebuild_contact_frame(0, true);
        state.set_rebuild_refinement(0, true);
        let (planet, _, _) = state.rebuild_candidate(0).unwrap();
        let frame = motion::SurfaceFrame::read(&state.world.physics, planet);
        let actor = state.spaceling_snapshot(0).unwrap();
        let map = state
            .local_ground_map(0, planet, actor.motion.position, false)
            .unwrap();
        let base = state
            .rebuild_ground_map(0, planet, actor.motion.position)
            .unwrap();
        let (node, selected) = map
            .nodes
            .iter()
            .find_map(|n| {
                let p = frame.position + n.position.rotate_radians(frame.angle);
                let (_, report) = state.find_rebuild_placement(
                    0,
                    planet,
                    p,
                    n.normal.rotate_radians(frame.angle),
                    Some(&base),
                );
                report.selected_offset.map(|offset| (n, offset))
            })
            .unwrap();
        let request = RebuildPlacementProbeRequest {
            tick: state.tick() + 1,
            preview_tick: state.tick(),
            planet,
            revision: map.revision,
            bearing: node.id,
            expected_point: node.position,
            expected_offset: selected,
        };
        let anchor = state.rebuild_placement_probe_anchor(0, &request).unwrap();
        assert!(state.rebuild_placement_probe_anchor(99, &request).is_none());
        assert!(state.rebuild_placement_probe(0, &anchor)["unavailable"].is_string());
        SurfaceSortieScenario::step(&mut state, &[], dt);
        let before = state.world.physics.snapshot_bytes();
        for radial in [false, true] {
            state.set_rebuild_radial_placement(0, radial);
            let result = state.rebuild_placement_probe(0, &anchor);
            assert_eq!(result["physics_unchanged"], true, "{result}");
            assert_eq!(result["cases"].as_array().unwrap().len(), 12);
            assert_eq!(result, state.rebuild_placement_probe(0, &anchor));
            assert_eq!(state.world.physics.snapshot_bytes(), before);
            let expected = if radial { "radial" } else { "contact" };
            let matched = result["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| {
                    c["map"] == "actual" && c["position"] == "actual" && c["direction"] == expected
                })
                .unwrap();
            assert_eq!(
                matched["report"]["attempts"],
                result["baseline"]["attempts"]
            );
        }
        let mut changed = anchor.clone();
        changed.request.revision += 1;
        assert!(state.rebuild_placement_probe(0, &changed)["unavailable"].is_string());
        state.world.physics.material_queries_dirty = true;
        assert!(state.rebuild_placement_probe(0, &anchor)["unavailable"].is_string());
    }
}
