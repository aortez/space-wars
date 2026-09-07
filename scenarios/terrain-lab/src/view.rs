use engine_common::{Camera2, RenderPoint};

use crate::TerrainLabState;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum TerrainView {
    #[default]
    Overview = 0,
    Mining = 1,
    Detail = 2,
}

impl TerrainView {
    pub fn name(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Mining => "Mining view",
            Self::Detail => "Detail view",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Overview => Self::Mining,
            Self::Mining => Self::Detail,
            Self::Detail => Self::Overview,
        }
    }
}

impl TerrainLabState {
    pub fn set_view(&mut self, view: TerrainView) {
        if self.view != view {
            self.view = view;
            // A pointer position was acquired through the old camera. Require a
            // fresh press after changing views instead of cutting at a stale point.
            self.mining.pointer = None;
        }
    }

    pub fn zoom_in(&mut self) {
        self.set_view(match self.view {
            TerrainView::Overview => TerrainView::Mining,
            TerrainView::Mining | TerrainView::Detail => TerrainView::Detail,
        });
    }

    pub fn zoom_out(&mut self) {
        self.set_view(match self.view {
            TerrainView::Overview | TerrainView::Mining => TerrainView::Overview,
            TerrainView::Detail => TerrainView::Mining,
        });
    }

    pub fn camera(&self) -> Camera2 {
        let (center, height) = match self.view {
            TerrainView::Overview => (
                self.planet_motion().position,
                self.config.radius * 2.0 + 38.0,
            ),
            TerrainView::Mining | TerrainView::Detail => {
                // Aim and disappearing target cells must not move the camera:
                // pointer aiming otherwise feeds its own camera motion back in.
                let center = self.spaceling_snapshot().motion.position;
                let profile = self.tool_profile();
                let tool_height = (profile.range + profile.cut_radius) * 2.9;
                (
                    center,
                    if self.view == TerrainView::Mining {
                        28.0_f32.max(tool_height)
                    } else {
                        16.0_f32.max(tool_height)
                    },
                )
            }
        };
        Camera2::new(RenderPoint::new(center.x, center.y), height)
    }
}
