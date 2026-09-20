//! Optional isolated control probes from a cloned ground state.
//! Never used by a controller or the measured main trial.
use engine_common::Scenario;
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{SurfaceSortieAction, SurfaceSortieScenario, SurfaceSortieState},
};
use serde_json::json;
use std::{path::Path, time::Duration};

pub fn run(state: &SurfaceSortieState, seat: usize, out: &Path) {
    let owner = PlayerId::from_index(seat).unwrap();
    let source = state.recovery_task_observation(seat, None);
    std::fs::write(
        out.join("ground-probe-source.json"),
        serde_json::to_vec_pretty(&json!({
            "observation": source,
            "contacts": state.ground_contact_diagnostics(seat),
            "terrain_colliders": format!("{:?}", state.terrain_collider_layout()),
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        out.join("ground-probe-terrain.json"),
        serde_json::to_vec(&state.planet_terrain(source.flight.pilot.planet.index)).unwrap(),
    )
    .unwrap();
    let mut probes = Vec::new();
    for (name, horizontal, jump) in [
        ("wait", 0.0, false),
        ("left", -1.0, false),
        ("right", 1.0, false),
        ("jump_left", -1.0, true),
        ("jump_right", 1.0, true),
        ("jump", 0.0, true),
    ] {
        let mut world = state.clone();
        let mut samples = Vec::new();
        for tick in 0..=8 * 60 {
            if tick <= 60 || tick % 30 == 0 {
                let o = world.recovery_task_observation(seat, None);
                let p = &o.flight.pilot;
                let Some(actor) = p.actor else {
                    samples.push(json!({"tick":tick,"location":p.location,"actor_lost":true}));
                    break;
                };
                let foot = (actor.position
                    - p.actor_up * scenario_spacewars::spaceling_geometry::HALF_HEIGHT
                    - p.planet.motion.position)
                    .rotate_radians(-p.planet.motion.angle);
                let distance = o.ground.as_ref().and_then(|m| {
                    m.nodes
                        .iter()
                        .map(|n| n.position.distance_to(foot))
                        .min_by(f32::total_cmp)
                });
                samples.push(json!({"tick":tick,"foot":foot,"nearest_footing":distance,"posture":o.posture,
                    "supported":p.supported_planet,"hatch_distance":p.hatch.map(|h| h.distance_to(actor.position)),
                    "contacts": world.ground_contact_diagnostics(seat),
                    "angle_from_up": Vec2::Y.rotate_radians(actor.angle).dot(p.actor_up)}));
            }
            if tick < 8 * 60 {
                SurfaceSortieScenario::step(
                    &mut world,
                    &[SurfaceSortieAction {
                        horizontal,
                        primary_held: jump && tick == 0,
                        ..Default::default()
                    }
                    .encode(owner)],
                    Duration::from_nanos(16_666_667),
                );
            }
        }
        std::fs::write(
            out.join(format!("ground-probe-{name}-frame.json")),
            serde_json::to_vec(&SurfaceSortieScenario::player_frame(&world, seat)).unwrap(),
        )
        .unwrap();
        probes.push(json!({"name":name,"samples":samples,"audit":world.terrain_diagnostics()}));
    }
    std::fs::write(out.join("ground-start-probes.json"), serde_json::to_vec_pretty(&json!({"start_tick":state.pilot_observation(seat,None).tick,"seat":seat,"probes":probes})).unwrap()).unwrap();
}
