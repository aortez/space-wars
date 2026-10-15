mod support;

use engine_ota::*;
use std::collections::BTreeSet;
use std::io::Cursor;
use std::sync::Arc;
use support::*;

#[tokio::test]
async fn persisted_recovery_quarantine_allows_only_a_freshly_verified_bounded_retry() {
    use engine_ota::recovery::{
        Action, BootBinding, BootObservation, RecoveryCapability, RecoveryRecord, Slot, SlotImage,
    };
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let trust_path = directory.path().join("trust");
    let recovery_path = directory.path().join("recovery");
    let mut trust = TrustStore::initialize(&trust_path, &fixture.root_bytes, location()).unwrap();
    let old = descriptor(Channel::Release, 1);
    let binding = BootBinding {
        device_id: Sha256::digest(b"board"),
        layout_id: Sha256::digest(b"layout"),
        boot_identity: old.boot_identity,
        hardware_profile: Sha256::digest(b"display"),
    };
    let boot = BootObservation {
        binding: binding.clone(),
        boot_id: "11111111-1111-1111-1111-111111111111".into(),
        uptime_ms: 1_000_000,
        actual_slot: Some(Slot::A),
        actual_build_id: Some(old.build_id.clone()),
        recovery_protocol: 1,
        default_slot: Some(Slot::A),
        tryboot: false,
        trial_id: None,
    };
    let good = SlotImage {
        slot: Slot::A,
        build_id: old.build_id,
        generation: old.generation,
        payload: old.compressed.sha256,
        expanded: old.expanded.sha256,
        data: old.data,
    };
    let cap = RecoveryCapability {
        binding: binding.clone(),
        tryboot_qualified: true,
        watchdog_qualified: true,
    };
    let record = RecoveryRecord::bootstrap(binding, good, &boot).unwrap();
    let mut recovery = RecoveryStore::initialize(&recovery_path, record).unwrap();
    let image = descriptor(Channel::Release, 2);
    let feed = fixture
        .publish(
            1,
            std::slice::from_ref(&image),
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    let verified = candidate(
        trust
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &recovery.record().image_history(),
            )
            .await
            .unwrap(),
    );
    assert!(matches!(
        recovery
            .advance(|state| state.begin(&verified, &boot, &cap))
            .unwrap(),
        Action::WriteInactive { slot: Slot::B, .. }
    ));
    drop(recovery);
    drop(trust);
    // Simulate service restart while still on the good root. The interrupted
    // write is quarantined, and reopening either store cannot erase its floor.
    let mut recovery = RecoveryStore::open(&recovery_path).unwrap();
    recovery.advance(|state| state.reconcile(&boot)).unwrap();
    let mut trust = TrustStore::open(&trust_path).unwrap();
    assert!(matches!(
        trust
            .cached(
                time(),
                Channel::Release,
                &device(),
                &recovery.record().image_history()
            )
            .await
            .unwrap(),
        Selection::Ineligible {
            reason: Eligibility::Quarantined,
            ..
        }
    ));
    let retry_history = recovery
        .record()
        .retry_verification_history(image.compressed.sha256)
        .unwrap();
    let retry = trust
        .revalidate_image(
            verified.id(),
            &mut Cursor::new(payload(2)),
            time(),
            Channel::Release,
            &device(),
            &retry_history,
        )
        .await
        .unwrap();
    assert!(
        recovery
            .advance(|state| state.begin(&retry, &boot, &cap))
            .is_err()
    );
    recovery
        .advance(|state| state.begin_explicit_retry(&retry, &boot, &cap))
        .unwrap();
    assert_eq!(recovery.record().transaction().unwrap().attempt, 2);
    assert_eq!(
        recovery
            .record()
            .image_history()
            .highest_attempted_generation,
        2
    );
    recovery.advance(|state| state.reconcile(&boot)).unwrap();
    assert!(
        recovery
            .record()
            .retry_verification_history(image.compressed.sha256)
            .is_err()
    );
}

