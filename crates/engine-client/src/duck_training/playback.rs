use super::decision::Decision;
use crate::{
    raster::{RasterOptions, RasterRenderer},
    render::{FrameLayout, Viewport},
};
use engine_agent::{
    duck_recipe::{DuckCase, DuckControls},
    trainer::{Replay, Trainer},
};
use scenario_clock::duck_training::{DuckDriver, DuckEpisode};
use spacewars_ai::c3::DECISION_TICKS;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread,
    time::Duration,
};

/// Each displayed visit reads its own genome and works a few decisions ahead.
/// Dropping the receiver stops the worker when the operator switches replays.
fn trace(
    source: &Replay,
    case: DuckCase,
    dir: &Path,
    tx: &SyncSender<Result<Option<Decision>, String>>,
) -> Result<(), String> {
    let model = Trainer::read_genome(dir, &source.genome_id)?;
    let mut episode = DuckEpisode::new(case, DuckDriver::External)?;
    let sensor = episode.sensor()?;
    let mut held = DuckControls::default();
    loop {
        let tick = episode.progress().tick;
        if tick.is_multiple_of(u64::from(DECISION_TICKS)) {
            let decision = episode
                .controls_active()
                .then(|| Decision::read(&episode, &sensor, &model, tick))
                .transpose()?;
            if let Some(d) = &decision {
                held = d.applied;
            }
            if case == source.case && source.controls_at(tick) != held {
                return Err(format!(
                    "Recomputed controls differ from the recorded decision at tick {tick}"
                ));
            }
            if tx.send(Ok(decision)).is_err() {
                return Ok(());
            }
        }
        if episode.step(held).outcome.is_some() {
            return Ok(());
        }
    }
}

pub(super) struct Playback {
    pub source: Arc<Replay>,
    case: DuckCase,
    episode: DuckEpisode,
    renderer: RasterRenderer,
    last_frame: Option<(u64, u32, u32)>,
    receiver: Receiver<Result<Option<Decision>, String>>,
    decisions: Vec<Option<Decision>>,
    pub error: Option<String>,
    pub terminal_time: Duration,
    pub shown_inspection: Option<(Option<u64>, usize, bool)>,
}
impl Playback {
    pub fn new(source: Arc<Replay>, entrance_right: bool, dir: PathBuf) -> Result<Self, String> {
        let case = DuckCase {
            entrance_right,
            ..source.case
        };
        let episode = DuckEpisode::new(case, DuckDriver::External)?;
        let (tx, receiver) = mpsc::sync_channel(8);
        let replay = source.clone();
        thread::Builder::new()
            .name("duck-replay-brain".into())
            .spawn(move || {
                if let Err(e) = trace(&replay, case, &dir, &tx) {
                    let _ = tx.send(Err(e));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            source,
            case,
            episode,
            renderer: RasterRenderer::new(),
            last_frame: None,
            receiver,
            decisions: vec![],
            error: None,
            terminal_time: Duration::ZERO,
            shown_inspection: None,
        })
    }

    pub fn matches(&self, source: &Replay) -> bool {
        self.source.generation == source.generation
            && self.source.genome_id == source.genome_id
            && self.source.case == source.case
    }
    pub fn terminal(&self) -> bool {
        let progress = self.episode.progress();
        progress.outcome.is_some()
            || (self.case == self.source.case && progress.tick >= self.source.ticks())
            || (self.case != self.source.case && self.error.is_some())
    }
    pub fn reset(&mut self) {
        self.episode =
            DuckEpisode::new(self.case, DuckDriver::External).expect("validated replay case");
        self.last_frame = None;
        self.shown_inspection = None;
        self.terminal_time = Duration::ZERO;
    }

    pub fn ready(&mut self) -> bool {
        if self.terminal() {
            return true;
        }
        let index = (self.episode.progress().tick / u64::from(DECISION_TICKS)) as usize;
        while self.decisions.len() <= index && self.error.is_none() {
            match self.receiver.try_recv() {
                Ok(Ok(decision)) => self.decisions.push(decision),
                Ok(Err(e)) => self.error = Some(e),
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => {
                    self.error = Some("Decision trace ended before the replay".into())
                }
            }
        }
        true
    }
    pub fn step(&mut self) {
        if self.terminal() {
            return;
        }
        let tick = self.episode.progress().tick;
        let controls = if self.case == self.source.case {
            self.source.controls_at(tick)
        } else {
            self.decisions[(tick / u64::from(DECISION_TICKS)) as usize]
                .as_ref()
                .map_or_else(DuckControls::default, |d| d.applied)
        };
        let progress = self.episode.step(controls);
        if self.case == self.source.case && self.terminal() && progress != self.source.progress {
            self.error =
                Some("Terminal physics differs from the scored visit in this build".into());
        }
    }

    pub fn decision(&self) -> Option<&Decision> {
        self.episode
            .progress()
            .tick
            .checked_sub(1)
            .and_then(|tick| {
                self.decisions
                    .get((tick / u64::from(DECISION_TICKS)) as usize)?
                    .as_ref()
            })
    }

    pub fn signal(&self, action: usize) -> Vec<f32> {
        let Some(tick) = self.episode.progress().tick.checked_sub(1) else {
            return Vec::new();
        };
        let end = self
            .decisions
            .len()
            .min((tick / u64::from(DECISION_TICKS)) as usize + 1);
        self.decisions[end.saturating_sub(32)..end]
            .iter()
            .flatten()
            .map(|d| d.outputs[action])
            .collect()
    }

    pub fn frame(&mut self, width: u32) -> (String, String, Option<slint::Image>) {
        let case = self.case;
        let height = (width * u32::from(case.height) / u32::from(case.width)).max(1);
        let progress = self.episode.progress();
        let key = (progress.tick, width, height);
        let image = if self.last_frame == Some(key) {
            None
        } else {
            self.last_frame = Some(key);
            Some(self.renderer.image_from_frames_with_layout(
                &[self.episode.render()],
                Viewport::new(width as f32, height as f32),
                FrameLayout::EqualHorizontal,
                RasterOptions::default(),
            ))
        };
        let id = format!(
            "Gen {} · {}",
            self.source.generation,
            self.source.genome_id.chars().take(8).collect::<String>()
        );
        let side = if case.entrance_right { "Right" } else { "Left" };
        let clock = if case == self.source.case {
            format!("{} / {}", progress.tick, self.source.ticks())
        } else {
            progress.tick.to_string()
        };
        let terminal = progress
            .outcome
            .map_or(String::new(), |o| format!(" · {o:?}"));
        (
            id,
            format!(
                "{:?} · {side} door · Case {} · Tick {clock}{terminal}",
                case.pattern, case.seed
            ),
            image,
        )
    }
}
