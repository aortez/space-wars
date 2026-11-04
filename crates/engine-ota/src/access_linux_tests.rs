use super::*;
use std::fs;
use std::os::unix::fs::symlink;
use std::sync::OnceLock;

// Fresh throwaway fixture identities, unrelated to any developer/device key.
fn key() -> &'static str {
    static KEY: OnceLock<String> = OnceLock::new();
    KEY.get_or_init(|| {
        let temp = tempfile::Builder::new()
            .permissions(Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = temp.path().join("fixture");
        assert!(
            Command::new("/usr/bin/ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-C", "fixture"])
                .arg("-f")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        fs::read_to_string(path.with_extension("pub")).unwrap()
    })
}

struct Fixture {
    temp: tempfile::TempDir,
    root: File,
    data: File,
    owners: Owners,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::Builder::new()
            .permissions(Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let owners = Owners {
            root: unsafe { libc::geteuid() },
            kiosk: unsafe { libc::geteuid() },
            admin: unsafe { libc::geteuid() },
        };
        for name in [
            "root/.ssh",
            "home/spacewars/.ssh",
            "home/swadmin/.ssh",
            "data",
        ] {
            fs::create_dir_all(temp.path().join(name)).unwrap();
        }
        for name in [
            "root",
            "root/.ssh",
            "home",
            "home/spacewars",
            "home/spacewars/.ssh",
            "home/swadmin",
            "home/swadmin/.ssh",
            "data",
        ] {
            fs::set_permissions(temp.path().join(name), Permissions::from_mode(0o755)).unwrap();
        }
        let root = open_directory(temp.path(), owners.root).unwrap();
        let data = open_at(&root, "data", libc::O_RDONLY | libc::O_DIRECTORY, 0).unwrap();
        Self {
            temp,
            root,
            data,
            owners,
        }
    }
    fn put(&self, path: &str, bytes: &[u8]) {
        fs::write(self.temp.path().join(path), bytes).unwrap();
        fs::set_permissions(self.temp.path().join(path), Permissions::from_mode(0o600)).unwrap();
    }
    fn init(&self) -> Result<AccessStatus> {
        initialize_under(&self.root, &self.data, self.owners)
    }
    fn stored(&self, role: &str) -> Vec<u8> {
        fs::read(self.temp.path().join("data/spacewars-access").join(role)).unwrap()
    }
}

#[test]
fn bootstrap_requires_both_preserved_roles_without_importing_or_promoting_keys() {
    let fixture = Fixture::new();
    assert!(preserved_under(&fixture.data, fixture.owners.root).is_err());
    fixture.put("home/spacewars/.ssh/authorized_keys", key().as_bytes());
    fixture.init().unwrap();
    assert!(preserved_under(&fixture.data, fixture.owners.root).is_err());
    assert!(fixture.stored("root").is_empty());
    fixture.put("root/.ssh/authorized_keys", key().as_bytes());
    assert!(preserved_under(&fixture.data, fixture.owners.root).is_err());
    import_under(&fixture.root, &fixture.data, fixture.owners, Role::Root).unwrap();
    let status = preserved_under(&fixture.data, fixture.owners.root).unwrap();
    assert_eq!(status.root_keys, 1);
    assert_eq!(status.runtime_keys, 1);
    fs::write(
        fixture.temp.path().join("data/spacewars-access/spacewars"),
        b"",
    )
    .unwrap();
    assert!(preserved_under(&fixture.data, fixture.owners.root).is_err());
}

#[test]
fn admin_is_a_separate_persistent_role_and_revocations_survive_new_images() {
    let fixture = Fixture::new();
    let admin = format!("# owner provisioned during flashing\n{}", key());
    fixture.put("home/swadmin/.ssh/authorized_keys", admin.as_bytes());
    let status = fixture.init().unwrap();
    assert_eq!(
        (status.admin_keys, status.root_keys, status.runtime_keys),
        (1, 0, 0)
    );
    assert_eq!(fixture.stored("swadmin"), admin.as_bytes());
    let path = fixture.temp.path().join("data/spacewars-access/swadmin");
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o644);
    fs::write(&path, b"# revoked\n").unwrap();
    assert_eq!(fixture.init().unwrap().admin_keys, 0);
    assert_eq!(fixture.stored("swadmin"), b"# revoked\n");
}

