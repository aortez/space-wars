//! Durable time-check scheduling. The runner performs network/provider work only
//! after these transitions are committed, and releases only its own hold.
//! No wall-clock calendar value participates in deadlines or retry scheduling.
use crate::network_policy::Clock;
use crate::time::{ClockReading, ProviderIdentity, TimeTracker};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[path = "time_schedule_store.rs"]
mod store;

pub const ATTEMPT_MS: u64 = 120_000;
pub const ADMISSION_MS: u64 = 30_000;
pub const ETHERNET_MS: u64 = 15_000;
const RECENT_MS: u64 = 5 * 60_000;
const DAILY_MS: u64 = 24 * 60 * 60_000;
const RETRY_MS: [u64; 3] = [15 * 60_000, 60 * 60_000, 6 * 60 * 60_000];

/// Constructed from the device's accepted time state, never from a UI request,
/// an active link, the provider's raw reply or a sticky synchronization boolean.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Evidence {
    boot_id: String,
    success: Success,
}

impl Evidence {
    pub fn accepted(tracker: &TimeTracker, clock: ClockReading) -> Option<Self> {
        let mut tracker = tracker.clone();
        tracker.tick(clock);
        if !tracker.status(clock).usable {
            return None;
        }
        let record = tracker.record();
        let sync = record.last_sync.as_ref()?;
        if record.success_generation == 0 || !sync.receipt.matches_running_clock(clock) {
            return None;
        }
        Some(Self {
            boot_id: sync.boot_id.clone(),
            success: Success {
                generation: record.success_generation,
                provider: sync.provider.clone(),
                at_ms: sync.receipt.boottime_us / 1000,
            },
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Success {
    generation: u64,
    provider: ProviderIdentity,
    at_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Trigger {
    Automatic,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Connection {
    Ethernet,
    Wireless,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Completion {
    Synchronized,
    RecentSuccess,
    AdmissionTimeout,
    AttemptTimeout,
    NetworkUnavailable,
    ProviderUnavailable,
    ProviderRejected,
    Cancelled,
    InterruptedByBoot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    requested_ms: u64,
    previous_provider: ProviderIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attempt {
    id: u64,
    trigger: Trigger,
    started_ms: u64,
    deadline_ms: u64,
    admission_deadline_ms: u64,
    baseline_generation: u64,
    connection: Option<Connection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    admitted_ms: Option<u64>,
    ethernet_until_ms: Option<u64>,
    query: Option<Query>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    generation: u64,
    boot_id: String,
    transition_ms: u64,
    enabled: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    mode_generation: u64,
    immediate_check: bool,
    failure_stage: usize,
    next_ms: Option<u64>,
    attempt_id: u64,
    success: Option<Success>,
    active: Option<Attempt>,
    completion: Option<Completion>,
}

impl Record {
    fn initial() -> Self {
        Self {
            schema: 1,
            generation: 0,
            boot_id: "00000000-0000-0000-0000-000000000000".into(),
            transition_ms: 0,
            enabled: false,
            mode_generation: 0,
            immediate_check: false,
            failure_stage: 0,
            next_ms: None,
            attempt_id: 0,
            success: None,
            active: None,
            completion: None,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || !(Clock {
                boot_id: self.boot_id.clone(),
                ms: 0,
            })
            .valid()
            || self.failure_stage > RETRY_MS.len()
            || (!self.enabled && (self.next_ms.is_some() || self.immediate_check))
            || self.success.as_ref().is_some_and(|s| {
                s.generation == 0 || s.at_ms > self.transition_ms || !valid_provider(&s.provider)
            })
        {
            return Err(invalid("invalid time-check schedule"));
        }
        if let Some(a) = &self.active
            && (a.id == 0
                || a.id != self.attempt_id
                || a.started_ms > self.transition_ms
                || a.deadline_ms.checked_sub(a.started_ms) != Some(ATTEMPT_MS)
                || a.admission_deadline_ms.checked_sub(a.started_ms)
                    != Some(if a.trigger == Trigger::Manual {
                        ADMISSION_MS
                    } else {
                        ATTEMPT_MS
                    })
                || (a.trigger == Trigger::Automatic && !self.enabled)
                || a.ethernet_until_ms
                    .is_some_and(|end| end < a.started_ms || end > a.deadline_ms)
                || a.admitted_ms.is_some_and(|at| {
                    at < a.started_ms || at >= a.deadline_ms || at > self.transition_ms
                })
                || (a.connection == Some(Connection::Ethernet) && a.ethernet_until_ms.is_none())
                || a.query.as_ref().is_some_and(|q| {
                    a.connection.is_none()
                        || q.requested_ms < a.started_ms
                        || q.requested_ms >= a.deadline_ms
                        || q.requested_ms > self.transition_ms
                        || !valid_provider(&q.previous_provider)
                }))
        {
            return Err(invalid("invalid retained time-check attempt"));
        }
        Ok(())
    }

    fn advance(&mut self) -> Result<()> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("time-check schedule generation exhausted"))?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Active {
    pub id: u64,
    pub trigger: Trigger,
    pub started_ms: u64,
    pub deadline_ms: u64,
    pub admission_deadline_ms: u64,
    pub connection: Option<Connection>,
    pub admitted: bool,
    /// Escalate the existing reservation, without creating another attempt or
    /// restarting the provider. This is permission to request Wi-Fi, not to
    /// override a hardware block or an external radio conflict.
    pub wireless_required: bool,
    pub query_requested: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub enabled: bool,
    pub mode_generation: u64,
    pub generation: u64,
    pub active: Option<Active>,
    pub next_ms: Option<u64>,
    pub failure_stage: usize,
    pub last_success_generation: Option<u64>,
    pub completion: Option<Completion>,
}

/// One service-side owner. Every transition is flushed before work is returned
/// to the runner. Storage uncertainty disables further actions until reopening;
/// missing or corrupt state in an existing directory is never a new schedule.
pub struct Schedule {
    store: store::Store,
    record: Record,
    last_ms: u64,
}

impl Schedule {
    pub fn open(path: &Path, clock: &Clock) -> Result<Self> {
        if !clock.valid() {
            return Err(invalid("invalid time-check boot clock"));
        }
        let (store, record) = store::Store::open(path)?;
        let mut schedule = Self {
            store,
            record,
            last_ms: clock.ms,
        };
        if schedule.record.boot_id != clock.boot_id {
            let mut next = schedule.record.clone();
            next.boot_id = clock.boot_id.clone();
            if next.active.take().is_some() {
                next.completion = Some(Completion::InterruptedByBoot);
            }
            next.success = None;
            next.immediate_check = next.enabled;
            next.next_ms = next.enabled.then_some(clock.ms);
            schedule.commit(next, clock)?;
        } else if schedule.record.transition_ms > clock.ms {
            return Err(invalid("time-check boot clock moved backwards"));
        }
        Ok(schedule)
    }

    fn check(&mut self, clock: &Clock) -> Result<()> {
        self.store.check()?;
        if clock.boot_id != self.record.boot_id || clock.ms < self.last_ms {
            return Err(invalid("time-check boot clock changed or regressed"));
        }
        self.last_ms = clock.ms;
        Ok(())
    }

    fn commit(&mut self, mut next: Record, clock: &Clock) -> Result<()> {
        if next != self.record {
            next.transition_ms = clock.ms;
            next.advance()?;
            self.store.save(&next)?;
            self.record = next;
        }
        Ok(())
    }

    /// Called with current saved radio policy and durably accepted time evidence.
    /// The runner must withhold Evidence while the time store is unhealthy.
    /// Overdue work collapses to one attempt; repeated ticks never refill it.
    pub fn observe(
        &mut self,
        clock: &Clock,
        enabled: bool,
        evidence: Option<&Evidence>,
    ) -> Result<Status> {
        let mode_generation = if enabled != self.record.enabled {
            self.record
                .mode_generation
                .checked_add(1)
                .ok_or_else(|| invalid("time-check mode generation exhausted"))?
        } else {
            self.record.mode_generation
        };
        self.observe_policy(clock, enabled, mode_generation, evidence)
    }

    pub fn observe_policy(
        &mut self,
        clock: &Clock,
        enabled: bool,
        mode_generation: u64,
        evidence: Option<&Evidence>,
    ) -> Result<Status> {
        self.check(clock)?;
        if mode_generation < self.record.mode_generation {
            return Err(invalid("network mode generation regressed"));
        }
        let evidence = self.evidence(clock, evidence)?;
        let mut next = self.record.clone();
        if enabled != next.enabled || mode_generation != next.mode_generation {
            next.enabled = enabled;
            next.mode_generation = mode_generation;
            next.immediate_check = enabled;
            next.next_ms = enabled.then_some(clock.ms);
            if next
                .active
                .as_ref()
                .is_some_and(|a| a.trigger == Trigger::Automatic)
            {
                next.active = None;
                next.completion = Some(Completion::Cancelled);
            }
        }
        expire(&mut next, clock)?;
        if let Some(success) = evidence {
            let newly_accepted = next
                .success
                .as_ref()
                .is_none_or(|s| success.generation > s.generation);
            if newly_accepted {
                next.failure_stage = 0;
                next.next_ms = if enabled {
                    Some(after(success.at_ms, DAILY_MS)?)
                } else {
                    None
                };
                if next.immediate_check && next.active.is_none() {
                    next.next_ms = Some(clock.ms);
                }
                if next.active.as_ref().is_some_and(|a| {
                    success.generation > a.baseline_generation
                        && success.at_ms >= a.started_ms
                        && success.at_ms < a.deadline_ms
                        && a.query.as_ref().is_none_or(|q| {
                            success.at_ms >= q.requested_ms
                                && success.provider != q.previous_provider
                        })
                }) {
                    next.active = None;
                    next.immediate_check = false;
                    next.completion = Some(Completion::Synchronized);
                }
                next.success = Some(success.clone());
            }
        }
        if next.active.is_none() && enabled && next.next_ms.is_some_and(|due| due <= clock.ms) {
            begin(&mut next, clock, Trigger::Automatic, evidence)?;
        }
        self.commit(next, clock)?;
        self.status(clock)
    }

    /// Coalesces with an active automatic or manual attempt, preserving its ID,
    /// provider-query allowance and original deadlines. Failure history remains.
    pub fn sync_now(&mut self, clock: &Clock, evidence: Option<&Evidence>) -> Result<Status> {
        self.sync_received(clock, clock, evidence)
    }

    /// The authenticated server timestamps receipt before queueing. Queue time
    /// is part of manual admission and cannot be replenished by a busy worker.
    pub fn sync_received(
        &mut self,
        clock: &Clock,
        received: &Clock,
        evidence: Option<&Evidence>,
    ) -> Result<Status> {
        self.check(clock)?;
        if received.boot_id != clock.boot_id || received.ms > clock.ms {
            return Err(invalid("invalid time-check request receipt"));
        }
        let evidence = self.evidence(clock, evidence)?;
        let mut next = self.record.clone();
        expire(&mut next, clock)?;
        if next.active.is_none() {
            begin(&mut next, received, Trigger::Manual, evidence)?;
            expire(&mut next, clock)?;
        }
        self.commit(next, clock)?;
        self.status(clock)
    }

    /// A coherent network admission with a usable route. Repeated Ethernet
    /// observations do not move the fallback deadline; Wi-Fi never downgrades.
    pub fn connected(&mut self, clock: &Clock, id: u64, connection: Connection) -> Result<bool> {
        self.check(clock)?;
        let mut next = self.record.clone();
        let Some(a) = live(&mut next, clock, id) else {
            return Ok(false);
        };
        if !admitted(a) && clock.ms >= a.admission_deadline_ms {
            return Ok(false);
        }
        if !admitted(a) {
            a.admitted_ms = Some(clock.ms);
        }
        if a.connection != Some(Connection::Wireless) {
            if connection == Connection::Ethernet && a.ethernet_until_ms.is_none() {
                a.ethernet_until_ms = Some(after(clock.ms, ETHERNET_MS)?.min(a.deadline_ms));
            }
            a.connection = Some(connection);
        }
        self.commit(next, clock)?;
        Ok(true)
    }

    /// Network admission can precede association. A manual hold starts within
    /// 30 seconds; a saved profile may connect later inside the same window.
    pub fn admitted(&mut self, clock: &Clock, id: u64) -> Result<bool> {
        self.check(clock)?;
        let mut next = self.record.clone();
        let Some(a) = live(&mut next, clock, id) else {
            return Ok(false);
        };
        if !admitted(a) {
            if clock.ms >= a.admission_deadline_ms {
                return Ok(false);
            }
            a.admitted_ms = Some(clock.ms);
        }
        self.commit(next, clock)?;
        Ok(true)
    }

    /// Commit before sending the single provider restart/request. If the runner
    /// dies or loses its reply, reopening cannot obtain another query allowance.
    /// The provider adapter must separately revalidate this exact identity before
    /// dispatch; it cannot start a missing or deliberately disabled provider.
    pub fn claim_query(
        &mut self,
        clock: &Clock,
        id: u64,
        previous: ProviderIdentity,
    ) -> Result<bool> {
        self.check(clock)?;
        if !valid_provider(&previous) {
            return Err(invalid("invalid time provider identity"));
        }
        let mut next = self.record.clone();
        let Some(a) = live(&mut next, clock, id) else {
            return Ok(false);
        };
        if a.connection.is_none() || a.query.is_some() {
            return Ok(false);
        }
        a.query = Some(Query {
            requested_ms: clock.ms,
            previous_provider: previous,
        });
        self.commit(next, clock)?;
        Ok(true)
    }

    pub fn fail(&mut self, clock: &Clock, id: u64, reason: Completion) -> Result<bool> {
        self.check(clock)?;
        if !matches!(
            reason,
            Completion::NetworkUnavailable
                | Completion::ProviderUnavailable
                | Completion::ProviderRejected
        ) {
            return Err(invalid("invalid time-check failure reason"));
        }
        let mut next = self.record.clone();
        if live(&mut next, clock, id).is_none() {
            return Ok(false);
        }
        failed(&mut next, clock.ms, reason)?;
        self.commit(next, clock)?;
        Ok(true)
    }

    /// Read-only: no admission, provider operation, schedule transition or wake.
    pub fn status(&mut self, clock: &Clock) -> Result<Status> {
        self.check(clock)?;
        Ok(Status {
            enabled: self.record.enabled,
            mode_generation: self.record.mode_generation,
            generation: self.record.generation,
            active: self.record.active.as_ref().map(|a| Active {
                id: a.id,
                trigger: a.trigger,
                started_ms: a.started_ms,
                deadline_ms: a.deadline_ms,
                admission_deadline_ms: a.admission_deadline_ms,
                connection: a.connection,
                admitted: admitted(a),
                wireless_required: a.connection == Some(Connection::Wireless)
                    || a.ethernet_until_ms.is_some_and(|until| clock.ms >= until),
                query_requested: a.query.is_some(),
            }),
            next_ms: self.record.next_ms,
            failure_stage: self.record.failure_stage,
            last_success_generation: self.record.success.as_ref().map(|s| s.generation),
            completion: self.record.completion,
        })
    }

    fn evidence<'a>(
        &self,
        clock: &Clock,
        evidence: Option<&'a Evidence>,
    ) -> Result<Option<&'a Success>> {
        let Some(evidence) = evidence else {
            return Ok(None);
        };
        if evidence.boot_id != clock.boot_id {
            return Ok(None);
        }
        let s = &evidence.success;
        if s.generation == 0
            || s.at_ms > clock.ms
            || !valid_provider(&s.provider)
            || self.record.success.as_ref().is_some_and(|old| {
                s.generation < old.generation
                    || s.at_ms < old.at_ms
                    || (s.generation == old.generation && s != old)
            })
        {
            return Err(invalid(
                "accepted time evidence regressed or changed identity",
            ));
        }
        Ok(Some(s))
    }
}

fn begin(
    record: &mut Record,
    clock: &Clock,
    trigger: Trigger,
    evidence: Option<&Success>,
) -> Result<()> {
    if let Some(s) = evidence.filter(|s| clock.ms.saturating_sub(s.at_ms) <= RECENT_MS) {
        record.success = Some(s.clone());
        record.failure_stage = 0;
        record.immediate_check = false;
        record.next_ms = if record.enabled {
            Some(after(s.at_ms, DAILY_MS)?)
        } else {
            None
        };
        record.completion = Some(Completion::RecentSuccess);
        return Ok(());
    }
    record.attempt_id = record
        .attempt_id
        .checked_add(1)
        .ok_or_else(|| invalid("time-check attempt ID exhausted"))?;
    record.active = Some(Attempt {
        id: record.attempt_id,
        trigger,
        started_ms: clock.ms,
        deadline_ms: after(clock.ms, ATTEMPT_MS)?,
        admission_deadline_ms: after(
            clock.ms,
            if trigger == Trigger::Manual {
                ADMISSION_MS
            } else {
                ATTEMPT_MS
            },
        )?,
        baseline_generation: record.success.as_ref().map_or(0, |s| s.generation),
        connection: None,
        admitted_ms: None,
        ethernet_until_ms: None,
        query: None,
    });
    record.immediate_check = false;
    record.next_ms = None;
    record.completion = None;
    Ok(())
}

fn failed(record: &mut Record, end: u64, reason: Completion) -> Result<()> {
    record.active = None;
    record.completion = Some(reason);
    record.immediate_check = false;
    let delay = RETRY_MS[record.failure_stage.min(RETRY_MS.len() - 1)];
    record.failure_stage = (record.failure_stage + 1).min(RETRY_MS.len());
    record.next_ms = if record.enabled {
        Some(after(end, delay)?)
    } else {
        None
    };
    Ok(())
}

fn expire(record: &mut Record, clock: &Clock) -> Result<()> {
    if let Some(a) = &record.active {
        if !admitted(a)
            && a.admission_deadline_ms < a.deadline_ms
            && clock.ms >= a.admission_deadline_ms
        {
            let end = a.admission_deadline_ms;
            failed(record, end, Completion::AdmissionTimeout)?;
        } else if clock.ms >= a.deadline_ms {
            let end = a.deadline_ms;
            failed(record, end, Completion::AttemptTimeout)?;
        } else if !admitted(a) && clock.ms >= a.admission_deadline_ms {
            let end = a.admission_deadline_ms;
            failed(record, end, Completion::AdmissionTimeout)?;
        }
    }
    Ok(())
}

fn admitted(a: &Attempt) -> bool {
    a.admitted_ms.is_some() || a.connection.is_some()
}

fn live<'a>(record: &'a mut Record, clock: &Clock, id: u64) -> Option<&'a mut Attempt> {
    record
        .active
        .as_mut()
        .filter(|a| a.id == id && clock.ms < a.deadline_ms)
}

fn valid_provider(p: &ProviderIdentity) -> bool {
    p.pid > 0
        && p.bus_owner.starts_with(':')
        && p.bus_owner.len() <= 255
        && p.invocation_id.len() == 32
        && p.invocation_id.bytes().all(|b| b.is_ascii_hexdigit())
        && p.invocation_id.bytes().any(|b| b != b'0')
}

fn after(at: u64, duration: u64) -> Result<u64> {
    at.checked_add(duration)
        .ok_or_else(|| invalid("time-check deadline overflow"))
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[cfg(test)]
#[path = "time_schedule_tests.rs"]
mod tests;
