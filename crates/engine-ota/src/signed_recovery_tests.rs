//! Exercise the signed-feed/staging/writer/recovery boundary with real signatures,
//! ext4 bytes and persistent journals. Boot, watchdog and app observations are
//! fixtures; this does not qualify a device or invoke its block/boot adapters.
use super::*;
use crate::coordinator::{RecoveryBackend, RecoveryOutcome, recover};
use crate::recovery::{
    Action, AppInstance, BootBinding, BootObservation, FailureReason, HealthSample, Phase,
    RECOVERY_PROTOCOL, RecoveryCapability, RecoveryError, RecoveryRecord, Slot, SlotImage,
    TRIAL_DEADLINE_MS,
};
use crate::staging_linux::{ResponseHeaders, StagingStore, TRANSFER_CHUNK_BYTES};
use crate::test_support::{self as feed, EXPIRES, Fault};
use crate::{DeviceIdentity, Eligibility, Selection, TrustStore, WatchdogCompletion};
use std::sync::Arc;

struct BootFixture {
    boot: BootObservation,
    healthy: bool,
    samples: u64,
    normal_reboots: u32,
    watchdog_completions: u32,
    selector_changes: u32,
}

impl RecoveryBackend for BootFixture {
    fn observe(&mut self) -> Result<BootObservation> {
        Ok(self.boot.clone())
    }

    fn verify_early_watchdog(&mut self) -> Result<()> {
        Ok(()) // Simulated capability, never hardware qualification.
    }

    fn health(&mut self, _: &BootObservation) -> Result<HealthSample> {
        self.samples += 1;
        if !self.healthy {
            return Err(invalid("fixture kiosk service is stopped"));
        }
        Ok(HealthSample {
            uptime_ms: self.boot.uptime_ms,
            instance: AppInstance {
                pid: 123,
                start_ticks: 1,
                invocation_id: "a".repeat(32),
            },
            build_id: self.boot.actual_build_id.clone().unwrap(),
            executable_verified: true,
            state_writable: true,
            control_response: self.samples,
            presented_frames: self.samples * 10,
        })
    }

    fn apply(&mut self, store: &RecoveryStore, action: &Action) -> Result<BootObservation> {
        let (slot, trial) = store
            .durable_record()?
            .selection_change(action, &self.boot)?;
        assert!(trial.is_none());
        self.boot.default_slot = Some(slot);
        self.selector_changes += 1;
        Ok(self.boot.clone())
    }

    fn complete_watchdog(&mut self, store: &RecoveryStore) -> Result<WatchdogCompletion> {
        store.durable_record()?.watchdog_completion(&self.boot)?;
        self.watchdog_completions += 1;
        Ok(WatchdogCompletion::Stopped)
    }

    fn reboot_previous_good(&mut self, store: &RecoveryStore, id: Sha256) -> Result<()> {
        let record = store.durable_record()?;
        record.rollback_reboot(id, &self.boot)?;
        // Failure and quarantine must already be durable when reboot is issued.
        assert_eq!(record.transaction().unwrap().phase, Phase::RollbackPending);
        assert_eq!(
            record.transaction().unwrap().failure,
            Some(FailureReason::HealthTimeout)
        );
        assert!(
            record
                .image_history()
                .failed_images
                .contains(&record.transaction().unwrap().candidate.payload)
        );
        self.normal_reboots += 1;
        Ok(())
    }

    fn wait_for_sample(&mut self) {
        self.boot.uptime_ms += 2_000;
    }
}