#[test]
fn old_store_adds_only_an_empty_admin_role_until_an_operator_imports_it() {
    let fixture = Fixture::new();
    fixture.put("home/spacewars/.ssh/authorized_keys", key().as_bytes());
    fixture.init().unwrap();
    fs::remove_file(fixture.temp.path().join("data/spacewars-access/swadmin")).unwrap();
    fixture.put("home/swadmin/.ssh/authorized_keys", key().as_bytes());
    let before = fixture.stored("spacewars");
    assert_eq!(fixture.init().unwrap().admin_keys, 0);
    assert!(fixture.stored("swadmin").is_empty());
    let status = import_under(&fixture.root, &fixture.data, fixture.owners, Role::Admin).unwrap();
    assert_eq!((status.admin_keys, status.root_keys), (1, 0));
    assert_eq!(fixture.stored("spacewars"), before);
    fixture.put("home/swadmin/.ssh/authorized_keys", b"bad replacement\n");
    assert_eq!(
        import_under(&fixture.root, &fixture.data, fixture.owners, Role::Admin)
            .unwrap()
            .admin_keys,
        1
    );
}

#[test]
fn admin_keys_reject_unsafe_files_and_do_not_inherit_runtime_authority() {
    for fault in ["symlink", "mode", "malformed", "runtime-owner"] {
        let mut fixture = Fixture::new();
        fixture.put("home/spacewars/.ssh/authorized_keys", key().as_bytes());
        fixture.init().unwrap();
        assert_eq!(fixture.init().unwrap().admin_keys, 0);
        assert!(import_under(&fixture.root, &fixture.data, fixture.owners, Role::Admin).is_err());
        let path = fixture.temp.path().join("data/spacewars-access/swadmin");
        match fault {
            "symlink" => {
                fs::remove_file(&path).unwrap();
                symlink(
                    fixture
                        .temp
                        .path()
                        .join("home/spacewars/.ssh/authorized_keys"),
                    &path,
                )
                .unwrap();
            }
            "mode" => fs::set_permissions(&path, Permissions::from_mode(0o666)).unwrap(),
            "malformed" => fs::write(&path, b"invalid public key\n").unwrap(),
            _ => {
                fixture.put("home/swadmin/.ssh/authorized_keys", key().as_bytes());
                // Model kiosk ownership: this UID is neither the selected
                // root owner nor the configured administrator UID.
                fixture.owners.root += 1;
                fixture.owners.admin += 2;
                assert!(source(&fixture.root, Role::Admin, fixture.owners).is_err());
                continue;
            }
        }
        assert!(fixture.init().is_err(), "{fault}");
        assert!(import_under(&fixture.root, &fixture.data, fixture.owners, Role::Admin).is_err());
    }
}

#[test]
fn interrupted_bootstrap_keeps_existing_roles_but_never_reimports_missing_roles() {
    let fixture = Fixture::new();
    fixture.put("root/.ssh/authorized_keys", key().as_bytes());
    fixture.put("home/spacewars/.ssh/authorized_keys", key().as_bytes());
    let active = fixture.temp.path().join("data/spacewars-bootstrap/active");
    fs::create_dir_all(&active).unwrap();
    fs::set_permissions(active.parent().unwrap(), Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&active, Permissions::from_mode(0o700)).unwrap();
    assert!(fixture.init().is_err());
    assert!(!fixture.temp.path().join("data/spacewars-access").exists());
    // Simulate roles already established before the supervised attempt.
    fs::remove_dir(&active).unwrap();
    assert_eq!(fixture.init().unwrap().root_keys, 1);
    fs::create_dir(&active).unwrap();
    let root = fixture.stored("root");
    let runtime = fixture.stored("spacewars");
    fixture.put("root/.ssh/authorized_keys", b"malformed replacement");
    let status = fixture.init().unwrap();
    assert!(!status.initialized);
    assert_eq!(status.root_keys, 1);
    assert_eq!(fixture.stored("root"), root);
    assert_eq!(fixture.stored("spacewars"), runtime);
}

