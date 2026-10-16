use super::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::Command;

struct Fixture {
    temporary: tempfile::TempDir,
    owner: u32,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let fixture = Self {
            temporary,
            owner: unsafe { libc::geteuid() },
        };
        for required in REQUIRED {
            let path = fixture.path(required.path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            if required.directory {
                fs::create_dir_all(&path).unwrap();
            } else {
                fs::write(&path, b"throwaway fixture, not a usable key\n").unwrap();
            }
            fs::set_permissions(path, fs::Permissions::from_mode(required.mode)).unwrap();
        }
        // Ancestors created by create_dir_all are affected by the caller's umask.
        for path in ["etc", "etc/NetworkManager", "var", "var/lib"] {
            fs::set_permissions(fixture.path(path), fs::Permissions::from_mode(0o755)).unwrap();
        }
        fixture
    }

    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.temporary.path().join(relative)
    }

    fn root(&self) -> File {
        open_directory(self.temporary.path(), self.owner).unwrap()
    }

    fn inspect(&self) -> Result<PersistentStores> {
        PersistentStores::capture(self.root(), File::open(self.path("data"))?, self.owner)
    }

    fn requirement(&self, path: &str) -> &Requirement {
        REQUIRED.iter().find(|item| item.path == path).unwrap()
    }

    fn entry(&self, path: &str) -> Result<Entry> {
        entry(&self.root(), self.requirement(path), self.owner)
    }
}

