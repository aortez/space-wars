// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Application-supplied geometry work, aligned with the bounded loop capture.
//! Durations are CPU-observed wall time, not GPU time or thread CPU time.
use super::PROFILE;
use std::{fmt::Write, time::Duration};

macro_rules! columns {
    ($name:ident, $names:ident, $count:ident, {$($variant:ident => $label:literal),+ $(,)?}) => {
        #[derive(Clone, Copy)]
        pub enum $name { $($variant),+ }
        const $names: &[&str] = &[$($label),+];
        const $count: usize = $names.len();
    };
}

columns!(Count, COUNT_NAMES, COUNT_LEN, {
    Passes => "passes",
    SourceDraws => "source_draws",
    Vertices => "vertices",
    Indices => "indices",
    CacheHits => "cache_hits",
    CacheMisses => "cache_misses",
    CacheAdmissions => "cache_admissions",
    CacheEvictions => "cache_evictions",
    ScaleMisses => "scale_misses",
    GuardedPolygons => "guarded_polygons",
    CulledIndices => "culled_indices",
    StrokeRejections => "stroke_rejections",
});
columns!(DetailCount, DETAIL_COUNT_NAMES, DETAIL_COUNT_LEN, {
    ExactVertices => "exact_vertices",
    ScalableVertices => "scalable_vertices",
    ScalableRanges => "scalable_ranges",
    FallbackFills => "fallback_fills",
    FallbackStrokes => "fallback_strokes",
    OrdinaryDraws => "ordinary_draws",
    StrokePoints => "stroke_points",
    StrokeVertices => "stroke_vertices",
    StrokeIndices => "stroke_indices",
});
columns!(Timing, TIMING_NAMES, TIMING_LEN, {
    Encode => "encode",
    CacheBuild => "cache_build",
});
columns!(DetailTiming, DETAIL_TIMING_NAMES, DETAIL_TIMING_LEN, {
    Resolve => "resolve",
    ExactExpand => "exact_expand",
    ScalableExpand => "scalable_expand",
    FallbackFill => "fallback_fill",
    FallbackStroke => "fallback_stroke",
    Ordinary => "ordinary",
    Validate => "validate",
    StrokePrepare => "stroke_prepare",
    StrokeTessellate => "stroke_tessellate",
    StrokeFinish => "stroke_finish",
    Visibility => "visibility",
});

/// One or more completed geometry passes. No heap allocation or formatting on
/// record. Detailed fields stay absent unless the producer calls `detail`.
#[derive(Clone, Default)]
pub struct Sample {
    counts: [u64; COUNT_LEN],
    timings: [Duration; TIMING_LEN],
    detail_passes: u64,
    detail_counts: [u64; DETAIL_COUNT_LEN],
    detail_timings: [Duration; DETAIL_TIMING_LEN],
}
impl Sample {
    pub fn count(&mut self, counter: Count, value: u64) {
        self.counts[counter as usize] += value;
    }
    pub fn time(&mut self, timing: Timing, value: Duration) {
        self.timings[timing as usize] += value;
    }
    pub fn detail(&mut self, counts: &[(DetailCount, u64)], timings: &[(DetailTiming, Duration)]) {
        self.detail_passes += 1;
        for (counter, value) in counts {
            self.detail_counts[*counter as usize] += value;
        }
        for (timing, value) in timings {
            self.detail_timings[*timing as usize] += *value;
        }
    }
    fn accumulate(&mut self, other: &Self) {
        for (a, b) in self.counts.iter_mut().zip(other.counts) {
            *a += b;
        }
        for (a, b) in self.timings.iter_mut().zip(other.timings) {
            *a += b;
        }
        self.detail_passes += other.detail_passes;
        for (a, b) in self.detail_counts.iter_mut().zip(other.detail_counts) {
            *a += b;
        }
        for (a, b) in self.detail_timings.iter_mut().zip(other.detail_timings) {
            *a += b;
        }
    }
    pub(super) fn header(text: &mut String) {
        for name in COUNT_NAMES {
            let _ = write!(text, ",geometry_{name}");
        }
        for name in TIMING_NAMES {
            let _ = write!(text, ",geometry_{name}_wall_ms");
        }
        text.push_str(",geometry_detail_passes");
        for name in DETAIL_COUNT_NAMES {
            let _ = write!(text, ",geometry_{name}");
        }
        for name in DETAIL_TIMING_NAMES {
            let _ = write!(text, ",geometry_{name}_wall_ms");
        }
    }
    pub(super) fn row(&self, text: &mut String) {
        for count in self.counts {
            let _ = write!(text, ",{count}");
        }
        for time in self.timings {
            text.push(',');
            if self.counts[Count::Passes as usize] > 0 {
                let _ = write!(text, "{:.6}", super::millis(time));
            }
        }
        let _ = write!(text, ",{}", self.detail_passes);
        for count in self.detail_counts {
            text.push(',');
            if self.detail_passes > 0 {
                let _ = write!(text, "{count}");
            }
        }
        for time in self.detail_timings {
            text.push(',');
            if self.detail_passes > 0 {
                let _ = write!(text, "{:.6}", super::millis(time));
            }
        }
    }
}

