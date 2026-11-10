//! When a saved Automatic preference may start an installation attempt: inside
//! the saved window of usable local time, at most once per window, and never
//! twice for one image. Nothing here observes the app, counts down or
//! installs; `install_handoff::automatic` asks and does those.
use crate::install_policy::{Mode, Preference, Status as Policy, Window};
use crate::saved_state::{Revised, Saved};
use crate::schedule_store::StateRecord;
use crate::{Error, Result};
use jiff::civil::Date;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

pub const STATE_DIRECTORY: &str = "/data/spacewars-system/install-admission";
/// The next attempt waits at least this long after the last one, whatever the
/// saved window, the time zone or a corrected clock say about the date.
pub const MINIMUM_GAP_S: i64 = 12 * 60 * 60;
const MAX_BUILD_ID: usize = 256;

/// Usable device time with its local civil reading.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Moment {
    /// Seconds since the Unix epoch.
    pub utc_s: i64,
    pub date: Date,
    /// Minutes after local midnight.
    pub minute: u16,
}

impl Moment {
    pub fn at(time: engine_ota::UsableTime, zone: &TimeZone) -> Self {
        let local = time.0.to_zoned(zone.clone());
        Self {
            utc_s: time.0.as_second(),
            date: local.date(),
            minute: local.hour().unsigned_abs() as u16 * 60 + local.minute().unsigned_abs() as u16,
        }
    }

    /// `None` when the system time zone is unknown: night is never assumed.
    pub fn local(time: engine_ota::UsableTime) -> Option<Self> {
        Some(Self::at(time, &TimeZone::try_system().ok()?))
    }
}

/// What an automatic attempt for the ready image is told.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Gate {
    /// Inside a window that has not had its attempt.
    Open,
    /// The saved mode is Ask, or the preference is unreadable.
    NotAutomatic,
    /// No verified image is ready.
    NoImage,
    /// An automatic attempt already tried this image; only a confirmation
    /// installs it now.
    ImageTried,
    /// No usable time, or no known time zone.
    TimeUnknown,
    OutsideWindow,
    /// This window had its attempt, or the last attempt is too recent.
    WindowUsed,
    /// The saved attempt history is unreadable.
    HistoryUnavailable,
}

/// The window an automatic attempt used.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    /// The local date on which that window opened.
    pub opened: Date,
    pub utc_s: i64,
}

/// The local reading a gate was judged at, so a unit's time zone can be checked
/// from its status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Local {
    pub date: Date,
    /// Minutes after local midnight.
    pub minute: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub gate: Gate,
    /// Absent while time or the time zone is unknown.
    pub local: Option<Local>,
    pub last_attempt: Option<Attempt>,
    /// The image an automatic attempt already tried.
    pub tried_build_id: Option<String>,
}

/// Later admission state belongs in a store of its own. An image rolled back
/// to this version refuses a record with a field it does not know.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    generation: u64,
    attempt: Option<Attempt>,
    tried_build_id: Option<String>,
}

impl StateRecord for Record {
    fn initial() -> Self {
        Self {
            schema: 1,
            generation: 0,
            attempt: None,
            tried_build_id: None,
        }
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.attempt.is_some_and(|attempt| attempt.utc_s < 0)
            || self
                .tried_build_id
                .as_deref()
                .is_some_and(|id| !build_id(id))
        {
            return Err(invalid("invalid install admission history"));
        }
        Ok(())
    }
}

impl Revised for Record {
    fn at(mut self, generation: u64) -> Self {
        self.generation = generation;
        self
    }
}

/// The device service's single owner of the automatic attempt history. A unit
/// that never attempts an automatic installation has no directory.
pub struct History(Saved<Record>);

impl History {
    pub fn open(path: &Path) -> Self {
        Self(Saved::open("install admission history", path))
    }

    /// What `begin` would answer now. Reads the copy in memory and saves
    /// nothing.
    pub fn status(
        &self,
        policy: Option<&Policy>,
        now: Option<&Moment>,
        image: Option<&str>,
    ) -> Status {
        let local = now.map(|now| Local {
            date: now.date,
            minute: now.minute,
        });
        match self.0.read() {
            Some(record) => Status {
                gate: judge(&record, policy, now, image).0,
                local,
                last_attempt: record.attempt,
                tried_build_id: record.tried_build_id,
            },
            None => Status {
                gate: Gate::HistoryUnavailable,
                local,
                last_attempt: None,
                tried_build_id: None,
            },
        }
    }

