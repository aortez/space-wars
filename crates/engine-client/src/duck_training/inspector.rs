use super::{Playback, View};
use crate::{DuckOutput, MainWindow};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn set_speed(w: &MainWindow, index: i32) {
    w.set_training_preview_speed(index);
    w.set_training_preview_speed_label(["1×", "2×", "4×", "Still"][index as usize].into());
}

impl View {
    pub(super) fn close_inspector(&mut self, w: &MainWindow) {
        if w.get_duck_inspect() >= 0 {
            w.set_training_preview_follow(self.follow_after_inspection);
        }
        if self.compare.take().is_some() {
            self.playing = Default::default();
        }
        self.pinned = Default::default();
        self.pending_step = false;
        let index = w.get_duck_inspect().max(0);
        w.set_duck_compare(false);
        w.set_duck_inspect(-1);
        w.set_training_focus(10 + index);
    }

    pub(super) fn brain_command(&mut self, w: &MainWindow, id: &str) -> Result<(), String> {
        for player in self.playing.iter_mut().flatten() {
            player.shown_inspection = None;
        }
        match id {
            "training.duck.brain.current"
            | "training.preview.current"
            | "training.duck.brain.best"
            | "training.preview.best" => {
                if w.get_duck_inspect() < 0 {
                    self.pinned = std::array::from_fn(|i| {
                        self.playing[i]
                            .as_ref()
                            .map(|p| p.source.clone())
                            .or_else(|| self.latest[i].clone())
                    });
                    self.follow_after_inspection = w.get_training_preview_follow();
                    w.set_training_preview_follow(false);
                }
                let index = i32::from(id.ends_with(".best"));
                w.set_duck_inspect(index);
                w.set_training_preview_mode(index + 1);
                w.set_training_focus(12 + w.get_duck_action());
            }
            "training.duck.brain.close" => self.close_inspector(w),
            "training.duck.brain.compare" => {
                if self.compare.take().is_none() {
                    let source = self.pinned[1]
                        .as_ref()
                        .or(self.latest[1].as_ref())
                        .cloned()
                        .ok_or("Waiting for a scored champion")?;
                    self.compare = Some(source);
                }
                self.playing = Default::default();
                self.fraction = 0.;
                self.previous = Instant::now();
                w.set_duck_compare(self.compare.is_some());
            }
            "training.duck.brain.pause" | "training.duck.replay.toggle" => {
                self.pending_step = false;
                let speed = w.get_training_preview_speed();
                if w.get_duck_result() {
                    let play = w.get_duck_replay_paused();
                    if play && self.visible_replays_finished(w) {
                        for p in self.playing.iter_mut().flatten() {
                            p.reset();
                        }
                    }
                    if play && speed == 3 {
                        set_speed(w, self.resume_speed);
                    }
                    w.set_duck_review_playing(play);
                    self.previous = Instant::now();
                } else if speed == 3 {
                    set_speed(w, self.resume_speed);
                } else {
                    self.resume_speed = speed;
                    set_speed(w, 3);
                }
            }
            "training.duck.brain.step" => {
                if w.get_duck_result() {
                    w.set_duck_review_playing(false);
                } else {
                    if w.get_training_preview_speed() != 3 {
                        self.resume_speed = w.get_training_preview_speed();
                    }
                    set_speed(w, 3);
                }
                self.pending_step = true;
            }
            name => {
                let index = ["move", "jump", "run", "dive"]
                    .iter()
                    .position(|&action| name.strip_prefix("training.duck.brain.") == Some(action))
                    .ok_or("Unknown brain control")?;
                w.set_duck_action(index as i32);
                w.set_training_focus(12 + index as i32);
            }
        }
        Ok(())
    }

    fn visible_replays_finished(&self, w: &MainWindow) -> bool {
        let selected = usize::from(w.get_training_preview_mode() == 2);
        self.playing
            .iter()
            .enumerate()
            .filter(|(i, _)| self.compare.is_some() || w.get_duck_wide() || *i == selected)
            .all(|(_, p)| p.as_ref().is_some_and(Playback::terminal))
    }

