// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Local, bounded LinuxKMS software-presentation diagnostics. These measure CPU
//! rendering and DRM calls, not GPU execution or verified physical scanouts.
//! Read on the UI thread. Set SPACEWARS_KMS_PROFILE=0 before startup to disable.

use nix::time::{clock_gettime, ClockId};
use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt::Write,
    time::{Duration, Instant},
};

const SAMPLE_LIMIT: usize = 120;
const STAGE_COUNT: usize = 26;
const COUNTER_COUNT: usize = 8;

#[derive(Clone, Copy)]
pub(crate) enum Stage {
    Loop,
    Timers,
    Callbacks,
    Render,
    Dispatch,
    Map,
    Draw,
    Unmap,
    FlipWait,
    Submit,
    Background,
    Copy,
    RgbBlit,
    DrawGeneric,
    DrawRgb,
    CoreRender,
    CorePrepare,
    CoreDirty,
    CoreItems,
    CoreImage,
    CoreTexture,
    CoreFallback,
    CoreRectangle,
    CoreText,
    CorePath,
    CoreBackground,
}

const STAGES: [(Stage, &str); STAGE_COUNT] = [
    (Stage::Loop, "loop"),
    (Stage::Timers, "timers"),
    (Stage::Callbacks, "callbacks"),
    (Stage::Render, "render"),
    (Stage::Dispatch, "dispatch"),
    (Stage::Map, "map"),
    (Stage::Draw, "draw"),
    (Stage::Unmap, "unmap"),
    (Stage::FlipWait, "flip_wait"),
    (Stage::Submit, "submit"),
    (Stage::Background, "background"),
    (Stage::Copy, "copy"),
    (Stage::RgbBlit, "rgb_blit"),
    (Stage::DrawGeneric, "draw_generic"),
    (Stage::DrawRgb, "draw_rgb"),
    (Stage::CoreRender, "core_render"),
    (Stage::CorePrepare, "core_prepare"),
    (Stage::CoreDirty, "core_dirty"),
    (Stage::CoreItems, "core_items"),
    (Stage::CoreImage, "core_image"),
    (Stage::CoreTexture, "core_texture"),
    (Stage::CoreFallback, "core_fallback"),
    (Stage::CoreRectangle, "core_rectangle"),
    (Stage::CoreText, "core_text"),
    (Stage::CorePath, "core_path"),
    (Stage::CoreBackground, "core_background"),
];

#[derive(Clone, Copy)]
pub(crate) enum Counter {
    Draws,
    FlipSubmissions,
    FlipCompletions,
    Modesets,
    FlipReadErrors,
    CopyBytes,
    RgbBlits,
    RgbPixels,
}

const COUNTERS: [&str; COUNTER_COUNT] = [
    "draws",
    "flip_submissions",
    "flip_completions",
    "modesets",
    "flip_read_errors",
    "copy_bytes",
    "rgb_blits",
    "rgb_pixels",
];

#[derive(Clone, Copy)]
struct Stamp {
    wall: Instant,
    cpu: Option<Duration>,
}

impl Stamp {
    fn now() -> Self {
        let wall = Instant::now();
        let cpu = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID).ok().and_then(|time| {
            Some(Duration::new(time.tv_sec().try_into().ok()?, time.tv_nsec().try_into().ok()?))
        });
        Self { wall, cpu }
    }

    fn elapsed(self) -> Elapsed {
        let end = Self::now();
        Elapsed {
            wall: end.wall.duration_since(self.wall),
            cpu: self.cpu.zip(end.cpu).and_then(|(start, end)| end.checked_sub(start)),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Elapsed {
    wall: Duration,
    cpu: Option<Duration>,
}

#[derive(Clone, Copy, Default)]
struct StageSample {
    calls: u64,
    elapsed: Elapsed,
}

impl StageSample {
    fn add(&mut self, elapsed: Elapsed) {
        self.elapsed.cpu = if self.calls == 0 {
            elapsed.cpu
        } else {
            self.elapsed.cpu.zip(elapsed.cpu).map(|(a, b)| a + b)
        };
        self.elapsed.wall += elapsed.wall;
        self.calls += 1;
    }
}

#[derive(Clone, Default)]
struct LoopSample {
    stages: [StageSample; STAGE_COUNT],
    counters: [u64; COUNTER_COUNT],
    dirty_pixels: u64,
}

struct Profile {
    active: bool,
    enabled: bool,
    size: (u32, u32),
    format: &'static str,
    buffer_bytes: usize,
    buffer_age: u8,
    buffer_mode: &'static str,
    shadow_bytes: usize,
    texture_mode: &'static str,
    draw_detail: bool,
    current: Option<LoopSample>,
    samples: VecDeque<LoopSample>,
    loops_total: u64,
    counters_total: [u64; COUNTER_COUNT],
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            active: false,
            enabled: false,
            size: (0, 0),
            format: "unknown",
            buffer_bytes: 0,
            buffer_age: 0,
            buffer_mode: "unknown",
            shadow_bytes: 0,
            texture_mode: "unknown",
            draw_detail: false,
            current: None,
            samples: VecDeque::new(),
            loops_total: 0,
            counters_total: [0; COUNTER_COUNT],
        }
    }
}

