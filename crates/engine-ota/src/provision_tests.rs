use super::*;
use crate::test_support::{self as feed, Fixture};
use crate::{Channel, ImageHistory, UsableTime};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::sync::Arc;

struct Case {
    temp: tempfile::TempDir,
    system: File,
    owner: u32,
    bundle: Bundle,
    plan: Plan,
    fixture: Fixture,
}
impl Case {
    async fn new() -> Self {
        let fixture = Fixture::new().await;
        let payload = bundle_bytes(&fixture);
        let bundle = Bundle::parse(&payload, Sha256::digest(&payload)).unwrap();
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let owner = unsafe { libc::geteuid() };
        let system = open_directory(temp.path(), owner).unwrap();
        let plan = Plan {
            bundle: bundle.fingerprint,
            root: Sha256::digest(&bundle.root),
            location: bundle.location.clone(),
            binding: BootBinding {
                device_id: Sha256::digest(b"board"),
                layout_id: Sha256::digest(b"layout"),
                boot_identity: Sha256::digest(b"boot"),
                hardware_profile: Sha256::digest(b"display"),
            },
            baseline: Sha256::digest(b"confirmed baseline"),
            initial_data_version: 1,
        };
        Self {
            temp,
            system,
            owner,
            bundle,
            plan,
            fixture,
        }
    }
    fn run(&self, checkpoint: impl FnMut(&str) -> Result<()>) -> Result<Status> {
        provision_under(
            &self.system,
            self.owner,
            &self.bundle,
            self.plan.clone(),
            true,
            || Ok(()),
            checkpoint,
        )
    }
    fn status(&self) -> Result<Status> {
        inspect_under(&self.system, self.owner, Some(&self.plan.binding))
    }
    fn path(&self, relative: &str) -> PathBuf {
        self.temp.path().join(relative)
    }
    fn private_write(&self, path: &str, bytes: &[u8]) {
        fs::write(self.path(path), bytes).unwrap();
        fs::set_permissions(self.path(path), fs::Permissions::from_mode(0o600)).unwrap();
    }
}
fn bundle_bytes(fixture: &Fixture) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "root": serde_json::from_slice::<serde_json::Value>(&fixture.root_bytes).unwrap(),
        "location": feed::location(),
    }))
    .unwrap()
}

#[tokio::test]
async fn explicit_setup_publishes_usable_trust_and_preserves_unrelated_data() {
    let case = Case::new().await;
    case.private_write("unrelated", b"settings remain");
    assert!(case.status().is_err());
    assert_eq!(case.run(|_| Ok(())).unwrap().persistent_schema, 1);
    assert_eq!(case.status().unwrap().bundle, case.bundle.fingerprint);
    assert!(!case.path(PENDING).exists());
    let mut trust = TrustStore::open(&case.path("updates/trust")).unwrap();
    let published = case
        .fixture
        .publish(
            1,
            &[feed::descriptor(Channel::Release, 1)],
            feed::EXPIRES.parse().unwrap(),
            feed::Fault::None,
        )
        .await;
    assert!(matches!(
        trust
            .refresh(
                Arc::new(published),
                feed::time(),
                Channel::Release,
                &feed::device(),
                &ImageHistory::default()
            )
            .await
            .unwrap(),
        crate::Selection::Available { .. }
    ));
    drop(trust);
    let stage = crate::staging_linux::StagingStore::under(
        File::open(case.path("updates/staging")).unwrap(),
        case.owner,
    )
    .unwrap();
    assert!(matches!(
        stage.status().unwrap(),
        crate::staging_linux::RetainedStatus::Empty
    ));
    assert_eq!(
        fs::read(case.path("unrelated")).unwrap(),
        b"settings remain"
    );
    for path in [
        "update-provision.json",
        "updates/data-schema.json",
        "updates/trust/state.json",
    ] {
        let metadata = fs::metadata(case.path(path)).unwrap();
        assert_eq!(metadata.uid(), case.owner);
        assert_eq!(metadata.mode() & 0o777, 0o600);
    }
}

