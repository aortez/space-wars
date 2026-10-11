//! Direct scene geometry renderer, with session-only diagnostic controls.
//! Scenarios still emit the
//! same RenderFrame; Slint receives one GPU texture plus the existing text/UI.

use crate::{
    MainWindow,
    render::{FrameLayout, Viewport},
};
use engine_common::RenderFrame;

mod stream_control;
pub(crate) use stream_control::StreamMode;
mod stroke_control;
pub(crate) use stroke_control::StrokeMode;
mod join_control;
pub(crate) use join_control::JoinMode;
mod writer_control;
pub(crate) use writer_control::StrokeWriter;
mod admission_control;
pub(crate) use admission_control::AdmissionMode;
mod plan_control;
pub(crate) use plan_control::PlanMode;
pub(crate) mod source_control;
pub(crate) use source_control::SourceMode;

#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod authored;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
mod circles;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod encode_profile;
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
mod frame_profile;
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
mod gpu;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod mesh;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod plan;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod presentation;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod resident;
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
pub(crate) mod resident_storage;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod retained;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod reusable;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod scalable;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod small_strokes;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod streaming;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod stroke_census;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod stroke_culling;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod stroke_output;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod stroke_single_pass;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) mod submit_profile;
#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
mod visibility;
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
pub(crate) use gpu::composite::Presenter as CompositePresenter;
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
pub(crate) use gpu::triangles::Presenter as TrianglePresenter;
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
pub(crate) use gpu::{DrawOutput, Presenter, after_rendering, before_rendering, teardown};
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
pub(crate) use gpu::{target_counts, validate_retirement};
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
pub(crate) use mesh::GeometryMode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Off,
    Paths,
    Triangles,
    Circles,
    Retained,
    RetainedCold,
    RetainedAdaptive,
    RetainedZoom,
    RetainedHybrid,
    RetainedOutlines,
    RetainedPaced,
    RetainedMemoized,
    RetainedScan,
    RetainedRanges,
    RetainedCulled,
    RetainedResident,
    RetainedResidentPasses,
    RetainedResidentBuffers,
    RetainedResidentUniforms,
}

impl Mode {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Paths => "paths",
            Self::Triangles => "triangles",
            Self::Circles => "circles",
            Self::Retained => "retained",
            Self::RetainedCold => "retained-cold",
            Self::RetainedAdaptive => "retained-adaptive",
            Self::RetainedZoom => "retained-zoom",
            Self::RetainedHybrid => "retained-hybrid",
            Self::RetainedOutlines => "retained-outlines",
            Self::RetainedPaced => "retained-paced",
            Self::RetainedMemoized => "retained-memoized",
            Self::RetainedScan => "retained-scan",
            Self::RetainedRanges => "retained-ranges",
            Self::RetainedCulled => "retained-culled",
            Self::RetainedResident => "retained-resident",
            Self::RetainedResidentPasses => "retained-resident-passes",
            Self::RetainedResidentBuffers => "retained-resident-buffers",
            Self::RetainedResidentUniforms => "retained-resident-uniforms",
        }
    }

    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    pub(crate) fn retention(self) -> Option<retained::Policy> {
        match self {
            Self::Retained => Some(retained::Policy::Always),
            Self::RetainedCold => Some(retained::Policy::Cold),
            Self::RetainedAdaptive => Some(retained::Policy::Adaptive),
            Self::RetainedZoom => Some(retained::Policy::ScaleIndependent),
            Self::RetainedHybrid => Some(retained::Policy::Hybrid),
            Self::RetainedOutlines => Some(retained::Policy::HybridOutlines),
            Self::RetainedPaced => Some(retained::Policy::HybridPaced),
            Self::RetainedMemoized => Some(retained::Policy::HybridMemoized),
            Self::RetainedScan => Some(retained::Policy::HybridScan),
            Self::RetainedRanges => Some(retained::Policy::HybridRanges),
            Self::RetainedCulled => Some(retained::Policy::HybridCulled),
            Self::RetainedResident => Some(retained::Policy::HybridResident),
            Self::RetainedResidentPasses => Some(retained::Policy::HybridResidentPasses),
            Self::RetainedResidentBuffers => Some(retained::Policy::HybridResidentBuffers),
            Self::RetainedResidentUniforms => Some(retained::Policy::HybridResidentUniforms),
            _ => None,
        }
    }

    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    pub(crate) fn supported(self, gl: &glow::Context) -> bool {
        self.retention().is_none_or(|p| p.supported(gl))
    }
}