impl Profile {
    fn record(&mut self, stage: Stage, elapsed: Elapsed) {
        if let Some(sample) = &mut self.current {
            sample.stages[stage as usize].add(elapsed);
        }
    }

    fn finish_loop(&mut self, elapsed: Elapsed) {
        self.record(Stage::Loop, elapsed);
        if let Some(sample) = self.current.take() {
            self.loops_total += 1;
            if self.samples.len() == SAMPLE_LIMIT {
                self.samples.pop_front();
            }
            self.samples.push_back(sample);
        }
    }

    fn count(&mut self, counter: Counter) {
        self.add(counter, 1);
    }

    fn add(&mut self, counter: Counter, amount: u64) {
        if !self.enabled {
            return;
        }
        self.counters_total[counter as usize] += amount;
        if let Some(sample) = &mut self.current {
            sample.counters[counter as usize] += amount;
        }
    }

    fn diagnostics(&self) -> String {
        if !self.active {
            return String::new();
        }
        let mut text = format!(
            "\nkms_profile_version=5\nkms_profile_enabled={}\nkms_loop_samples={}\nkms_loops_total={}\nkms_output_width={}\nkms_output_height={}\nkms_pixel_format={}\nkms_buffer_bytes={}\nkms_buffer_age={}\nkms_buffer_mode={}\nkms_shadow_bytes={}\nkms_texture_mode={}\nkms_draw_detail={}",
            self.enabled, self.samples.len(), self.loops_total, self.size.0, self.size.1,
            self.format, self.buffer_bytes, self.buffer_age, self.buffer_mode, self.shadow_bytes, self.texture_mode, self.draw_detail,
        );
        let window_wall = self
            .samples
            .iter()
            .map(|s| s.stages[Stage::Loop as usize].elapsed.wall)
            .sum::<Duration>();
        let _ = write!(
            text,
            "\nkms_window_wall_ms={:.3}\nkms_dirty_pixels_window={}",
            millis(window_wall),
            self.samples.iter().map(|s| s.dirty_pixels).sum::<u64>()
        );
        for (index, name) in COUNTERS.iter().enumerate() {
            let count = self.samples.iter().map(|s| s.counters[index]).sum::<u64>();
            let _ = write!(
                text,
                "\nkms_{name}_total={}\nkms_{name}_window={count}",
                self.counters_total[index]
            );
        }
        for (stage, name) in STAGES {
            let samples: Vec<_> = self
                .samples
                .iter()
                .map(|s| s.stages[stage as usize])
                .filter(|s| s.calls > 0)
                .collect();
            let calls = samples.iter().map(|s| s.calls).sum::<u64>();
            let cpu_samples = samples.iter().filter(|s| s.elapsed.cpu.is_some()).count();
            let _ =
                write!(text, "\nkms_{name}_calls={calls}\nkms_{name}_cpu_samples={cpu_samples}");
            if samples.is_empty() {
                continue;
            }
            let wall: Vec<_> = samples.iter().map(|s| s.elapsed.wall).collect();
            append_stats(&mut text, name, "wall", wall);
            let cpu: Vec<_> = samples.iter().filter_map(|s| s.elapsed.cpu).collect();
            if !cpu.is_empty() {
                append_stats(&mut text, name, "cpu", cpu);
            }
        }
        text
    }
}

fn millis(time: Duration) -> f64 {
    time.as_secs_f64() * 1000.0
}

