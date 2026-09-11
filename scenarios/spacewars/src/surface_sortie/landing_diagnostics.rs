//! Bounded read-only evidence for touchdown and hatch failures. Evaluators call
//! this outside controller timing; it is not part of a pilot's observation.
use super::*;
use serde_json::{Value, json};

impl SurfaceSortieState {
    pub fn landing_diagnostics(
        &self,
        player: usize,
        site: Option<&pilot::PilotLandingSite>,
    ) -> Value {
        let pilot = &self.pilots[player];
        let physics = &self.world.physics;
        let id = physics.ship_body(pilot.vehicle.0);
        let Some(body) = physics.world.motion(id) else {
            return Value::Null;
        };
        let surface = motion::SurfaceFrame::read(physics, pilot.planet);
        let up = (body.position - surface.position).normalized();
        let (feet, radius) = physics.landing_geometry(pilot.vehicle.0);
        let contact_parts = std::array::from_fn::<_, 3, _>(|part| {
            let count = physics
                .surface_vehicle_contacts(pilot.vehicle.0, part)
                .count();
            let contacts = physics
                .surface_vehicle_contacts(pilot.vehicle.0, part)
                .take(16)
                .map(|contact| {
                    let relative = physics.world.velocity_at_point(id, contact.position).unwrap()
                        - contact.velocity;
                    json!({
                        "collider": format!("{:?}", contact.collider),
                        "retained_planet": physics::is_planet_surface_support(contact.collider, pilot.planet),
                        "position": contact.position,
                        "local_position": contact.local_surface.position,
                        "normal": contact.normal,
                        "up_alignment": contact.normal.dot(up),
                        "separation": contact.separation,
                        "normal_speed": relative.dot(contact.normal),
                    })
                })
                .collect::<Vec<_>>();
            json!({"count":count,"contacts":contacts})
        });
        let foot_clearances = feet.map(|foot| {
            let origin = body.position + foot.rotate_radians(body.angle);
            physics
                .material_ground_ray(pilot.planet, origin + up * 0.1, -up, 27.0)
                .map(|hit| hit.distance - 0.1 - radius)
        });
        let access = self.material_access(player);
        let exit = access.map(|hit| {
            let spec = Self::spec();
            let position =
                self.access_position(player) + self.access_up(player) * (spec.half_height() + 0.12);
            let angle = rotation_for_direction(self.access_up(player));
            let clear = |excluded| {
                physics.world.capsule_is_clear_except(
                    position,
                    angle,
                    spec.half_segment,
                    spec.radius + 0.04,
                    spec.collision_groups,
                    excluded,
                )
            };
            let occupied = self.pilots.iter().any(|other| {
                other.snapshot(physics).is_some_and(|snapshot| {
                    snapshot.motion.position.distance_to(position) < spec.half_height() * 2.0 + 0.04
                })
            });
            json!({
                "floor":hit.point,"normal":hit.normal,"capsule_position":position,
                "occupied":occupied,"capsule_clear":clear(None),
                "clear_without_ship":clear(Some(id.entity)),
                "clear_without_planet":clear(Some(hit.collider.entity)),
            })
        });
        json!({
            "version":1,"tick":self.world.tick,"planet":pilot.planet,
            "queries_ready":!physics.material_queries_dirty,
            "landing":pilot.landing,"transfer":self.transfer_readiness(player),
            "foot_clearances":foot_clearances,"hull":contact_parts[0],
            "feet":[contact_parts[1],contact_parts[2]],"exit":exit,
            "site_hull_clearance":site.map(|site| {
                [-0.2, 0.0, 0.2].map(|height| [-0.75,0.0,0.75].map(|offset| {
                    physics.surface_hull_fits_at(pilot.vehicle.0,
                        site.vehicle_position + site.normal * height + Vec2::new(site.normal.y,-site.normal.x) * offset,
                        rotation_for_direction(site.normal))
                }))
            }),
        })
    }
}
