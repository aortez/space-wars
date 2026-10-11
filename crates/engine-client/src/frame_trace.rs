//! Optional application spans in the bounded KMS render trace. Software and
//! non-kiosk builds compile these calls into no-ops, without clocks or storage.
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
pub(crate) use i_slint_backend_linuxkms::profiling::application::{Scope, Stage};

#[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
#[allow(dead_code)]
mod disabled {
    pub(crate) enum Stage {
        HostTick,
        HostStep,
        HostResult,
        HostScene,
        HostPublish,
        HostDiagnostics,
        BeforeRender,
        AfterRender,
        SceneProject,
        SceneStage,
        SceneOverlay,
        SourceDrop,
        SceneDraw,
        PlanDrop,
    }
    pub(crate) struct Scope;
    impl Scope {
        pub(crate) fn new(_: Stage) -> Self {
            Self
        }
    }
}
#[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(crate) use disabled::{Scope, Stage};