#[tokio::test]
async fn signed_channels_and_offline_install_survive_reopening_the_store() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("trust");
    let mut store = TrustStore::initialize(&path, &fixture.root_bytes, location()).unwrap();
    let feed = fixture
        .publish(
            1,
            &[
                descriptor(Channel::Release, 5),
                descriptor(Channel::Nightly, 7),
            ],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    let history = ImageHistory::default();
    let release = candidate(
        store
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &history,
            )
            .await
            .unwrap(),
    );
    assert_eq!(release.image().generation, 5);
    let nightly = candidate(
        store
            .cached(time(), Channel::Nightly, &device(), &history)
            .await
            .unwrap(),
    );
    assert_eq!(nightly.image().generation, 7);
    drop(store);
    // This is a new library/process instance with no bootstrap root or network
    // transport supplied. Only the persistent store and staged image remain.
    let mut store = TrustStore::open(&path).unwrap();
    let verified = store
        .revalidate_image(
            release.id(),
            &mut Cursor::new(payload(5)),
            time(),
            Channel::Release,
            &device(),
            &history,
        )
        .await
        .unwrap();
    assert_eq!(verified.id(), release.id());
    assert!(matches!(
        store
            .revalidate_image(
                nightly.id(),
                &mut Cursor::new(payload(7)),
                time(),
                Channel::Release,
                &device(),
                &history
            )
            .await,
        Err(Error::CandidateChanged)
    ));
}

#[tokio::test]
async fn signature_threshold_requires_distinct_authorized_signatures() {
    for fault in [
        Fault::MissingReleaseSignature,
        Fault::RepeatedReleaseSignature,
        Fault::WrongReleaseSignature,
    ] {
        let fixture = Fixture::new().await;
        let directory = tempfile::tempdir().unwrap();
        let mut store = TrustStore::initialize(
            &directory.path().join("trust"),
            &fixture.root_bytes,
            location(),
        )
        .unwrap();
        let feed = fixture
            .publish(
                1,
                &[descriptor(Channel::Release, 1)],
                EXPIRES.parse().unwrap(),
                fault,
            )
            .await;
        let result = store
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default(),
            )
            .await;
        assert!(matches!(result, Err(Error::Tuf(_))), "{result:?}");
        assert!(matches!(
            store
                .cached(
                    time(),
                    Channel::Release,
                    &device(),
                    &ImageHistory::default()
                )
                .await,
            Err(Error::RefreshRequired)
        ));
    }
}

