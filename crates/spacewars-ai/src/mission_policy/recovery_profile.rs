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
    /// SupportedHatchV1 sensing/controller with ordinary native rebuild offsets.
    #[serde(rename = "terrain_recovery_supported_hatch_ordinary_offsets_v1")]
    SupportedHatchOrdinaryOffsetsV1,
}

impl MissionRecoveryProfile {
    pub const ALL: [Self; 4] = [
        Self::Standard,
        Self::TerrainV1,
        Self::SupportedHatchV1,
        Self::SupportedHatchOrdinaryOffsetsV1,
    ];

    pub fn supported_hatch_arrival(self) -> bool {
        matches!(
            self,
            Self::SupportedHatchV1 | Self::SupportedHatchOrdinaryOffsetsV1
        )
    }

    pub fn native_refinement(self) -> bool {
        matches!(self, Self::TerrainV1 | Self::SupportedHatchV1)
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::TerrainV1 => "terrain_recovery_v1",
            Self::SupportedHatchV1 => "terrain_recovery_supported_hatch_v1",
            Self::SupportedHatchOrdinaryOffsetsV1 => {
                "terrain_recovery_supported_hatch_ordinary_offsets_v1"
            }
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
