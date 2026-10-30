use super::*;
use crate::install_handoff::{Context, Coordinator, Ready};
use engine_ota::recovery::SlotImage;
use engine_ota::{DataCompatibility, SchemaRange};
use spacewars_control::install::{Candidate, Channel, Consent, HostGate, HostInstance, Id};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use crate::test_feed as feed;

const BOOT: &str = "11111111-2222-3333-4444-555555555555";

fn hash(name: &str) -> Sha256 {
    Sha256::digest(name.as_bytes())
}

#[test]
fn deployment_capability_survives_restart_and_is_retained_until_terminal_release() {
    use crate::network_policy::HoldToken;
    let mut f = Fixture::new();
    let token = HoldToken::generate().unwrap();
    f.journal
        .as_mut()
        .unwrap()
        .prepare_inner(&f.prepare, BOOT, 4, &f.before, Some(token.clone()))
        .unwrap();
    f.reopen();
    let journal = f.journal.as_mut().unwrap();
    assert_eq!(journal.deployment().unwrap().as_ref(), Some(&token));
    assert!(!format!("{:?}", journal.record).contains(token.expose()));
    assert!(matches!(
        journal.reconcile_inner(&f.before).unwrap(),
        Resolution::Aborted(_)
    ));
    let mut host = f.context.host.clone().unwrap();
    host.fenced = true;
    assert!(
        journal
            .release_inner(&f.before, &f.prepare.ticket, &host, 100, 100)
            .is_err()
    );
    f.reopen();
    assert_eq!(
        f.journal.as_ref().unwrap().deployment().unwrap().as_ref(),
        Some(&token)
    );
    host.fenced = false;
    f.journal
        .as_mut()
        .unwrap()
        .release_inner(&f.before, &f.prepare.ticket, &host, 100, 100)
        .unwrap();
    f.reopen();
    assert!(f.journal.as_ref().unwrap().deployment().unwrap().is_none());
}

#[test]
fn prior_journals_remain_readable_but_malformed_hold_capabilities_do_not() {
    let mut f = Fixture::new();
    f.prepare();
    let mut value = serde_json::to_value(&f.journal.as_ref().unwrap().record).unwrap();
    assert!(value["handoff"].get("deployment").is_none());
    assert!(
        serde_json::from_value::<Record>(value.clone())
            .unwrap()
            .handoff
            .unwrap()
            .deployment
            .is_none()
    );
    value["handoff"]["deployment"] = "invalid-token".into();
    assert!(serde_json::from_value::<Record>(value).is_err());
}

