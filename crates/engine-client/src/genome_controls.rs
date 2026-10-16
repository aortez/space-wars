//! Bounded background library reads, saved brains and isolated practice trials.
mod worker;

use crate::MainWindow;
use engine_agent::{
    brain_assembly::{Artifact, Record},
    genome_library::{Entry, Practice},
};
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
use worker::{Reply, Request, Selection};

#[derive(Default)]
struct State {
    components: Vec<Entry>,
    assemblies: Vec<Record>,
    component_index: usize,
    assembly_index: usize,
    brains: bool,
    copying: Option<Artifact>,
    busy: bool,
    warning: String,
}
impl State {
    fn component(&self) -> Option<&Entry> {
        self.components.get(self.component_index)
    }
    fn brain(&self) -> Option<&Record> {
        self.assemblies.get(self.assembly_index)
    }
    fn selection(&self) -> Option<Selection> {
        if self.brains {
            self.brain()
                .map(|b| Selection::Brain(b.artifact.id.clone()))
        } else {
            self.component().map(|c| Selection::Component(c.id.clone()))
        }
    }
    fn replace(&mut self, catalog: worker::Catalog, selected: Option<Selection>) {
        let component = self.component().map(|e| e.id.clone());
        let brain = self.brain().map(|e| e.artifact.id.clone());
        self.components = catalog.components;
        self.assemblies = catalog.brains;
        self.component_index = component
            .and_then(|id| self.components.iter().position(|e| e.id == id))
            .unwrap_or(0);
        self.assembly_index = brain
            .and_then(|id| self.assemblies.iter().position(|e| e.artifact.id == id))
            .unwrap_or(0);
        self.warning = catalog.warnings.first().cloned().unwrap_or_default();
        if let Some(selected) = selected {
            self.copying = None;
            match selected {
                Selection::Component(id) => {
                    self.brains = false;
                    self.component_index =
                        self.components.iter().position(|e| e.id == id).unwrap_or(0);
                }
                Selection::Brain(id) => {
                    self.brains = true;
                    self.assembly_index = self
                        .assemblies
                        .iter()
                        .position(|e| e.artifact.id == id)
                        .unwrap_or(0);
                }
            }
        }
    }
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
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    worker::execute(&root, request)
                }));
                let reply = match result {
                    Ok(Ok(reply)) => reply,
                    Ok(Err(e)) => Reply::Error(e.to_string()),
                    Err(_) => Reply::Error(
                        "Library worker failed; saved brains and genomes are preserved.".into(),
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
    let selection_state = state.clone();
    let selection_sender = sender.clone();
    window.on_genomes_select(move |brain, id| {
        let Some(w) = weak.upgrade() else {
            return false;
        };
        let mut s = selection_state.borrow_mut();
        if s.busy || !w.get_launcher_visible() || w.get_launcher_busy() {
            return false;
        }
        let selection = if brain {
            Selection::Brain(id.to_string())
        } else {
            Selection::Component(id.to_string())
        };
        if selection_sender
            .try_send(Request::Select(selection))
            .is_err()
        {
            return false;
        }
        // A failed exact link must never expose a previously selected genome
        // as though it were the requested artifact.
        s.components.clear();
        s.assemblies.clear();
        s.copying = None;
        s.brains = brain;
        s.busy = true;
        s.warning.clear();
        clear_trial(&w);
        publish(&w, &s);
        w.set_training_visible(false);
        w.set_genomes_visible(true);
        w.set_genomes_focus(1);
        true
    });
    let weak = window.as_weak();
    window.on_genomes_open(move || {
        let Some(w) = weak.upgrade() else { return };
        if !w.get_launcher_visible() || w.get_launcher_busy() {
            return;
        }
        w.set_training_visible(false);
        w.set_launcher_focus_index(8);
        w.set_genomes_focus(1);
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
                if s.busy {
                    w.set_genomes_visible(false);
                    return;
                }
                if s.copying.take().is_some() {
                    s.brains = true;
                    w.set_genomes_focus(4);
                    clear_trial(&w);
                    publish(&w, &s);
                } else {
                    w.set_genomes_visible(false);
                }
                return;
            }
            "genomes.components" | "genomes.brains" => {
                s.brains = id == "genomes.brains";
                clear_trial(&w);
                publish(&w, &s);
                return;
            }
            "genomes.previous" | "genomes.next" => {
                let delta = if id == "genomes.previous" { -1 } else { 1 };
                if s.brains {
                    s.assembly_index = (s.assembly_index as i32 + delta)
                        .rem_euclid(s.assemblies.len() as i32)
                        as usize;
                } else {
                    s.component_index = (s.component_index as i32 + delta)
                        .rem_euclid(s.components.len() as i32)
                        as usize;
                }
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
                let Some(entry) = s.component() else { return };
                w.set_training_seed_genome(serde_json::to_string(&entry.genome).unwrap().into());
                w.set_training_seed_label(format!("Selected {}", &entry.id[..12]).into());
                w.set_genomes_visible(false);
                w.invoke_training_open();
                w.set_training_setup(true);
                w.set_training_focus(4);
                crate::training_controls::publish_setup(&w);
                return;
            }
            "genomes.copy" => {
                let Some(brain) = s.brain() else { return };
                let artifact = brain.artifact.clone();
                s.component_index = s
                    .components
                    .iter()
                    .position(|e| e.id == artifact.definition.brain.controller)
                    .unwrap_or(0);
                s.copying = Some(artifact);
                s.brains = false;
                w.set_genomes_focus(1);
                clear_trial(&w);
                publish(&w, &s);
                return;
            }
            "genomes.inspect" => {
                let Some(brain) = s.brain() else { return };
                Request::Component(brain.artifact.definition.brain.controller.clone())
            }
            "genomes.save" => {
                let Some(entry) = s.component() else { return };
                Request::Save(entry.id.clone())
            }
            "genomes.copy.save" => {
                let (Some(source), Some(entry)) = (&s.copying, s.component()) else {
                    return;
                };
                Request::Copy {
                    source: source.id.clone(),
                    component: entry.id.clone(),
                }
            }
            "genomes.try" => {
                let Some(selected) = s.selection() else {
                    return;
                };
                clear_trial(&w);
                w.set_genomes_result("Evaluating in a separate practice world…".into());
                Request::Trial(
                    selected,
                    if w.get_genomes_moving() {
                        Practice::Moving
                    } else {
                        Practice::Stationary
                    },
                )
            }
            "genomes.refresh" => {
                if !w.get_genomes_error().is_empty() {
                    clear_trial(&w);
                }
                Request::Refresh
            }
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
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(w) = weak.upgrade() else { return };
        if let Ok(reply) = receiver.try_recv() {
            let mut s = state.borrow_mut();
            s.busy = false;
            match reply {
                Reply::Catalog(catalog, selected) => {
                    let previous = s.selection();
                    let unavailable = s.brain().and_then(|b| b.unavailable.clone());
                    let copying = s.copying.is_some();
                    let selected_explicitly = selected.is_some();
                    s.replace(catalog, selected);
                    if previous != s.selection() || copying != s.copying.is_some()
                        || unavailable != s.brain().and_then(|b| b.unavailable.clone()) {
                        clear_trial(&w);
                    }
                    if selected_explicitly { w.set_genomes_focus(1); }
                }
                Reply::Trial(trial) => {
                    let e = &trial.episode;
                    w.set_genomes_result(format!("{:?} · {} ticks · distance {:.2} · relative speed {:.2}\nPractice result · current build · {}", e.outcome, e.elapsed_ticks, e.measure.distance, e.measure.relative_speed, e.case.name).into());
                    w.set_genomes_receipt(trial.receipt.to_string_lossy().into_owned().into());
                    w.set_genomes_plot(plot(e));
                }
                Reply::Error(error) => w.set_genomes_error(error.into()),
            }
            publish(&w, &s);
        }
    });
    Ok(Browser {
        timer,
        sender,
        worker: Some(worker),
    })
}

