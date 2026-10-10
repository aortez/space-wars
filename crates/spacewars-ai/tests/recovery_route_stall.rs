//! Native graph queries over consumed observations from the failed recovery.
//! A synthetic edge below isolates the missing connection; it is not a flight.
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId,
    spaceling_geometry::HALF_HEIGHT,
    surface_sortie::ground_navigation::{
        GroundEdge, GroundEdgeKind, GroundMap, GroundNode, GroundNodeRejection, GroundRejectedNode,
    },
};
use serde_json::{Value, json};

fn number(value: &Value) -> f32 {
    value.as_f64().unwrap() as f32
}

fn vector(value: &Value) -> Vec2 {
    Vec2::new(number(&value["x"]), number(&value["y"]))
}

fn node_id(value: &Value) -> u16 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn map(value: &Value) -> GroundMap {
    assert_eq!(value["actor"], "player_2");
    GroundMap {
        version: value["version"].as_u64().unwrap() as u32,
        actor: PlayerId::PLAYER_2,
        planet: value["planet"].as_u64().unwrap() as usize,
        revision: value["revision"].as_u64().unwrap(),
        tick: value["tick"].as_u64().unwrap(),
        nodes: value["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| GroundNode {
                id: node_id(&n["id"]),
                position: vector(&n["position"]),
                normal: vector(&n["normal"]),
            })
            .collect(),
        edges: value["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| GroundEdge {
                from: node_id(&e["from"]),
                to: node_id(&e["to"]),
                length: number(&e["length"]),
                kind: match e["kind"].as_str().unwrap() {
                    "walk" => GroundEdgeKind::Walk,
                    "jump" => GroundEdgeKind::Jump,
                    _ => panic!("the consumed ground map cannot contain flight edges"),
                },
            })
            .collect(),
        rejected: value["rejected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| GroundRejectedNode {
                id: node_id(&n["id"]),
                reason: match n["reason"].as_str().unwrap() {
                    "no_retained_floor" => GroundNodeRejection::NoRetainedFloor,
                    "steep_floor" => GroundNodeRejection::SteepFloor,
                    "capsule_obstructed" => GroundNodeRejection::CapsuleObstructed,
                    "replacement_obstructed" => GroundNodeRejection::ReplacementObstructed,
                    _ => panic!("unknown rejected-node reason"),
                },
            })
            .collect(),
    }
}

#[test]
fn retained_recovery_maps_have_no_complete_flag_or_pod_route() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/recovery-route-stall.json")).unwrap();
    let samples = fixture["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 4);
    for s in samples {
        let tick = s["tick"].as_u64().unwrap();
        let p = &s["pilot"];
        let frame = &p["planet"]["motion"];
        let local = |point: Vec2| {
            (point - vector(&frame["position"])).rotate_radians(-number(&frame["angle"]))
        };
        let foot = local(vector(&p["actor"]["position"]) - vector(&p["actor_up"]) * HALF_HEIGHT);
        let flag = local(vector(&p["planet"]["claim"]["flag"]["position"]));
        assert_eq!(flag, vector(&s["target"]));
        let range = number(&p["planet"]["claim"]["flag_interaction_range"]) - 0.2;
        let hatches = std::array::from_fn(|i| {
            let h = &p["boarding_hatches"][i];
            (!h.is_null()).then(|| local(vector(h)))
        });
        let ground = map(&s["ground"]);
        let mut combined = ground.clone();
        let j = &s["jetpack"];
        assert_eq!(j["surveyed"], true);
        assert!(j["terrain_crossings"].as_array().unwrap().len() <= 8);
        let candidates = (!j["crossing"].is_null())
            .then_some(&j["crossing"])
            .into_iter()
            .chain(j["terrain_crossings"].as_array().unwrap());
        for plan in candidates {
            assert_eq!(plan["planet"], ground.planet);
            assert_eq!(plan["revision"], ground.revision);
            let start = vector(&plan["start"]);
            let end = vector(&plan["destination"]);
            let radius = number(&plan["cruise_radius"]);
            assert!(start.distance_to(end) > 1.0 && start.distance_to(end) < 30.0);
            assert!(radius > start.length().max(end.length()));
            assert!(radius < start.length().min(end.length()) + 20.0);
            combined.connect_jetpack(start, end);
            combined.connect_jetpack(end, start);
        }
        let ground_routes = ground.routes();
        let combined_routes = combined.routes();
        let ground_flag = ground_routes.route_to_actor_target(foot, flag, range);
        let combined_flag = combined_routes.route_to_actor_target(foot, flag, range);
        let ground_hatches = ground_routes.route_to_hatches(foot, hatches);
        let combined_hatches = combined_routes.route_to_hatches(foot, hatches);
        for route in [
            &ground_flag,
            &combined_flag,
            &ground_hatches,
            &combined_hatches,
        ] {
            assert!(route.path.is_empty(), "unexpected complete route at {tick}");
            assert!(route.diagnostics.failure.is_some());
        }
        let partial = combined_routes.route_toward_actor_target(foot, flag, range);
        // With no complete route, the planner selects a nonempty partial route
        // if available; otherwise its telemetry retains the direct failure.
        let selected = if partial.path.is_empty() {
            &ground_flag
        } else {
            &partial
        };
        // The engine stores these metrics as f32. Normalize parsed JSON's f64
        // representation before comparing; no tolerance is needed in f32.
        let mut observed = s["observed_route"].clone();
        for key in [
            "closest_reachable_distance",
            "nearest_destination_distance",
            "start_distance",
            "length",
        ] {
            observed[key] = serde_json::to_value(number(&observed[key])).unwrap();
        }
        assert_eq!(
            serde_json::to_value(&selected.diagnostics).unwrap(),
            observed,
            "reproduce the recorded native route diagnostics at {tick}"
        );

        // Positive control: only in this cloned graph, bridge the ledge without
        // a clearance survey. This establishes which missing edge matters. It
        // does not authorize a crossing or show that a pilot can execute it.
        let a = combined
            .nodes
            .iter()
            .find(|n| n.id == 276)
            .unwrap()
            .position;
        let b = combined
            .nodes
            .iter()
            .find(|n| n.id == 282)
            .unwrap()
            .position;
        assert_eq!(combined.connect_jetpack(a, b), Some((276, 282)));
        let synthetic = combined.route_to_actor_target(foot, flag, range);
        assert!(!synthetic.path.is_empty());
        assert!(synthetic.diagnostics.failure.is_none() && !synthetic.diagnostics.partial);
        assert!(synthetic.path.windows(2).any(|pair| pair == [276, 282]));
        assert_eq!(ground, map(&s["ground"]), "source map stays immutable");
        println!(
            "RECOVERY_ROUTE_PROBE {}",
            json!({"tick":tick, "ground_flag":ground_flag, "combined_flag":combined_flag,
                "ground_hatches":ground_hatches, "combined_hatches":combined_hatches,
                "partial_flag":partial, "synthetic_bridge_flag":synthetic})
        );
    }
}
