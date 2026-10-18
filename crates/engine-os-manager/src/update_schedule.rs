//! OTA check cadence, independent of the time provider's retry schedule.
//! Transitions commit before the caller acquires a connection or dispatches IO.
//! This record never authorizes installation or represents a verified candidate.
use crate::network_policy::Clock;
use crate::schedule_store::{StateRecord, Store};
use crate::{Error, Result};
use engine_ota::Channel;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const STATE_DIRECTORY: &str = "/data/spacewars-system/update-check";
pub const DAILY_MS: u64 = 24 * 60 * 60_000;
pub const ADMISSION_MS: u64 = 30_000;
pub const PREPARATION_MS: u64 = 120_000;
pub const SESSION_MS: u64 = 30 * 60_000;

/// Supplied from fresh coordinator evidence, never from a client request. A
/// partial-offline automatic check cannot manufacture its own radio opportunity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Opportunity {
    None,
    Connected,
    TimeCheck,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Trigger {
    Automatic,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Completion {
    Ready,
    NoCandidate,
    Ineligible,
    Failed,
    Cancelled,
    ChannelChanged,
    AdmissionTimeout,
    PreparationTimeout,
    SessionTimeout,
    Interrupted,
    InterruptedByBoot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Active {
    pub id: u64,
    pub trigger: Trigger,
    pub started_ms: u64,
    pub admission_deadline_ms: u64,
    pub preparation_deadline_ms: u64,
    pub deadline_ms: u64,
    pub admitted: bool,
    pub payload_started: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    generation: u64,
    boot_id: String,
    transition_ms: u64,
    channel: Channel,
    selection_generation: u64,
    next_ms: u64,
    attempt_id: u64,
    active: Option<Active>,
    completion: Option<Completion>,
}

impl StateRecord for Record {
    fn initial() -> Self {
        Self {
            schema: 1,
            generation: 0,
            boot_id: "00000000-0000-0000-0000-000000000000".into(),
            transition_ms: 0,
            channel: Channel::Release,
            selection_generation: 0,
            next_ms: 0,
            attempt_id: 0,
            active: None,
            completion: None,
        }
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.selection_generation > self.generation
            || !(Clock {
                boot_id: self.boot_id.clone(),
                ms: 0,
            })
            .valid()
        {
            return Err(invalid("invalid update schedule"));
        }
        if let Some(a) = &self.active
            && (a.id == 0
                || a.id != self.attempt_id
                || self.completion.is_some()
                || a.started_ms > self.transition_ms
                || a.admission_deadline_ms.checked_sub(a.started_ms)
                    != Some(if a.trigger == Trigger::Manual {
                        ADMISSION_MS
                    } else {
                        PREPARATION_MS
                    })
                || a.preparation_deadline_ms.checked_sub(a.started_ms) != Some(PREPARATION_MS)
                || a.deadline_ms.checked_sub(a.started_ms) != Some(SESSION_MS)
                || self
                    .next_ms
                    .checked_sub(a.started_ms)
                    .is_none_or(|ms| ms < DAILY_MS)
                || (a.payload_started && !a.admitted))
        {
            return Err(invalid("invalid retained update attempt"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub generation: u64,
    pub channel: Channel,
    pub selection_generation: u64,
    pub active: Option<Active>,
    pub next_ms: u64,
    pub due: bool,
    pub completion: Option<Completion>,
}

pub struct Schedule {
    store: Store<Record>,
    record: Record,
    last_ms: u64,
}

impl Schedule {
    /// A restarted process terminates its old combined session. It retains the
    /// daily allowance and partial bytes for a later eligible/manual attempt;
    /// reopening cannot create a fresh HTTP or connection budget for old work.
    pub fn open(path: &Path, clock: &Clock) -> Result<Self> {
        if !clock.valid() {
            return Err(invalid("invalid update boot clock"));
        }
        let (store, record) = Store::<Record>::open(path)?;
        let mut schedule = Self {
            store,
            record,
            last_ms: clock.ms,
        };
        let mut next = schedule.record.clone();
        if next.boot_id != clock.boot_id {
            next.boot_id = clock.boot_id.clone();
            if next.active.take().is_some() {
                next.completion = Some(Completion::InterruptedByBoot);
            }
            next.next_ms = clock.ms;
        } else {
            if next.transition_ms > clock.ms {
                return Err(invalid("update boot clock moved backwards"));
            }
            expire(&mut next, clock);
            if next.active.take().is_some() {
                next.completion = Some(Completion::Interrupted);
            }
        }
        schedule.commit(next, clock)?;
        Ok(schedule)
    }

    /// Only the worker calls this, after deriving the opportunity from current
    /// network state. There is no NTP retry counter or wall-clock deadline here.
    pub fn automatic(
        &mut self,
        clock: &Clock,
        partial_offline: bool,
        opportunity: Opportunity,
    ) -> Result<Status> {
        self.check(clock)?;
        let mut next = self.record.clone();
        expire(&mut next, clock);
        if next.active.is_none()
            && next.next_ms <= clock.ms
            && (!partial_offline || opportunity != Opportunity::None)
        {
            begin(&mut next, clock, clock, Trigger::Automatic)?;
        }
        self.commit(next, clock)?;
        self.status(clock)
    }

    /// The server must timestamp receipt before queueing. The selection revision
    /// prevents a queued request for an old feed from starting after a switch.
    pub fn check_now(
        &mut self,
        clock: &Clock,
        received: &Clock,
        selection_generation: u64,
    ) -> Result<Status> {
        self.check(clock)?;
        if received.boot_id != clock.boot_id
            || received.ms > clock.ms
            || selection_generation != self.record.selection_generation
        {
            return Err(invalid("stale update check request"));
        }
        let mut next = self.record.clone();
        expire(&mut next, clock);
        if next.active.is_none() {
            begin(&mut next, clock, received, Trigger::Manual)?;
            expire(&mut next, clock);
        }
        self.commit(next, clock)?;
        self.status(clock)
    }

    pub fn admitted(&mut self, clock: &Clock, id: u64) -> Result<bool> {
        self.change_active(clock, id, |active| {
            active.admitted = true;
            Ok(())
        })
    }

    /// The fast downloader callback timestamps first payload progress after feed
    /// verification. Queue latency cannot turn an on-time event into a timeout;
    /// the coordinator processes this event before advancing expiration. The
    /// actual total deadline still applies when this event is consumed.
    pub fn payload_started(&mut self, clock: &Clock, id: u64, received: &Clock) -> Result<bool> {
        self.check(clock)?;
        if received.boot_id != clock.boot_id || received.ms > clock.ms {
            return Err(invalid("invalid update progress receipt"));
        }
        let mut next = self.record.clone();
        let mut accepted = false;
        if let Some(active) = next.active.as_mut().filter(|a| a.id == id) {
            if !active.admitted || received.ms < active.started_ms {
                return Err(invalid("update payload has no admitted connection"));
            }
            if received.ms < active.preparation_deadline_ms && clock.ms < active.deadline_ms {
                active.payload_started = true;
                accepted = true;
            }
        }
        expire(&mut next, clock);
        self.commit(next, clock)?;
        Ok(accepted)
    }

    pub fn finish(&mut self, clock: &Clock, id: u64, completion: Completion) -> Result<bool> {
        if !matches!(
            completion,
            Completion::Ready
                | Completion::NoCandidate
                | Completion::Ineligible
                | Completion::Failed
                | Completion::Cancelled
        ) {
            return Err(invalid("invalid update completion"));
        }
        self.check(clock)?;
        let mut next = self.record.clone();
        expire(&mut next, clock);
        let matches = next.active.as_ref().is_some_and(|a| a.id == id);
        if matches {
            if completion == Completion::Ready
                && !next.active.as_ref().is_some_and(|a| a.payload_started)
            {
                return Err(invalid("update readiness preceded payload verification"));
            }
            next.active = None;
            next.completion = Some(completion);
        }
        self.commit(next, clock)?;
        Ok(matches)
    }

    /// Switching feeds invalidates outstanding work and makes the new feed due.
    /// The runner must cancel/join old IO before it calls automatic/check_now
    /// again; candidate approvals belong to the later installation policy.
    pub fn select(&mut self, clock: &Clock, channel: Channel) -> Result<Status> {
        self.check(clock)?;
        let mut next = self.record.clone();
        if channel != next.channel {
            next.selection_generation = next
                .selection_generation
                .checked_add(1)
                .ok_or_else(|| invalid("update selection generation exhausted"))?;
            next.channel = channel;
            next.active = None;
            next.completion = Some(Completion::ChannelChanged);
            next.next_ms = clock.ms;
        }
        self.commit(next, clock)?;
        self.status(clock)
    }

    /// No transition, write, connection request or IO job is created by status.
    pub fn status(&mut self, clock: &Clock) -> Result<Status> {
        self.check(clock)?;
        Ok(Status {
            generation: self.record.generation,
            channel: self.record.channel,
            selection_generation: self.record.selection_generation,
            active: self.record.active.clone(),
            next_ms: self.record.next_ms,
            due: self.record.active.is_none() && clock.ms >= self.record.next_ms,
            completion: self.record.completion,
        })
    }

    fn change_active(
        &mut self,
        clock: &Clock,
        id: u64,
        change: impl FnOnce(&mut Active) -> Result<()>,
    ) -> Result<bool> {
        self.check(clock)?;
        let mut next = self.record.clone();
        expire(&mut next, clock);
        let matches = if let Some(active) = next.active.as_mut().filter(|a| a.id == id) {
            change(active)?;
            true
        } else {
            false
        };
        self.commit(next, clock)?;
        Ok(matches)
    }

    fn check(&mut self, clock: &Clock) -> Result<()> {
        self.store.check()?;
        if !clock.valid() || clock.boot_id != self.record.boot_id || clock.ms < self.last_ms {
            return Err(invalid("update boot clock changed or regressed"));
        }
        self.last_ms = clock.ms;
        Ok(())
    }

    fn commit(&mut self, mut next: Record, clock: &Clock) -> Result<()> {
        if next != self.record {
            next.generation = next
                .generation
                .checked_add(1)
                .ok_or_else(|| invalid("update schedule generation exhausted"))?;
            next.transition_ms = clock.ms;
            self.store.save(&next)?;
            self.record = next;
        }
        Ok(())
    }
}

fn begin(record: &mut Record, observed: &Clock, received: &Clock, trigger: Trigger) -> Result<()> {
    record.attempt_id = record
        .attempt_id
        .checked_add(1)
        .ok_or_else(|| invalid("update attempt ID exhausted"))?;
    record.active = Some(Active {
        id: record.attempt_id,
        trigger,
        started_ms: received.ms,
        admission_deadline_ms: after(
            received.ms,
            if trigger == Trigger::Manual {
                ADMISSION_MS
            } else {
                PREPARATION_MS
            },
        )?,
        preparation_deadline_ms: after(received.ms, PREPARATION_MS)?,
        deadline_ms: after(received.ms, SESSION_MS)?,
        admitted: false,
        payload_started: false,
    });
    // Consume this day's automatic allowance before any network admission.
    // Even a late queued request cannot cause replay of overdue daily windows.
    record.next_ms = after(observed.ms, DAILY_MS)?;
    record.completion = None;
    Ok(())
}

fn expire(record: &mut Record, clock: &Clock) {
    let Some(active) = &record.active else {
        return;
    };
    let completion = if !active.admitted && clock.ms >= active.admission_deadline_ms {
        Some(Completion::AdmissionTimeout)
    } else if !active.payload_started && clock.ms >= active.preparation_deadline_ms {
        Some(Completion::PreparationTimeout)
    } else if clock.ms >= active.deadline_ms {
        Some(Completion::SessionTimeout)
    } else {
        None
    };
    if let Some(completion) = completion {
        record.active = None;
        record.completion = Some(completion);
    }
}

fn after(ms: u64, duration: u64) -> Result<u64> {
    ms.checked_add(duration)
        .ok_or_else(|| invalid("update deadline overflow"))
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

#[cfg(test)]
#[path = "update_schedule_tests.rs"]
mod tests;