fn clear_trial(w: &MainWindow) {
    w.set_genomes_error("".into());
    w.set_genomes_result(
        "Try a fixed arrival case. Each trial uses its own world and exact brain definition."
            .into(),
    );
    w.set_genomes_receipt("".into());
    w.set_genomes_plot(Image::default());
}
fn publish(w: &MainWindow, s: &State) {
    w.set_genomes_busy(s.busy);
    w.set_genomes_brains(s.brains);
    w.set_genomes_copying(s.copying.is_some());
    w.set_genomes_warning(s.warning.clone().into());
    w.set_genomes_count(if s.brains {
        s.assemblies.len()
    } else {
        s.components.len()
    } as i32);
    w.set_genomes_id("".into());
    w.set_genomes_dependency("".into());
    w.set_genomes_definition("".into());
    w.set_genomes_compatible(false);
    w.set_genomes_seedable(false);
    w.set_genomes_saveable(false);
    w.set_genomes_copyable(false);
    w.set_genomes_practice_choice(false);
    w.set_genomes_copy_source("".into());
    if s.brains {
        let Some(record) = s.brain() else {
            w.set_genomes_label("No saved brains".into());
            w.set_genomes_detail(
                "Choose a component, then Save to create its complete practice brain.".into(),
            );
            return;
        };
        let artifact = &record.artifact;
        let definition = &artifact.definition;
        w.set_genomes_label(
            format!(
                "{}/{} · {}",
                s.assembly_index + 1,
                s.assemblies.len(),
                record.name
            )
            .into(),
        );
        w.set_genomes_id(artifact.id.clone().into());
        w.set_genomes_dependency(definition.brain.controller.clone().into());
        w.set_genomes_definition(serde_json::to_string(artifact).unwrap().into());
        w.set_genomes_compatible(record.unavailable.is_none());
        w.set_genomes_copyable(definition.require_supported().is_ok());
        w.set_genomes_practice_choice(
            record.unavailable.is_none() && definition.practices.len() == 2,
        );
        if definition.practices.len() == 1 {
            w.set_genomes_moving(definition.practices[0] == Practice::Moving);
        }
        let detail = if definition.require_supported().is_ok() {
            format!(
                "Handwritten: arrival host, target source, coordinator v1\nLearned: Guidance v1 · {} · Brain {}",
                &definition.brain.controller[..12],
                &artifact.id[..12]
            )
        } else {
            format!(
                "Saved host: {}\nLearned component: {} · Brain {}",
                definition.host,
                &definition.brain.controller[..12],
                &artifact.id[..12]
            )
        };
        w.set_genomes_detail(detail.into());
        if let Some(reason) = &record.unavailable {
            w.set_genomes_result(reason.clone().into());
        }
        return;
    }
    let Some(entry) = s.component() else {
        w.set_genomes_label("No saved components".into());
        w.set_genomes_detail("Train a generation to save its selected genome.".into());
        return;
    };
    w.set_genomes_label(
        format!(
            "{}/{} · {}",
            s.component_index + 1,
            s.components.len(),
            entry.origin.label.replace(" experiment", "")
        )
        .into(),
    );
    w.set_genomes_id(entry.id.clone().into());
    w.set_genomes_dependency(entry.id.clone().into());
    w.set_genomes_compatible(entry.supports_practice());
    w.set_genomes_practice_choice(entry.supports_practice());
    w.set_genomes_seedable(
        s.copying.is_none()
            && entry
                .genome
                .same_architecture(&engine_agent::ship_training::template()),
    );
    w.set_genomes_saveable(
        entry.supports_practice()
            && s.copying
                .as_ref()
                .is_none_or(|source| source.definition.brain.controller != entry.id),
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
    if let Some(source) = &s.copying {
        let name = s
            .brain()
            .map(|b| b.name.as_str())
            .unwrap_or("Saved practice brain");
        w.set_genomes_copy_source(format!("Original: {name} · {}", &source.id[..12]).into());
        w.set_genomes_definition(serde_json::to_string(source).unwrap().into());
    }
    if !entry.supports_practice() {
        w.set_genomes_result(
            "This component does not support the available ship-guidance practices.".into(),
        );
    }
}

pub(crate) fn inventory(w: &MainWindow) -> Vec<UiControl> {
    let ready = !w.get_genomes_busy();
    let selected = w.get_genomes_count() > 0;
    let brains = w.get_genomes_brains();
    let mut controls = vec![
        UiControl::new("genomes.id", "Entry identity", false)
            .with_value(w.get_genomes_id().to_string()),
        UiControl::new("genomes.kind", "Entry kind", false).with_value(if brains {
            "practice_brain"
        } else {
            "component"
        }),
        UiControl::new("genomes.dependency", "Guidance genome identity", false)
            .with_value(w.get_genomes_dependency().to_string()),
        UiControl::new("genomes.definition", "Saved brain definition", false)
            .with_value(w.get_genomes_definition().to_string()),
        UiControl::new("genomes.status", "Library state", false).with_value(if ready {
            "ready"
        } else {
            "busy"
        }),
        UiControl::new("genomes.receipt", "Practice receipt", false)
            .with_value(w.get_genomes_receipt().to_string()),
        UiControl::new("genomes.warning", "Library warning", false)
            .with_value(w.get_genomes_warning().to_string()),
        UiControl::new("genomes.previous", "Previous entry", ready && selected),
        UiControl::new("genomes.next", "Next entry", ready && selected),
    ];
    if w.get_genomes_copying() {
        controls.push(UiControl::new(
            "genomes.copy.save",
            "Save copy with selected guidance",
            ready && w.get_genomes_saveable(),
        ));
        controls.push(UiControl::new(
            "genomes.back",
            if ready { "Cancel copy" } else { "Back" },
            true,
        ));
        return controls;
    }
    controls.extend([
        UiControl::new("genomes.components", "Components", ready),
        UiControl::new("genomes.brains", "Complete practice brains", ready),
        UiControl::new(
            "genomes.practice.previous",
            "Previous practice",
            ready && w.get_genomes_practice_choice(),
        ),
        UiControl::new(
            "genomes.practice.next",
            "Next practice",
            ready && w.get_genomes_practice_choice(),
        ),
        UiControl::new(
            "genomes.try",
            "Try in practice",
            ready && w.get_genomes_compatible(),
        ),
    ]);
    if brains {
        controls.push(UiControl::new(
            "genomes.copy",
            "Copy with another guidance component",
            ready && w.get_genomes_copyable(),
        ));
        controls.push(UiControl::new(
            "genomes.inspect",
            "Inspect exact guidance component",
            ready && selected,
        ));
    } else {
        controls.push(UiControl::new(
            "genomes.seed",
            "Use as training seed",
            ready && w.get_genomes_seedable() && w.get_training_can_start(),
        ));
        controls.push(UiControl::new(
            "genomes.save",
            "Save complete practice brain",
            ready && w.get_genomes_saveable(),
        ));
    }
    controls.push(UiControl::new("genomes.refresh", "Refresh library", ready));
    controls.push(UiControl::new("genomes.back", "Back", true));
    controls
}
pub(crate) fn selected_id(w: &MainWindow) -> &'static str {
    match w.get_genomes_focus() {
        0 => {
            if w.get_genomes_brains() {
                "genomes.components"
            } else {
                "genomes.brains"
            }
        }
        1 => "genomes.next",
        2 => "genomes.practice.next",
        3 => "genomes.try",
        4 => {
            if w.get_genomes_copying() {
                "genomes.copy.save"
            } else if w.get_genomes_brains() {
                "genomes.copy"
            } else {
                "genomes.seed"
            }
        }
        5 => {
            if w.get_genomes_brains() {
                "genomes.inspect"
            } else {
                "genomes.save"
            }
        }
        6 => "genomes.refresh",
        _ => "genomes.back",
    }
}
fn move_focus(w: &MainWindow, delta: i32) {
    let choices: &[i32] = if w.get_genomes_copying() {
        &[1, 4, 7]
    } else {
        &[0, 1, 2, 3, 4, 5, 6, 7]
    };
    let index = choices
        .iter()
        .position(|focus| *focus == w.get_genomes_focus())
        .unwrap_or(0) as i32;
    w.set_genomes_focus(choices[(index + delta).rem_euclid(choices.len() as i32) as usize]);
}
pub(crate) fn handle_action(w: &MainWindow, action: UiAction) {
    let focus = w.get_genomes_focus();
    match action {
        UiAction::Up | UiAction::Down => move_focus(w, if action == UiAction::Up { -1 } else { 1 }),
        UiAction::Left | UiAction::Right if focus < 3 => {
            let id = match (focus, action == UiAction::Left) {
                (0, _) => selected_id(w),
                (1, true) => "genomes.previous",
                (1, false) => "genomes.next",
                (_, true) => "genomes.practice.previous",
                _ => "genomes.practice.next",
            };
            w.invoke_genomes_command(id.into());
        }
        UiAction::Left | UiAction::Right => {
            move_focus(w, if action == UiAction::Left { -1 } else { 1 })
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
