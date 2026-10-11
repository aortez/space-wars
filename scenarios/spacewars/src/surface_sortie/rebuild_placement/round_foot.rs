//! First-contact predictions using the real round-foot radius. These sweeps
//! measure independent contacts during straight descent, not a settled pose.
use super::*;
use engine_rapier::world::{BallCastOptions, BallCastStatus};
use serde_json::{Value, json};

impl SurfaceSortieState {
    pub fn rebuild_round_foot_diagnostics(&self, player: usize) -> Value {
        let footprint = self.rebuild_footprint_diagnostics(player);
        if footprint.get("unavailable").is_some() {
            return footprint;
        }
        let pilot = &self.pilots[player];
        let report = pilot
            .recovery
            .as_ref()
            .unwrap()
            .observation()
            .placement
            .unwrap();
        let physics = &self.world.physics;
        let before = physics.world.snapshot_bytes().unwrap();
        let contacts = self.rebuild_contact_diagnostics(player);
        let frame = motion::SurfaceFrame::read(physics, report.planet);
        let local = |p: Vec2| (p - frame.position).rotate_radians(-frame.angle);
        let local_direction = |p: Vec2| p.rotate_radians(-frame.angle);
        let body = physics
            .world
            .motion(physics.ship_body(pilot.vehicle.0))
            .unwrap();
        let normal = Vec2::Y.rotate_radians(body.angle);
        let radial = (body.position - frame.position).normalized();
        let (feet, radius) = physics::surface_landing_geometry(ShipForm::Ship);
        let sweeps = [("normal", normal), ("radial", radial)].map(|(name, up)| {
            let hits = feet.map(|foot| {
                physics.world.cast_ball(
                    body.position + foot.rotate_radians(body.angle),
                    radius,
                    -up,
                    BallCastOptions {
                        max_distance: 4.0,
                        collision_groups: physics::material_ground_groups(),
                        ..Default::default()
                    },
                )
            });
            let supported = hits.map(|hit| {
                hit.filter(|h| {
                    h.status == BallCastStatus::Converged
                        && physics::is_planet_surface_support(h.collider, report.planet)
                })
                .map(|h| {
                    let at_impact = body.position - up * h.distance;
                    h.normal.dot((at_impact - frame.position).normalized())
                        >= physics::LANDING_MIN_SUPPORT_ALIGNMENT
                })
            });
            let prediction = match supported {
                [Some(a), Some(b)] => Some(a && b),
                _ => None,
            };
            let measurements = std::array::from_fn::<_, 2, _>(|i| {
                let origin = body.position + feet[i].rotate_radians(body.angle);
                hits[i].map(|h| {
                    let at_impact = body.position - up * h.distance;
                    let impact_radial = (at_impact - frame.position).normalized();
                    json!({"origin":local(origin),"center_at_impact":local(origin-up*h.distance),
                        "ship_origin_at_impact":local(at_impact),"radial_up_at_impact":local_direction(impact_radial),
                        "point":local(h.point),"normal":local_direction(h.normal),"distance":h.distance,
                        "status":format!("{:?}",h.status),"collider":format!("{:?}",h.collider),
                        "retained_surface":physics::is_planet_surface_support(h.collider,report.planet),
                        "up_alignment":h.normal.dot(impact_radial),"supports_first_contact":supported[i]})
                })
            });
            json!({"name":name,"down":local_direction(-up),"feet":measurements,
                "both_first_contacts_supported":prediction,
                "distance_gap":hits[0].zip(hits[1]).map(|(a,b)|(a.distance-b.distance).abs())})
        });
        assert_eq!(physics.world.snapshot_bytes().unwrap(), before);
        assert_eq!(self.rebuild_contact_diagnostics(player), contacts);
        json!({"tick":self.tick(),"seat":player,"footprint":footprint,"sweeps":sweeps,
            "support_threshold":physics::LANDING_MIN_SUPPORT_ALIGNMENT,
            "prediction_kind":"independent_first_contacts_without_rotation",
            "read_only":true,"physics_unchanged":true})
    }
}
