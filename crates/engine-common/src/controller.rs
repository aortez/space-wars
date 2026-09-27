//! Local controller profiles. These translate hardware inputs into the engine's
//! common controller layout, not into scenario-specific actions or player seats.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerControl {
    Up,
    Down,
    Left,
    Right,
    South,
    East,
    West,
    North,
    LeftBumper,
    RightBumper,
    Select,
    Start,
    LeftTrigger,
    RightTrigger,
}

impl ControllerControl {
    pub const ALL: [Self; 14] = [
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::South,
        Self::East,
        Self::West,
        Self::North,
        Self::LeftBumper,
        Self::RightBumper,
        Self::Select,
        Self::Start,
        Self::LeftTrigger,
        Self::RightTrigger,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::South => "A / South — confirm",
            Self::East => "B / East — back",
            Self::West => "X / West",
            Self::North => "Y / North",
            Self::LeftBumper => "Left shoulder / L1",
            Self::RightBumper => "Right shoulder / R1",
            Self::Select => "Select / Back",
            Self::Start => "Start",
            Self::LeftTrigger => "Left trigger / L2",
            Self::RightTrigger => "Right trigger / R2",
        }
    }

    pub fn direction(self) -> bool {
        matches!(self, Self::Up | Self::Down | Self::Left | Self::Right)
    }

    pub fn required(self) -> bool {
        self.direction() || matches!(self, Self::South | Self::East)
    }
}

/// Codes come from gilrs, after its platform/device normalization. They are
/// scoped to a versioned, OS-specific device key; never portable between OSes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ControllerSource {
    Button { code: u32 },
    AxisPositive { code: u32 },
    AxisNegative { code: u32 },
}

impl ControllerSource {
    pub fn axis_code(self) -> Option<u32> {
        match self {
            Self::Button { .. } => None,
            Self::AxisPositive { code } | Self::AxisNegative { code } => Some(code),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerBinding {
    pub control: ControllerControl,
    pub source: ControllerSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerProfile {
    pub device_key: String,
    pub name: String,
    pub bindings: Vec<ControllerBinding>,
}

impl ControllerProfile {
    pub fn valid(&self) -> bool {
        if !self.device_key.starts_with("gilrs-v1:")
            || self.device_key.len() > 512
            || self.name.len() > 256
            || self.bindings.len() > ControllerControl::ALL.len()
        {
            return false;
        }
        for (index, binding) in self.bindings.iter().enumerate() {
            if (binding.source.axis_code().is_some() && !binding.control.direction())
                || self.bindings[..index]
                    .iter()
                    .any(|other| other.control == binding.control || other.source == binding.source)
            {
                return false;
            }
        }
        ControllerControl::ALL
            .iter()
            .filter(|c| c.required())
            .all(|control| {
                self.bindings
                    .iter()
                    .any(|binding| binding.control == *control)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> ControllerProfile {
        ControllerProfile {
            device_key: "gilrs-v1:linux:test".into(),
            name: "Cabinet".into(),
            bindings: ControllerControl::ALL
                .into_iter()
                .enumerate()
                .map(|(i, control)| ControllerBinding {
                    control,
                    source: ControllerSource::Button { code: i as u32 },
                })
                .collect(),
        }
    }

    #[test]
    fn profile_round_trips_through_settings_and_old_settings_default() {
        let mut settings = crate::Settings::default();
        settings.controls.controller_profiles.push(profile());
        settings.controls.player_1_device = Some("gilrs-v1:linux:gamepad".into());
        settings.controls.player_2_device = Some("gilrs-v1:linux:picade".into());
        let encoded = toml::to_string(&settings).unwrap();
        let decoded: crate::Settings = toml::from_str(&encoded).unwrap();
        assert_eq!(
            decoded.controls.controller_profiles,
            settings.controls.controller_profiles
        );
        assert_eq!(
            decoded.controls.player_1_device,
            settings.controls.player_1_device
        );
        assert_eq!(
            decoded.controls.player_2_device,
            settings.controls.player_2_device
        );
        let old = toml::from_str::<crate::Settings>("[controls]").unwrap();
        assert_eq!(old.controls.player_1_device, None);
        assert_eq!(old.controls.player_2_device, None);
        assert!(
            toml::from_str::<crate::Settings>("[controls]")
                .unwrap()
                .controls
                .controller_profiles
                .is_empty()
        );
    }

    #[test]
    fn validation_rejects_duplicates_missing_navigation_and_axis_buttons() {
        let original = profile();
        assert!(original.valid());
        let mut changed = original.clone();
        changed.bindings[1].source = changed.bindings[0].source;
        assert!(!changed.valid());
        let mut changed = original.clone();
        changed.bindings.remove(4);
        assert!(!changed.valid());
        let mut changed = original;
        changed.bindings[4].source = ControllerSource::AxisPositive { code: 99 };
        assert!(!changed.valid());
    }
}
