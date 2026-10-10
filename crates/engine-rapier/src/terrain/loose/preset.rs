use super::*;

/// Shared comparison settings for surface shedding, bank failure and impact
/// chipping. Material blast hardness remains a separate terrain property.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DirtStrength {
    #[default]
    Off,
    Weak,
    Strong,
}
impl DirtStrength {
    pub fn config(self) -> Option<CohesionConfig> {
        Some(CohesionConfig {
            strength: match self {
                Self::Off => return None,
                Self::Weak => 500.0,
                Self::Strong => 4000.0,
            },
            ..Default::default()
        })
    }
    pub fn impact_config(self) -> Option<ImpactFractureConfig> {
        Some(ImpactFractureConfig {
            yield_speed: match self {
                Self::Off => return None,
                Self::Weak => 6.0,
                Self::Strong => 24.0,
            },
            ..Default::default()
        })
    }
    pub fn apply(self, mut config: LooseTerrainConfig) -> LooseTerrainConfig {
        config.slumping = (self != Self::Off).then(SlumpingConfig::default);
        config.cohesion = self.config();
        config.impact_fracture = self.impact_config();
        config
    }
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Weak,
            Self::Weak => Self::Strong,
            Self::Strong => Self::Off,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Weak => "weak",
            Self::Strong => "strong",
        }
    }
}
