//! One-sided flow across the actual Clock floor panels. Keeping the aperture
//! fixed isolates wall contact from both the actuator and opposing-jet mixing.
use super::*;
use crate::floor::responsive::FloorShape;
use engine_core::Vec2;

pub struct DrainFixture {
    pub water: WaterWorld,
    shape: FloorShape,
    opening: f64,
    depth: f64,
    source: usize,
}

impl DrainFixture {
    pub fn new(opening: f64, depth: f64, mirrored: bool) -> Self {
        assert!((0.0..=1.0).contains(&opening));
        assert!((0.0..=20.0).contains(&depth));
        let shape = FloorShape {
            half_width: 80.0,
            floor_y: 0.0,
            max_gap: 14.0,
            max_drop: 12.0,
            thickness: 40.0,
            columns: 32,
            load_depth: 8.0,
        };
        let mut water = WaterWorld::new(
            WaterConfig {
                max_parcels: 256,
                exit_y: -80.0,
                ..WaterConfig::default()
            },
            shape.pools_at(opening).into(),
        )
        .unwrap();
        shape.configure_at(&mut water, opening);
        Self {
            water,
            shape,
            opening,
            depth,
            source: usize::from(mirrored),
        }
    }

    pub fn step(&mut self, dt: f64) {
        // Replenish only the donor reservoir. All flight, impacts and subsequent
        // flow from the opposite panel come from the production water solver.
        for i in 0..self.shape.columns {
            let c = self.water.pools()[self.source].columns().nth(i).unwrap();
            let missing = (self.depth * c.width - c.volume).max(0.0);
            if missing > 0.0 {
                self.water
                    .add_to_pool(self.source, c.left + c.width * 0.5, missing)
                    .unwrap();
            }
        }
        self.water.step(dt).unwrap();
    }

    pub fn inside_panel(&self, point: Vec2, inset: f32) -> bool {
        (0..2).any(|side| {
            let (center, angle) = self.shape.panel_pose(side, self.opening);
            let p = (point - center).rotate_radians(-angle);
            let half = self.shape.panel_half_extents();
            p.x.abs() < half.x - inset && p.y.abs() < half.y - inset
        })
    }

    pub fn frame(&self) -> RenderFrame {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::new(0.0, -25.0), 120.0));
        for side in 0..2 {
            frame.push_primitive(
                -1,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    self.shape
                        .panel_points(side, self.opening)
                        .map(|p| RenderPoint::new(p.x, p.y))
                        .to_vec(),
                    RenderColor::rgb(0.28, 0.38, 0.44),
                )),
            );
        }
        crate::render::water_fixture(&mut frame, &self.water);
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outfalls_do_not_cross_the_opposite_drain_panel() {
        for hz in [30, 60, 120] {
            for mirrored in [false, true] {
                for opening in [0.25, 0.5, 0.8] {
                    for depth in [1.0, 5.0, 15.0] {
                        let mut f = DrainFixture::new(opening, depth, mirrored);
                        for tick in 0..hz * 3 {
                            f.step(1.0 / f64::from(hz));
                            for p in f.water.parcels() {
                                assert!(
                                    !f.inside_panel(p.position, 0.001),
                                    "{hz} Hz mirror={mirrored} opening={opening} depth={depth} tick={tick}: {p:?}"
                                );
                            }
                            let s = f.water.stats();
                            assert!(
                                (s.injected - s.pooled - s.in_flight - s.drained).abs()
                                    < s.injected.max(1.0) * 1.0e-9,
                                "{s:?}"
                            );
                        }
                        assert!(f.water.stats().drained > 0.0);
                    }
                }
            }
        }
    }
}
