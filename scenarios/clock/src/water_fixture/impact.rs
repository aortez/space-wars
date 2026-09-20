//! Same tank and drop, with only the optional surface response changed.
use super::*;
use engine_core::Vec2;
use engine_water::Parcel;

pub struct ImpactFixture {
    pub water: WaterWorld,
}

impl ImpactFixture {
    pub fn new(impact_response: f64) -> Self {
        let mut water = WaterWorld::new(
            WaterConfig {
                impact_response,
                ..WaterConfig::default()
            },
            vec![PoolSpec {
                left: -62.5,
                column_width: 5.0,
                bed: vec![0.0; 25],
                boundaries: [Boundary::Closed; 2],
            }],
        )
        .unwrap();
        for i in 0..25 {
            water.add_to_pool(0, -60.0 + i as f64 * 5.0, 30.0).unwrap();
        }
        water
            .add_falling(Parcel {
                position: Vec2::new(0.0, 34.0),
                velocity: Vec2::new(0.0, -180.0),
                volume: 12.5,
                duration: 1.0 / 60.0,
                horizontal_bounds: None,
            })
            .unwrap();
        Self { water }
    }

    pub fn step(&mut self) {
        self.water.step(1.0 / 60.0).unwrap();
    }

    pub fn frame(&self) -> RenderFrame {
        frame(&self.water, 0.0, 12.0, 60.0)
    }
}
