//! Training views observe a worker; navigation never starts/stops evaluation.

use engine_agent::training_job::{JobSnapshot, JobSpec, JobState, Recipe, TrainingManager};
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use spacewars_control::{UiAction, UiControl};
use std::{path::PathBuf, time::Duration};

use crate::{MainWindow, TrainingPoint};

pub(crate) struct Training {
    // Stop refreshing before joining the worker at application shutdown.
    _timer: Timer,
    _preview: crate::training_preview::Preview,
    _history: crate::training_history::Browser,
    _manager: TrainingManager,
}

pub(crate) fn install(
    window: &MainWindow,
    root: PathBuf,
) -> Result<Training, engine_agent::evolution::EvolutionError> {
    let manager = TrainingManager::new(root.clone())?;
    let history = crate::training_history::install(window, root)
        .map_err(|e| engine_agent::evolution::EvolutionError(e.to_string()))?;
    let control = manager.control();
    let preview = crate::training_preview::install(window, control.clone())
        .map_err(|e| engine_agent::evolution::EvolutionError(e.to_string()))?;
    publish(window, &control.snapshot());
    publish_setup(window);
    let weak = window.as_weak();
    window.on_training_open(move || {
        let Some(window) = weak.upgrade() else { return };
        if !window.get_launcher_visible() || window.get_launcher_busy() {
            return;
        }
        window.set_launcher_focus_index(7);
        window.set_training_setup(!window.get_training_has_job());
        window.set_training_focus(0);
        window.set_training_history(false);
        window.set_training_visible(true);
    });
    let weak = window.as_weak();
    let commands = control.clone();
    window.on_training_command(move |id| {
        let Some(window) = weak.upgrade() else { return };
        if !window.get_training_visible() || !window.get_launcher_visible() {
            return;
        }
        if !inventory(&window)
            .iter()
            .any(|c| c.id == id.as_str() && c.enabled)
        {
            return;
        }
        let result = match id.as_str() {
            id if id.starts_with("training.history.") => {
                window.invoke_history_command(id.into());
                return;
            }
            id if id.starts_with("training.preview.") => {
                crate::training_preview::command(&window, id);
                return;
            }
            "training.back" => {
                window.set_training_visible(false);
                return;
            }
            "training.new" => {
                window.set_training_setup(true);
                window.set_training_focus(0);
                publish_setup(&window);
                return;
            }
            "training.current" => {
                window.set_training_setup(false);
                window.set_training_focus(0);
                return;
            }
            "training.start" => {
                let result = setup_spec(&window).and_then(|spec| commands.start(spec));
                if result.is_ok() {
                    window.set_training_setup(false);
                    window.set_training_focus(0);
                }
                result
            }
            "training.initial.reset" => {
                window.set_training_seed_genome("".into());
                publish_setup(&window);
                return;
            }
            "training.pause" => commands.pause(),
            "training.resume" => commands.resume(),
            "training.stop" => commands.stop(),
            id => {
                adjust(&window, id);
                return;
            }
        };
        publish(&window, &commands.snapshot());
        if let Err(e) = result {
            window.set_training_error(e.to_string().into());
        }
    });
    let timer = Timer::default();
    let weak = window.as_weak();
    timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        if let Some(window) = weak.upgrade() {
            publish(&window, &control.snapshot());
        }
    });
    Ok(Training {
        _timer: timer,
        _preview: preview,
        _history: history,
        _manager: manager,
    })
}

fn setup_spec(window: &MainWindow) -> Result<JobSpec, engine_agent::evolution::EvolutionError> {
    Ok(JobSpec {
        recipe: if window.get_training_recipe() == 0 {
            Recipe::StationaryArrival
        } else {
            Recipe::MixedArrival
        },
        seed: window.get_training_seed() as u64,
        population: window.get_training_population() as usize,
        generations: window.get_training_generations() as usize,
        initial_genomes: if window.get_training_seed_genome().is_empty() {
            None
        } else {
            Some(vec![
                serde_json::from_str(&window.get_training_seed_genome())
                    .map_err(|e| engine_agent::evolution::EvolutionError(e.to_string()))?,
            ])
        },
    })
}