#[test]
fn bootstrap_accepts_preserved_administrator_without_enabling_root_login_or_reimporting_keys() {
    let fixture = Fixture::new();
    fixture.put("home/swadmin/.ssh/authorized_keys", key().as_bytes());
    fixture.put("home/spacewars/.ssh/authorized_keys", key().as_bytes());
    fixture.init().unwrap();
    let access = preserved_under(&fixture.data, fixture.owners.root).unwrap();
    assert_eq!(access.root_keys, 0);
    assert_eq!(access.admin_keys, 1);
    assert_eq!(access.runtime_keys, 1);
    assert!(fixture.stored("root").is_empty());
    fs::write(
        fixture.temp.path().join("data/spacewars-access/swadmin"),
        b"",
    )
    .unwrap();
    assert!(preserved_under(&fixture.data, fixture.owners.root).is_err());
    fixture.init().unwrap();
    assert!(fixture.stored("swadmin").is_empty());
    assert!(preserved_under(&fixture.data, fixture.owners.root).is_err());
}

#[test]
fn first_boot_preserves_roles_verbatim_and_never_resurrects_revoked_keys() {
    let fixture = Fixture::new();
    let runtime = format!(
        "# runtime only\nrestrict,command=\"echo 'never executed'\" {}",
        key()
    );
    let operator = format!("# independently provisioned operator\n{}", key());
    fixture.put("root/.ssh/authorized_keys", operator.as_bytes());
    fixture.put("home/spacewars/.ssh/authorized_keys", runtime.as_bytes());
    assert_eq!(
        fixture.init().unwrap(),
        AccessStatus {
            initialized: true,
            root_keys: 1,
            runtime_keys: 1,
            admin_keys: 0
        }
    );
    assert_eq!(fixture.stored("root"), operator.as_bytes());
    assert_eq!(fixture.stored("spacewars"), runtime.as_bytes());
    for (path, mode) in [
        ("data/spacewars-access", 0o755),
        ("data/spacewars-access/root", 0o600),
        ("data/spacewars-access/spacewars", 0o644),
    ] {
        let metadata = fs::metadata(fixture.temp.path().join(path)).unwrap();
        assert_eq!(metadata.mode() & 0o7777, mode);
        assert_eq!(metadata.uid(), fixture.owners.root);
    }
    // Revocations survive both a fresh image's empty homes and rollback to
    // this still-keyed old image. Initialization never consults sources again.
    fixture.put("data/spacewars-access/root", b"");
    fs::write(
        fixture.temp.path().join("data/spacewars-access/spacewars"),
        b"# revoked\n",
    )
    .unwrap();
    assert_eq!(
        fixture.init().unwrap(),
        AccessStatus {
            initialized: false,
            root_keys: 0,
            runtime_keys: 0,
            admin_keys: 0
        }
    );
    fixture.put("root/.ssh/authorized_keys", b"now malformed");
    fixture.put("home/spacewars/.ssh/authorized_keys", b"also malformed");
    assert_eq!(fixture.init().unwrap().root_keys, 0);
    assert!(fixture.stored("root").is_empty());
    assert_eq!(fixture.stored("spacewars"), b"# revoked\n");
}

