use super::*;
use crate::ImageDescriptor;
use crate::test_support::{self as support, Fault, Fixture};

async fn package(fixture: &Fixture, images: &[ImageDescriptor], fault: Fault) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let metadata = fixture
        .publish(1, images, support::EXPIRES.parse().unwrap(), fault)
        .await;
    for (name, bytes) in metadata.files {
        fs::write(directory.path().join(name), bytes).unwrap();
    }
    for image in images {
        fs::write(
            directory.path().join(format!(
                "{}.{}",
                image.compressed.sha256,
                image.target_name()
            )),
            support::payload(image.generation),
        )
        .unwrap();
    }
    directory
}

async fn check(fixture: &Fixture, directory: &Path) -> Result<DeliveryManifest> {
    inventory(&fixture.root_bytes, directory, support::time().unwrap()).await
}

#[tokio::test]
async fn exact_inventory_accepts_release_promotion_and_both_boot_generations() {
    let fixture = Fixture::new().await;
    let release = support::descriptor(Channel::Release, 1);
    let mut promoted = release.clone();
    promoted.channel = Channel::Nightly;
    let mut newer = support::descriptor(Channel::Nightly, 2);
    newer.boot_identity = Sha256::digest(b"new boot assets");
    let directory = package(&fixture, &[release.clone(), promoted, newer], Fault::None).await;
    let plan = check(&fixture, directory.path()).await.unwrap();
    assert_eq!(plan.timestamp_version, 1);
    assert_eq!(
        plan.trusted_root_sha256,
        Sha256::digest(&fixture.root_bytes)
    );
    assert_eq!(plan.files.len(), 9);
    let name = format!("{}.{}", release.compressed.sha256, release.target_name());
    assert_eq!(plan.files[&name], release.compressed);
    for (name, blob) in &plan.files {
        let bytes = fs::read(directory.path().join(name)).unwrap();
        assert_eq!(blob.sha256, Sha256::digest(&bytes));
        assert_eq!(blob.bytes, bytes.len() as u64);
    }
}

#[tokio::test]
async fn root_rotation_requires_the_exact_complete_supported_chain() {
    let mut fixture = Fixture::new().await;
    fixture.rotate_root().await;
    let directory = package(&fixture, &[], Fault::None).await;
    assert!(check(&fixture, directory.path()).await.is_ok());
    fs::remove_file(directory.path().join("1.root.json")).unwrap();
    assert!(check(&fixture, directory.path()).await.is_err());
    fs::write(directory.path().join("1.root.json"), &fixture.root_bytes).unwrap();
    fs::write(directory.path().join("3.root.json"), &fixture.root_bytes).unwrap();
    assert!(check(&fixture, directory.path()).await.is_err());
}

#[tokio::test]
async fn incomplete_mixed_and_unreferenced_metadata_cannot_make_a_manifest() {
    let fixture = Fixture::new().await;
    let directory = package(&fixture, &[], Fault::None).await;
    let snapshot = directory.path().join("1.snapshot.json");
    let bytes = fs::read(&snapshot).unwrap();
    fs::remove_file(&snapshot).unwrap();
    assert!(check(&fixture, directory.path()).await.is_err());
    fs::write(&snapshot, &bytes).unwrap();
    let other = fixture
        .publish(2, &[], support::EXPIRES.parse().unwrap(), Fault::None)
        .await;
    fs::write(
        directory.path().join("timestamp.json"),
        &other.files["timestamp.json"],
    )
    .unwrap();
    assert!(check(&fixture, directory.path()).await.is_err());
    let directory = package(&fixture, &[], Fault::None).await;
    fs::write(directory.path().join("42.snapshot.json"), bytes).unwrap();
    assert!(check(&fixture, directory.path()).await.is_err());
}

#[tokio::test]
async fn signatures_namespaces_and_channel_authorities_are_checked_by_the_device_verifier() {
    let fixture = Fixture::new().await;
    let image = support::descriptor(Channel::Release, 1);
    for fault in [
        Fault::MissingReleaseSignature,
        Fault::RepeatedReleaseSignature,
        Fault::WrongReleaseSignature,
        Fault::CrossChannelName,
        Fault::UnsafeName,
        Fault::DescriptorHash,
        Fault::ExtraDelegation,
        Fault::SharedChannelKey,
        Fault::UnboundSnapshotReference,
        Fault::ExtraSnapshotReference,
    ] {
        let directory = package(&fixture, std::slice::from_ref(&image), fault).await;
        assert!(check(&fixture, directory.path()).await.is_err());
    }
    let directory = package(&fixture, &[image], Fault::None).await;
    let untrusted = Fixture::new().await;
    assert!(check(&untrusted, directory.path()).await.is_err());
}