struct Fixture {
    _directory: tempfile::TempDir,
    path: std::path::PathBuf,
    journal: Option<Journal>,
    coordinator: Coordinator,
    context: Context,
    prepare: Prepare,
    host: HostGate,
    before: RecoveryView,
    transaction: Transaction,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("handoff");
        let journal = Journal::open(&path).unwrap();
        let mut coordinator = Coordinator::new(BOOT.into()).unwrap();
        let candidate = hash("candidate");
        let context = Context {
            boot_id: BOOT.into(),
            ready: Some(Ready {
                candidate: Candidate {
                    id: Id::from_bytes(*candidate.as_bytes()).unwrap(),
                    channel: Channel::Release,
                    generation: 2,
                    build_id: "candidate".into(),
                },
                selection_generation: 4,
            }),
            host: Some(HostObservation {
                instance: HostInstance {
                    pid: 42,
                    start_ticks: 123,
                    invocation: "a".repeat(32),
                },
                revision: 10,
                settings_revision: 20,
                game: true,
                fenced: false,
                observed_ms: 100,
            }),
            maintenance_clear: true,
        };
        let offer = coordinator.offer(&context, 100).unwrap();
        let prepare = coordinator
            .confirm(
                &Consent {
                    schema: SCHEMA,
                    ticket: offer.ticket,
                    end_game: true,
                },
                &context,
                101,
            )
            .unwrap();
        let good = SlotImage {
            slot: engine_ota::recovery::Slot::A,
            build_id: "good".into(),
            generation: 1,
            payload: hash("good-payload"),
            expanded: hash("good-expanded"),
            data: DataCompatibility {
                reads: SchemaRange { min: 1, max: 1 },
                writes: 1,
            },
        };
        // Synthetic recovery observations exercise association decisions. They
        // do not construct a VerifiedCandidate or qualify a physical device.
        let transaction = Transaction {
            id: hash("transaction"),
            candidate_id: candidate,
            previous_good: good.clone(),
            candidate: SlotImage {
                slot: good.slot.other(),
                generation: 2,
                build_id: "candidate".into(),
                payload: hash("payload"),
                expanded: hash("expanded"),
                ..good
            },
            attempt: 1,
            origin_boot_id: BOOT.into(),
            trial_boot_id: None,
            phase: Phase::Writing,
            health: None,
            failure: None,
        };
        Self {
            _directory: directory,
            path,
            journal: Some(journal),
            coordinator,
            context,
            prepare,
            host: HostGate::default(),
            before: RecoveryView {
                fingerprint: hash("before"),
                transaction: None,
            },
            transaction,
        }
    }
    fn prepare(&mut self) {
        self.journal
            .as_mut()
            .unwrap()
            .prepare_inner(&self.prepare, BOOT, 4, &self.before, None)
            .unwrap();
        self.host
            .prepare(
                self.prepare.clone(),
                &self.prepare.host,
                self.prepare.host_revision,
                self.prepare.settings_revision,
                102,
            )
            .unwrap();
    }
    fn acknowledge(&mut self) -> ManualAuthorization {
        self.host
            .saved(&self.prepare.ticket, self.prepare.settings_revision, true)
            .unwrap();
        self.host.presented(&self.prepare.ticket).unwrap();
        let acknowledgement = self.host.acknowledge(&self.prepare.ticket, 103).unwrap();
        self.coordinator
            .consume(&acknowledgement, &self.prepare.host, &self.context, 104)
            .unwrap()
    }
    fn arm(&mut self) {
        let authorization = self.acknowledge();
        self.journal
            .as_mut()
            .unwrap()
            .arm_inner(authorization, self.before.fingerprint, &self.transaction)
            .unwrap();
    }
    fn reopen(&mut self) {
        drop(self.journal.take());
        self.journal = Some(Journal::open(&self.path).unwrap());
    }
    fn current(&self, phase: Phase) -> RecoveryView {
        let mut transaction = self.transaction.clone();
        transaction.phase = phase;
        RecoveryView {
            fingerprint: hash(&format!("recovery-{phase:?}")),
            transaction: Some(transaction),
        }
    }
}

#[test]
fn restart_before_preparation_or_after_acknowledgement_never_replays_consent() {
    for boundary in 0..4 {
        let mut f = Fixture::new();
        if boundary > 0 {
            f.prepare();
        }
        if boundary == 2 {
            let _consumed = f.acknowledge();
        }
        if boundary == 3 {
            f.arm();
        }
        f.reopen();
        let resolution = f
            .journal
            .as_mut()
            .unwrap()
            .reconcile_inner(&f.before)
            .unwrap();
        if boundary == 0 {
            assert_eq!(resolution, Resolution::Clear);
            continue;
        }
        let Resolution::Aborted(aborted) = &resolution else {
            panic!("{resolution:?}")
        };
        // Even an acknowledged host remains fenced after an arbitrarily long
        // wait until the root service delivers this exact durable decision.
        if boundary >= 2 {
            f.host.expire(u64::MAX);
            assert!(f.host.fenced());
        }
        f.host.aborted(aborted).unwrap();
        assert!(!f.host.fenced());
        f.reopen();
        assert_eq!(
            f.journal
                .as_mut()
                .unwrap()
                .reconcile_inner(&f.before)
                .unwrap(),
            resolution
        );
        assert!(
            f.journal
                .as_mut()
                .unwrap()
                .prepare_inner(&f.prepare, BOOT, 4, &f.before, None)
                .is_err()
        );
    }
}

