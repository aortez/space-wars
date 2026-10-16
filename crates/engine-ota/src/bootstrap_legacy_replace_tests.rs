use super::super::legacy_state::tests::Fixture;
use super::*;
use std::os::unix::fs::symlink;
use xattr::FileExt;

struct Prepared {
    fixture: Fixture,
    originals: Sha256,
    source: Snapshot,
}

impl Prepared {
    fn new() -> Self {
        Self::from_fixture(Fixture::new())
    }

    fn from_fixture(fixture: Fixture) -> Self {
        let source = legacy_files::inspect(&fixture.root, fixture.owner).unwrap();
        let originals = fixture.run().unwrap().receipt_sha256;
        let package = fixture.temp.path().join("package");
        fs::create_dir(&package).unwrap();
        for (i, replacement) in fixture.inputs.replacements.iter().enumerate() {
            let path = package.join(i.to_string());
            fs::write(&path, replacement.path.as_bytes()).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        Self {
            fixture,
            originals,
            source,
        }
    }

    fn run(
        &self,
        progress: &mut impl FnMut(usize, Stage) -> Result<()>,
    ) -> Result<LegacyReplacementReport> {
        self.with_check(|| Ok(()), progress)
    }

    fn with_check(
        &self,
        check: impl FnMut() -> Result<()>,
        progress: &mut impl FnMut(usize, Stage) -> Result<()>,
    ) -> Result<LegacyReplacementReport> {
        let f = &self.fixture;
        let sources = (0..f.inputs.replacements.len())
            .map(|i| File::open(f.temp.path().join("package").join(i.to_string())).unwrap())
            .collect();
        apply(
            &f.root,
            &File::open(f.active()).unwrap(),
            f.owner,
            &f.binding,
            &f.inputs,
            self.originals,
            sources,
            check,
            progress,
        )
    }

    fn assert_finished(&self) {
        let f = &self.fixture;
        for replacement in &f.inputs.replacements {
            let path = f.temp.path().join("root").join(&replacement.path);
            assert_eq!(fs::read(&path).unwrap(), replacement.path.as_bytes());
            let metadata = fs::metadata(path).unwrap();
            assert_eq!(metadata.mode() & 0o7777, replacement.mode);
            assert_eq!(metadata.uid(), f.owner);
            assert_eq!(metadata.gid(), f.root.metadata().unwrap().gid());
        }
        let root = f.temp.path().join("root");
        for (i, action) in actions(&self.source).into_iter().enumerate() {
            let path = action.path(&self.source);
            let (parent, _) = split_path(&path);
            let temp = root.join(parent).join(scratch(self.originals, i));
            if let Some(Node::File(original)) = action.original(&self.source) {
                let file = File::open(&temp).unwrap();
                let actual = legacy_files::opened_file(
                    &f.root,
                    &file,
                    f.owner,
                    crate::MAX_IMAGE_EXECUTABLE_BYTES,
                    false,
                    &mut std::io::sink(),
                )
                .unwrap();
                assert!(actual.after_rename(&original));
            } else {
                assert!(!temp.exists(), "{temp:?}");
            }
        }
        let preserved = legacy_state::verified_receipt(
            &File::open(f.active()).unwrap(),
            f.owner,
            &f.binding,
            self.originals,
        )
        .unwrap();
        assert_eq!(preserved.source, self.source);
    }

    fn stop(&self, wanted_index: usize, wanted_stage: Stage) {
        let mut stopped = false;
        let result = self.run(&mut |index, stage| {
            if index == wanted_index && stage == wanted_stage {
                stopped = true;
                return Err(invalid("interruption fixture"));
            }
            Ok(())
        });
        assert!(stopped && result.is_err());
    }
}

#[test]
fn absent_top_level_directories_are_created_in_parent_order() {
    let fixture = Fixture::new();
    for name in ["etc", "usr"] {
        fs::remove_dir_all(fixture.temp.path().join("root").join(name)).unwrap();
    }
    let fixture = Prepared::from_fixture(fixture);
    fixture.run(&mut |_, _| Ok(())).unwrap();
    fixture.assert_finished();
}

#[test]
fn replacement_preserves_original_inodes_attributes_and_empty_files() {
    let fixture = Prepared::new();
    let report = fixture.run(&mut |_, _| Ok(())).unwrap();
    assert!(!report.ready_for_trial);
    assert_eq!(report.files_replaced, 18);
    fixture.assert_finished();
    let before = legacy_files::inspect(&fixture.fixture.root, fixture.fixture.owner).unwrap();
    let repeated = fixture
        .run(&mut |_, _| panic!("completed actions must not run again"))
        .unwrap();
    assert_eq!(report.journal_sha256, repeated.journal_sha256);
    assert_eq!(
        before,
        legacy_files::inspect(&fixture.fixture.root, fixture.fixture.owner).unwrap()
    );
}

#[test]
fn every_directory_and_file_boundary_resumes_without_replacing_preserved_originals() {
    let reference = Prepared::new();
    let mut events = Vec::new();
    reference
        .run(&mut |i, stage| {
            events.push((i, stage));
            Ok(())
        })
        .unwrap();
    for (index, stage) in events {
        let fixture = Prepared::new();
        fixture.stop(index, stage);
        fixture
            .run(&mut |_, _| Ok(()))
            .unwrap_or_else(|e| panic!("{index} {stage:?}: {e}"));
        fixture.assert_finished();
    }
}

#[test]
fn independently_opened_transactions_cannot_replace_files_concurrently() {
    let fixture = Prepared::new();
    let mut checked = false;
    fixture
        .run(&mut |_, stage| {
            if !checked && stage == Stage::Intent {
                assert!(matches!(fixture.run(&mut |_, _| Ok(())), Err(Error::Busy)));
                checked = true;
            }
            Ok(())
        })
        .unwrap();
    assert!(checked);
    fixture.assert_finished();
}

#[test]
fn abrupt_process_exit_after_link_and_exchange_resumes() {
    const CHILD: &str = "SPACEWARS_REPLACEMENT_CRASH_FIXTURE";
    if let Some(path) = std::env::var_os(CHILD) {
        let path = std::path::PathBuf::from(path);
        let active = File::open(path.join("data/spacewars-bootstrap/active")).unwrap();
        let owner = active.metadata().unwrap().uid();
        let binding = super::super::state::read_binding(&active, owner).unwrap();
        let digest: Sha256 = std::env::var("SPACEWARS_REPLACEMENT_CRASH_RECEIPT")
            .unwrap()
            .try_into()
            .unwrap();
        let receipt = legacy_state::verified_receipt(&active, owner, &binding, digest).unwrap();
        let index = actions(&receipt.source)
            .iter()
            .position(|a| matches!(a, Action::File(0)))
            .unwrap();
        let stage = match std::env::var("SPACEWARS_REPLACEMENT_CRASH_STAGE")
            .unwrap()
            .as_str()
        {
            "linked" => Stage::Linked,
            "renamed" => Stage::Renamed,
            _ => panic!("unknown crash fixture"),
        };
        let sources = (0..receipt.inputs.replacements.len())
            .map(|i| File::open(path.join("package").join(i.to_string())).unwrap())
            .collect();
        apply(
            &File::open(path.join("root")).unwrap(),
            &active,
            owner,
            &binding,
            &receipt.inputs,
            digest,
            sources,
            || Ok(()),
            &mut |action, at| {
                if action == index && at == stage {
                    // Deliberately bypass Rust destructors and any cleanup.
                    unsafe { libc::_exit(86) };
                }
                Ok(())
            },
        )
        .unwrap();
        panic!("crash point was not reached");
    }
    for stage in ["linked", "renamed"] {
        let fixture = Prepared::new();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "bootstrap::legacy_replace::tests::abrupt_process_exit_after_link_and_exchange_resumes", "--nocapture"])
            .env(CHILD, fixture.fixture.temp.path())
            .env("SPACEWARS_REPLACEMENT_CRASH_RECEIPT", fixture.originals.to_string())
            .env("SPACEWARS_REPLACEMENT_CRASH_STAGE", stage)
            .output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(86),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fixture.run(&mut |_, _| Ok(())).unwrap();
        fixture.assert_finished();
    }
}

