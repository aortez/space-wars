//! Session-only exact-scale cache admission experiment, not a rendering LOD.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AdmissionMode {
    /// The existing policy: reuse a scale seen in any earlier presentation.
    #[default]
    Repeated,
    /// Require the scale in three consecutive presentations before promotion.
    Settled,
}

impl AdmissionMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Repeated => "repeated",
            Self::Settled => "settled",
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "repeated" => Ok(Self::Repeated),
            "settled" => Ok(Self::Settled),
            _ => Err("gpu-admission requires repeated or settled"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_is_explicit_and_keeps_the_existing_default() {
        assert_eq!(AdmissionMode::default(), AdmissionMode::Repeated);
        for mode in [AdmissionMode::Repeated, AdmissionMode::Settled] {
            assert_eq!(AdmissionMode::parse(mode.name()), Ok(mode));
        }
        for invalid in ["", "auto", "on", "off", "settled\nstatus"] {
            assert!(AdmissionMode::parse(invalid).is_err());
        }
    }
}