#[test]
fn matching_recovery_stays_fenced_through_every_nonterminal_phase() {
    for phase in [
        Phase::Writing,
        Phase::Prepared,
        Phase::TrialRequested,
        Phase::Observing,
        Phase::Promoting,
        Phase::RollbackPending,
    ] {
        let mut f = Fixture::new();
        f.prepare();
        f.arm();
        f.reopen();
        let current = f.current(phase);
        let result = f
            .journal
            .as_mut()
            .unwrap()
            .reconcile_inner(&current)
            .unwrap();
        assert_eq!(
            result,
            Resolution::InProgress {
                ticket: f.prepare.ticket.clone(),
                transaction: f.transaction.id,
                phase
            }
        );
        f.host.expire(u64::MAX);
        assert!(f.host.fenced());
        assert!(
            f.journal
                .as_mut()
                .unwrap()
                .release_inner(
                    &current,
                    &f.prepare.ticket,
                    f.context.host.as_ref().unwrap(),
                    100,
                    105
                )
                .is_err()
        );
    }
}

#[test]
fn only_exact_commit_or_rollback_can_be_reported_as_finished() {
    for (phase, outcome) in [
        (Phase::Committed, Outcome::Committed),
        (Phase::RolledBack, Outcome::RolledBack),
    ] {
        let mut f = Fixture::new();
        f.prepare();
        f.arm();
        let current = f.current(phase);
        let result = f
            .journal
            .as_mut()
            .unwrap()
            .reconcile_inner(&current)
            .unwrap();
        assert_eq!(
            result,
            Resolution::Finished {
                ticket: f.prepare.ticket.clone(),
                transaction: f.transaction.id,
                outcome
            }
        );
        f.reopen();
        assert_eq!(
            f.journal
                .as_mut()
                .unwrap()
                .reconcile_inner(&current)
                .unwrap(),
            result
        );
        let previous = f.current(Phase::Writing);
        assert!(
            f.journal
                .as_mut()
                .unwrap()
                .reconcile_inner(&previous)
                .is_err()
        );
        let opposite = f.current(if phase == Phase::Committed {
            Phase::RolledBack
        } else {
            Phase::Committed
        });
        assert!(
            f.journal
                .as_mut()
                .unwrap()
                .reconcile_inner(&opposite)
                .is_err()
        );
    }
}

#[test]
fn changed_or_unassociated_recovery_never_releases_the_host() {
    for change in 0..5 {
        let mut f = Fixture::new();
        f.prepare();
        if change == 4 {
            let _consumed = f.acknowledge();
        } else {
            f.arm();
        }
        let mut current = f.current(Phase::Committed);
        match change {
            0 => current.transaction = None,
            1 => current.transaction.as_mut().unwrap().id = hash("other transaction"),
            2 => current.transaction.as_mut().unwrap().candidate_id = hash("other candidate"),
            3 => {
                current.transaction.as_mut().unwrap().origin_boot_id =
                    "22222222-2222-3333-4444-555555555555".into()
            }
            _ => {}
        }
        assert!(
            f.journal
                .as_mut()
                .unwrap()
                .reconcile_inner(&current)
                .is_err()
        );
        assert!(f.host.fenced());
    }
    let mut f = Fixture::new();
    let current = f.current(Phase::Writing);
    assert!(
        f.journal
            .as_mut()
            .unwrap()
            .reconcile_inner(&current)
            .is_err()
    );
    assert!(
        f.journal
            .as_mut()
            .unwrap()
            .prepare_inner(&f.prepare, BOOT, 4, &current, None)
            .is_err()
    );
}