fn append_stats(text: &mut String, stage: &str, clock: &str, mut values: Vec<Duration>) {
    values.sort_unstable();
    let total = values.iter().copied().sum::<Duration>();
    let p95 = (values.len() * 95).div_ceil(100) - 1;
    let _ = write!(text,
        "\nkms_{stage}_{clock}_total_ms={:.3}\nkms_{stage}_{clock}_avg_ms={:.3}\nkms_{stage}_{clock}_p95_ms={:.3}\nkms_{stage}_{clock}_max_ms={:.3}",
        millis(total), millis(total) / values.len() as f64, millis(values[p95]), millis(*values.last().unwrap()));
}

thread_local! { static PROFILE: RefCell<Profile> = RefCell::new(Profile::default()); }

pub(crate) fn activate(size: (u32, u32)) {
    PROFILE.with(|profile| {
        *profile.borrow_mut() = Profile {
            active: true,
            enabled: std::env::var_os("SPACEWARS_KMS_PROFILE").as_deref()
                != Some(std::ffi::OsStr::new("0")),
            draw_detail: std::env::var_os("SPACEWARS_KMS_DRAW_DETAIL").as_deref()
                != Some(std::ffi::OsStr::new("0")),
            size,
            samples: VecDeque::with_capacity(SAMPLE_LIMIT),
            ..Profile::default()
        };
    });
}

#[cfg(test)]
pub(crate) fn activate_for_test(size: (u32, u32)) {
    PROFILE.with(|profile| {
        *profile.borrow_mut() =
            Profile { active: true, enabled: true, draw_detail: true, size, ..Profile::default() };
    });
}

/// Snapshot completed loop iterations on the UI thread. Formatting/sorting is
/// only done on request, never in the rendering hot path. Empty on other renderers.
pub fn diagnostics() -> String {
    PROFILE.with(|profile| profile.borrow().diagnostics())
}

pub(crate) struct Scope {
    stage: Stage,
    start: Option<Stamp>,
}

impl Scope {
    pub(crate) fn new(stage: Stage) -> Self {
        let enabled = PROFILE.with(|p| p.borrow().enabled);
        let start = enabled.then(Stamp::now);
        if matches!(stage, Stage::Loop) && enabled {
            PROFILE.with(|p| p.borrow_mut().current = Some(LoopSample::default()));
        }
        Self { stage, start }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let elapsed = start.elapsed();
            PROFILE.with(|p| {
                let mut profile = p.borrow_mut();
                if matches!(self.stage, Stage::Loop) {
                    profile.finish_loop(elapsed);
                } else {
                    profile.record(self.stage, elapsed);
                }
            });
        }
    }
}

pub(crate) fn count(counter: Counter) {
    PROFILE.with(|p| p.borrow_mut().count(counter));
}

pub(crate) fn copied(bytes: usize) {
    PROFILE.with(|p| p.borrow_mut().add(Counter::CopyBytes, bytes as u64));
}

pub(crate) fn rgb_blit(pixels: usize) {
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        p.count(Counter::RgbBlits);
        p.add(Counter::RgbPixels, pixels as u64);
    });
}

pub(crate) fn texture_mode(mode: &'static str) {
    PROFILE.with(|p| p.borrow_mut().texture_mode = mode);
}

#[cfg(feature = "renderer-software")]
pub(crate) fn draw_observer() -> Option<i_slint_core::software_renderer::DrawDiagnosticObserver> {
    PROFILE.with(|p| (p.borrow().enabled && p.borrow().draw_detail).then_some(draw_event as _))
}

#[cfg(feature = "renderer-software")]
fn draw_event(operation: i_slint_core::software_renderer::DrawDiagnostic, begin: bool) {
    use i_slint_core::software_renderer::{DrawDiagnostic as D, DRAW_DIAGNOSTIC_COUNT};
    // Fixed storage. Nested instances of the same operation are timed as one
    // outer span, avoiding double-counting recursive rendering.
    #[derive(Clone, Copy, Default)]
    struct Active {
        start: Option<Stamp>,
        depth: usize,
    }
    thread_local! {
        static ACTIVE: RefCell<[Active; DRAW_DIAGNOSTIC_COUNT]> =
            RefCell::new([Active::default(); DRAW_DIAGNOSTIC_COUNT]);
    }
    let stage = match operation {
        D::Render => Stage::CoreRender,
        D::Prepare => Stage::CorePrepare,
        D::DirtyRegion => Stage::CoreDirty,
        D::Items => Stage::CoreItems,
        D::Image => Stage::CoreImage,
        D::Texture => Stage::CoreTexture,
        D::TextureFallback => Stage::CoreFallback,
        D::Rectangle => Stage::CoreRectangle,
        D::Text => Stage::CoreText,
        D::Path => Stage::CorePath,
        D::Background => Stage::CoreBackground,
    };
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let active = &mut active[operation as usize];
        if begin {
            if active.depth == 0 {
                active.start = Some(Stamp::now());
            }
            active.depth += 1;
        } else if active.depth > 0 {
            active.depth -= 1;
            if active.depth == 0 {
                if let Some(start) = active.start.take() {
                    let elapsed = start.elapsed();
                    PROFILE.with(|p| p.borrow_mut().record(stage, elapsed));
                }
            }
        }
    });
}

