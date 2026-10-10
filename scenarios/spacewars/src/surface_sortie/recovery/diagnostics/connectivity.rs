//! Frozen retained-terrain connectivity; no physics or controller mutation.
use super::*;
use engine_rapier::world::ConnectivityGrid;

impl SurfaceSortieState {
    pub fn recovery_connectivity_diagnostics(&self, player: usize) -> Value {
        let Some(body) = self.pilots.get(player).and_then(|p| p.body.as_ref()) else {
            return json!({"unavailable":"no on-foot actor"});
        };
        if self.world.physics.material_queries_dirty {
            return json!({"unavailable":"material queries pending"});
        }
        let physics = &self.world.physics.world;
        let before = physics.snapshot_bytes().unwrap();
        let actor = body.snapshot(physics).unwrap();
        let p = self.pilot_observation(player, None);
        let frame = motion::SurfaceFrame::read(&self.world.physics, p.planet.index);
        let map = self.survey_ground(
            player,
            p.planet.index,
            0..ground_navigation::GROUND_SAMPLES as u16,
            false,
        );
        let targets: Vec<_> = map.as_ref().into_iter().flat_map(|m| &m.nodes).map(|node| {
            let up = Vec2::Y.rotate_radians(node.id as f32 * std::f32::consts::TAU / ground_navigation::GROUND_SAMPLES as f32);
            let center = node.position + up * ground_navigation::standing_height();
            json!({"node":node,"standing_center":frame.position+center.rotate_radians(frame.angle)})
        }).collect();
        let grids: Vec<_> = [(0.1, 60), (0.05, 120), (0.025, 240)]
            .into_iter()
            .map(|(spacing, half_cells)| {
                physics
                    .diagnose_capsule_connectivity(
                        body.collider(),
                        ConnectivityGrid {
                            half_cells,
                            spacing,
                            angle: rotation_for_direction(actor.up),
                            penetration_allowance: 0.025,
                        },
                        |id| {
                            physics::is_planet_surface_support(id, p.planet.index)
                                && id.role.value() >= physics::terrain_spec().first_chunk_role
                        },
                    )
                    .expect("valid centered native capsule and diagnostic grid")
            })
            .collect();
        assert_eq!(before, physics.snapshot_bytes().unwrap());
        json!({"schema":1,"tick":self.tick(),"seat":player,"planet":p.planet.index,
            "revision":p.planet.revision,"actor":p.actor,"actor_up":p.actor_up,
            "capsule_angle":actor.motion.angle,"terrain_first_chunk_role":physics::terrain_spec().first_chunk_role,
            "terrain_entity":self.world.physics.planet_body(p.planet.index).entity,
            "standing_targets":targets,"grids":grids,"physics_unchanged":true,
            "physics_fingerprint":self.recovery_physics_fingerprint(),"controller_input":false,
            "obstacles":"Only this planet's retained terrain chunks, with native capsule groups; sensors and the actor entity excluded.",
            "scope":"Every capsule orientation, frozen retained terrain. Unknown cells and diagonal passages are open. An enclosed component bounds possible centers; an open component does not establish a route or achievable controls. Future terrain destruction, solver tunneling and rebuilding elsewhere are outside this diagnostic."})
    }
}