pub(crate) fn publish_setup(window: &MainWindow) {
    let spec = match setup_spec(window) {
        Ok(spec) => spec,
        Err(e) => {
            window.set_training_error(e.to_string().into());
            return;
        }
    };
    window.set_training_recipe_label(
        if spec.recipe == Recipe::StationaryArrival {
            "Stationary"
        } else {
            "Mixed arrivals"
        }
        .into(),
    );
    let cases = if spec.recipe == Recipe::StationaryArrival {
        "4 stationary arrivals"
    } else {
        "4 stationary + 4 moving + 4 inward arrivals"
    };
    let seeds = spec
        .initial_genomes
        .as_ref()
        .map_or(usize::from(spec.recipe == Recipe::MixedArrival), Vec::len);
    if let Some(genomes) = &spec.initial_genomes {
        let label = if let Some(genome) = genomes.first() {
            spacewars_ai::model::DenseModel::new(genome.clone())
                .map(|m| {
                    format!(
                        "{} + {} random",
                        &m.id()[..12],
                        spec.population.saturating_sub(seeds)
                    )
                })
                .unwrap_or_else(|_| "Invalid selected genome".into())
        } else {
            format!("{} random", spec.population)
        };
        window.set_training_seed_label(label.into());
    } else {
        window.set_training_seed_label(
            format!("{} saved + {} random", seeds, spec.population - seeds).into(),
        );
    }
    window
        .set_training_setup_detail(format!("{cases} · Guidance evolves; ship stays fixed.").into());
    window.set_training_budget(
        format!(
            "{} episode evaluations · This device · 1 worker",
            spec.population * spec.generations * spec.recipe.cases()
        )
        .into(),
    );
}

fn cycle(value: i32, values: &[i32], direction: i32) -> i32 {
    let index = values.iter().position(|v| *v == value).unwrap_or(0) as i32;
    values[(index + direction).rem_euclid(values.len() as i32) as usize]
}

fn adjust(window: &MainWindow, id: &str) {
    let direction = if id.ends_with("previous") { -1 } else { 1 };
    if id.starts_with("training.recipe.") {
        window.set_training_recipe(1 - window.get_training_recipe());
        window.set_training_generations(if window.get_training_recipe() == 0 {
            48
        } else {
            32
        });
    } else if id.starts_with("training.population.") {
        window.set_training_population(cycle(
            window.get_training_population(),
            &[8, 16, 32, 64],
            direction,
        ));
    } else if id.starts_with("training.generations.") {
        window.set_training_generations(cycle(
            window.get_training_generations(),
            &[1, 4, 8, 16, 32, 48, 64],
            direction,
        ));
    } else if id.starts_with("training.seed.") {
        window.set_training_seed((window.get_training_seed() - 1 + direction).rem_euclid(99) + 1);
    }
    publish_setup(window);
}

fn state_label(state: JobState) -> &'static str {
    match state {
        JobState::Loading => "Reading saved run…",
        JobState::Ready => "Ready",
        JobState::Starting => "Starting…",
        JobState::Running => "Training",
        JobState::Validating => "Checking fixed validation cases…",
        JobState::Saving => "Saving checkpoint…",
        JobState::PauseRequested => "Pausing after this candidate…",
        JobState::Paused => "Paused · checkpoint saved",
        JobState::ResumeRequested => "Resuming…",
        JobState::StopRequested => "Saving and stopping…",
        JobState::Stopped => "Stopped · checkpoint saved",
        JobState::Completed => "Completed · results saved",
        JobState::Failed => "Training needs attention",
        JobState::Unavailable => "Training unavailable",
    }
}