pub(crate) fn render_target(mode: &'static str, shadow_bytes: usize) {
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        p.buffer_mode = mode;
        p.shadow_bytes = shadow_bytes;
    });
}

pub(crate) fn buffer(age: u8, bytes: usize, format: &'static str, dirty_pixels: u64) {
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        if !p.enabled {
            return;
        }
        p.buffer_age = age;
        p.buffer_bytes = bytes;
        p.format = format;
        if let Some(sample) = &mut p.current {
            sample.dirty_pixels += dirty_pixels;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elapsed(wall: u64, cpu: Option<u64>) -> Elapsed {
        Elapsed { wall: Duration::from_millis(wall), cpu: cpu.map(Duration::from_millis) }
    }

    #[test]
    fn completed_windows_are_bounded_and_do_not_mix_inflight_work() {
        let mut p = Profile { active: true, enabled: true, ..Profile::default() };
        for i in 0..150 {
            p.current = Some(LoopSample::default());
            p.record(Stage::Draw, elapsed(i, Some(2)));
            p.count(Counter::Draws);
            p.finish_loop(elapsed(i + 5, Some(3)));
        }
        assert_eq!(p.samples.len(), SAMPLE_LIMIT);
        p.current = Some(LoopSample::default());
        p.record(Stage::Draw, elapsed(9999, None));
        let text = p.diagnostics();
        assert!(text.contains("kms_draw_wall_avg_ms=89.500"));
        assert!(text.contains("kms_draw_wall_p95_ms=143.000"));
        assert!(text.contains("kms_draw_cpu_avg_ms=2.000"));
        assert!(text.contains("kms_draws_total=150\nkms_draws_window=120"));
        assert!(text.contains("kms_loops_total=150"));
    }

    #[test]
    fn idle_loops_waits_and_missing_cpu_clocks_are_explicit() {
        let mut p = Profile { active: true, enabled: true, ..Profile::default() };
        p.current = Some(LoopSample::default());
        p.record(Stage::Draw, elapsed(4, None));
        p.count(Counter::Draws);
        p.finish_loop(elapsed(10, None));
        p.current = Some(LoopSample::default());
        p.record(Stage::Dispatch, elapsed(50, Some(0)));
        p.finish_loop(elapsed(50, Some(0)));
        let text = p.diagnostics();
        assert!(text.contains("kms_window_wall_ms=60.000"));
        assert!(text.contains("kms_draw_calls=1\nkms_draw_cpu_samples=0"));
        assert!(!text.contains("kms_draw_cpu_avg_ms"));
        assert!(text.contains("kms_dispatch_wall_avg_ms=50.000"));
        assert!(text.contains("kms_dispatch_cpu_avg_ms=0.000"));
    }

    #[test]
    fn counters_distinguish_initial_modesets_submissions_and_completions() {
        let mut p = Profile { active: true, enabled: true, ..Profile::default() };
        p.current = Some(LoopSample::default());
        p.count(Counter::Modesets);
        p.count(Counter::FlipSubmissions);
        p.count(Counter::FlipSubmissions);
        p.count(Counter::FlipCompletions);
        p.count(Counter::FlipReadErrors);
        p.finish_loop(elapsed(1, Some(1)));
        let text = p.diagnostics();
        assert!(text.contains("kms_modesets_window=1"));
        assert!(text.contains("kms_flip_submissions_window=2"));
        assert!(text.contains("kms_flip_completions_window=1"));
        assert!(text.contains("kms_flip_read_errors_window=1"));
    }

    #[test]
    fn copy_accounting_records_bytes_and_keeps_background_nested_in_draw() {
        let mut p = Profile { active: true, enabled: true, ..Profile::default() };
        p.current = Some(LoopSample::default());
        p.record(Stage::Background, elapsed(2, Some(2)));
        p.record(Stage::Draw, elapsed(5, Some(5)));
        p.record(Stage::Copy, elapsed(1, Some(1)));
        p.add(Counter::CopyBytes, 3 * 1024 * 1024);
        p.finish_loop(elapsed(7, Some(6)));
        let text = p.diagnostics();
        assert!(text.contains("kms_background_cpu_avg_ms=2.000"));
        assert!(text.contains("kms_draw_cpu_avg_ms=5.000"));
        assert!(text.contains("kms_copy_cpu_avg_ms=1.000"));
        assert!(text.contains("kms_copy_bytes_window=3145728"));
    }

    #[test]
    fn rgb_blit_accounting_is_nested_in_draw_and_counts_only_handled_pixels() {
        let mut p =
            Profile { active: true, enabled: true, texture_mode: "rgb", ..Profile::default() };
        p.current = Some(LoopSample::default());
        p.record(Stage::RgbBlit, elapsed(1, Some(1)));
        p.record(Stage::Draw, elapsed(4, Some(4)));
        p.count(Counter::RgbBlits);
        p.add(Counter::RgbPixels, 1024 * 768);
        p.finish_loop(elapsed(6, Some(6)));
        let text = p.diagnostics();
        assert!(text.contains("kms_profile_version=5"));
        assert!(text.contains("kms_texture_mode=rgb"));
        assert!(text.contains("kms_rgb_blit_cpu_avg_ms=1.000"));
        assert!(text.contains("kms_draw_cpu_avg_ms=4.000"));
        assert!(text.contains("kms_rgb_blits_window=1"));
        assert!(text.contains("kms_rgb_pixels_window=786432"));
        assert!(text.contains("kms_copy_bytes_window=0"));
    }

    #[test]
    fn disabled_and_uninitialized_profiles_do_not_report_measurements() {
        let mut p = Profile::default();
        assert!(p.diagnostics().is_empty());
        p.active = true;
        p.count(Counter::Draws);
        let text = p.diagnostics();
        assert!(text.contains("kms_profile_enabled=false"));
        assert!(text.contains("kms_draws_total=0"));
        assert!(!text.contains("kms_draw_wall_avg_ms"));
    }

    #[test]
    fn scopes_commit_completed_work_on_early_return() {
        PROFILE.with(|p| {
            *p.borrow_mut() = Profile { active: true, enabled: true, ..Profile::default() };
        });
        let render = || -> Result<(), ()> {
            let _iteration = Scope::new(Stage::Loop);
            let _render = Scope::new(Stage::Render);
            {
                let _draw = Scope::new(Stage::Draw);
                count(Counter::Draws);
                buffer(3, 4096, "XRGB8888", 1024);
            }
            assert!(diagnostics().contains("kms_loop_samples=0"));
            Err(())
        };
        assert_eq!(render(), Err(()));
        PROFILE.with(|p| {
            let p = p.borrow();
            assert!(p.current.is_none());
            assert_eq!(p.samples.len(), 1);
            let sample = &p.samples[0];
            for stage in [Stage::Loop, Stage::Render, Stage::Draw] {
                assert_eq!(sample.stages[stage as usize].calls, 1);
            }
            assert_eq!(sample.dirty_pixels, 1024);
            assert_eq!(p.buffer_age, 3);
            assert_eq!(p.buffer_bytes, 4096);
            assert_eq!(p.format, "XRGB8888");
        });
        PROFILE.with(|p| *p.borrow_mut() = Profile::default());
        {
            let _iteration = Scope::new(Stage::Loop);
            let _draw = Scope::new(Stage::Draw);
            count(Counter::Draws);
        }
        assert!(diagnostics().is_empty());
    }

    #[test]
    fn repeated_stages_sum_within_a_loop_and_preserve_unknown_cpu() {
        let mut sample = StageSample::default();
        sample.add(elapsed(3, Some(2)));
        sample.add(elapsed(5, Some(4)));
        assert_eq!(sample.calls, 2);
        assert_eq!(sample.elapsed.wall, Duration::from_millis(8));
        assert_eq!(sample.elapsed.cpu, Some(Duration::from_millis(6)));
        sample.add(elapsed(1, None));
        sample.add(elapsed(1, Some(1)));
        assert_eq!(sample.calls, 4);
        assert_eq!(sample.elapsed.wall, Duration::from_millis(10));
        assert_eq!(sample.elapsed.cpu, None);
    }
}
