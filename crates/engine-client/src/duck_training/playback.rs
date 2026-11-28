use crate::{
    raster::{RasterOptions, RasterRenderer},
    render::{FrameLayout, Viewport},
};
use engine_agent::trainer::Replay;
use scenario_clock::duck_training::{DuckDriver, DuckEpisode};
use std::{rc::Rc, time::Duration};

pub(super) struct Playback {
    source: Rc<Replay>,
    episode: DuckEpisode,
    renderer: RasterRenderer,
    fraction: f64,
    last_frame: Option<(u64, u32, u32)>,
    terminal_time: Duration,
}

impl Playback {
    pub(super) fn new(source: Rc<Replay>) -> Result<Self, String> {
        Ok(Self {
            episode: DuckEpisode::new(source.case, DuckDriver::External)?,
            source,
            renderer: RasterRenderer::new(),
            fraction: 0.0,
            last_frame: None,
            terminal_time: Duration::ZERO,
        })
    }

    pub(super) fn matches(&self, source: &Replay) -> bool {
        self.source.generation == source.generation
            && self.source.genome_id == source.genome_id
            && self.source.case == source.case
    }

    pub(super) fn frame(
        &mut self,
        elapsed: Duration,
        speed: f64,
        target_width: u32,
    ) -> (String, String, Option<slint::Image>) {
        if self.episode.progress().outcome.is_some() && speed > 0.0 {
            self.terminal_time += elapsed;
            if self.terminal_time >= Duration::from_secs(2) {
                self.episode = DuckEpisode::new(self.source.case, DuckDriver::External)
                    .expect("previously loaded case");
                self.terminal_time = Duration::ZERO;
                self.fraction = 0.0;
            }
        }
        self.fraction += elapsed.as_secs_f64() * speed * 60.;
        let steps = self.fraction as usize;
        self.fraction -= steps as f64;
        for _ in 0..steps {
            let progress = self.episode.progress();
            if progress.outcome.is_some() || progress.tick >= self.source.ticks() {
                break;
            }
            self.episode.step(self.source.controls_at(progress.tick));
        }
        let case = self.source.case;
        let height = (target_width * u32::from(case.height) / u32::from(case.width)).max(1);
        let progress = self.episode.progress();
        let key = (progress.tick, target_width, height);
        let image = if self.last_frame == Some(key) {
            None
        } else {
            self.last_frame = Some(key);
            Some(self.renderer.image_from_frames_with_layout(
                &[self.episode.render()],
                Viewport::new(target_width as f32, height as f32),
                FrameLayout::EqualHorizontal,
                RasterOptions::default(),
            ))
        };
        let identity = format!(
            "Gen {} · {}",
            self.source.generation,
            self.source.genome_id.chars().take(8).collect::<String>()
        );
        let side = if case.entrance_right { "Right" } else { "Left" };
        let details = format!(
            "{:?} · {side} door · Case {} · Tick {} / {}",
            case.pattern,
            case.seed,
            progress.tick,
            self.source.ticks()
        );
        (identity, details, image)
    }
}
