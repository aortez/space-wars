use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::sync::Arc;

use crate::test_support::{EXPIRES, Fault, Fixture, candidate, descriptor, device, location, time};

struct Case {
    directory: tempfile::TempDir,
    feed: Fixture,
    trust: TrustStore,
    stage: StagingStore,
    candidate: VerifiedCandidate,
    compressed: Vec<u8>,
    device: DeviceIdentity,
}

impl Case {
    async fn new() -> Self {
        Self::alter(|_, _| {}).await
    }

    async fn alter(alter: impl FnOnce(&mut ImageDescriptor, &mut Vec<u8>)) -> Self {
        let feed = Fixture::new().await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("staging");
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let mut trust = TrustStore::initialize(
            &directory.path().join("trust"),
            &feed.root_bytes,
            location(),
        )
        .unwrap();
        let mut image = descriptor(Channel::Release, 2);
        let bytes: Vec<u8> = (0u32..32_768).flat_map(u32::to_le_bytes).collect();
        let mut compressed = zstd::stream::encode_all(bytes.as_slice(), 1).unwrap();
        image.expanded = Blob {
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(&bytes),
        };
        alter(&mut image, &mut compressed);
        image.compressed = Blob {
            bytes: compressed.len() as u64,
            sha256: Sha256::digest(&compressed),
        };
        let mut device = device();
        device.inactive_slot_bytes = 1024 * 1024;
        let transport = feed
            .publish(
                1,
                std::slice::from_ref(&image),
                EXPIRES.parse().unwrap(),
                Fault::None,
            )
            .await;
        let candidate = candidate(
            trust
                .refresh(
                    Arc::new(transport),
                    time(),
                    Channel::Release,
                    &device,
                    &ImageHistory::default(),
                )
                .await
                .unwrap(),
        );
        let stage = open_stage(&path);
        Self {
            directory,
            feed,
            trust,
            stage,
            candidate,
            compressed,
            device,
        }
    }

    fn path(&self) -> std::path::PathBuf {
        self.directory.path().join("staging")
    }
    fn ready(&self) -> std::path::PathBuf {
        self.path().join(ready_name(self.candidate.image()))
    }

    async fn replace_feed(&mut self, images: &[ImageDescriptor]) {
        let feed = self
            .feed
            .publish(2, images, EXPIRES.parse().unwrap(), Fault::None)
            .await;
        self.trust
            .refresh(
                Arc::new(feed),
                time(),
                Channel::Release,
                &self.device,
                &ImageHistory::default(),
            )
            .await
            .unwrap();
    }
}

fn open_stage(path: &Path) -> StagingStore {
    let owner = unsafe { libc::geteuid() };
    StagingStore::under(open_directory(path, owner).unwrap(), owner).unwrap()
}

fn full(total: u64, etag: Option<&str>) -> ResponseHeaders<'_> {
    ResponseHeaders {
        status: 200,
        length: Some(total),
        range: None,
        etag,
        encoding: None,
    }
}

fn write_all(transfer: &mut Transfer<'_>, bytes: &[u8]) {
    for chunk in bytes.chunks(TRANSFER_CHUNK_BYTES) {
        transfer.append(chunk).unwrap();
    }
}

