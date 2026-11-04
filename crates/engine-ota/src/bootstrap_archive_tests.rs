use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::Command;

fn fixture() -> (tempfile::TempDir, File, File) {
    let temp = tempfile::tempdir().unwrap();
    for path in [
        "data/spacewars/config/roms",
        "data/NetworkManager/system-connections",
        "data/bluetooth",
        "etc/NetworkManager",
        "root/.ssh",
        "home/swadmin/.ssh",
    ] {
        fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    fs::write(
        temp.path().join("data/spacewars/config/settings.json"),
        b"{\"fixture\":true}\n",
    )
    .unwrap();
    fs::write(
        temp.path()
            .join("data/NetworkManager/system-connections/fixture"),
        b"synthetic credential only\n",
    )
    .unwrap();
    fs::set_permissions(
        temp.path()
            .join("data/NetworkManager/system-connections/fixture"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    fs::write(temp.path().join("etc/hostname"), b"fixture\n").unwrap();
    fs::write(
        temp.path().join("home/swadmin/.ssh/authorized_keys"),
        b"synthetic administrator fixture\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("root/.ssh/authorized_keys"),
        b"synthetic public fixture\n",
    )
    .unwrap();
    let root = File::open(temp.path()).unwrap();
    let data = File::open(temp.path().join("data")).unwrap();
    (temp, root, data)
}

#[test]
fn fixed_selection_includes_unknown_data_and_preserves_links_without_following_them() {
    let (temp, root, data) = fixture();
    for path in [
        "data/spacewars-bootstrap/package",
        "data/logs",
        "data/spacewars/coredumps",
        "etc/NetworkManager/system-connections",
        "data/new-user-state",
    ] {
        fs::create_dir_all(temp.path().join(path)).unwrap();
        fs::write(temp.path().join(path).join("file"), path).unwrap();
    }
    fs::hard_link(
        temp.path().join("data/new-user-state/file"),
        temp.path().join("data/new-user-state/hardlink"),
    )
    .unwrap();
    symlink(
        "/unrelated/outside/data",
        temp.path().join("data/new-user-state/link"),
    )
    .unwrap();
    let before = inspect(&root, &data).unwrap();
    let mut bytes = Vec::new();
    let (blob, copied) = capture(
        &root,
        &data,
        &mut bytes,
        before.archive_upper_bound,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(before, copied);
    assert_eq!(blob.sha256, Sha256::digest(&bytes));
    let (second, again) = capture(
        &root,
        &data,
        std::io::sink(),
        before.archive_upper_bound,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!((blob, before.clone()), (second, again));
    assert_eq!(before, inspect(&root, &data).unwrap());
    let mut entries = BTreeMap::new();
    let mut archive = tar::Archive::new(bytes.as_slice());
    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        entries.insert(
            entry.path().unwrap().into_owned(),
            (
                entry.header().entry_type(),
                entry.link_name().unwrap().map(|p| p.into_owned()),
            ),
        );
    }
    for path in [
        "data/new-user-state/file",
        "etc/hostname",
        "root/.ssh/authorized_keys",
        "home/swadmin/.ssh/authorized_keys",
        "data/spacewars/config/settings.json",
    ] {
        assert!(entries.contains_key(&PathBuf::from(path)), "{path}");
    }
    for path in DATA_EXCLUSIONS.iter().copied() {
        assert!(!entries.keys().any(|p| p.starts_with(path)), "{path}");
    }
    assert!(entries.contains_key(&PathBuf::from("etc/NetworkManager/system-connections/file")));
    assert_eq!(
        entries[&PathBuf::from("data/new-user-state/link")],
        (
            tar::EntryType::Symlink,
            Some(PathBuf::from("/unrelated/outside/data"))
        )
    );
    assert_eq!(
        entries[&PathBuf::from("data/new-user-state/hardlink")],
        (
            tar::EntryType::Link,
            Some(PathBuf::from("data/new-user-state/file"))
        )
    );
}

#[test]
fn metadata_attributes_and_inventory_changes_are_detected_without_reporting_contents() {
    let (temp, root, data) = fixture();
    let original = inspect(&root, &data).unwrap();
    let file = temp.path().join("data/spacewars/config/settings.json");
    fs::write(&file, b"different fixture content").unwrap();
    let changed = inspect(&root, &data).unwrap();
    assert_ne!(original.source_sha256, changed.source_sha256);
    let handle = File::open(&file).unwrap();
    handle
        .set_xattr("user.fixture", b"private attribute")
        .unwrap();
    let with_attribute = inspect(&root, &data).unwrap();
    assert_ne!(changed.source_sha256, with_attribute.source_sha256);
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
    assert_ne!(
        with_attribute.source_sha256,
        inspect(&root, &data).unwrap().source_sha256
    );
    fs::remove_file(&file).unwrap();
    assert_ne!(
        original.source_sha256,
        inspect(&root, &data).unwrap().source_sha256
    );
}

#[test]
fn unexpected_special_files_and_symlinked_source_parents_are_rejected() {
    let (temp, root, data) = fixture();
    let c_name = crate::owned_fs::name("fifo").unwrap();
    assert_eq!(
        unsafe { libc::mkfifoat(data.as_raw_fd(), c_name.as_ptr(), 0o600) },
        0
    );
    assert!(inspect(&root, &data).is_err());
    fs::remove_file(temp.path().join("data/fifo")).unwrap();
    fs::rename(temp.path().join("root"), temp.path().join("old-root")).unwrap();
    symlink("old-root", temp.path().join("root")).unwrap();
    assert!(inspect(&root, &data).is_err());
}

#[test]
fn writer_failure_and_capacity_bounds_cannot_produce_success() {
    let (_temp, root, data) = fixture();
    assert!(capture(&root, &data, std::io::sink(), 1, &mut |_| {}).is_err());
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("fixture failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(capture(&root, &data, Broken, MAX_ARCHIVE_BYTES, &mut |_| {}).is_err());
}

#[test]
#[ignore = "requires unshare and mount in a private user/mount namespace"]
fn real_network_bind_is_captured_once_and_other_filesystems_are_rejected() {
    const CHILD: &str = "SPACEWARS_ARCHIVE_MOUNT_FIXTURE";
    if std::env::var_os(CHILD).is_none() {
        let output = Command::new("unshare")
            .args(["--user", "--map-root-user", "--mount", "--fork", "--"])
            .arg(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "bootstrap::archive::tests::real_network_bind_is_captured_once_and_other_filesystems_are_rejected", "--nocapture"])
            .env(CHILD, "1")
            .env("SPACEWARS_ARCHIVE_PARENT_USER_NS", fs::read_link("/proc/self/ns/user").unwrap())
            .env("SPACEWARS_ARCHIVE_PARENT_MOUNT_NS", fs::read_link("/proc/self/ns/mnt").unwrap())
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/user").unwrap(),
        PathBuf::from(std::env::var_os("SPACEWARS_ARCHIVE_PARENT_USER_NS").unwrap())
    );
    assert_ne!(
        fs::read_link("/proc/self/ns/mnt").unwrap(),
        PathBuf::from(std::env::var_os("SPACEWARS_ARCHIVE_PARENT_MOUNT_NS").unwrap())
    );
    assert!(
        Command::new("mount")
            .args(["--make-rprivate", "/"])
            .status()
            .unwrap()
            .success()
    );
    let (temp, root, data) = fixture();
    let source = temp.path().join("data/NetworkManager/system-connections");
    let destination = temp.path().join("etc/NetworkManager/system-connections");
    fs::create_dir(&destination).unwrap();
    assert!(
        Command::new("mount")
            .arg("--bind")
            .arg(&source)
            .arg(&destination)
            .status()
            .unwrap()
            .success()
    );
    let mut bytes = Vec::new();
    capture(&root, &data, &mut bytes, MAX_ARCHIVE_BYTES, &mut |_| {}).unwrap();
    let mut archive = tar::Archive::new(bytes.as_slice());
    let names: Vec<_> = archive
        .entries()
        .unwrap()
        .map(|e| e.unwrap().path().unwrap().into_owned())
        .collect();
    assert!(names.contains(&PathBuf::from(
        "data/NetworkManager/system-connections/fixture"
    )));
    assert!(
        !names
            .iter()
            .any(|p| p.starts_with("etc/NetworkManager/system-connections"))
    );
    assert!(
        Command::new("umount")
            .arg(&destination)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("mount")
            .args(["-t", "tmpfs", "tmpfs"])
            .arg(&destination)
            .status()
            .unwrap()
            .success()
    );
    assert!(inspect(&root, &data).is_err());
    assert!(
        Command::new("umount")
            .arg(&destination)
            .status()
            .unwrap()
            .success()
    );
}

#[test]
#[ignore = "requires GNU tar with xattrs; extracts only into a temporary workstation directory"]
fn standard_tar_restores_modes_owners_links_nanoseconds_binary_xattrs_and_acl() {
    let (temp, root, data) = fixture();
    let path = temp.path().join("data/spacewars/config/settings.json");
    let handle = File::open(&path).unwrap();
    handle
        .set_xattr("user.fixture", b"binary\0attribute\nvalue")
        .unwrap();
    let mut acl = 2u32.to_le_bytes().to_vec();
    for (tag, permission, id) in [
        (1u16, 6u16, u32::MAX),
        (2, 4, unsafe { libc::geteuid() } + 1),
        (4, 4, u32::MAX),
        (16, 4, u32::MAX),
        (32, 0, u32::MAX),
    ] {
        acl.extend_from_slice(&tag.to_le_bytes());
        acl.extend_from_slice(&permission.to_le_bytes());
        acl.extend_from_slice(&id.to_le_bytes());
    }
    handle.set_xattr("system.posix_acl_access", &acl).unwrap();
    let modified = std::time::UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_789);
    handle
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    fs::hard_link(&path, path.with_file_name("linked-settings")).unwrap();
    symlink("settings.json", path.with_file_name("settings-link")).unwrap();
    let output = tempfile::tempdir().unwrap();
    let archive_path = output.path().join("archive.tar");
    let mut archive = File::create(&archive_path).unwrap();
    capture(&root, &data, &mut archive, MAX_ARCHIVE_BYTES, &mut |_| {}).unwrap();
    let restored = output.path().join("restore");
    fs::create_dir(&restored).unwrap();
    let result = Command::new("tar")
        .args([
            "--extract",
            "--xattrs",
            "--xattrs-include=*",
            "--same-permissions",
            "--file",
        ])
        .arg(&archive_path)
        .arg("--directory")
        .arg(&restored)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let restored_file = restored.join("data/spacewars/config/settings.json");
    let saved = File::open(&restored_file).unwrap();
    let metadata = saved.metadata().unwrap();
    let original = handle.metadata().unwrap();
    assert_eq!(fs::read(&path).unwrap(), fs::read(&restored_file).unwrap());
    assert_eq!(
        (
            metadata.uid(),
            metadata.gid(),
            metadata.mode(),
            metadata.mtime_nsec()
        ),
        (
            original.uid(),
            original.gid(),
            original.mode(),
            original.mtime_nsec()
        )
    );
    assert_eq!(
        saved.get_xattr("user.fixture").unwrap(),
        handle.get_xattr("user.fixture").unwrap()
    );
    assert_eq!(
        saved.get_xattr("system.posix_acl_access").unwrap(),
        Some(acl)
    );
    assert_eq!(
        metadata.ino(),
        fs::metadata(restored_file.with_file_name("linked-settings"))
            .unwrap()
            .ino()
    );
    assert_eq!(
        fs::read_link(restored_file.with_file_name("settings-link")).unwrap(),
        PathBuf::from("settings.json")
    );
}
