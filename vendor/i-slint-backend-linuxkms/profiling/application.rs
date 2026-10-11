// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Application-owned spans aligned with the same KMS loop as UI and GL work.
//! No clocks outside an explicitly armed `start-work` or `start-geometry`
//! capture. Nested spans are inclusive; HostTick contains the other Host stages.
use super::{Scope as BackendScope, Stage as BackendStage, Stamp, PROFILE};

/// Coarse boundaries, never per-entity or per-vertex instrumentation.
#[derive(Clone, Copy)]
pub enum Stage {
    /// Whole scenario-host timer callback, excluding launcher-only polls.
    HostTick,
    /// Input/action processing and simulation stepping.
    HostStep,
    /// First-result model publication and menu/visibility properties.
    HostResult,
    /// Scenario render frames and input projection construction.
    HostScene,
    /// Project/stage scene data and publish text/image properties to Slint.
    HostPublish,
    /// Diagnostic strings, performance labels and the center panel.
    HostDiagnostics,
    /// Application BeforeRendering callback, including direct GPU scene work.
    BeforeRender,
    /// Application AfterRendering callback, including resource retirement.
    AfterRender,
    /// Validate and project source frames into the ordered GPU presentation plan.
    SceneProject,
    /// Publish that plan to the renderer, including replacing any pending plan.
    SceneStage,
    /// Publish the separate Slint text overlay and presentation properties.
    SceneOverlay,
    /// Release temporary RenderFrames after staging a GPU scene.
    SourceDrop,
    /// Encode/submit the scene, before releasing its temporary presentation plan.
    SceneDraw,
    /// Release or recycle the CPU plan after drawing, not GPU resources.
    PlanDrop,
}
impl Stage {
    fn backend(self) -> BackendStage {
        match self {
            Self::HostTick => BackendStage::AppHostTick,
            Self::HostStep => BackendStage::AppHostStep,
            Self::HostResult => BackendStage::AppHostResult,
            Self::HostScene => BackendStage::AppHostScene,
            Self::HostPublish => BackendStage::AppHostPublish,
            Self::HostDiagnostics => BackendStage::AppHostDiagnostics,
            Self::BeforeRender => BackendStage::AppBeforeRender,
            Self::AfterRender => BackendStage::AppAfterRender,
            Self::SceneProject => BackendStage::AppSceneProject,
            Self::SceneStage => BackendStage::AppSceneStage,
            Self::SceneOverlay => BackendStage::AppSceneOverlay,
            Self::SourceDrop => BackendStage::AppSourceDrop,
            Self::SceneDraw => BackendStage::AppSceneDraw,
            Self::PlanDrop => BackendStage::AppPlanDrop,
        }
    }
}

/// Records one inclusive CPU/wall span during work/geometry captures only.
pub struct Scope {
    _inner: BackendScope,
}
impl Scope {
    /// Starts only within a fully armed loop, never the partial request loop.
    pub fn new(stage: Stage) -> Self {
        let enabled = PROFILE.with(|p| {
            let p = p.borrow();
            p.current.is_some() && p.trace.application_recording()
        });
        Self {
            _inner: BackendScope {
                stage: stage.backend(),
                start: enabled.then(Stamp::now),
            },
        }
    }
}

#[cfg(all(test, feature = "renderer-femtovg"))]
mod tests {
    use super::*;
    use crate::profiling::{activate_femtovg, render_trace};

    #[test]
    fn work_scopes_require_explicit_capture_and_do_not_time_partial_or_later_loops() {
        activate_femtovg((640, 480), None, Some(8));
        for command in ["start", "start-text", "start-work", "start-geometry"] {
            let work = matches!(command, "start-work" | "start-geometry");
            render_trace(command).unwrap();
            assert!(Scope::new(Stage::HostTick)._inner.start.is_none());
            for _ in 0..240 {
                let _iteration = BackendScope::new(BackendStage::Loop);
                let host = Scope::new(Stage::HostTick);
                assert_eq!(host._inner.start.is_some(), work);
                {
                    let _step = Scope::new(Stage::HostStep);
                }
                drop(host);
            }
            PROFILE.with(|p| {
                let p = p.borrow();
                assert!(!p.trace.recording());
                let last = p.samples.back().unwrap();
                for s in [BackendStage::AppHostTick, BackendStage::AppHostStep] {
                    assert_eq!(last.stages[s as usize].calls, u64::from(work));
                }
            });
            let _iteration = BackendScope::new(BackendStage::Loop);
            assert!(Scope::new(Stage::HostTick)._inner.start.is_none());
        }
    }

    #[test]
    fn scope_finishes_on_early_return_and_cancel_keeps_completed_samples() {
        activate_femtovg((640, 480), None, Some(8));
        render_trace("start-work").unwrap();
        fn application() -> Result<(), ()> {
            let _scope = Scope::new(Stage::HostPublish);
            Err(())
        }
        {
            let _iteration = BackendScope::new(BackendStage::Loop);
            assert!(application().is_err());
        }
        PROFILE.with(|p| {
            assert_eq!(
                p.borrow().samples.back().unwrap().stages[BackendStage::AppHostPublish as usize]
                    .calls,
                1
            );
        });
        render_trace("cancel").unwrap();
        let recorded = render_trace("read").unwrap();
        {
            let _iteration = BackendScope::new(BackendStage::Loop);
            assert!(Scope::new(Stage::HostPublish)._inner.start.is_none());
        }
        assert_eq!(render_trace("read").unwrap(), recorded);
        assert!(recorded.contains("kms_trace_application_detail=true"));
    }

    #[test]
    fn scene_substages_are_capture_only_and_present_in_the_report() {
        activate_femtovg((640, 480), None, Some(8));
        let stages = [
            Stage::SceneProject,
            Stage::SceneStage,
            Stage::SceneOverlay,
            Stage::SourceDrop,
            Stage::SceneDraw,
            Stage::PlanDrop,
        ];
        for command in ["start", "start-text", "start-work", "start-geometry"] {
            let work = matches!(command, "start-work" | "start-geometry");
            render_trace(command).unwrap();
            {
                let _iteration = BackendScope::new(BackendStage::Loop);
                for stage in stages {
                    let scope = Scope::new(stage);
                    assert_eq!(scope._inner.start.is_some(), work);
                }
            }
            PROFILE.with(|p| {
                let p = p.borrow();
                let sample = p.samples.back().unwrap();
                for stage in stages {
                    assert_eq!(
                        sample.stages[stage.backend() as usize].calls,
                        u64::from(work)
                    );
                }
            });
            render_trace("cancel").unwrap();
            let report = render_trace("read").unwrap();
            for name in [
                "scene_project",
                "scene_stage",
                "scene_overlay",
                "source_drop",
                "scene_draw",
                "plan_drop",
            ] {
                assert!(report.contains(&format!("app_{name}_calls")));
            }
        }
    }
}