#[tokio::test]
async fn interrupted_prefix_survives_restart_and_only_complete_reverification_publishes() {
    let mut case = Case::new().await;
    let path = case.path();
    let ready = case.ready();
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer
        .accept(full(transfer.total(), Some("\"immutable-v1\"")))
        .unwrap();
    transfer.append(&case.compressed[..1234]).unwrap();
    transfer.checkpoint().unwrap();
    drop(transfer);
    assert!(!ready.exists());
    drop(case.stage);
    case.stage = open_stage(&path);
    assert!(matches!(
        case.stage.status().unwrap(),
        RetainedStatus::Unverified { bytes: 1234, .. }
    ));
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    assert_eq!(transfer.offset(), 1234);
    let range = format!("bytes 1234-{}/{}", transfer.total() - 1, transfer.total());
    transfer
        .accept(ResponseHeaders {
            status: 206,
            length: Some(transfer.total() - 1234),
            range: Some(&range),
            etag: Some("\"immutable-v1\""),
            encoding: Some("identity"),
        })
        .unwrap();
    write_all(&mut transfer, &case.compressed[1234..]);
    let verified = transfer
        .finish(
            &mut case.trust,
            &case.device,
            &ImageHistory::default(),
            || Ok(time()),
        )
        .await
        .unwrap();
    assert_eq!(verified.id(), case.candidate.id());
    assert_eq!(fs::read(&ready).unwrap(), case.compressed);
    assert!(!path.join(PARTIAL).exists());
    drop(case.stage);
    case.stage = open_stage(&path);
    // Disk state cannot mint Ready, even after a clean completed download.
    assert!(
        matches!(case.stage.status().unwrap(), RetainedStatus::Unverified { bytes, total, .. } if bytes == total)
    );
    let transfer = case.stage.prepare(&case.candidate).unwrap();
    assert!(!ready.exists());
    transfer
        .finish(
            &mut case.trust,
            &case.device,
            &ImageHistory::default(),
            || Ok(time()),
        )
        .await
        .unwrap();
    assert!(ready.exists());
}

#[tokio::test]
async fn weak_or_missing_validator_restarts_an_interrupted_representation() {
    for etag in [
        None,
        Some("W/\"weak\""),
        Some("bad\r\nheader"),
        Some("\"good\""),
    ] {
        let mut case = Case::new().await;
        let mut transfer = case.stage.prepare(&case.candidate).unwrap();
        transfer.accept(full(transfer.total(), etag)).unwrap();
        transfer.append(&case.compressed[..123]).unwrap();
        transfer.checkpoint().unwrap();
        drop(transfer);
        let transfer = case.stage.prepare(&case.candidate).unwrap();
        assert_eq!(
            transfer.offset(),
            if etag == Some("\"good\"") { 123 } else { 0 }
        );
    }
}

#[tokio::test]
async fn a_full_response_to_range_replaces_the_prefix_instead_of_appending() {
    let mut case = Case::new().await;
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer
        .accept(full(transfer.total(), Some("\"old\"")))
        .unwrap();
    transfer.append(b"bad prefix").unwrap();
    drop(transfer);
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    assert!(transfer.offset() > 0);
    transfer
        .accept(full(transfer.total(), Some("\"new\"")))
        .unwrap();
    assert_eq!(transfer.offset(), 0);
    write_all(&mut transfer, &case.compressed);
    transfer
        .finish(
            &mut case.trust,
            &case.device,
            &ImageHistory::default(),
            || Ok(time()),
        )
        .await
        .unwrap();
    assert_eq!(fs::read(case.ready()).unwrap(), case.compressed);
}

#[tokio::test]
async fn inconsistent_ranges_cannot_append_or_publish() {
    let mut case = Case::new().await;
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer
        .accept(full(transfer.total(), Some("\"original\"")))
        .unwrap();
    transfer.append(&case.compressed[..12]).unwrap();
    let range = format!("bytes 12-{}/{}", transfer.total() - 1, transfer.total());
    for fault in 0..7 {
        let mut headers = ResponseHeaders {
            status: 206,
            length: Some(transfer.total() - 12),
            range: Some(&range),
            etag: Some("\"original\""),
            encoding: None,
        };
        match fault {
            0 => headers.range = Some("bytes 0-10/11"),
            1 => headers.length = None,
            2 => headers.etag = Some("\"different\""),
            3 => headers.encoding = Some("gzip"),
            4 => headers.status = 416,
            5 => headers.etag = None,
            _ => headers.length = Some(0),
        }
        assert!(transfer.accept(headers).is_err());
        assert!(transfer.append(b"anything").is_err());
        assert_eq!(transfer.offset(), 12);
    }
    drop(transfer);
    assert!(!case.ready().exists());
}

