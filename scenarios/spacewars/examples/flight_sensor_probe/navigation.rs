//! Actual-hull geometric diagnostics, kept separate from policy observations.
use super::*;
use scenario_spacewars::surface_sortie::flight_calibration::{CalibrationStart, FlightCalibration};
use std::f32::consts::{FRAC_PI_2, PI};

const ACTOR: u64 = 9_000_000;

pub fn geometry(seed: u64) -> Vec<ColliderSpec> {
    FlightCalibration::new(CalibrationStart {
        seed,
        speed: 0.,
        velocity_bearing: 0.,
        heading: 0.,
        altitude: 150.,
        gravity: false,
        planet_spin: 0.,
    })
    .unwrap()
    .geometry()
    .into_iter()
    .enumerate()
    .map(|(i, s)| ColliderSpec {
        id: ColliderId::new(PhysicsId::new(ACTOR), ColliderRole::PRIMARY, i as u16),
        collision_groups: CollisionGroups::ALL,
        solver_groups: CollisionGroups::ALL,
        ..s
    })
    .collect()
}

fn add_actor(world: &mut PhysicsWorld, specs: &[ColliderSpec]) {
    assert!(world.insert_body(
        BodyId::new(PhysicsId::new(ACTOR), BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        specs
    ));
    world.step(1. / 60.);
}

pub fn outlines(specs: &[ColliderSpec]) -> Vec<engine_rapier::world::polar::GeometryOutline> {
    let mut w = PhysicsWorld::new(Default::default());
    add_actor(&mut w, specs);
    w.polar_geometry_outlines(Default::default()).unwrap()
}

fn ship_scene(name: &str, description: &str) -> Scene {
    let mut s = scene(name, description);
    s.ship_probe = true;
    s.angle = -FRAC_PI_2;
    s
}

pub fn fixtures() -> Vec<Scene> {
    let mut scenes = Vec::new();
    for distance in [12., 20., 40., 90., 170.] {
        for (phase, angle) in [("aligned", -FRAC_PI_2), ("offset", -FRAC_PI_2 + PI / 128.)] {
            for (label, gap, clear) in [("narrow", 10.8, false), ("wide", 11.2, true)] {
                let mut s = ship_scene(
                    &format!("ship-gap-{distance}-{phase}-{label}"),
                    "Paired gap around the real ship's 11-unit width. Native hull/feet translation is checked independently of the map; the small heading offset tests angular bin alignment.",
                );
                s.angle = angle;
                s.goal = Vec2::new(distance + 25., 0.);
                s.route = vec![s.origin, s.goal];
                s.comparison_group = Some(format!("gap-{distance}-{phase}"));
                s.sequence = format!("gap-{distance}-{phase}");
                for (n, sign) in [(1, -1.), (2, 1.)] {
                    block(
                        &mut s,
                        n,
                        Vec2::new(distance, sign * (gap / 2. + 18.)),
                        Vec2::new(2., 18.),
                    );
                }
                s.physical_check = json!({"gap_width":gap,"crossing":Vec2::new(distance,0.),"expected_route_clear":clear,"expected_direct_clear":clear});
                scenes.push(s);
            }
        }
    }
    for narrow in [false, true] {
        let mut s = ship_scene(
            if narrow {
                "curved-corridor-bottleneck"
            } else {
                "curved-corridor-wide"
            },
            "A quarter-circle corridor bends around an inner wall. The prescribed centerline translates the actual hull at fixed heading; it is geometry evidence, not a dynamically flown route.",
        );
        for (entity, sign) in [(1, -1.), (2, 1.)] {
            let points: Vec<_> = (0..=96)
                .map(|i| {
                    let t = -0.1 + 1.2 * i as f32 / 96.;
                    let a = -FRAC_PI_2 + t * FRAC_PI_2;
                    let width = 16.
                        - if narrow {
                            6. * (t.clamp(0., 1.) * PI).sin().powi(2)
                        } else {
                            0.
                        };
                    Vec2::new(0., 50.) + Vec2::new(a.cos(), a.sin()) * (50. + sign * width / 2.)
                })
                .collect();
            s.witnesses.extend(points.iter().copied());
            insert(
                &mut s,
                entity,
                Vec2::ZERO,
                ColliderShape::Polyline { vertices: points },
                "curved-wall",
            );
        }
        s.route = (0..=32)
            .map(|i| {
                let a = -FRAC_PI_2 + i as f32 / 32. * FRAC_PI_2;
                Vec2::new(0., 50.) + Vec2::new(a.cos(), a.sin()) * 50.
            })
            .collect();
        s.route[0] = s.origin;
        s.goal = *s.route.last().unwrap();
        s.physical_check = json!({"expected_route_clear":!narrow,"expected_direct_clear":false});
        s.comparison_group = Some("curved-corridor".into());
        s.sequence = "curved-corridor".into();
        scenes.push(s);
    }
    for sealed in [false, true] {
        let mut s = ship_scene(
            if sealed {
                "dead-end-sealed"
            } else {
                "dead-end-open"
            },
            "The goal is beyond the back wall. The open case has a geometric escape path whose first leg moves away from the goal. The sealed case is an unreachable diagnostic, not a valid training start.",
        );
        block(&mut s, 1, Vec2::new(0., 20.), Vec2::new(42., 2.));
        block(&mut s, 2, Vec2::new(0., -20.), Vec2::new(42., 2.));
        block(&mut s, 3, Vec2::new(40., 0.), Vec2::new(2., 22.));
        if sealed {
            block(&mut s, 4, Vec2::new(-40., 0.), Vec2::new(2., 22.));
        }
        s.goal = Vec2::new(70., 0.);
        s.route = vec![
            s.origin,
            Vec2::new(-55., 0.),
            Vec2::new(-55., 35.),
            Vec2::new(55., 35.),
            s.goal,
        ];
        s.physical_check = json!({"expected_route_clear":!sealed,"expected_direct_clear":false,
            "first_leg_moves_away":s.route[1].distance_to(s.goal)>s.origin.distance_to(s.goal)});
        s.comparison_group = Some("dead-end".into());
        s.sequence = "dead-end".into();
        scenes.push(s);
    }
    let mut s = ship_scene(
        "wall-detour",
        "A solid wall blocks the straight route; a prescribed translation path goes around its end. Route samples and exact shape sweeps use all hull/foot colliders.",
    );
    block(&mut s, 1, Vec2::new(40., 0.), Vec2::new(3., 45.));
    s.goal = Vec2::new(80., 0.);
    s.route = vec![s.origin, Vec2::new(20., 60.), Vec2::new(60., 60.), s.goal];
    s.physical_check = json!({"expected_route_clear":true,"expected_direct_clear":false});
    scenes.push(s);
    for s in &mut scenes {
        s.world.step(1. / 60.);
    }
    scenes
}

fn fits(world: &PhysicsWorld, specs: &[ColliderSpec], p: Vec2, angle: f32) -> bool {
    specs.iter().all(|s| {
        world.collider_fits_at(
            s.id,
            p + s.local_position.rotate_radians(angle),
            angle + s.local_angle,
        ) == Some(true)
    })
}

fn translates(world: &PhysicsWorld, specs: &[ColliderSpec], a: Vec2, b: Vec2, angle: f32) -> bool {
    if !fits(world, specs, a, angle) || !fits(world, specs, b, angle) {
        return false;
    }
    let distance = a.distance_to(b);
    if distance < 0.0001 {
        return true;
    }
    specs.iter().all(|s| {
        world
            .collider_translation_clearance_at(
                s.id,
                a + s.local_position.rotate_radians(angle),
                angle + s.local_angle,
                b - a,
                distance,
            )
            .is_some_and(|clear| clear >= distance - 0.0001)
    })
}

pub fn inspect(s: &Scene, specs: &[ColliderSpec]) -> Value {
    assert!(s.route.len() >= 2);
    assert_eq!(s.route.first(), Some(&s.origin));
    assert_eq!(s.route.last(), Some(&s.goal));
    let before = s.world.snapshot_bytes().unwrap();
    let mut world = s.world.clone();
    add_actor(&mut world, specs);
    let direct = translates(&world, specs, s.origin, s.goal, s.angle);
    let route_clear = s
        .route
        .windows(2)
        .all(|p| translates(&world, specs, p[0], p[1], s.angle));
    assert!(
        fits(&world, specs, s.origin, s.angle),
        "invalid ship start: {}",
        s.name
    );
    assert_eq!(
        direct,
        s.physical_check["expected_direct_clear"].as_bool().unwrap(),
        "direct: {}",
        s.name
    );
    assert_eq!(
        route_clear,
        s.physical_check["expected_route_clear"].as_bool().unwrap(),
        "route: {}",
        s.name
    );
    let mut samples = Vec::new();
    for nodes in s.route.windows(2) {
        let steps = (nodes[0].distance_to(nodes[1]).ceil() as usize).max(1);
        for i in 0..steps {
            let p = nodes[0] + (nodes[1] - nodes[0]) * (i as f32 / steps as f32);
            samples.push(json!({"position":p,"hull_fits":fits(&world,specs,p,s.angle)}));
        }
    }
    samples.push(json!({"position":s.goal,"hull_fits":fits(&world,specs,s.goal,s.angle)}));
    let crossing: Option<Vec2> = serde_json::from_value(s.physical_check["crossing"].clone()).ok();
    let mut result = s.physical_check.clone();
    result.as_object_mut().unwrap().extend(json!({"probe":"production open-wing hull and feet","fixed_heading":s.angle,
        "direct_translation_clear":direct,"route_translation_clear":route_clear,
        "crossing_hull_fits":crossing.map(|p|fits(&world,specs,p,s.angle)),"route_samples":samples,
        "semantics":"native fixed-orientation translation checks; not a dynamically feasible flight or policy input"}).as_object().unwrap().clone());
    assert_eq!(before, s.world.snapshot_bytes().unwrap());
    result
}

/// Compare only geometry feature values. Native shape IDs and diagnostic object
/// records are not learned input channels and do not count as distinctions.
pub fn comparisons(records: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for r in records {
        let Some(group) = r["comparison_group"].as_str() else {
            continue;
        };
        if !seen.insert(group) {
            continue;
        }
        let pair: Vec<_> = records
            .iter()
            .filter(|s| s["comparison_group"] == group)
            .collect();
        assert_eq!(pair.len(), 2);
        let a = pair[0];
        let b = pair[1];
        let variants:Vec<_>=a["variants"].as_array().unwrap().iter().zip(b["variants"].as_array().unwrap()).map(|(a,b)|{
            let ac=a["observation"]["cells"].as_array().unwrap();let bc=b["observation"]["cells"].as_array().unwrap();
            let changed=|key:&str|ac.iter().zip(bc).filter(|(a,b)|a[key]!=b[key]).count();
            let rays=a["observation"]["rays"].as_array().unwrap().iter().zip(b["observation"]["rays"].as_array().unwrap());
            let changed_rays=rays.filter(|(a,b)|a["distance"]!=b["distance"]).count();
            let ap:Vec2=serde_json::from_value(a["observation"]["nearest_point"].clone()).unwrap();
            let bp:Vec2=serde_json::from_value(b["observation"]["nearest_point"].clone()).unwrap();
            json!({"label":a["label"],"changed_overlap_cells":changed("overlap_coverage"),"changed_occupancy_cells":changed("sampled_occupancy"),
                "changed_ray_distances":changed_rays,"nearest_point_separation":ap.distance_to(bp),"nearest_distance_delta":b["observation"]["nearest_distance"].as_f64().unwrap()-a["observation"]["nearest_distance"].as_f64().unwrap()})
        }).collect();
        result.push(json!({"group":group,"a":a["name"],"b":b["name"],"route_clear_a":a["physical_check"]["route_translation_clear"],"route_clear_b":b["physical_check"]["route_translation_clear"],"variants":variants}));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_navigation_fixtures_have_valid_starts_and_declared_hull_clearances() {
        let ship = geometry(42);
        for s in fixtures() {
            let result = inspect(&s, &ship);
            assert!(!result["route_samples"].as_array().unwrap().is_empty());
        }
    }

    #[test]
    fn shape_sweep_detects_midsegment_collision_between_clear_endpoint_poses() {
        let ship = geometry(42);
        let s = fixtures()
            .into_iter()
            .find(|s| s.name == "ship-gap-40-aligned-narrow")
            .unwrap();
        let mut world = s.world.clone();
        add_actor(&mut world, &ship);
        assert!(fits(&world, &ship, s.origin, s.angle));
        assert!(fits(&world, &ship, s.goal, s.angle));
        assert!(!translates(&world, &ship, s.origin, s.goal, s.angle));
    }
}
