//! Manual consent and app acknowledgement, independent of disk installation.
//! The native service adapter is intentionally not enabled by this module.
//! Consuming authorization does not construct a RecoveryCapability, acquire
//! locks, grant maintenance ownership or replace final payload verification.
use spacewars_control::install::{
    Aborted, Acknowledgement, Candidate, Channel, Consent, Error, HostInstance, Id, Message, Offer,
    PREPARE_MS, Prepare, SCHEMA, Ticket,
};
use std::io::Read;

const OFFER_MS: u64 = 5 * 60_000;
const HOST_FRESH_MS: u64 = 2_000;

/// Constructed only from the verifier, never by decoding a caller's descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ready {
    candidate: Candidate,
    selection_generation: u64,
}

impl Ready {
    pub fn from_verified(
        candidate: &engine_ota::VerifiedCandidate,
        selection_generation: u64,
    ) -> Result<Self, Error> {
        let image = candidate.image();
        let value = Self {
            candidate: Candidate {
                id: Id::from_bytes(*candidate.id().as_bytes())?,
                channel: match image.channel {
                    engine_ota::Channel::Release => Channel::Release,
                    engine_ota::Channel::Nightly => Channel::Nightly,
                },
                generation: image.generation,
                build_id: image.build_id.clone(),
            },
            selection_generation,
        };
        value.candidate.validate()?;
        Ok(value)
    }
}

/// Fresh UI-thread evidence, authenticated against the live kiosk instance.
/// Revision changes on unrelated input, scene, settings and setup changes.
/// The confirmation action and handoff's own updater screen retain it. Paused games
/// still count as games. Missing/crashed hosts cannot be treated as idle.
#[derive(Clone, Debug)]
pub struct HostObservation {
    pub instance: HostInstance,
    pub revision: u64,
    pub settings_revision: u64,
    pub game: bool,
    pub fenced: bool,
    pub observed_ms: u64,
}

/// Owner-sampled inputs, not a wire request. The adapter serializes these with
/// channel/ready changes and acquires actual install/maintenance locks before
/// the final consume. A Boolean from a socket caller is not maintenance proof.
#[derive(Clone, Debug)]
pub struct Context {
    pub boot_id: String,
    pub ready: Option<Ready>,
    pub host: Option<HostObservation>,
    pub maintenance_clear: bool,
}

#[derive(Clone)]
struct Selection {
    offer: Offer,
    ready: Ready,
    host: HostObservation,
}

enum State {
    Idle,
    Offered(Selection),
    Preparing(Selection, Prepare),
    Aborted(Ticket),
    Consumed(Ticket),
}

/// One bounded handoff at a time. No queue, persistence or automatic retries.
/// Restart discards uncommitted consent and creates a fresh random epoch.
pub struct Coordinator {
    boot_id: String,
    epoch: Id,
    sequence: u64,
    observed_ms: u64,
    state: State,
}

/// One-use local value for the future installer adapter; deliberately neither
/// Clone nor serializable. Only the durable recovery journal authorizes writes.
pub struct ManualAuthorization {
    ticket: Ticket,
    selection_generation: u64,
}

impl ManualAuthorization {
    pub fn ticket(&self) -> &Ticket {
        &self.ticket
    }
    pub fn selection_generation(&self) -> u64 {
        self.selection_generation
    }
}

impl Coordinator {
    pub fn new(boot_id: String) -> crate::Result<Self> {
        if boot_id.is_empty() || boot_id.len() > 128 || boot_id.chars().any(char::is_control) {
            return Err(crate::Error::Invalid(
                "invalid handoff boot identity".into(),
            ));
        }
        let mut bytes = [0; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        let epoch = Id::from_bytes(bytes).map_err(|e| crate::Error::Invalid(e.to_string()))?;
        Ok(Self {
            boot_id,
            epoch,
            sequence: 0,
            observed_ms: 0,
            state: State::Idle,
        })
    }

    fn inputs<'a>(
        &mut self,
        context: &'a Context,
        now_ms: u64,
    ) -> Result<(&'a Ready, &'a HostObservation), Error> {
        if context.boot_id != self.boot_id || now_ms < self.observed_ms {
            return Err(Error::Stale);
        }
        self.observed_ms = now_ms;
        if !context.maintenance_clear {
            return Err(Error::Busy);
        }
        let ready = context.ready.as_ref().ok_or(Error::NotReady)?;
        let host = context.host.as_ref().ok_or(Error::NotReady)?;
        host.instance.validate()?;
        if now_ms
            .checked_sub(host.observed_ms)
            .is_none_or(|age| age > HOST_FRESH_MS)
        {
            return Err(Error::Stale);
        }
        Ok((ready, host))
    }