    pub(super) fn play(&mut self, w: &MainWindow) {
        let now = Instant::now();
        let elapsed = now
            .duration_since(self.previous)
            .min(Duration::from_millis(250));
        self.previous = now;
        if !w.get_training_visible()
            || w.get_training_setup()
            || w.get_duck_steering()
            || w.get_training_history()
        {
            return;
        }
        let Some(directory) = &self.directory else {
            return;
        };
        let wide = w.get_duck_wide();
        let selected = usize::from(w.get_training_preview_mode() == 2);
        let focused = usize::try_from(w.get_duck_inspect()).ok();
        let width = w.window().size().width;
        let target = if wide {
            width.saturating_sub(56) / 2
        } else {
            width * 56 / 100
        }
        .clamp(160, 800);
        let paired = self.compare.is_some();
        let visible = [wide || selected == 0, wide || selected == 1];
        for (i, slot) in self.playing.iter_mut().enumerate() {
            if !visible[i] && !paired {
                continue;
            }
            if focused.is_some() && self.pinned[i].is_none() {
                self.pinned[i] = self.latest[i].clone();
            }
            let Some(source) = self
                .compare
                .as_ref()
                .or(self.pinned[i].as_ref())
                .or(self.latest[i].as_ref())
            else {
                continue;
            };
            if slot.is_none()
                || (w.get_training_preview_follow()
                    && slot.as_ref().is_some_and(|p| !p.matches(source)))
            {
                let side = if paired {
                    i == 1
                } else {
                    source.case.entrance_right
                };
                match Playback::new(Arc::clone(source), side, directory.clone()) {
                    Ok(player) => *slot = Some(player),
                    Err(e) => {
                        w.set_training_error(e.into());
                        continue;
                    }
                }
            }
        }

        if focused.is_some_and(|i| self.playing[i].is_none()) {
            w.set_duck_inspect_status("Waiting for a scored visit".into());
            w.set_duck_units(Default::default());
            w.set_duck_signal(Default::default());
            w.set_duck_polar(Default::default());
        }
        let speed = if w.get_duck_replay_paused() {
            0.
        } else {
            crate::training_preview::speed(w.get_training_preview_speed())
        };
        let manual = self.pending_step;
        let pair_done = paired
            && self
                .playing
                .iter()
                .all(|p| p.as_ref().is_some_and(Playback::terminal));
        for (i, player) in self.playing.iter_mut().enumerate() {
            let Some(player) = player else {
                continue;
            };
            if (paired && pair_done || !paired && visible[i] && player.terminal())
                && (manual || (speed > 0. && !w.get_duck_result()))
            {
                player.terminal_time += elapsed;
                if manual || player.terminal_time >= Duration::from_secs(2) {
                    player.reset();
                }
            }
        }
        self.fraction += elapsed.as_secs_f64() * speed * 60.;
        let steps = self.fraction as usize;
        self.fraction -= steps as f64;
        // Manual stepping skips the door-opening animation and stops on the next
        // applied decision. An asynchronously loading trace resumes on the next poll.
        let watch = focused.unwrap_or(selected);
        let watch = if paired && self.playing[watch].as_ref().is_some_and(Playback::terminal) {
            1 - watch
        } else {
            watch
        };
        let before = self.playing[watch]
            .as_ref()
            .and_then(Playback::decision)
            .map(|d| d.tick);
        for _ in 0..if manual { 60 } else { steps } {
            if paired {
                if !self
                    .playing
                    .iter_mut()
                    .all(|p| p.as_mut().is_some_and(Playback::ready))
                {
                    break;
                }
                for p in self.playing.iter_mut().flatten() {
                    p.step();
                }
            } else {
                for (i, p) in self.playing.iter_mut().enumerate() {
                    if visible[i]
                        && let Some(p) = p
                        && p.ready()
                    {
                        p.step();
                    }
                }
            }
            if manual && let Some(p) = &self.playing[watch] {
                if p.terminal() || p.error.is_some() || p.decision().map(|d| d.tick) != before {
                    self.pending_step = false;
                    break;
                }
            }
        }

        if w.get_duck_result() && w.get_duck_review_playing() && self.visible_replays_finished(w) {
            w.set_duck_review_playing(false);
        }
        for (i, slot) in self.playing.iter_mut().enumerate() {
            if !visible[i] {
                continue;
            }
            let Some(player) = slot else {
                continue;
            };
            let (id, details, image) = player.frame(target);
            let decision = player.decision();
            let outputs = decision
                .map(|d| {
                    let c = d.applied;
                    let applied = [
                        f32::from(c.move_milli) / 1000.,
                        f32::from(u8::from(c.jump)),
                        f32::from(u8::from(c.run)),
                        f32::from(u8::from(c.dive)),
                    ];
                    ModelRc::new(VecModel::from(
                        (0..4)
                            .map(|n| DuckOutput {
                                value: d.outputs[n],
                                applied: applied[n],
                            })
                            .collect::<Vec<_>>(),
                    ))
                })
                .unwrap_or_default();
            if i == 0 {
                w.set_duck_current_outputs(outputs);
                w.set_duck_current_id(id.into());
                w.set_duck_current_details(details.into());
                if let Some(img) = image {
                    w.set_duck_current_frame(img);
                }
            } else {
                w.set_duck_best_outputs(outputs);
                w.set_duck_best_id(id.into());
                w.set_duck_best_details(details.into());
                if let Some(img) = image {
                    w.set_duck_best_frame(img);
                }
            }
            if focused == Some(i) {
                publish_decision(w, player);
            }
        }
    }
}

fn publish_decision(w: &MainWindow, player: &mut Playback) {
    let action = w.get_duck_action().clamp(0, 3) as usize;
    let key = (
        player.decision().map(|d| d.tick),
        action,
        player.error.is_some(),
    );
    if player.shown_inspection == Some(key) {
        return;
    }
    player.shown_inspection = Some(key);
    if let Some(d) = player.decision()
        && player.error.is_none()
    {
        let units = d.contributions(action);
        let bias = d.bias(action);
        let sum = units.iter().fold(bias, |v, x| v + x);
        w.set_duck_inspect_status(
            format!(
                "Decision {}  ·  Bias {bias:+.2}  ·  Before tanh {sum:+.2}  ·  Output {:+.2}",
                d.tick, d.outputs[action]
            )
            .into(),
        );
        w.set_duck_units(ModelRc::new(VecModel::from(units.to_vec())));
        w.set_duck_unit_scale(units.iter().map(|v| v.abs()).fold(0.0001, f32::max));
        w.set_duck_signal(ModelRc::new(VecModel::from(player.signal(action))));
        w.set_duck_signal_scale(1.);
        w.set_duck_polar(d.polar_image());
    } else {
        w.set_duck_inspect_status(
            player
                .error
                .as_deref()
                .unwrap_or("Waiting for the first decision")
                .into(),
        );
        w.set_duck_units(Default::default());
        w.set_duck_signal(Default::default());
        w.set_duck_polar(Default::default());
    }
}
