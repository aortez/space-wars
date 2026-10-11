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
    pub raster: crate::raster::RasterTimings,
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

    pub fn prepare_average_ms(&self) -> Option<f64> {
        (!self.samples.is_empty()).then(|| {
            self.samples
                .iter()
                .map(|s| s.prepare.as_secs_f64() * 1000.0)
                .sum::<f64>()
                / self.samples.len() as f64
        })
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
            ("raster_buffers", |s: &FrameSample| s.raster.buffers),
            ("raster_clear", |s: &FrameSample| s.raster.clear),
            ("raster_player", |s: &FrameSample| s.raster.player_views),
            ("raster_player_starfield", |s: &FrameSample| {
                s.raster.player_starfield
            }),
            ("raster_player_world", |s: &FrameSample| {
                s.raster.player_world
            }),
            ("raster_player_sun_planets", |s: &FrameSample| {
                s.raster.player_sun_planets
            }),
            ("raster_player_spaceports", |s: &FrameSample| {
                s.raster.player_spaceports
            }),
            ("raster_player_effects", |s: &FrameSample| {
                s.raster.player_effects
            }),
            ("raster_player_ships", |s: &FrameSample| {
                s.raster.player_ships
            }),
            ("raster_player_debris", |s: &FrameSample| {
                s.raster.player_debris
            }),
            ("raster_player_particles", |s: &FrameSample| {
                s.raster.player_particles
            }),
            ("raster_player_other", |s: &FrameSample| {
                s.raster.player_other
            }),
            ("raster_player_hud", |s: &FrameSample| s.raster.player_hud),
            ("raster_player_sun_corona", |s: &FrameSample| {
                s.raster.player_sun_corona
            }),
            ("raster_overview_refresh", |s: &FrameSample| {
                s.raster.overview_refresh
            }),
            ("raster_overview_live", |s: &FrameSample| {
                s.raster.overview_live
            }),
            ("raster_overview_blit", |s: &FrameSample| {
                s.raster.overview_blit
            }),
            ("raster_other_frames", |s: &FrameSample| {
                s.raster.other_frames
            }),
            ("raster_image", |s: &FrameSample| s.raster.image),
            ("raster_image_buffers", |s: &FrameSample| {
                s.raster.publication.buffers
            }),
            ("raster_image_make_mut", |s: &FrameSample| {
                s.raster.publication.make_mut
            }),
            ("raster_image_convert", |s: &FrameSample| {
                s.raster.publication.convert
            }),
            ("raster_image_wrap", |s: &FrameSample| {
                s.raster.publication.wrap
            }),
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
        let _ = write!(
            result,
            "\nhost_raster_image_new_buffers_window={}\nhost_raster_image_cow_copies_window={}\nhost_raster_image_alignment_prefix_pixels_window={}",
            self.samples
                .iter()
                .map(|s| s.raster.publication.new_buffers)
                .sum::<u64>(),
            self.samples
                .iter()
                .map(|s| s.raster.publication.cow_copies)
                .sum::<u64>(),
            self.samples
                .iter()
                .map(|s| s.raster.publication.alignment_prefix_pixels)
                .sum::<u64>(),
        );
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
        assert_eq!(profile.prepare_average_ms(), None);
        for i in 0..150 {
            profile.record(
                FrameSample {
                    interval: Duration::from_millis(i),
                    step: Duration::from_millis(1),
                    total: Duration::from_millis(5),
                    prepare: Duration::from_millis(3),
                    updates: 2,
                    ..Default::default()
                },
                [1024, 768, 2048, 1536],
                125,
            );
        }
        assert_eq!(profile.samples.len(), SAMPLE_LIMIT);
        assert_eq!(profile.prepare_average_ms(), Some(3.0));
        let text = profile.diagnostics();
        assert!(text.contains("host_interval_avg_ms=89.500"));
        assert!(text.contains("host_interval_p95_ms=143.000"));
        assert!(text.contains("host_step_avg_ms=1.000"));
        assert!(text.contains("host_callback_max_ms=5.000"));
        assert!(text.contains("host_internal_width=2048"));
        assert!(text.contains("host_updates_per_callback=2.000"));
        profile.clear();
        assert_eq!(profile.diagnostics(), "");
        assert_eq!(profile.prepare_average_ms(), None);
    }

    #[test]
    fn publication_counts_and_substages_use_the_same_bounded_window() {
        let mut profile = CpuProfile::default();
        for i in 0..150 {
            let mut raster = crate::raster::RasterTimings::default();
            raster.publication.buffers = Duration::from_millis(1);
            raster.publication.make_mut = Duration::from_millis(2);
            raster.publication.convert = Duration::from_millis(3);
            raster.publication.wrap = Duration::from_millis(4);
            raster.publication.new_buffers = u64::from(i < 3);
            raster.publication.cow_copies = u64::from(i >= 100);
            raster.publication.alignment_prefix_pixels = 2;
            profile.record(
                FrameSample {
                    raster,
                    ..Default::default()
                },
                [8, 8, 8, 8],
                1,
            );
        }
        let text = profile.diagnostics();
        assert!(text.contains("host_raster_image_buffers_avg_ms=1.000"));
        assert!(text.contains("host_raster_image_make_mut_avg_ms=2.000"));
        assert!(text.contains("host_raster_image_convert_avg_ms=3.000"));
        assert!(text.contains("host_raster_image_wrap_avg_ms=4.000"));
        assert!(text.contains("host_raster_image_new_buffers_window=0"));
        assert!(text.contains("host_raster_image_cow_copies_window=50"));
        assert!(text.contains("host_raster_image_alignment_prefix_pixels_window=240"));
        profile.clear();
        assert!(profile.diagnostics().is_empty());

        let mut total = crate::raster::RasterTimings::default();
        let mut next = total;
        next.publication.convert = Duration::from_millis(3);
        next.publication.new_buffers = 1;
        next.publication.cow_copies = 2;
        next.publication.alignment_prefix_pixels = 3;
        total += next;
        total += next;
        assert_eq!(total.publication.convert, Duration::from_millis(6));
        assert_eq!(
            (total.publication.new_buffers, total.publication.cow_copies),
            (2, 4)
        );
        assert_eq!(total.publication.alignment_prefix_pixels, 6);
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