#[tokio::test]
async fn signed_lengths_bound_downloads_and_truncated_files_never_publish() {
    let mut case = Case::new().await;
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    assert!(transfer.append(b"before headers").is_err());
    assert!(transfer.accept(full(transfer.total() + 1, None)).is_err());
    transfer.accept(full(transfer.total(), None)).unwrap();
    assert!(transfer.append(&vec![0; TRANSFER_CHUNK_BYTES + 1]).is_err());
    transfer.accept(full(transfer.total(), None)).unwrap();
    transfer.append(&case.compressed[..10]).unwrap();
    assert!(matches!(
        transfer
            .finish(
                &mut case.trust,
                &case.device,
                &ImageHistory::default(),
                || Ok(time())
            )
            .await,
        Err(Error::ImageMismatch)
    ));
    assert!(!case.ready().exists());
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer.accept(full(transfer.total(), None)).unwrap();
    write_all(&mut transfer, &case.compressed);
    assert!(transfer.append(b"trailing").is_err());
    assert!(
        transfer
            .finish(
                &mut case.trust,
                &case.device,
                &ImageHistory::default(),
                || Ok(time())
            )
            .await
            .is_err()
    );
    assert!(!case.ready().exists());
}

#[tokio::test]
async fn corrupt_resumed_prefix_is_detected_by_the_complete_digest() {
    let mut case = Case::new().await;
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer
        .accept(full(transfer.total(), Some("\"same\"")))
        .unwrap();
    let mut corrupt = case.compressed.clone();
    corrupt[1] ^= 1;
    write_all(&mut transfer, &corrupt);
    assert!(matches!(
        transfer
            .finish(
                &mut case.trust,
                &case.device,
                &ImageHistory::default(),
                || Ok(time())
            )
            .await,
        Err(Error::ImageMismatch)
    ));
    assert!(!case.ready().exists());
    assert_eq!(case.stage.prepare(&case.candidate).unwrap().offset(), 0);
}

#[tokio::test]
async fn expanded_hash_length_and_single_frame_are_checked_before_publication() {
    for fault in 0..6 {
        let mut case = Case::alter(|image, compressed| match fault {
            0 => image.expanded.sha256 = Sha256::digest(b"wrong expanded hash"),
            1 => image.expanded.bytes += 1,
            2 => image.expanded.bytes -= 1,
            3 => compressed.extend_from_slice(
                &zstd::stream::encode_all(b"second frame".as_slice(), 1).unwrap(),
            ),
            4 => compressed.extend_from_slice(b"trailing junk"),
            _ => {
                compressed.pop();
            }
        })
        .await;
        let mut transfer = case.stage.prepare(&case.candidate).unwrap();
        transfer.accept(full(transfer.total(), None)).unwrap();
        write_all(&mut transfer, &case.compressed);
        assert!(
            matches!(
                transfer
                    .finish(
                        &mut case.trust,
                        &case.device,
                        &ImageHistory::default(),
                        || Ok(time())
                    )
                    .await,
                Err(Error::ImageMismatch)
            ),
            "fault {fault}"
        );
        assert!(!case.ready().exists());
    }
}

#[tokio::test]
async fn changed_channel_and_generation_discard_old_prefixes() {
    let mut case = Case::new().await;
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer
        .accept(full(transfer.total(), Some("\"old\"")))
        .unwrap();
    transfer.append(&case.compressed[..123]).unwrap();
    drop(transfer);
    let mut newer = case.candidate.image().clone();
    newer.channel = Channel::Nightly;
    case.replace_feed(&[newer]).await;
    let selected = candidate(
        case.trust
            .cached(
                time(),
                Channel::Nightly,
                &case.device,
                &ImageHistory::default(),
            )
            .await
            .unwrap(),
    );
    let transfer = case.stage.prepare(&selected).unwrap();
    assert_eq!(transfer.offset(), 0);
    assert!(transfer.etag().is_none());
    drop(transfer);
    case.stage.clear().unwrap();
    assert!(matches!(
        case.stage.status().unwrap(),
        RetainedStatus::Empty
    ));
    assert!(!case.path().join(PARTIAL).exists());
}

#[tokio::test]
async fn revoked_or_superseded_metadata_and_changed_device_cannot_publish() {
    for fault in 0..4 {
        let mut case = Case::new().await;
        match fault {
            0 => case.replace_feed(&[]).await,
            1 => {
                let mut newer = case.candidate.image().clone();
                newer.generation += 1;
                newer.compressed.sha256 = Sha256::digest(b"new asset");
                case.replace_feed(&[newer]).await;
            }
            2 => case.device.boot_identity = Sha256::digest(b"new boot assets"),
            _ => case.device.inactive_slot_bytes = 10,
        }
        let mut transfer = case.stage.prepare(&case.candidate).unwrap();
        transfer.accept(full(transfer.total(), None)).unwrap();
        write_all(&mut transfer, &case.compressed);
        assert!(
            transfer
                .finish(
                    &mut case.trust,
                    &case.device,
                    &ImageHistory::default(),
                    || Ok(time())
                )
                .await
                .is_err()
        );
        assert!(!case.ready().exists());
    }
}

