//! Manual-install handoff contract. These messages convey consent and app
//! readiness, never permission to write a root slot. Transport adapters must
//! authenticate the root service and the actual kiosk process before using them.
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
#[path = "install_transport.rs"]
pub mod transport;
#[path = "install_wire.rs"]
pub mod wire;

pub const SCHEMA: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 4096;
pub const PREPARE_MS: u64 = 30_000;

/// A public identity, not a bearer secret or proof of verification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Id(String);

impl Id {
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, Error> {
        Self::try_from(
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        )
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Id {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || value.bytes().all(|b| b == b'0')
        {
            return Err(Error::Invalid);
        }
        Ok(Self(value))
    }
}

impl From<Id> for String {
    fn from(value: Id) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    Release,
    Nightly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    /// The verifier's identity for the complete signed image descriptor.
    pub id: Id,
    pub channel: Channel,
    pub generation: u64,
    pub build_id: String,
}

impl Candidate {
    pub fn validate(&self) -> Result<(), Error> {
        if self.generation == 0
            || self.build_id.is_empty()
            || self.build_id.len() > 128
            || !self
                .build_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

/// Compare against SO_PEERCRED, /proc start time and the active systemd
/// invocation. Request JSON alone must never establish this identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostInstance {
    pub pid: u32,
    pub start_ticks: u64,
    pub invocation: String,
}

impl HostInstance {
    pub fn validate(&self) -> Result<(), Error> {
        if self.pid <= 1
            || self.start_ticks == 0
            || self.invocation.len() != 32
            || !self
                .invocation
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.invocation.bytes().all(|b| b == b'0')
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    /// Fresh random identity on every service start, including same-boot restart.
    pub service: Id,
    pub sequence: u64,
    pub candidate: Id,
}

impl Ticket {
    fn validate(&self) -> Result<(), Error> {
        if self.sequence == 0 {
            Err(Error::Invalid)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub schema: u32,
    pub ticket: Ticket,
    pub candidate: Candidate,
    pub ends_game: bool,
    pub expires_ms: u64,
}

/// Sending this command explicitly approves installation and restart of this
/// ticket's image. A game additionally requires its end-session confirmation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Consent {
    pub schema: u32,
    pub ticket: Ticket,
    pub end_game: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prepare {
    pub schema: u32,
    pub ticket: Ticket,
    pub host: HostInstance,
    pub host_revision: u64,
    pub settings_revision: u64,
    /// CLOCK_BOOTTIME milliseconds on the same device; retries never extend it.
    pub expires_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgement {
    pub schema: u32,
    pub ticket: Ticket,
    pub host: HostInstance,
    pub host_revision: u64,
    pub settings_revision: u64,
}

/// Authenticated live cancellation or durable proof that no transaction began.
/// A timeout/disconnect is not an abort.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aborted {
    pub schema: u32,
    pub ticket: Ticket,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Invalid,
    Busy,
    Stale,
    Expired,
    EndGameRequired,
    SaveFailed,
    NotReady,
    TooLate,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid install handoff",
            Self::Busy => "another install handoff is active",
            Self::Stale => "install selection or host changed",
            Self::Expired => "install handoff expired",
            Self::EndGameRequired => "confirm that installation ends the game",
            Self::SaveFailed => "settings save failed",
            Self::NotReady => "app has not saved and presented the updater",
            Self::TooLate => "install handoff already acknowledged or consumed",
        })
    }
}
impl std::error::Error for Error {}

pub trait Message: serde::de::DeserializeOwned {
    fn validate(&self) -> Result<(), Error>;
    fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(Error::Invalid);
        }
        let message: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        message.validate()?;
        Ok(message)
    }
}

fn header(schema: u32, ticket: &Ticket) -> Result<(), Error> {
    if schema != SCHEMA {
        return Err(Error::Invalid);
    }
    ticket.validate()
}

