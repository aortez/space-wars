use super::FIELDS;
use crate::MainWindow;
use slint::Model;
use spacewars_control::{UiAction, UiControl};

const ACTIONS: [&str; 12] = [
    "training.pause",
    "training.stop",
    "training.new",
    "training.back",
    "training.preview.current",
    "training.preview.best",
    "training.duck.term",
    "training.duck.steer",
    "training.preview.speed",
    "training.preview.follow",
    "training.duck.brain.current",
    "training.duck.brain.best",
];

const BRAIN_ACTIONS: [(&str, &str); 8] = [
    ("training.duck.brain.move", "Inspect move output"),
    ("training.duck.brain.jump", "Inspect jump output"),
    ("training.duck.brain.run", "Inspect run output"),
    ("training.duck.brain.dive", "Inspect dive output"),
    ("training.duck.brain.pause", "Play or pause replay"),
    ("training.duck.brain.step", "Advance one decision"),
    (
        "training.duck.brain.compare",
        "Compare champion from both entrances",
    ),
    ("training.duck.brain.close", "Close brain inspector"),
];

fn enabled(w: &MainWindow, id: &str) -> bool {
    let scores = !w.get_duck_steering() && w.get_duck_inspect() < 0;
    match id {
        "training.pause" => w.get_training_can_pause(),
        "training.resume" => w.get_training_can_resume(),
        "training.stop" => w.get_training_can_stop(),
        "training.new" => w.get_training_can_start(),
        "training.preview.follow" => w.get_duck_can_follow(),
        "training.preview.speed" | "training.duck.term" => scores,
        "training.preview.current"
        | "training.preview.best"
        | "training.duck.brain.current"
        | "training.duck.brain.best" => !w.get_duck_steering(),
        "training.duck.replay.toggle" => scores && w.get_training_points().row_count() > 0,
        "training.duck.brain.compare" => {
            w.get_duck_compare() || w.get_training_points().row_count() > 0
        }
        _ => true,
    }
}

pub(crate) fn inventory(w: &MainWindow) -> Vec<UiControl> {
    let latest = if w.get_duck_archived_sample() {
        "Saved sample"
    } else {
        "Latest replay"
    };
    let (first, second) = if w.get_duck_compare() {
        ("Left door", "Right door")
    } else {
        (latest, "Best replay")
    };
    let mut controls = vec![
        UiControl::new("training.snapshot", "Training job", false)
            .with_value(w.get_training_snapshot().to_string()),
        UiControl::new("training.status", "Run state", false)
            .with_value(w.get_training_status_code().to_string()),
        UiControl::new("training.duck.current", first, false).with_value(format!(
            "{} · {}",
            w.get_duck_current_id(),
            w.get_duck_current_details()
        )),
        UiControl::new("training.duck.best", second, false).with_value(format!(
            "{} · {}",
            w.get_duck_best_id(),
            w.get_duck_best_details()
        )),
        UiControl::new("training.duck.pending", "Generation settings", false)
            .with_value(w.get_duck_edit_status().to_string()),
    ];
    for (id, label) in [
        ("training.pause", "Pause"),
        ("training.resume", "Resume"),
        ("training.stop", "Save and stop"),
        ("training.new", "New run"),
        ("training.back", "Back"),
        ("training.preview.current", first),
        ("training.preview.best", second),
        ("training.duck.term", "Chart term"),
        ("training.duck.steer", "Generation settings"),
        ("training.preview.speed", "Replay speed"),
        (
            "training.preview.follow",
            if w.get_training_preview_follow() {
                "Pin replay"
            } else {
                "Follow new"
            },
        ),
        (
            "training.duck.brain.current",
            if w.get_duck_compare() {
                "Inspect left-door replay"
            } else if w.get_duck_archived_sample() {
                "Inspect saved sample brain"
            } else {
                "Inspect latest brain"
            },
        ),
        (
            "training.duck.brain.best",
            if w.get_duck_compare() {
                "Inspect right-door replay"
            } else {
                "Inspect best brain"
            },
        ),
    ] {
        controls.push(UiControl::new(id, label, enabled(w, id)));
    }
    if w.get_duck_result() && !w.get_duck_steering() {
        controls.push(UiControl::new(
            "training.duck.replay.toggle",
            if w.get_duck_replay_paused() {
                "Play replay"
            } else {
                "Pause replay"
            },
            w.get_training_points().row_count() > 0,
        ));
    }
    if !w.get_duck_steering() && w.get_duck_inspect() >= 0 {
        for (id, label) in BRAIN_ACTIONS {
            controls.push(UiControl::new(id, label, enabled(w, id)));
        }
        controls.push(
            UiControl::new("training.duck.brain.status", "Brain inspection", false)
                .with_value(w.get_duck_inspect_status().to_string()),
        );
    }
    if w.get_duck_steering() {
        for field in FIELDS {
            for direction in ["next", "previous"] {
                controls.push(UiControl::new(
                    format!("training.duck.{field}.{direction}"),
                    field,
                    w.get_duck_can_edit(),
                ));
            }
        }
    }
    controls
}