#[test]
fn runtime_only_flash_never_creates_root_authority_and_operator_import_is_explicit() {
    let fixture = Fixture::new();
    fixture.put("home/spacewars/.ssh/authorized_keys", key().as_bytes());
    assert_eq!(
        fixture.init().unwrap(),
        AccessStatus {
            initialized: true,
            root_keys: 0,
            runtime_keys: 1,
            admin_keys: 0
        }
    );
    assert!(fixture.stored("root").is_empty());
    assert!(import_under(&fixture.root, &fixture.data, fixture.owners, Role::Root).is_err());
    let root = format!("# root operator\n{}", key());
    fixture.put("root/.ssh/authorized_keys", root.as_bytes());
    assert_eq!(fixture.init().unwrap().root_keys, 0);
    let before = fixture.stored("spacewars");
    assert_eq!(
        import_under(&fixture.root, &fixture.data, fixture.owners, Role::Root)
            .unwrap()
            .root_keys,
        1
    );
    assert_eq!(fixture.stored("root"), root.as_bytes());
    assert_eq!(fixture.stored("spacewars"), before);
    fixture.put(
        "root/.ssh/authorized_keys",
        b"changed source, do not replace persistent root keys",
    );
    assert_eq!(
        import_under(&fixture.root, &fixture.data, fixture.owners, Role::Root)
            .unwrap()
            .root_keys,
        1
    );
    assert_eq!(fixture.stored("root"), root.as_bytes());
}

#[test]
fn invalid_or_incomplete_persistent_store_never_falls_back_to_image_keys() {
    for fault in [
        "missing-root",
        "missing-runtime",
        "root-mode",
        "runtime-mode",
        "directory-mode",
        "hardlink",
        "symlink",
        "bad-keys",
    ] {
        let fixture = Fixture::new();
        fixture.put("root/.ssh/authorized_keys", key().as_bytes());
        fixture.put("home/spacewars/.ssh/authorized_keys", key().as_bytes());
        fixture.init().unwrap();
        let store = fixture.temp.path().join("data/spacewars-access");
        match fault {
            "missing-root" => fs::remove_file(store.join("root")).unwrap(),
            "missing-runtime" => fs::remove_file(store.join("spacewars")).unwrap(),
            "root-mode" => {
                fs::set_permissions(store.join("root"), Permissions::from_mode(0o644)).unwrap()
            }
            "runtime-mode" => {
                fs::set_permissions(store.join("spacewars"), Permissions::from_mode(0o666)).unwrap()
            }
            "directory-mode" => fs::set_permissions(&store, Permissions::from_mode(0o777)).unwrap(),
            "hardlink" => {
                fs::hard_link(store.join("root"), fixture.temp.path().join("alias")).unwrap()
            }
            "symlink" => {
                fs::remove_file(store.join("root")).unwrap();
                symlink(
                    fixture.temp.path().join("root/.ssh/authorized_keys"),
                    store.join("root"),
                )
                .unwrap();
            }
            "bad-keys" => fs::write(store.join("root"), b"invalid\n").unwrap(),
            _ => unreachable!(),
        }
        assert!(fixture.init().is_err(), "{fault}");
        assert!(
            import_under(&fixture.root, &fixture.data, fixture.owners, Role::Root).is_err(),
            "{fault}"
        );
    }
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.temp.path().join("data/spacewars-system/recovery")).unwrap();
    fs::set_permissions(
        fixture.temp.path().join("data/spacewars-system"),
        Permissions::from_mode(0o700),
    )
    .unwrap();
    fixture.put("root/.ssh/authorized_keys", key().as_bytes());
    assert!(
        fixture
            .init()
            .unwrap_err()
            .to_string()
            .contains("provisioned")
    );
    assert!(!fixture.temp.path().join("data/spacewars-access").exists());
}