    pub fn offer(&mut self, context: &Context, now_ms: u64) -> Result<Offer, Error> {
        match &self.state {
            State::Offered(selected) => {
                let selected = selected.clone();
                if self
                    .revalidate(&selected, context, now_ms, selected.offer.expires_ms)
                    .is_ok()
                {
                    return Ok(selected.offer);
                }
            }
            State::Preparing(selected, prepare) => {
                let selected = selected.clone();
                let deadline = prepare.expires_ms;
                if self
                    .revalidate(&selected, context, now_ms, deadline)
                    .is_ok()
                {
                    return Err(Error::Busy);
                }
            }
            State::Consumed(_) => return Err(Error::TooLate),
            State::Idle | State::Aborted(_) => {}
        }
        let (ready, host) = self.inputs(context, now_ms)?;
        // Preserve the prior abort decision until the app has released its
        // fence. Otherwise a lost abort reply could strand the old handoff.
        if host.fenced {
            return Err(Error::Busy);
        }
        let expires_ms = now_ms.checked_add(OFFER_MS).ok_or(Error::Expired)?;
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Expired)?;
        let offer = Offer {
            schema: SCHEMA,
            ticket: Ticket {
                service: self.epoch.clone(),
                sequence: self.sequence,
                candidate: ready.candidate.id.clone(),
            },
            candidate: ready.candidate.clone(),
            ends_game: host.game,
            expires_ms,
        };
        self.state = State::Offered(Selection {
            offer: offer.clone(),
            ready: ready.clone(),
            host: host.clone(),
        });
        Ok(offer)
    }

    fn selected(&self, ticket: &Ticket) -> Result<(Selection, Option<Prepare>), Error> {
        match &self.state {
            State::Offered(s) if &s.offer.ticket == ticket => Ok((s.clone(), None)),
            State::Preparing(s, p) if &s.offer.ticket == ticket => Ok((s.clone(), Some(p.clone()))),
            State::Consumed(t) if t == ticket => Err(Error::TooLate),
            _ => Err(Error::Stale),
        }
    }

    fn revalidate(
        &mut self,
        selected: &Selection,
        context: &Context,
        now_ms: u64,
        deadline_ms: u64,
    ) -> Result<(), Error> {
        let result = (|| {
            let (ready, host) = self.inputs(context, now_ms)?;
            if now_ms >= deadline_ms {
                return Err(Error::Expired);
            }
            if matches!(self.state, State::Offered(_)) && host.fenced {
                return Err(Error::Busy);
            }
            if ready != &selected.ready
                || host.instance != selected.host.instance
                || host.revision != selected.host.revision
                || host.settings_revision != selected.host.settings_revision
                || host.game != selected.host.game
            {
                return Err(Error::Stale);
            }
            Ok(())
        })();
        if result.is_err() {
            // Retain the abort decision for a lost reply; restored conditions
            // can never revive this ticket. A new offer needs fresh consent.
            self.state = State::Aborted(selected.offer.ticket.clone());
        }
        result
    }

    pub fn confirm(
        &mut self,
        consent: &Consent,
        context: &Context,
        now_ms: u64,
    ) -> Result<Prepare, Error> {
        consent.validate()?;
        let (selected, pending) = self.selected(&consent.ticket)?;
        let deadline = pending
            .as_ref()
            .map_or(selected.offer.expires_ms, |p| p.expires_ms);
        self.revalidate(&selected, context, now_ms, deadline)?;
        if selected.host.game && !consent.end_game {
            return Err(Error::EndGameRequired);
        }
        if let Some(pending) = pending {
            return Ok(pending);
        }
        let prepare = Prepare {
            schema: SCHEMA,
            ticket: consent.ticket.clone(),
            host: selected.host.instance.clone(),
            host_revision: selected.host.revision,
            settings_revision: selected.host.settings_revision,
            expires_ms: now_ms
                .checked_add(PREPARE_MS)
                .ok_or(Error::Expired)?
                .min(selected.offer.expires_ms),
        };
        self.state = State::Preparing(selected, prepare.clone());
        Ok(prepare)
    }

    /// Call under the same serialization that prevents channel/host/maintenance
    /// changes. peer is OS-authenticated kiosk identity, not ack.host copied out
    /// of JSON. Revalidate trust, payload, recovery and locks separately.
    pub fn consume(
        &mut self,
        ack: &Acknowledgement,
        peer: &HostInstance,
        context: &Context,
        now_ms: u64,
    ) -> Result<ManualAuthorization, Error> {
        ack.validate()?;
        let (selected, pending) = self.selected(&ack.ticket)?;
        let pending = pending.ok_or(Error::NotReady)?;
        self.revalidate(&selected, context, now_ms, pending.expires_ms)?;
        if peer != &pending.host
            || ack.host != pending.host
            || ack.host_revision != pending.host_revision
            || ack.settings_revision != pending.settings_revision
        {
            return Err(Error::Stale);
        }
        self.state = State::Consumed(ack.ticket.clone());
        Ok(ManualAuthorization {
            ticket: ack.ticket.clone(),
            selection_generation: selected.ready.selection_generation,
        })
    }

    pub fn cancel(&mut self, ticket: &Ticket) -> Result<Aborted, Error> {
        match &self.state {
            State::Consumed(t) if t == ticket => return Err(Error::TooLate),
            State::Aborted(t) if t == ticket => {}
            _ => {
                self.selected(ticket)?;
            }
        }
        self.state = State::Aborted(ticket.clone());
        Ok(Aborted {
            schema: SCHEMA,
            ticket: ticket.clone(),
        })
    }
}

#[cfg(test)]
#[path = "install_handoff_tests.rs"]
mod tests;
