// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Short, frame-aligned diagnostic captures, separate from rolling aggregates.
//! Storage is allocated only on explicit start and never grows past LIMIT.

use super::{millis, LoopSample, Stage, STAGES};
use std::fmt::Write;
use std::time::{Duration, Instant};

const LIMIT: usize = 240;

#[derive(Default, PartialEq, Eq)]
enum State {
    #[default]
    Idle,
    Armed,
    Recording,
    Complete,
    Cancelled,
}

#[derive(Default)]
pub(super) struct Capture {
    state: State,
    samples: Vec<(u64, LoopSample)>,
    text_detail: bool,
    application_detail: bool,
    pub(super) geometry_detail: bool,
    origin: Option<Instant>,
}

impl Capture {
    pub(super) fn recording(&self) -> bool {
        self.state == State::Recording
    }

    #[cfg(feature = "renderer-femtovg")]
    pub(super) fn text_recording(&self) -> bool {
        self.recording() && self.text_detail
    }

    pub(super) fn application_recording(&self) -> bool {
        self.recording() && self.application_detail
    }

    pub(super) fn geometry_recording(&self) -> bool {
        self.recording() && self.geometry_detail
    }

    pub(super) fn start(
        &mut self,
        text_detail: bool,
        application_detail: bool,
    ) -> Result<(), &'static str> {
        if matches!(self.state, State::Armed | State::Recording) {
            return Err("render trace already active; read or cancel it first");
        }
        self.samples = Vec::with_capacity(LIMIT);
        self.text_detail = text_detail;
        self.application_detail = application_detail;
        self.geometry_detail = false;
        self.origin = None;
        // Requests run inside an event-loop iteration. Start at the next loop
        // so no partial sample is mistaken for a fully instrumented frame.
        self.state = State::Armed;
        Ok(())
    }

    pub(super) fn begin_loop(&mut self, now: Instant) {
        if self.state == State::Armed {
            self.state = State::Recording;
            self.origin = Some(now);
        }
    }

    pub(super) fn offset(&self, now: Instant) -> Option<Duration> {
        self.recording().then_some(())?;
        now.checked_duration_since(self.origin?)
    }

    pub(super) fn record(&mut self, sequence: u64, sample: &LoopSample) {
        if self.recording() {
            self.samples.push((sequence, sample.clone()));
            if self.samples.len() == LIMIT {
                self.state = State::Complete;
            }
        }
    }

    pub(super) fn cancel(&mut self) {
        if matches!(self.state, State::Armed | State::Recording) {
            self.state = State::Cancelled;
        }
    }

    pub(super) fn status(&self) -> String {
        let state = match self.state {
            State::Idle => "idle",
            State::Armed => "armed",
            State::Recording => "recording",
            State::Complete => "complete",
            State::Cancelled => "cancelled",
        };
        format!(
            "\nkms_trace_state={state}\nkms_trace_samples={}\nkms_trace_limit={LIMIT}\nkms_trace_text_detail={}\nkms_trace_application_detail={}\nkms_trace_geometry_detail={}",
            self.samples.len(), self.text_detail, self.application_detail, self.geometry_detail
        )
    }

    pub(super) fn report(&self) -> String {
        let mut text = format!(
            "kms_trace_version=6{}\n# CPU-observed times, not GPU execution; nested stages must not be added.\n# Text stages cover the full loop, including measurement outside drawing.\n# marker: 0 unspecified, 1 gameplay, 2 paused, 3 game-over, 4 launcher\n# *_at_ms use a capture-relative CPU clock. flip_kernel_us is the raw DRM event timestamp, not a duration; do not subtract these clock epochs.\n# flip_sequence is the kernel vblank sequence (u32 wrapping). The consumed flip belongs to an earlier submission. Event fields are empty when absent; with multiple events they describe the last one.\n# geometry_* aggregates completed triangle passes, not UI/GL work. Geometry times are wall time; detail is empty if unmeasured. Cache build is inside resolve; stroke subphases are inside fallback_stroke.\nloop,marker,draws,gl_draws,texture_bytes,buffer_bytes,flip_submissions,flip_completions,modesets,flip_read_errors,loop_start_at_ms,submit_at_ms,flip_observed_at_ms,flip_sequence,flip_kernel_us",
            self.status()
        );
        super::geometry::Sample::header(&mut text);
        // Keep idle/absent calls explicit. The compact subset excludes the
        // software-only breakdown but retains all CPU/driver transition stages.
        let stages: Vec<_> = STAGES
            .into_iter()
            .filter(|(stage, _)| {
                matches!(
                    stage,
                    Stage::Loop
                        | Stage::Timers
                        | Stage::Callbacks
                        | Stage::Render
                        | Stage::Dispatch
                        | Stage::Draw
                        | Stage::FlipWait
                        | Stage::Submit
                        | Stage::EglSwap
                        | Stage::GlTextureAllocate
                        | Stage::GlTextureUpload
                        | Stage::GlTextureDelete
                        | Stage::GlBufferUpload
                        | Stage::GlDraw
                        | Stage::GlCompile
                        | Stage::GlLink
                        | Stage::FemtoBuildItems
                        | Stage::FemtoAfterItems
                        | Stage::FemtoTextMeasure
                        | Stage::FemtoTextBreak
                        | Stage::FemtoTextShape
                        | Stage::FemtoTextShapeMiss
                        | Stage::FemtoTextDraw
                        | Stage::FemtoCpuMaskBuild
                        | Stage::FemtoCpuMaskUpload
                        | Stage::FemtoGlyphQuads
                        | Stage::FemtoPathFill
                        | Stage::FemtoPathStroke
                        | Stage::AppHostTick
                        | Stage::AppHostStep
                        | Stage::AppHostResult
                        | Stage::AppHostScene
                        | Stage::AppHostPublish
                        | Stage::AppHostDiagnostics
                        | Stage::AppBeforeRender
                        | Stage::AppAfterRender
                        | Stage::AppSceneProject
                        | Stage::AppSceneStage
                        | Stage::AppSceneOverlay
                        | Stage::AppSourceDrop
                        | Stage::AppSceneDraw
                        | Stage::AppPlanDrop
                )
            })
            .collect();
        for (_, name) in &stages {
            let _ = write!(text, ",{name}_calls,{name}_wall_ms,{name}_cpu_ms");
        }
        for (sequence, sample) in &self.samples {
            use super::Counter;
            let _ = write!(
                text,
                "\n{sequence},{},{},{},{},{},{},{},{},{}",
                sample.trace_marker,
                sample.counters[Counter::Draws as usize],
                sample.counters[Counter::GlDrawCalls as usize],
                sample.counters[Counter::GlUploadBytes as usize],
                sample.counters[Counter::GlBufferBytes as usize],
                sample.counters[Counter::FlipSubmissions as usize],
                sample.counters[Counter::FlipCompletions as usize],
                sample.counters[Counter::Modesets as usize],
                sample.counters[Counter::FlipReadErrors as usize]
            );
            let events = sample.presentation;
            for time in [events.loop_start, events.submitted, events.flip_observed] {
                text.push(',');
                if let Some(time) = time {
                    let _ = write!(text, "{:.6}", millis(time));
                }
            }
            text.push(',');
            if let Some(sequence) = events.flip_sequence {
                let _ = write!(text, "{sequence}");
            }
            text.push(',');
            if let Some(time) = events.flip_kernel_time {
                let _ = write!(text, "{}", time.as_micros());
            }
            sample.geometry.row(&mut text);
            for (stage, _) in &stages {
                let s = sample.stages[*stage as usize];
                let _ = write!(text, ",{},{:.6},", s.calls, millis(s.elapsed.wall));
                // Empty CPU values mean unmeasured, not zero.
                if let Some(cpu) = s.elapsed.cpu {
                    let _ = write!(text, "{:.6}", millis(cpu));
                }
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_starts_at_a_loop_boundary_stops_at_limit_and_preserves_result() {
        let mut c = Capture::default();
        assert!(c.status().contains("state=idle"));
        assert_eq!(c.samples.capacity(), 0);
        c.start(false, false).unwrap();
        assert!(c.start(true, true).is_err());
        c.record(1, &LoopSample::default());
        assert!(c.samples.is_empty());
        c.begin_loop(Instant::now());
        for sequence in 2..=500 {
            c.record(sequence, &LoopSample::default());
        }
        assert_eq!(c.samples.len(), LIMIT);
        assert_eq!(c.samples.capacity(), LIMIT);
        assert!(!c.recording());
        assert!(c.status().contains("state=complete"));
        let report = c.report();
        c.cancel();
        assert_eq!(c.report(), report);
        assert_eq!(c.samples[0].0, 2);
        c.start(true, true).unwrap();
        assert!(c.samples.is_empty());
        c.cancel();
        c.begin_loop(Instant::now());
        assert!(c.status().contains("state=cancelled"));
        assert!(!c.recording());
    }

    #[test]
    fn cancellation_keeps_completed_loops_and_missing_cpu_is_not_zero() {
        let mut c = Capture::default();
        c.start(false, false).unwrap();
        c.begin_loop(Instant::now());
        let mut sample = LoopSample {
            trace_marker: 3,
            ..Default::default()
        };
        sample.stages[Stage::Draw as usize].add(super::super::Elapsed {
            wall: std::time::Duration::from_millis(123),
            cpu: None,
        });
        c.record(77, &sample);
        c.cancel();
        c.record(78, &sample);
        assert_eq!(c.samples.len(), 1);
        let report = c.report();
        let rows: Vec<_> = report
            .lines()
            .filter(|line| !line.starts_with('#') && !line.contains('='))
            .collect();
        let header: Vec<_> = rows[0].split(',').collect();
        let values: Vec<_> = rows[1].split(',').collect();
        assert_eq!(header.len(), values.len());
        assert_eq!(values[1], "3");
        assert_eq!(
            values[header.iter().position(|h| *h == "draw_wall_ms").unwrap()],
            "123.000000"
        );
        assert_eq!(
            values[header.iter().position(|h| *h == "draw_cpu_ms").unwrap()],
            ""
        );
    }

    #[test]
    fn capture_clock_starts_at_the_first_full_loop_and_resets_on_restart() {
        let mut c = Capture::default();
        let first = Instant::now();
        assert_eq!(c.offset(first), None);
        c.start(false, false).unwrap();
        assert_eq!(c.offset(first), None);
        c.begin_loop(first);
        assert_eq!(c.offset(first), Some(Duration::ZERO));
        let next = first + Duration::from_millis(20);
        c.begin_loop(next);
        assert_eq!(c.offset(next), Some(Duration::from_millis(20)));
        c.cancel();
        assert_eq!(c.offset(next), None);
        c.start(false, false).unwrap();
        c.begin_loop(next);
        assert_eq!(c.offset(next), Some(Duration::ZERO));
        for i in 0..LIMIT {
            c.record(i as u64, &LoopSample::default());
        }
        assert_eq!(c.offset(next), None);
    }

    #[test]
    fn presentation_columns_preserve_absence_zero_sequence_and_separate_clocks() {
        let mut c = Capture::default();
        c.start(false, false).unwrap();
        c.begin_loop(Instant::now());
        c.record(1, &LoopSample::default());
        let mut sample = LoopSample {
            presentation: super::super::PresentationSample {
                loop_start: Some(Duration::from_millis(10)),
                submitted: Some(Duration::from_millis(25)),
                flip_observed: Some(Duration::from_millis(24)),
                flip_sequence: Some(0),
                flip_kernel_time: Some(Duration::from_micros(123_456_789)),
            },
            ..LoopSample::default()
        };
        sample.counters[super::super::Counter::FlipSubmissions as usize] = 1;
        sample.counters[super::super::Counter::FlipCompletions as usize] = 1;
        c.record(2, &sample);
        let report = c.report();
        let rows: Vec<_> = report
            .lines()
            .filter(|line| !line.starts_with('#') && !line.contains('='))
            .map(|line| line.split(',').collect::<Vec<_>>())
            .collect();
        assert_eq!(rows[0].len(), rows[1].len());
        assert_eq!(rows[0].len(), rows[2].len());
        for (key, value) in [
            ("loop_start_at_ms", "10.000000"),
            ("submit_at_ms", "25.000000"),
            ("flip_observed_at_ms", "24.000000"),
            ("flip_sequence", "0"),
            ("flip_kernel_us", "123456789"),
        ] {
            let column = rows[0].iter().position(|name| *name == key).unwrap();
            assert_eq!(rows[1][column], "");
            assert_eq!(rows[2][column], value);
        }
        let column = rows[0]
            .iter()
            .position(|name| *name == "flip_completions")
            .unwrap();
        assert_eq!(rows[1][column], "0");
        assert_eq!(rows[2][column], "1");
    }

    #[test]
    fn presentation_hooks_are_inert_outside_a_bounded_recording() {
        use super::super::{page_flip_completed, page_flip_submitted, Scope, PROFILE};
        std::thread::spawn(|| {
            super::super::activate_for_test((32, 32));
            {
                let _scope = Scope::new(Stage::Loop);
                page_flip_submitted();
                page_flip_completed(5, Duration::from_secs(99));
            }
            PROFILE.with_borrow(|p| {
                let events = p.samples.back().unwrap().presentation;
                assert!(events.loop_start.is_none());
                assert!(events.submitted.is_none());
                assert!(events.flip_kernel_time.is_none());
            });
            PROFILE.with_borrow_mut(|p| p.trace.start(false, false).unwrap());
            {
                let _scope = Scope::new(Stage::Loop);
                page_flip_completed(u32::MAX, Duration::from_secs(123));
                page_flip_submitted();
            }
            PROFILE.with_borrow_mut(|p| {
                let events = p.samples.back().unwrap().presentation;
                assert_eq!(events.loop_start, Some(Duration::ZERO));
                assert!(events.flip_observed >= events.loop_start);
                assert!(events.submitted >= events.flip_observed);
                assert_eq!(events.flip_sequence, Some(u32::MAX));
                assert_eq!(events.flip_kernel_time, Some(Duration::from_secs(123)));
                p.trace.cancel();
            });
            {
                let _scope = Scope::new(Stage::Loop);
                page_flip_submitted();
                page_flip_completed(0, Duration::from_secs(124));
            }
            PROFILE.with_borrow(|p| {
                assert!(p
                    .samples
                    .back()
                    .unwrap()
                    .presentation
                    .flip_sequence
                    .is_none());
                assert_eq!(p.trace.samples.len(), 1);
            });
        })
        .join()
        .unwrap();
    }
}
