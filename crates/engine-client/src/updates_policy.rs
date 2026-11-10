//! The saved Automatic install setting: read when Updates opens, shown on its
//! button there and changed on its own page. Its requests have a thread of
//! their own, so a slow save never delays an installation's requests.
use super::{Exchange, MainWindow};
use spacewars_control::install::SCHEMA;
use spacewars_control::install::wire::{
    InstallMode, InstallPolicy, InstallWindow, PolicyChange, ServiceReply, ServiceRequest,
};
use std::sync::mpsc;
use std::time::Instant;

pub(super) struct Setting {
    requests: mpsc::SyncSender<ServiceRequest>,
    replies: mpsc::Receiver<Result<ServiceReply, String>>,
    /// A request is with the service. There is never more than one.
    asking: bool,
    /// The setting as the service last reported it, and when.
    saved: Option<(InstallPolicy, Instant)>,
    /// The change the service is saving now.
    sending: PolicyChange,
    /// Changes made while it saves, sent once it answers.
    unsent: PolicyChange,
    /// The last save failed, so the page shows the saved setting again.
    failed: bool,
    /// The last read failed.
    unavailable: bool,
}

impl Setting {
    pub(super) fn new(exchange: Exchange) -> Self {
        let (requests, work) = mpsc::sync_channel::<ServiceRequest>(1);
        let (answers, replies) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while let Ok(request) = work.recv() {
                if answers.send(exchange(&request)).is_err() {
                    break;
                }
            }
        });
        Self {
            requests,
            replies,
            asking: false,
            saved: None,
            sending: PolicyChange::default(),
            unsent: PolicyChange::default(),
            failed: false,
            unavailable: false,
        }
    }

    /// Reads the setting again as Updates opens. A save still running brings
    /// the setting back with its reply instead.
    pub(super) fn read(&mut self) {
        if self.asking {
            return;
        }
        self.saved = None;
        self.failed = false;
        self.unavailable = false;
        self.ask(ServiceRequest::Policy { schema: SCHEMA });
    }

    /// Whether the setting is known, so that its page can open.
    pub(super) fn known(&self) -> bool {
        self.saved.is_some()
    }

    /// Changes the mode (row 0) or moves the window by an hour (row 1), and
    /// saves only what changed.
    pub(super) fn adjust(&mut self, row: i32, step: i32) {
        let Some((mode, window)) = self.shown() else {
            return;
        };
        match row {
            0 => {
                self.unsent.mode = Some(match mode {
                    InstallMode::Ask => InstallMode::Automatic,
                    InstallMode::Automatic => InstallMode::Ask,
                })
            }
            1 => self.unsent.window = Some(window.shifted(step.signum())),
            _ => return,
        }
        self.failed = false;
        self.flush();
    }

    /// Takes the service's answers, sends the changes that waited for them
    /// and shows the result.
    pub(super) fn tick(&mut self, window: &MainWindow) {
        while let Ok(reply) = self.replies.try_recv() {
            self.answered(reply);
        }
        self.flush();
        self.show(window);
    }

    pub(super) fn show(&self, window: &MainWindow) {
        let shown = self.shown();
        let label = match shown {
            Some((InstallMode::Ask, _)) => "Automatic install: Off".to_owned(),
            Some((InstallMode::Automatic, window)) => {
                format!("Automatic install: {}", span(window))
            }
            None if self.unavailable => "Automatic install: unavailable".to_owned(),
            None => "Automatic install".to_owned(),
        };
        let (mode, span) = match shown {
            Some((InstallMode::Ask, window)) => ("Ask first", span(window)),
            Some((InstallMode::Automatic, window)) => ("Automatic", span(window)),
            None => ("", String::new()),
        };
        let status = if changing(&self.sending) || changing(&self.unsent) {
            "Saving…".to_owned()
        } else if self.failed {
            "Could not save. Try again.".to_owned()
        } else if self.asking {
            "Checking…".to_owned()
        } else {
            match &self.saved {
                Some((policy, at)) => match policy.local_minute {
                    Some(minute) => {
                        // The service's reading, moved on by the time since.
                        let elapsed = (at.elapsed().as_secs() / 60) % 1440;
                        format!(
                            "Saved · Device time {}",
                            clock((u64::from(minute) + elapsed) % 1440)
                        )
                    }
                    None => "Saved · Device time not yet confirmed".to_owned(),
                },
                None => String::new(),
            }
        };
        if window.get_updates_automatic_label() != label.as_str() {
            window.set_updates_automatic_label(label.into());
        }
        if window.get_updates_automatic_mode() != mode {
            window.set_updates_automatic_mode(mode.into());
        }
        if window.get_updates_automatic_window() != span.as_str() {
            window.set_updates_automatic_window(span.into());
        }
        if window.get_updates_automatic_status() != status.as_str() {
            window.set_updates_automatic_status(status.into());
        }
        if window.get_updates_automatic_failed() != self.failed {
            window.set_updates_automatic_failed(self.failed);
        }
    }

    /// The saved setting with the changes not yet saved.
    fn shown(&self) -> Option<(InstallMode, InstallWindow)> {
        let (policy, _) = self.saved.as_ref()?;
        Some((
            self.unsent
                .mode
                .or(self.sending.mode)
                .unwrap_or(policy.mode),
            self.unsent
                .window
                .or(self.sending.window)
                .unwrap_or(policy.window),
        ))
    }

    fn flush(&mut self) {
        if !self.asking && changing(&self.unsent) {
            self.sending = std::mem::take(&mut self.unsent);
            self.ask(ServiceRequest::SavePolicy {
                schema: SCHEMA,
                change: self.sending,
            });
        }
    }

    fn ask(&mut self, request: ServiceRequest) {
        if self.requests.try_send(request).is_ok() {
            self.asking = true;
        } else {
            self.answered(Err("the setting's request thread stopped".into()));
        }
    }

    fn answered(&mut self, reply: Result<ServiceReply, String>) {
        self.asking = false;
        let saving = changing(&std::mem::take(&mut self.sending));
        match reply.ok().and_then(|reply| reply.policy) {
            Some(policy) => {
                self.saved = Some((policy, Instant::now()));
                self.unavailable = false;
            }
            None if saving => {
                // Changes made after the failed one are dropped too: the page
                // shows what is saved, and another press tries again.
                self.failed = true;
                self.unsent = PolicyChange::default();
            }
            None => {
                self.saved = None;
                self.unavailable = true;
            }
        }
    }
}

fn changing(change: &PolicyChange) -> bool {
    change.mode.is_some() || change.window.is_some()
}

fn clock(minute: u64) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

fn span(window: InstallWindow) -> String {
    format!(
        "{}–{}",
        clock(window.start_minute.into()),
        clock(window.end_minute.into())
    )
}
