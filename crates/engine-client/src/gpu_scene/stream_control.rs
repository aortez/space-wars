//! Session-only upload selection, independent of geometry retention and pacing.
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
use super::Mode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum StreamMode {
    #[default]
    Shared,
    Exact,
    Bucketed,
}

impl StreamMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Exact => "exact",
            Self::Bucketed => "bucketed",
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "shared" => Ok(Self::Shared),
            "exact" => Ok(Self::Exact),
            "bucketed" => Ok(Self::Bucketed),
            _ => Err("gpu-stream requires shared, exact or bucketed"),
        }
    }

    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    pub fn policy(self) -> super::streaming::Policy {
        match self {
            Self::Shared => super::streaming::Policy::SharedExact,
            Self::Exact => super::streaming::Policy::PassExact,
            Self::Bucketed => super::streaming::Policy::PassBucketed,
        }
    }
}

#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
impl Mode {
    pub fn supports_stream_control(self) -> bool {
        matches!(
            self,
            Self::Triangles
                | Self::Retained
                | Self::RetainedCold
                | Self::RetainedAdaptive
                | Self::RetainedZoom
                | Self::RetainedHybrid
                | Self::RetainedOutlines
                | Self::RetainedPaced
                | Self::RetainedMemoized
                | Self::RetainedScan
                | Self::RetainedRanges
                | Self::RetainedCulled
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_control_is_explicit_and_defaults_to_the_original_path() {
        assert_eq!(StreamMode::default(), StreamMode::Shared);
        for mode in [StreamMode::Shared, StreamMode::Exact, StreamMode::Bucketed] {
            assert_eq!(StreamMode::parse(mode.name()), Ok(mode));
        }
        for value in ["", "on", "off", "auto", "split", "exact\nstatus"] {
            assert!(StreamMode::parse(value).is_err());
        }
        for mode in [Mode::Triangles, Mode::RetainedRanges, Mode::RetainedCulled] {
            assert!(mode.supports_stream_control());
        }
        for mode in [
            Mode::Off,
            Mode::Paths,
            Mode::Circles,
            Mode::RetainedResident,
            Mode::RetainedResidentPasses,
            Mode::RetainedResidentBuffers,
            Mode::RetainedResidentUniforms,
        ] {
            assert!(!mode.supports_stream_control());
        }
    }
}