#[test]
fn durable_association_requires_the_consumed_ticket_selection_boot_and_plan() {
    for change in 0..6 {
        let mut f = Fixture::new();
        f.prepare();
        let mut authorization = f.acknowledge();
        let mut transaction = f.transaction.clone();
        let mut before = f.before.fingerprint;
        match change {
            0 => authorization.ticket.sequence += 1,
            1 => authorization.selection_generation += 1,
            2 => before = hash("other baseline"),
            3 => transaction.candidate_id = hash("other candidate"),
            4 => transaction.origin_boot_id = "22222222-2222-3333-4444-555555555555".into(),
            _ => transaction.phase = Phase::Prepared,
        }
        assert!(
            f.journal
                .as_mut()
                .unwrap()
                .arm_inner(authorization, before, &transaction)
                .is_err()
        );
        assert_eq!(
            f.journal
                .as_ref()
                .unwrap()
                .record
                .handoff
                .as_ref()
                .unwrap()
                .state,
            State::Preparing
        );
    }
}

#[test]
fn lost_terminal_reply_is_retained_until_a_fresh_unfenced_host_is_observed() {
    let mut f = Fixture::new();
    f.prepare();
    f.arm();
    let decision = f
        .journal
        .as_mut()
        .unwrap()
        .reconcile_inner(&f.before)
        .unwrap();
    let mut host = f.context.host.clone().unwrap();
    let mut wrong = f.prepare.ticket.clone();
    wrong.sequence += 1;
    assert!(
        f.journal
            .as_mut()
            .unwrap()
            .release_inner(&f.before, &wrong, &host, 100, 105)
            .is_err()
    );
    host.fenced = true;
    assert!(
        f.journal
            .as_mut()
            .unwrap()
            .release_inner(&f.before, &f.prepare.ticket, &host, 100, 105)
            .is_err()
    );
    host.fenced = false;
    // A recent sample from before the durable decision is not a receipt for
    // releasing it, even when it is still within the ordinary freshness bound.
    assert!(
        f.journal
            .as_mut()
            .unwrap()
            .release_inner(&f.before, &f.prepare.ticket, &host, 101, 105)
            .is_err()
    );
    for now in [99, 2101] {
        assert!(
            f.journal
                .as_mut()
                .unwrap()
                .release_inner(&f.before, &f.prepare.ticket, &host, 100, now)
                .is_err()
        );
    }
    assert_eq!(
        f.journal
            .as_mut()
            .unwrap()
            .reconcile_inner(&f.before)
            .unwrap(),
        decision
    );
    f.journal
        .as_mut()
        .unwrap()
        .release_inner(&f.before, &f.prepare.ticket, &host, 100, 105)
        .unwrap();
    f.reopen();
    assert_eq!(
        f.journal
            .as_mut()
            .unwrap()
            .reconcile_inner(&f.before)
            .unwrap(),
        Resolution::Clear
    );
    // A new epoch may now prepare; no prior consent or write is replayed.
    f.prepare.ticket.service = Id::from_bytes([9; 32]).unwrap();
    f.journal
        .as_mut()
        .unwrap()
        .prepare_inner(&f.prepare, BOOT, 4, &f.before, None)
        .unwrap();
}

