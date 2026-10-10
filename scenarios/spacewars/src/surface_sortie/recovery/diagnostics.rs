//! Read-only physical evidence for retained recovery failures.
use super::*;
use serde_json::{Value, json};

mod connectivity;

impl SurfaceSortieState {
    /// Same-build equality receipt for native physics, independent of recovery
    /// reports and profile flags. Not a cryptographic source identity.
    pub fn recovery_physics_fingerprint(&self) -> String {
        let bytes = self.world.physics.world.snapshot_bytes().unwrap();
        let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        format!("{hash:016x}")
    }

    /// Unscheduled inspection of the current measured graph. Never supplied
    /// to the bot, and never advances physics or refreshes controller memory.
    pub fn recovery_route_diagnostics(&self, player: usize) -> Value {
        let Some(actor) = self.spaceling_snapshot(player) else {
            return json!({"unavailable":"no on-foot actor"});
        };
        if self.world.physics.material_queries_dirty {
            return json!({"unavailable":"material queries pending"});
        }
        let before = self.recovery_physics_fingerprint();
        let p = self.pilot_observation(player, None);
        let frame = motion::SurfaceFrame::read(&self.world.physics, p.planet.index);
        let local = |point: Vec2| (point - frame.position).rotate_radians(-frame.angle);
        let foot = local(actor.motion.position - actor.up * Self::spec().half_height());
        let map = self.survey_ground(
            player,
            p.planet.index,
            0..ground_navigation::GROUND_SAMPLES as u16,
            false,
        );
        let hatch_route = map.as_ref().map(|map| {
            map.routes()
                .route_to_hatches(foot, p.boarding_hatches.map(|h| h.map(local)))
        });
        let flag_route = map.as_ref().and_then(|map| {
            let claim = p.planet.claim.as_ref()?;
            let flag = claim.flag.as_ref()?;
            Some(map.routes().route_to_actor_target(
                foot,
                local(flag.position),
                claim.flag_interaction_range,
            ))
        });
        let native_footing = match self.rebuild_candidate(player) {
            Ok((planet, point, up)) => {
                let base = self.rebuild_ground_map(player, planet, point);
                let nearest = base.as_ref().and_then(|map| {
                    map.nodes
                        .iter()
                        .map(|n| n.position.distance_to(local(point)))
                        .min_by(f32::total_cmp)
                });
                json!({"eligible":true,"planet":planet,"point":point,"up":up,
                    "local_point":local(point),"nearest_replacement_start":nearest,
                    "replacement_map":base,"note":"The full native update also requires the ship to be unavailable."})
            }
            Err(reason) => json!({"eligible":false,"reason":reason}),
        };
        let result = json!({"tick":self.tick(),"seat":player,"pilot":p,"actor_foot":foot,
            "support_debug":format!("{:?}",actor.support),"map":map,
            "hatch_route":hatch_route,"flag_route":flag_route,"native_footing":native_footing,
            "replacement_distance":actor.motion.position.distance_to(p.ship.position),
            "replacement_distance_limit":24.0,
            "actor_relative_speed":(actor.motion.linear_velocity-p.planet.velocity_at(actor.motion.position)).length(),
            "posture":self.ground_posture_clearance_diagnostics(player),
            "physics_fingerprint":before,"controller_input":false});
        assert_eq!(before, self.recovery_physics_fingerprint());
        result
    }
}
