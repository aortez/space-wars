use super::*;
use crate::image_tools::ImageTools;
use crate::install_linux::WriteStage;
use std::cell::Cell;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;

struct Image {
    compressed: Vec<u8>,
    expanded: Vec<u8>,
    inputs: Inputs,
}

fn image() -> &'static Image {
    static IMAGE: OnceLock<Image> = OnceLock::new();
    IMAGE.get_or_init(|| {
        let fixture = crate::bootstrap::tests::linux::Fixture::new();
        let package = fixture.build().unwrap();
        let compressed = fs::read(fixture.output().join("rootfs.ext4.zst")).unwrap();
        let expanded = zstd::stream::decode_all(compressed.as_slice()).unwrap();
        let original = vec![0xa5; expanded.len() + 4096];
        Image {
            compressed,
            expanded,
            inputs: Inputs {
                binding_sha256: Sha256::digest(b"binding"),
                package_sha256: Sha256::digest(b"selected package"),
                raw_copies_sha256: Sha256::digest(b"verified raw copies"),
                data_receipt_sha256: Sha256::digest(b"verified archive"),
                originals_sha256: Sha256::digest(b"verified originals"),
                replacement_journal_sha256: Sha256::digest(b"completed replacements"),
                original_inactive: Blob {
                    bytes: original.len() as u64,
                    sha256: Sha256::digest(&original),
                },
                compressed: package.compressed,
                expanded: package.expanded,
                image: package.image,
                target: package.plan.target,
            },
        }
    })
}

struct Fixture {
    _temp: tempfile::TempDir,
    active: File,
    target: PathBuf,
    owner: u32,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("active");
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let target = temp.path().join("inactive.bin");
        fs::write(
            &target,
            vec![0xa5; image().inputs.original_inactive.bytes as usize],
        )
        .unwrap();
        fs::write(temp.path().join("payload.zst"), &image().compressed).unwrap();
        fs::write(
            temp.path().join("inputs.json"),
            serde_json::to_vec(&image().inputs).unwrap(),
        )
        .unwrap();
        Self {
            _temp: temp,
            active: File::open(path).unwrap(),
            target,
            owner: unsafe { libc::geteuid() },
        }
    }
    fn run(
        &self,
        report: &mut impl FnMut(RootInstallProgress) -> Result<()>,
    ) -> Result<RootInstallReport> {
        let mut tools = ImageTools::default();
        apply(
            &self.active,
            self.owner,
            &image().inputs,
            &mut Cursor::new(&image().compressed),
            || FileTarget::open(&self.target, Failure::None),
            |_| Ok(()),
            |file| tools.check(file),
            report,
        )
    }
    fn journal(&self) -> Option<Vec<u8>> {
        optional_record(
            &File::open(self._temp.path().join("active/root-install")).ok()?,
            RECORD,
            self.owner,
        )
        .unwrap()
    }
    fn phase(&self) -> Option<Phase> {
        self.journal().map(|bytes| {
            serde_json::from_slice::<Envelope>(&bytes)
                .unwrap()
                .record
                .phase
        })
    }
    fn assert_image(&self) {
        let bytes = fs::read(&self.target).unwrap();
        assert_eq!(&bytes[..image().expanded.len()], &image().expanded);
        assert!(bytes[image().expanded.len()..].iter().all(|b| *b == 0xa5));
    }
}

#[derive(Clone, Copy)]
enum Failure {
    None,
    Write,
    PartialWrite,
    Flush,
    Readback,
}
struct FileTarget {
    file: File,
    capacity: u64,
    written: u64,
    failure: Failure,
}
impl FileTarget {
    fn open(path: &Path, failure: Failure) -> Result<Self> {
        // The child test receives a fixture path through its environment. Pin
        // and classify it before IO so even a mistaken path cannot open a device.
        let pinned = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_NOFOLLOW)
            .open(path)?;
        if !pinned.metadata()?.is_file() || pinned.metadata()?.nlink() != 1 {
            return Err(invalid("test target must be a single regular file"));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/proc/self/fd/{}", pinned.as_raw_fd()))?;
        Ok(Self {
            capacity: file.metadata()?.len(),
            file,
            written: 0,
            failure,
        })
    }
}
impl Read for FileTarget {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if matches!(self.failure, Failure::Readback) && self.written > 0 {
            return Err(std::io::Error::other("injected readback failure"));
        }
        self.file.read(bytes)
    }
}
impl Write for FileTarget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if matches!(self.failure, Failure::Write) {
            return Err(std::io::Error::other("injected write failure"));
        }
        let bytes = if matches!(self.failure, Failure::PartialWrite) {
            if self.written >= 8192 {
                return Err(std::io::Error::other("injected partial-write failure"));
            }
            &bytes[..bytes.len().min((8192 - self.written) as usize)]
        } else {
            bytes
        };
        let count = self.file.write(bytes)?;
        self.written += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl Seek for FileTarget {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(position)
    }
}
impl Target for FileTarget {
    fn capacity(&self) -> u64 {
        self.capacity
    }
    fn file(&self) -> &File {
        &self.file
    }
    fn flush_to_device(&mut self) -> Result<()> {
        if matches!(self.failure, Failure::Flush) {
            return Err(invalid("injected flush failure"));
        }
        self.file.sync_all()?;
        Ok(())
    }
}