#[test]
fn unsafe_sources_and_destinations_reject_before_publication() {
    for fault in [
        "source-symlink",
        "source-hardlink",
        "source-mode",
        "parent-symlink",
        "parent-mode",
        "store-owner",
        "data-mode",
        "directory-symlink",
        "fifo",
        "bad-keys",
    ] {
        let mut fixture = Fixture::new();
        fixture.put("root/.ssh/authorized_keys", key().as_bytes());
        let source = fixture.temp.path().join("root/.ssh/authorized_keys");
        match fault {
            "source-symlink" => {
                fs::rename(&source, source.with_extension("other")).unwrap();
                symlink(source.with_extension("other"), &source).unwrap();
            }
            "source-hardlink" => fs::hard_link(&source, fixture.temp.path().join("alias")).unwrap(),
            "source-mode" => fs::set_permissions(&source, Permissions::from_mode(0o622)).unwrap(),
            "parent-symlink" => {
                fs::rename(
                    fixture.temp.path().join("root/.ssh"),
                    fixture.temp.path().join("other"),
                )
                .unwrap();
                symlink(
                    fixture.temp.path().join("other"),
                    fixture.temp.path().join("root/.ssh"),
                )
                .unwrap();
            }
            "parent-mode" => fs::set_permissions(
                fixture.temp.path().join("root/.ssh"),
                Permissions::from_mode(0o777),
            )
            .unwrap(),
            "store-owner" => fixture.owners.root += 1,
            "data-mode" => fs::set_permissions(
                fixture.temp.path().join("data"),
                Permissions::from_mode(0o777),
            )
            .unwrap(),
            "directory-symlink" => symlink(
                fixture.temp.path().join("root/.ssh"),
                fixture.temp.path().join("data/spacewars-access"),
            )
            .unwrap(),
            "fifo" => {
                fs::remove_file(&source).unwrap();
                let path = std::ffi::CString::new(source.to_str().unwrap()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "bad-keys" => fs::write(&source, b"invalid\n").unwrap(),
            _ => unreachable!(),
        }
        assert!(fixture.init().is_err(), "{fault}");
        assert!(
            fs::symlink_metadata(fixture.temp.path().join("data/spacewars-access")).is_err()
                || fault == "directory-symlink"
        );
    }
}

#[test]
fn public_key_validation_rejects_skipped_lines_secrets_and_unbounded_input() {
    assert_eq!(validate(b"\n  # empty by choice\n").unwrap(), 0);
    assert_eq!(validate(key().as_bytes()).unwrap(), 1);
    assert_eq!(
        validate(format!("{}{}", key(), key()).as_bytes()).unwrap(),
        2
    );
    for bytes in [
        b"not a key\n".to_vec(),
        format!("{}garbage skipped by ssh-keygen\n", key()).into_bytes(),
        format!("{}\0", key()).into_bytes(),
        b"-----BEGIN OPENSSH PRIVATE KEY-----\n".to_vec(),
        key().repeat(MAX_KEYS + 1).into_bytes(),
        vec![b'#'; MAX_LINE + 1],
        vec![b' '; MAX_BYTES + 1],
    ] {
        assert!(validate(&bytes).is_err());
    }
}

#[test]
fn snapshot_detects_named_replacement_and_in_place_changes() {
    let fixture = Fixture::new();
    fixture.put("root/.ssh/authorized_keys", key().as_bytes());
    let directory =
        open_directory(&fixture.temp.path().join("root/.ssh"), fixture.owners.root).unwrap();
    let file = open_at(&directory, "authorized_keys", libc::O_RDONLY, 0).unwrap();
    let metadata = file.metadata().unwrap();
    fixture.put("root/.ssh/authorized_keys", b"changed\n");
    assert!(read_keys(&directory, "authorized_keys", file, metadata).is_err());
    let file = open_at(&directory, "authorized_keys", libc::O_RDONLY, 0).unwrap();
    let metadata = file.metadata().unwrap();
    fs::rename(
        fixture.temp.path().join("root/.ssh/authorized_keys"),
        fixture.temp.path().join("old"),
    )
    .unwrap();
    fixture.put("root/.ssh/authorized_keys", key().as_bytes());
    assert!(read_keys(&directory, "authorized_keys", file, metadata).is_err());
}
