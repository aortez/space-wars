//! Saved installation preference: whether a ready update waits for an explicit
//! confirmation or may also install by itself inside a nightly window.
//! This record only remembers the choice. It never installs, schedules or
//! authorizes an installation, and no admission reads it yet.
use crate::schedule_store::{StateRecord, Store};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;

pub const STATE_DIRECTORY: &str = "/data/spacewars-system/install-policy";
const DAY_MINUTES: u16 = 24 * 60;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// A ready update installs only after an explicit confirmation.
    #[default]
    Ask,
    /// A ready update may also install by itself inside the window.
    Automatic,
}

/// One daily interval of local wall-clock time, in minutes after midnight. The
/// end is exclusive, and an end before the start crosses midnight.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub start_minute: u16,
    pub end_minute: u16,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            start_minute: 3 * 60,
            end_minute: 5 * 60,
        }
    }
}

impl Window {
    /// Equal ends could mean no time or the whole day, so they are refused.
    pub fn valid(&self) -> bool {
        self.start_minute < DAY_MINUTES
            && self.end_minute < DAY_MINUTES
            && self.start_minute != self.end_minute
    }
}

/// `HH:MM-HH:MM` on a 24-hour clock, such as `03:00-05:00` or `22:30-01:00`.
impl FromStr for Window {
    type Err = String;

    fn from_str(text: &str) -> std::result::Result<Self, String> {
        let minute = |text: &str| {
            let (hours, minutes) = text.split_once(':')?;
            let two_digits =
                |part: &str| part.len() == 2 && part.bytes().all(|b| b.is_ascii_digit());
            if !two_digits(hours) || !two_digits(minutes) {
                return None;
            }
            let (hours, minutes): (u16, u16) = (hours.parse().ok()?, minutes.parse().ok()?);
            (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
        };
        text.split_once('-')
            .and_then(|(start, end)| {
                Some(Self {
                    start_minute: minute(start)?,
                    end_minute: minute(end)?,
                })
            })
            .filter(Self::valid)
            .ok_or_else(|| {
                "expected HH:MM-HH:MM with two different times, such as 03:00-05:00".into()
            })
    }
}

/// A requested change. An absent field keeps its saved value, so a caller
/// showing an old snapshot cannot overwrite the setting it did not touch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Change {
    pub mode: Option<Mode>,
    pub window: Option<Window>,
}

impl Change {
    pub fn valid(&self) -> bool {
        (self.mode.is_some() || self.window.is_some())
            && self.window.is_none_or(|window| window.valid())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    /// Zero until the first saved change; each saved change adds one.
    pub generation: u64,
    pub mode: Mode,
    pub window: Window,
}

/// Later installation state belongs in a store of its own. An image rolled
/// back to this version refuses a record with a field it does not know.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    generation: u64,
    mode: Mode,
    window: Window,
}

impl StateRecord for Record {
    fn initial() -> Self {
        Self {
            schema: 1,
            generation: 0,
            mode: Mode::default(),
            window: Window::default(),
        }
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1 || !self.window.valid() {
            return Err(invalid("invalid install policy"));
        }
        Ok(())
    }
}

enum State {
    /// Nothing was ever saved: the defaults apply and no directory exists.
    Default,
    Stored(Store<Record>, Record),
    /// Existing state could not be read. Defaults are not assumed in its
    /// place, and a change is refused until it reads again.
    Unavailable,
}

/// The device service's single owner of the saved preference. Reading returns
/// the copy in memory; only a change touches storage.
pub struct Preference {
    path: PathBuf,
    /// Held across a save, so changes apply one at a time.
    state: Mutex<State>,
    /// What status reports. A save in progress never delays a reader.
    published: Mutex<Option<Status>>,
}

impl Preference {
    /// Creates nothing: a unit that keeps the defaults has no directory.
    pub fn open(path: &Path) -> Self {
        let state = load(path);
        Self {
            path: path.into(),
            published: Mutex::new(snapshot(&state)),
            state: Mutex::new(state),
        }
    }

    /// `None` while saved state is unreadable. Whoever decides an installation
    /// must then treat the mode as `Ask`.
    pub fn status(&self) -> Option<Status> {
        self.published.lock().ok()?.clone()
    }

    /// Saves durably before reporting the new status. A change that alters
    /// nothing writes nothing.
    pub fn change(&self, change: Change) -> Result<Status> {
        if !change.valid() {
            return Err(invalid("invalid install policy change"));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| invalid("install policy lock poisoned"))?;
        let result = apply(&self.path, &mut state, change);
        *self
            .published
            .lock()
            .map_err(|_| invalid("install policy lock poisoned"))? = snapshot(&state);
        result
    }
}

fn apply(path: &Path, state: &mut State, change: Change) -> Result<Status> {
    if matches!(*state, State::Unavailable) {
        *state = load(path);
    }
    let current = match &*state {
        State::Default => Record::initial(),
        State::Stored(_, record) => record.clone(),
        State::Unavailable => return Err(invalid("saved install policy is unreadable")),
    };
    let mut next = current.clone();
    if let Some(mode) = change.mode {
        next.mode = mode;
    }
    if let Some(window) = change.window {
        next.window = window;
    }
    if next == current {
        return Ok(status(&current));
    }
    next.generation = current
        .generation
        .checked_add(1)
        .ok_or_else(|| invalid("install policy generation exhausted"))?;
    let saved = match std::mem::replace(state, State::Unavailable) {
        State::Stored(store, _) => Ok(store),
        _ => Store::<Record>::open(path).and_then(|(store, found)| {
            if found == current {
                Ok(store)
            } else {
                Err(invalid("install policy appeared outside its owner"))
            }
        }),
    }
    .and_then(|mut store| store.save(&next).map(|()| store));
    match saved {
        Ok(store) => {
            let saved = status(&next);
            *state = State::Stored(store, next);
            Ok(saved)
        }
        Err(error) => {
            // The failed store is already dropped and its lock released, so
            // this reads whichever record is durable.
            *state = load(path);
            Err(error)
        }
    }
}

fn snapshot(state: &State) -> Option<Status> {
    match state {
        State::Default => Some(status(&Record::initial())),
        State::Stored(_, record) => Some(status(record)),
        State::Unavailable => None,
    }
}

fn load(path: &Path) -> State {
    match Store::<Record>::existing(path) {
        Ok(None) => State::Default,
        Ok(Some((store, record))) => State::Stored(store, record),
        Err(error) => {
            tracing::warn!(%error, "saved install policy is unreadable; updates wait for confirmation");
            State::Unavailable
        }
    }
}

fn status(record: &Record) -> Status {
    Status {
        generation: record.generation,
        mode: record.mode,
        window: record.window,
    }
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

#[cfg(test)]
#[path = "install_policy_tests.rs"]
mod tests;