#[tokio::test]
async fn unknown_expired_and_backwards_time_never_publish() {
    for timestamp in [None, Some(EXPIRES), Some("2026-10-13T00:00:00Z")] {
        let mut case = Case::new().await;
        let mut transfer = case.stage.prepare(&case.candidate).unwrap();
        transfer.accept(full(transfer.total(), None)).unwrap();
        write_all(&mut transfer, &case.compressed);
        let result = transfer
            .finish(
                &mut case.trust,
                &case.device,
                &ImageHistory::default(),
                || Ok(timestamp.map(|t| UsableTime(t.parse().unwrap()))),
            )
            .await;
        assert!(result.is_err());
        assert!(!case.ready().exists());
    }
}

#[tokio::test]
async fn cancellation_during_verification_leaves_no_installable_name() {
    let mut case = Case::new().await;
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer.accept(full(transfer.total(), None)).unwrap();
    write_all(&mut transfer, &case.compressed);
    let mut checks = 0;
    let result = transfer
        .finish(
            &mut case.trust,
            &case.device,
            &ImageHistory::default(),
            || {
                checks += 1;
                if checks == 3 {
                    Err(Error::Staging("cancelled by test"))
                } else {
                    Ok(time())
                }
            },
        )
        .await;
    assert!(matches!(result, Err(Error::Staging("cancelled by test"))));
    assert!(!case.ready().exists());
    assert!(case.path().join(PARTIAL).exists());
}

#[tokio::test]
async fn free_space_is_checked_before_transfer_during_progress_and_before_ready() {
    let mut case = Case::new().await;
    let unrelated = case.directory.path().join("rom-data");
    fs::write(&unrelated, b"leave me alone").unwrap();
    case.stage.available_limit = Some(STAGING_RESERVE_BYTES);
    assert!(matches!(
        case.stage.prepare(&case.candidate),
        Err(Error::StagingSpace)
    ));
    case.stage.available_limit = None;
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer.accept(full(transfer.total(), None)).unwrap();
    transfer.store.available_limit = Some(STAGING_RESERVE_BYTES);
    assert!(matches!(
        transfer.append(&case.compressed[..10]),
        Err(Error::StagingSpace)
    ));
    transfer.store.available_limit = None;
    transfer.accept(full(transfer.total(), None)).unwrap();
    write_all(&mut transfer, &case.compressed);
    transfer.store.available_limit = Some(STAGING_RESERVE_BYTES - 1);
    assert!(matches!(
        transfer
            .finish(
                &mut case.trust,
                &case.device,
                &ImageHistory::default(),
                || Ok(time())
            )
            .await,
        Err(Error::StagingSpace)
    ));
    assert!(!case.ready().exists());
    assert_eq!(fs::read(unrelated).unwrap(), b"leave me alone");
}

#[tokio::test]
async fn exclusive_lock_also_blocks_the_installers_shared_lock() {
    let case = Case::new().await;
    let owner = unsafe { libc::geteuid() };
    let directory = open_directory(&case.path(), owner).unwrap();
    assert!(matches!(staging_lock(&directory, owner), Err(Error::Busy)));
    assert!(matches!(
        StagingStore::under(directory, owner),
        Err(Error::Busy)
    ));
    let path = case.path();
    drop(case.stage);
    let directory = open_directory(&path, owner).unwrap();
    let installer_lock = staging_lock(&directory, owner).unwrap();
    assert!(matches!(
        StagingStore::under(directory, owner),
        Err(Error::Busy)
    ));
    drop(installer_lock);
    open_stage(&path);
}

