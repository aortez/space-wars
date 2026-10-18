//! Saved Wi-Fi intent and bounded connection ownership for the device service.
//! Decisions are policy intent, not permission to write NetworkManager: the
//! serialized radio adapter must still recheck actual state and live operations.
use crate::network_radio_state::RadioState;
use crate::{Error, Result};
use engine_ota::Sha256;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

#[path = "network_policy_store.rs"]
mod store;

const SETUP_MS: u64 = 120_000;
const CHECK_MS: u64 = 120_000;
const DOWNLOAD_MS: u64 = 30 * 60_000;
const DEPLOYMENT_MS: u64 = 2 * 60 * 60_000;
const RECONNECT_MS: u64 = 15 * 60_000;
const CHECKPOINT_MS: u64 = 100_000;
const QUIET_MS: u64 = 10_000;
const OBSERVATION_GAP_MS: u64 = 5_000;
const MAX_HOLDS: usize = 32;
const MAX_DEPLOYMENTS: usize = 8;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub partial_offline: bool,
    pub stay_connected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HoldKind {
    /// Reserved by the root-owned, socket-activated SSH admission helper.
    Ssh,
    Setup,
    TimeCheck,
    Download,
    Deployment,
}

/// Filled from socket peer credentials and /proc start time, never request JSON.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Caller {
    pub uid: u32,
    pub pid: u32,
    pub process_start_ticks: u64,
}

/// Root-worker admission tied to the original combined check/download window.
/// None mode generation denotes an explicit manual check; automatic work must
/// still match the mode observed when it selected a connection opportunity.
pub struct DownloadReservation {
    pub deadline: Clock,
    pub admission_deadline_ms: u64,
    pub mode_generation: Option<u64>,
    pub time_hold: Option<HoldToken>,
    pub handoff_deadline_ms: Option<u64>,
}

/// A caller-retained capability, generated before admission so a lost response
/// can be retried without allocating another hold. Never publish it in status.
#[derive(Clone, Eq, PartialEq)]
pub struct HoldToken(String);

impl std::fmt::Debug for HoldToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("HoldToken([redacted])")
    }
}

impl HoldToken {
    pub fn generate() -> Result<Self> {
        let mut bytes = [0; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(Self(bytes.iter().map(|b| format!("{b:02x}")).collect()))
    }

    pub fn parse(value: String) -> Result<Self> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("invalid network hold token"));
        }
        Ok(Self(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    fn id(&self) -> Sha256 {
        Sha256::digest(self.0.as_bytes())
    }
}

/// Boot-relative milliseconds, including suspend; a wall-clock correction
/// cannot extend a lease. Callers must use the actual current boot ID.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clock {
    pub boot_id: String,
    pub ms: u64,
}

impl Clock {
    pub(crate) fn valid(&self) -> bool {
        self.boot_id.len() == 36
            && self.boot_id.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
                }
            })
    }

    fn after(&self, duration: u64) -> Result<u64> {
        self.ms
            .checked_add(duration)
            .ok_or_else(|| invalid("network lease clock overflow"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Deployment {
    id: Sha256,
    uid: u32,
    boot_id: String,
    expires_ms: u64,
    reboot_grace_used: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    generation: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    mode_generation: u64,
    policy: Policy,
    deployments: Vec<Deployment>,
    radio: RadioState,
}

impl Record {
    fn initial() -> Self {
        Self {
            schema: 3,
            ..Self::default()
        }
    }

    fn validate(&self) -> Result<()> {
        let mut ids = std::collections::BTreeSet::new();
        self.radio.validate()?;
        if self.schema != 3
            || self.mode_generation > self.generation
            || self.deployments.len() > MAX_DEPLOYMENTS
            || self.deployments.iter().any(|hold| {
                !ids.insert(hold.id)
                    || !Clock {
                        boot_id: hold.boot_id.clone(),
                        ms: 0,
                    }
                    .valid()
            })
        {
            return Err(invalid("invalid saved network policy"));
        }
        Ok(())
    }

    fn advance(&mut self) -> Result<()> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("network policy generation exhausted"))?;
        Ok(())
    }
}

#[derive(Clone)]
struct Temporary {
    kind: HoldKind,
    caller: Caller,
    expires_ms: u64,
    absolute_ms: Option<u64>,
    wireless_required: bool,
}

/// None means the observer is incomplete, not that no activity exists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Activity {
    pub ssh_sessions: Option<bool>,
    pub checkpoints: Option<bool>,
    pub install_lock: Option<bool>,
    pub operations: Option<bool>,
}

