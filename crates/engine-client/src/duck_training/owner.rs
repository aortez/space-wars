//! Serializes the view's lifecycle and edit commands against the headless trainer.
use engine_agent::{
    trainer::{Generation, GenerationSummary, LatestVisit, Trainer, TrainerConfig},
    training_workers::WorkerControl,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, Sender},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) enum Command {
    Start(TrainerConfig),
    Pause,
    Stop,
    Resume,
    Deselect,
    Shutdown,
}

pub(super) enum Event {
    Opened {
        path: PathBuf,
        config: TrainerConfig,
        pending: Option<(TrainerConfig, u32)>,
        completed: u32,
        history: Vec<GenerationSummary>,
    },
    Scoring(u32, TrainerConfig, WorkerControl, LatestVisit),
    Finished(Box<Generation>),
    RecoveryFailed(String, String),
    Paused,
    Stopped,
    Complete,
    Failed(String),
}

pub(super) fn serve(
    root: PathBuf,
    commands: Receiver<Command>,
    events: Sender<Event>,
    restore: bool,
) {
    let pointer = root.join("last-run");
    let mut active = None;
    let mut directory = None;
    let mut running = false;
    if restore
        && let Ok(id) = fs::read_to_string(&pointer)
        && Path::new(id.trim()).components().count() == 1
        && id.trim().starts_with("run-")
    {
        let path = root.join(id.trim());
        match Trainer::resume(path.clone()).and_then(|trainer| {
            opened(&trainer, &path, &events)?;
            Ok(trainer)
        }) {
            Ok(trainer) => {
                if let Some(generation) = trainer.latest() {
                    let _ = events.send(Event::Finished(Box::new(generation.clone())));
                }
                if trainer.completed() >= trainer.config().generations {
                    let _ = events.send(Event::Complete);
                } else {
                    let _ = events.send(Event::Paused);
                    active = Some(trainer);
                }
                directory = Some(path);
            }
            Err(error) => {
                let _ = events.send(Event::RecoveryFailed(id, error));
            }
        }
    }

    loop {
        let command = if running {
            match commands.try_recv() {
                Ok(c) => Some(c),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(_) => return,
            }
        } else {
            let Ok(c) = commands.recv() else { return };
            Some(c)
        };
        if let Some(command) = command {
            match command {
                Command::Shutdown => return,
                Command::Deselect => {
                    active = None;
                    let clear_pointer = directory.take().is_some();
                    running = false;
                    if clear_pointer && pointer.exists() {
                        // The run remains available by its directory from the CLI.
                        if let Err(error) = fs::write(&pointer, "") {
                            let _ = events.send(Event::Failed(error.to_string()));
                        }
                    }
                }
                Command::Start(config) => {
                    let result = (|| {
                        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                        let nonce = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map_err(|e| e.to_string())?
                            .as_nanos();
                        let id = format!("run-{nonce}");
                        let path = root.join(&id);
                        let trainer = Trainer::new(config, path.clone())?;
                        let mut latest =
                            tempfile::NamedTempFile::new_in(&root).map_err(|e| e.to_string())?;
                        std::io::Write::write_all(&mut latest, id.as_bytes())
                            .map_err(|e| e.to_string())?;
                        latest.persist(&pointer).map_err(|e| e.to_string())?;
                        opened(&trainer, &path, &events)?;
                        Ok::<_, String>((trainer, path))
                    })();
                    match result {
                        Ok((trainer, path)) => {
                            active = Some(trainer);
                            directory = Some(path);
                            running = true;
                        }
                        Err(e) => {
                            running = false;
                            let _ = events.send(Event::Failed(e));
                        }
                    }
                }
                Command::Pause | Command::Stop => {
                    running = false;
                    let complete = active
                        .as_ref()
                        .is_some_and(|trainer| trainer.completed() >= trainer.config().generations);
                    let state = if complete {
                        Event::Complete
                    } else if matches!(command, Command::Stop) {
                        Event::Stopped
                    } else {
                        Event::Paused
                    };
                    if complete || matches!(command, Command::Stop) {
                        active = None;
                    }
                    let _ = events.send(state);
                }
                Command::Resume => {
                    if active.is_none()
                        && let Some(path) = &directory
                    {
                        match Trainer::resume(path.clone()) {
                            Ok(trainer) => active = Some(trainer),
                            Err(e) => {
                                let _ = events.send(Event::Failed(e));
                            }
                        }
                    }
                    running = active.is_some();
                }
            }
            continue;
        }

        let Some(trainer) = &mut active else {
            running = false;
            continue;
        };
        if trainer.completed() >= trainer.config().generations {
            running = false;
            active = None;
            let _ = events.send(Event::Complete);
            continue;
        }
        let workers = trainer.worker_control();
        match trainer.run_generation_live(|generation, settings, latest| {
            let _ = events.send(Event::Scoring(
                generation,
                settings.clone(),
                workers,
                latest,
            ));
        }) {
            Ok(generation) => {
                let _ = events.send(Event::Finished(Box::new(generation)));
            }
            Err(e) => {
                running = false;
                let _ = events.send(Event::Failed(e));
            }
        }
    }
}

fn opened(trainer: &Trainer, path: &Path, events: &Sender<Event>) -> Result<(), String> {
    let _ = events.send(Event::Opened {
        path: path.to_owned(),
        config: trainer.config().clone(),
        pending: trainer.pending().cloned().zip(trainer.pending_generation()),
        completed: trainer.completed(),
        history: trainer.history()?,
    });
    Ok(())
}
