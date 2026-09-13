//! The Pi kiosk uses Slint's software backend, whose Path drawing is a no-op.
//! Keep this presentation restriction out of scenarios and headless conversion.

use engine_common::{RendererSetting, Settings};

use crate::host::RenderBackend;

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
