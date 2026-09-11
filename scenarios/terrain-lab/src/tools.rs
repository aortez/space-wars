use crate::{DRILL_DAMAGE, DRILL_INTERVAL_TICKS, DRILL_RADIUS, DRILL_RANGE, TerrainLabState};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum MiningTool {
    Precision = 0,
    #[default]
    Drill = 1,
    Excavator = 2,
}

impl MiningTool {
    pub const ALL: [Self; 3] = [Self::Precision, Self::Drill, Self::Excavator];

    pub fn name(self) -> &'static str {
        match self {
            Self::Precision => "PRECISION LASER",
            Self::Drill => "DRILL",
            Self::Excavator => "EXCAVATOR",
        }
    }

    pub fn next(self) -> Self {
        Self::ALL[(self as usize + 1) % Self::ALL.len()]
    }
}

/// World-unit tool dimensions, quantized only when a hit becomes a terrain edit.
/// Zero cut dimensions select exactly the first solid cell hit by the beam.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MiningToolProfile {
    pub range: f32,
    pub cut_half_width: f32,
    pub cut_radius: f32,
    pub damage: u8,
    pub interval_ticks: u32,
}

impl MiningToolProfile {
    pub const DEFAULTS: [Self; 3] = [
        Self {
            range: 5.0,
            cut_half_width: 0.0,
            cut_radius: 0.0,
            damage: 20,
            interval_ticks: 3,
        },
        Self {
            range: DRILL_RANGE,
            cut_half_width: DRILL_RADIUS,
            cut_radius: 0.5,
            damage: DRILL_DAMAGE,
            interval_ticks: DRILL_INTERVAL_TICKS,
        },
        Self {
            range: 6.0,
            cut_half_width: 2.5,
            cut_radius: 1.0,
            damage: 30,
            interval_ticks: 12,
        },
    ];

    pub(super) fn normalized(self, default: Self) -> Self {
        let bounded = |value: f32, fallback, min, max| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        };
        let cut_half_width = bounded(self.cut_half_width, default.cut_half_width, 0.0, 4.0);
        Self {
            range: bounded(self.range, default.range, 0.5, 12.0),
            cut_half_width,
            cut_radius: bounded(self.cut_radius, default.cut_radius, 0.0, 4.0).min(cut_half_width),
            damage: self.damage.max(1),
            interval_ticks: self.interval_ticks.clamp(1, 120),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ToolControls {
    pub cycle_tool: bool,
    pub cycle_view: bool,
}

impl TerrainLabState {
    pub fn selected_tool(&self) -> MiningTool {
        self.mining.tool
    }

    pub fn tool_profile(&self) -> MiningToolProfile {
        self.config.mining_tools[self.selected_tool() as usize]
    }

    pub fn select_tool(&mut self, tool: MiningTool) {
        // Preserve the previous pulse's cooldown so switching cannot accelerate it.
        self.mining.tool = tool;
    }
}
