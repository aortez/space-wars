use engine_common::ClockCrowWaterTolerance;
use rand::{Rng, SeedableRng, rngs::StdRng};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WaterTolerance {
    pub kind: ClockCrowWaterTolerance,
    pub foot_depth: f32,
    pub soak_ticks: f32,
    pub avoids_spray: bool,
}

impl WaterTolerance {
    pub fn new(requested: ClockCrowWaterTolerance, seed: u64) -> Self {
        let kind = if requested == ClockCrowWaterTolerance::Varied {
            // A separate stream keeps temperament independent of perch choices,
            // facing, the water source and other event randomness.
            let mut rng = StdRng::seed_from_u64(seed ^ 0x6372_6f77_7765_7421);
            if rng.random_bool(0.1) {
                ClockCrowWaterTolerance::Hardy
            } else {
                ClockCrowWaterTolerance::Shy
            }
        } else {
            requested
        };
        match kind {
            ClockCrowWaterTolerance::Hardy => Self {
                kind,
                // Rain pools on digit tops often reach 0.5–0.8 cell pitches.
                // Allow wading/hopping through them, but reject deeper water.
                foot_depth: 0.85,
                soak_ticks: 180.0,
                avoids_spray: false,
            },
            _ => Self {
                kind,
                foot_depth: 0.04,
                soak_ticks: 18.0,
                avoids_spray: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varied_is_repeatable_and_usually_shy_while_forced_choices_are_exact() {
        let mut hardy = 0;
        for seed in 0..1000 {
            let varied = WaterTolerance::new(ClockCrowWaterTolerance::Varied, seed);
            assert_eq!(
                varied,
                WaterTolerance::new(ClockCrowWaterTolerance::Varied, seed)
            );
            assert_ne!(varied.kind, ClockCrowWaterTolerance::Varied);
            hardy += usize::from(varied.kind == ClockCrowWaterTolerance::Hardy);
            for kind in [ClockCrowWaterTolerance::Shy, ClockCrowWaterTolerance::Hardy] {
                assert_eq!(WaterTolerance::new(kind, seed).kind, kind);
            }
        }
        assert!(
            (50..=150).contains(&hardy),
            "{hardy} hardy crows / 1000 seeds"
        );
        eprintln!("{hardy} hardy crows / 1000 seeds");
    }
}
