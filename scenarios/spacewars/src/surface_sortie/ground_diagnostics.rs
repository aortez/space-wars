//! Read-only contact evidence for ground traversal probes, outside bot sensors.
use super::*;
use serde_json::{Value, json};

impl SurfaceSortieState {
    /// Compare solver contacts with a fresh ray through the actor's actual floor.
    /// This diagnostic does not grant support, move actors or enter a bot's view.
    pub fn ground_contact_diagnostics(&self, player: usize) -> Value {
        let Some(pilot) = self.pilots.get(player) else {
            return Value::Null;
        };
        let Some(body) = &pilot.body else {
            return Value::Null;
        };
        let physics = &self.world.physics;
        let Some(snapshot) = body.snapshot(&physics.world) else {
            return Value::Null;
        };
        let surface = motion::SurfaceFrame::read(physics, pilot.planet);
        let local = |point: Vec2| (point - surface.position).rotate_radians(-surface.angle);
        let contacts = physics.world.surface_contacts(body.collider());
        let mut count = 0;
        let mut recorded = Vec::new();
        for contact in contacts {
            count += 1;
            if recorded.len() < 16 {
                recorded.push(json!({
                    "collider": format!("{:?}", contact.collider),
                    "retained_planet": physics::is_planet_surface_support(contact.collider, pilot.planet),
                    "position": contact.position, "normal": contact.normal,
                    "local_position": contact.local_surface.position,
                    "local_normal": contact.local_surface.normal,
                    "up_alignment": contact.normal.dot(snapshot.up),
                    "separation": contact.separation,
                }));
            }
        }
        let floor = physics
            .material_ground_ray(
                pilot.planet,
                snapshot.motion.position + snapshot.up * 2.0,
                -snapshot.up,
                4.0,
            )
            .map(|hit| {
                let height = (snapshot.motion.position - hit.point).dot(snapshot.up);
                // Projection onto this ray, not a full-capsule penetration test.
                let spec = Self::spec();
                let extent = spec.radius
                    + spec.half_segment
                        * Vec2::Y
                            .rotate_radians(snapshot.motion.angle)
                            .dot(snapshot.up)
                            .abs();
                json!({
                    "position": hit.point, "normal": hit.normal,
                    "local_position": local(hit.point),
                    "center_height": height, "capsule_extent": extent,
                    "projected_clearance": height - extent,
                })
            });
        json!({
            "version": 1, "tick": self.world.tick, "seat": player, "planet": pilot.planet,
            "queries_ready": !physics.material_queries_dirty,
            "position": snapshot.motion.position, "local_position": local(snapshot.motion.position),
            "up": snapshot.up, "gravity": pilot.gravity,
            "angle": snapshot.motion.angle, "spin": snapshot.motion.angular_velocity,
            "relative_speed": snapshot.relative_speed, "jumps": snapshot.jumps,
            "contact_count": count, "contacts": recorded, "floor": floor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_diagnostics_measure_real_material_without_advancing_physics() {
        let dt = Duration::from_nanos(16_666_667);
        let mut state = SurfaceSortieScenario::init_material_surface(
            42,
            1,
            engine_terrain::TerrainSurface::Interpolated,
        );
        assert!(state.ground_contact_diagnostics(0).is_null());
        assert!(state.ground_contact_diagnostics(2).is_null());
        state.world.planets[0].wrapper_omega = 0.0;
        state.world.ships[0].position += Vec2::new(200.0, 200.0);
        SurfaceSortieScenario::step(&mut state, &[], dt);
        let up = Vec2::from_radians(0.7);
        let hit = state
            .world
            .physics
            .material_ground_ray(0, state.world.planets[0].position + up * 80.0, -up, 100.0)
            .unwrap();
        let spec = SurfaceSortieState::spec();
        state.pilots[0].body = SpacelingAssembly::insert(
            &mut state.world.physics.world,
            pilot_physics_id(PlayerId::PLAYER_1),
            hit.point + up * (spec.half_height() + 0.02),
            rotation_for_direction(up),
            spec,
        );
        for _ in 0..60 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let snapshot = state.spaceling_snapshot(0).unwrap();
        assert!(snapshot.grounded());
        let diagnostic = state.ground_contact_diagnostics(0);
        assert_eq!(diagnostic, state.ground_contact_diagnostics(0));
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
        assert_eq!(state.spaceling_snapshot(0), Some(snapshot));
        assert_eq!(diagnostic["tick"], state.tick());
        assert_eq!(diagnostic["gravity"], json!(state.pilots[0].gravity));
        assert!(diagnostic["contact_count"].as_u64().unwrap() > 0);
        assert!(diagnostic["contacts"].as_array().unwrap().len() <= 16);
        assert!(
            diagnostic["floor"]["projected_clearance"]
                .as_f64()
                .unwrap()
                .abs()
                < 0.04,
            "{diagnostic}"
        );

        // Stale geometry must not produce a fresh floor measurement.
        state.world.physics.material_queries_dirty = true;
        let dirty = state.ground_contact_diagnostics(0);
        assert_eq!(dirty["queries_ready"], false);
        assert!(dirty["floor"].is_null());
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
    }
}
