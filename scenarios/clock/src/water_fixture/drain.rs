//! One-sided flow across the actual Clock floor panels. Keeping the aperture
//! fixed isolates wall contact from both the actuator and opposing-jet mixing.
use super::*;
use crate::floor::responsive::FloorShape;
use engine_core::Vec2;

pub const DRAIN_SPLASH_SEEDS: [u64; 3] = [7, 19, 61];

#[derive(Clone, Copy)]
pub struct DrainSplashCase {
    pub name: &'static str,
    pub opening: f64,
    pub depths: [f64; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplashTreatment {
    Off,
    Restrained,
    Lively,
}

impl SplashTreatment {
    pub const ALL: [Self; 3] = [Self::Off, Self::Restrained, Self::Lively];

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Restrained => "restrained",
            Self::Lively => "lively",
        }
    }

    pub fn config(self) -> Option<engine_water::SplashConfig> {
        match self {
            Self::Off => None,
            Self::Restrained => Some(engine_water::SplashConfig {
                energy_fraction: 0.3,
                interval: 0.55,
                ..engine_water::SplashConfig::default()
            }),
            Self::Lively => Some(engine_water::SplashConfig::default()),
        }
    }

    pub fn seeded_config(self, seed: Option<u64>) -> Option<engine_water::SplashConfig> {
        if let Some(seed) = seed
            && self == Self::Lively
        {
            return Some(crate::floor::responsive::drain_splash(seed));
        }
        self.config().map(|mut config| {
            if let Some(seed) = seed {
                config.interval = 0.45;
                config.variation = Some(engine_water::SplashVariation {
                    seed,
                    max_interval: 0.85,
                });
            }
            config
        })
    }
}

impl DrainSplashCase {
    pub fn fixture(self, treatment: SplashTreatment) -> DrainFixture {
        self.seeded_fixture(treatment, None)
    }

    pub fn seeded_fixture(self, treatment: SplashTreatment, seed: Option<u64>) -> DrainFixture {
        DrainFixture::splash_lab(self.opening, self.depths, treatment.seeded_config(seed))
    }
}

pub const DRAIN_SPLASH_CASES: [DrainSplashCase; 7] = [
    DrainSplashCase {
        name: "gentle",
        opening: 0.25,
        depths: [1.0, 1.0],
    },
    DrainSplashCase {
        name: "opposed",
        opening: 0.25,
        depths: [5.0, 5.0],
    },
    DrainSplashCase {
        name: "heavy",
        opening: 0.25,
        depths: [15.0, 15.0],
    },
    DrainSplashCase {
        name: "wall-right",
        opening: 0.25,
        depths: [5.0, 0.0],
    },
    DrainSplashCase {
        name: "wall-left",
        opening: 0.25,
        depths: [0.0, 5.0],
    },
    DrainSplashCase {
        name: "unequal",
        opening: 0.5,
        depths: [15.0, 5.0],
    },
    DrainSplashCase {
        name: "wide",
        opening: 0.8,
        depths: [5.0, 5.0],
    },
];

pub struct DrainFixture {
    pub water: WaterWorld,
    shape: FloorShape,
    opening: f64,
    depths: [f64; 2],
}

impl DrainFixture {
    pub fn new(opening: f64, depth: f64, mirrored: bool) -> Self {
        Self::splash_lab(
            opening,
            if mirrored { [0.0, depth] } else { [depth, 0.0] },
            None,
        )
    }

    /// Identical panels and replenishment for the opt-in splash/control pair.
    pub fn splash_lab(
        opening: f64,
        depths: [f64; 2],
        splash: Option<engine_water::SplashConfig>,
    ) -> Self {
        assert!((0.0..=1.0).contains(&opening));
        assert!(depths.iter().all(|d| (0.0..=20.0).contains(d)));
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
                splash,
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
            depths,
        }
    }

    pub fn step(&mut self, dt: f64) {
        self.step_feeding(dt, true);
    }

    pub fn step_feeding(&mut self, dt: f64, feeding: bool) {
        // Replenish only the donor reservoir. All flight, impacts and subsequent
        // flow from the opposite panel come from the production water solver.
        if feeding {
            for (source, depth) in self.depths.into_iter().enumerate() {
                for i in 0..self.shape.columns {
                    let c = self.water.pools()[source].columns().nth(i).unwrap();
                    let missing = (depth * c.width - c.volume).max(0.0);
                    if missing > 0.0 {
                        self.water
                            .add_to_pool(source, c.left + c.width * 0.5, missing)
                            .unwrap();
                    }
                }
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
    fn splash_lab_is_repeatable_conservative_and_drains_after_feeding_stops() {
        for hz in [30, 60, 120] {
            for case in DRAIN_SPLASH_CASES {
                for seed in [None, Some(7), Some(19), Some(61)] {
                    let mut f = case.seeded_fixture(SplashTreatment::Lively, seed);
                    let mut replay = case.seeded_fixture(SplashTreatment::Lively, seed);
                    let mut peak_spray_y = f32::NEG_INFINITY;
                    for tick in 0..hz * 16 {
                        let feeding = tick < hz * 4;
                        f.step_feeding(1.0 / f64::from(hz), feeding);
                        replay.step_feeding(1.0 / f64::from(hz), feeding);
                        assert_eq!(f.water.parcels(), replay.water.parcels());
                        let s = f.water.stats();
                        assert_eq!(s, replay.water.stats());
                        assert!(
                            (s.injected - s.pooled - s.in_flight - s.drained).abs()
                                < s.injected.max(1.0) * 1.0e-9
                        );
                        assert!(s.parcels <= 256);
                        for p in f.water.parcels() {
                            assert!(
                                !f.inside_panel(p.position, 0.001),
                                "{} {hz}Hz: {p:?}",
                                case.name
                            );
                            if p.velocity.y > 0.0 {
                                peak_spray_y = peak_spray_y.max(p.position.y);
                            }
                        }
                    }
                    let s = f.water.stats();
                    if case.name == "gentle" {
                        assert_eq!(s.splash_bursts, 0);
                    }
                    assert!(
                        s.in_flight < s.injected * 0.001,
                        "{} {hz}Hz: {s:?}",
                        case.name
                    );
                    // Closed outer walls and the finite lip may retain a film.
                    assert!(s.drained > s.injected * 0.99, "{} {hz}Hz: {s:?}", case.name);
                    println!(
                        "{} {hz}Hz seed={seed:?}: bursts={} spray={:.3} peak_y={peak_spray_y:.2} remaining={:.3}",
                        case.name,
                        s.splash_bursts,
                        s.splash_volume,
                        s.pooled + s.in_flight
                    );
                    f.water.reclaim();
                    assert_eq!(f.water.stats().pooled + f.water.stats().in_flight, 0.0);
                }
            }
        }
    }

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