#[test]
fn metadata_checks_reject_missing_unsafe_or_replaced_stores_without_repair() {
    for case in [
        "missing",
        "mode",
        "symlink",
        "hardlink",
        "fifo",
        "empty",
        "oversized",
        "ancestor",
    ] {
        let fixture = Fixture::new();
        let relative = "data/ssh/ssh_host_ed25519_key";
        let key = fixture.path(relative);
        match case {
            "missing" => fs::remove_file(&key).unwrap(),
            "mode" => fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).unwrap(),
            "symlink" => {
                fs::remove_file(&key).unwrap();
                symlink("ssh_host_rsa_key", &key).unwrap();
            }
            "hardlink" => fs::hard_link(&key, fixture.path("saved-key")).unwrap(),
            "fifo" => {
                fs::remove_file(&key).unwrap();
                assert!(Command::new("mkfifo").arg(&key).status().unwrap().success());
            }
            "empty" => fs::write(&key, b"").unwrap(),
            "oversized" => File::options()
                .write(true)
                .open(&key)
                .unwrap()
                .set_len(MAX_KEY_BYTES + 1)
                .unwrap(),
            "ancestor" => {
                fs::rename(fixture.path("data/ssh"), fixture.path("moved-ssh")).unwrap();
                symlink("../moved-ssh", fixture.path("data/ssh")).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(fixture.entry(relative).is_err(), "{case}");
        assert!(fixture.inspect().is_err(), "{case}");
        if case == "missing" {
            assert!(
                !key.exists(),
                "the health check must never recreate a missing key"
            );
        }
    }
}

#[test]
fn revoked_login_keys_and_empty_profile_pairing_stores_are_valid() {
    let fixture = Fixture::new();
    for relative in [
        "data/spacewars-access/root",
        "data/spacewars-access/spacewars",
    ] {
        fs::write(fixture.path(relative), b"").unwrap();
        fixture.entry(relative).unwrap();
    }
    fixture.entry(NETWORK).unwrap();
    fixture.entry(BLUETOOTH).unwrap();
    assert!(
        entry(
            &fixture.root(),
            fixture.requirement(NETWORK),
            fixture.owner + 1
        )
        .is_err()
    );
    // Directory copies on the same filesystem are not bind mounts of the store.
    let error = fixture.inspect().err().unwrap();
    assert!(error.to_string().contains("bind does not refer"), "{error}");
}

#[test]
fn observation_detects_key_replacement_and_permission_changes_but_allows_daemon_writes() {
    let fixture = Fixture::new();
    let relative = "data/ssh/ssh_host_ed25519_key";
    let previous = fixture.entry(relative).unwrap();
    fs::rename(fixture.path(relative), fixture.path("retained")).unwrap();
    fs::write(fixture.path(relative), b"different fixture key\n").unwrap();
    fs::set_permissions(fixture.path(relative), fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!previous.matches(&fixture.entry(relative).unwrap()));
    let profiles = fixture.entry(NETWORK).unwrap();
    fs::write(fixture.path(NETWORK).join("new-profile"), b"fixture").unwrap();
    assert!(profiles.matches(&fixture.entry(NETWORK).unwrap()));
    fs::set_permissions(fixture.path(NETWORK), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(fixture.entry(NETWORK).is_err());
}

#[test]
#[ignore = "requires unshare and mount in an unprivileged private namespace"]
fn live_bind_mounts_rechecks_and_sandbox_read_only_views() {
    const TEST: &str =
        "persistence_linux::tests::live_bind_mounts_rechecks_and_sandbox_read_only_views";
    const PARENT: &str = "SPACEWARS_PERSISTENCE_TEST_PARENT_NAMESPACE";
    let namespace = fs::read_link("/proc/self/ns/mnt").unwrap();
    let Some(parent) = std::env::var_os(PARENT) else {
        let output = Command::new("unshare")
            .args(["-Ur", "-m", "--"])
            .arg(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", TEST, "--nocapture"])
            .env(PARENT, namespace)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        print!("{}", String::from_utf8_lossy(&output.stdout));
        return;
    };
    assert_ne!(namespace.as_os_str(), parent);
    assert!(
        Command::new("mount")
            .args(["--make-rprivate", "/"])
            .status()
            .unwrap()
            .success()
    );
    let fixture = Fixture::new();
    struct Mounts(Vec<std::path::PathBuf>);
    impl Mounts {
        fn bind(&mut self, source: &Path, target: &Path) {
            assert!(
                Command::new("mount")
                    .arg("--bind")
                    .arg(source)
                    .arg(target)
                    .status()
                    .unwrap()
                    .success()
            );
            self.0.push(target.into());
        }
        fn pop(&mut self) {
            assert!(
                Command::new("umount")
                    // Pinned O_PATH handles intentionally keep a mount busy.
                    // Detach only these throwaway mounts in our private namespace.
                    .arg("--lazy")
                    .arg(self.0.pop().unwrap())
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
    impl Drop for Mounts {
        fn drop(&mut self) {
            while !self.0.is_empty() {
                self.pop();
            }
        }
    }
    let mut mounts = Mounts(Vec::new());
    mounts.bind(&fixture.path("data"), &fixture.path("data"));
    mounts.bind(&fixture.path(NETWORK), &fixture.path(NETWORK_BIND));
    mounts.bind(&fixture.path(BLUETOOTH), &fixture.path(BLUETOOTH_BIND));
    let initial = fixture.inspect().unwrap();
    initial.compare(&fixture.inspect().unwrap()).unwrap();
    // The recovery unit sees read-only private stores under ProtectSystem.
    // This is a persistence check, not a root-writability test in that sandbox.
    assert!(
        Command::new("mount")
            .args(["-o", "remount,bind,ro"])
            .arg(fixture.path(NETWORK_BIND))
            .status()
            .unwrap()
            .success()
    );
    initial.compare(&fixture.inspect().unwrap()).unwrap();
    fs::write(fixture.path(NETWORK).join("daemon-created"), b"fixture").unwrap();
    initial.compare(&fixture.inspect().unwrap()).unwrap();

    mounts.pop();
    assert!(fixture.inspect().is_err(), "missing Bluetooth bind");
    mounts.bind(&fixture.path(BLUETOOTH), &fixture.path(BLUETOOTH_BIND));
    assert!(
        initial.compare(&fixture.inspect().unwrap()).is_err(),
        "replaced mount during sample"
    );
    let rebound = fixture.inspect().unwrap();
    let key = fixture.path("data/ssh/ssh_host_ed25519_key");
    fs::write(&key, b"changed fixture").unwrap();
    assert!(
        rebound.compare(&fixture.inspect().unwrap()).is_err(),
        "changed retained key"
    );

    mounts.bind(&fixture.path("data/ssh"), &fixture.path("data/ssh"));
    assert!(fixture.inspect().is_err(), "over-mounted persistent source");
    mounts.pop();
    fixture.inspect().unwrap();
    mounts.bind(&fixture.path(BLUETOOTH), &fixture.path(NETWORK_BIND));
    assert!(
        fixture.inspect().is_err(),
        "wrong source with the right owner and mode"
    );
    mounts.pop();
    fixture.inspect().unwrap();
}