#[tokio::test]
async fn signed_feed_cannot_escape_the_channel_or_image_contract() {
    for fault in [
        Fault::CrossChannelName,
        Fault::UnsafeName,
        Fault::DescriptorHash,
        Fault::ExtraDelegation,
        Fault::SharedChannelKey,
    ] {
        let fixture = Fixture::new().await;
        let directory = tempfile::tempdir().unwrap();
        let mut store = TrustStore::initialize(
            &directory.path().join("trust"),
            &fixture.root_bytes,
            location(),
        )
        .unwrap();
        let feed = fixture
            .publish(
                1,
                &[descriptor(Channel::Release, 1)],
                EXPIRES.parse().unwrap(),
                fault,
            )
            .await;
        assert!(
            store
                .refresh(
                    Arc::new(feed),
                    time(),
                    Channel::Release,
                    &device(),
                    &ImageHistory::default()
                )
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn accepted_root_rotation_is_retained_across_restart_and_offline_validation() {
    let mut fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("trust");
    let mut store = TrustStore::initialize(&path, &fixture.root_bytes, location()).unwrap();
    fixture.rotate_root().await;
    let feed = fixture
        .publish(
            1,
            &[descriptor(Channel::Release, 1)],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    let selected = candidate(
        store
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default(),
            )
            .await
            .unwrap(),
    );
    drop(store);
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("state.json")).unwrap()).unwrap();
    let root: serde_json::Value =
        serde_json::from_str(state["state"]["trusted"]["root.json"].as_str().unwrap()).unwrap();
    assert_eq!(root["signed"]["version"], 2);
    let mut store = TrustStore::open(&path).unwrap();
    assert_eq!(
        candidate(
            store
                .cached(
                    time(),
                    Channel::Release,
                    &device(),
                    &ImageHistory::default()
                )
                .await
                .unwrap()
        )
        .id(),
        selected.id()
    );
    assert!(TrustStore::initialize(&path, &fixture.root_bytes, location()).is_err());
}

#[tokio::test]
async fn replay_after_restart_is_rejected_without_discarding_the_current_cached_feed() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("trust");
    let mut store = TrustStore::initialize(&path, &fixture.root_bytes, location()).unwrap();
    let latest = fixture
        .publish(
            2,
            &[descriptor(Channel::Release, 2)],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    store
        .refresh(
            Arc::new(latest),
            time(),
            Channel::Release,
            &device(),
            &ImageHistory::default(),
        )
        .await
        .unwrap();
    drop(store);
    let mut store = TrustStore::open(&path).unwrap();
    let old = fixture
        .publish(
            1,
            &[descriptor(Channel::Release, 1)],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    assert!(matches!(
        store
            .refresh(
                Arc::new(old),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await,
        Err(Error::Tuf(_))
    ));
    assert_eq!(
        candidate(
            store
                .cached(
                    time(),
                    Channel::Release,
                    &device(),
                    &ImageHistory::default()
                )
                .await
                .unwrap()
        )
        .image()
        .generation,
        2
    );
}

#[tokio::test]
async fn interrupted_new_publication_invalidates_old_evidence_but_preserves_the_new_version_floor()
{
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("trust");
    let mut store = TrustStore::initialize(&path, &fixture.root_bytes, location()).unwrap();
    let old = fixture
        .publish(
            1,
            &[descriptor(Channel::Release, 1)],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    store
        .refresh(
            Arc::new(old.clone()),
            time(),
            Channel::Release,
            &device(),
            &ImageHistory::default(),
        )
        .await
        .unwrap();
    let mut mixed = fixture
        .publish(
            2,
            &[descriptor(Channel::Release, 2)],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    mixed.files.insert(
        "2.snapshot.json".into(),
        old.files["1.snapshot.json"].clone(),
    );
    assert!(
        store
            .refresh(
                Arc::new(mixed),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await
            .is_err()
    );
    drop(store);
    let mut store = TrustStore::open(&path).unwrap();
    assert!(matches!(
        store
            .cached(
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await,
        Err(Error::RefreshRequired)
    ));
    assert!(
        store
            .refresh(
                Arc::new(old),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await
            .is_err()
    );
    let recovered = fixture
        .publish(
            2,
            &[descriptor(Channel::Release, 2)],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    assert_eq!(
        candidate(
            store
                .refresh(
                    Arc::new(recovered),
                    time(),
                    Channel::Release,
                    &device(),
                    &ImageHistory::default()
                )
                .await
                .unwrap()
        )
        .image()
        .generation,
        2
    );
}

#[tokio::test]
async fn unknown_expired_and_backwards_time_never_accept_metadata() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let mut store = TrustStore::initialize(
        &directory.path().join("trust"),
        &fixture.root_bytes,
        location(),
    )
    .unwrap();
    let feed = fixture
        .publish(
            1,
            &[descriptor(Channel::Release, 1)],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    let requests = feed.requests.clone();
    assert!(matches!(
        store
            .refresh(
                Arc::new(feed.clone()),
                None,
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await,
        Err(Error::WaitingForTime)
    ));
    assert!(requests.lock().unwrap().is_empty());
    store
        .refresh(
            Arc::new(feed),
            time(),
            Channel::Release,
            &device(),
            &ImageHistory::default(),
        )
        .await
        .unwrap();
    let backwards = Some(UsableTime("2026-10-14T11:59:59Z".parse().unwrap()));
    assert!(matches!(
        store
            .cached(
                backwards,
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await,
        Err(Error::TimeMovedBackwards)
    ));
    assert!(
        store
            .cached(
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await
            .is_ok()
    );
    let expired = Some(UsableTime(EXPIRES.parse().unwrap()));
    assert!(matches!(
        store
            .cached(
                expired,
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await,
        Err(Error::Tuf(_))
    ));
    // Once later time was observed, stepping back cannot revive expired data.
    assert!(
        store
            .cached(
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn signed_unsupported_schema_is_not_an_installable_candidate() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let mut store = TrustStore::initialize(
        &directory.path().join("trust"),
        &fixture.root_bytes,
        location(),
    )
    .unwrap();
    let mut image = descriptor(Channel::Release, 1);
    image.schema = 2;
    let feed = fixture
        .publish(1, &[image], EXPIRES.parse().unwrap(), Fault::None)
        .await;
    assert!(matches!(
        store
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await,
        Err(Error::Descriptor(_))
    ));
}

#[tokio::test]
async fn image_identity_survives_promotion_and_rejects_relabelled_generations() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let mut store = TrustStore::initialize(
        &directory.path().join("trust"),
        &fixture.root_bytes,
        location(),
    )
    .unwrap();
    let nightly = descriptor(Channel::Nightly, 7);
    let feed = fixture
        .publish(
            1,
            std::slice::from_ref(&nightly),
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    store
        .refresh(
            Arc::new(feed),
            time(),
            Channel::Nightly,
            &device(),
            &ImageHistory::default(),
        )
        .await
        .unwrap();
    let mut release = nightly.clone();
    release.channel = Channel::Release;
    let feed = fixture
        .publish(
            2,
            &[nightly, release.clone()],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    let history = ImageHistory {
        installed_sha256: Some(release.compressed.sha256),
        highest_attempted_generation: 7,
        ..ImageHistory::default()
    };
    assert!(matches!(
        store
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &history
            )
            .await
            .unwrap(),
        Selection::Ineligible {
            reason: Eligibility::AlreadyInstalled,
            ..
        }
    ));
    release.generation = 8;
    let feed = fixture
        .publish(3, &[release], EXPIRES.parse().unwrap(), Fault::None)
        .await;
    assert!(matches!(
        store
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &history
            )
            .await,
        Err(Error::Feed(_))
    ));
}

#[tokio::test]
async fn compatible_generation_is_retained_when_a_new_boot_image_is_published() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let mut store = TrustStore::initialize(
        &directory.path().join("trust"),
        &fixture.root_bytes,
        location(),
    )
    .unwrap();
    let old = descriptor(Channel::Release, 5);
    let mut new_boot = descriptor(Channel::Release, 8);
    new_boot.boot_identity = Sha256::digest(b"new kernel");
    let feed = fixture
        .publish(1, &[old, new_boot], EXPIRES.parse().unwrap(), Fault::None)
        .await;
    assert_eq!(
        candidate(
            store
                .refresh(
                    Arc::new(feed),
                    time(),
                    Channel::Release,
                    &device(),
                    &ImageHistory::default()
                )
                .await
                .unwrap()
        )
        .image()
        .generation,
        5
    );
}

#[tokio::test]
async fn quarantined_newest_image_does_not_fall_back_to_an_older_download() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let mut store = TrustStore::initialize(
        &directory.path().join("trust"),
        &fixture.root_bytes,
        location(),
    )
    .unwrap();
    let newest = descriptor(Channel::Release, 7);
    let history = ImageHistory {
        highest_attempted_generation: 7,
        failed_images: BTreeSet::from([newest.compressed.sha256]),
        ..ImageHistory::default()
    };
    let feed = fixture
        .publish(
            1,
            &[descriptor(Channel::Release, 5), newest],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    assert!(matches!(
        store
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &history
            )
            .await
            .unwrap(),
        Selection::Ineligible {
            reason: Eligibility::Quarantined,
            ..
        }
    ));
}

#[test]
fn compatibility_and_downgrade_gates_return_specific_reasons() {
    let image = descriptor(Channel::Release, 5);
    let history = ImageHistory::default();
    let baseline = device();
    let mut variants = Vec::new();
    let mut device = baseline.clone();
    device.machine = "other".into();
    variants.push((device, Eligibility::WrongMachine));
    let mut device = baseline.clone();
    device.architecture = "x86_64".into();
    variants.push((device, Eligibility::WrongArchitecture));
    let mut device = baseline.clone();
    device.layout = "other".into();
    variants.push((device, Eligibility::WrongLayout));
    let mut device = baseline.clone();
    device.boot_identity = Sha256::digest(b"other");
    variants.push((device, Eligibility::ManualBootUpgrade));
    let mut device = baseline.clone();
    device.updater_protocol = 0;
    variants.push((device, Eligibility::UpdaterTooOld));
    let mut device = baseline.clone();
    device.recovery_protocol = 0;
    variants.push((device, Eligibility::RecoveryUnavailable));
    let mut device = baseline.clone();
    device.inactive_slot_bytes = 1023;
    variants.push((device, Eligibility::ImageTooLarge));
    let mut device = baseline.clone();
    device.persistent_schema = 3;
    variants.push((device, Eligibility::IncompatibleData));
    let mut device = baseline.clone();
    device.rollback_reads = SchemaRange { min: 2, max: 2 };
    variants.push((device, Eligibility::IncompatibleData));
    for (device, reason) in variants {
        assert_eq!(image.eligibility(&device, &history), reason);
    }
    // Recovery to the prior slot leaves the attempt floor intact.
    let recovered = ImageHistory {
        installed_sha256: Some(descriptor(Channel::Release, 4).compressed.sha256),
        highest_attempted_generation: 7,
        ..history
    };
    assert_eq!(
        image.eligibility(&baseline, &recovered),
        Eligibility::NotNewer
    );
}

#[test]
fn corrupt_truncated_and_oversized_payloads_are_rejected() {
    let expected = descriptor(Channel::Release, 1).compressed;
    assert!(verify_blob(&mut Cursor::new(payload(1)), &expected).is_ok());
    for bytes in [
        payload(2),
        payload(1)[..3].to_vec(),
        [payload(1), vec![0]].concat(),
    ] {
        assert!(matches!(
            verify_blob(&mut Cursor::new(bytes), &expected),
            Err(Error::ImageMismatch)
        ));
    }
}

#[tokio::test]
async fn trust_store_is_exclusive_and_corruption_does_not_rebootstrap() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("trust");
    let store = TrustStore::initialize(&path, &fixture.root_bytes, location()).unwrap();
    assert!(matches!(TrustStore::open(&path), Err(Error::Busy)));
    drop(store);
    std::fs::write(path.join("state.json"), b"{broken").unwrap();
    assert!(TrustStore::open(&path).is_err());
    assert!(TrustStore::initialize(&path, &fixture.root_bytes, location()).is_err());
}

#[tokio::test]
async fn metadata_size_is_bounded_before_parsing() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let mut store = TrustStore::initialize(
        &directory.path().join("trust"),
        &fixture.root_bytes,
        location(),
    )
    .unwrap();
    let mut feed = fixture
        .publish(
            1,
            &[descriptor(Channel::Release, 1)],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    feed.files
        .insert("timestamp.json".into(), vec![b' '; MAX_METADATA_BYTES + 1]);
    assert!(matches!(
        store
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await,
        Err(Error::Tuf(_))
    ));
}

#[test]
fn feed_endpoints_require_https_without_credentials_or_path_ambiguity() {
    for base in [
        "http://updates.example/",
        "file:///tmp/feed/",
        "https://user:secret@updates.example/",
        "https://updates.example/feed?token=value",
        "https://updates.example/feed",
    ] {
        assert!(FeedLocation::new(base.parse().unwrap(), location().targets).is_err());
    }
}

#[tokio::test]
async fn removing_a_candidate_from_a_new_feed_revokes_its_cached_approval() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let mut store = TrustStore::initialize(
        &directory.path().join("trust"),
        &fixture.root_bytes,
        location(),
    )
    .unwrap();
    let feed = fixture
        .publish(
            1,
            &[descriptor(Channel::Release, 1)],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    let approved = candidate(
        store
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default(),
            )
            .await
            .unwrap(),
    );
    for bad in [
        payload(2),
        payload(1)[..3].to_vec(),
        [payload(1), vec![0]].concat(),
    ] {
        assert!(matches!(
            store
                .revalidate_image(
                    approved.id(),
                    &mut Cursor::new(bad),
                    time(),
                    Channel::Release,
                    &device(),
                    &ImageHistory::default()
                )
                .await,
            Err(Error::ImageMismatch)
        ));
    }
    let revoked = fixture
        .publish(2, &[], EXPIRES.parse().unwrap(), Fault::None)
        .await;
    assert!(matches!(
        store
            .refresh(
                Arc::new(revoked),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await
            .unwrap(),
        Selection::NoCandidate
    ));
    assert!(matches!(
        store
            .revalidate_image(
                approved.id(),
                &mut Cursor::new(payload(1)),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await,
        Err(Error::CandidateChanged)
    ));
}

#[tokio::test]
async fn signed_metadata_lengths_cannot_override_the_local_size_limit() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let mut store = TrustStore::initialize(
        &directory.path().join("trust"),
        &fixture.root_bytes,
        location(),
    )
    .unwrap();
    let mut image = descriptor(Channel::Release, 1);
    image.build_id = "a".repeat(MAX_METADATA_BYTES);
    let feed = fixture
        .publish(1, &[image], EXPIRES.parse().unwrap(), Fault::None)
        .await;
    // The snapshot correctly signs the oversized role's hash and length. It
    // still fails in the bounded transport, before descriptor parsing.
    assert!(matches!(
        store
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await,
        Err(Error::Tuf(_))
    ));
}

#[tokio::test]
async fn structurally_valid_storage_corruption_cannot_reset_trust_history() {
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("trust");
    let mut store = TrustStore::initialize(&path, &fixture.root_bytes, location()).unwrap();
    let feed = fixture
        .publish(
            2,
            &[descriptor(Channel::Release, 2)],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    store
        .refresh(
            Arc::new(feed),
            time(),
            Channel::Release,
            &device(),
            &ImageHistory::default(),
        )
        .await
        .unwrap();
    drop(store);
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("state.json")).unwrap()).unwrap();
    state["state"]["trusted"]
        .as_object_mut()
        .unwrap()
        .remove("timestamp.json");
    std::fs::write(path.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(matches!(TrustStore::open(&path), Err(Error::Store(_))));
}

#[cfg(unix)]
#[tokio::test]
async fn caller_readable_or_symlinked_trust_state_is_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("trust");
    drop(TrustStore::initialize(&path, &fixture.root_bytes, location()).unwrap());
    let state = path.join("state.json");
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(TrustStore::open(&path), Err(Error::Store(_))));
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o600)).unwrap();
    let moved = directory.path().join("caller-owned-state");
    std::fs::rename(&state, &moved).unwrap();
    symlink(&moved, &state).unwrap();
    assert!(matches!(TrustStore::open(&path), Err(Error::Store(_))));
}

#[tokio::test]
async fn cancelled_metadata_check_still_retains_observed_time() {
    #[derive(Clone, Debug)]
    struct PendingTransport;
    #[async_trait::async_trait]
    impl tough::Transport for PendingTransport {
        async fn fetch(
            &self,
            _: url::Url,
        ) -> std::result::Result<tough::TransportStream, tough::TransportError> {
            std::future::pending().await
        }
    }
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("trust");
    let mut store = TrustStore::initialize(&path, &fixture.root_bytes, location()).unwrap();
    let later = Some(UsableTime("2026-10-14T13:00:00Z".parse().unwrap()));
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(20),
            store.refresh(
                Arc::new(PendingTransport),
                later,
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
        )
        .await
        .is_err()
    );
    drop(store);
    let mut store = TrustStore::open(&path).unwrap();
    let feed = fixture
        .publish(
            1,
            &[descriptor(Channel::Release, 1)],
            EXPIRES.parse().unwrap(),
            Fault::None,
        )
        .await;
    assert!(matches!(
        store
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &device(),
                &ImageHistory::default()
            )
            .await,
        Err(Error::TimeMovedBackwards)
    ));
}
