//! Bounded CPU-only host timings. These do not time Slint drawing, GPU work,
//! scanout, or the event-loop wait between callbacks.

use std::{collections::VecDeque, fmt::Write, time::Duration};

const SAMPLE_LIMIT: usize = 120;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct FrameSample {
    pub interval: Duration,
    pub step: Duration,
    pub scene: Duration,
    pub prepare: Duration,
    pub total: Duration,
    pub updates: usize,
}

#[derive(Debug, Clone, Default)]
pub(super) struct CpuProfile {
    samples: VecDeque<FrameSample>,
    dimensions: [u32; 4],
    primitives: usize,
}

impl CpuProfile {
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn record(&mut self, sample: FrameSample, dimensions: [u32; 4], primitives: usize) {
        if dimensions != self.dimensions {
            self.clear();
        }
        self.dimensions = dimensions;
        self.primitives = primitives;
        if self.samples.len() == SAMPLE_LIMIT {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn diagnostics(&self) -> String {
        if self.samples.is_empty() {
            return String::new();
        }
        let [width, height, raster_width, raster_height] = self.dimensions;
        let mut result = format!(
            "\nhost_cpu_samples={}\nhost_viewport_width={width}\nhost_viewport_height={height}\nhost_internal_width={raster_width}\nhost_internal_height={raster_height}\nhost_scene_primitives={}\nhost_updates_per_callback={:.3}",
            self.samples.len(),
            self.primitives,
            self.samples.iter().map(|s| s.updates).sum::<usize>() as f64
                / self.samples.len() as f64
        );
        for (name, select) in [
            (
                "interval",
                (|s: &FrameSample| s.interval) as fn(&FrameSample) -> Duration,
            ),
            ("step", |s: &FrameSample| s.step),
            ("scene", |s: &FrameSample| s.scene),
            ("prepare", |s: &FrameSample| s.prepare),
            ("callback", |s: &FrameSample| s.total),
        ] {
            let mut values = [Duration::ZERO; SAMPLE_LIMIT];
            for (value, sample) in values.iter_mut().zip(&self.samples) {
                *value = select(sample);
            }
            let values = &mut values[..self.samples.len()];
            values.sort_unstable();
            let millis = |d: Duration| d.as_secs_f64() * 1000.0;
            // Nearest-rank percentiles, with a bounded rolling sample window.
            let p95 = (values.len() * 95).div_ceil(100) - 1;
            let _ = write!(
                result,
                "\nhost_{name}_avg_ms={:.3}\nhost_{name}_p95_ms={:.3}\nhost_{name}_max_ms={:.3}",
                values.iter().map(|d| millis(*d)).sum::<f64>() / values.len() as f64,
                millis(values[p95]),
                millis(*values.last().unwrap())
            );
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_bounded_and_timings_have_explicit_units() {
        let mut profile = CpuProfile::default();
        assert_eq!(profile.diagnostics(), "");
        for i in 0..150 {
            profile.record(
                FrameSample {
                    interval: Duration::from_millis(i),
                    step: Duration::from_millis(1),
                    total: Duration::from_millis(5),
                    updates: 2,
                    ..Default::default()
                },
                [1024, 768, 2048, 1536],
                125,
            );
        }
        assert_eq!(profile.samples.len(), SAMPLE_LIMIT);
        let text = profile.diagnostics();
        assert!(text.contains("host_interval_avg_ms=89.500"));
        assert!(text.contains("host_interval_p95_ms=143.000"));
        assert!(text.contains("host_step_avg_ms=1.000"));
        assert!(text.contains("host_callback_max_ms=5.000"));
        assert!(text.contains("host_internal_width=2048"));
        assert!(text.contains("host_updates_per_callback=2.000"));
        profile.clear();
        assert_eq!(profile.diagnostics(), "");
    }

    #[test]
    fn resize_does_not_mix_incomparable_samples() {
        let mut profile = CpuProfile::default();
        profile.record(FrameSample::default(), [800, 480, 1600, 960], 100);
        profile.record(FrameSample::default(), [1024, 768, 2048, 1536], 120);
        assert_eq!(profile.samples.len(), 1);
        assert!(profile.diagnostics().contains("host_scene_primitives=120"));
    }
}
