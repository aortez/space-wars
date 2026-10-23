//! Bounded SSH protection and diagnostic snapshots shared with socket workers.
//! No lock in this module is held across filesystem, bus or socket operations.
use crate::network_controller::{Deferred, Status};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const HANDOFF: Duration = Duration::from_secs(10);

#[derive(Clone, Default)]
pub(crate) struct Access(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    initialized: bool,
    radio_blocked: bool,
    failure: Option<Deferred>,
    busy: bool,
    protected_until: Option<Instant>,
    admission_epoch: u64,
    status: Option<(Status, Instant)>,
}

impl Access {
    pub fn initialize(&self, radio_blocked: bool) {
        let mut state = self.0.lock().expect("network access mutex");
        state.initialized = true;
        state.radio_blocked = radio_blocked;
    }

    pub fn begin(&self) {
        self.0.lock().expect("network access mutex").busy = true;
    }

    pub fn finish(&self, status: Option<Status>, radio_blocked: bool) {
        let mut state = self.0.lock().expect("network access mutex");
        state.busy = false;
        // A failed operation cannot clear the fence installed before a radio
        // command. Only a completed observation of durable state can do so.
        if let Some(status) = status {
            state.radio_blocked = radio_blocked;
            state.status = Some((status, Instant::now()));
        }
    }

    pub fn fail(&self) {
        self.0.lock().expect("network access mutex").failure = Some(Deferred::StorageUnavailable);
    }

    pub fn stop(&self) {
        self.0.lock().expect("network access mutex").failure =
            Some(Deferred::CoordinatorUnavailable);
    }

    pub fn busy(&self) -> bool {
        self.0.lock().expect("network access mutex").busy
    }

    /// The accepted TCP connection and activating sshd unit are already visible
    /// to the OS observer. One short bridge protects that handoff; it allocates
    /// no per-connection lease and cannot exhaust the general hold table.
    pub fn protect_ssh(&self) -> bool {
        let mut state = self.0.lock().expect("network access mutex");
        if !state.initialized || state.failure.is_some() {
            return false;
        }
        let Some(epoch) = state.admission_epoch.checked_add(1) else {
            state.failure = Some(Deferred::ProtectionUnavailable);
            return false;
        };
        state.admission_epoch = epoch;
        state.protected_until = Some(Instant::now() + HANDOFF);
        !state.radio_blocked
    }

    pub fn protected(&self) -> bool {
        self.0
            .lock()
            .expect("network access mutex")
            .protected_until
            .is_some_and(|end| Instant::now() < end)
    }

    pub fn epoch(&self) -> u64 {
        self.0.lock().expect("network access mutex").admission_epoch
    }

    /// Serialize admission with the decision to prepare a radio write. A hold
    /// admitted before this fence prevents shutdown; later callers wait for
    /// durable reconciliation. This also fences saves before Set is submitted.
    pub fn begin_radio(&self, enabled: bool, observed_epoch: u64) -> bool {
        let mut state = self.0.lock().expect("network access mutex");
        if state.failure.is_some()
            || (!enabled
                && (state.admission_epoch != observed_epoch
                    || state
                        .protected_until
                        .is_some_and(|end| Instant::now() < end)))
        {
            return false;
        }
        state.radio_blocked = true;
        true
    }

    pub fn status(&self) -> Option<Status> {
        let state = self.0.lock().expect("network access mutex");
        let (mut status, published) = state.status.clone()?;
        let age = published.elapsed();
        // Keep the original observation clock and saved generation. A cached
        // diagnostic must never become a fresh download/radio authorization.
        if state.failure.is_some() || state.busy || age > Duration::from_secs(5) {
            status.actual = None;
            status.activity = None;
            status.deferred = Some(if let Some(failure) = state.failure {
                failure
            } else if state.busy {
                Deferred::CoordinatorBusy
            } else {
                Deferred::ObservationUnavailable
            });
            status.policy.intent = crate::network_policy::Intent::ObserveActivity;
        }
        let elapsed = u64::try_from(age.as_millis()).unwrap_or(u64::MAX);
        for hold in &mut status.policy.holds {
            hold.expires_in_ms = hold.expires_in_ms.saturating_sub(elapsed);
        }
        Some(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_and_shutdown_are_mutually_exclusive_in_both_orders() {
        let access = Access::default();
        assert!(!access.protect_ssh());
        access.initialize(false);
        assert!(access.protect_ssh());
        assert!(!access.begin_radio(false, access.epoch()));
        // The observer takes over live connections. Once the bridge has ended,
        // the ordinary observed-idle and quiet-grace checks may allow shutdown.
        access.0.lock().unwrap().protected_until = Some(Instant::now());
        assert!(access.begin_radio(false, access.epoch()));
        assert!(!access.protect_ssh());
        assert!(access.protected()); // requests restoration, never a second Set
        access.finish(None, false);
        assert!(!access.protect_ssh()); // failed work cannot clear the fence
    }

    #[test]
    fn admission_during_a_slow_observation_invalidates_it_even_after_the_bridge_expires() {
        let access = Access::default();
        access.initialize(false);
        let observed = access.epoch();
        assert!(access.protect_ssh());
        access.0.lock().unwrap().protected_until = Some(Instant::now());
        assert!(!access.begin_radio(false, observed));
        assert!(access.begin_radio(false, access.epoch()));
    }

    #[test]
    fn repeated_connections_use_one_bridge_and_storage_failure_is_latched() {
        let access = Access::default();
        access.initialize(false);
        for _ in 0..10_000 {
            assert!(access.protect_ssh());
        }
        access.fail();
        assert!(!access.protect_ssh());
        assert!(!access.begin_radio(true, access.epoch()));
        assert!(!access.begin_radio(false, access.epoch()));
    }
}
