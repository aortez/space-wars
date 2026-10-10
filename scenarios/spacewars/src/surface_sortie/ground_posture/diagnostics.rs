//! Offline native shape queries, never supplied as movement permission.
use super::*;
use serde_json::{Value, json};

impl SurfaceSortieState {
    pub fn ground_posture_clearance_diagnostics(&self, player: usize) -> Value {
        let Some(body) = self
            .pilots
            .get(player)
            .and_then(|pilot| pilot.body.as_ref())
        else {
            return json!({"unavailable":"no on-foot actor"});
        };
        if self.world.physics.material_queries_dirty {
            return json!({"unavailable":"material queries pending"});
        }
        let physics = &self.world.physics.world;
        let before = physics.snapshot_bytes().unwrap();
        let snapshot = body.snapshot(physics).unwrap();
        let pilot = self.pilot_observation(player, None);
        let posture = self.ground_posture_observation(player, &pilot);
        let normal = snapshot.support.map_or(snapshot.up, |c| c.normal);
        // The existing crawl sensor uses this query-only separation allowance.
        // Retain both raw and separated fits; do not move the physical actor.
        let start = snapshot.motion.position + normal * 0.025;
        let directions: Vec<_> = (0..8)
            .map(|index| {
                let direction = normal.rotate_radians(index as f32 * std::f32::consts::FRAC_PI_4);
                json!({"degrees_from_contact_normal":index*45,"direction":direction,
                    "clearance":physics.collider_translation_clearance_at(
                        body.collider(),start,snapshot.motion.angle,direction,6.0)})
            })
            .collect();
        let tangent = Vec2::new(normal.y, -normal.x);
        let crawl: Vec<_> = [-1.0, 1.0]
            .into_iter()
            .map(|sign| {
                let direction = tangent * sign;
                let clearance = physics.collider_translation_clearance_at(
                    body.collider(),
                    start,
                    snapshot.motion.angle,
                    direction,
                    CRAWL_DISTANCE + 0.04,
                );
                let distance = clearance.map(|c| (c - 0.04).clamp(0.0, CRAWL_DISTANCE));
                let floor = snapshot.support.zip(distance).map(|(contact, distance)| {
                    [distance / 3.0, distance * 2.0 / 3.0, distance].map(|offset| {
                        let origin = contact.position + direction * offset + normal * 0.2;
                        self.world
                            .physics
                            .material_ground_ray(pilot.planet.index, origin, -normal, 0.4)
                            .map(|hit| {
                                json!({"position":hit.point,"normal":hit.normal,
                                    "alignment":hit.normal.dot(snapshot.up),
                                    "supported":hit.normal.dot(snapshot.up)>=Self::spec().min_support_alignment})
                            })
                    })
                });
                json!({"direction":sign,"clearance":clearance,
                    "distance_after_margin":distance,"retained_floor_probes":floor})
            })
            .collect();
        let result = json!({"schema":1,"tick":self.tick(),"seat":player,
            "planet":pilot.planet.index,"revision":pilot.planet.revision,
            "supported_planet":pilot.supported_planet,"balanced":pilot.balanced,
            "ordinary_posture":posture,"snapshot_debug":format!("{snapshot:?}"),
            "query_start":start,"query_separation":0.025,"query_angle":snapshot.motion.angle,
            "actual_pose_fits":physics.collider_fits_at(body.collider(),snapshot.motion.position,snapshot.motion.angle),
            "separated_pose_fits":physics.collider_fits_at(body.collider(),start,snapshot.motion.angle),
            "straight_sweeps":directions,"ungated_crawl_queries":crawl,
            "maximum_sweep_distance":6.0,"physics_unchanged":true,"controller_input":false,
            "scope":"Current capsule at fixed orientation; query-only contact separation. These sweeps do not establish a supported route, standing permission, or achievable controls."});
        assert_eq!(before, physics.snapshot_bytes().unwrap());
        result
    }
}
