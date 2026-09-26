//! Scoped ownership of the ordinary floor.
//!
//! Face-only events make no claim. The visit's course or moving panels can be
//! claimed jointly with Rain, Falling or Meltdown; the ordinary floor returns
//! only after both release.
//! Standalone physical events retain exclusive floor ownership.

pub(crate) mod responsive;

use engine_common::{ClockEventKind, ClockFloorMode};
use engine_core::Vec2;
#[cfg(test)]
use engine_water::{Boundary, PoolSpec, WaterConfig, WaterWorld};

use crate::layout::Layout;

#[derive(Default)]
pub(crate) struct FloorManager {
    owner: Option<FloorOwner>,
    mode: ClockFloorMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FloorOwner {
    Event(ClockEventKind),
    VisitArena {
        visit: bool,
        event: Option<ClockEventKind>,
    },
}

impl FloorManager {
    pub fn acquire(&mut self, kind: ClockEventKind) {
        if !crate::EVENT_CATALOG[kind as usize].uses_floor() {
            return;
        }
        assert!(
            self.owner.is_none(),
            "finish the previous floor owner first"
        );
        self.owner = Some(FloorOwner::Event(kind));
        self.mode = match kind {
            ClockEventKind::Falling => ClockFloorMode::DrainOpen,
            ClockEventKind::Meltdown | ClockEventKind::Duck | ClockEventKind::Rain => {
                ClockFloorMode::EventOwned
            }
            ClockEventKind::ColorCycle
            | ClockEventKind::Marquee
            | ClockEventKind::DigitSlide
            | ClockEventKind::Crow => ClockFloorMode::Closed,
        };
    }

    pub fn release(&mut self, kind: ClockEventKind) {
        if let Some(FloorOwner::VisitArena { visit, event }) = self.owner
            && event == Some(kind)
        {
            self.owner = visit.then_some(FloorOwner::VisitArena { visit, event: None });
            if !visit {
                self.mode = ClockFloorMode::Closed;
            }
            return;
        }
        self.release_owner(FloorOwner::Event(kind));
    }

    pub fn acquire_visit(&mut self) {
        if let Some(FloorOwner::Event(
            kind @ (ClockEventKind::Rain | ClockEventKind::Falling | ClockEventKind::Meltdown),
        )) = self.owner
        {
            self.owner = Some(FloorOwner::VisitArena {
                visit: true,
                event: Some(kind),
            });
            self.mode = ClockFloorMode::EventOwned;
            return;
        }
        if let Some(FloorOwner::VisitArena { visit, event }) = &mut self.owner {
            assert!(!*visit && event.is_some());
            *visit = true;
            return;
        }
        assert!(
            self.owner.is_none(),
            "finish the previous floor owner first"
        );
        self.owner = Some(FloorOwner::VisitArena {
            visit: true,
            event: None,
        });
        self.mode = ClockFloorMode::EventOwned;
    }

    pub fn release_visit(&mut self) {
        if let Some(FloorOwner::VisitArena { event, .. }) = self.owner {
            self.owner = event.map(|kind| FloorOwner::VisitArena {
                visit: false,
                event: Some(kind),
            });
            if event.is_none() {
                self.mode = ClockFloorMode::Closed;
            }
        }
    }

    pub fn acquire_visit_event(&mut self, kind: ClockEventKind) {
        assert!(matches!(
            kind,
            ClockEventKind::Rain | ClockEventKind::Falling | ClockEventKind::Meltdown
        ));
        let Some(FloorOwner::VisitArena { event, .. }) = &mut self.owner else {
            panic!("shared event requires a visit arena");
        };
        assert!(event.is_none());
        *event = Some(kind);
    }

    fn release_owner(&mut self, owner: FloorOwner) {
        if self.owner == Some(owner) {
            self.owner = None;
            self.mode = ClockFloorMode::Closed;
        }
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
    pub fn geometry(self) -> FloorGeometry {
        self.0
    }

    pub fn layout(self) -> Layout {
        self.0.layout
    }

    #[cfg(test)]
    pub fn half_width(self) -> f32 {
        self.layout().drain_half_width()
    }

    pub fn slabs(self) -> impl Iterator<Item = (Vec2, Vec2)> {
        self.0.slabs()
    }

    #[cfg(test)]
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

    /// Flat-bank baseline retained for the one-way buoyancy regression fixture.
    #[cfg(test)]
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
    manager.acquire(ClockEventKind::Falling);
    manager.geometry(layout).drain().unwrap()
}
