//! Radio ownership is committed in the same record as policy and deployments.
use crate::network_observation::Observation;
use crate::network_policy::Clock;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Suspension {
    ExternalRadioChange,
    ProfilesChanged,
    ManagerChanged,
    CommandRejected,
    CommandUncertain,
    ReadbackMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Witness {
    pub boot_id: String,
    pub owner: String,
    pub pid: u32,
    pub settings_version: u64,
    pub networking: bool,
    pub wireless: bool,
}

impl Witness {
    pub fn new(clock: &Clock, radio: &Observation) -> Self {
        Self {
            boot_id: clock.boot_id.clone(),
            owner: radio.bus_owner.clone(),
            pid: radio.pid,
            settings_version: radio.settings_version,
            networking: radio.networking_enabled,
            wireless: radio.wireless_enabled,
        }
    }

    fn validate(&self) -> bool {
        Clock {
            boot_id: self.boot_id.clone(),
            ms: 0,
        }
        .valid()
            && self.pid != 0
            && self.owner.len() <= 255
            && zbus::names::UniqueName::try_from(self.owner.as_str()).is_ok()
    }

    pub fn conflict(&self, observed: &Self) -> Option<Suspension> {
        if self.boot_id == observed.boot_id {
            if self.owner != observed.owner || self.pid != observed.pid {
                return Some(Suspension::ManagerChanged);
            }
            if self.settings_version != observed.settings_version {
                return Some(Suspension::ProfilesChanged);
            }
        }
        if self.networking != observed.networking || self.wireless != observed.wireless {
            return Some(Suspension::ExternalRadioChange);
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Phase {
    Requested,
    /// The method returned successfully; no request is still in flight.
    Returned,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pending {
    pub before: Witness,
    pub enabled: bool,
    pub phase: Phase,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RadioState {
    pub witness: Option<Witness>,
    pub owned_off: bool,
    pub pending: Option<Pending>,
    pub suspended: Option<Suspension>,
}

impl RadioState {
    pub fn validate(&self) -> Result<()> {
        let valid = self.witness.as_ref().is_none_or(Witness::validate)
            && (!self.owned_off || self.witness.as_ref().is_some_and(|w| !w.wireless))
            && self.pending.as_ref().is_none_or(|pending| {
                pending.before.validate()
                    && self.witness.as_ref() == Some(&pending.before)
                    && pending.before.wireless != pending.enabled
            });
        if !valid {
            return Err(Error::Invalid("invalid saved radio ownership".into()));
        }
        Ok(())
    }
}
