//! Session-only switch for the experimental live FemtoVG glyph atlas.
//! No saved preferences, renderer selection, physics or UI layout changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Vector,
    Cpu,
    CpuRunFaces,
    CpuShapeFaces,
}
impl Mode {
    pub(crate) fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "vector" => Ok(Self::Vector),
            "cpu" => Ok(Self::Cpu),
            "cpu-run-faces" => Ok(Self::CpuRunFaces),
            "cpu-shape-faces" => Ok(Self::CpuShapeFaces),
            _ => Err("gpu-text requires vector, cpu, cpu-run-faces or cpu-shape-faces"),
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Vector => "vector",
            Self::Cpu => "cpu",
            Self::CpuRunFaces => "cpu-run-faces",
            Self::CpuShapeFaces => "cpu-shape-faces",
        }
    }
}
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
impl From<Mode> for femtovg::GlyphCacheMode {
    fn from(mode: Mode) -> Self {
        match mode {
            Mode::Vector => Self::Vector,
            Mode::Cpu => Self::Cpu,
            Mode::CpuRunFaces => Self::CpuRunFaces,
            Mode::CpuShapeFaces => Self::CpuShapeFaces,
        }
    }
}
pub(crate) fn request(window: &crate::MainWindow, mode: Mode) -> Result<(), String> {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    {
        use slint::ComponentHandle;
        if !i_slint_backend_linuxkms::profiling::diagnostics()
            .lines()
            .any(|l| l == "kms_renderer=femtovg")
        {
            return Err("gpu-text requires the active LinuxKMS FemtoVG renderer".into());
        }
        femtovg::request_glyph_cache(mode.into());
        window.window().request_redraw();
        Ok(())
    }
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    {
        let _ = (window, mode);
        Err("gpu-text requires Linux and the pi-kiosk-gpu build".into())
    }
}
pub(crate) fn diagnostics() -> String {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    {
        let s = femtovg::glyph_cache_stats();
        let mode = match s.requested {
            femtovg::GlyphCacheMode::Vector => "vector",
            femtovg::GlyphCacheMode::Cpu => "cpu",
            femtovg::GlyphCacheMode::CpuRunFaces => "cpu-run-faces",
            femtovg::GlyphCacheMode::CpuShapeFaces => "cpu-shape-faces",
        };
        format!(
            "\ngpu_text_available=true\ngpu_text_requested={mode}\ngpu_text_cpu_canvases={}\ngpu_text_pages={}\ngpu_text_entries={}\ngpu_text_cpu_pixel_bytes={}\ngpu_text_rasterized_total={}\ngpu_text_face_parses_total={}\ngpu_text_uploads_total={}\ngpu_text_upload_bytes_total={}\ngpu_text_fallback_runs_total={}\ngpu_text_resets_total={}\ngpu_text_errors_total={}",
            s.cpu_canvases,
            s.pages,
            s.entries,
            s.pages * 512 * 512 * 4,
            s.rasterized,
            s.face_parses,
            s.uploads,
            s.uploaded_bytes,
            s.fallback_runs,
            s.resets,
            s.errors
        )
    }
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    {
        "\ngpu_text_available=false\ngpu_text_requested=vector".into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_requires_explicit_known_name() {
        for mode in [
            Mode::Vector,
            Mode::Cpu,
            Mode::CpuRunFaces,
            Mode::CpuShapeFaces,
        ] {
            assert_eq!(Mode::parse(mode.name()), Ok(mode));
        }
        for value in ["", "on", "off", "auto", "CPU"] {
            assert!(Mode::parse(value).is_err());
        }
    }
}