impl Activity {
    fn idle(self) -> bool {
        [
            self.ssh_sessions,
            self.checkpoints,
            self.install_lock,
            self.operations,
        ]
        .into_iter()
        .all(|value| value == Some(false))
    }

    fn busy(self) -> bool {
        [
            self.ssh_sessions,
            self.checkpoints,
            self.install_lock,
            self.operations,
        ]
        .contains(&Some(true))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Intent {
    /// Mode Off: do not override an independently disabled radio, even for a hold.
    Unmanaged,
    Available,
    ObserveActivity,
    RecoverOperations,
    QuietGrace,
    MayDisable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Status {
    pub generation: u64,
    pub mode_generation: u64,
    pub policy: Policy,
    pub intent: Intent,
    pub holds: Vec<HoldStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HoldStatus {
    pub kind: HoldKind,
    pub expires_in_ms: u64,
    pub reconnect_grace: bool,
    pub wireless_required: bool,
}

/// One instance belongs to the serialized device coordinator. The private
/// store lock excludes a second owner. An fsync failure poisons the instance;
/// callers must defer all radio changes until it has been reopened/reconciled.
pub struct NetworkPolicy {
    store: store::Store,
    record: Record,
    temporary: BTreeMap<Sha256, Temporary>,
    boot_id: String,
    last_ms: u64,
    recover_until_ms: u64,
    quiet_since_ms: Option<u64>,
    activity: Activity,
    activity_seen_ms: Option<u64>,
}

/// Kept until the SSH helper returns. A new coordinator cannot acquire policy
/// ownership until then, and subsequently observes the activating SSH unit.
pub(crate) struct OfflineSshGuard {
    _store: Option<store::Store>,
}

pub(crate) fn protect_ssh_without_service(directory: &Path) -> Result<OfflineSshGuard> {
    let store = match store::Store::existing(directory)? {
        Some((store, record)) => {
            if record.radio.pending.is_some() || record.radio.owned_off {
                return Err(invalid("SSH protection requires the radio coordinator"));
            }
            Some(store)
        }
        None => None,
    };
    Ok(OfflineSshGuard { _store: store })
}

impl NetworkPolicy {
    pub fn open(directory: &Path, clock: Clock) -> Result<Self> {
        if !clock.valid() {
            return Err(invalid("invalid network lease clock"));
        }
        let (mut store, mut record) = store::Store::open(directory)?;
        for hold in &mut record.deployments {
            if hold.boot_id != clock.boot_id {
                // A reboot grants one reconnect allowance per explicit renewal.
                // Repeated service starts or further reboots cannot replenish it.
                hold.expires_ms = if hold.reboot_grace_used {
                    clock.ms
                } else {
                    clock.after(RECONNECT_MS)?
                };
                hold.boot_id = clock.boot_id.clone();
                hold.reboot_grace_used = true;
            }
        }
        record.advance()?;
        store.save(&record)?;
        Ok(Self {
            store,
            record,
            temporary: BTreeMap::new(),
            boot_id: clock.boot_id,
            last_ms: clock.ms,
            recover_until_ms: clock
                .ms
                .checked_add(CHECKPOINT_MS + QUIET_MS)
                .ok_or_else(|| invalid("network startup grace overflow"))?,
            quiet_since_ms: None,
            activity: Activity::default(),
            activity_seen_ms: None,
        })
    }

    fn check(&mut self, clock: &Clock) -> Result<()> {
        self.store.check()?;
        if clock.boot_id != self.boot_id || clock.ms < self.last_ms {
            return Err(invalid("network lease clock changed or regressed"));
        }
        self.last_ms = clock.ms;
        Ok(())
    }

    fn commit(&mut self, mut next: Record) -> Result<()> {
        next.advance()?;
        self.store.save(&next)?;
        self.record = next;
        Ok(())
    }

    pub(crate) fn radio(&mut self, clock: &Clock) -> Result<RadioState> {
        self.check(clock)?;
        Ok(self.record.radio.clone())
    }

    pub(crate) fn save_radio(&mut self, radio: RadioState, clock: &Clock) -> Result<()> {
        self.check(clock)?;
        if self.record.radio != radio {
            let mut next = self.record.clone();
            next.radio = radio;
            self.commit(next)?;
        }
        Ok(())
    }

    pub(crate) fn reset_quiet(&mut self) {
        self.quiet_since_ms = None;
    }

    #[cfg(test)]
    pub(crate) fn fail_save_at(&mut self, point: &'static str) {
        self.store.fail_at = Some(point);
    }

    pub(crate) fn held_kind(
        &mut self,
        token: &HoldToken,
        caller: Caller,
        clock: &Clock,
    ) -> Result<HoldKind> {
        self.check(clock)?;
        let id = token.id();
        if let Some(hold) = self.temporary.get(&id) {
            if hold.caller == caller && hold.expires_ms > clock.ms {
                return Ok(hold.kind);
            }
        } else if let Some(hold) = self.record.deployments.iter().find(|hold| hold.id == id)
            && hold.uid == caller.uid
            && hold.expires_ms > clock.ms
        {
            return Ok(HoldKind::Deployment);
        }
        Err(invalid("network hold is not live for this caller"))
    }

    pub fn set_policy(&mut self, generation: u64, policy: Policy, clock: &Clock) -> Result<()> {
        self.check(clock)?;
        if generation != self.record.generation {
            return Err(invalid("stale network policy generation"));
        }
        if policy != self.record.policy {
            let mut next = self.record.clone();
            if policy.partial_offline != next.policy.partial_offline {
                next.mode_generation = next
                    .mode_generation
                    .checked_add(1)
                    .ok_or_else(|| invalid("network mode generation exhausted"))?;
            }
            next.policy = policy;
            self.commit(next)?;
            self.quiet_since_ms = None;
        }
        Ok(())
    }

    /// Reserve ownership. This is not a successful connection/admission reply;
    /// the radio coordinator acknowledges only after actual availability checks.
    pub fn reserve(
        &mut self,
        token: &HoldToken,
        caller: Caller,
        kind: HoldKind,
        clock: &Clock,
    ) -> Result<()> {
        self.reserve_inner(token, caller, kind, clock, None)
    }

    /// Root-owned scheduler reservation. A queue delay or coordinator restart
    /// cannot grant a fresh two-minute window to an existing time attempt.
    pub(crate) fn reserve_time(
        &mut self,
        token: &HoldToken,
        caller: Caller,
        clock: &Clock,
        deadline: &Clock,
        admission_deadline_ms: u64,
        mode_generation: Option<u64>,
    ) -> Result<()> {
        self.check(clock)?;
        if caller.uid != 0
            || deadline.boot_id != clock.boot_id
            || deadline.ms <= clock.ms
            || admission_deadline_ms > deadline.ms
            || clock.ms >= admission_deadline_ms
            || deadline.ms - clock.ms > CHECK_MS
            || mode_generation.is_some_and(|expected| {
                expected != self.record.mode_generation || !self.record.policy.partial_offline
            })
        {
            return Err(invalid(
                "time reservation is expired or no longer authorized",
            ));
        }
        self.reserve_inner(token, caller, HoldKind::TimeCheck, clock, Some(deadline.ms))
    }

    pub(crate) fn reserve_download(
        &mut self,
        token: &HoldToken,
        caller: Caller,
        clock: &Clock,
        request: &DownloadReservation,
        connected: bool,
    ) -> Result<()> {
        self.check(clock)?;
        let start = request.deadline.ms.checked_sub(DOWNLOAD_MS);
        let admission = start.and_then(|start| {
            start.checked_add(if request.mode_generation.is_none() {
                30_000
            } else {
                CHECK_MS
            })
        });
        if caller.uid != 0
            || request.deadline.boot_id != clock.boot_id
            || start.is_none_or(|start| start > clock.ms)
            || admission != Some(request.admission_deadline_ms)
            || clock.ms >= request.admission_deadline_ms
            || request.deadline.ms <= clock.ms
            || request
                .mode_generation
                .is_some_and(|mode| mode != self.record.mode_generation)
            || (request.mode_generation.is_none() && request.time_hold.is_some())
            || (request.time_hold.is_some() != request.handoff_deadline_ms.is_some())
            || request
                .handoff_deadline_ms
                .is_some_and(|end| clock.ms >= end)
        {
            return Err(invalid(
                "download reservation is expired or no longer authorized",
            ));
        }
        // Retrying an already reserved token does not need its former parent.
        // reserve_inner still verifies the caller, kind, lease and exact cap.
        if !self.temporary.contains_key(&token.id()) {
            if let Some(parent) = &request.time_hold {
                if self.held_kind(parent, caller, clock)? != HoldKind::TimeCheck {
                    return Err(invalid("download handoff requires a live owned time hold"));
                }
            } else if request.mode_generation.is_some()
                && self.record.policy.partial_offline
                && !connected
            {
                return Err(invalid("automatic download has no connection opportunity"));
            }
        }
        self.reserve_inner(
            token,
            caller,
            HoldKind::Download,
            clock,
            Some(request.deadline.ms),
        )
    }

    fn reserve_inner(
        &mut self,
        token: &HoldToken,
        caller: Caller,
        kind: HoldKind,
        clock: &Clock,
        deadline_ms: Option<u64>,
    ) -> Result<()> {
        self.check(clock)?;
        if kind == HoldKind::Ssh && caller.uid != 0 {
            return Err(invalid("SSH admission requires the root helper"));
        }
        let id = token.id();
        if let Some(old) = self.temporary.get(&id) {
            if old.caller != caller || old.kind != kind {
                return Err(invalid("network hold owner or kind differs"));
            }
            if old.expires_ms <= clock.ms {
                return Err(invalid("network hold expired"));
            }
            if deadline_ms.is_some_and(|end| old.absolute_ms != Some(end)) {
                return Err(invalid("network hold deadline changed"));
            }
            return Ok(());
        }
        if let Some(old) = self.record.deployments.iter().find(|h| h.id == id) {
            if old.uid != caller.uid || kind != HoldKind::Deployment {
                return Err(invalid("network hold owner or kind differs"));
            }
            if old.expires_ms <= clock.ms {
                return Err(invalid("network hold expired"));
            }
            return Ok(());
        }
        // A live session or incomplete observer may prevent idle cleanup for
        // hours. A new SSH or setup admission can replace expired SSH
        // reservations because its fresh hold preserves network protection.
        // Releasing it still requires observed idle and the normal quiet grace
        // before shutdown. Other hold kinds keep their existing recovery grace.
        let replaced: Vec<_> = self
            .temporary
            .iter()
            .filter_map(|(id, hold)| {
                (matches!(kind, HoldKind::Ssh | HoldKind::Setup)
                    && hold.kind == HoldKind::Ssh
                    && hold.expires_ms <= clock.ms)
                    .then_some(*id)
            })
            .collect();
        if self.temporary.len() + self.record.deployments.len() - replaced.len() >= MAX_HOLDS {
            return Err(invalid("network hold capacity reached"));
        }
        let mut next = self.record.clone();
        if kind == HoldKind::Deployment {
            if next.deployments.len() >= MAX_DEPLOYMENTS {
                return Err(invalid("deployment hold capacity reached"));
            }
            next.deployments.push(Deployment {
                id,
                uid: caller.uid,
                boot_id: clock.boot_id.clone(),
                expires_ms: clock.after(DEPLOYMENT_MS)?,
                reboot_grace_used: false,
            });
            self.commit(next)?;
        } else {
            let absolute_ms = match kind {
                HoldKind::TimeCheck => Some(deadline_ms.unwrap_or(clock.after(CHECK_MS)?)),
                HoldKind::Download => Some(deadline_ms.unwrap_or(clock.after(DOWNLOAD_MS)?)),
                _ => None,
            };
            let expires_ms = clock.after(SETUP_MS)?.min(absolute_ms.unwrap_or(u64::MAX));
            self.commit(next)?;
            for id in replaced {
                self.temporary.remove(&id);
            }
            self.temporary.insert(
                id,
                Temporary {
                    kind,
                    caller,
                    expires_ms,
                    absolute_ms,
                    wireless_required: false,
                },
            );
        }
        self.quiet_since_ms = None;
        Ok(())
    }

    pub fn renew(&mut self, token: &HoldToken, caller: Caller, clock: &Clock) -> Result<()> {
        self.check(clock)?;
        let id = token.id();
        if let Some(old) = self.temporary.get(&id) {
            if old.caller != caller {
                return Err(invalid("network hold belongs to another process"));
            }
            if old.expires_ms <= clock.ms {
                return Err(invalid("network hold expired"));
            }
            let mut renewed = old.clone();
            renewed.expires_ms = clock
                .after(SETUP_MS)?
                .min(old.absolute_ms.unwrap_or(u64::MAX));
            self.commit(self.record.clone())?;
            self.temporary.insert(id, renewed);
        } else {
            let mut next = self.record.clone();
            let hold = next
                .deployments
                .iter_mut()
                .find(|h| h.id == id)
                .ok_or_else(|| invalid("unknown network hold"))?;
            if hold.uid != caller.uid {
                return Err(invalid("network hold belongs to another account"));
            }
            if hold.expires_ms <= clock.ms {
                return Err(invalid("network hold expired"));
            }
            hold.expires_ms = clock.after(DEPLOYMENT_MS)?;
            hold.reboot_grace_used = false;
            self.commit(next)?;
        }
        Ok(())
    }

    /// Escalate an existing time-check reservation after Ethernet fails to
    /// produce a sync. No new hold, renewal or absolute deadline is granted.
    pub(crate) fn require_wireless(
        &mut self,
        token: &HoldToken,
        caller: Caller,
        clock: &Clock,
    ) -> Result<()> {
        if self.held_kind(token, caller, clock)? != HoldKind::TimeCheck {
            return Err(invalid("wireless fallback requires a time-check hold"));
        }
        let id = token.id();
        if !self.temporary[&id].wireless_required {
            self.commit(self.record.clone())?;
            self.temporary
                .get_mut(&id)
                .expect("live time hold")
                .wireless_required = true;
        }
        Ok(())
    }

    pub(crate) fn requires_wireless(
        &mut self,
        token: &HoldToken,
        caller: Caller,
        clock: &Clock,
    ) -> Result<bool> {
        self.held_kind(token, caller, clock)?;
        Ok(self
            .temporary
            .get(&token.id())
            .is_some_and(|h| h.wireless_required))
    }

    pub fn release(&mut self, token: &HoldToken, caller: Caller, clock: &Clock) -> Result<()> {
        self.check(clock)?;
        let id = token.id();
        if let Some(old) = self.temporary.get(&id) {
            if old.caller != caller {
                return Err(invalid("network hold belongs to another process"));
            }
            self.commit(self.record.clone())?;
            self.temporary.remove(&id);
        } else if let Some(old) = self.record.deployments.iter().find(|h| h.id == id) {
            if old.uid != caller.uid {
                return Err(invalid("network hold belongs to another account"));
            }
            let mut next = self.record.clone();
            next.deployments.retain(|h| h.id != id);
            self.commit(next)?;
        } else {
            // Retrying a completed release cannot restart someone else's quiet
            // grace or postpone cleanup of another expired lease.
            return Ok(());
        }
        self.quiet_since_ms = None;
        Ok(())
    }

    /// Observations must come from the OS adapters, not a caller's assertions.
    pub fn observe(&mut self, activity: Activity, clock: &Clock) -> Result<()> {
        self.check(clock)?;
        if self
            .activity_seen_ms
            .is_none_or(|last| clock.ms.saturating_sub(last) > OBSERVATION_GAP_MS)
        {
            self.quiet_since_ms = None;
        }
        self.activity_seen_ms = Some(clock.ms);
        self.activity = activity;
        if !activity.idle() {
            self.quiet_since_ms = None;
            return Ok(());
        }
        let quiet = *self.quiet_since_ms.get_or_insert(clock.ms);
        if clock.ms < self.recover_until_ms || clock.ms.saturating_sub(quiet) < QUIET_MS {
            return Ok(());
        }
        let expired: Vec<_> = self
            .temporary
            .iter()
            .filter_map(|(id, h)| {
                let grace = if h.kind == HoldKind::Setup {
                    CHECKPOINT_MS
                } else {
                    0
                };
                (clock.ms >= h.expires_ms.saturating_add(grace)).then_some(*id)
            })
            .collect();
        let mut next = self.record.clone();
        next.deployments.retain(|h| h.expires_ms > clock.ms);
        if !expired.is_empty() || next.deployments != self.record.deployments {
            self.commit(next)?;
            for id in expired {
                self.temporary.remove(&id);
            }
        }
        Ok(())
    }

    pub fn status(&mut self, clock: &Clock) -> Result<Status> {
        self.check(clock)?;
        let mut holds: Vec<_> = self
            .temporary
            .values()
            .map(|h| HoldStatus {
                kind: h.kind,
                expires_in_ms: h.expires_ms.saturating_sub(clock.ms),
                reconnect_grace: false,
                wireless_required: h.wireless_required,
            })
            .collect();
        holds.extend(self.record.deployments.iter().map(|h| HoldStatus {
            kind: HoldKind::Deployment,
            expires_in_ms: h.expires_ms.saturating_sub(clock.ms),
            reconnect_grace: h.reboot_grace_used,
            wireless_required: false,
        }));
        let intent = if !self.record.policy.partial_offline {
            Intent::Unmanaged
        } else if self.record.policy.stay_connected || !holds.is_empty() || self.activity.busy() {
            Intent::Available
        } else if !self.activity.idle()
            || self
                .activity_seen_ms
                .is_none_or(|last| clock.ms.saturating_sub(last) > OBSERVATION_GAP_MS)
        {
            Intent::ObserveActivity
        } else if clock.ms < self.recover_until_ms {
            Intent::RecoverOperations
        } else if self
            .quiet_since_ms
            .is_none_or(|start| clock.ms.saturating_sub(start) < QUIET_MS)
        {
            Intent::QuietGrace
        } else {
            Intent::MayDisable
        };
        Ok(Status {
            generation: self.record.generation,
            mode_generation: self.record.mode_generation,
            policy: self.record.policy,
            intent,
            holds,
        })
    }
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[cfg(test)]
#[path = "network_policy_tests.rs"]
mod tests;
