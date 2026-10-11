//! Compact, once-per-second diagnostics. Never imply that CPU-side OpenGL
//! submission time is GPU execution time or that selecting GL proves GPU use.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub(super) struct ProcessCpu {
    previous: Option<(Instant, Duration)>,
    pub percent: Option<f64>,
}

impl ProcessCpu {
    pub fn sample(&mut self) {
        let cpu = process_cpu_time();
        self.record(Instant::now(), cpu);
    }

    fn record(&mut self, now: Instant, cpu: Option<Duration>) {
        self.percent = self.previous.zip(cpu).and_then(|((start, previous), cpu)| {
            let wall = now.checked_duration_since(start)?.as_secs_f64();
            let elapsed = cpu.checked_sub(previous)?.as_secs_f64();
            (wall > 0.0).then(|| 100.0 * elapsed / wall)
        });
        self.previous = cpu.map(|cpu| (now, cpu));
    }

    pub fn reset(&mut self) {
        *self = Self::default();
        self.sample();
    }
}

#[cfg(target_os = "linux")]
fn process_cpu_time() -> Option<Duration> {
    use nix::time::{ClockId, clock_gettime};
    let time = clock_gettime(ClockId::CLOCK_PROCESS_CPUTIME_ID).ok()?;
    Some(Duration::new(
        time.tv_sec().try_into().ok()?,
        time.tv_nsec().try_into().ok()?,
    ))
}

#[cfg(not(target_os = "linux"))]
fn process_cpu_time() -> Option<Duration> {
    None
}

#[derive(Default)]
pub(super) struct Presentation {
    pub renderer: Option<&'static str>,
    pub driver: Option<String>,
    pub draw_cpu_ms: Option<f64>,
}

impl Presentation {
    pub fn current() -> Self {
        #[cfg(all(target_os = "linux", feature = "pi-kiosk"))]
        if let Some(summary) = i_slint_backend_linuxkms::profiling::overlay_summary() {
            return Self {
                renderer: Some(summary.renderer),
                driver: summary.gl_renderer,
                draw_cpu_ms: summary.draw_cpu_ms,
            };
        }
        Self::default()
    }

    fn label(&self) -> &'static str {
        match self.renderer {
            Some("software") => "CPU software",
            Some("femtovg") => {
                let driver = self
                    .driver
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if driver.contains("llvmpipe") || driver.contains("softpipe") {
                    "GL software"
                } else if driver.starts_with("v3d ") || driver == "v3d" {
                    "GPU (V3D)"
                } else {
                    "GL (unverified)"
                }
            }
            _ => "desktop",
        }
    }
}

pub(super) fn details(
    process_cpu: Option<f64>,
    prepare_ms: Option<f64>,
    scene: &str,
    presentation: &Presentation,
) -> String {
    let cpu = process_cpu.map_or_else(|| "--".into(), |value| format!("{value:.0}%"));
    let ms =
        |value: Option<f64>| value.map_or_else(|| "--".into(), |value| format!("{value:.1}ms"));
    let gpu = if presentation.renderer == Some("software") {
        ""
    } else {
        " | GPU n/a"
    };
    format!(
        "CPU {cpu} | Prep {}\nDraw CPU {}{gpu}\n{scene} -> {}",
        ms(prepare_ms),
        ms(presentation.draw_cpu_ms),
        presentation.label()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_is_process_wide_and_100_percent_means_one_core() {
        let start = Instant::now();
        let mut cpu = ProcessCpu::default();
        cpu.record(start, Some(Duration::from_secs(5)));
        assert_eq!(cpu.percent, None);
        cpu.record(start + Duration::from_secs(2), Some(Duration::from_secs(8)));
        assert_eq!(cpu.percent, Some(150.0));
        cpu.record(start + Duration::from_secs(3), None);
        assert_eq!(cpu.percent, None);
        cpu.record(start + Duration::from_secs(4), Some(Duration::from_secs(9)));
        assert_eq!(cpu.percent, None, "missing samples must not be bridged");
        cpu.record(start + Duration::from_secs(4), Some(Duration::from_secs(9)));
        assert_eq!(cpu.percent, None, "zero wall interval is not a CPU rate");
        cpu.record(start + Duration::from_secs(5), Some(Duration::from_secs(1)));
        assert_eq!(cpu.percent, None, "counter reset is not zero CPU use");
    }

    #[test]
    fn labels_separate_scene_work_cpu_draw_and_verified_gpu_path() {
        let mut p = Presentation {
            renderer: Some("software"),
            draw_cpu_ms: Some(5.12),
            ..Default::default()
        };
        assert_eq!(
            details(Some(74.0), Some(6.24), "raster", &p),
            "CPU 74% | Prep 6.2ms\nDraw CPU 5.1ms\nraster -> CPU software"
        );
        p.renderer = Some("femtovg");
        for (driver, label) in [
            (None, "GL (unverified)"),
            (Some("llvmpipe (LLVM 19)"), "GL software"),
            (Some("softpipe"), "GL software"),
            (Some("V3D 4.2"), "GPU (V3D)"),
        ] {
            p.driver = driver.map(str::to_owned);
            let text = details(None, None, "video", &p);
            assert!(text.starts_with("CPU -- | Prep --\n"));
            assert!(text.contains("GPU n/a"));
            assert!(text.ends_with(label));
        }
        assert_eq!(Presentation::default().label(), "desktop");
        assert!(details(None, None, "vector", &Presentation::default()).contains("Draw CPU --"));
    }
}
