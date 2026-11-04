use super::*;
use crate::bootstrap::trial_store::tests::Fixture;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

fn setup(fixture: &Fixture) -> std::path::PathBuf {
    let boot = fixture.temp.path().join("boot");
    fs::create_dir(&boot).unwrap();
    fs::set_permissions(&boot, fs::Permissions::from_mode(0o755)).unwrap();
    for (path, bytes) in &fixture.boot_files {
        let path = boot.join(path);
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        let mut current = Some(parent);
        while let Some(directory) = current {
            if directory == fixture.temp.path() {
                break;
            }
            fs::set_permissions(directory, fs::Permissions::from_mode(0o755)).unwrap();
            current = directory.parent();
        }
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
    }
    boot
}

#[test]
fn selector_interruptions_preserve_a_complete_old_or_new_config_and_all_other_assets() {
    for fresh in [false, true] {
        selector_interruptions(fresh);
    }
}

fn selector_interruptions(fresh: bool) {
    for point in [
        "created",
        "written",
        "file-sync",
        "rename",
        "directory-sync",
        "filesystem-sync",
    ] {
        let fixture = if fresh {
            Fixture::fresh()
        } else {
            Fixture::new()
        };
        let boot = setup(&fixture);
        let mut files = TrialBootFiles::open(
            File::open(&boot).unwrap(),
            fixture.owner,
            fixture.inputs.clone(),
        )
        .unwrap();
        files.fault = Some(point);
        assert!(files.select(true, |_| Ok(())).is_err(), "{point}");
        drop(files);
        let reopened = TrialBootFiles::open(
            File::open(&boot).unwrap(),
            fixture.owner,
            fixture.inputs.clone(),
        )
        .unwrap();
        assert_eq!(
            reopened.staged_default().unwrap(),
            matches!(point, "rename" | "directory-sync" | "filesystem-sync")
        );
        if !reopened.staged_default().unwrap() {
            reopened.select(true, |_| Ok(())).unwrap();
        }
        reopened.select(false, |_| Ok(())).unwrap();
        for (path, bytes) in &fixture.boot_files {
            assert_eq!(
                &fs::read(boot.join(path)).unwrap(),
                bytes,
                "{point}: {path}"
            );
        }
    }
}

#[test]
fn changed_boot_assets_unknown_files_or_links_prevent_selection() {
    for case in ["legacy", "staged", "trial", "unknown", "link"] {
        let fixture = Fixture::new();
        let boot = setup(&fixture);
        let files = TrialBootFiles::open(
            File::open(&boot).unwrap(),
            fixture.owner,
            fixture.inputs.clone(),
        )
        .unwrap();
        match case {
            "legacy" => fs::write(boot.join("kernel8.img"), b"damaged old kernel").unwrap(),
            "staged" => {
                fs::write(boot.join("sw-bootstrap/kernel8.img"), b"changed candidate").unwrap()
            }
            "trial" => fs::write(boot.join("tryboot.txt"), b"wrong trial").unwrap(),
            "unknown" => fs::write(boot.join("unexpected.txt"), b"extra").unwrap(),
            "link" => {
                fs::remove_file(boot.join("sw-bootstrap/kernel8.img")).unwrap();
                symlink("../kernel8.img", boot.join("sw-bootstrap/kernel8.img")).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            files
                .select(true, |_| panic!(
                    "changed assets must fail before authorization"
                ))
                .is_err()
        );
        assert_eq!(
            fs::read(boot.join("config.txt")).unwrap(),
            fixture.inputs.legacy_config.as_bytes()
        );
    }
}

#[test]
fn final_authorization_can_expire_after_preparation_without_changing_default() {
    let fixture = Fixture::new();
    let boot = setup(&fixture);
    let files = TrialBootFiles::open(
        File::open(&boot).unwrap(),
        fixture.owner,
        fixture.inputs.clone(),
    )
    .unwrap();
    let mut checks = 0;
    assert!(
        files
            .select(true, |_| {
                checks += 1;
                if checks == 2 {
                    Err(invalid("health expired"))
                } else {
                    Ok(())
                }
            })
            .is_err()
    );
    assert_eq!(checks, 2);
    assert!(!files.staged_default().unwrap());
    files.verify().unwrap();
}