#[tokio::test]
async fn every_persistence_boundary_resumes_without_exposing_partial_setup() {
    let reference = Case::new().await;
    let mut steps = Vec::new();
    reference
        .run(|point| {
            steps.push(point.to_owned());
            Ok(())
        })
        .unwrap();
    assert!(steps.len() > 15);
    for (stopped, point) in steps.iter().enumerate() {
        let case = Case::new().await;
        let mut step = 0;
        assert!(
            case.run(|_| {
                let current = step;
                step += 1;
                if current == stopped {
                    Err(invalid("injected interruption"))
                } else {
                    Ok(())
                }
            })
            .is_err(),
            "{stopped}"
        );
        let complete = load_record(&case.system, case.owner)
            .unwrap()
            .is_some_and(|r| r.phase == Phase::Complete);
        assert_eq!(case.status().is_ok(), complete, "{stopped}: {}", point);
        case.run(|_| Ok(()))
            .unwrap_or_else(|e| panic!("{stopped}: {point}: {e}"));
        let trust = fs::read(case.path("updates/trust/state.json")).unwrap();
        assert_eq!(trust, case.bundle.initial_trust);
        assert_eq!(case.status().unwrap().persistent_schema, 1);
        let inode = fs::metadata(case.path(RECEIPT)).unwrap().ino();
        case.run(|_| panic!("completed setup must not write"))
            .unwrap();
        assert_eq!(fs::metadata(case.path(RECEIPT)).unwrap().ino(), inode);
    }
}

#[tokio::test]
async fn changed_requests_and_existing_untracked_files_cannot_establish_new_authority() {
    for existing in [
        "updates",
        ".updates-pending",
        "trust",
        "staging",
        "data-schema.json",
    ] {
        let case = Case::new().await;
        case.private_write(existing, b"existing state");
        assert!(case.run(|_| Ok(())).is_err());
        assert!(!case.path(RECEIPT).exists());
        assert_eq!(fs::read(case.path(existing)).unwrap(), b"existing state");
    }
    for field in ["root", "bundle", "location", "binding", "baseline", "data"] {
        let mut case = Case::new().await;
        assert!(
            case.run(|p| if p == "receipt-synced" {
                Err(invalid("stop"))
            } else {
                Ok(())
            })
            .is_err()
        );
        let before = fs::read(case.path(RECEIPT)).unwrap();
        match field {
            "root" => case.plan.root = Sha256::digest(b"new root"),
            "bundle" => case.plan.bundle = Sha256::digest(b"new bundle"),
            "location" => case.plan.location.metadata.set_path("/different/"),
            "binding" => case.plan.binding.device_id = Sha256::digest(b"other board"),
            "baseline" => case.plan.baseline = Sha256::digest(b"different baseline"),
            _ => case.plan.initial_data_version = 2,
        }
        assert!(case.run(|_| Ok(())).is_err(), "{field}");
        assert_eq!(fs::read(case.path(RECEIPT)).unwrap(), before);
        assert!(!case.path(UPDATES).exists());
    }
    let case = Case::new().await;
    assert!(
        provision_under(
            &case.system,
            case.owner,
            &case.bundle,
            case.plan.clone(),
            false,
            || Ok(()),
            |_| Ok(())
        )
        .is_err()
    );
    assert!(!case.path(RECEIPT).exists());
}

#[tokio::test]
async fn missing_corrupt_or_unsafe_completed_state_is_never_reinitialized() {
    for target in [
        "update-provision.json",
        "updates/data-schema.json",
        "updates/trust/state.json",
    ] {
        for fault in ["missing", "corrupt", "public", "link", "symlink"] {
            let case = Case::new().await;
            case.run(|_| Ok(())).unwrap();
            let path = case.path(target);
            match fault {
                "missing" => fs::remove_file(&path).unwrap(),
                "corrupt" => fs::write(&path, b"corrupt").unwrap(),
                "public" => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
                "link" => fs::hard_link(&path, case.path("link")).unwrap(),
                _ => {
                    fs::rename(&path, case.path("elsewhere")).unwrap();
                    symlink(case.path("elsewhere"), &path).unwrap();
                }
            }
            assert!(case.run(|_| Ok(())).is_err(), "{target}/{fault}");
            if fault == "missing" {
                assert!(!path.exists());
            }
        }
    }
    let case = Case::new().await;
    case.run(|_| Ok(())).unwrap();
    fs::rename(case.path(UPDATES), case.path("saved-updates")).unwrap();
    assert!(case.run(|_| Ok(())).is_err());
    assert!(!case.path(UPDATES).exists());
    assert!(case.status().is_err());
}