#[test]
fn admission_waits_until_the_kiosk_releases_the_handoff() {
    // Admission replaces the recovery transaction. A handoff still naming the
    // old one could never match again, so every unreleased handoff refuses.
    let mut f = Fixture::new();
    assert!(
        f.journal
            .as_mut()
            .unwrap()
            .require_clear_inner(&f.before)
            .is_ok()
    );
    f.prepare();
    f.arm();
    for phase in [Phase::Writing, Phase::Committed] {
        let current = f.current(phase);
        assert!(
            f.journal
                .as_mut()
                .unwrap()
                .require_clear_inner(&current)
                .is_err()
        );
    }
    let host = f.context.host.clone().unwrap();
    let committed = f.current(Phase::Committed);
    f.journal
        .as_mut()
        .unwrap()
        .release_inner(&committed, &f.prepare.ticket, &host, 100, 105)
        .unwrap();
    f.reopen();
    assert!(
        f.journal
            .as_mut()
            .unwrap()
            .require_clear_inner(&committed)
            .is_ok()
    );

    // The first refusal aborts an abandoned preparation, which then also
    // refuses until it is released.
    let mut f = Fixture::new();
    f.prepare();
    for _ in 0..2 {
        assert!(
            f.journal
                .as_mut()
                .unwrap()
                .require_clear_inner(&f.before)
                .is_err()
        );
    }
    f.journal
        .as_mut()
        .unwrap()
        .release_inner(&f.before, &f.prepare.ticket, &host, 100, 105)
        .unwrap();
    assert!(
        f.journal
            .as_mut()
            .unwrap()
            .require_clear_inner(&f.before)
            .is_ok()
    );
}

#[test]
fn stale_release_ticket_cannot_abort_a_new_preparation() {
    let mut f = Fixture::new();
    f.prepare();
    let mut wrong = f.prepare.ticket.clone();
    wrong.sequence += 1;
    assert!(
        f.journal
            .as_mut()
            .unwrap()
            .release_inner(
                &f.before,
                &wrong,
                f.context.host.as_ref().unwrap(),
                100,
                105
            )
            .is_err()
    );
    assert_eq!(
        f.journal
            .as_ref()
            .unwrap()
            .record
            .handoff
            .as_ref()
            .unwrap()
            .state,
        State::Preparing
    );
    f.arm();
}

#[test]
fn every_persistence_failure_poisoned_handle_requires_reopen_before_a_decision() {
    for point in ["written", "file-synced", "renamed", "directory-synced"] {
        for operation in ["prepare", "arm", "abort", "finish", "release"] {
            let mut f = Fixture::new();
            if operation != "prepare" {
                f.prepare();
            }
            if ["abort", "finish", "release"].contains(&operation) {
                f.arm();
            }
            if operation == "release" {
                f.journal
                    .as_mut()
                    .unwrap()
                    .reconcile_inner(&f.before)
                    .unwrap();
            }
            let current = if operation == "finish" {
                f.current(Phase::Committed)
            } else {
                RecoveryView {
                    fingerprint: f.before.fingerprint,
                    transaction: None,
                }
            };
            f.journal.as_mut().unwrap().store.fail_at = Some(point);
            let result = match operation {
                "prepare" => f
                    .journal
                    .as_mut()
                    .unwrap()
                    .prepare_inner(&f.prepare, BOOT, 4, &f.before, None),
                "arm" => {
                    let authorization = f.acknowledge();
                    f.journal.as_mut().unwrap().arm_inner(
                        authorization,
                        f.before.fingerprint,
                        &f.transaction,
                    )
                }
                "release" => f.journal.as_mut().unwrap().release_inner(
                    &current,
                    &f.prepare.ticket,
                    f.context.host.as_ref().unwrap(),
                    100,
                    105,
                ),
                _ => f
                    .journal
                    .as_mut()
                    .unwrap()
                    .reconcile_inner(&current)
                    .map(|_| ()),
            };
            assert!(result.is_err(), "{operation}/{point}");
            assert!(
                f.journal
                    .as_mut()
                    .unwrap()
                    .reconcile_inner(&current)
                    .is_err()
            );
            f.reopen();
            let result = f
                .journal
                .as_mut()
                .unwrap()
                .reconcile_inner(&current)
                .unwrap();
            match operation {
                "finish" => assert!(matches!(
                    result,
                    Resolution::Finished {
                        outcome: Outcome::Committed,
                        ..
                    }
                )),
                "prepare" | "release" => {
                    assert!(matches!(result, Resolution::Clear | Resolution::Aborted(_)))
                }
                _ => assert!(matches!(result, Resolution::Aborted(_))),
            }
        }
    }
}

