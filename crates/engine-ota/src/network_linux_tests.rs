use super::*;
use std::os::unix::fs::symlink;

const PROFILE: &[u8] = b"[connection]\nid=fixture only\nuuid=46f9d72c-85ad-45a0-b37c-f75d716c3fe6\ntype=wifi\n[wifi-security]\nkey-mgmt=wpa-psk\npsk=not-a-real-credential\n";

struct Fixture {
    temp: tempfile::TempDir,
    root: File,
    data: File,
    owner: u32,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::Builder::new()
            .permissions(Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        for component in [
            "etc",
            "etc/NetworkManager",
            "etc/NetworkManager/system-connections",
            "data",
        ] {
            fs::create_dir(temp.path().join(component)).unwrap();
            fs::set_permissions(temp.path().join(component), Permissions::from_mode(0o755))
                .unwrap();
        }
        let owner = unsafe { libc::geteuid() };
        let root = open_directory(temp.path(), owner).unwrap();
        let data = open_at(&root, "data", libc::O_RDONLY | libc::O_DIRECTORY, 0).unwrap();
        Self {
            temp,
            root,
            data,
            owner,
        }
    }
    fn init(&self) -> Result<NetworkStatus> {
        initialize_under(&self.root, &self.data, self.owner)
    }
    fn seed(&self, component: &str, bytes: &[u8]) {
        let path = self
            .temp
            .path()
            .join("etc/NetworkManager/system-connections")
            .join(component);
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(path, Permissions::from_mode(0o600)).unwrap();
    }
    fn profiles(&self) -> std::path::PathBuf {
        self.temp
            .path()
            .join("data/NetworkManager/system-connections")
    }
    fn provisioned(&self, bootstrap: bool) {
        let components = if bootstrap {
            ["spacewars-bootstrap", "spacewars-bootstrap/active"]
        } else {
            ["spacewars-system", "spacewars-system/recovery"]
        };
        for component in components {
            let path = self.temp.path().join("data").join(component);
            fs::create_dir(&path).unwrap();
            fs::set_permissions(path, Permissions::from_mode(0o700)).unwrap();
        }
    }
}

#[test]
fn first_import_preserves_bytes_and_never_resurrects_deleted_profiles() {
    let fixture = Fixture::new();
    fixture.seed("home.nmconnection", PROFILE);
    fixture.seed("second.nmconnection", b"# second fixture\n");
    assert_eq!(
        fixture.init().unwrap(),
        NetworkStatus {
            initialized: true,
            imported_profiles: 2
        }
    );
    assert_eq!(
        fs::read(fixture.profiles().join("home.nmconnection")).unwrap(),
        PROFILE
    );
    for (path, mode) in [
        (fixture.profiles().parent().unwrap().to_owned(), 0o755),
        (fixture.profiles(), 0o700),
        (fixture.profiles().join("home.nmconnection"), 0o600),
    ] {
        let metadata = fs::metadata(path).unwrap();
        assert_eq!(metadata.uid(), fixture.owner);
        assert_eq!(metadata.mode() & 0o7777, mode);
    }
    fs::remove_file(fixture.profiles().join("home.nmconnection")).unwrap();
    fs::write(
        fixture.profiles().join("second.nmconnection"),
        b"changed on device",
    )
    .unwrap();
    fixture.seed("new-image.nmconnection", PROFILE);
    assert_eq!(
        fixture.init().unwrap(),
        NetworkStatus {
            initialized: false,
            imported_profiles: 0
        }
    );
    assert!(!fixture.profiles().join("home.nmconnection").exists());
    assert!(!fixture.profiles().join("new-image.nmconnection").exists());
    assert_eq!(
        fs::read(fixture.profiles().join("second.nmconnection")).unwrap(),
        b"changed on device"
    );
    fs::remove_file(fixture.profiles().join("second.nmconnection")).unwrap();
    // Deliberately forgetting every network must survive rollback too.
    fixture.init().unwrap();
    assert_eq!(fs::read_dir(fixture.profiles()).unwrap().count(), 0);
}

#[test]
fn empty_first_boot_and_existing_legacy_store_do_not_need_seed_credentials() {
    let fixture = Fixture::new();
    assert_eq!(fixture.init().unwrap().imported_profiles, 0);
    // Existing stores are validated without consulting either image seeds or
    // file contents (NM owns profile updates after initialization).
    fs::remove_dir_all(fixture.temp.path().join("etc/NetworkManager")).unwrap();
    fixture.provisioned(true);
    fixture.init().unwrap();
    let directory = open_at(
        &fixture.data,
        DIRECTORY,
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    )
    .unwrap();
    preserved(&fixture.data, &directory, fixture.owner).unwrap();
}

#[test]
fn missing_store_during_bootstrap_or_after_recovery_never_reimports() {
    for bootstrap in [false, true] {
        let fixture = Fixture::new();
        fixture.seed("home.nmconnection", PROFILE);
        fixture.provisioned(bootstrap);
        assert!(fixture.init().is_err());
        assert!(!fixture.profiles().parent().unwrap().exists());
    }
}