#[test]
fn real_ext4_install_retains_unused_tail_and_rechecks_completed_image_without_writing() {
    let f = Fixture::new();
    let report = f.run(&mut |_| Ok(())).unwrap();
    assert!(!report.ready_for_trial);
    assert!(!report.reused_written);
    f.assert_image();
    let before = f.journal().unwrap();
    let mut tools = ImageTools::default();
    let repeated = apply(
        &f.active,
        f.owner,
        &image().inputs,
        &mut Cursor::new(&image().compressed),
        || FileTarget::open(&f.target, Failure::Write),
        |_| Ok(()),
        |file| tools.check(file),
        &mut |_| Ok(()),
    )
    .unwrap();
    assert!(repeated.reused_written);
    assert_eq!(repeated.journal_sha256, report.journal_sha256);
    assert_eq!(before, f.journal().unwrap());
    f.assert_image();
}

#[test]
fn every_reported_boundary_resumes_with_the_original_selection_and_backup_binding() {
    let initial = Fixture::new();
    let mut count = 0;
    initial
        .run(&mut |_| {
            count += 1;
            Ok(())
        })
        .unwrap();
    for cut in 0..count {
        let f = Fixture::new();
        let mut reached = 0;
        assert!(
            f.run(&mut |_| {
                let current = reached;
                reached += 1;
                if current == cut {
                    Err(invalid("injected interruption"))
                } else {
                    Ok(())
                }
            })
            .is_err(),
            "cut {cut}"
        );
        let already_written = f.phase() == Some(Phase::Written);
        let report = f.run(&mut |_| Ok(())).unwrap();
        assert_eq!(report.reused_written, already_written);
        f.assert_image();
    }
}

#[test]
fn altered_original_and_completed_targets_are_never_silently_overwritten() {
    for completed in [false, true] {
        let f = Fixture::new();
        if completed {
            f.run(&mut |_| Ok(())).unwrap();
        }
        let before = f.journal();
        let mut file = OpenOptions::new().write(true).open(&f.target).unwrap();
        file.write_all(b"operator edit").unwrap();
        let changed = fs::read(&f.target).unwrap();
        assert!(f.run(&mut |_| Ok(())).is_err());
        assert_eq!(fs::read(&f.target).unwrap(), changed);
        assert_eq!(f.journal(), before);
    }
}

#[test]
fn write_flush_readback_and_image_check_failures_cannot_publish_written() {
    for failure in [
        Failure::Write,
        Failure::PartialWrite,
        Failure::Flush,
        Failure::Readback,
    ] {
        let f = Fixture::new();
        let mut tools = ImageTools::default();
        assert!(
            apply(
                &f.active,
                f.owner,
                &image().inputs,
                &mut Cursor::new(&image().compressed),
                || FileTarget::open(&f.target, failure),
                |_| Ok(()),
                |file| tools.check(file),
                &mut |_| Ok(())
            )
            .is_err()
        );
        assert_eq!(f.phase(), Some(Phase::Writing));
        f.run(&mut |_| Ok(())).unwrap();
        f.assert_image();
    }
    for failure in ["fsck", "mutation", "identity"] {
        let f = Fixture::new();
        assert!(
            apply(
                &f.active,
                f.owner,
                &image().inputs,
                &mut Cursor::new(&image().compressed),
                || FileTarget::open(&f.target, Failure::None),
                |_| Ok(()),
                |file| {
                    if failure == "fsck" {
                        return Err(invalid("injected fsck failure"));
                    }
                    if failure == "identity" {
                        let mut wrong = image().inputs.image.clone();
                        wrong.boot_identity = Sha256::digest(b"another boot image");
                        return Ok(wrong);
                    }
                    let mut file = file.try_clone()?;
                    file.seek(SeekFrom::Start(9000))?;
                    file.write_all(b"changed by checker")?;
                    Ok(image().inputs.image.clone())
                },
                &mut |_| Ok(())
            )
            .is_err()
        );
        assert_eq!(f.phase(), Some(Phase::Writing));
    }
}

