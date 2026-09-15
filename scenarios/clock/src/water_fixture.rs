//! Deterministic presentation test bed, not another launcher scenario. The
//! headless client tests capture this through the production raster/vector paths.
use engine_common::{
    Camera2, RenderColor, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive,
};
use engine_water::{Boundary, PoolSpec, WaterConfig, WaterWorld};

#[derive(Clone, Copy, Debug)]
pub enum Profile {
    Ledge,
    Steps,
    Ramp,
}

impl Profile {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ledge => "ledge",
            Self::Steps => "steps",
            Self::Ramp => "ramp",
        }
    }
}

pub struct WaterFixture {
    pub water: WaterWorld,
    depth: f64,
    mirrored: bool,
}

impl WaterFixture {
    pub fn new(depth: f64, mirrored: bool, profile: Profile) -> Self {
        assert!(depth.is_finite() && depth > 0.0 && depth <= 30.0);
        let mut bed = match profile {
            Profile::Ledge => vec![0.0],
            Profile::Steps => vec![9.0, 6.0, 3.0, 0.0],
            Profile::Ramp => (0..16).map(|i| (15 - i) as f64 * 0.4).collect(),
        };
        let boundaries = if mirrored {
            bed.reverse();
            [Boundary::Spill { lip: 0.0 }, Boundary::Closed]
        } else {
            [Boundary::Closed, Boundary::Spill { lip: 0.0 }]
        };
        let dx = 40.0 / bed.len() as f64;
        let left = if mirrored { 0.0 } else { -40.0 };
        let mut water = WaterWorld::new(
            WaterConfig {
                exit_y: -100.0,
                max_parcels: 256,
                ..WaterConfig::default()
            },
            vec![
                PoolSpec {
                    left,
                    column_width: dx,
                    bed,
                    boundaries,
                },
                PoolSpec {
                    left: if mirrored { -160.0 } else { 0.0 },
                    column_width: 5.0,
                    bed: vec![-60.0; 32],
                    boundaries: [Boundary::Closed; 2],
                },
            ],
        )
        .unwrap();
        for i in 0..water.pools()[0].spec().bed.len() {
            water
                .add_to_pool(0, left + (i as f64 + 0.5) * dx, depth * dx)
                .unwrap();
        }
        Self {
            water,
            depth,
            mirrored,
        }
    }

    pub fn step(&mut self, dt: f64, feeding: bool) {
        if feeding {
            // Hold the upstream reservoir head. Water farther downstream is
            // transported by the solver; no prescribed falling stream shape.
            let column = {
                let mut columns = self.water.pools()[0].columns();
                if self.mirrored {
                    columns.last().unwrap()
                } else {
                    columns.next().unwrap()
                }
            };
            let missing = self.depth * column.width - column.volume;
            if missing > 0.0 {
                self.water
                    .add_to_pool(0, column.left + column.width * 0.5, missing)
                    .unwrap();
            }
        }
        self.water.step(dt).unwrap();
    }

    pub fn frame(&self) -> RenderFrame {
        let center_x = if self.mirrored { -50.0 } else { 50.0 };
        frame(&self.water, center_x, -25.0, 110.0)
    }
}

/// Opposing ledges feeding a lower collector. `mix_spills = false` is the
/// ballistic pass-through control, using exactly the same source geometry.
pub struct OpposedFixture {
    pub water: WaterWorld,
    depths: [f64; 2],
}

impl OpposedFixture {
    pub fn new(depths: [f64; 2], mix_spills: bool, channel: bool) -> Self {
        assert!(
            depths
                .iter()
                .all(|d| d.is_finite() && *d > 0.0 && *d <= 30.0)
        );
        let mut water = WaterWorld::new(
            WaterConfig {
                mix_spills,
                max_parcels: 256,
                exit_y: -130.0,
                spill_channel: channel.then_some([-18.0, 18.0]),
                ..WaterConfig::default()
            },
            vec![
                PoolSpec {
                    left: -58.0,
                    column_width: 40.0,
                    bed: vec![0.0],
                    boundaries: [Boundary::Closed, Boundary::Spill { lip: 0.0 }],
                },
                PoolSpec {
                    left: 18.0,
                    column_width: 40.0,
                    bed: vec![0.0],
                    boundaries: [Boundary::Spill { lip: 0.0 }, Boundary::Closed],
                },
                PoolSpec {
                    left: -110.0,
                    column_width: 5.0,
                    bed: vec![-90.0; 44],
                    boundaries: [Boundary::Closed; 2],
                },
            ],
        )
        .unwrap();
        water.add_to_pool(0, -38.0, depths[0] * 40.0).unwrap();
        water.add_to_pool(1, 38.0, depths[1] * 40.0).unwrap();
        Self { water, depths }
    }

    pub fn step(&mut self, dt: f64, feeding: [bool; 2]) {
        for (i, feed) in feeding.into_iter().enumerate() {
            if feed {
                let c = self.water.pools()[i].columns().next().unwrap();
                let missing = (self.depths[i] * c.width - c.volume).max(0.0);
                if missing > 0.0 {
                    self.water
                        .add_to_pool(i, c.left + c.width * 0.5, missing)
                        .unwrap();
                }
            }
        }
        self.water.step(dt).unwrap();
    }

    pub fn frame(&self) -> RenderFrame {
        frame(&self.water, 0.0, -35.0, 140.0)
    }
}

fn frame(water: &WaterWorld, x: f32, y: f32, height: f32) -> RenderFrame {
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::new(x, y), height));
    for pool in water.pools() {
        for c in pool.columns() {
            frame.push_primitive(
                -1,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    vec![
                        RenderPoint::new(c.left as f32, c.bed as f32 - 4.0),
                        RenderPoint::new((c.left + c.width) as f32, c.bed as f32 - 4.0),
                        RenderPoint::new((c.left + c.width) as f32, c.bed as f32),
                        RenderPoint::new(c.left as f32, c.bed as f32),
                    ],
                    RenderColor::rgb(0.28, 0.38, 0.44),
                )),
            );
        }
    }
    crate::render::water_fixture(&mut frame, water);
    frame
}
