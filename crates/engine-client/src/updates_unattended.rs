//! An installation nobody pressed Install for: the Automatic setting or a
//! command confirmed it elsewhere. Nobody may be at the screen for its outcome,
//! so that outcome closes Updates once it has gone a minute without use. The
//! outcome usually shows after the restart, so the app notes the installation
//! in its own directory as it starts. The note stays until the device service
//! retires the outcome: a trial boot that commits can show it, and the normal
//! boot after it then shows it again.
use spacewars_control::install::Ticket;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// How long an outcome stays without use before Updates closes.
pub(super) const CLOSE_MS: u64 = 60_000;
/// In the app's directory, next to its settings. Older images ignore it.
const FILE: &str = "unattended-install.json";
/// A ticket takes a few hundred bytes. A longer file is not one of ours.
const MAX_BYTES: u64 = 4096;

pub(super) struct Unattended {
    path: Option<PathBuf>,
    /// The installation nobody pressed Install for, until reconciliation
    /// settles another installation or none.
    ticket: Option<Ticket>,
    /// When that outcome showed, while it waits to close.
    shown_ms: Option<u64>,
    /// `CLOSE_MS`; tests wait less.
    pub(super) close_ms: u64,
}

impl Unattended {
    pub(super) fn new() -> Self {
        Self {
            path: None,
            ticket: None,
            shown_ms: None,
            close_ms: CLOSE_MS,
        }
    }

    /// Reads the installation noted before the app restarted. A note that
    /// cannot be read names none.
    pub(super) fn load(&mut self, directory: Option<&Path>) {
        self.path = directory.map(|directory| directory.join(FILE));
        self.ticket = self.path.as_deref().and_then(|path| {
            let mut bytes = Vec::new();
            fs::File::open(path)
                .ok()?
                .take(MAX_BYTES)
                .read_to_end(&mut bytes)
                .ok()?;
            serde_json::from_slice(&bytes).ok()
        });
    }

    /// An installation starts. Without a press here it is noted until the
    /// device service retires its outcome; a pressed one removes any older
    /// note.
    pub(super) fn started(&mut self, ticket: &Ticket, pressed: bool) {
        self.shown_ms = None;
        self.ticket = (!pressed).then(|| ticket.clone());
        let Some(path) = &self.path else {
            return;
        };
        match &self.ticket {
            Some(ticket) => {
                if let Err(error) = write(path, ticket) {
                    // Without the note, the outcome after a restart stays open.
                    tracing::warn!(%error, "unattended installation could not be noted");
                }
            }
            None => remove(path),
        }
    }

    /// Reconciliation settled the installation `ticket`, or found none to
    /// settle, as it does once the device service has retired an outcome. A
    /// note for any other installation is dropped.
    pub(super) fn resolved(&mut self, ticket: Option<&Ticket>) {
        if self.ticket.as_ref() != ticket {
            self.forget();
        }
    }

    /// The installation hold lifted: the installation ended without a restart,
    /// or the restart found its outcome. A noted installation's outcome
    /// closes Updates once it has gone `close_ms` without use.
    pub(super) fn ended(&mut self, showing: bool, now: u64) {
        if self.ticket.is_some() && showing && now != u64::MAX {
            self.shown_ms = Some(now);
        }
    }

    /// Whether the outcome has waited long enough. `idle_ms` is the time since
    /// a person last used the app.
    pub(super) fn due(&mut self, showing: bool, now: u64, idle_ms: Option<u64>) -> bool {
        let Some(shown_ms) = self.shown_ms else {
            return false;
        };
        if !showing {
            // Somebody closed Updates.
            self.shown_ms = None;
            return false;
        }
        let Some(idle_ms) = idle_ms else {
            return false;
        };
        if idle_ms.min(now.saturating_sub(shown_ms)) < self.close_ms {
            return false;
        }
        self.shown_ms = None;
        true
    }

    /// Whether an outcome waits to close.
    pub(super) fn waiting(&self) -> bool {
        self.shown_ms.is_some()
    }

    fn forget(&mut self) {
        self.ticket = None;
        if let Some(path) = &self.path {
            remove(path);
        }
    }
}

fn write(path: &Path, ticket: &Ticket) -> std::io::Result<()> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(&serde_json::to_vec(ticket)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn remove(path: &Path) {
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            tracing::warn!(%error, "unattended installation note could not be removed");
        }
        _ => {}
    }
}
