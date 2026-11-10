//! Fixed install commands; no caller-selected paths, programs or devices.
use super::*;

pub const SERVICE_SOCKET: &str = "/run/spacewars-system/install.sock";
pub const HOST_SOCKET: &str = "/run/spacewars-kiosk/install.sock";
pub const RECONCILE_SOCKET: &str = "/run/spacewars-system/install-reconcile.sock";

/// Fixed reconciliation request from the live kiosk. The service authenticates its PID and
/// systemd instance independently; the claimed host is only correlation data.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconcileRequest {
    pub schema: u32,
    pub nonce: u64,
    pub host: HostInstance,
    pub held: Option<Ticket>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallOutcome {
    Committed,
    RolledBack,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RecoveryDecision {
    Clear {},
    Aborted {
        ticket: Ticket,
    },
    Finished {
        ticket: Ticket,
        transaction: Id,
        outcome: InstallOutcome,
    },
    Waiting {},
    Blocked {},
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconcileReply {
    pub schema: u32,
    pub nonce: u64,
    pub host: HostInstance,
    pub service: Id,
    pub result: RecoveryDecision,
}

impl Message for ReconcileRequest {
    fn validate(&self) -> Result<(), Error> {
        if self.schema != SCHEMA || self.nonce == 0 {
            return Err(Error::Invalid);
        }
        self.host.validate()?;
        if let Some(ticket) = &self.held {
            ticket.validate()?;
        }
        Ok(())
    }
}
impl Message for ReconcileReply {
    fn validate(&self) -> Result<(), Error> {
        if self.schema != SCHEMA || self.nonce == 0 {
            return Err(Error::Invalid);
        }
        self.host.validate()?;
        if let RecoveryDecision::Aborted { ticket } | RecoveryDecision::Finished { ticket, .. } =
            &self.result
        {
            ticket.validate()?;
        }
        Ok(())
    }
}

/// Requests to the install service. `Policy` reads the saved installation
/// setting for the Updates panel, and `SavePolicy` saves the settings its
/// change names.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ServiceRequest {
    Status { schema: u32 },
    Offer { schema: u32 },
    Confirm { schema: u32, consent: Consent },
    Cancel { schema: u32, ticket: Ticket },
    Policy { schema: u32 },
    SavePolicy { schema: u32, change: PolicyChange },
}

impl Message for ServiceRequest {
    fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Status { schema: SCHEMA } | Self::Offer { schema: SCHEMA } => Ok(()),
            Self::Confirm {
                schema: SCHEMA,
                consent,
            } => consent.validate(),
            Self::Cancel {
                schema: SCHEMA,
                ticket,
            } => ticket.validate(),
            Self::Policy { schema: SCHEMA } => Ok(()),
            Self::SavePolicy {
                schema: SCHEMA,
                change,
            } if change.valid() => Ok(()),
            _ => Err(Error::Invalid),
        }
    }
}

/// How a ready update installs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallMode {
    /// Only after somebody confirms it.
    Ask,
    /// Also by itself inside the window, from an idle launcher or Clock.
    Automatic,
}

/// One daily interval of the device's local time, in minutes after midnight.
/// The end is exclusive, and an end before the start crosses midnight.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallWindow {
    pub start_minute: u16,
    pub end_minute: u16,
}

impl InstallWindow {
    pub const DAY_MINUTES: u16 = 24 * 60;

    /// Equal ends could mean no time or the whole day, so they are refused.
    pub fn valid(&self) -> bool {
        self.start_minute < Self::DAY_MINUTES
            && self.end_minute < Self::DAY_MINUTES
            && self.start_minute != self.end_minute
    }

    /// The same window moved by whole hours, wrapping at midnight. Its length
    /// and minutes stay as they were.
    pub fn shifted(&self, hours: i32) -> Self {
        let day = i32::from(Self::DAY_MINUTES);
        let shift = |minute: u16| (i32::from(minute) + hours * 60).rem_euclid(day) as u16;
        Self {
            start_minute: shift(self.start_minute),
            end_minute: shift(self.end_minute),
        }
    }
}

/// Settings to save. An absent one keeps its saved value, so a panel showing
/// an old snapshot cannot overwrite the setting it left alone.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<InstallMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<InstallWindow>,
}

impl PolicyChange {
    /// A change names at least one setting, and a window it names is valid.
    pub fn valid(&self) -> bool {
        (self.mode.is_some() || self.window.is_some())
            && self.window.as_ref().is_none_or(InstallWindow::valid)
    }
}

/// The saved setting, as the Updates panel shows it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallPolicy {
    pub mode: InstallMode,
    pub window: InstallWindow,
    /// The device's local wall-clock minute, when it has a usable time. An
    /// automatic installation never starts without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_minute: Option<u16>,
}

