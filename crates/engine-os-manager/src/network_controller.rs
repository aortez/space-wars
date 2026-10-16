//! Serialized Wi-Fi control. One coordinator owns this value; requests queue
//! behind a radio command and are acknowledged only after a fresh readback.
use crate::network_observation::Observation;
use crate::network_policy::{
    self, Activity, Caller, Clock, HoldKind, HoldToken, Intent, NetworkPolicy, Policy,
};
use crate::network_radio_state::{Pending, Phase, RadioState, Witness};
use crate::{Error, Result};
use serde::Serialize;
use std::path::Path;

pub use crate::network_radio_state::Suspension;

pub struct Snapshot {
    pub clock: Clock,
    pub radio: Observation,
    pub activity: Activity,
}

/// A successful D-Bus reply fences the request, but still requires readback.
/// Rejected is only for a command known not to have been submitted or applied.
/// Transport errors/timeouts after dispatch must return Err, never Rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteResult {
    Returned,
    Rejected,
    /// Transport/preflight failed before dispatch. A later tick may retry.
    NotSubmitted,
}

/// OS adapters own clocks, observations and the admission/install locks. None
/// of these values may be filled from an untrusted client's request.
pub trait Backend {
    /// Held through the final snapshot, radio write and readback. This excludes
    /// new SSH admissions and installs, while snapshot accounts for live work.
    type ShutdownGuard;
    fn clock(&mut self) -> Result<Clock>;
    fn snapshot(&mut self) -> Result<Snapshot>;
    /// None means protection is busy or incomplete; it never permits shutdown.
    fn shutdown_guard(&mut self) -> Result<Option<Self::ShutdownGuard>>;
    /// Must target only WirelessEnabled on the observed unique bus owner, with
    /// a bounded deadline and a last comparison against expected before sending.
    fn set_wireless(&mut self, expected: &Observation, enabled: bool) -> Result<WriteResult>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Deferred {
    ObservationUnavailable,
    ProtectionUnavailable,
    RadioUnavailable,
    Suspended,
    CommandUncertain,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Transition {
    pub target_enabled: bool,
    pub method_returned: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Status {
    pub policy: network_policy::Status,
    pub actual: Option<Observation>,
    pub owned_off: bool,
    pub pending: Option<Transition>,
    pub suspended: Option<Suspension>,
    pub deferred: Option<Deferred>,
}

/// Availability permits an operation to start. Neither value asserts Internet
/// access or a successful association with a particular access point.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Availability {
    WirelessReady,
    EthernetReady,
}

#[derive(Debug, Serialize)]
pub struct Admission {
    pub availability: Availability,
    pub status: Status,
}

pub struct Controller<B: Backend> {
    policy: NetworkPolicy,
    backend: B,
    actual: Option<Observation>,
    deferred: Option<Deferred>,
}

impl<B: Backend> Controller<B> {
    pub fn open(directory: &Path, mut backend: B) -> Result<Self> {
        let policy = NetworkPolicy::open(directory, backend.clock()?)?;
        Ok(Self {
            policy,
            backend,
            actual: None,
            deferred: None,
        })
    }

    /// Refresh actual state without requesting a radio change.
    pub fn status(&mut self) -> Result<Status> {
        self.refresh()?;
        self.report()
    }

    pub fn set_policy(&mut self, generation: u64, policy: Policy) -> Result<Status> {
        let clock = self.backend.clock()?;
        self.policy.set_policy(generation, policy, &clock)?;
        self.tick()
    }

    pub fn admit(
        &mut self,
        token: &HoldToken,
        caller: Caller,
        kind: HoldKind,
    ) -> Result<Admission> {
        let clock = self.backend.clock()?;
        self.policy.reserve(token, caller, kind, &clock)?;
        self.admission(token, caller)
    }

    pub fn renew(&mut self, token: &HoldToken, caller: Caller) -> Result<Admission> {
        let clock = self.backend.clock()?;
        self.policy.renew(token, caller, &clock)?;
        self.admission(token, caller)
    }

    fn admission(&mut self, token: &HoldToken, caller: Caller) -> Result<Admission> {
        let status = self.tick()?;
        let clock = self.backend.clock()?;
        let kind = self.policy.held_kind(token, caller, &clock)?;
        if status.pending.is_some() || status.suspended.is_some() || status.deferred.is_some() {
            return Err(invalid(
                "network hold reserved but availability is not confirmed",
            ));
        }
        let radio = status
            .actual
            .as_ref()
            .ok_or_else(|| invalid("network state unavailable"))?;
        let availability = if any_network(kind) && radio.ethernet_available {
            Availability::EthernetReady
        } else if radio.networking_enabled
            && radio.wireless_enabled
            && radio.wireless_hardware_enabled
            && !radio.startup
        {
            Availability::WirelessReady
        } else {
            return Err(invalid(
                "network hold reserved but the required radio is unavailable",
            ));
        };
        Ok(Admission {
            availability,
            status,
        })
    }

    pub fn release(&mut self, token: &HoldToken, caller: Caller) -> Result<Status> {
        let clock = self.backend.clock()?;
        self.policy.release(token, caller, &clock)?;
        self.tick()
    }

    /// Explicitly accept a new baseline after an external change. An ambiguous
    /// old request cannot be dismissed in the same boot: a late Set could still
    /// disconnect a newly admitted caller. A reboot fences that old request.
    pub fn resume(&mut self, generation: u64) -> Result<Status> {
        let clock = self.backend.clock()?;
        if self.policy.status(&clock)?.generation != generation {
            return Err(invalid("stale network policy generation"));
        }
        let Some(snapshot) = self.snapshot()? else {
            return self.report();
        };
        let mut state = self.policy.radio(&snapshot.clock)?;
        if state.pending.as_ref().is_some_and(|pending| {
            pending.phase == Phase::Requested && pending.before.boot_id == snapshot.clock.boot_id
        }) {
            return Err(invalid(
                "uncertain radio command requires a new boot before reconciliation",
            ));
        }
        // Keep responsibility for a policy-owned off state. Otherwise accepting
        // an externally disabled radio does not silently make it our off state.
        state.owned_off = !snapshot.radio.wireless_enabled
            && (state.owned_off || state.pending.as_ref().is_some_and(|p| !p.enabled));
        state.witness = Some(Witness::new(&snapshot.clock, &snapshot.radio));
        state.pending = None;
        state.suspended = None;
        self.policy.save_radio(state, &snapshot.clock)?;
        self.policy.reset_quiet();
        self.tick()
    }

    pub fn tick(&mut self) -> Result<Status> {
        let Some(mut snapshot) = self.refresh()? else {
            return self.report();
        };
        let Some(enabled) = self.desired(&snapshot)? else {
            return self.report();
        };
        if enabled == snapshot.radio.wireless_enabled {
            return self.report();
        }
        if !snapshot.radio.networking_enabled
            || !snapshot.radio.wireless_hardware_enabled
            || snapshot.radio.startup
            || snapshot.radio.checkpoint_count != 0
        {
            self.deferred = Some(Deferred::RadioUnavailable);
            return self.report();
        }
        // The guard stays in scope until readback or uncertainty is recorded.
        let _guard = if enabled {
            None
        } else {
            match self.backend.shutdown_guard() {
                Ok(Some(guard)) => Some(guard),
                Ok(None) | Err(_) => {
                    self.deferred = Some(Deferred::ProtectionUnavailable);
                    return self.report();
                }
            }
        };
        // Observations before obtaining protection cannot authorize this write.
        let Some(fresh) = self.refresh()? else {
            return self.report();
        };
        snapshot = fresh;
        if self.desired(&snapshot)? != Some(enabled)
            || snapshot.radio.wireless_enabled == enabled
            || !snapshot.radio.networking_enabled
            || !snapshot.radio.wireless_hardware_enabled
            || snapshot.radio.startup
            || snapshot.radio.checkpoint_count != 0
        {
            return self.report();
        }
        let mut state = self.policy.radio(&snapshot.clock)?;
        state.pending = Some(Pending {
            before: Witness::new(&snapshot.clock, &snapshot.radio),
            enabled,
            phase: Phase::Requested,
        });
        self.policy.save_radio(state.clone(), &snapshot.clock)?;
        let outcome = self.backend.set_wireless(&snapshot.radio, enabled);
        let clock = self.backend.clock()?;
        match outcome {
            Ok(WriteResult::Returned) => {
                state.pending.as_mut().expect("saved transition").phase = Phase::Returned;
                self.policy.save_radio(state, &clock)?;
                self.refresh()?;
            }
            Ok(WriteResult::Rejected) => {
                state.pending = None;
                state.suspended = Some(Suspension::CommandRejected);
                self.policy.save_radio(state, &clock)?;
                self.actual = None;
                self.deferred = Some(Deferred::Suspended);
            }
            Ok(WriteResult::NotSubmitted) => {
                state.pending = None;
                self.policy.save_radio(state, &clock)?;
                self.actual = None;
                self.deferred = Some(Deferred::ObservationUnavailable);
                self.policy.reset_quiet();
            }
            Err(_) => {
                state.suspended = Some(Suspension::CommandUncertain);
                self.policy.save_radio(state, &clock)?;
                self.actual = None;
                self.deferred = Some(Deferred::CommandUncertain);
            }
        }
        self.report()
    }

    fn snapshot(&mut self) -> Result<Option<Snapshot>> {
        self.actual = None;
        self.deferred = None;
        let mut snapshot = match self.backend.snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => {
                let clock = self.backend.clock()?;
                self.policy.observe(Activity::default(), &clock)?;
                self.deferred = Some(Deferred::ObservationUnavailable);
                return Ok(None);
            }
        };
        // Checkpoints always come from the same coherent NetworkManager read.
        snapshot.activity.checkpoints = Some(snapshot.radio.checkpoint_count != 0);
        self.policy.observe(snapshot.activity, &snapshot.clock)?;
        self.actual = Some(snapshot.radio.clone());
        Ok(Some(snapshot))
    }

    fn refresh(&mut self) -> Result<Option<Snapshot>> {
        let Some(snapshot) = self.snapshot()? else {
            return Ok(None);
        };
        let mut state = self.policy.radio(&snapshot.clock)?;
        let observed = Witness::new(&snapshot.clock, &snapshot.radio);
        if let Some(pending) = &state.pending {
            if pending.phase == Phase::Requested {
                state.suspended = Some(Suspension::CommandUncertain);
            } else if state.suspended.is_none() {
                let mut expected = pending.before.clone();
                expected.wireless = pending.enabled;
                // A reply followed by a daemon/reboot change is not attribution.
                if expected != observed {
                    state.suspended = Some(Suspension::ReadbackMismatch);
                } else {
                    state.owned_off = !pending.enabled;
                    state.witness = Some(observed);
                    state.pending = None;
                }
            }
        } else if state.suspended.is_none() {
            state.suspended = state
                .witness
                .as_ref()
                .and_then(|old| old.conflict(&observed));
            if state.suspended.is_none() {
                // NetworkManager's settings counter restarts on a new boot.
                // This is a baseline, not proof profiles were unchanged offline.
                state.witness = Some(observed);
            }
        }
        self.policy.save_radio(state.clone(), &snapshot.clock)?;
        self.deferred = if state.pending.is_some() {
            Some(Deferred::CommandUncertain)
        } else if state.suspended.is_some() {
            Some(Deferred::Suspended)
        } else {
            None
        };
        Ok(Some(snapshot))
    }

    fn desired(&mut self, snapshot: &Snapshot) -> Result<Option<bool>> {
        let state = self.policy.radio(&snapshot.clock)?;
        if state.pending.is_some() || state.suspended.is_some() {
            return Ok(None);
        }
        let status = self.policy.status(&snapshot.clock)?;
        if !status.policy.partial_offline {
            return Ok(state.owned_off.then_some(true));
        }
        if status.policy.stay_connected
            || status
                .holds
                .iter()
                .any(|hold| !any_network(hold.kind) || !snapshot.radio.ethernet_available)
        {
            return Ok(Some(true));
        }
        Ok((status.intent == Intent::MayDisable).then_some(false))
    }

    fn report(&mut self) -> Result<Status> {
        let clock = self.backend.clock()?;
        let state: RadioState = self.policy.radio(&clock)?;
        Ok(Status {
            policy: self.policy.status(&clock)?,
            actual: self.actual.clone(),
            owned_off: state.owned_off,
            pending: state.pending.map(|p| Transition {
                target_enabled: p.enabled,
                method_returned: p.phase == Phase::Returned,
            }),
            suspended: state.suspended,
            deferred: self.deferred,
        })
    }
}

fn any_network(kind: HoldKind) -> bool {
    matches!(kind, HoldKind::TimeCheck | HoldKind::Download)
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

#[cfg(test)]
#[path = "network_controller_tests.rs"]
mod tests;