/// None outside an active, complete-loop capture. Some(false) requests existing
/// coarse counters/times only; Some(true) explicitly enables encoding clocks.
pub fn capture_mode() -> Option<bool> {
    PROFILE.with_borrow(|p| {
        (p.current.is_some() && p.trace.recording()).then(|| p.trace.geometry_recording())
    })
}

pub fn record(sample: &Sample) {
    PROFILE.with_borrow_mut(|p| {
        if !p.trace.recording() {
            return;
        }
        if let Some(current) = &mut p.current {
            current.geometry.accumulate(sample);
        }
    });
}

#[cfg(all(test, feature = "renderer-femtovg"))]
mod tests {
    use super::*;
    use crate::profiling::{activate_femtovg, render_trace, Scope, Stage};

    #[test]
    fn geometry_capture_is_explicit_bounded_and_accumulates_camera_passes() {
        activate_femtovg((640, 480), None, Some(8));
        assert_eq!(capture_mode(), None);
        for command in ["start", "start-work", "start-geometry"] {
            render_trace(command).unwrap();
            assert_eq!(capture_mode(), None); // Partial request loop.
            for _ in 0..240 {
                let _iteration = Scope::new(Stage::Loop);
                assert_eq!(capture_mode(), Some(command == "start-geometry"));
                let mut sample = Sample::default();
                sample.count(Count::Passes, 1);
                sample.count(Count::Vertices, 17);
                record(&sample);
                record(&sample);
            }
            assert_eq!(capture_mode(), None);
            let report = render_trace("read").unwrap();
            {
                let _iteration = Scope::new(Stage::Loop);
                record(&Sample::default());
                assert_eq!(capture_mode(), None);
            }
            assert_eq!(render_trace("read").unwrap(), report);
            let rows: Vec<_> = report
                .lines()
                .filter(|l| !l.contains('=') && !l.starts_with('#'))
                .collect();
            let columns: Vec<_> = rows[0].split(',').collect();
            let values: Vec<_> = rows[1].split(',').collect();
            assert_eq!(columns.len(), values.len());
            assert_eq!(
                values[columns
                    .iter()
                    .position(|c| *c == "geometry_passes")
                    .unwrap()],
                "2"
            );
            assert_eq!(
                values[columns
                    .iter()
                    .position(|c| *c == "geometry_vertices")
                    .unwrap()],
                "34"
            );
        }
        render_trace("start-geometry").unwrap();
        {
            let _iteration = Scope::new(Stage::Loop);
        }
        render_trace("cancel").unwrap();
        assert_eq!(capture_mode(), None);
    }

    #[test]
    fn unmeasured_geometry_detail_is_empty_not_zero() {
        let mut header = String::new();
        Sample::header(&mut header);
        let header: Vec<_> = header.split(',').collect();
        let column = header
            .iter()
            .position(|c| *c == "geometry_fallback_stroke_wall_ms")
            .unwrap();
        let mut sample = Sample::default();
        let mut row = String::new();
        sample.row(&mut row);
        assert_eq!(header.len(), row.split(',').count());
        assert_eq!(row.split(',').nth(column), Some(""));
        sample.detail(&[], &[(DetailTiming::FallbackStroke, Duration::ZERO)]);
        row.clear();
        sample.row(&mut row);
        assert_eq!(row.split(',').nth(column), Some("0.000000"));
    }
}
