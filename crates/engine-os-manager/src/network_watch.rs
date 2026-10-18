//! In-process read-only observations from the serialized network coordinator.
//! Reading this cache never opens a bus/socket, modifies policy or wakes Wi-Fi.
use crate::network_controller::Status;
use crate::network_policy::Clock;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct Watch(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    epoch: u64,
    status: Option<Status>,
}

#[derive(Clone)]
pub struct Snapshot {
    pub epoch: u64,
    pub status: Status,
}

impl Watch {
    pub fn read(&self, now: &Clock) -> Option<Snapshot> {
        let state = self.0.lock().ok()?;
        let status = state.status.as_ref()?;
        if status.observed.boot_id != now.boot_id
            || now
                .ms
                .checked_sub(status.observed.ms)
                .is_none_or(|age| age > 5_000)
        {
            return None;
        }
        Some(Snapshot {
            epoch: state.epoch,
            status: status.clone(),
        })
    }

    pub(crate) fn reset(&self) {
        let mut state = self.0.lock().expect("network watch mutex");
        state.epoch = state
            .epoch
            .checked_add(1)
            .expect("network watch generation exhausted");
        state.status = None;
    }

    pub(crate) fn publish(&self, status: Option<Status>) {
        self.0.lock().expect("network watch mutex").status = status;
    }
}