impl Message for Offer {
    fn validate(&self) -> Result<(), Error> {
        header(self.schema, &self.ticket)?;
        self.candidate.validate()?;
        if self.ticket.candidate != self.candidate.id || self.expires_ms == 0 {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
impl Message for Consent {
    fn validate(&self) -> Result<(), Error> {
        header(self.schema, &self.ticket)
    }
}
impl Message for Prepare {
    fn validate(&self) -> Result<(), Error> {
        header(self.schema, &self.ticket)?;
        self.host.validate()?;
        if self.expires_ms == 0 {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
impl Message for Acknowledgement {
    fn validate(&self) -> Result<(), Error> {
        header(self.schema, &self.ticket)?;
        self.host.validate()
    }
}
impl Message for Aborted {
    fn validate(&self) -> Result<(), Error> {
        header(self.schema, &self.ticket)
    }
}

struct Preparing {
    request: Prepare,
    saved: bool,
    presented: bool,
    acknowledged: bool,
}

/// UI-thread state for the future client adapter. All scenario launches,
/// autostart, input and settings changes must serialize through this gate.
#[derive(Default)]
pub struct HostGate {
    service: Option<Id>,
    pending: Option<Preparing>,
    retired: Option<Ticket>,
}

impl HostGate {
    pub fn held(&self) -> Option<&Ticket> {
        self.pending.as_ref().map(|pending| &pending.request.ticket)
    }

    /// Apply only an authenticated, nonce/host-matched recovery response. Clear
    /// cannot release an unknown acknowledged fence. Same-epoch retirement is
    /// retained; a reconciled service restart rejects delayed old preparations.
    pub fn reconciled(&mut self, service: &Id, resolved: Option<&Ticket>) -> Result<(), Error> {
        if let Some(held) = self.held()
            && resolved != Some(held)
        {
            return Err(Error::Stale);
        }
        if let Some(ticket) = resolved {
            ticket.validate()?;
        }
        self.retire();
        if self.service.as_ref() != Some(service) {
            self.service = Some(service.clone());
            self.retired = None;
        }
        if let Some(ticket) = resolved.filter(|ticket| &ticket.service == service)
            && self
                .retired
                .as_ref()
                .is_none_or(|old| old.sequence <= ticket.sequence)
        {
            self.retired = Some(ticket.clone());
        }
        Ok(())
    }

    fn service(&mut self, ticket: &Ticket) -> Result<(), Error> {
        if let Some(service) = &self.service {
            if service != &ticket.service {
                return Err(Error::Stale);
            }
        } else {
            self.service = Some(ticket.service.clone());
        }
        Ok(())
    }

    pub fn fenced(&self) -> bool {
        self.pending.is_some()
    }

    pub fn prepare(
        &mut self,
        request: Prepare,
        host: &HostInstance,
        host_revision: u64,
        settings_revision: u64,
        now_ms: u64,
    ) -> Result<(), Error> {
        request.validate()?;
        if self.retired.as_ref().is_some_and(|old| {
            old.service == request.ticket.service && old.sequence >= request.ticket.sequence
        }) {
            return Err(Error::Stale);
        }
        if &request.host != host
            || request.host_revision != host_revision
            || request.settings_revision != settings_revision
        {
            return Err(Error::Stale);
        }
        if now_ms >= request.expires_ms || request.expires_ms - now_ms > PREPARE_MS {
            return Err(Error::Expired);
        }
        self.service(&request.ticket)?;
        if let Some(pending) = &self.pending {
            return if pending.request == request {
                Ok(())
            } else {
                Err(Error::Busy)
            };
        }
        self.pending = Some(Preparing {
            request,
            saved: false,
            presented: false,
            acknowledged: false,
        });
        Ok(())
    }

    fn pending(&mut self, ticket: &Ticket) -> Result<&mut Preparing, Error> {
        self.pending
            .as_mut()
            .filter(|p| &p.request.ticket == ticket)
            .ok_or(Error::Stale)
    }

    fn retire(&mut self) {
        if let Some(pending) = self.pending.take() {
            self.retired = Some(pending.request.ticket);
        }
    }

    /// Call only for the completion of this exact settings snapshot's receipt.
    pub fn saved(&mut self, ticket: &Ticket, revision: u64, success: bool) -> Result<(), Error> {
        let pending = self.pending(ticket)?;
        if pending.acknowledged {
            return Err(Error::TooLate);
        }
        if revision != pending.request.settings_revision {
            return Err(Error::Stale);
        }
        if !success {
            self.retire();
            return Err(Error::SaveFailed);
        }
        pending.saved = true;
        Ok(())
    }

    pub fn presented(&mut self, ticket: &Ticket) -> Result<(), Error> {
        self.pending(ticket)?.presented = true;
        Ok(())
    }

    /// This is the app-side linearization point. Once acknowledged, keep the
    /// launch/input fence even if the response or service connection is lost.
    pub fn acknowledge(&mut self, ticket: &Ticket, now_ms: u64) -> Result<Acknowledgement, Error> {
        let pending = self.pending(ticket)?;
        if !pending.acknowledged && now_ms >= pending.request.expires_ms {
            self.retire();
            return Err(Error::Expired);
        }
        if !pending.saved || !pending.presented {
            return Err(Error::NotReady);
        }
        pending.acknowledged = true;
        Ok(Acknowledgement {
            schema: SCHEMA,
            ticket: ticket.clone(),
            host: pending.request.host.clone(),
            host_revision: pending.request.host_revision,
            settings_revision: pending.request.settings_revision,
        })
    }

    /// Input, scene or settings mutation before acknowledgement wins. The
    /// caller must advance its host revision before admitting a later Prepare.
    pub fn interrupt(&mut self, ticket: &Ticket) -> Result<(), Error> {
        if self.pending(ticket)?.acknowledged {
            return Err(Error::TooLate);
        }
        self.retire();
        Ok(())
    }

    pub fn expire(&mut self, now_ms: u64) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| !p.acknowledged && now_ms >= p.request.expires_ms)
        {
            self.retire();
        }
    }

    pub fn aborted(&mut self, response: &Aborted) -> Result<(), Error> {
        response.validate()?;
        self.service(&response.ticket)?;
        if self.pending.is_some() {
            self.pending(&response.ticket)?;
            self.retire();
        } else {
            if self.retired.as_ref().is_some_and(|old| {
                old.service == response.ticket.service && old.sequence > response.ticket.sequence
            }) {
                return Err(Error::Stale);
            }
            // An abort can arrive before its queued Prepare. Remember it too.
            self.retired = Some(response.ticket.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