#[tokio::test]
async fn unsafe_and_unknown_files_are_rejected_without_deleting_other_data() {
    for fault in 0..5 {
        let case = Case::new().await;
        let path = case.path();
        let victim = case.directory.path().join("saved-settings");
        fs::write(&victim, b"settings").unwrap();
        fs::set_permissions(&victim, fs::Permissions::from_mode(0o600)).unwrap();
        drop(case.stage);
        match fault {
            0 => symlink(&victim, path.join(PARTIAL)).unwrap(),
            1 => fs::hard_link(&victim, path.join(PARTIAL)).unwrap(),
            2 => {
                fs::write(path.join(PARTIAL), b"bad modes").unwrap();
                fs::set_permissions(path.join(PARTIAL), fs::Permissions::from_mode(0o644)).unwrap();
            }
            3 => fs::create_dir(path.join(PARTIAL)).unwrap(),
            _ => fs::write(path.join("unrelated"), b"never delete").unwrap(),
        }
        let owner = unsafe { libc::geteuid() };
        assert!(StagingStore::under(open_directory(&path, owner).unwrap(), owner).is_err());
        assert_eq!(fs::read(victim).unwrap(), b"settings");
        if fault == 4 {
            assert_eq!(fs::read(path.join("unrelated")).unwrap(), b"never delete");
        }
    }
}

#[tokio::test]
async fn interrupted_record_publication_retains_old_candidate_and_discards_only_temp_record() {
    let mut case = Case::new().await;
    let path = case.path();
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer
        .accept(full(transfer.total(), Some("\"keep\"")))
        .unwrap();
    transfer.append(&case.compressed[..99]).unwrap();
    transfer.checkpoint().unwrap();
    drop(transfer);
    drop(case.stage);
    fs::write(path.join(NEXT), b"interrupted new JSON").unwrap();
    fs::set_permissions(path.join(NEXT), fs::Permissions::from_mode(0o600)).unwrap();
    case.stage = open_stage(&path);
    assert!(!path.join(NEXT).exists());
    assert_eq!(case.stage.prepare(&case.candidate).unwrap().offset(), 99);
}

#[tokio::test]
async fn corrupt_record_requires_repair_and_does_not_erase_a_partial() {
    let mut case = Case::new().await;
    let path = case.path();
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer
        .accept(full(transfer.total(), Some("\"keep\"")))
        .unwrap();
    transfer.append(b"prefix").unwrap();
    drop(transfer);
    drop(case.stage);
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join(RECORD)).unwrap()).unwrap();
    json["record"]["etag"] = serde_json::json!("\"changed\"");
    fs::write(path.join(RECORD), serde_json::to_vec(&json).unwrap()).unwrap();
    let owner = unsafe { libc::geteuid() };
    assert!(StagingStore::under(open_directory(&path, owner).unwrap(), owner).is_err());
    assert_eq!(fs::read(path.join(PARTIAL)).unwrap(), b"prefix");
}

async fn publication_check_count() -> usize {
    let mut case = Case::new().await;
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer.accept(full(transfer.total(), None)).unwrap();
    write_all(&mut transfer, &case.compressed);
    let mut checks = 0;
    transfer
        .finish(
            &mut case.trust,
            &case.device,
            &ImageHistory::default(),
            || {
                checks += 1;
                Ok(time())
            },
        )
        .await
        .unwrap();
    checks
}

#[tokio::test]
async fn cancellation_at_every_verification_boundary_preserves_the_partial_and_error() {
    let total = publication_check_count().await;
    for stop in 1..=total {
        let mut case = Case::new().await;
        let mut transfer = case.stage.prepare(&case.candidate).unwrap();
        transfer.accept(full(transfer.total(), None)).unwrap();
        write_all(&mut transfer, &case.compressed);
        let mut checks = 0;
        let result = transfer
            .finish(
                &mut case.trust,
                &case.device,
                &ImageHistory::default(),
                || {
                    checks += 1;
                    if checks == stop {
                        Err(Error::Staging("session cancelled"))
                    } else {
                        Ok(time())
                    }
                },
            )
            .await;
        assert!(
            matches!(result, Err(Error::Staging("session cancelled"))),
            "checkpoint {stop}: {result:?}"
        );
        assert!(!case.ready().exists());
        assert_eq!(
            fs::metadata(case.path().join(PARTIAL)).unwrap().len(),
            case.compressed.len() as u64
        );
    }
}

