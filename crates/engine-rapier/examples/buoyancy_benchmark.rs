//! Isolated coupling and active non-contact mechanics costs, not rendering or
//! a dense collision benchmark. Uniform water is deliberately not stepped.
use engine_core::Vec2;
use engine_rapier::{
    buoyancy::{BuoyancyConfig, BuoyantBody},
    world::{BodySpec, PhysicsId, PhysicsWorld, PhysicsWorldConfig},
};
use engine_water::{Boundary, PoolSpec, WaterConfig, WaterWorld, immersion::HullShape};
use std::time::Instant;

fn p95(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() * 95 / 100]
}

fn main() {
    for count in [3, 100, 1000] {
        let width = count as f64 * 20.0;
        let mut water = WaterWorld::new(
            WaterConfig::default(),
            vec![PoolSpec {
                left: 0.0,
                column_width: width / 128.0,
                bed: vec![-100.0; 128],
                boundaries: [Boundary::Closed; 2],
            }],
        )
        .unwrap();
        for i in 0..128 {
            water
                .add_to_pool(0, (i as f64 + 0.5) * width / 128.0, width * 100.0 / 128.0)
                .unwrap();
        }
        let before = water.stats();
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -40.0),
            length_unit: 10.0,
            collect_events: false,
            ..PhysicsWorldConfig::default()
        });
        world.reserve(count, count, 0);
        let bodies: Vec<_> = (0..count)
            .map(|i| {
                BuoyantBody::insert(
                    &mut world,
                    PhysicsId::new(i as u64 + 1),
                    BodySpec {
                        position: Vec2::new(i as f32 * 20.0 + 10.0, -20.0),
                        can_sleep: false,
                        ..BodySpec::default()
                    },
                    if i % 2 == 0 {
                        HullShape::Box {
                            half_width: 4.0,
                            half_height: 2.0,
                        }
                    } else {
                        HullShape::Circle { radius: 3.0 }
                    },
                    1.0,
                )
                .unwrap()
            })
            .collect();
        let mut coupling = Vec::with_capacity(600);
        let mut mechanics = Vec::with_capacity(600);
        for tick in 0..660 {
            world.clear_forces();
            let start = Instant::now();
            for body in &bodies {
                body.apply_forces(&mut world, &water, BuoyancyConfig::default(), 1.0 / 60.0)
                    .unwrap();
            }
            let coupled = start.elapsed().as_secs_f64() * 1e6;
            let start = Instant::now();
            world.step(1.0 / 60.0);
            let stepped = start.elapsed().as_secs_f64() * 1e6;
            if tick >= 60 {
                coupling.push(coupled);
                mechanics.push(stepped);
            }
        }
        assert_eq!(before, water.stats());
        println!(
            "bodies={count} ticks=600 coupling_p95={:.2}us mechanics_p95={:.2}us",
            p95(&mut coupling),
            p95(&mut mechanics)
        );
    }
}
