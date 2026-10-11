//! Session-only texture-lifetime experiment. Software/default publication is
//! untouched. The GPU implementation uses Slint's borrowed-texture API, not a
//! new scene renderer; all scene geometry still goes through our CPU rasterizer.

use crate::MainWindow;
use slint::Image;

#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
mod gpu;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Slint,
    Fresh,
    Single,
    Triple,
}

impl Mode {
    pub(crate) fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "slint" => Ok(Self::Slint),
            "fresh" => Ok(Self::Fresh),
            "single" => Ok(Self::Single),
            "triple" => Ok(Self::Triple),
            _ => Err("raster-textures requires slint, fresh, single or triple"),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Slint => "slint",
            Self::Fresh => "fresh",
            Self::Single => "single",
            Self::Triple => "triple",
        }
    }

    #[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    fn slot(self, frame: u64) -> Option<usize> {
        match self {
            Self::Slint | Self::Fresh => None,
            Self::Single => Some(0),
            Self::Triple => Some((frame % 3) as usize),
        }
    }
}

pub(crate) fn request(window: &MainWindow, mode: Mode) -> Result<(), String> {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::request(window, mode);
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    {
        let _ = (window, mode);
        Err("raster-textures requires the pi-kiosk-gpu build running linuxkms-femtovg".into())
    }
}

pub(crate) fn requested() -> Mode {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::requested();
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    Mode::Slint
}

pub(crate) fn present(window: &MainWindow, image: Image) {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    if gpu::stage(&image) {
        return;
    }
    window.set_raster_frame(image);
}

pub(crate) fn diagnostics() -> String {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::diagnostics();
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    "\nraster_texture_mode=slint\nraster_texture_notifier=false".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_are_strict_and_default_does_not_change_publication() {
        assert_eq!(Mode::default(), Mode::Slint);
        for mode in [Mode::Slint, Mode::Fresh, Mode::Single, Mode::Triple] {
            assert_eq!(Mode::parse(mode.name()), Ok(mode));
        }
        for invalid in ["", "rgba", "double", "Single", "single ", "single\ntriple"] {
            assert!(Mode::parse(invalid).is_err());
        }
    }

    #[test]
    fn texture_slots_are_bounded_and_wrap_without_frame_overflow() {
        for frame in [0, 1, 2, 3, 7, 120, u64::MAX] {
            assert_eq!(Mode::Slint.slot(frame), None);
            assert_eq!(Mode::Fresh.slot(frame), None);
            assert_eq!(Mode::Single.slot(frame), Some(0));
            assert_eq!(Mode::Triple.slot(frame), Some((frame % 3) as usize));
        }
    }
}
