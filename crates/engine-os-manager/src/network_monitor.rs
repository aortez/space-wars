//! Continuous radio evidence on a retained D-Bus connection. Snapshot equality
//! cannot erase a radio change that was reversed between coordinator polls.
use crate::network_controller::{Suspension, WriteResult};
use crate::network_observation::{Observation, bounded, bus_error, connection_observation};
use crate::{Error, Result};
use futures_lite::StreamExt;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zbus::zvariant::OwnedValue;
use zbus::{Connection, MatchRule, Message, MessageStream};

const NAME: &str = "org.freedesktop.NetworkManager";
const OBJECT: &str = "/org/freedesktop/NetworkManager";
const DEADLINE: Duration = Duration::from_secs(5);
const QUEUE: usize = 128;

struct Expected {
    enabled: bool,
    until: Instant,
    /// The matching transition was drained; readback may still trail it.
    signalled: bool,
}

pub struct Monitor {
    connection: Connection,
    properties: MessageStream,
    owners: MessageStream,
    owner: String,
    uid: u32,
    change: Option<Suspension>,
    expected: Option<Expected>,
    wireless_enabled: bool,
}

impl Monitor {
    pub fn system() -> Result<Self> {
        futures_lite::future::block_on(bounded(DEADLINE, async {
            let connection = zbus::connection::Builder::system()
                .map_err(bus_error)?
                .method_timeout(DEADLINE)
                .build()
                .await
                .map_err(bus_error)?;
            Self::connect(connection, 0).await
        }))
    }

    pub(crate) async fn connect(connection: Connection, uid: u32) -> Result<Self> {
        let owner_rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")
            .map_err(bus_error)?
            .path("/org/freedesktop/DBus")
            .map_err(bus_error)?
            .interface("org.freedesktop.DBus")
            .map_err(bus_error)?
            .member("NameOwnerChanged")
            .map_err(bus_error)?
            .arg(0, NAME)
            .map_err(bus_error)?
            .build();
        let owners = MessageStream::for_match_rule(owner_rule, &connection, Some(QUEUE))
            .await
            .map_err(bus_error)?;
        let bus = zbus::fdo::DBusProxy::new(&connection)
            .await
            .map_err(bus_error)?;
        let owner = bus
            .get_name_owner(NAME.try_into().map_err(bus_error)?)
            .await
            .map_err(bus_error)?
            .to_string();
        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(owner.as_str())
            .map_err(bus_error)?
            .path(OBJECT)
            .map_err(bus_error)?
            .interface("org.freedesktop.DBus.Properties")
            .map_err(bus_error)?
            .member("PropertiesChanged")
            .map_err(bus_error)?
            .build();
        let properties = MessageStream::for_match_rule(rule, &connection, Some(QUEUE))
            .await
            .map_err(bus_error)?;
        // Subscribe before obtaining a baseline. The observation validates the
        // owner's UID/PID and supplies a reply fence for earlier queued signals.
        let baseline = connection_observation(&connection, uid).await?;
        if baseline.bus_owner != owner || baseline.startup {
            return Err(unavailable());
        }
        let mut monitor = Self {
            connection,
            properties,
            owners,
            owner,
            uid,
            change: None,
            expected: None,
            wireless_enabled: baseline.wireless_enabled,
        };
        monitor.drain().await;
        Ok(monitor)
    }

    pub fn snapshot(&mut self) -> Result<(Observation, Option<Suspension>)> {
        futures_lite::future::block_on(bounded(DEADLINE, self.observe()))
    }

    pub(crate) async fn observe(&mut self) -> Result<(Observation, Option<Suspension>)> {
        // Drain before requesting a reply so a full signal queue cannot block
        // the connection's single reader while it delivers that reply.
        self.drain().await;
        let observation = connection_observation(&self.connection, self.uid).await?;
        self.drain().await;
        if observation.bus_owner != self.owner {
            self.latch(Suspension::ManagerChanged);
        }
        self.expire();
        // The transition and its readback now agree, so the write is complete.
        if self
            .expected
            .as_ref()
            .is_some_and(|write| write.signalled && write.enabled == observation.wireless_enabled)
        {
            self.expected = None;
        }
        Ok((observation, self.change))
    }

    /// The prepared target, if its original deadline was still live at the
    /// latest observation. Reading it never renews that deadline, and a new
    /// monitor has none.
    pub fn expected(&self) -> Option<bool> {
        self.expected.as_ref().map(|write| write.enabled)
    }

