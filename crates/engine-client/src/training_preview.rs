//! A bounded replay worker observes immutable candidates, never the live trainer.
use crate::MainWindow;
use engine_agent::{
    ship_arrival::{ArrivalEpisode, DT, DWELL_TICKS, POSITION_TOLERANCE},
    training_job::{JobControl, PreviewCandidate},
};
use engine_core::Vec2;
use slint::{ComponentHandle, Image, Rgb8Pixel, SharedPixelBuffer, Timer, TimerMode};
use std::{
    sync::{
        Arc,
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct Request {
    slot: usize,
    candidate: Arc<PreviewCandidate>,
    case_index: usize,
}
struct Reply {
    request: Request,
    result: Result<ArrivalEpisode, String>,
}
struct Replay {
    candidate: Arc<PreviewCandidate>,
    episode: ArrivalEpisode,
    frame: f64,
}
#[derive(Default)]
struct Slot {
    replay: Option<Replay>,
    next_case: usize,
    error: Option<String>,
}

pub(crate) struct Preview {
    timer: Timer,
    requests: SyncSender<Option<Request>>,
    worker: Option<JoinHandle<()>>,
}
impl Drop for Preview {
    fn drop(&mut self) {
        self.timer.stop();
        let _ = self.requests.send(None);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(crate) fn install(window: &MainWindow, control: JobControl) -> std::io::Result<Preview> {
    let (requests, receiver) = mpsc::sync_channel::<Option<Request>>(1);
    let (replies, results) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("training-preview".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            while let Ok(Some(request)) = receiver.recv() {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    request.candidate.replay(request.case_index)
                }))
                .map_err(|_| "Preview failed; the training job is unaffected.".to_owned())
                .and_then(|r| r.map_err(|e| e.to_string()));
                // The UI may disappear before draining a reply. Shutdown cannot
                // depend on an event loop consuming a result.
                if replies.try_send(Reply { request, result }).is_err() {
                    break;
                }
            }
        })?;
    let mut slots: [Slot; 2] = Default::default();
    let mut run_id = None;
    let mut busy = false;
    let mut previous = Instant::now();
    let mut last_request = None::<Instant>;
    let mut rendered = None;
    let sender = requests.clone();
    let weak = window.as_weak();
    let timer = Timer::default();
    // At most 15 images/s, one in-flight evaluation, two retained 1,200-tick
    // traces, and at most one new replay per second. Nothing is written to disk.
    timer.start(TimerMode::Repeated, Duration::from_millis(67), move || {
        let Some(w) = weak.upgrade() else { return };
        let now = Instant::now();
        let elapsed = now.duration_since(previous).min(Duration::from_millis(250));
        previous = now;
        let candidates = control.preview_candidates();
        let next_run = candidates.current.as_ref().map(|c| c.run_id.clone());
        if run_id != next_run {
            run_id = next_run;
            slots = Default::default();
            rendered = None;
            clear(&w);
            w.set_training_preview_follow(true);
        }
        if let Ok(reply) = results.try_recv() {
            busy = false;
            if run_id.as_deref() == Some(&reply.request.candidate.run_id) {
                let slot = &mut slots[reply.request.slot];
                match reply.result {
                    _ if !w.get_training_preview_follow() && slot.replay.is_some() => {}
                    Ok(episode) => {
                        slot.replay = Some(Replay {
                            candidate: reply.request.candidate,
                            episode,
                            frame: 0.0,
                        });
                        slot.error = None;
                    }
                    Err(error) => slot.error = Some(error),
                }
            }
        }
        if !w.get_training_visible()
            || w.get_training_setup()
            || w.get_training_history()
            || !w.get_launcher_visible()
            || w.get_training_preview_mode() == 0
        {
            return;
        }
        let index = (w.get_training_preview_mode() - 1).clamp(0, 1) as usize;
        let slot = &mut slots[index];
        let follow = w.get_training_preview_follow();
        let mut next = slot.replay.is_none();
        if let Some(replay) = &mut slot.replay {
            replay.frame += elapsed.as_secs_f64() * 60.0 * speed(w.get_training_preview_speed());
            if replay.frame >= replay.episode.trace.len() as f64 + 60.0
                && speed(w.get_training_preview_speed()) > 0.0
            {
                if follow {
                    next = true;
                } else {
                    replay.frame = 0.0;
                }
            }
        }
        if next
            && !busy
            && slot.error.is_none()
            && last_request.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(1))
        {
            let source = if index == 0 {
                candidates.current
            } else {
                candidates.best
            };
            if let Some(candidate) = source.filter(|c| Some(&c.run_id) == run_id.as_ref()) {
                let request = Request {
                    slot: index,
                    candidate,
                    case_index: slot.next_case,
                };
                match sender.try_send(Some(request)) {
                    Ok(()) => {
                        slot.next_case += 1;
                        busy = true;
                        last_request = Some(now);
                    }
                    Err(e) => slot.error = Some(format!("Preview unavailable: {e}")),
                }
            }
        }
        if let Some(replay) = &slot.replay {
            let key = (
                index,
                replay.episode.brain.controller.clone(),
                replay.episode.case.name.clone(),
                (replay.frame as usize).min(replay.episode.trace.len()),
                w.get_training_preview_width(),
                w.get_training_preview_height(),
            );
            let redraw = rendered.as_ref() != Some(&key);
            publish(&w, replay, redraw);
            rendered = Some(key);
        } else {
            clear(&w);
            w.set_training_preview_label(
                if index == 1 {
                    "Best replay · waiting for a saved generation…"
                } else {
                    "Current replay · preparing a sampled candidate…"
                }
                .into(),
            );
        }
        if let Some(error) = &slot.error {
            w.set_training_preview_detail(error.clone().into());
        }
    });
    Ok(Preview {
        timer,
        requests,
        worker: Some(worker),
    })
}