#[test]
fn torn_unpublished_journal_is_replaced_but_the_published_record_stays_authoritative() {
    for begun in [false, true] {
        let fixture = Prepared::new();
        if begun {
            fixture.stop(0, Stage::Intent);
        } else {
            fs::create_dir(fixture.fixture.active().join(DIRECTORY)).unwrap();
            fs::set_permissions(
                fixture.fixture.active().join(DIRECTORY),
                fs::Permissions::from_mode(0o700),
            )
            .unwrap();
        }
        let pending = fixture.fixture.active().join(DIRECTORY).join(PENDING);
        fs::write(&pending, b"partial unpublished journal").unwrap();
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o600)).unwrap();
        fixture.run(&mut |_, _| Ok(())).unwrap();
        fixture.assert_finished();
        assert!(!pending.exists());
    }
}

#[test]
fn changed_original_or_completed_target_stops_without_overwriting_the_edit() {
    for after_swap in [false, true] {
        let fixture = Prepared::new();
        let index = actions(&fixture.source)
            .iter()
            .position(|a| matches!(a, Action::File(0)))
            .unwrap();
        fixture.stop(
            index,
            if after_swap {
                Stage::Done
            } else {
                Stage::Intent
            },
        );
        let manager = fixture.fixture.manager();
        fs::write(&manager, b"operator edit retained").unwrap();
        assert!(fixture.run(&mut |_, _| Ok(())).is_err());
        assert_eq!(fs::read(manager).unwrap(), b"operator edit retained");
    }
}