pub(crate) fn selected_id(w: &MainWindow) -> &'static str {
    focus_id(w, w.get_training_focus())
}

fn focus_id(w: &MainWindow, focus: i32) -> &'static str {
    let focus = focus as usize;
    if !w.get_duck_steering() && w.get_duck_inspect() >= 0 && focus >= ACTIONS.len() {
        return BRAIN_ACTIONS
            .get(focus - ACTIONS.len())
            .map_or("training.duck.brain.close", |action| action.0);
    }
    let action = if w.get_duck_steering() {
        if focus < 5 {
            return [
                "training.duck.population.next",
                "training.duck.probability.next",
                "training.duck.scale.next",
                "training.duck.tournament.next",
                "training.duck.mix.next",
            ][focus];
        }
        [0, 1, 2, 3, 7, 8, 9]
            .get(focus.saturating_sub(5))
            .copied()
            .unwrap_or(3)
    } else {
        focus
    };
    if action == 0 && w.get_training_can_resume() {
        "training.resume"
    } else if action == 1 && w.get_duck_result() {
        "training.duck.replay.toggle"
    } else {
        ACTIONS.get(action).copied().unwrap_or("training.back")
    }
}

pub(crate) fn handle_action(w: &MainWindow, action: UiAction) {
    let focus = w.get_training_focus();
    let steer = w.get_duck_steering();
    match action {
        UiAction::Left | UiAction::Right if steer && (0..5).contains(&focus) => {
            let direction = if action == UiAction::Left {
                "previous"
            } else {
                "next"
            };
            w.invoke_training_command(
                format!("training.duck.{}.{direction}", FIELDS[focus as usize]).into(),
            );
        }
        UiAction::Left | UiAction::Right | UiAction::Up | UiAction::Down => {
            let delta = if matches!(action, UiAction::Left | UiAction::Up) {
                -1
            } else {
                1
            };
            let selected_best = w.get_training_preview_mode() == 2;
            let (rail, content): (&[i32], &[i32]) = if steer {
                (
                    &[9, 5, 6, 7, 8],
                    if w.get_duck_can_edit() {
                        &[0, 1, 2, 3, 4]
                    } else {
                        &[]
                    },
                )
            } else if w.get_duck_inspect() >= 0 {
                (&[10, 11, 12, 13, 14, 15, 16, 17, 18, 19], &[])
            } else {
                (
                    &[4, 5, 8, 9, 7, 0, 1, 2, 3],
                    if selected_best { &[11, 6] } else { &[10, 6] },
                )
            };
            let next = if action == UiAction::Right && rail.contains(&focus) && !content.is_empty()
            {
                content[0]
            } else if action == UiAction::Left && content.contains(&focus) {
                if focus == 6 {
                    8
                } else if selected_best {
                    5
                } else {
                    4
                }
            } else {
                let available: Vec<_> = rail
                    .iter()
                    .chain(content)
                    .copied()
                    .filter(|&index| enabled(w, focus_id(w, index)))
                    .collect();
                let index = available.iter().position(|&index| index == focus);
                let next = index.map_or(0, |i| {
                    crate::ui_navigation::moved_selection(i as i32, available.len() as i32, delta)
                        as usize
                });
                available[next]
            };
            w.set_training_focus(next);
        }
        UiAction::Confirm => w.invoke_training_command(selected_id(w).into()),
        UiAction::Back | UiAction::Controls if steer => {
            w.invoke_training_command("training.duck.steer".into())
        }
        UiAction::Back | UiAction::Controls if w.get_duck_inspect() >= 0 => {
            w.invoke_training_command("training.duck.brain.close".into())
        }
        UiAction::Back | UiAction::Controls => w.invoke_training_command("training.back".into()),
        UiAction::Start => {}
    }
}
