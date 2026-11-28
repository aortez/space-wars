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

pub(crate) fn inventory(w: &MainWindow) -> Vec<UiControl> {
    let mut controls = vec![
        UiControl::new("training.snapshot", "Training job", false)
            .with_value(w.get_training_snapshot().to_string()),
        UiControl::new("training.status", "Run state", false)
            .with_value(w.get_training_status_code().to_string()),
        UiControl::new("training.duck.current", "Current replay", false).with_value(format!(
            "{} · {}",
            w.get_duck_current_id(),
            w.get_duck_current_details()
        )),
        UiControl::new("training.duck.best", "Best replay", false).with_value(format!(
            "{} · {}",
            w.get_duck_best_id(),
            w.get_duck_best_details()
        )),
        UiControl::new("training.duck.pending", "Generation settings", false)
            .with_value(w.get_duck_edit_status().to_string()),
    ];
    for (id, label, enabled) in [
        ("training.pause", "Pause", w.get_training_can_pause()),
        ("training.resume", "Resume", w.get_training_can_resume()),
        ("training.stop", "Save and stop", w.get_training_can_stop()),
        ("training.new", "New run", w.get_training_can_start()),
        ("training.back", "Back", true),
        ("training.preview.current", "Current", true),
        ("training.preview.best", "Best", true),
        ("training.duck.term", "Chart term", true),
        ("training.duck.steer", "Generation settings", true),
        ("training.preview.speed", "Replay speed", true),
        (
            "training.preview.follow",
            "Follow or pin replay",
            !w.get_duck_compare(),
        ),
        (
            "training.duck.brain.current",
            if w.get_duck_compare() {
                "Inspect left-door replay"
            } else {
                "Inspect current brain"
            },
            !w.get_duck_steering(),
        ),
        (
            "training.duck.brain.best",
            if w.get_duck_compare() {
                "Inspect right-door replay"
            } else {
                "Inspect best brain"
            },
            !w.get_duck_steering(),
        ),
    ] {
        controls.push(UiControl::new(id, label, enabled));
    }
    if !w.get_duck_steering() && w.get_duck_inspect() >= 0 {
        for (id, label) in BRAIN_ACTIONS {
            let enabled = id != "training.duck.brain.compare"
                || w.get_duck_compare()
                || w.get_training_points().row_count() > 0;
            controls.push(UiControl::new(id, label, enabled));
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
    let focus = w.get_training_focus() as usize;
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
            let next = if steer {
                crate::ui_navigation::moved_selection(focus, 12, delta)
            } else if w.get_duck_inspect() >= 0 {
                // The two pane selectors precede the eight inspector actions.
                10 + crate::ui_navigation::moved_selection((focus - 10).max(0), 10, delta)
            } else {
                let count = ACTIONS.len() as i32;
                let mut next = crate::ui_navigation::moved_selection(focus, count, delta);
                if w.get_duck_wide() && matches!(next, 4 | 5) {
                    next = if delta > 0 { 6 } else { 3 };
                }
                if !w.get_duck_wide()
                    && next
                        == (if w.get_training_preview_mode() == 2 {
                            10
                        } else {
                            11
                        })
                {
                    next = crate::ui_navigation::moved_selection(next, count, delta);
                }
                next
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