pub(crate) fn request(window: &MainWindow, mode: Mode) -> Result<(), String> {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::request(window, mode);
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    {
        let _ = (window, mode);
        Err("gpu-scene requires the pi-kiosk-gpu build running linuxkms-femtovg".into())
    }
}

/// Apply the supported profile once per scenario, never every frame. An
/// explicit CLI override or a rendering failure therefore stays in effect for
/// the rest of that session. New scenarios start from a known configuration.
pub(crate) fn start_session(
    window: &MainWindow,
    renderer: crate::host::RenderBackend,
    raster_scale: f32,
    native_video: bool,
) -> Result<(), String> {
    end_session();
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    {
        let gpu_active = i_slint_backend_linuxkms::profiling::overlay_summary()
            .is_some_and(|summary| summary.renderer == "femtovg");
        if crate::renderer_policy::direct_scene_default(
            gpu_active,
            renderer,
            raster_scale,
            native_video,
        ) {
            gpu::request_default(window)?;
        }
    }
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    let _ = (window, renderer, raster_scale, native_video);
    Ok(())
}

pub(crate) fn requested() -> bool {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::requested();
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    false
}

pub(crate) fn request_stream(window: &MainWindow, mode: StreamMode) -> Result<(), String> {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::request_stream(window, mode);
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    {
        let _ = (window, mode);
        Err("gpu-stream requires the pi-kiosk-gpu build running linuxkms-femtovg".into())
    }
}

pub(crate) fn request_strokes(window: &MainWindow, mode: StrokeMode) -> Result<(), String> {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::request_strokes(window, mode);
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    {
        let _ = (window, mode);
        Err("gpu-strokes requires the pi-kiosk-gpu build running linuxkms-femtovg".into())
    }
}

pub(crate) fn request_plan(window: &MainWindow, mode: PlanMode) -> Result<(), String> {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::request_plan(window, mode);
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    {
        let _ = (window, mode);
        Err("gpu-plan requires the pi-kiosk-gpu build running linuxkms-femtovg".into())
    }
}

pub(crate) fn request_joins(window: &MainWindow, mode: JoinMode) -> Result<(), String> {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::request_joins(window, mode);
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    {
        let _ = (window, mode);
        Err("gpu-joins requires the pi-kiosk-gpu build running linuxkms-femtovg".into())
    }
}

pub(crate) fn request_writer(window: &MainWindow, mode: StrokeWriter) -> Result<(), String> {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::request_writer(window, mode);
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    {
        let _ = (window, mode);
        Err("gpu-stroke-writer requires the pi-kiosk-gpu build running linuxkms-femtovg".into())
    }
}

pub(crate) fn request_admission(window: &MainWindow, mode: AdmissionMode) -> Result<(), String> {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::request_admission(window, mode);
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    {
        let _ = (window, mode);
        Err("gpu-admission requires the pi-kiosk-gpu build running linuxkms-femtovg".into())
    }
}

pub(crate) fn request_source(window: &MainWindow, mode: SourceMode) -> Result<(), String> {
    if !requested() {
        return Err("gpu-source requires an active GPU scene".into());
    }
    source_control::select(mode);
    use slint::ComponentHandle;
    window.window().request_redraw();
    Ok(())
}

pub(crate) fn applied() -> bool {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::applied();
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    false
}

pub(crate) fn end_session() {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    gpu::end_session();
}

pub(crate) fn stage(
    frames: &[RenderFrame],
    viewport: Viewport,
    layout: FrameLayout,
    scale: f32,
) -> bool {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::stage(frames, viewport, layout, scale);
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    {
        let _ = (frames, viewport, layout, scale);
        false
    }
}

pub(crate) fn diagnostics() -> String {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    return gpu::diagnostics() + &source_control::diagnostics();
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    "\ngpu_scene_requested=false\ngpu_scene_applied=false".into()
}
