//! Prints the physical conditions a flight controller meets in generated
//! match worlds: body sizes, orbital speeds, planet spin, sun and gravity at
//! transfer altitudes. Read-only; no controller runs.
use engine_common::Scenario;
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{SurfaceSortieScenario, free_flight::CelestialKind};
use std::time::Duration;

fn main() {
    let seeds: Vec<u64> = std::env::args()
        .skip(1)
        .map(|s| s.parse().expect("seed"))
        .collect();
    let seeds = if seeds.is_empty() {
        vec![42, 1, 2, 3, 4]
    } else {
        seeds
    };
    for seed in seeds {
        let mut state = SurfaceSortieScenario::init_material_match(seed);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let environment = state.transfer_environment().expect("transfer environment");
        println!("== match seed {seed}");
        let bodies = state.celestial_bodies();
        let sun = bodies
            .iter()
            .find(|b| b.kind == CelestialKind::Sun)
            .map(|b| b.position);
        for body in &bodies {
            let spin = match body.kind {
                CelestialKind::Planet(index) => state
                    .free_flight_observation(0)
                    .filter(|o| o.planet == index)
                    .map(|o| o.planet_motion.spin)
                    .or_else(|| {
                        state
                            .free_flight_observation(1)
                            .filter(|o| o.planet == index)
                            .map(|o| o.planet_motion.spin)
                    }),
                _ => None,
            };
            println!(
                "  {:?}: radius {:.1}, speed {:.2} units/s, spin {:?}",
                body.kind,
                body.radius,
                body.velocity.length(),
                spin
            );
            if let CelestialKind::Planet(_) = body.kind {
                let away = sun.map_or(Vec2::Y, |s| (body.position - s).normalized());
                let line: Vec<String> = [30.0, 60.0, 85.0, 120.0, 150.0, 200.0]
                    .into_iter()
                    .map(|altitude| {
                        let point = body.position + away * (body.radius + altitude);
                        let g = environment.gravity(point);
                        format!("{altitude}:{:.2}", g.length())
                    })
                    .collect();
                println!(
                    "    gravity by altitude away from the sun: {}",
                    line.join("  ")
                );
                if let Some(s) = sun {
                    let toward = -away;
                    let point = body.position + toward * (body.radius + 85.0);
                    println!(
                        "    gravity at altitude 85 toward the sun: {:.2}; distance to sun {:.0}",
                        environment.gravity(point).length(),
                        point.distance_to(s)
                    );
                }
            }
        }
        for seat in 0..2 {
            if let Some(o) = state.free_flight_observation(seat) {
                println!(
                    "  seat {seat}: planet {}, gravity at ship {:.2}, frame velocity {:.2}",
                    o.planet,
                    o.gravity.length(),
                    o.frame_velocity.length()
                );
            }
        }
    }
}