    /// For the install worker, immediately before a countdown starts.
    /// `Gate::Open` means this window's single attempt is spent, durably, so a
    /// crash or restart cannot count down twice in one window. An error means
    /// nothing may start.
    pub fn begin(
        &self,
        policy: Option<&Policy>,
        now: Option<&Moment>,
        image: Option<&str>,
    ) -> Result<Gate> {
        let mut gate = Gate::HistoryUnavailable;
        self.0.change(|record| {
            let (answer, attempt) = judge(record, policy, now, image);
            gate = answer;
            Ok(Record {
                attempt: attempt.or(record.attempt),
                ..record.clone()
            })
        })?;
        Ok(gate)
    }

    /// Remembers the image once an automatic attempt consumed its consent.
    /// Whatever the outcome, that image never installs by itself again.
    pub fn tried(&self, image: &str) -> Result<()> {
        if !build_id(image) {
            return Err(invalid("invalid image build identity"));
        }
        self.0.change(|record| {
            Ok(Record {
                tried_build_id: Some(image.to_owned()),
                ..record.clone()
            })
        })?;
        Ok(())
    }
}

/// Usable device time in local terms, or `None` while either is unknown.
pub type Clock = Arc<dyn Fn() -> Option<Moment> + Send + Sync>;

/// The gate as this device stands: the saved preference, usable local time and
/// the attempt history read together, for the status and for the service that
/// starts an attempt.
pub struct Gatekeeper {
    preference: Arc<Preference>,
    history: History,
    now: Clock,
}

impl Gatekeeper {
    pub fn new(preference: Arc<Preference>, history: History, now: Clock) -> Self {
        Self {
            preference,
            history,
            now,
        }
    }

    /// What `begin` would answer for this ready image. Saves nothing.
    pub fn status(&self, image: Option<&str>) -> Status {
        self.history.status(
            self.preference.status().as_ref(),
            (self.now)().as_ref(),
            image,
        )
    }

    /// Whether the saved mode is Automatic now. A countdown reads it again, so
    /// that saving Ask stops one that already spent its window.
    pub fn automatic(&self) -> bool {
        self.preference
            .status()
            .is_some_and(|policy| policy.mode == Mode::Automatic)
    }

    /// See `History::begin`.
    pub fn begin(&self, image: &str) -> Result<Gate> {
        self.history.begin(
            self.preference.status().as_ref(),
            (self.now)().as_ref(),
            Some(image),
        )
    }

    /// See `History::tried`.
    pub fn tried(&self, image: &str) -> Result<()> {
        self.history.tried(image)
    }
}

/// The answer, and the attempt to save with it: the new one when the gate is
/// open, or the old one restated when the clock reads earlier than it.
fn judge(
    record: &Record,
    policy: Option<&Policy>,
    now: Option<&Moment>,
    image: Option<&str>,
) -> (Gate, Option<Attempt>) {
    let Some(policy) = policy.filter(|policy| policy.mode == Mode::Automatic) else {
        return (Gate::NotAutomatic, None);
    };
    let Some(image) = image else {
        return (Gate::NoImage, None);
    };
    if record.tried_build_id.as_deref() == Some(image) {
        return (Gate::ImageTried, None);
    }
    let Some(now) = now else {
        return (Gate::TimeUnknown, None);
    };
    let Some(opened) = opened(policy.window, now) else {
        return (Gate::OutsideWindow, None);
    };
    let attempt = Attempt {
        opened,
        utc_s: now.utc_s,
    };
    match record.attempt {
        // The clock was corrected to before the saved attempt, so the two
        // cannot be ordered. This window counts as used and the attempt is
        // restated in the corrected time: a clock that once ran ahead costs
        // one window, not every window until the wrong date.
        Some(last) if now.utc_s < last.utc_s => (Gate::WindowUsed, Some(attempt)),
        Some(last) if opened <= last.opened || now.utc_s - last.utc_s < MINIMUM_GAP_S => {
            (Gate::WindowUsed, None)
        }
        _ => (Gate::Open, Some(attempt)),
    }
}

/// The local date on which the window containing this moment opened. A window
/// that crosses midnight opened the evening before its early hours.
fn opened(window: Window, now: &Moment) -> Option<Date> {
    let (start, end, minute) = (window.start_minute, window.end_minute, now.minute);
    if start < end {
        (start <= minute && minute < end).then_some(now.date)
    } else if minute >= start {
        Some(now.date)
    } else if minute < end {
        now.date.yesterday().ok()
    } else {
        None
    }
}

fn build_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_BUILD_ID && id.bytes().all(|b| b.is_ascii_graphic())
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

#[cfg(test)]
#[path = "install_admission_tests.rs"]
mod tests;
