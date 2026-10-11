use serde::{Deserialize, Serialize};

/// Shared launcher choices. The default preserves the original loose-dirt mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DirtMode {
    #[default]
    Round,
    Angular,
    WeakRound,
    WeakAngular,
    StrongRound,
    StrongAngular,
}
impl DirtMode {
    pub const ALL: [Self; 6] = [
        Self::Round,
        Self::Angular,
        Self::WeakRound,
        Self::WeakAngular,
        Self::StrongRound,
        Self::StrongAngular,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Round => "Round grains",
            Self::Angular => "Angular grains",
            Self::WeakRound => "Weak · Round",
            Self::WeakAngular => "Weak · Angular",
            Self::StrongRound => "Strong · Round",
            Self::StrongAngular => "Strong · Angular",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MaterialCombatSettings;

    #[test]
    fn old_loose_dirt_settings_keep_plain_round_grains() {
        let old: MaterialCombatSettings = toml::from_str("loose_dirt = true").unwrap();
        assert!(old.loose_dirt);
        assert_eq!(old.dirt_mode, DirtMode::Round);
        assert!(!MaterialCombatSettings::default().loose_dirt);
        for dirt_mode in DirtMode::ALL {
            let settings = MaterialCombatSettings {
                loose_dirt: true,
                dirt_mode,
                ..Default::default()
            };
            let saved = toml::to_string(&settings).unwrap();
            let loaded: MaterialCombatSettings = toml::from_str(&saved).unwrap();
            assert_eq!(loaded, settings);
        }
    }
}