#[test]
fn same_bytes_on_another_inode_are_not_adopted_as_a_completed_replacement() {
    let fixture = Prepared::new();
    let index = actions(&fixture.source)
        .iter()
        .position(|a| matches!(a, Action::File(0)))
        .unwrap();
    fixture.stop(index, Stage::Renamed);
    let manager = fixture.fixture.manager();
    let moved = manager.with_extension("retained");
    fs::rename(&manager, &moved).unwrap();
    fs::copy(&moved, &manager).unwrap();
    assert!(fixture.run(&mut |_, _| Ok(())).is_err());
    assert_eq!(fs::read(&manager).unwrap(), fs::read(moved).unwrap());
}

#[test]
fn changed_backup_or_parent_is_not_overwritten_during_resume() {
    for parent_changed in [false, true] {
        let fixture = Prepared::new();
        let index = actions(&fixture.source)
            .iter()
            .position(|a| matches!(a, Action::File(0)))
            .unwrap();
        fixture.stop(index, Stage::Renamed);
        if parent_changed {
            let parent = fixture.fixture.manager().parent().unwrap().to_owned();
            fs::rename(&parent, parent.with_extension("retained")).unwrap();
            fs::create_dir(&parent).unwrap();
            fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
        } else {
            let temp = fixture
                .fixture
                .manager()
                .parent()
                .unwrap()
                .join(scratch(fixture.originals, index));
            fs::write(temp, b"changed former original").unwrap();
        }
        assert!(fixture.run(&mut |_, _| Ok(())).is_err());
    }
}

#[test]
fn unrecognized_temporary_objects_are_retained_and_block_progress() {
    for kind in ["symlink", "hardlink", "file", "nonempty-directory"] {
        let fixture = Prepared::new();
        let action = actions(&fixture.source)[0];
        assert!(matches!(action, Action::Directory(_)));
        fixture.stop(0, Stage::Intent);
        let path = action.path(&fixture.source);
        let (parent, _) = split_path(&path);
        let temp = fixture
            .fixture
            .temp
            .path()
            .join("root")
            .join(parent)
            .join(scratch(fixture.originals, 0));
        let outside = fixture.fixture.temp.path().join("outside");
        fs::write(&outside, b"retain this evidence").unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o600)).unwrap();
        match kind {
            "symlink" => symlink(&outside, &temp).unwrap(),
            "hardlink" => fs::hard_link(&outside, &temp).unwrap(),
            "file" => fs::write(&temp, b"foreign temporary contents").unwrap(),
            _ => {
                fs::create_dir(&temp).unwrap();
                fs::set_permissions(&temp, fs::Permissions::from_mode(0o700)).unwrap();
                fs::write(temp.join("keep"), b"foreign child").unwrap();
            }
        }
        assert!(fixture.run(&mut |_, _| Ok(())).is_err(), "{kind}");
        assert!(fs::symlink_metadata(temp).is_ok());
        assert_eq!(fs::read(outside).unwrap(), b"retain this evidence");
    }
}