#[tokio::test]
async fn repeated_setup_preserves_rotated_root_replay_and_time_floors() {
    let mut case = Case::new().await;
    case.run(|_| Ok(())).unwrap();
    case.fixture.rotate_root().await;
    let published = case
        .fixture
        .publish(
            3,
            &[feed::descriptor(Channel::Nightly, 7)],
            feed::EXPIRES.parse().unwrap(),
            feed::Fault::None,
        )
        .await;
    let path = case.path("updates/trust");
    let later = UsableTime("2026-10-14T13:00:00Z".parse().unwrap());
    let mut trust = TrustStore::open(&path).unwrap();
    trust
        .refresh(
            Arc::new(published),
            Some(later),
            Channel::Nightly,
            &feed::device(),
            &ImageHistory::default(),
        )
        .await
        .unwrap();
    drop(trust);
    let before = fs::read(path.join("state.json")).unwrap();
    let inode = fs::metadata(path.join("state.json")).unwrap().ino();
    case.run(|_| panic!("setup retry must not mutate trust"))
        .unwrap();
    assert_eq!(fs::read(path.join("state.json")).unwrap(), before);
    assert_eq!(fs::metadata(path.join("state.json")).unwrap().ino(), inode);
    let mut trust = TrustStore::open(&path).unwrap();
    assert!(matches!(
        trust
            .cached(
                feed::time(),
                Channel::Nightly,
                &feed::device(),
                &ImageHistory::default()
            )
            .await,
        Err(Error::TimeMovedBackwards)
    ));
    assert!(matches!(
        trust
            .cached(
                Some(later),
                Channel::Nightly,
                &feed::device(),
                &ImageHistory::default()
            )
            .await
            .unwrap(),
        crate::Selection::Available { .. }
    ));
}

#[tokio::test]
async fn data_schema_is_checksumbound_and_cannot_regress_or_move_between_devices() {
    let mut case = Case::new().await;
    case.plan.initial_data_version = 2;
    case.run(|_| Ok(())).unwrap();
    for version in [0, 1] {
        case.private_write(
            "updates/data-schema.json",
            &bytes(&DataVersion { schema: 1, version }).unwrap(),
        );
        assert!(case.status().is_err());
    }
    case.private_write(
        "updates/data-schema.json",
        &bytes(&DataVersion {
            schema: 1,
            version: 3,
        })
        .unwrap(),
    );
    assert_eq!(case.status().unwrap().persistent_schema, 3);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(case.path("updates/data-schema.json")).unwrap()).unwrap();
    value["record"]["version"] = 4.into();
    case.private_write(
        "updates/data-schema.json",
        &serde_json::to_vec(&value).unwrap(),
    );
    assert!(case.status().is_err());
    case.private_write(
        "updates/data-schema.json",
        &bytes(&DataVersion {
            schema: 1,
            version: 3,
        })
        .unwrap(),
    );
    case.plan.binding.device_id = Sha256::digest(b"another device");
    assert!(case.status().is_err());
}

#[tokio::test]
async fn invalid_bundle_is_rejected_before_a_store_can_be_created() {
    let case = Case::new().await;
    let original = bundle_bytes(&case.fixture);
    assert!(Bundle::parse(&original, Sha256::digest(b"unselected")).is_err());
    for field in [
        "schema",
        "signature",
        "root",
        "url",
        "url_size",
        "private",
        "extra",
    ] {
        let mut json: serde_json::Value = serde_json::from_slice(&original).unwrap();
        match field {
            "schema" => json["schema"] = 2.into(),
            "signature" => json["root"]["signatures"] = serde_json::json!([]),
            "root" => json["root"] = serde_json::json!({}),
            "url" => json["location"]["metadata"] = "https://secret:token@example.org/".into(),
            "url_size" => {
                json["location"]["metadata"] =
                    format!("https://example.org/{}/", "x".repeat(4096)).into()
            }
            "private" => {
                let key = json["root"]["signed"]["keys"]
                    .as_object_mut()
                    .unwrap()
                    .values_mut()
                    .next()
                    .unwrap();
                key["keyval"]["private"] = "must not be embedded".into();
            }
            _ => json["command"] = "anything".into(),
        }
        let bytes = serde_json::to_vec(&json).unwrap();
        assert!(
            Bundle::parse(&bytes, Sha256::digest(&bytes)).is_err(),
            "{field}"
        );
    }
    let oversized = vec![b' '; MAX_BUNDLE_BYTES as usize + 1];
    assert!(Bundle::parse(&oversized, Sha256::digest(&oversized)).is_err());
    assert!(!case.path(RECEIPT).exists());
}