pub(crate) fn publish(window: &MainWindow, snapshot: &JobSnapshot) {
    if snapshot.run_id.is_some() && !window.get_training_has_job() {
        window.set_training_setup(false);
    }
    window.set_training_has_job(snapshot.run_id.is_some());
    window.set_training_run_id(snapshot.run_id.as_deref().unwrap_or("").into());
    window.set_training_active(snapshot.state.busy());
    window.set_training_status(state_label(snapshot.state).into());
    window.set_training_status_code(
        serde_json::to_value(snapshot.state)
            .unwrap()
            .as_str()
            .unwrap()
            .into(),
    );
    window.set_training_snapshot(serde_json::to_string(snapshot).unwrap().into());
    window.set_training_error(snapshot.error.as_deref().unwrap_or("").into());
    window.set_training_validation(
        snapshot
            .validation
            .last()
            .map_or_else(
                || "Validation: waiting for a completed generation".into(),
                |v| {
                    format!(
                        "Validation · Gen {}: {} / {} arrivals · Outside selection",
                        v.generation,
                        v.arrivals(),
                        v.cases.len()
                    )
                },
            )
            .into(),
    );
    window
        .set_training_can_start(!snapshot.state.busy() && snapshot.state != JobState::Unavailable);
    window.set_training_can_pause(matches!(
        snapshot.state,
        JobState::Running | JobState::Validating | JobState::Saving
    ));
    window.set_training_can_resume(snapshot.can_resume);
    window.set_training_can_stop(
        snapshot.run_id.is_some()
            && matches!(
                snapshot.state,
                JobState::Running
                    | JobState::Validating
                    | JobState::Saving
                    | JobState::Paused
                    | JobState::Failed
            ),
    );
    window.set_training_launcher_label(
        if snapshot.state.busy() && snapshot.state != JobState::Loading {
            "Training · Active"
        } else if snapshot.can_resume {
            "Training · Saved run"
        } else {
            "Training"
        }
        .into(),
    );
    let Some(spec) = &snapshot.spec else {
        window.set_training_run_label("Ship guidance · This device · 1 worker".into());
        window.set_training_progress_label("Choose a curriculum to begin".into());
        window.set_training_progress(0.0);
        window.set_training_best("No completed generation yet".into());
        window.set_training_saved("".into());
        window.set_training_points(ModelRc::default());
        return;
    };
    let total = spec.population * spec.generations;
    let recipe = match spec.recipe {
        Recipe::StationaryArrival => "Stationary arrival",
        Recipe::MixedArrival => "Mixed arrivals",
        Recipe::DiagnosticLine => "Diagnostic line mover",
    };
    window.set_training_run_label(
        format!("{recipe} · Seed {} · This device · 1 worker", spec.seed).into(),
    );
    window.set_training_progress(snapshot.evaluated_candidates as f32 / total as f32);
    window.set_training_progress_label(
        format!(
            "Generations {} / {} · Candidates {} / {}",
            snapshot.completed_generations, spec.generations, snapshot.evaluated_candidates, total
        )
        .into(),
    );
    window.set_training_best(
        snapshot
            .history
            .last()
            .map_or_else(
                || "Evaluating the first generation…".into(),
                |h| {
                    format!(
                        "Selected best: {} / {} arrivals · Generation {}",
                        h.best_arrivals,
                        spec.recipe.cases(),
                        h.generation
                    )
                },
            )
            .into(),
    );
    let seconds = snapshot.evaluation_millis as f64 / 1000.0;
    let rate = if seconds >= 1.0 {
        format!(
            " · {:.0} episodes/s",
            snapshot.evaluated_episodes as f64 / seconds
        )
    } else {
        String::new()
    };
    window.set_training_saved(
        format!(
            "Saved {} candidates · {} episodes · {:.0}s evaluation{}",
            snapshot.saved_candidates, snapshot.evaluated_episodes, seconds, rate
        )
        .into(),
    );
    let points: Vec<_> = snapshot
        .history
        .iter()
        .map(|h| TrainingPoint {
            best: h.best_arrivals as f32 / spec.recipe.cases() as f32,
            median: h.median_arrivals as f32 / spec.recipe.cases() as f32,
        })
        .collect();
    if !window
        .get_training_points()
        .iter()
        .eq(points.iter().cloned())
    {
        window.set_training_points(ModelRc::new(VecModel::from(points)));
    }
}

