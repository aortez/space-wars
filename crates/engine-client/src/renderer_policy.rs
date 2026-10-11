//! Slint's software backend cannot draw Paths; the GPU backend can.
//! Keep this presentation restriction out of scenarios and headless conversion.

use engine_common::{RendererSetting, Settings};

use crate::host::RenderBackend;

#[cfg(any(feature = "pi-kiosk", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KioskRenderer {
    Software,
    FemtoVg,
}

#[cfg(any(feature = "pi-kiosk", test))]
impl KioskRenderer {
    pub(crate) fn select(backend: Option<&str>, gpu_available: bool) -> Result<Self, String> {
        let backend = backend.unwrap_or_default().to_ascii_lowercase();
        match backend.as_str() {
            "" | "linuxkms" if gpu_available => Ok(Self::FemtoVg),
            "" | "linuxkms" | "linuxkms-software" => Ok(Self::Software),
            "linuxkms-femtovg" if gpu_available => Ok(Self::FemtoVg),
            "linuxkms-femtovg" => Err(
                "linuxkms-femtovg requires a build with the pi-kiosk-gpu feature; use linuxkms-software for this build".into(),
            ),
            _ => Err(format!(
                "unsupported kiosk SLINT_BACKEND {backend:?}; use linuxkms-software or linuxkms-femtovg"
            )),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Software => "software",
            Self::FemtoVg => "femtovg",
        }
    }

    pub(crate) fn raster_only(self) -> bool {
        self == Self::Software
    }
}

/// Native-video scenarios retain their own presentation path. Respect an
/// explicit vector or supersampled raster choice rather than changing saved
/// preferences to make a scene eligible for direct geometry rendering.
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) fn direct_scene_default(
    gpu_active: bool,
    renderer: RenderBackend,
    raster_scale: f32,
    native_video: bool,
) -> bool {
    gpu_active && renderer == RenderBackend::Raster && raster_scale == 1.0 && !native_video
}

pub(crate) fn initialize(window: &crate::MainWindow) -> Result<(), String> {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    if i_slint_backend_linuxkms::profiling::overlay_summary()
        .is_some_and(|summary| summary.renderer == "femtovg")
    {
        crate::gpu_text::request(window, crate::gpu_text::Mode::CpuShapeFaces)?;
        crate::raster::publication::request_format(crate::raster::publication::UploadFormat::Rgba)?;
        tracing::info!("GPU defaults: cached triangles, bounded buffers, CPU glyph masks");
    }
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    let _ = window;
    Ok(())
}

pub(crate) fn resolve(requested: RenderBackend, raster_only: bool, source: &str) -> RenderBackend {
    if raster_only && requested == RenderBackend::Vector {
        tracing::warn!(
            source,
            requested = "vector",
            effective = "raster",
            "renderer fallback: the Pi software backend cannot draw vector paths."
        );
        RenderBackend::Raster
    } else {
        requested
    }
}

pub(crate) fn normalize_settings(settings: &mut Settings, raster_only: bool) -> bool {
    let requested = settings.launch.renderer.into();
    let effective = resolve(requested, raster_only, "saved_settings");
    if effective == requested {
        return false;
    }
    settings.launch.renderer = RendererSetting::Raster;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiosk_prefers_gpu_when_built_and_preserves_explicit_software_recovery() {
        for gpu_available in [false, true] {
            for backend in [None, Some(""), Some("linuxkms")] {
                let selected = KioskRenderer::select(backend, gpu_available).unwrap();
                assert_eq!(
                    selected.name(),
                    if gpu_available { "femtovg" } else { "software" }
                );
                assert_eq!(selected.raster_only(), !gpu_available);
            }
            assert_eq!(
                KioskRenderer::select(Some("linuxkms-software"), gpu_available),
                Ok(KioskRenderer::Software)
            );
            for backend in ["linuxkms-femtovgg", "linuxkms-skia", "winit", "software"] {
                assert!(KioskRenderer::select(Some(backend), gpu_available).is_err());
            }
        }
        assert!(KioskRenderer::select(Some("linuxkms-femtovg"), false).is_err());
        let selected = KioskRenderer::select(Some("LINUXKMS-FEMTOVG"), true).unwrap();
        assert_eq!(selected.name(), "femtovg");
        assert!(!selected.raster_only());
        assert_eq!(
            resolve(RenderBackend::Vector, selected.raster_only(), "test"),
            RenderBackend::Vector
        );
        assert_eq!(
            resolve(RenderBackend::Raster, selected.raster_only(), "test"),
            RenderBackend::Raster
        );
    }

    #[test]
    fn direct_scene_defaults_only_apply_to_native_scale_geometry_on_gpu() {
        for gpu in [false, true] {
            for renderer in [RenderBackend::Raster, RenderBackend::Vector] {
                for scale in [0.5, 1.0, 2.0, f32::NAN] {
                    for native in [false, true] {
                        assert_eq!(
                            direct_scene_default(gpu, renderer, scale, native),
                            gpu && renderer == RenderBackend::Raster && scale == 1.0 && !native
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn raster_only_policy_rejects_vector_but_preserves_raster() {
        assert_eq!(
            resolve(RenderBackend::Vector, true, "test"),
            RenderBackend::Raster
        );
        assert_eq!(
            resolve(RenderBackend::Raster, true, "test"),
            RenderBackend::Raster
        );
        assert_eq!(
            resolve(RenderBackend::Vector, false, "test"),
            RenderBackend::Vector
        );
    }

    #[test]
    fn saved_vector_is_migrated_once_without_changing_other_settings() {
        let mut settings = Settings::default();
        settings.launch.renderer = RendererSetting::Vector;
        let mut expected = settings.clone();
        expected.launch.renderer = RendererSetting::Raster;
        assert!(normalize_settings(&mut settings, true));
        assert_eq!(
            toml::to_string(&settings).unwrap(),
            toml::to_string(&expected).unwrap()
        );
        assert!(!normalize_settings(&mut settings, true));
    }

    #[test]
    fn desktop_or_headless_settings_are_unchanged() {
        let mut settings = Settings::default();
        let expected = settings.clone();
        assert!(!normalize_settings(&mut settings, false));
        assert_eq!(
            toml::to_string(&settings).unwrap(),
            toml::to_string(&expected).unwrap()
        );
    }
}
