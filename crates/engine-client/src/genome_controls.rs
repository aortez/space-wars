//! Bounded background library reads and isolated practice trials.
use crate::MainWindow;
use engine_agent::genome_library::{self as library, Catalog, Entry, Practice, Trial};
use slint::{ComponentHandle, Image, Rgb8Pixel, SharedPixelBuffer, Timer, TimerMode};
use spacewars_control::{UiAction, UiControl};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::mpsc::{self, SyncSender},
    thread::{self, JoinHandle},
    time::Duration,
};

enum Request {
    Refresh,
    Trial(Box<Entry>, Practice),
    Shutdown,
}
enum Reply {
    Catalog(Catalog),
    Trial(Box<Trial>),
    Error(String),
}
#[derive(Default)]
struct State {
    entries: Vec<Entry>,
    index: usize,
    busy: bool,
    warning: String,
}
pub(crate) struct Browser {
    timer: Timer,
    sender: SyncSender<Request>,
    worker: Option<JoinHandle<()>>,
}
impl Drop for Browser {
    fn drop(&mut self) {
        self.timer.stop();
        let _ = self.sender.send(Request::Shutdown);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
pub(crate) fn install(window: &MainWindow, root: PathBuf) -> Result<Browser, std::io::Error> {
    let (sender, requests) = mpsc::sync_channel(1);
    let (replies, receiver) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("genome-browser".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            while let Ok(request) = requests.recv() {
                if matches!(request, Request::Shutdown) {
                    break;
                }
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match request {
                        Request::Refresh => library::refresh(&root).map(Reply::Catalog),
                        Request::Trial(entry, preset) => {
                            library::practice(&root.join("trials"), &entry, preset)
                                .map(|r| Reply::Trial(Box::new(r)))
                        }
                        Request::Shutdown => unreachable!(),
                    }));
                let reply = match result {
                    Ok(Ok(reply)) => reply,
                    Ok(Err(e)) => Reply::Error(e.to_string()),
                    Err(_) => Reply::Error(
                        "Practice worker failed; the saved genome is preserved.".into(),
                    ),
                };
                // Never block shutdown because the view stopped observing.
                if replies.try_send(reply).is_err() {
                    break;
                }
            }
        })?;
    let state = Rc::new(RefCell::new(State::default()));
    let weak = window.as_weak();
    window.on_genomes_open(move || {
        let Some(w) = weak.upgrade() else { return };
        if !w.get_launcher_visible() || w.get_launcher_busy() {
            return;
        }
        w.set_training_visible(false);
        w.set_launcher_focus_index(8);
        w.set_genomes_focus(0);
        w.set_genomes_visible(true);
        w.invoke_genomes_command("genomes.refresh".into());
    });
    let weak = window.as_weak();
    let commands = sender.clone();
    let actions = Rc::clone(&state);
    window.on_genomes_command(move |id| {
        let Some(w) = weak.upgrade() else { return };
        if !w.get_genomes_visible()
            || !w.get_launcher_visible()
            || !inventory(&w)
                .iter()
                .any(|c| c.id == id.as_str() && c.enabled)
        {
            return;
        }
        let mut s = actions.borrow_mut();
        let request = match id.as_str() {
            "genomes.back" => {
                w.set_genomes_visible(false);
                return;
            }
            "genomes.previous" | "genomes.next" => {
                let delta = if id == "genomes.previous" { -1 } else { 1 };
                s.index = (s.index as i32 + delta).rem_euclid(s.entries.len() as i32) as usize;
                clear_trial(&w);
                publish(&w, &s);
                return;
            }
            "genomes.practice.previous" | "genomes.practice.next" => {
                w.set_genomes_moving(!w.get_genomes_moving());
                clear_trial(&w);
                return;
            }
            "genomes.seed" => {
                let Some(entry) = s.entries.get(s.index) else {
                    return;
                };
                w.set_training_seed_genome(serde_json::to_string(&entry.genome).unwrap().into());
                w.set_training_seed_label(format!("Selected {}", &entry.id[..12]).into());
                w.set_genomes_visible(false);
                w.invoke_training_open();
                w.set_training_setup(true);
                w.set_training_focus(4);
                crate::training_controls::publish_setup(&w);
                return;
            }
            "genomes.try" => {
                let Some(entry) = s.entries.get(s.index) else {
                    return;
                };
                clear_trial(&w);
                w.set_genomes_result("Evaluating in a separate practice world…".into());
                Request::Trial(
                    Box::new(entry.clone()),
                    if w.get_genomes_moving() {
                        Practice::Moving
                    } else {
                        Practice::Stationary
                    },
                )
            }
            "genomes.refresh" => Request::Refresh,
            _ => return,
        };
        match commands.try_send(request) {
            Ok(()) => {
                s.busy = true;
                w.set_genomes_error("".into());
            }
            Err(e) => w.set_genomes_error(e.to_string().into()),
        }
        publish(&w, &s);
    });
    let timer = Timer::default();
    let weak = window.as_weak();
    let updates = state;
    timer.start(TimerMode::Repeated,Duration::from_millis(50),move || {
        let Some(w)=weak.upgrade() else{return};
        if let Ok(reply)=receiver.try_recv(){
            let mut s=updates.borrow_mut();s.busy=false;
            match reply {
                Reply::Catalog(catalog)=>{
                    let selected=s.entries.get(s.index).map(|e|e.id.clone());
                    s.entries=catalog.entries;s.index=selected.and_then(|id|s.entries.iter().position(|e|e.id==id)).unwrap_or(0);
                    s.warning=catalog.warnings.first().cloned().unwrap_or_default();
                },
                Reply::Trial(trial)=>{
                    let e=&trial.episode;
                    w.set_genomes_result(format!("{:?} · {} ticks · distance {:.2} · relative speed {:.2}\nPractice result · current build · {}",e.outcome,e.elapsed_ticks,e.measure.distance,e.measure.relative_speed,e.case.name).into());
                    w.set_genomes_receipt(trial.receipt.to_string_lossy().into_owned().into());
                    w.set_genomes_plot(plot(e));
                },
                Reply::Error(error)=>w.set_genomes_error(error.into()),
            }
            publish(&w,&s);
        }
    });
    Ok(Browser {
        timer,
        sender,
        worker: Some(worker),
    })
}
fn clear_trial(w: &MainWindow) {
    w.set_genomes_result(
        "Try this genome in a fixed arrival case. Each trial uses the named practice brain.".into(),
    );
    w.set_genomes_receipt("".into());
    w.set_genomes_plot(Image::default());
}
fn publish(w: &MainWindow, s: &State) {
    w.set_genomes_busy(s.busy);
    w.set_genomes_count(s.entries.len() as i32);
    w.set_genomes_warning(s.warning.clone().into());
    let Some(entry) = s.entries.get(s.index) else {
        w.set_genomes_label("No saved genomes".into());
        w.set_genomes_detail("Train a generation to save its selected genome.".into());
        w.set_genomes_compatible(false);
        w.set_genomes_seedable(false);
        return;
    };
    w.set_genomes_label(
        format!(
            "{}/{} · {}",
            s.index + 1,
            s.entries.len(),
            entry.origin.label.replace(" experiment", "")
        )
        .into(),
    );
    w.set_genomes_id(entry.id.clone().into());
    w.set_genomes_compatible(entry.supports_practice());
    w.set_genomes_seedable(
        entry
            .genome
            .same_architecture(&engine_agent::ship_training::template()),
    );
    let score = entry
        .origin
        .training_arrivals
        .zip(entry.origin.training_cases)
        .map_or_else(
            || "No training score in this entry".into(),
            |(a, c)| format!("Recorded training: {a}/{c} arrivals"),
        );
    let scope = if entry.supports_practice() {
        "Ship pilot · Guidance v1"
    } else {
        "Unsupported component"
    };
    let generation = entry
        .origin
        .generation
        .map_or_else(|| "—".into(), |g| g.to_string());
    w.set_genomes_detail(
        format!(
            "{scope} · {} parameters · Gen {generation}\n{score} · ID {}",
            entry.genome.parameter_count(),
            &entry.id[..12]
        )
        .into(),
    );
    if !entry.supports_practice() {
        w.set_genomes_result(
            "This component does not support the available ship-guidance practices.".into(),
        );
    }
}
pub(crate) fn inventory(w: &MainWindow) -> Vec<UiControl> {
    let ready = !w.get_genomes_busy();
    let selected = w.get_genomes_count() > 0;
    vec![
        UiControl::new("genomes.id", "Genome identity", false)
            .with_value(w.get_genomes_id().to_string()),
        UiControl::new("genomes.status", "Library state", false).with_value(if ready {
            "ready"
        } else {
            "busy"
        }),
        UiControl::new("genomes.receipt", "Practice receipt", false)
            .with_value(w.get_genomes_receipt().to_string()),
        UiControl::new("genomes.warning", "Library warning", false)
            .with_value(w.get_genomes_warning().to_string()),
        UiControl::new("genomes.previous", "Previous genome", ready && selected),
        UiControl::new("genomes.next", "Next genome", ready && selected),
        UiControl::new(
            "genomes.practice.previous",
            "Previous practice",
            ready && w.get_genomes_compatible(),
        ),
        UiControl::new(
            "genomes.practice.next",
            "Next practice",
            ready && w.get_genomes_compatible(),
        ),
        UiControl::new(
            "genomes.try",
            "Try in practice",
            ready && w.get_genomes_compatible(),
        ),
        UiControl::new(
            "genomes.seed",
            "Use as training seed",
            ready && w.get_genomes_seedable() && w.get_training_can_start(),
        ),
        UiControl::new("genomes.refresh", "Refresh library", ready),
        UiControl::new("genomes.back", "Back", true),
    ]
}
pub(crate) fn selected_id(w: &MainWindow) -> &'static str {
    match w.get_genomes_focus() {
        0 => "genomes.next",
        1 => "genomes.practice.next",
        2 => "genomes.try",
        3 => "genomes.seed",
        4 => "genomes.refresh",
        _ => "genomes.back",
    }
}
pub(crate) fn handle_action(w: &MainWindow, action: UiAction) {
    let focus = w.get_genomes_focus();
    match action {
        UiAction::Up | UiAction::Down => {
            w.set_genomes_focus(crate::ui_navigation::moved_selection(
                focus,
                6,
                if action == UiAction::Up { -1 } else { 1 },
            ))
        }
        UiAction::Left | UiAction::Right if focus < 2 => {
            let id = match (focus, action == UiAction::Left) {
                (0, true) => "genomes.previous",
                (0, false) => "genomes.next",
                (_, true) => "genomes.practice.previous",
                _ => "genomes.practice.next",
            };
            w.invoke_genomes_command(id.into());
        }
        UiAction::Left | UiAction::Right => {
            w.set_genomes_focus(crate::ui_navigation::moved_selection(
                focus,
                6,
                if action == UiAction::Left { -1 } else { 1 },
            ))
        }
        UiAction::Confirm => w.invoke_genomes_command(selected_id(w).into()),
        UiAction::Back | UiAction::Controls => w.invoke_genomes_command("genomes.back".into()),
        UiAction::Start => {}
    }
}
fn plot(episode: &engine_agent::ship_arrival::ArrivalEpisode) -> Image {
    const WIDTH: usize = 640;
    const HEIGHT: usize = 100;
    let mut points = vec![episode.initial.ship.position];
    points.extend(
        episode
            .trace
            .iter()
            .filter_map(|t| t.after.as_ref().map(|o| o.ship.position)),
    );
    let targets = [
        episode.initial_target.position,
        episode.initial_target.position
            + episode.initial_target.velocity
                * (episode.elapsed_ticks as f32 * engine_agent::ship_arrival::DT.as_secs_f32()),
    ];
    let mut lo = points[0];
    let mut hi = lo;
    for p in points.iter().chain(targets.iter()) {
        lo.x = lo.x.min(p.x);
        lo.y = lo.y.min(p.y);
        hi.x = hi.x.max(p.x);
        hi.y = hi.y.max(p.y);
    }
    let scale = ((WIDTH - 16) as f32 / (hi.x - lo.x).max(1.0))
        .min((HEIGHT - 16) as f32 / (hi.y - lo.y).max(1.0));
    let center = (lo + hi) * 0.5;
    let pixel = |p: engine_core::Vec2| {
        (
            (WIDTH as f32 / 2.0 + (p.x - center.x) * scale) as i32,
            (HEIGHT as f32 / 2.0 - (p.y - center.y) * scale) as i32,
        )
    };
    let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(WIDTH as u32, HEIGHT as u32);
    buffer.make_mut_slice().fill(Rgb8Pixel::new(8, 15, 28));
    let mut line = |a: (i32, i32), b: (i32, i32), color: Rgb8Pixel| {
        let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs()).max(1);
        for n in 0..=steps {
            let x = a.0 + (b.0 - a.0) * n / steps;
            let y = a.1 + (b.1 - a.1) * n / steps;
            // Remain visible after fitting the plot on a 1x portrait display.
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let x = x + dx;
                    let y = y + dy;
                    if (0..WIDTH as i32).contains(&x) && (0..HEIGHT as i32).contains(&y) {
                        buffer.make_mut_slice()[y as usize * WIDTH + x as usize] = color;
                    }
                }
            }
        }
    };
    for pair in points.windows(2) {
        line(
            pixel(pair[0]),
            pixel(pair[1]),
            Rgb8Pixel::new(128, 216, 255),
        );
    }
    line(
        pixel(targets[0]),
        pixel(targets[1]),
        Rgb8Pixel::new(233, 182, 93),
    );
    for (p, color) in [
        (targets[1], Rgb8Pixel::new(233, 182, 93)),
        (*points.last().unwrap(), Rgb8Pixel::new(255, 255, 255)),
    ] {
        let (x, y) = pixel(p);
        line((x - 3, y), (x + 3, y), color);
        line((x, y - 3), (x, y + 3), color);
    }
    Image::from_rgb8(buffer)
}