fn speed(index: i32) -> f64 {
    [1.0, 2.0, 4.0, 0.0][index.rem_euclid(4) as usize]
}
pub(crate) fn command(w: &MainWindow, id: &str) {
    match id {
        "training.preview.progress" => w.set_training_preview_mode(0),
        "training.preview.current" => w.set_training_preview_mode(1),
        "training.preview.best" => w.set_training_preview_mode(2),
        "training.preview.speed" => {
            let index = (w.get_training_preview_speed() + 1).rem_euclid(4);
            w.set_training_preview_speed(index);
            w.set_training_preview_speed_label(["1×", "2×", "4×", "Still"][index as usize].into());
        }
        "training.preview.follow" => {
            w.set_training_preview_follow(!w.get_training_preview_follow())
        }
        _ => {}
    }
}
fn clear(w: &MainWindow) {
    w.set_training_preview_image(Image::default());
    w.set_training_preview_label("".into());
    w.set_training_preview_detail("".into());
    w.set_training_preview_id("".into());
    w.set_training_preview_tick(0);
}
fn publish(w: &MainWindow, replay: &Replay, redraw: bool) {
    let episode = &replay.episode;
    let frame = (replay.frame as usize).min(episode.trace.len());
    let tick = frame.checked_sub(1).and_then(|i| episode.trace.get(i));
    let observed = tick.map_or(&episode.initial, |t| t.after.as_ref().unwrap_or(&t.before));
    let elapsed = observed.tick.saturating_sub(episode.initial.tick);
    let target = episode.initial_target.position
        + episode.initial_target.velocity * (elapsed as f32 * DT.as_secs_f32());
    let distance = (target - observed.ship.position).length();
    let relative_speed = (episode.initial_target.velocity - observed.ship.velocity).length();
    let dwell = tick.map_or(0, |t| t.measure.dwell);
    let controls = observed.applied;
    let result = if frame == episode.trace.len() {
        format!("{:?}", episode.outcome)
    } else {
        format!("Dwell {dwell}/{DWELL_TICKS}")
    };
    let id = &episode.brain.controller;
    let label = replay.candidate.candidate.map_or_else(
        || format!("Best · Gen {}", replay.candidate.generation),
        |c| format!("Sample · Gen {} · #{c}", replay.candidate.generation),
    );
    w.set_training_preview_label(
        format!(
            "Replay · {label} · {}\n{} · World seed {} · Blue ship / gold target",
            &id[..12],
            episode.case.name,
            episode.case.seed
        )
        .into(),
    );
    w.set_training_preview_detail(format!(
        "Tick {elapsed} · Distance {distance:.1} · Rel speed {relative_speed:.1} · {result}\nTurn {:+.2} · Thrust {} · Brake {}",
        controls[0], if controls[1] > 0.0 {"on"} else {"off"}, if controls[2] > 0.0 {"on"} else {"off"}
    ).into());
    w.set_training_preview_id(id.clone().into());
    w.set_training_preview_tick(elapsed.min(i32::MAX as u64) as i32);
    if redraw {
        w.set_training_preview_image(draw(
            episode,
            frame,
            w.get_training_preview_width() as u32,
            w.get_training_preview_height() as u32,
        ));
    }
}