#[test]
fn partial_or_unsafe_persistent_store_is_rejected_without_repair() {
    for failure in [
        "missing",
        "parent-link",
        "child-link",
        "parent-mode",
        "child-mode",
    ] {
        let fixture = Fixture::new();
        fixture.init().unwrap();
        match failure {
            "missing" => fs::remove_dir(fixture.profiles()).unwrap(),
            "parent-link" => {
                fs::rename(
                    fixture.profiles().parent().unwrap(),
                    fixture.temp.path().join("saved"),
                )
                .unwrap();
                symlink(
                    fixture.temp.path().join("saved"),
                    fixture.profiles().parent().unwrap(),
                )
                .unwrap();
            }
            "child-link" => {
                fs::rename(fixture.profiles(), fixture.temp.path().join("saved")).unwrap();
                symlink(fixture.temp.path().join("saved"), fixture.profiles()).unwrap();
            }
            "parent-mode" => fs::set_permissions(
                fixture.profiles().parent().unwrap(),
                Permissions::from_mode(0o777),
            )
            .unwrap(),
            "child-mode" => {
                fs::set_permissions(fixture.profiles(), Permissions::from_mode(0o755)).unwrap()
            }
            _ => unreachable!(),
        }
        fixture.seed("should-not-be-imported.nmconnection", PROFILE);
        assert!(fixture.init().is_err(), "{failure}");
        assert!(
            !fixture
                .profiles()
                .join("should-not-be-imported.nmconnection")
                .exists()
        );
        if failure == "missing" {
            assert!(!fixture.profiles().exists());
        }
    }
}

#[test]
fn unsafe_seed_files_never_publish_a_partial_store() {
    for failure in [
        "symlink",
        "hardlink",
        "directory",
        "mode",
        "large",
        "fifo",
        "source-link",
        "owner",
    ] {
        let fixture = Fixture::new();
        fixture.seed("valid.nmconnection", PROFILE);
        let path = fixture
            .temp
            .path()
            .join("etc/NetworkManager/system-connections/bad.nmconnection");
        match failure {
            "symlink" => symlink("valid.nmconnection", &path).unwrap(),
            "hardlink" => fs::hard_link(path.with_file_name("valid.nmconnection"), &path).unwrap(),
            "directory" => fs::create_dir(&path).unwrap(),
            "mode" => {
                fixture.seed("bad.nmconnection", PROFILE);
                fs::set_permissions(&path, Permissions::from_mode(0o644)).unwrap();
            }
            "large" => {
                fixture.seed("bad.nmconnection", b"");
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(MAX_PROFILE_BYTES + 1)
                    .unwrap();
            }
            "fifo" => {
                let directory = source_directory(&fixture.root, fixture.owner).unwrap();
                let component = name("bad.nmconnection").unwrap();
                assert_eq!(
                    unsafe { libc::mkfifoat(directory.as_raw_fd(), component.as_ptr(), 0o600) },
                    0
                );
            }
            "source-link" => {
                fs::rename(path.parent().unwrap(), fixture.temp.path().join("saved")).unwrap();
                symlink(fixture.temp.path().join("saved"), path.parent().unwrap()).unwrap();
            }
            "owner" => {
                let source = source_directory(&fixture.root, fixture.owner).unwrap();
                assert!(snapshot(&source, fixture.owner + 1).is_err());
                continue;
            }
            _ => unreachable!(),
        }
        assert!(fixture.init().is_err(), "{failure}");
        assert!(!fixture.profiles().parent().unwrap().exists());
    }
}

#[test]
fn import_is_bounded_and_detects_changes_before_publication() {
    let fixture = Fixture::new();
    fixture.seed("home.nmconnection", PROFILE);
    let source = source_directory(&fixture.root, fixture.owner).unwrap();
    let original = source.metadata().unwrap();
    let profiles = snapshot(&source, fixture.owner).unwrap();
    fixture.seed("home.nmconnection", b"changed after snapshot");
    assert!(recheck_source(&source, &original, &profiles).is_err());
    let profiles = snapshot(&source, fixture.owner).unwrap();
    fixture.seed("added.nmconnection", PROFILE);
    assert!(recheck_source(&source, &original, &profiles).is_err());

    for index in 0..MAX_PROFILES {
        fixture.seed(&format!("{index}.nmconnection"), b"");
    }
    assert!(fixture.init().is_err());
    assert!(!fixture.profiles().parent().unwrap().exists());

    let fixture = Fixture::new();
    for index in 0..=MAX_TOTAL_BYTES / MAX_PROFILE_BYTES as usize {
        fixture.seed(
            &format!("{index}.nmconnection"),
            &vec![b'x'; MAX_PROFILE_BYTES as usize],
        );
    }
    assert!(fixture.init().is_err());
    assert!(!fixture.profiles().parent().unwrap().exists());
}