#[test]
fn conflicting_or_corrupt_records_and_payloads_fail_before_opening_the_target() {
    let f = Fixture::new();
    f.run(&mut |event| {
        if matches!(event, RootInstallProgress::WritingRecorded) {
            Err(invalid("stop before write"))
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    let before = f.journal().unwrap();
    let try_inputs = |inputs: &Inputs, bytes: &[u8]| {
        let opened = Cell::new(false);
        let result = apply(
            &f.active,
            f.owner,
            inputs,
            &mut Cursor::new(bytes),
            || {
                opened.set(true);
                FileTarget::open(&f.target, Failure::None)
            },
            |_| Ok(()),
            |_| Ok(image().inputs.image.clone()),
            &mut |_| Ok(()),
        );
        assert!(result.is_err());
        assert!(!opened.get());
    };
    let mut changed = image().inputs.clone();
    changed.raw_copies_sha256 = Sha256::digest(b"another backup");
    try_inputs(&changed, &image().compressed);
    try_inputs(&image().inputs, b"corrupt payload");
    assert_eq!(before, f.journal().unwrap());
    let record = f._temp.path().join("active/root-install/record.json");
    fs::write(&record, b"torn published record").unwrap();
    try_inputs(&image().inputs, &image().compressed);
    assert_eq!(fs::read(record).unwrap(), b"torn published record");
}

#[test]
fn pending_files_cannot_override_published_state_or_redirect_writes() {
    let f = Fixture::new();
    f.run(&mut |_| Ok(())).unwrap();
    let before = f.journal().unwrap();
    let pending = f._temp.path().join("active/root-install/.record.pending");
    fs::write(&pending, b"incomplete next record").unwrap();
    fs::set_permissions(&pending, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(f.run(&mut |_| Ok(())).unwrap().reused_written);
    assert_eq!(before, f.journal().unwrap());
    fs::remove_file(&pending).unwrap();
    symlink(&f.target, &pending).unwrap();
    assert!(f.run(&mut |_| Ok(())).is_err());
    f.assert_image();
}

#[test]
fn changed_guard_and_concurrent_transaction_stop_before_the_first_byte() {
    let f = Fixture::new();
    let deny = Rc::new(Cell::new(false));
    let guard_deny = deny.clone();
    assert!(
        apply(
            &f.active,
            f.owner,
            &image().inputs,
            &mut Cursor::new(&image().compressed),
            || FileTarget::open(&f.target, Failure::None),
            |_| {
                if guard_deny.get() {
                    Err(invalid("reservation changed"))
                } else {
                    Ok(())
                }
            },
            |_| Ok(image().inputs.image.clone()),
            &mut |event| {
                if matches!(event, RootInstallProgress::WritingRecorded) {
                    assert!(matches!(f.run(&mut |_| Ok(())), Err(Error::Busy)));
                    deny.set(true);
                }
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(
        Sha256::digest(&fs::read(&f.target).unwrap()),
        image().inputs.original_inactive.sha256
    );
    assert_eq!(f.phase(), Some(Phase::Writing));
    f.run(&mut |_| Ok(())).unwrap();
}

#[test]
fn abrupt_process_exit_after_intent_flush_and_completion_is_restartable() {
    const CHILD: &str = "SPACEWARS_ROOT_INSTALL_CRASH_FIXTURE";
    if let Some(path) = std::env::var_os(CHILD) {
        let path = PathBuf::from(path);
        let inputs: Inputs =
            serde_json::from_slice(&fs::read(path.join("inputs.json")).unwrap()).unwrap();
        let cut = std::env::var("SPACEWARS_ROOT_INSTALL_CUT").unwrap();
        let active = File::open(path.join("active")).unwrap();
        let mut source = File::open(path.join("payload.zst")).unwrap();
        let mut tools = ImageTools::default();
        apply(
            &active,
            active.metadata().unwrap().uid(),
            &inputs,
            &mut source,
            || FileTarget::open(&path.join("inactive.bin"), Failure::None),
            |_| Ok(()),
            |file| tools.check(file),
            &mut |event| {
                let stop = match event {
                    RootInstallProgress::WritingRecorded => cut == "intent",
                    RootInstallProgress::Image(p) => {
                        cut == "flushed" && p.stage == WriteStage::ReadingBack
                    }
                    RootInstallProgress::WrittenRecorded => cut == "written",
                    _ => false,
                };
                if stop {
                    unsafe { libc::_exit(86) };
                }
                Ok(())
            },
        )
        .unwrap();
        panic!("crash boundary was not reached");
    }
    for cut in ["intent", "flushed", "written"] {
        let f = Fixture::new();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "bootstrap::root_install::tests::abrupt_process_exit_after_intent_flush_and_completion_is_restartable", "--nocapture"])
            .env(CHILD, f._temp.path()).env("SPACEWARS_ROOT_INSTALL_CUT", cut).status().unwrap();
        assert_eq!(status.code(), Some(86));
        assert_eq!(
            f.phase(),
            Some(if cut == "written" {
                Phase::Written
            } else {
                Phase::Writing
            })
        );
        assert_eq!(
            f.run(&mut |_| Ok(())).unwrap().reused_written,
            cut == "written"
        );
        f.assert_image();
    }
}