pub(crate) fn inventory(window: &MainWindow) -> Vec<UiControl> {
    if window.get_training_history() {
        return crate::training_history::inventory(window);
    }
    let mut controls = vec![
        UiControl::new("training.status", "Run state", false)
            .with_value(window.get_training_status_code().to_string()),
        UiControl::new("training.snapshot", "Training job", false)
            .with_value(window.get_training_snapshot().to_string()),
    ];
    if window.get_training_setup() {
        for (field, label, value) in [
            (
                "recipe",
                "Curriculum",
                window.get_training_recipe_label().to_string(),
            ),
            (
                "population",
                "Population",
                window.get_training_population().to_string(),
            ),
            (
                "generations",
                "Generations",
                window.get_training_generations().to_string(),
            ),
            ("seed", "Run seed", window.get_training_seed().to_string()),
        ] {
            for direction in ["previous", "next"] {
                controls.push(
                    UiControl::new(
                        format!("training.{field}.{direction}"),
                        label,
                        window.get_training_can_start(),
                    )
                    .with_value(value.clone()),
                );
            }
        }
        controls.push(
            UiControl::new(
                "training.initial.reset",
                "Use recipe initial population",
                window.get_training_can_start(),
            )
            .with_value(window.get_training_seed_label().to_string()),
        );
        controls.push(UiControl::new(
            "training.start",
            "Start training",
            window.get_training_can_start(),
        ));
        controls.push(UiControl::new(
            "training.current",
            "View saved run",
            window.get_training_has_job(),
        ));
    } else {
        for (id, label) in [
            ("progress", "Progress"),
            ("current", "Current replay"),
            ("best", "Best replay"),
            ("speed", "Replay speed"),
            ("follow", "Follow latest or pin replay"),
        ] {
            controls.push(UiControl::new(
                format!("training.preview.{id}"),
                label,
                window.get_training_has_job()
                    && (matches!(id, "progress" | "current" | "best")
                        || window.get_training_preview_mode() != 0),
            ));
        }
        controls.push(
            UiControl::new("training.preview.id", "Replay genome", false)
                .with_value(window.get_training_preview_id().to_string()),
        );
        controls.push(
            UiControl::new("training.preview.tick", "Replay tick", false)
                .with_value(window.get_training_preview_tick().to_string()),
        );
        controls.push(
            UiControl::new("training.preview.detail", "Replay details", false)
                .with_value(window.get_training_preview_detail().to_string()),
        );
        controls.push(UiControl::new(
            "training.pause",
            "Pause",
            window.get_training_can_pause(),
        ));
        controls.push(UiControl::new(
            "training.resume",
            "Resume",
            window.get_training_can_resume(),
        ));
        controls.push(UiControl::new(
            "training.stop",
            "Save and stop",
            window.get_training_can_stop(),
        ));
        controls.push(UiControl::new(
            "training.new",
            "New run",
            window.get_training_can_start(),
        ));
    }
    controls.push(UiControl::new("training.back", "Back", true));
    controls.push(UiControl::new(
        "training.history.open",
        "Saved history",
        true,
    ));
    controls
}

pub(crate) fn selected_id(window: &MainWindow) -> &'static str {
    if window.get_training_history() {
        return crate::training_history::selected_id(window);
    }
    if window.get_training_setup() {
        match window.get_training_focus() {
            0 => "training.recipe.next",
            1 => "training.population.next",
            2 => "training.generations.next",
            3 => "training.seed.next",
            4 => "training.initial.reset",
            5 => "training.start",
            7 => "training.current",
            8 => "training.history.open",
            _ => "training.back",
        }
    } else {
        match window.get_training_focus() {
            0 if window.get_training_can_resume() => "training.resume",
            0 => "training.pause",
            1 => "training.stop",
            2 => "training.new",
            4 => "training.preview.progress",
            5 => "training.preview.current",
            6 => "training.preview.best",
            7 => "training.preview.speed",
            8 => "training.preview.follow",
            9 => "training.history.open",
            _ => "training.back",
        }
    }
}

pub(crate) fn handle_action(window: &MainWindow, action: UiAction) {
    if window.get_training_history() {
        crate::training_history::handle_action(window, action);
        return;
    }
    let setup = window.get_training_setup();
    let current = window.get_training_focus();
    let count = if setup { 9 } else { 10 };
    let move_focus = |delta| {
        let mut next = crate::ui_navigation::moved_selection(current, count, delta);
        if setup && !window.get_training_has_job() && next == 7 {
            next = crate::ui_navigation::moved_selection(next, count, delta);
        }
        window.set_training_focus(next);
    };
    match action {
        UiAction::Up | UiAction::Down => move_focus(if action == UiAction::Up { -1 } else { 1 }),
        UiAction::Left | UiAction::Right if setup && current < 4 => {
            let field =
                ["recipe", "population", "generations", "seed"][current.clamp(0, 3) as usize];
            let direction = if action == UiAction::Left {
                "previous"
            } else {
                "next"
            };
            window.invoke_training_command(format!("training.{field}.{direction}").into());
        }
        UiAction::Left | UiAction::Right if setup && current == 4 => {
            window.invoke_training_command("training.initial.reset".into());
        }
        UiAction::Left | UiAction::Right => {
            move_focus(if action == UiAction::Left { -1 } else { 1 })
        }
        UiAction::Confirm => window.invoke_training_command(selected_id(window).into()),
        UiAction::Back | UiAction::Controls => {
            window.invoke_training_command("training.back".into())
        }
        UiAction::Start => {} // Start must not accidentally launch a match or a training run.
    }
}
