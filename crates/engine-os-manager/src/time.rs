use serde::{Deserialize, Serialize};

const SECOND: u64 = 1_000_000;
const OVERDUE_US: u64 = 48 * 60 * 60 * SECOND;
const CLOCK_TOLERANCE_US: u64 = 2 * SECOND;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderIdentity {
    pub bus_owner: String,
    pub invocation_id: String,
    pub pid: u32,
    pub uid: u32,
}

/// The provider records these clocks only after clock_adjtime succeeds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockReceipt {
    pub packet_count: u64,
    pub realtime_us: u64,
    pub monotonic_us: u64,
    pub boottime_us: u64,
}

impl ClockReceipt {
    pub fn matches_running_clock(&self, now: ClockReading) -> bool {
        self.monotonic_us > 0
            && self.monotonic_us <= now.monotonic_us
            && now
                .boottime_us
                .checked_sub(self.boottime_us)
                .is_some_and(|age| {
                    self.realtime_us.checked_add(age).is_some_and(|expected| {
                        expected.abs_diff(now.realtime_us) <= CLOCK_TOLERANCE_US
                    })
                })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockReading {
    pub realtime_us: u64,
    pub monotonic_us: u64,
    pub boottime_us: u64,
}

/// One coherent provider read. Link state, checkpoint files, and timedated's
/// sticky NTPSynchronized property deliberately do not appear in this contract.
#[derive(Clone, Debug, Serialize)]
pub struct SyncObservation {
    pub provider: ProviderIdentity,
    pub receipt: ClockReceipt,
    pub ntp_packet_count: u64,
    pub ntp_spike: bool,
    pub ntp_valid: bool,
    pub notification_realtime_us: Option<u64>,
    pub kernel_synchronized: bool,
    pub now: ClockReading,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmedSync {
    pub boot_id: String,
    pub provider: ProviderIdentity,
    pub receipt: ClockReceipt,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RtcAssociation {
    pub driver: String,
    pub initialized_utc_us: u64,
    pub sync_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeRecord {
    pub schema: u32,
    pub success_generation: u64,
    pub last_sync: Option<ConfirmedSync>,
    pub rtc: Option<RtcAssociation>,
    pub invalidated_boot_id: Option<String>,
    pub rtc_accepted_boot_id: Option<String>,
}

impl Default for TimeRecord {
    fn default() -> Self {
        Self {
            schema: 1,
            success_generation: 0,
            last_sync: None,
            rtc: None,
            invalidated_boot_id: None,
            rtc_accepted_boot_id: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimeSource {
    NetworkSync,
    RetainedRtc,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimeReason {
    WaitingForNetworkSync,
    MissingState,
    CorruptState,
    RtcAbsent,
    RtcUninitialized,
    RtcUnreadable,
    RtcResetOrBackwards,
    RtcRetentionUnqualified,
    UnconfirmedClockChange,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncAge {
    Known { seconds: u64, overdue: bool },
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeStatus {
    pub usable: bool,
    pub source: Option<TimeSource>,
    pub reason: Option<TimeReason>,
    pub utc_us: Option<u64>,
    pub last_confirmed_sync_utc_us: Option<u64>,
    pub sync_age: SyncAge,
    pub success_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RtcBootReading {
    pub boot_id: String,
    pub driver: Option<String>,
    pub utc_us: Option<u64>,
    pub boottime_us: u64,
    /// Only the platform qualification adapter may establish this. A plausible
    /// date, battery voltage, or hctosys flag is insufficient.
    pub retention_established: bool,
}

#[derive(Clone)]
pub struct TimeTracker {
    boot_id: String,
    record: TimeRecord,
    source: Option<TimeSource>,
    reason: Option<TimeReason>,
    previous_clock: Option<ClockReading>,
}

impl TimeTracker {
    pub fn new(
        boot_id: String,
        record: TimeRecord,
        initial_reason: TimeReason,
        now: ClockReading,
        rtc_boot: Option<&RtcBootReading>,
    ) -> Self {
        let invalidated = record.invalidated_boot_id.as_deref() == Some(boot_id.as_str());
        let same_boot = !invalidated
            && record.last_sync.as_ref().is_some_and(|sync| {
                sync.boot_id == boot_id && sync.receipt.boottime_us <= now.boottime_us
            });
        let rtc_same_boot =
            !invalidated && record.rtc_accepted_boot_id.as_deref() == Some(boot_id.as_str());
        let mut tracker = Self {
            boot_id,
            record,
            source: same_boot.then_some(TimeSource::NetworkSync),
            reason: (!same_boot).then_some(initial_reason),
            previous_clock: Some(now),
        };
        if invalidated {
            tracker.reason = Some(TimeReason::UnconfirmedClockChange);
        } else if !same_boot && rtc_same_boot {
            tracker.source = Some(TimeSource::RetainedRtc);
            tracker.reason = None;
        } else if !same_boot && let Some(reading) = rtc_boot {
            tracker.evaluate_rtc(reading, now);
        }
        tracker
    }

    pub fn record(&self) -> &TimeRecord {
        &self.record
    }

    fn evaluate_rtc(&mut self, reading: &RtcBootReading, now: ClockReading) {
        let reason = match (&reading.driver, reading.utc_us, &self.record.rtc) {
            (None, _, _) => Some(TimeReason::RtcAbsent),
            (_, None, _) => Some(TimeReason::RtcUnreadable),
            (_, _, None) => Some(TimeReason::RtcUninitialized),
            (Some(driver), Some(utc), Some(saved)) => {
                if reading.boot_id != self.boot_id
                    || reading.boottime_us > now.boottime_us
                    || saved.driver != *driver
                    || utc < saved.initialized_utc_us
                {
                    Some(TimeReason::RtcResetOrBackwards)
                } else if !reading.retention_established {
                    Some(TimeReason::RtcRetentionUnqualified)
                } else if now
                    .realtime_us
                    .abs_diff(utc.saturating_add(now.boottime_us - reading.boottime_us))
                    > CLOCK_TOLERANCE_US
                {
                    Some(TimeReason::UnconfirmedClockChange)
                } else {
                    None
                }
            }
        };
        if reason.is_none() {
            self.source = Some(TimeSource::RetainedRtc);
            self.record.rtc_accepted_boot_id = Some(self.boot_id.clone());
        }
        self.reason = reason;
    }

    /// Returns true only for a newly confirmed receipt. Age is anchored to the
    /// provider's boot clock, never the polling/restart time or wall-clock delta.
    pub fn observe(&mut self, observation: &SyncObservation) -> bool {
        let receipt = observation.receipt;
        let previously_seen = self.record.last_sync.as_ref().is_some_and(|sync| {
            sync.boot_id == self.boot_id
                && sync.provider == observation.provider
                && sync.receipt.packet_count >= receipt.packet_count
        });
        let valid = !previously_seen
            && receipt.packet_count > 0
            && observation.ntp_packet_count == receipt.packet_count
            && !observation.ntp_spike
            && observation.ntp_valid
            && observation.kernel_synchronized
            && receipt.matches_running_clock(observation.now)
            && observation.notification_realtime_us.is_some_and(|mtime| {
                // Inode timestamps use the kernel's coarse realtime clock, so
                // a marker touched after correction can predate the precise
                // receipt. Correlate within the same bounded clock tolerance.
                mtime.abs_diff(receipt.realtime_us) <= CLOCK_TOLERANCE_US
            });
        if !valid {
            self.tick(observation.now);
            return false;
        }
        let Some(generation) = self.record.success_generation.checked_add(1) else {
            self.tick(observation.now);
            return false;
        };
        self.record.success_generation = generation;
        self.record.invalidated_boot_id = None;
        self.record.last_sync = Some(ConfirmedSync {
            boot_id: self.boot_id.clone(),
            provider: observation.provider.clone(),
            receipt,
        });
        self.source = Some(TimeSource::NetworkSync);
        self.reason = None;
        self.previous_clock = Some(observation.now);
        true
    }

    /// Missing connectivity/provider data does not revoke a usable running clock.
    /// An unexplained clock step does; a verified receipt can re-establish it.
    pub fn tick(&mut self, now: ClockReading) {
        if let Some(previous) = self.previous_clock {
            let continuous = now
                .boottime_us
                .checked_sub(previous.boottime_us)
                .is_some_and(|age| {
                    previous
                        .realtime_us
                        .checked_add(age)
                        .is_some_and(|expected| {
                            expected.abs_diff(now.realtime_us) <= CLOCK_TOLERANCE_US
                        })
                });
            if !continuous {
                self.source = None;
                self.reason = Some(TimeReason::UnconfirmedClockChange);
                self.record.invalidated_boot_id = Some(self.boot_id.clone());
            }
        }
        self.previous_clock = Some(now);
    }

    /// An RTC failure never retracts the successful network observation. Only
    /// a checked write/readback may replace the next-boot association.
    pub fn record_rtc_write(&mut self, driver: String, utc_us: u64) {
        if self.source == Some(TimeSource::NetworkSync) {
            self.record.rtc = Some(RtcAssociation {
                driver,
                initialized_utc_us: utc_us,
                sync_generation: self.record.success_generation,
            });
        }
    }

    pub fn invalidate_rtc(&mut self) {
        self.record.rtc = None;
    }

    pub fn status(&self, now: ClockReading) -> TimeStatus {
        let sync = self.record.last_sync.as_ref();
        let age = sync
            .filter(|sync| sync.boot_id == self.boot_id)
            .and_then(|sync| now.boottime_us.checked_sub(sync.receipt.boottime_us));
        TimeStatus {
            usable: self.source.is_some(),
            source: self.source.clone(),
            reason: self.reason.clone(),
            utc_us: self.source.as_ref().map(|_| now.realtime_us),
            last_confirmed_sync_utc_us: sync.map(|sync| sync.receipt.realtime_us),
            sync_age: age.map_or(SyncAge::Unknown, |age| SyncAge::Known {
                seconds: age / SECOND,
                overdue: age > OVERDUE_US,
            }),
            success_generation: self.record.success_generation,
        }
    }
}
