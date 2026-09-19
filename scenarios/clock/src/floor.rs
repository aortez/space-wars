//! Event-owned access to the ordinary floor. No per-tick work or extra bodies.
//!
//! Clock serializes events, so one explicit owner is sufficient. Acquire before
//! constructing event resources; release only after dropping them. A future
//! moving hatch must also move its physical/water boundaries, not just its art.

use engine_common::{ClockEventKind, ClockFloorMode};
use engine_core::Vec2;
use engine_water::{Boundary, PoolSpec, WaterConfig, WaterWorld};

use crate::{ClockWaterLab, layout::Layout};

#[derive(Default)]
pub(crate) struct FloorManager {
    owner: Option<ClockEventKind>,
    mode: ClockFloorMode,
}

impl FloorManager {
    pub fn acquire(&mut self, kind: ClockEventKind, water_lab: ClockWaterLab) {
        assert!(
            self.owner.is_none(),
            "finish the previous floor owner first"
        );
        self.owner = Some(kind);
        self.mode = match kind {
            ClockEventKind::Falling | ClockEventKind::Rain => ClockFloorMode::DrainOpen,
            ClockEventKind::Meltdown if water_lab == ClockWaterLab::Off => {
                ClockFloorMode::DrainOpen
            }
            ClockEventKind::Meltdown | ClockEventKind::Duck => ClockFloorMode::EventOwned,
            ClockEventKind::ColorCycle | ClockEventKind::Marquee | ClockEventKind::DigitSlide => {
                ClockFloorMode::Closed
            }
        };
    }

    pub fn release(&mut self) {
        self.owner = None;
        self.mode = ClockFloorMode::Closed;
    }

    pub fn mode(&self) -> ClockFloorMode {
        self.mode
    }

    pub fn geometry(&self, layout: Layout) -> FloorGeometry {
        FloorGeometry {
            layout,
            mode: self.mode,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FloorGeometry {
    layout: Layout,
    mode: ClockFloorMode,
}

impl FloorGeometry {
    /// The ordinary arena used underneath a custom event's entrance/exit fade.
    /// This is presentation only; the custom event still owns its colliders.
    pub fn closed(layout: Layout) -> Self {
        Self {
            layout,
            mode: ClockFloorMode::Closed,
        }
    }

    /// The same bounds are consumed by rendering and event colliders.
    pub fn slabs(self) -> impl Iterator<Item = (Vec2, Vec2)> {
        let layout = self.layout;
        let slabs = match self.mode {
            ClockFloorMode::Closed => [
                Some((
                    Vec2::new(layout.bounds_min.x, layout.bounds_min.y),
                    Vec2::new(layout.bounds_max.x, layout.floor_y),
                )),
                None,
            ],
            ClockFloorMode::DrainOpen => {
                let lip = layout.drain_half_width();
                [
                    Some((
                        Vec2::new(layout.bounds_min.x, layout.bounds_min.y),
                        Vec2::new(-lip, layout.floor_y),
                    )),
                    Some((
                        Vec2::new(lip, layout.bounds_min.y),
                        Vec2::new(layout.bounds_max.x, layout.floor_y),
                    )),
                ]
            }
            ClockFloorMode::EventOwned => [None, None],
        };
        slabs.into_iter().flatten()
    }

    /// Proof that this event has acquired the ordinary drain. Event-local worlds
    /// retain a copy; Clock never closes it until those worlds have been dropped.
    pub fn drain(self) -> Option<DrainGeometry> {
        (self.mode == ClockFloorMode::DrainOpen).then_some(DrainGeometry(self))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DrainGeometry(FloorGeometry);

impl DrainGeometry {
    pub fn layout(self) -> Layout {
        self.0.layout
    }

    pub fn half_width(self) -> f32 {
        self.layout().drain_half_width()
    }

    pub fn slabs(self) -> impl Iterator<Item = (Vec2, Vec2)> {
        self.0.slabs()
    }

    pub fn water_world(self, columns: usize, max_parcels: usize) -> WaterWorld {
        let layout = self.layout();
        let lip = f64::from(self.half_width());
        WaterWorld::new(
            WaterConfig {
                exit_y: f64::from(layout.bounds_min.y),
                max_parcels,
                spill_channel: Some([-lip, lip]),
                ..WaterConfig::default()
            },
            self.water_pools(columns).into(),
        )
        .expect("bounded Clock drain geometry")
    }

    /// Stable floor geometry shared by ordinary drain worlds and digit rain.
    pub fn water_pools(self, columns: usize) -> [PoolSpec; 2] {
        assert!(columns >= 2 && columns.is_multiple_of(2));
        let layout = self.layout();
        let half = f64::from(layout.bounds_max.x);
        let lip = f64::from(self.half_width());
        let floor = f64::from(layout.floor_y);
        [
            PoolSpec {
                left: -half,
                column_width: (half - lip) / (columns / 2) as f64,
                bed: vec![floor; columns / 2],
                boundaries: [Boundary::Closed, Boundary::Spill { lip: floor }],
            },
            PoolSpec {
                left: lip,
                column_width: (half - lip) / (columns / 2) as f64,
                bed: vec![floor; columns / 2],
                boundaries: [Boundary::Spill { lip: floor }, Boundary::Closed],
            },
        ]
    }
}

#[cfg(test)]
pub(crate) fn test_drain(layout: Layout) -> DrainGeometry {
    let mut manager = FloorManager::default();
    manager.acquire(ClockEventKind::Rain, ClockWaterLab::Off);
    manager.geometry(layout).drain().unwrap()
}