#[tokio::test]
async fn expired_metadata_is_not_accepted_for_delivery() {
    let fixture = Fixture::new().await;
    let directory = package(&fixture, &[], Fault::None).await;
    let now = UsableTime("2026-10-22T00:00:00Z".parse().unwrap());
    assert!(
        inventory(&fixture.root_bytes, directory.path(), now)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn payload_length_hash_names_and_file_set_are_bound_to_signed_metadata() {
    let fixture = Fixture::new().await;
    let image = support::descriptor(Channel::Nightly, 1);
    let directory = package(&fixture, std::slice::from_ref(&image), Fault::None).await;
    let path = directory.path().join(format!(
        "{}.{}",
        image.compressed.sha256,
        image.target_name()
    ));
    let bytes = fs::read(&path).unwrap();
    for changed in [
        bytes[..bytes.len() - 1].to_vec(),
        [bytes.as_slice(), b"x"].concat(),
        vec![0; bytes.len()],
    ] {
        fs::write(&path, changed).unwrap();
        assert!(check(&fixture, directory.path()).await.is_err());
    }
    fs::write(&path, &bytes).unwrap();
    fs::write(directory.path().join("unused.ext4.zst"), b"unused").unwrap();
    assert!(check(&fixture, directory.path()).await.is_err());
    fs::remove_file(directory.path().join("unused.ext4.zst")).unwrap();
    fs::rename(path, directory.path().join(image.target_name())).unwrap();
    assert!(check(&fixture, directory.path()).await.is_err());
}

#[tokio::test]
async fn promotion_cannot_reassign_generation_or_change_immutable_descriptor() {
    let fixture = Fixture::new().await;
    let release = support::descriptor(Channel::Release, 1);
    let mut nightly = release.clone();
    nightly.channel = Channel::Nightly;
    nightly.min_updater_protocol += 1;
    let directory = package(&fixture, &[release.clone(), nightly], Fault::None).await;
    assert!(check(&fixture, directory.path()).await.is_err());
    let mut nightly = release.clone();
    nightly.channel = Channel::Nightly;
    nightly.generation += 1;
    let directory = package(&fixture, &[release, nightly.clone()], Fault::None).await;
    let name = format!("{}.{}", nightly.compressed.sha256, nightly.target_name());
    fs::write(directory.path().join(name), support::payload(1)).unwrap();
    assert!(check(&fixture, directory.path()).await.is_err());
}

#[tokio::test]
async fn size_limits_and_unknown_metadata_names_fail_before_tuf_processing() {
    let fixture = Fixture::new().await;
    for name in [
        "../x",
        "01.root.json",
        "0.root.json",
        "root.json",
        "1.timestamp.json",
        "1.other.json",
    ] {
        assert!(!metadata_name(name));
    }
    let directory = package(&fixture, &[], Fault::None).await;
    fs::write(
        directory.path().join("timestamp.json"),
        vec![b'x'; MAX_METADATA_BYTES + 1],
    )
    .unwrap();
    assert!(check(&fixture, directory.path()).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn symlinks_and_directories_are_not_publication_assets() {
    let fixture = Fixture::new().await;
    let directory = package(&fixture, &[], Fault::None).await;
    std::os::unix::fs::symlink("timestamp.json", directory.path().join("link")).unwrap();
    assert!(check(&fixture, directory.path()).await.is_err());
    fs::remove_file(directory.path().join("link")).unwrap();
    fs::create_dir(directory.path().join("nested")).unwrap();
    assert!(check(&fixture, directory.path()).await.is_err());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn signed_inventory_is_delivered_through_the_mock_github_transaction() {
    let fixture = Fixture::new().await;
    let directory = package(
        &fixture,
        &[
            support::descriptor(Channel::Release, 1),
            support::descriptor(Channel::Nightly, 2),
        ],
        Fault::None,
    )
    .await;
    let manifest = check(&fixture, directory.path()).await.unwrap();
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("manifest.json");
    fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    let helper =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/ci/test_ota_delivery.py");
    let result = std::process::Command::new("python3")
        .arg(helper)
        .arg("--signed-fixture")
        .arg(directory.path())
        .arg(path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
