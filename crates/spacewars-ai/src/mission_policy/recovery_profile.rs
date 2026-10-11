//! Pinned recovery configurations for integration hosts and retained comparisons.
use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum MissionRecoveryProfile {
    /// Ordinary mission recovery; no experimental recovery options.
    #[default]
    #[serde(rename = "standard")]
    Standard,
    /// Bounded pod reconsideration, rebuild search, staging and precise arrival.
    #[serde(rename = "terrain_recovery_v1")]
    TerrainV1,
    /// TerrainV1 with measured-route steering until hatch arrival has support.
    #[serde(rename = "terrain_recovery_supported_hatch_v1")]
    SupportedHatchV1,
}

impl MissionRecoveryProfile {
    pub const ALL: [Self; 3] = [Self::Standard, Self::TerrainV1, Self::SupportedHatchV1];

    pub fn id(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::TerrainV1 => "terrain_recovery_v1",
            Self::SupportedHatchV1 => "terrain_recovery_supported_hatch_v1",
        }
    }
}

impl std::str::FromStr for MissionRecoveryProfile {
    type Err = String;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|profile| profile.id() == id)
            .ok_or_else(|| format!("unknown mission recovery profile {id:?}"))
    }
}
