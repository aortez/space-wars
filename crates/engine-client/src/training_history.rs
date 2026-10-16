//! History owns only read requests. The live job controller stays in Training.
mod details;
use crate::MainWindow;
use details::{Page, Sample};
use engine_agent::training_history::{self as history, Catalog, Key};
use slint::{ComponentHandle, Timer, TimerMode};
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
    Refresh(i32, Option<Key>),
    Inspect(Key, bool),
    Shutdown,
}
struct Reply {
    catalog: Option<Catalog>,
    key: Option<Key>,
    record: Result<(Vec<Page>, String), String>,
}
#[derive(Default)]
struct State {
    keys: Vec<Key>,
    key: Option<Key>,
    run: Option<Key>,
    trial: Option<Key>,
    snapshot: String,
    pages: Vec<Page>,
    page: usize,
    section: i32,
    busy: bool,
    error: String,
    warning: String,
    pinned: Option<Sample>,
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
pub(crate) fn install(w: &MainWindow, root: PathBuf) -> Result<Browser, std::io::Error> {
    let (sender, requests) = mpsc::sync_channel(1);
    let (replies, receiver) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("training-history".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            while let Ok(request) = requests.recv() {
                if matches!(request, Request::Shutdown) {
                    break;
                }
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    || -> Result<Reply, String> {
                        let (catalog, key, validation) = match request {
                            Request::Refresh(section, selection) => {
                                let mut catalog =
                                    history::catalog(&root, section == 2).map_err(|e| e.0)?;
                                // Retain a selected identity even if missing or outside the
                                // bounded scan; never silently select a different result.
                                if let Some(key) = &selection
                                    && !catalog.keys.contains(key) {
                                    catalog.keys.push(key.clone());
                                }
                                let key = selection.or_else(|| catalog.keys.first().cloned());
                                (Some(catalog), key, section == 1)
                            }
                            Request::Inspect(key, validation) => (None, Some(key), validation),
                            Request::Shutdown => unreachable!(),
                        };
                        let record = key
                            .as_ref()
                            .ok_or_else(|| "No saved records in this view.".to_string())
                            .and_then(|k| history::inspect(&root, k).map_err(|e| e.0))
                            .and_then(|record| {
                                let snapshot = serde_json::json!({"selection": key, "record": record}).to_string();
                                if snapshot.len() > 512 * 1024 {
                                    return Err("Record exceeds the 512 KiB history display limit; the saved file is preserved.".into());
                                }
                                Ok((details::pages(&record, validation), snapshot))
                            });
                        Ok(Reply {
                            catalog,
                            key,
                            record,
                        })
                    },
                ));
                let reply = match result {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => Reply {
                        catalog: None,
                        key: None,
                        record: Err(e),
                    },
                    Err(_) => Reply {
                        catalog: None,
                        key: None,
                        record: Err("History reader failed; saved files are preserved.".into()),
                    },
                };
                if replies.try_send(reply).is_err() {
                    break;
                }
            }
        })?;
    let state = Rc::new(RefCell::new(State::default()));
    let actions = state.clone();
    let commands = sender.clone();
    let weak = w.as_weak();
    w.on_history_command(move |id| {
        let Some(w) = weak.upgrade() else { return };
        if !w.get_training_visible() || !w.get_launcher_visible() {
            return;
        }
        let mut s = actions.borrow_mut();
        if id == "training.history.open" {
            w.set_training_history(true);
            w.set_history_focus(1);
        } else if !w.get_training_history()
            || !inventory(&w)
                .iter()
                .any(|c| c.id == id.as_str() && c.enabled)
        {
            return;
        }
        let request = match id.as_str() {
            "training.history.current" | "training.history.back" => {
                w.set_training_history(false);
                w.set_training_setup(!w.get_training_has_job());
                w.set_training_focus(0);
                return;
            }
            "training.history.open" | "training.history.refresh" => {
                if s.busy {
                    return;
                }
                Request::Refresh(s.section, s.key.clone())
            }
            "training.history.runs" | "training.history.validation" | "training.history.trials" => {
                let section = if id.ends_with("runs") {
                    0
                } else if id.ends_with("validation") {
                    1
                } else {
                    2
                };
                let changed = (section == 2) != (s.section == 2);
                s.section = section;
                s.page = 0;
                if !changed {
                    if let Some(key) = &s.key {
                        Request::Inspect(key.clone(), section == 1)
                    } else {
                        publish(&w, &s);
                        return;
                    }
                } else {
                    let key = if section == 2 {
                        s.trial.clone()
                    } else {
                        s.run.clone()
                    };
                    s.keys.clear();
                    s.key = key.clone();
                    Request::Refresh(section, key)
                }
            }
            "training.history.previous" | "training.history.next" => {
                let i = s
                    .keys
                    .iter()
                    .position(|k| Some(k) == s.key.as_ref())
                    .unwrap_or(0) as i32;
                let i = (i + if id.ends_with("previous") { -1 } else { 1 })
                    .rem_euclid(s.keys.len() as i32) as usize;
                s.page = 0;
                Request::Inspect(s.keys[i].clone(), s.section == 1)
            }
            "training.history.detail.previous" | "training.history.detail.next" => {
                let delta = if id.ends_with("previous") { -1 } else { 1 };
                s.page = (s.page as i32 + delta).rem_euclid(s.pages.len() as i32) as usize;
                publish(&w, &s);
                return;
            }
            "training.history.pin" => {
                s.pinned = s.pages.get(s.page).and_then(|p| p.sample.clone());
                publish(&w, &s);
                return;
            }
            "training.history.unpin" => {
                s.pinned = None;
                publish(&w, &s);
                return;
            }
            "training.history.artifact" => {
                if let Some(link) = s.pages.get(s.page).and_then(|p| p.link.as_ref())
                    && !w.invoke_genomes_select(link.brain, link.id.clone().into())
                {
                    s.error =
                        "The genome browser is busy. Try again when its current action finishes."
                            .into();
                    publish(&w, &s);
                }
                return;
            }
            _ => return,
        };
        match commands.try_send(request) {
            Ok(()) => {
                s.busy = true;
                s.error.clear();
                s.snapshot.clear();
                s.pages.clear();
            }
            Err(e) => {
                s.error = e.to_string();
                s.pages.clear();
            }
        }
        publish(&w, &s);
    });
    let timer = Timer::default();
    let weak = w.as_weak();
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(w) = weak.upgrade() else { return };
        if let Ok(reply) = receiver.try_recv() {
            let mut s = state.borrow_mut();
            s.busy = false;
            if let Some(c) = reply.catalog {
                s.keys = c.keys;
                s.warning = c.warnings.join("\n");
            }
            if let Some(key) = reply.key {
                if s.section == 2 {
                    s.trial = Some(key.clone());
                } else {
                    s.run = Some(key.clone());
                }
                s.key = Some(key);
            }
            match reply.record {
                Ok((pages, snapshot)) => {
                    s.pages = pages;
                    s.snapshot = snapshot;
                    s.error.clear();
                }
                Err(e) => {
                    s.pages.clear();
                    s.snapshot = serde_json::json!({"selection": s.key, "error": e}).to_string();
                    s.error = e;
                }
            }
            s.page = s.page.min(s.pages.len().saturating_sub(1));
            publish(&w, &s);
        }
    });
    Ok(Browser {
        timer,
        sender,
        worker: Some(worker),
    })
}