#[test]
fn missing_corrupt_or_replaced_state_and_competing_owners_fail_closed() {
    for alteration in [
        "missing",
        "corrupt",
        "symlink",
        "hardlink",
        "permissions",
        "oversized",
    ] {
        let mut f = Fixture::new();
        f.prepare();
        assert!(Journal::open(&f.path).is_err());
        let state = f.path.join("state.json");
        match alteration {
            "missing" => fs::remove_file(&state).unwrap(),
            "corrupt" => fs::write(&state, b"{}").unwrap(),
            "symlink" => {
                fs::rename(&state, f.path.join("old.json")).unwrap();
                symlink(f.path.join("old.json"), &state).unwrap();
            }
            "hardlink" => fs::hard_link(&state, f.path.join("second.json")).unwrap(),
            "permissions" => {
                fs::set_permissions(&state, fs::Permissions::from_mode(0o666)).unwrap()
            }
            _ => fs::write(&state, vec![b' '; 16 * 1024 + 1]).unwrap(),
        }
        assert!(
            f.journal
                .as_mut()
                .unwrap()
                .reconcile_inner(&f.before)
                .is_err()
        );
        drop(f.journal.take());
        assert!(Journal::open(&f.path).is_err(), "{alteration}");
    }
}

#[test]
fn verified_consent_commits_both_journals_before_returning_a_write_action() {
    use engine_ota::recovery::{BootBinding, BootObservation, RecoveryCapability, RecoveryRecord};
    use engine_ota::{Channel as FeedChannel, TrustStore};
    use std::sync::Arc;

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let signed = feed::Fixture::new().await;
            for failure in [None, Some("association"), Some("recovery")] {
                let mut f = Fixture::new();
                let directory = f._directory.path().to_owned();
                let baseline = feed::descriptor(FeedChannel::Release, 1);
                let binding = BootBinding {
                    device_id: hash("fixture-board"),
                    layout_id: hash("fixture-layout"),
                    boot_identity: baseline.boot_identity,
                    hardware_profile: hash("fixture-profile"),
                };
                let good = SlotImage {
                    slot: engine_ota::recovery::Slot::A,
                    build_id: baseline.build_id,
                    generation: 1,
                    payload: baseline.compressed.sha256,
                    expanded: baseline.expanded.sha256,
                    data: baseline.data,
                };
                let boot = BootObservation {
                    binding: binding.clone(),
                    boot_id: BOOT.into(),
                    uptime_ms: 100,
                    actual_slot: Some(good.slot),
                    actual_build_id: Some(good.build_id.clone()),
                    recovery_protocol: engine_ota::recovery::RECOVERY_PROTOCOL,
                    default_slot: Some(good.slot),
                    tryboot: false,
                    trial_id: None,
                };
                let record = RecoveryRecord::bootstrap(binding.clone(), good, &boot).unwrap();
                let recovery_path = directory.join("recovery");
                let mut recovery = RecoveryStore::initialize(&recovery_path, record).unwrap();
                let mut trust = TrustStore::initialize(
                    &directory.join("trust"),
                    &signed.root_bytes,
                    feed::location(),
                )
                .unwrap();
                let image = feed::descriptor(FeedChannel::Release, 2);
                let transport = signed
                    .publish(
                        1,
                        &[image],
                        feed::EXPIRES.parse().unwrap(),
                        feed::Fault::None,
                    )
                    .await;
                let candidate = feed::candidate(
                    trust
                        .refresh(
                            Arc::new(transport),
                            feed::time(),
                            FeedChannel::Release,
                            &feed::device(),
                            &recovery.record().image_history(),
                        )
                        .await
                        .unwrap(),
                );
                f.context.ready = Some(Ready::from_verified(&candidate, 4).unwrap());
                f.coordinator = Coordinator::new(BOOT.into()).unwrap();
                let offer = f.coordinator.offer(&f.context, 100).unwrap();
                f.prepare = f
                    .coordinator
                    .confirm(
                        &Consent {
                            schema: SCHEMA,
                            ticket: offer.ticket,
                            end_game: true,
                        },
                        &f.context,
                        101,
                    )
                    .unwrap();
                f.before = RecoveryView::read(&recovery).unwrap();
                f.prepare();
                // Only this fixture supplies qualification flags. The ordinary
                // begin gate still refuses a verified candidate without them.
                let mut capability = RecoveryCapability {
                    binding,
                    tryboot_qualified: false,
                    watchdog_qualified: false,
                };
                assert!(matches!(
                    recovery.record().begin(&candidate, &boot, &capability),
                    Err(engine_ota::recovery::RecoveryError::Unqualified)
                ));
                capability.tryboot_qualified = true;
                capability.watchdog_qualified = true;
                let plan = recovery
                    .record()
                    .begin(&candidate, &boot, &capability)
                    .unwrap();
                f.transaction = plan.installation().unwrap().1.clone();
                let authorization = f.acknowledge();
                let moved = directory.join("moved-recovery");
                match failure {
                    Some("association") => {
                        f.journal.as_mut().unwrap().store.fail_at = Some("renamed")
                    }
                    Some("recovery") => fs::rename(&recovery_path, &moved).unwrap(),
                    _ => {}
                }
                let action =
                    f.journal
                        .as_mut()
                        .unwrap()
                        .begin_inner(&mut recovery, authorization, plan);
                if failure.is_some() {
                    assert!(action.is_err());
                    assert_eq!(recovery.record().fingerprint(), f.before.fingerprint);
                    if failure == Some("recovery") {
                        assert!(
                            recovery.durable_record().is_err(),
                            "failed recovery commit must poison the handle"
                        );
                        assert!(RecoveryView::read(&recovery).is_err());
                        fs::rename(&moved, &recovery_path).unwrap();
                    }
                } else {
                    assert_eq!(
                        action.unwrap(),
                        Action::WriteInactive {
                            slot: engine_ota::recovery::Slot::B,
                            transaction: f.transaction.id
                        }
                    );
                    assert_eq!(
                        recovery.durable_record().unwrap().transaction().unwrap().id,
                        f.transaction.id
                    );
                    assert_eq!(
                        f.journal
                            .as_ref()
                            .unwrap()
                            .record
                            .handoff
                            .as_ref()
                            .unwrap()
                            .state,
                        State::Armed {
                            transaction: f.transaction.id
                        }
                    );
                }
                drop(recovery);
                f.reopen();
                let mut recovery = RecoveryStore::open(&recovery_path).unwrap();
                let result = f
                    .journal
                    .as_mut()
                    .unwrap()
                    .reconcile_inner(&RecoveryView::read(&recovery).unwrap())
                    .unwrap();
                if failure.is_some() {
                    assert!(matches!(result, Resolution::Aborted(_)));
                } else {
                    assert!(matches!(
                        result,
                        Resolution::InProgress {
                            phase: Phase::Writing,
                            ..
                        }
                    ));
                    // The existing recovery protocol, not the handoff journal,
                    // quarantines a write interrupted before activation.
                    recovery
                        .advance(|record| {
                            let plan = record.reconcile(&boot)?;
                            assert!(plan.installation().is_none());
                            Ok(plan)
                        })
                        .unwrap();
                    assert!(
                        recovery
                            .record()
                            .image_history()
                            .failed_images
                            .contains(&candidate.image().compressed.sha256)
                    );
                    assert_eq!(
                        f.journal
                            .as_mut()
                            .unwrap()
                            .reconcile_inner(&RecoveryView::read(&recovery).unwrap())
                            .unwrap(),
                        Resolution::Finished {
                            ticket: f.prepare.ticket.clone(),
                            transaction: f.transaction.id,
                            outcome: Outcome::RolledBack
                        }
                    );
                }
            }
        });
}
