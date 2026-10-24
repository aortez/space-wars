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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ServiceRequest {
    Status { schema: u32 },
    Offer { schema: u32 },
    Confirm { schema: u32, consent: Consent },
    Cancel { schema: u32, ticket: Ticket },
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
            _ => Err(Error::Invalid),
        }
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
    Aborted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub phase: Phase,
    pub ticket: Option<Ticket>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceReply {
    pub schema: u32,
    pub error: Option<Failure>,
    pub status: Status,
    pub offer: Option<Offer>,
    pub aborted: Option<Aborted>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Failure {
    Invalid,
    Unavailable,
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
    Observe { schema: u32, nonce: u64 },
    Prepare { schema: u32, prepare: Prepare },
    Abort { schema: u32, aborted: Aborted },
}
impl Message for HostRequest {
    fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Observe { schema: SCHEMA, .. } => Ok(()),
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