// A stable camera covers the retained trajectory. Rendering only reads trace
// records; playback does not step a simulation or execute a controller.
fn draw(episode: &ArrivalEpisode, frame: usize, width: u32, height: u32) -> Image {
    let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
    pixels.make_mut_slice().fill(Rgb8Pixel::new(8, 15, 28));
    let mut low = episode.initial.ship.position;
    let mut high = low;
    let mut include = |p: Vec2| {
        low.x = low.x.min(p.x);
        low.y = low.y.min(p.y);
        high.x = high.x.max(p.x);
        high.y = high.y.max(p.y);
    };
    include(episode.initial_target.position);
    for t in &episode.trace {
        include(t.after.as_ref().unwrap_or(&t.before).ship.position);
        include(t.target_before.position + t.target_before.velocity * DT.as_secs_f32());
    }
    let center = (high + low) * 0.5;
    let scale = ((width as f32 - 32.0) / (high.x - low.x).max(12.0))
        .min((height as f32 - 28.0) / (high.y - low.y).max(12.0));
    let project = |p: Vec2| {
        Vec2::new(
            width as f32 / 2.0 + (p.x - center.x) * scale,
            height as f32 / 2.0 - (p.y - center.y) * scale,
        )
    };
    let mut paint = |a: Vec2, b: Vec2, color: Rgb8Pixel, thick: i32| {
        let count = ((b - a).x.abs().max((b - a).y.abs()).ceil() as usize).max(1);
        for i in 0..=count {
            let p = a + (b - a) * (i as f32 / count as f32);
            for dy in -thick..=thick {
                for dx in -thick..=thick {
                    let x = p.x.round() as i32 + dx;
                    let y = p.y.round() as i32 + dy;
                    if x >= 0 && y >= 0 && x < width as i32 && y < height as i32 {
                        pixels.make_mut_slice()[y as usize * width as usize + x as usize] = color;
                    }
                }
            }
        }
    };
    let mut previous = project(episode.initial.ship.position);
    for t in &episode.trace {
        let p = project(t.after.as_ref().unwrap_or(&t.before).ship.position);
        paint(previous, p, Rgb8Pixel::new(34, 70, 95), 0);
        previous = p;
    }
    let target_end = episode.initial_target.position
        + episode.initial_target.velocity * (episode.elapsed_ticks as f32 * DT.as_secs_f32());
    paint(
        project(episode.initial_target.position),
        project(target_end),
        Rgb8Pixel::new(93, 73, 37),
        0,
    );
    let tick = frame.checked_sub(1).and_then(|i| episode.trace.get(i));
    let o = tick.map_or(&episode.initial, |t| t.after.as_ref().unwrap_or(&t.before));
    let elapsed = o.tick.saturating_sub(episode.initial.tick);
    let marker = project(
        episode.initial_target.position
            + episode.initial_target.velocity * (elapsed as f32 * DT.as_secs_f32()),
    );
    let radius = (POSITION_TOLERANCE * scale).clamp(4.0, 20.0);
    for i in 0..32 {
        let angle = i as f32 * std::f32::consts::TAU / 32.0;
        let next = (i + 1) as f32 * std::f32::consts::TAU / 32.0;
        paint(
            marker + Vec2::new(angle.cos(), angle.sin()) * radius,
            marker + Vec2::new(next.cos(), next.sin()) * radius,
            Rgb8Pixel::new(233, 182, 93),
            0,
        );
    }
    paint(
        marker - Vec2::new(4.0, 0.0),
        marker + Vec2::new(4.0, 0.0),
        Rgb8Pixel::new(233, 182, 93),
        0,
    );
    paint(
        marker - Vec2::new(0.0, 4.0),
        marker + Vec2::new(0.0, 4.0),
        Rgb8Pixel::new(233, 182, 93),
        0,
    );
    let ship = project(o.ship.position);
    let direction = Vec2::new(-o.ship.angle.sin(), -o.ship.angle.cos());
    let side = Vec2::new(-direction.y, direction.x);
    let nose = ship + direction * 9.0;
    let left = ship - direction * 6.0 + side * 7.0;
    let right = ship - direction * 6.0 - side * 7.0;
    let blue = Rgb8Pixel::new(128, 216, 255);
    paint(nose, left, blue, 1);
    paint(left, right, blue, 1);
    paint(right, nose, blue, 1);
    if o.applied[1] > 0.0 {
        paint(
            ship - direction * 7.0,
            ship - direction * 16.0,
            Rgb8Pixel::new(255, 148, 80),
            1,
        );
    }
    if o.applied[2] > 0.0 {
        paint(
            ship - side * 11.0,
            ship + side * 11.0,
            Rgb8Pixel::new(189, 155, 255),
            0,
        );
    }
    Image::from_rgb8(pixels)
}
