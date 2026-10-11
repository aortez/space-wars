//! Session-only CPU preparation control. All paths produce the same owned plan;
//! Reserved storage retains nothing; scratch reuses bounded CPU allocations,
//! never geometry results or GPU resources.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PlanMode {
    #[default]
    Reference,
    Reserved,
    Scratch,
    /// Scratch storage with camera bounds and hint rotations prepared once.
    Prepared,
}

impl PlanMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Reserved => "reserved",
            Self::Scratch => "scratch",
            Self::Prepared => "prepared",
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "reference" => Ok(Self::Reference),
            "reserved" => Ok(Self::Reserved),
            "scratch" => Ok(Self::Scratch),
            "prepared" => Ok(Self::Prepared),
            _ => Err("gpu-plan requires reference, reserved, scratch or prepared"),
        }
    }

    pub fn reuses_storage(self) -> bool {
        matches!(self, Self::Scratch | Self::Prepared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_control_is_explicit_and_preserves_the_reference_default() {
        assert_eq!(PlanMode::default(), PlanMode::Reference);
        for mode in [
            PlanMode::Reference,
            PlanMode::Reserved,
            PlanMode::Scratch,
            PlanMode::Prepared,
        ] {
            assert_eq!(PlanMode::parse(mode.name()), Ok(mode));
            assert_eq!(
                mode.reuses_storage(),
                matches!(mode, PlanMode::Scratch | PlanMode::Prepared)
            );
        }
        for value in ["", "auto", "on", "reserved\nstatus"] {
            assert!(PlanMode::parse(value).is_err());
        }
    }
}