#[tokio::test]
async fn signed_ext4_health_failure_survives_restart_and_a_new_feed() {
    // The control and failing case use identical image bytes. Only the app
    // health observation changes; a bad image cannot explain the failure.
    let image = Fixture::new();
    for (healthy, supervised) in [(true, false), (false, false), (true, true), (false, true)] {
        let directory = tempfile::tempdir().unwrap();
        let trust_path = directory.path().join("trust");
        let recovery_path = directory.path().join("recovery");
        let staging_path = directory.path().join("staging");
        fs::create_dir(&staging_path).unwrap();
        fs::set_permissions(&staging_path, fs::Permissions::from_mode(0o700)).unwrap();
        let owner = unsafe { libc::geteuid() };
        let binding = BootBinding {
            device_id: Sha256::digest(b"fixture board"),
            layout_id: Sha256::digest(b"fixture layout"),
            boot_identity: image.image.boot_identity,
            hardware_profile: Sha256::digest(b"fixture profile"),
        };
        let good = SlotImage {
            slot: Slot::A,
            build_id: "unpublished-bootstrap".into(),
            generation: 0,
            payload: Sha256::digest(b"bootstrap compressed"),
            expanded: Sha256::digest(b"bootstrap expanded"),
            data: image.image.data,
        };
        let normal = BootObservation {
            binding: binding.clone(),
            boot_id: "11111111-1111-1111-1111-111111111111".into(),
            uptime_ms: 1_000_000,
            actual_slot: Some(good.slot),
            actual_build_id: Some(good.build_id.clone()),
            recovery_protocol: RECOVERY_PROTOCOL,
            default_slot: Some(good.slot),
            tryboot: false,
            trial_id: None,
        };
        let capability = RecoveryCapability {
            binding,
            tryboot_qualified: true,
            watchdog_qualified: true,
        };
        let baseline =
            RecoveryRecord::bootstrap(normal.binding.clone(), good.clone(), &normal).unwrap();
        let mut recovery = RecoveryStore::initialize(&recovery_path, baseline).unwrap();
        let device = DeviceIdentity {
            machine: image.image.machine.clone(),
            architecture: image.image.architecture.clone(),
            layout: image.image.layout.clone(),
            boot_identity: image.image.boot_identity,
            updater_protocol: image.image.min_updater_protocol,
            recovery_protocol: RECOVERY_PROTOCOL,
            inactive_slot_bytes: image.image.expanded.bytes + 4096,
            persistent_schema: good.data.writes,
            rollback_reads: good.data.reads,
        };
        let feed = feed::Fixture::new().await;
        let mut trust =
            TrustStore::initialize(&trust_path, &feed.root_bytes, feed::location()).unwrap();
        let transport = feed
            .publish(
                1,
                std::slice::from_ref(&image.image),
                EXPIRES.parse().unwrap(),
                Fault::None,
            )
            .await;
        let candidate = feed::candidate(
            trust
                .refresh(
                    Arc::new(transport),
                    feed::time(),
                    Channel::Release,
                    &device,
                    &recovery.record().image_history(),
                )
                .await
                .unwrap(),
        );
        let mut staging =
            StagingStore::under(open_directory(&staging_path, owner).unwrap(), owner).unwrap();
        let mut transfer = staging.prepare(&candidate).unwrap();
        transfer
            .accept(ResponseHeaders {
                status: 200,
                length: Some(image.image.compressed.bytes),
                range: None,
                etag: None,
                encoding: None,
            })
            .unwrap();
        for chunk in image.compressed.chunks(TRANSFER_CHUNK_BYTES) {
            transfer.append(chunk).unwrap();
        }
        let candidate = transfer
            .finish(
                &mut trust,
                &device,
                &recovery.record().image_history(),
                || Ok(feed::time()),
            )
            .await
            .unwrap();
        drop(staging);
        let mut payload = StagedPayload::under(
            open_directory(&staging_path, owner).unwrap(),
            owner,
            candidate.image().compressed.clone(),
        )
        .unwrap();
        payload.verify(candidate.image()).unwrap();

        // Even a signed, fully staged candidate cannot waive qualification.
        let unqualified = RecoveryCapability {
            watchdog_qualified: false,
            ..capability.clone()
        };
        assert!(matches!(
            recovery.advance(|r| r.begin(&candidate, &normal, &unqualified)),
            Err(Error::Recovery(RecoveryError::Unqualified))
        ));
        assert!(recovery.record().transaction().is_none());
        let selected_record = recovery.record().fingerprint();
        let action = recovery
            .advance(|r| {
                if supervised {
                    r.begin_supervised(&candidate, &normal, selected_record, false)
                } else {
                    r.begin(&candidate, &normal, &capability)
                }
            })
            .unwrap();
        let id = recovery.record().transaction().unwrap().id;
        assert_eq!(
            action,
            Action::WriteInactive {
                slot: Slot::B,
                transaction: id
            }
        );
        assert_eq!(
            recovery
                .durable_record()
                .unwrap()
                .inactive_write(candidate.image(), &normal)
                .unwrap(),
            id
        );
        let mut target = FileTarget::new(device.inactive_slot_bytes);
        write_image(
            candidate.image(),
            payload.reader().unwrap(),
            &mut target,
            |file| ImageTools::default().check(file),
            &mut |_| {},
        )
        .unwrap();
        target.file.rewind().unwrap();
        let mut installed = vec![0; image.bytes.len()];
        target.file.read_exact(&mut installed).unwrap();
        assert_eq!(installed, image.bytes);
        let prepared = recovery
            .advance(|r| {
                r.written(
                    id,
                    candidate.image().compressed.sha256,
                    candidate.image().expanded.sha256,
                    &normal,
                )
            })
            .unwrap();
        assert_eq!(
            prepared,
            Action::PrepareTrial {
                slot: Slot::B,
                transaction: id
            }
        );
        assert_eq!(
            recovery.advance(|r| r.trial_prepared(id, &normal)).unwrap(),
            Action::RebootTrial { transaction: id }
        );
        assert!(recovery.advance(|r| r.trial_prepared(id, &normal)).is_err());
        drop(recovery);
        let mut recovery = RecoveryStore::open(&recovery_path).unwrap();
        let mut backend = BootFixture {
            boot: BootObservation {
                boot_id: "22222222-2222-2222-2222-222222222222".into(),
                uptime_ms: 20_000,
                actual_slot: Some(Slot::B),
                actual_build_id: Some(candidate.image().build_id.clone()),
                tryboot: true,
                trial_id: Some(id),
                ..normal.clone()
            },
            healthy,
            samples: 0,
            normal_reboots: 0,
            watchdog_completions: 0,
            selector_changes: 0,
        };
        let outcome = recover(&mut recovery, &mut backend, |_| {}).unwrap();
        if healthy {
            assert_eq!(outcome, RecoveryOutcome::Healthy);
            assert_eq!(
                recovery.record().transaction().unwrap().phase,
                Phase::Committed
            );
            assert_eq!(backend.boot.uptime_ms, 80_000);
            assert_eq!(backend.boot.default_slot, Some(Slot::B));
            assert_eq!(backend.normal_reboots, 0);
            assert_eq!(backend.watchdog_completions, 1);
            assert_eq!(backend.selector_changes, 1);
            assert_eq!(
                recovery.record().good().payload,
                candidate.image().compressed.sha256
            );
            continue;
        }
        assert_eq!(outcome, RecoveryOutcome::RebootRequested);
        assert_eq!(backend.boot.uptime_ms, TRIAL_DEADLINE_MS);
        assert_eq!(backend.boot.default_slot, Some(good.slot));
        assert_eq!(backend.normal_reboots, 1);
        assert_eq!(backend.watchdog_completions, 0);
        assert_eq!(backend.selector_changes, 0);
        assert_eq!(recovery.record().good(), &good);
        drop(recovery);
        let mut recovery = RecoveryStore::open(&recovery_path).unwrap();
        backend.boot = BootObservation {
            boot_id: "33333333-3333-3333-3333-333333333333".into(),
            uptime_ms: 20_000,
            ..normal
        };
        for _ in 0..2 {
            assert_eq!(
                recover(&mut recovery, &mut backend, |_| {}).unwrap(),
                RecoveryOutcome::Healthy
            );
            assert_eq!(
                recovery.record().transaction().unwrap().phase,
                Phase::RolledBack
            );
            assert_eq!(backend.normal_reboots, 1);
        }
        // Replaying the old verified object is refused by the durable journal.
        assert!(matches!(
            recovery.advance(|r| r.begin(&candidate, &backend.boot, &capability)),
            Err(Error::Recovery(RecoveryError::Quarantined))
        ));
        drop(trust);
        let mut trust = TrustStore::open(&trust_path).unwrap();
        let history = recovery.record().image_history();
        assert_eq!(
            history.highest_attempted_generation,
            candidate.image().generation
        );
        assert!(matches!(
            trust
                .cached(feed::time(), Channel::Release, &device, &history)
                .await
                .unwrap(),
            Selection::Ineligible {
                reason: Eligibility::Quarantined,
                ..
            }
        ));
        // Publishing a newer, freshly signed catalog cannot clear quarantine.
        let newer = feed
            .publish(
                2,
                std::slice::from_ref(&image.image),
                EXPIRES.parse().unwrap(),
                Fault::None,
            )
            .await;
        assert!(matches!(
            trust
                .refresh(
                    Arc::new(newer),
                    feed::time(),
                    Channel::Release,
                    &device,
                    &history
                )
                .await
                .unwrap(),
            Selection::Ineligible {
                reason: Eligibility::Quarantined,
                ..
            }
        ));
        assert_eq!(recovery.durable_record().unwrap().good(), &good);
        assert_eq!(recovery.record().transaction().unwrap().attempt, 1);
        assert_eq!(backend.normal_reboots, 1);
    }
}
