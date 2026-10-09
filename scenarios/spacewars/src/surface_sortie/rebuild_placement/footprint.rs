//! Read-only comparison of the placement rays with the actual round feet.
use super::*;
use serde_json::{Value, json};

impl SurfaceSortieState {
    /// Called only on a native build tick, after the replacement has been
    /// inserted. Extra queries never enter the bot's observation or controller.
    pub fn rebuild_footprint_diagnostics(&self, player: usize) -> Value {
        let Some(pilot) = self.pilots.get(player) else {
            return json!({"unavailable":"invalid seat"});
        };
        let Some(report) = pilot
            .recovery
            .as_ref()
            .and_then(|r| r.observation().placement)
        else {
            return json!({"unavailable":"no native placement"});
        };
        let Some(offset) = report
            .selected_offset
            .filter(|_| report.tick == self.tick())
        else {
            return json!({"unavailable":"not a fresh accepted placement"});
        };
        if self.world.physics.material_queries_dirty || !self.vehicle_available(player) {
            return json!({"unavailable":"queries pending or replacement unavailable"});
        }
        let Ok((planet, point, contact_up)) = self.rebuild_candidate(player) else {
            return json!({"unavailable":"native standing contact unavailable"});
        };
        assert_eq!(planet, report.planet);
        let physics = &self.world.physics;
        let before = physics.world.snapshot_bytes().unwrap();
        let contacts = self.rebuild_contact_diagnostics(player);
        let frame = motion::SurfaceFrame::read(physics, planet);
        let local = |p: Vec2| (p - frame.position).rotate_radians(-frame.angle);
        let local_direction = |p: Vec2| p.rotate_radians(-frame.angle);
        assert!(local(point).distance_to(report.standing) < 0.001);
        let up = if pilot.rebuild_radial_placement {
            (point - frame.position).normalized()
        } else {
            contact_up
        };
        let right = Vec2::new(up.y, -up.x);
        let ground = |origin, direction, length| {
            physics.material_ground_ray(planet, origin, direction, length)
        };
        let hit = ground(point + right * offset + up * 12.0, -up, 24.0).unwrap();
        let samples =
            [-3.0, 3.0].map(|side| ground(hit.point + right * side + up * 8.0, -up, 16.0).unwrap());
        let tangent = (samples[1].point - samples[0].point).normalized();
        let normal = Vec2::new(-tangent.y, tangent.x);
        let floor = samples[0].point.midpoint(samples[1].point);
        let replacement = self.replacement_ship(player);
        let radius = physics::SpacewarsPhysics::surface_vehicle_clearance_radius(&replacement);
        let spawn = floor + normal * (radius + 0.6);
        let settled = floor + normal * 5.45;
        let radial = (spawn - frame.position).normalized();
        let projected = spawn - radial * ((spawn - settled).dot(normal) / radial.dot(normal));
        let angle = rotation_for_direction(normal);
        let body = physics
            .world
            .motion(physics.ship_body(pilot.vehicle.0))
            .unwrap();
        assert!(body.position.distance_to(spawn) < 0.001);
        assert!(Vec2::Y.rotate_radians(body.angle).distance_to(normal) < 0.001);
        let (feet, foot_radius) = physics::surface_landing_geometry(ShipForm::Ship);
        let colliders = physics.surface_preview_geometry(pilot.vehicle.0, &replacement);
        let hit_json = |h: engine_rapier::world::RayHit| {
            json!({"point":local(h.point),"normal":local_direction(h.normal),"distance":h.distance,
                "collider":format!("{:?}",h.collider)})
        };
        let poses = [("spawn", spawn), ("predicted_rest", settled), ("radial_descent", projected)]
            .map(|(name, center)| {
                let radial = (center - frame.position).normalized();
                let feet = feet.map(|foot| {
                    let center = center + foot.rotate_radians(angle);
                    json!({"center":local(center),"bottom":local(center-normal*foot_radius),
                        "normal_ray":ground(center+normal*0.1,-normal,4.0).map(hit_json),
                        "radial_ray":ground(center+radial*0.1,-radial,4.0).map(hit_json)})
                });
                json!({"name":name,"center":local(center),"radial_up":local_direction(radial),"feet":feet})
            });
        let sweeps = [normal, radial].map(|direction| {
            let feet = feet.map(|foot| spawn + foot.rotate_radians(angle));
            let distances = std::array::from_fn::<_, 2, _>(|i| {
                let collider = colliders
                    .iter()
                    .find(|c| c.local_position == physics::LANDING_FEET[i])
                    .unwrap();
                physics.world.collider_translation_clearance_at(
                    collider.id,
                    feet[i],
                    angle,
                    -direction,
                    4.0,
                )
            });
            json!({"down":local_direction(-direction),"first_collision_distances":distances})
        });
        assert_eq!(before, physics.world.snapshot_bytes().unwrap());
        assert_eq!(contacts, self.rebuild_contact_diagnostics(player));
        json!({"tick":self.tick(),"seat":player,"report":report,"query_up":local_direction(up),
            "center_hit":hit_json(hit),"samples":samples.map(hit_json),"normal":local_direction(normal),
            "floor":local(floor),"spawn_radius":radius,"foot_radius":foot_radius,"poses":poses,"sweeps":sweeps,
            "physics_unchanged":true,"read_only":true})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_footprint_probe_requires_a_fresh_build_and_retains_physics() {
        let mut state = SurfaceSortieScenario::init_material(42, 1);
        let dt = Duration::from_nanos(16_666_667);
        assert!(state.rebuild_footprint_diagnostics(99)["unavailable"].is_string());
        assert!(state.rebuild_round_foot_diagnostics(99)["unavailable"].is_string());
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
        state.set_rebuild_contact_frame(0, true);
        state.world.ships[0].translate_life(-state.world.ships[0].life_max);
        assert!(state.rebuild_footprint_diagnostics(0)["unavailable"].is_string());
        assert!(state.rebuild_round_foot_diagnostics(0)["unavailable"].is_string());
        for _ in 0..900 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
            let report = state.pilots[0].recovery.as_ref().unwrap().observation();
            if report.rebuilds == 0 {
                continue;
            }
            let before = state.world.physics.snapshot_bytes();
            let result = state.rebuild_footprint_diagnostics(0);
            assert_eq!(result["physics_unchanged"], true, "{result}");
            assert_eq!(result, state.rebuild_footprint_diagnostics(0));
            assert_eq!(state.world.physics.snapshot_bytes(), before);
            assert_eq!(result["poses"].as_array().unwrap().len(), 3);
            assert_eq!(result["sweeps"].as_array().unwrap().len(), 2);
            let round = state.rebuild_round_foot_diagnostics(0);
            assert_eq!(round["footprint"], result);
            assert_eq!(round, state.rebuild_round_foot_diagnostics(0));
            assert_eq!(state.world.physics.snapshot_bytes(), before);
            for sweep in round["sweeps"].as_array().unwrap() {
                for foot in sweep["feet"].as_array().unwrap() {
                    assert_eq!(foot["status"], "Converged", "{round}");
                    assert_eq!(foot["retained_surface"], true, "{round}");
                }
            }
            state.world.physics.material_queries_dirty = true;
            assert!(state.rebuild_footprint_diagnostics(0)["unavailable"].is_string());
            assert!(state.rebuild_round_foot_diagnostics(0)["unavailable"].is_string());
            state.world.physics.material_queries_dirty = false;
            SurfaceSortieScenario::step(&mut state, &[], dt);
            assert!(state.rebuild_footprint_diagnostics(0)["unavailable"].is_string());
            assert!(state.rebuild_round_foot_diagnostics(0)["unavailable"].is_string());
            return;
        }
        panic!("native replacement did not build");
    }
}
