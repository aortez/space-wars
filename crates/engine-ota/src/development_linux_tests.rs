use super::*;
use crate::ImageIdentity;
use crate::image_tools::ImageTools;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const RUNTIME: &[u8] = include_bytes!(
    "../../../yocto/meta-spacewars/recipes-spacewars/spacewars/files/development-runtime.json"
);
const IDENTITY: &[u8] = include_bytes!("../tests/fixtures/image-identity-v1.json");

fn binding(device: &str) -> BootBinding {
    BootBinding {
        device_id: Sha256::digest(device.as_bytes()),
        layout_id: Sha256::digest(b"layout"),
        boot_identity: Sha256::digest(b"boot files"),
        hardware_profile: Sha256::digest(b"hardware"),
    }
}

fn system() -> (tempfile::TempDir, File, u32) {
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let system = File::open(directory.path()).unwrap();
    // SAFETY: geteuid has no preconditions.
    (directory, system, unsafe { libc::geteuid() })
}

#[test]
fn opt_in_is_bound_to_this_unit_and_can_be_withdrawn() {
    let (_directory, system, owner) = system();
    let unit = binding("this unit");
    assert!(!enabled(&system, owner, &unit).unwrap());
    write_marker(&system, owner, &unit).unwrap();
    write_marker(&system, owner, &unit).unwrap();
    assert!(enabled(&system, owner, &unit).unwrap());
    // Another unit, or this one after a reflash with other boot files.
    assert!(!enabled(&system, owner, &binding("another unit")).unwrap());
    let mut reflashed = unit.clone();
    reflashed.boot_identity = Sha256::digest(b"other boot files");
    assert!(!enabled(&system, owner, &reflashed).unwrap());
    remove_marker(&system).unwrap();
    remove_marker(&system).unwrap();
    assert!(!enabled(&system, owner, &unit).unwrap());
}

#[test]
fn unsafe_unknown_or_linked_markers_are_refused() {
    let unit = binding("this unit");
    let valid = serde_json::to_vec(&Marker {
        schema: 1,
        binding: unit.clone(),
    })
    .unwrap();
    let mut extra: serde_json::Value = serde_json::from_slice(&valid).unwrap();
    extra["force"] = true.into();
    for (contents, mode) in [
        (valid.clone(), 0o644),
        (
            serde_json::to_vec(&Marker {
                schema: 2,
                binding: unit.clone(),
            })
            .unwrap(),
            0o600,
        ),
        (serde_json::to_vec(&extra).unwrap(), 0o600),
        (b"enabled".to_vec(), 0o600),
    ] {
        let (directory, system, owner) = system();
        let path = directory.path().join(MARKER);
        fs::write(&path, &contents).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        assert!(
            enabled(&system, owner, &unit).is_err(),
            "{mode:o} {}",
            String::from_utf8_lossy(&contents)
        );
    }
    let (directory, system, owner) = system();
    let target = directory.path().join("elsewhere");
    fs::write(&target, &valid).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    std::os::unix::fs::symlink(&target, directory.path().join(MARKER)).unwrap();
    assert!(enabled(&system, owner, &unit).is_err());
}

#[test]
fn only_images_that_declare_development_records_can_be_written() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("root");
    fs::create_dir_all(source.join("etc/spacewars")).unwrap();
    fs::write(source.join("etc/spacewars/image.json"), IDENTITY).unwrap();
    let declaration = source.join("etc/spacewars/development-runtime.json");
    for contents in [
        Some(RUNTIME),
        None,
        Some(&br#"{"schema":1,"protocol":0}"#[..]),
        Some(&br#"{"schema":1,"protocol":2}"#[..]),
        Some(&br#"{"schema":2,"protocol":1}"#[..]),
        Some(&br#"{"schema":1,"protocol":1,"bypass":true}"#[..]),
        Some(&b"not a runtime record"[..]),
    ] {
        let _ = fs::remove_file(&declaration);
        if let Some(contents) = contents {
            fs::write(&declaration, contents).unwrap();
        }
        let root = directory.path().join("root.ext4");
        let _ = fs::remove_file(&root);
        let result = Command::new("/usr/sbin/mke2fs")
            .args([
                "-q",
                "-t",
                "ext4",
                "-b",
                "1024",
                "-E",
                "lazy_itable_init=0,lazy_journal_init=0",
                "-d",
            ])
            .arg(&source)
            .arg(&root)
            .arg("8192")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let checked = ImageTools::default().check_development(&File::open(&root).unwrap());
        if contents == Some(RUNTIME) {
            assert_eq!(checked.unwrap(), ImageIdentity::parse(IDENTITY).unwrap());
        } else {
            assert!(checked.is_err(), "{contents:?}");
        }
    }
}