impl InstallPolicy {
    fn valid(&self) -> bool {
        self.window.valid()
            && self
                .local_minute
                .is_none_or(|minute| minute < InstallWindow::DAY_MINUTES)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Unavailable,
    Idle,
    Offered,
    Preparing,
    Acknowledged,
    CheckingPayload,
    Writing,
    Flushing,
    ReadingBack,
    CheckingFilesystem,
    RebootRequested,
    RecoveryRequired,
    Aborted,
}

/// Bytes of the inactive root written so far and the image total. The native
/// writer reports them during its write and flush stages; other phases carry
/// no byte counts and the panel shows an indeterminate activity bar.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Written {
    pub bytes: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub phase: Phase,
    pub ticket: Option<Ticket>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written: Option<Written>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceReply {
    pub schema: u32,
    pub error: Option<Failure>,
    pub status: Status,
    pub offer: Option<Offer>,
    pub aborted: Option<Aborted>,
    /// The saved setting, in reply to `policy` and `save-policy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<InstallPolicy>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Failure {
    Invalid,
    Unavailable,
    /// The last update committed on a trial boot that has not been followed
    /// by a normal boot yet; installation admission waits for that restart.
    NormalBootRequired,
    Busy,
    Stale,
    Expired,
    EndGameRequired,
    SaveFailed,
    NotReady,
    TooLate,
}
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        match error {
            Error::Invalid => Self::Invalid,
            Error::Busy => Self::Busy,
            Error::Stale => Self::Stale,
            Error::Expired => Self::Expired,
            Error::EndGameRequired => Self::EndGameRequired,
            Error::SaveFailed => Self::SaveFailed,
            Error::NotReady => Self::NotReady,
            Error::TooLate => Self::TooLate,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case", deny_unknown_fields)]
pub enum HostRequest {
    Observe {
        schema: u32,
        nonce: u64,
    },
    /// `Observe` from a service that is counting down to an installation
    /// nobody confirmed. The app shows the countdown while these keep
    /// arriving, and any use of the app ends it.
    Countdown {
        schema: u32,
        nonce: u64,
        /// The generation that installs when the countdown ends.
        generation: u64,
        /// CLOCK_BOOTTIME milliseconds on the same device.
        ends_ms: u64,
    },
    Prepare {
        schema: u32,
        prepare: Prepare,
    },
    Abort {
        schema: u32,
        aborted: Aborted,
    },
}
impl Message for HostRequest {
    fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Observe { schema: SCHEMA, .. } => Ok(()),
            Self::Countdown {
                schema: SCHEMA,
                generation,
                ends_ms,
                ..
            } if *generation != 0 && *ends_ms != 0 => Ok(()),
            Self::Prepare {
                schema: SCHEMA,
                prepare,
            } => prepare.validate(),
            Self::Abort {
                schema: SCHEMA,
                aborted,
            } => aborted.validate(),
            _ => Err(Error::Invalid),
        }
    }
}

/// What the app shows, for a decision that would interrupt it without asking.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scene {
    /// The launcher or one of its panels; no scenario is running.
    Launcher,
    /// The Clock, however it was started.
    Clock,
    /// Any other scenario, whether it is running, paused or finished and
    /// whoever started it.
    Game,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostView {
    pub nonce: u64,
    pub host: HostInstance,
    pub revision: u64,
    pub settings_revision: u64,
    pub game: bool,
    pub fenced: bool,
    pub available: bool,
    /// Absent from an app that does not report it; an automatic installation
    /// then assumes a game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<Scene>,
    /// Milliseconds since a person last used the app, counted from its start
    /// at the latest. Absent when the app cannot tell, which is never idle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostReply {
    pub schema: u32,
    pub error: Option<Failure>,
    pub view: Option<HostView>,
    pub acknowledgement: Option<Acknowledgement>,
}

impl Message for ServiceReply {
    fn validate(&self) -> Result<(), Error> {
        if self.schema != SCHEMA {
            return Err(Error::Invalid);
        }
        if matches!(self.status.phase, Phase::Idle | Phase::Unavailable)
            != self.status.ticket.is_none()
        {
            return Err(Error::Invalid);
        }
        if let Some(ticket) = &self.status.ticket {
            ticket.validate()?;
        }
        if let Some(written) = &self.status.written
            && (written.bytes > written.total || self.status.ticket.is_none())
        {
            return Err(Error::Invalid);
        }
        if let Some(offer) = &self.offer {
            offer.validate()?;
            if self.error.is_some()
                || self.status.phase != Phase::Offered
                || self.status.ticket.as_ref() != Some(&offer.ticket)
            {
                return Err(Error::Invalid);
            }
        }
        if let Some(aborted) = &self.aborted {
            aborted.validate()?;
            if self.status.phase != Phase::Aborted
                || self.status.ticket.as_ref() != Some(&aborted.ticket)
            {
                return Err(Error::Invalid);
            }
        } else if self.status.phase == Phase::Aborted {
            return Err(Error::Invalid);
        }
        if let Some(policy) = &self.policy
            && (!policy.valid() || self.error.is_some() || self.offer.is_some())
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
impl Message for HostReply {
    fn validate(&self) -> Result<(), Error> {
        if self.schema != SCHEMA {
            return Err(Error::Invalid);
        }
        if (self.error.is_some() && (self.view.is_some() || self.acknowledgement.is_some()))
            || (self.view.is_some() && self.acknowledgement.is_some())
        {
            return Err(Error::Invalid);
        }
        if let Some(view) = &self.view {
            view.host.validate()?;
        }
        if let Some(ack) = &self.acknowledgement {
            ack.validate()?;
        }
        Ok(())
    }
}
