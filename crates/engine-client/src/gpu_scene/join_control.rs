//! Session-only exact round joins. Reference remains the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum JoinMode {
    #[default]
    Reference,
    Exact,
    /// Run the original subdivision calculation to verify every shortcut.
    Checked,
}

impl JoinMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Exact => "exact",
            Self::Checked => "checked",
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "reference" => Ok(Self::Reference),
            "exact" => Ok(Self::Exact),
            "checked" => Ok(Self::Checked),
            _ => Err("gpu-joins requires reference, exact or checked"),
        }
    }

    #[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    pub fn policy(self) -> lyon_tessellation::RoundJoinMode {
        match self {
            Self::Reference => lyon_tessellation::RoundJoinMode::Reference,
            Self::Exact => lyon_tessellation::RoundJoinMode::Exact,
            Self::Checked => lyon_tessellation::RoundJoinMode::Checked,
        }
    }
}

#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(super) fn delta(
    after: lyon_tessellation::RoundJoinStats,
    before: lyon_tessellation::RoundJoinStats,
) -> lyon_tessellation::RoundJoinStats {
    lyon_tessellation::RoundJoinStats {
        shortcuts: after.shortcuts.saturating_sub(before.shortcuts),
        fallbacks: after.fallbacks.saturating_sub(before.fallbacks),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyon_tessellation::{RoundJoinMode, RoundJoinStats};

    #[test]
    fn reference_is_default_and_controls_map_to_the_exact_specializations() {
        assert_eq!(JoinMode::default(), JoinMode::Reference);
        for (mode, policy) in [
            (JoinMode::Reference, RoundJoinMode::Reference),
            (JoinMode::Exact, RoundJoinMode::Exact),
            (JoinMode::Checked, RoundJoinMode::Checked),
        ] {
            assert_eq!(JoinMode::parse(mode.name()), Ok(mode));
            assert_eq!(mode.policy(), policy);
        }
        for invalid in ["", "on", "off", "auto", "check", "exact\nstatus"] {
            assert!(JoinMode::parse(invalid).is_err());
        }
    }

    #[test]
    fn counter_deltas_are_bounded_at_saturation_or_reset() {
        let before = RoundJoinStats {
            shortcuts: u64::MAX - 2,
            fallbacks: 10,
        };
        let after = RoundJoinStats {
            shortcuts: u64::MAX,
            fallbacks: 15,
        };
        assert_eq!(
            delta(after, before),
            RoundJoinStats {
                shortcuts: 2,
                fallbacks: 5
            }
        );
        assert_eq!(delta(after, after), RoundJoinStats::default());
        assert_eq!(
            delta(RoundJoinStats::default(), after),
            RoundJoinStats::default()
        );
    }
}