    /// Reserve exactly one expected radio transition after a fresh fence.
    /// No caller may use this to dismiss prior or intervening external changes.
    pub fn prepare_write(&mut self, expected: &Observation, enabled: bool) -> Result<bool> {
        futures_lite::future::block_on(bounded(DEADLINE, self.prepare(expected, enabled)))
    }

    pub(crate) async fn prepare(&mut self, expected: &Observation, enabled: bool) -> Result<bool> {
        let (actual, change) = self.observe().await?;
        if change.is_some()
            || self.expected.is_some()
            || actual != *expected
            || actual.wireless_enabled == enabled
        {
            return Ok(false);
        }
        self.expected = Some(Expected {
            enabled,
            until: Instant::now() + DEADLINE,
            signalled: false,
        });
        Ok(true)
    }

    pub fn finish_write(&mut self, result: &Result<WriteResult>) {
        if !matches!(result, Ok(WriteResult::Returned)) {
            self.expected = None;
        }
    }

    fn latch(&mut self, change: Suspension) {
        self.change.get_or_insert(change);
    }

    /// Signals carry no arrival time, so the deadline is checked when one is
    /// drained. A notification queued before it but drained after it is late.
    fn expire(&mut self) {
        let Some(write) = &self.expected else {
            return;
        };
        if Instant::now() >= write.until {
            let signalled = write.signalled;
            self.expected = None;
            if !signalled {
                self.latch(Suspension::ObservationLost);
            }
        }
    }

    /// Moves a live deadline to now, without draining queued signals.
    #[cfg(test)]
    pub(crate) fn reach_deadline(&mut self) {
        if let Some(write) = &mut self.expected {
            write.until = Instant::now();
        }
    }

    async fn drain(&mut self) {
        for owners in [true, false] {
            for index in 0..=QUEUE {
                let stream = if owners {
                    &mut self.owners
                } else {
                    &mut self.properties
                };
                let Some(next) = futures_lite::future::poll_once(stream.next()).await else {
                    break;
                };
                let Some(Ok(message)) = next else {
                    self.latch(Suspension::ObservationLost);
                    break;
                };
                if index == QUEUE || message.data().len() > 128 * 1024 {
                    self.latch(Suspension::ObservationLost);
                    break;
                }
                let valid = if owners {
                    self.owner_changed(&message)
                } else {
                    self.properties_changed(&message)
                };
                if valid.is_err() {
                    self.latch(Suspension::ObservationLost);
                }
            }
        }
    }

    fn owner_changed(&mut self, message: &Message) -> Result<()> {
        let (name, old, new): (String, String, String) =
            message.body().deserialize().map_err(bus_error)?;
        if name != NAME || old.len() > 255 || new.len() > 255 {
            return Err(unavailable());
        }
        self.latch(Suspension::ManagerChanged);
        Ok(())
    }

    fn properties_changed(&mut self, message: &Message) -> Result<()> {
        let (interface, fields, invalidated): (String, HashMap<String, OwnedValue>, Vec<String>) =
            message.body().deserialize().map_err(bus_error)?;
        if interface.len() > 255 || fields.len() > 128 || invalidated.len() > 128 {
            return Err(unavailable());
        }
        if interface != NAME {
            return Ok(());
        }
        for name in [
            "WirelessEnabled",
            "NetworkingEnabled",
            "WirelessHardwareEnabled",
        ] {
            if invalidated.iter().any(|key| key == name) {
                return Err(unavailable());
            }
            let Some(value) = fields.get(name) else {
                continue;
            };
            let value: bool = value
                .try_clone()
                .map_err(bus_error)?
                .try_into()
                .map_err(bus_error)?;
            if name == "WirelessEnabled" {
                // NM can notify with the old exported value before its cached
                // rfkill bit catches up. Only a transition consumes our Set's
                // allowance; duplicates neither renew it nor erase a fault.
                if value == self.wireless_enabled {
                    continue;
                }
                self.wireless_enabled = value;
                self.expire();
                if let Some(write) = self
                    .expected
                    .as_mut()
                    .filter(|write| !write.signalled && write.enabled == value)
                {
                    // Keep the original deadline until readback agrees.
                    write.signalled = true;
                    continue;
                }
            }
            self.latch(Suspension::ExternalRadioChange);
        }
        Ok(())
    }
}

fn unavailable() -> Error {
    Error::Invalid("radio change monitor is unavailable".into())
}
