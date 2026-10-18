use super::*;
use crate::bootstrap::trial_store::tests::Fixture;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

fn modes(directory: &Path) {
    fs::set_permissions(directory, fs::Permissions::from_mode(0o755)).unwrap();
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            modes(&entry.path());
        } else if entry.file_type().unwrap().is_file() {
            fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o644)).unwrap();
        }
    }
}

struct Staging {
    fixture: Fixture,
    boot: PathBuf,
    sources: PathBuf,
}

impl Staging {
    fn new() -> Self {
        Self::with_fresh_origin(false)
    }

    fn with_fresh_origin(fresh: bool) -> Self {
        let fixture = if fresh {
            Fixture::fresh()
        } else {
            Fixture::new()
        };
        let boot = fixture.temp.path().join("boot");
        let sources = fixture.temp.path().join("sources");
        for (path, bytes) in &fixture.boot_files {
            let destination = if let Some(path) = path.strip_prefix("sw-bootstrap/") {
                sources.join(path)
            } else if path == "tryboot.txt" {
                continue;
            } else {
                boot.join(path)
            };
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::write(&destination, bytes).unwrap();
        }
        if fresh {
            fs::write(
                boot.join("tryboot.txt"),
                format!(
                    "{}\n[all]\ncmdline=spacewars-trial.txt\n",
                    fixture.inputs.legacy_config.trim_end()
                ),
            )
            .unwrap();
        }
        for base in [&boot, &sources] {
            modes(base);
        }
        Self {
            fixture,
            boot,
            sources,
        }
    }
    fn apply(&self, checkpoint: impl FnMut(&str) -> Result<()>) -> Result<()> {
        apply(
            &File::open(&self.boot)?,
            self.fixture.owner,
            &self.fixture.inputs,
            |path| Ok(File::open(self.sources.join(path))?),
            checkpoint,
        )
    }
    fn retained(&self) {
        for path in self
            .fixture
            .inputs
            .binding
            .original_boot_files
            .keys()
            .filter(|path| path.as_str() != "tryboot.txt")
        {
            assert_eq!(
                fs::read(self.boot.join(path)).unwrap(),
                self.fixture.boot_files[path],
                "{path}"
            );
        }
    }
    fn complete(&self) {
        let files = inventory(&File::open(&self.boot).unwrap(), self.fixture.owner).unwrap();
        assert_eq!(
            retained(&files, &self.fixture.inputs, 512).unwrap().0,
            self.fixture.inputs.binding.original_boot_files
        );
        self.retained();
        for (path, bytes) in &self.fixture.boot_files {
            assert_eq!(&fs::read(self.boot.join(path)).unwrap(), bytes, "{path}");
        }
        assert!(!self.boot.join(PENDING).exists());
        assert!(!self.boot.join(SELECTOR_PENDING).exists());
    }
}

#[test]
fn staging_publication_faults_resume_without_changing_the_retained_normal_boot() {
    for fresh in [false, true] {
        staging_faults_resume(fresh);
    }
}