fn publish(w: &MainWindow, s: &State) {
    w.set_history_busy(s.busy);
    w.set_history_section(s.section);
    w.set_history_count(s.keys.len() as i32);
    w.set_history_details(s.pages.len() as i32);
    let index = s
        .keys
        .iter()
        .position(|k| Some(k) == s.key.as_ref())
        .map_or(0, |i| i + 1);
    w.set_history_label(
        s.key
            .as_ref()
            .map_or_else(
                || "No saved records".into(),
                |k| {
                    format!(
                        "{index}/{} · {}",
                        s.keys.len(),
                        if k.kind == history::Kind::Run {
                            k.id.clone()
                        } else {
                            format!("{:?} · {}", k.kind, &k.receipt[..12])
                        }
                    )
                },
            )
            .into(),
    );
    w.set_history_id(s.key.as_ref().map_or("", |k| k.id.as_str()).into());
    let page = s.pages.get(s.page);
    w.set_history_title(
        page.map_or_else(
            || {
                if s.busy {
                    "Reading…".into()
                } else {
                    "Unavailable".into()
                }
            },
            |p| format!("{}/{} · {}", s.page + 1, s.pages.len(), p.title),
        )
        .into(),
    );
    w.set_history_body(
        if !s.error.is_empty() {
            s.error.clone()
        } else {
            page.map_or(String::new(), |p| p.body.clone())
        }
        .into(),
    );
    w.set_history_error(!s.error.is_empty());
    w.set_history_warning(s.warning.clone().into());
    w.set_history_can_open(!s.busy && page.is_some_and(|p| p.link.is_some()));
    w.set_history_can_pin(!s.busy && page.is_some_and(|p| p.sample.is_some()));
    w.set_history_pinned(s.pinned.is_some());
    let comparison = s.pinned.as_ref().map_or(String::new(), |pin| {
        if let Some(current) = page.and_then(|p| p.sample.as_ref()) {
            if let Some(reason) = current.protocol.mismatch(&pin.protocol) { format!("Separate results: {reason}. Pinned: {}", pin.label) }
            else { format!("Same protocol · pinned: {}\n{}\nSelected: {} · Cases may overlap; scores are not pooled.", pin.label, pin.value, current.value) }
        } else { format!("Pinned: {} · select a measured result to compare.", pin.label) }
    });
    w.set_history_comparison(comparison.into());
    w.set_history_snapshot(s.snapshot.clone().into());
}
const IDS: [&str; 9] = [
    "training.history.runs",
    "training.history.next",
    "training.history.detail.next",
    "training.history.artifact",
    "training.history.pin",
    "training.history.refresh",
    "training.history.current",
    "training.history.validation",
    "training.history.trials",
];
pub(crate) fn selected_id(w: &MainWindow) -> &'static str {
    if w.get_history_focus() == 4 && w.get_history_pinned() {
        "training.history.unpin"
    } else {
        IDS[w.get_history_focus().clamp(0, 8) as usize]
    }
}
pub(crate) fn inventory(w: &MainWindow) -> Vec<UiControl> {
    let ready = !w.get_history_busy();
    let mut controls = vec![
        UiControl::new("training.snapshot", "Current training job", false)
            .with_value(w.get_training_snapshot().to_string()),
        UiControl::new("training.status", "Current job state", false)
            .with_value(w.get_training_status_code().to_string()),
        UiControl::new(
            "training.history.snapshot",
            "Selected historical evidence",
            false,
        )
        .with_value(w.get_history_snapshot().to_string()),
        UiControl::new("training.history.detail", "Selected detail", false)
            .with_value(w.get_history_title().to_string()),
        UiControl::new("training.history.comparison", "Comparison", false)
            .with_value(w.get_history_comparison().to_string()),
    ];
    for (id, label, enabled) in [
        ("runs", "Training runs", ready),
        ("validation", "Periodic validation", ready),
        ("trials", "Practice receipts", ready),
        (
            "previous",
            "Previous record",
            ready && w.get_history_count() > 1,
        ),
        ("next", "Next record", ready && w.get_history_count() > 1),
        (
            "detail.previous",
            "Previous detail",
            ready && w.get_history_details() > 1,
        ),
        (
            "detail.next",
            "Next detail",
            ready && w.get_history_details() > 1,
        ),
        (
            "artifact",
            "Open exact genome / brain",
            w.get_history_can_open(),
        ),
        ("pin", "Pin result for comparison", w.get_history_can_pin()),
        ("unpin", "Clear comparison", w.get_history_pinned()),
        ("refresh", "Refresh saved records", ready),
        ("current", "Current job", true),
        ("back", "Back to Training", true),
    ] {
        controls.push(UiControl::new(
            format!("training.history.{id}"),
            label,
            enabled,
        ));
    }
    controls
}
pub(crate) fn handle_action(w: &MainWindow, action: UiAction) {
    let focus = w.get_history_focus();
    match action {
        UiAction::Left | UiAction::Right if focus == 1 || focus == 2 => {
            let part = if focus == 2 { "detail." } else { "" };
            let direction = if action == UiAction::Left {
                "previous"
            } else {
                "next"
            };
            w.invoke_training_command(format!("training.history.{part}{direction}").into());
        }
        UiAction::Up | UiAction::Down | UiAction::Left | UiAction::Right => {
            // Tabs first, followed by record/detail selectors and footer actions.
            let order = [0, 7, 8, 1, 2, 3, 4, 5, 6];
            let index = order.iter().position(|f| *f == focus).unwrap_or(0);
            let next = crate::ui_navigation::moved_selection(
                index as i32,
                IDS.len() as i32,
                if matches!(action, UiAction::Up | UiAction::Left) {
                    -1
                } else {
                    1
                },
            );
            w.set_history_focus(order[next as usize]);
        }
        UiAction::Confirm => w.invoke_training_command(selected_id(w).into()),
        UiAction::Back | UiAction::Controls => {
            w.invoke_training_command("training.history.back".into())
        }
        UiAction::Start => {}
    }
}