#[test]
fn unrecorded_files_and_corrupt_journals_never_reinitialize_the_transaction() {
    for kind in ["record", "link", "unknown"] {
        let fixture = Prepared::new();
        fixture.stop(0, Stage::Intent);
        let directory = fixture.fixture.active().join(DIRECTORY);
        let path = directory.join(RECORD);
        match kind {
            "record" => fs::write(&path, b"torn record").unwrap(),
            "link" => {
                fs::remove_file(directory.join(PENDING)).ok();
                symlink(&path, directory.join(PENDING)).unwrap();
            }
            _ => fs::write(directory.join("foreign"), b"keep").unwrap(),
        }
        let before = fs::read(&path).unwrap();
        assert!(fixture.run(&mut |_, _| Ok(())).is_err());
        assert_eq!(fs::read(path).unwrap(), before);
    }
}

#[test]
fn all_replacement_sources_are_checked_before_any_live_root_change() {
    let fixture = Prepared::new();
    let last = fixture.fixture.inputs.replacements.len() - 1;
    fs::write(
        fixture
            .fixture
            .temp
            .path()
            .join("package")
            .join(last.to_string()),
        b"wrong",
    )
    .unwrap();
    assert!(fixture.run(&mut |_, _| Ok(())).is_err());
    assert_eq!(
        legacy_files::inspect(&fixture.fixture.root, fixture.fixture.owner).unwrap(),
        fixture.source
    );
    assert!(!fixture.fixture.active().join(DIRECTORY).exists());
}

#[test]
fn guard_failure_after_publication_leaves_a_resumable_transaction() {
    use std::cell::Cell;
    let fixture = Prepared::new();
    let stopped = Cell::new(false);
    assert!(
        fixture
            .with_check(
                || if stopped.get() {
                    Err(invalid("maintenance guard changed"))
                } else {
                    Ok(())
                },
                &mut |_, stage| {
                    if stage == Stage::Renamed {
                        stopped.set(true);
                    }
                    Ok(())
                }
            )
            .is_err()
    );
    fixture.run(&mut |_, _| Ok(())).unwrap();
    fixture.assert_finished();
}

#[test]
fn inherited_default_acls_are_not_silently_carried_into_privileged_replacements() {
    let fixture = Prepared::new();
    let parent = File::open(fixture.fixture.manager().parent().unwrap()).unwrap();
    // A user xattr is not inherited, so use a real POSIX default ACL.
    let mut acl = 2u32.to_le_bytes().to_vec();
    for (tag, permission, id) in [(1u16, 7u16, u32::MAX), (4, 5, u32::MAX), (32, 5, u32::MAX)] {
        acl.extend_from_slice(&tag.to_le_bytes());
        acl.extend_from_slice(&permission.to_le_bytes());
        acl.extend_from_slice(&id.to_le_bytes());
    }
    parent.set_xattr("system.posix_acl_default", &acl).unwrap();
    // Minimal ACLs can collapse into ordinary mode bits; a named entry keeps
    // an access ACL on newly created candidates and must be rejected.
    let mut acl = 2u32.to_le_bytes().to_vec();
    for (tag, permission, id) in [
        (1u16, 7u16, u32::MAX),
        (2, 4, fixture.fixture.owner + 1),
        (4, 5, u32::MAX),
        (16, 5, u32::MAX),
        (32, 5, u32::MAX),
    ] {
        acl.extend_from_slice(&tag.to_le_bytes());
        acl.extend_from_slice(&permission.to_le_bytes());
        acl.extend_from_slice(&id.to_le_bytes());
    }
    parent.set_xattr("system.posix_acl_default", &acl).unwrap();
    let before = fs::read(fixture.fixture.manager()).unwrap();
    assert!(fixture.run(&mut |_, _| Ok(())).is_err());
    assert_eq!(fs::read(fixture.fixture.manager()).unwrap(), before);
}