fn staging_faults_resume(fresh: bool) {
    let full = Staging::with_fresh_origin(fresh);
    let mut points = Vec::new();
    full.apply(|point| {
        points.push(point.to_owned());
        Ok(())
    })
    .unwrap();
    full.complete();
    let mut offsets = BTreeSet::new();
    for name in points.iter().collect::<BTreeSet<_>>() {
        let matching: Vec<_> = points
            .iter()
            .enumerate()
            .filter_map(|(n, p)| (p == name).then_some(n))
            .collect();
        offsets.insert(matching[0]);
        offsets.insert(matching[matching.len() / 2]);
        offsets.insert(*matching.last().unwrap());
    }
    for failure in offsets {
        let fixture = Staging::with_fresh_origin(fresh);
        let mut count = 0;
        let result = fixture.apply(|_| {
            let current = count;
            count += 1;
            if current == failure {
                return Err(invalid("interrupted staging fixture"));
            }
            Ok(())
        });
        assert!(result.is_err(), "{} at {failure}", points[failure]);
        fixture.retained();
        fixture.apply(|_| Ok(())).unwrap();
        fixture.complete();
        // Repeated preparation checks the published tree without recopying it.
        fixture
            .apply(|point| {
                assert!(!point.starts_with("stage-file-") && !point.starts_with("trial-file-"));
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn fresh_staging_never_overwrites_unknown_missing_or_concurrently_changed_trial_selectors() {
    for missing in [true, false] {
        let fixture = Staging::with_fresh_origin(true);
        let path = fixture.boot.join("tryboot.txt");
        if missing {
            fs::remove_file(&path).unwrap();
        } else {
            fs::write(&path, b"another selector\n").unwrap();
        }
        assert!(
            fixture
                .apply(|_| panic!("must reject before staging"))
                .is_err()
        );
        assert!(!fixture.boot.join(STAGE_DIRECTORY).exists());
        assert!(!fixture.boot.join(PENDING).exists());
    }
    let fixture = Staging::with_fresh_origin(true);
    assert!(
        fixture
            .apply(|point| {
                if point == "trial-before-publish" {
                    fs::write(fixture.boot.join("tryboot.txt"), b"concurrent change\n").unwrap();
                }
                Ok(())
            })
            .is_err()
    );
    assert_eq!(
        fs::read(fixture.boot.join("tryboot.txt")).unwrap(),
        b"concurrent change\n"
    );
}

#[test]
fn changed_completed_assets_selectors_and_legacy_files_are_never_repaired() {
    for relative in [
        "sw-bootstrap/kernel8.img",
        "sw-bootstrap/trial-cmdline.txt",
        "tryboot.txt",
        "config.txt",
        "kernel8.img",
    ] {
        let fixture = Staging::new();
        fixture.apply(|_| Ok(())).unwrap();
        let path = fixture.boot.join(relative);
        fs::write(&path, b"operator change").unwrap();
        assert!(fixture.apply(|_| Ok(())).is_err(), "{relative}");
        assert_eq!(fs::read(path).unwrap(), b"operator change");
    }
    let fixture = Staging::new();
    fixture.apply(|_| Ok(())).unwrap();
    fs::remove_file(fixture.boot.join("sw-bootstrap/kernel8.img")).unwrap();
    assert!(fixture.apply(|_| Ok(())).is_err());
    assert!(!fixture.boot.join("sw-bootstrap/kernel8.img").exists());
}

#[test]
fn unexpected_pending_files_directories_links_and_conflicting_trees_block_before_copy() {
    for case in [
        "file",
        "directory",
        "link",
        "hardlink",
        "oversized",
        "both",
        "trial",
    ] {
        let fixture = Staging::new();
        let pending = fixture.boot.join(PENDING);
        fs::create_dir(&pending).unwrap();
        match case {
            "file" => fs::write(pending.join("unknown"), b"keep").unwrap(),
            "directory" => fs::create_dir(pending.join("unknown")).unwrap(),
            "link" => symlink("/dev/zero", pending.join("kernel8.img")).unwrap(),
            "hardlink" => fs::hard_link(
                fixture.sources.join("kernel8.img"),
                pending.join("kernel8.img"),
            )
            .unwrap(),
            "oversized" => fs::write(pending.join("kernel8.img"), vec![0; 1024]).unwrap(),
            "both" => fs::create_dir(fixture.boot.join(STAGE_DIRECTORY)).unwrap(),
            "trial" => fs::write(
                fixture.boot.join("tryboot.txt"),
                fixture.fixture.inputs.plan.trial_config.as_bytes(),
            )
            .unwrap(),
            _ => unreachable!(),
        }
        modes(&fixture.boot);
        assert!(fixture.apply(|_| Ok(())).is_err(), "{case}");
        fixture.retained();
        assert!(!fixture.boot.join("sw-bootstrap/kernel8.img").exists());
    }
}

#[test]
fn changed_source_or_final_guard_cannot_publish_a_trial_file() {
    let fixture = Staging::new();
    let source = fixture.sources.join("kernel8.img");
    let original = fs::read(&source).unwrap();
    fs::write(&source, vec![b'x'; original.len()]).unwrap();
    assert!(fixture.apply(|_| Ok(())).is_err());
    assert!(!fixture.boot.join("tryboot.txt").exists());
    fixture.retained();
    fs::write(source, original).unwrap();
    assert!(
        fixture
            .apply(|point| {
                if point == "trial-before-publish" {
                    return Err(invalid("prerequisite changed"));
                }
                Ok(())
            })
            .is_err()
    );
    assert!(!fixture.boot.join("tryboot.txt").exists());
    fixture.apply(|_| Ok(())).unwrap();
    fixture.complete();
}

#[test]
fn resume_space_credit_is_only_for_bounded_recorded_staging_files() {
    let fixture = Staging::new();
    let boot = File::open(&fixture.boot).unwrap();
    assert_eq!(
        retained(
            &inventory(&boot, fixture.fixture.owner).unwrap(),
            &fixture.fixture.inputs,
            4096
        )
        .unwrap()
        .1,
        0
    );
    assert!(
        fixture
            .apply(|point| {
                if point == "stage-file-written" {
                    return Err(invalid("stop after one file"));
                }
                Ok(())
            })
            .is_err()
    );
    let (_, credit) = retained(
        &inventory(&boot, fixture.fixture.owner).unwrap(),
        &fixture.fixture.inputs,
        4096,
    )
    .unwrap();
    assert!(credit > 0 && credit.is_multiple_of(4096));
    fixture.apply(|_| Ok(())).unwrap();
    let (_, credit) = retained(
        &inventory(&boot, fixture.fixture.owner).unwrap(),
        &fixture.fixture.inputs,
        4096,
    )
    .unwrap();
    let expected = expected(&fixture.fixture.inputs)
        .values()
        .map(|blob| blob.bytes.div_ceil(4096) * 4096)
        .sum::<u64>();
    assert_eq!(credit, expected);
}
