//! Session-only guarded stroke rejection. Keep the original path as default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum StrokeMode {
    #[default]
    Off,
    Conservative,
    /// Verify each proposed rejection against ordinary tessellation output.
    Checked,
}

impl StrokeMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Conservative => "conservative",
            Self::Checked => "checked",
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "off" => Ok(Self::Off),
            "conservative" => Ok(Self::Conservative),
            "checked" => Ok(Self::Checked),
            _ => Err("gpu-strokes requires off, conservative or checked"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_path_is_default_and_modes_are_explicit() {
        assert_eq!(StrokeMode::default(), StrokeMode::Off);
        for mode in [
            StrokeMode::Off,
            StrokeMode::Conservative,
            StrokeMode::Checked,
        ] {
            assert_eq!(StrokeMode::parse(mode.name()), Ok(mode));
        }
        for invalid in ["", "on", "auto", "check", "conservative\nstatus"] {
            assert!(StrokeMode::parse(invalid).is_err());
        }
    }
}