#[tokio::test]
async fn expiry_at_publication_is_persistent_across_restart_and_clock_rollback() {
    let total = publication_check_count().await;
    let mut case = Case::new().await;
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer.accept(full(transfer.total(), None)).unwrap();
    write_all(&mut transfer, &case.compressed);
    let mut checks = 0;
    let result = transfer
        .finish(
            &mut case.trust,
            &case.device,
            &ImageHistory::default(),
            || {
                checks += 1;
                if checks == total {
                    Ok(Some(UsableTime(EXPIRES.parse().unwrap())))
                } else {
                    Ok(time())
                }
            },
        )
        .await;
    assert!(matches!(
        result,
        Err(Error::Staging("metadata expired before publication"))
    ));
    assert!(!case.ready().exists());
    let trust_path = case.directory.path().join("trust");
    drop(case.trust);
    case.trust = TrustStore::open(&trust_path).unwrap();
    assert!(matches!(
        case.trust
            .cached(
                time(),
                Channel::Release,
                &case.device,
                &ImageHistory::default()
            )
            .await,
        Err(Error::TimeMovedBackwards)
    ));
}

#[tokio::test]
async fn replacement_at_the_last_guard_cannot_publish_an_open_but_detached_inode() {
    let total = publication_check_count().await;
    let mut case = Case::new().await;
    let path = case.path();
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer.accept(full(transfer.total(), None)).unwrap();
    write_all(&mut transfer, &case.compressed);
    let mut checks = 0;
    let result = transfer
        .finish(
            &mut case.trust,
            &case.device,
            &ImageHistory::default(),
            || {
                checks += 1;
                if checks == total {
                    fs::rename(path.join(PARTIAL), path.join("detached")).unwrap();
                    fs::write(path.join(PARTIAL), &case.compressed).unwrap();
                    fs::set_permissions(path.join(PARTIAL), fs::Permissions::from_mode(0o600))
                        .unwrap();
                }
                Ok(time())
            },
        )
        .await;
    assert!(matches!(
        result,
        Err(Error::Staging("payload changed before publication"))
    ));
    assert!(!case.ready().exists());
}

#[tokio::test]
async fn a_newer_signed_generation_removes_the_previous_complete_candidate() {
    let mut case = Case::new().await;
    let old_path = case.ready();
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer.accept(full(transfer.total(), None)).unwrap();
    write_all(&mut transfer, &case.compressed);
    transfer
        .finish(
            &mut case.trust,
            &case.device,
            &ImageHistory::default(),
            || Ok(time()),
        )
        .await
        .unwrap();
    assert!(old_path.exists());
    let mut newer = case.candidate.image().clone();
    newer.generation += 1;
    newer.compressed.sha256 = Sha256::digest(b"distinct new payload");
    case.replace_feed(&[newer]).await;
    let selected = candidate(
        case.trust
            .cached(
                time(),
                Channel::Release,
                &case.device,
                &ImageHistory::default(),
            )
            .await
            .unwrap(),
    );
    let transfer = case.stage.prepare(&selected).unwrap();
    assert_eq!(transfer.offset(), 0);
    assert!(!old_path.exists());
    drop(transfer);
    let names: Vec<_> = fs::read_dir(case.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.len(), 3);
    for expected in [LOCK, RECORD, PARTIAL] {
        assert!(names.iter().any(|name| name == expected));
    }
}

#[tokio::test]
async fn server_validators_stay_private_and_never_make_a_partial_executable() {
    let mut case = Case::new().await;
    let mut transfer = case.stage.prepare(&case.candidate).unwrap();
    transfer
        .accept(full(transfer.total(), Some("\"private-server-validator\"")))
        .unwrap();
    transfer.append(b"part").unwrap();
    drop(transfer);
    let status = serde_json::to_string(&case.stage.status().unwrap()).unwrap();
    assert!(!status.contains("private-server-validator"));
    assert!(!status.contains("etag"));
    assert_eq!(
        fs::metadata(case.path().join(PARTIAL)).unwrap().mode() & 0o777,
        0o600
    );
    fs::set_permissions(case.path().join(PARTIAL), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(case.stage.prepare(&case.candidate).is_err());
}