#[tokio::test]
async fn changed_guard_or_pending_contents_never_publish_a_completed_setup() {
    for point in ["directory-synced", "directory-published"] {
        let case = Case::new().await;
        let denied = std::cell::Cell::new(false);
        assert!(
            provision_under(
                &case.system,
                case.owner,
                &case.bundle,
                case.plan.clone(),
                true,
                || if denied.get() {
                    Err(invalid("changed boot"))
                } else {
                    Ok(())
                },
                |p| {
                    if p == point {
                        denied.set(true);
                    }
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(case.status().is_err());
        case.run(|_| Ok(())).unwrap();
    }
    for fault in ["unknown", "data", "trust", "duplicate", "symlink"] {
        let case = Case::new().await;
        assert!(
            case.run(|p| if p == "directory-synced" {
                Err(invalid("stop"))
            } else {
                Ok(())
            })
            .is_err()
        );
        match fault {
            "unknown" => case.private_write(".updates-pending/other", b"keep"),
            "data" => case.private_write(".updates-pending/data-schema.json", b"changed"),
            "trust" => case.private_write(".updates-pending/trust/state.json", b"changed"),
            "duplicate" => fs::create_dir(case.path(UPDATES)).unwrap(),
            _ => {
                fs::remove_file(case.path(".updates-pending/data-schema.json")).unwrap();
                symlink("../outside", case.path(".updates-pending/data-schema.json")).unwrap();
            }
        }
        assert!(case.run(|_| Ok(())).is_err(), "{fault}");
        assert!(case.status().is_err());
    }
}

#[test]
fn abrupt_exit_fixture() {
    let Ok(path) = std::env::var("SPACEWARS_PROVISION_TEST_DIRECTORY") else {
        return;
    };
    let path = PathBuf::from(path);
    let owner = unsafe { libc::geteuid() };
    let system = open_directory(&path, owner).unwrap();
    let payload = fs::read(path.join("bundle.fixture")).unwrap();
    let bundle = Bundle::parse(&payload, Sha256::digest(&payload)).unwrap();
    let plan: Plan = serde_json::from_slice(&fs::read(path.join("plan.fixture")).unwrap()).unwrap();
    provision_under(
        &system,
        owner,
        &bundle,
        plan,
        true,
        || Ok(()),
        |point| {
            if point == "directory-published" {
                std::process::exit(73);
            }
            Ok(())
        },
    )
    .unwrap();
    panic!("exit point not reached");
}

#[tokio::test]
async fn substituted_temporary_cannot_publish_a_different_trust_selection() {
    for phase in [Phase::Prepared, Phase::Complete] {
        let case = Case::new().await;
        let mut changed = false;
        assert!(
            case.run(|point| {
                let path = case.path(".update-provision.next");
                if point == "file-synced" && path.exists() {
                    let mut record: Envelope<Record> =
                        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                    if record.record.phase == phase {
                        record.record.plan.bundle = Sha256::digest(b"unselected trust bundle");
                        case.private_write(
                            ".update-provision.next",
                            &bytes(&record.record).unwrap(),
                        );
                        changed = true;
                    }
                }
                Ok(())
            })
            .is_err()
        );
        assert!(changed);
        assert!(case.status().is_err());
        case.run(|_| Ok(())).unwrap();
        assert_eq!(case.status().unwrap().bundle, case.bundle.fingerprint);
    }
}

#[tokio::test]
async fn process_exit_after_directory_publication_requires_completion_receipt() {
    let case = Case::new().await;
    case.private_write("bundle.fixture", &bundle_bytes(&case.fixture));
    case.private_write("plan.fixture", &serde_json::to_vec(&case.plan).unwrap());
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "provision_linux::tests::abrupt_exit_fixture",
            "--nocapture",
        ])
        .env("SPACEWARS_PROVISION_TEST_DIRECTORY", case.temp.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(73),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(case.path(UPDATES).exists());
    assert!(case.status().is_err());
    case.run(|_| Ok(())).unwrap();
    assert_eq!(case.status().unwrap().persistent_schema, 1);
}
