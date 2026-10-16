//! Restartable fixed-file replacements. This is a filesystem transaction,
//! not evidence that privilege closure, services, access or boot are ready.
use super::backup::{check_named_directory, directory, inventory, writable};
use super::legacy_files::{self, Identity, Original, Snapshot};
use super::legacy_state::{self, Inputs};
use super::{BootstrapBinding, invalid};
use crate::owned_fs::{io_result, name, open_at, owned, read_file, unchanged};
use crate::{Error, Result, Sha256, verify_blob};
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::fs::{self, File};
use std::io::{Read, Seek, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};

pub(super) const DIRECTORY: &str = "legacy-migration";
const RECORD: &str = "record.json";
const PENDING: &str = ".record.pending";
const LIMIT: u64 = 256 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum Node {
    Directory(Identity),
    File(Original),
}

impl Node {
    fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Directory(a), Self::Directory(b)) => a == b,
            (Self::File(a), Self::File(b)) => a.after_rename(b),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum Step {
    Intent,
    Prepared(Node),
    Done(Node),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    originals_sha256: Sha256,
    /// A contiguous prefix of the fixed actions; only its last may be pending.
    steps: Vec<Step>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    journal: Journal,
    checksum: Sha256,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LegacyReplacementStage {
    Intent,
    DirectoryCreated,
    FileWritten,
    Prepared,
    Linked,
    Renamed,
    Flushed,
    Done,
}

use LegacyReplacementStage as Stage;

#[derive(Debug, Serialize)]
pub struct LegacyReplacementProgress {
    pub action: usize,
    pub stage: LegacyReplacementStage,
}

#[derive(Debug, Serialize)]
pub struct LegacyReplacementReport {
    pub schema: u32,
    pub scope: &'static str,
    pub originals_sha256: Sha256,
    pub journal_sha256: Sha256,
    pub files_replaced: usize,
    pub directories_created: usize,
    pub ready_for_trial: bool,
}

/// Indices are derived from the fixed receipt paths, never deserialized paths.
#[derive(Clone, Copy)]
enum Action {
    Directory(usize),
    File(usize),
}

impl Action {
    fn path(self, source: &Snapshot) -> String {
        match self {
            Self::Directory(i) => source.directories[i].path.clone(),
            Self::File(i) => legacy_files::targets()[i].0.into(),
        }
    }

    fn original(self, source: &Snapshot) -> Option<Node> {
        match self {
            Self::Directory(i) => source.directories[i].original.clone().map(Node::Directory),
            Self::File(i) => source.files[i].clone().map(Node::File),
        }
    }

    fn mode(self) -> u32 {
        match self {
            Self::Directory(i) if legacy_files::parents()[i] == "usr/libexec/spacewars" => 0o700,
            Self::Directory(_) => 0o755,
            Self::File(i) => legacy_files::targets()[i].1,
        }
    }
}

fn actions(source: &Snapshot) -> Vec<Action> {
    source
        .directories
        .iter()
        .enumerate()
        .filter(|(_, d)| d.original.is_none())
        .map(|(i, _)| Action::Directory(i))
        .chain((0..legacy_files::targets().len()).map(Action::File))
        .collect()
}

fn scratch(receipt: Sha256, index: usize) -> String {
    // Dots exclude sudoers snippets; .pending does not match SSH/systemd *.conf.
    format!(".spacewars-{receipt}-{index:02}.pending")
}

struct Transaction<'a> {
    root: &'a File,
    active: &'a File,
    directory: File,
    owner: u32,
    receipt: legacy_state::Receipt,
    journal: Journal,
    persisted: Option<Vec<u8>>,
    actions: Vec<Action>,
}

impl Transaction<'_> {
    fn persist(&mut self) -> Result<()> {
        self.check_store()?;
        let bytes = serde_json::to_vec(&Envelope {
            checksum: Sha256::digest(&serde_json::to_vec(&self.journal)?),
            journal: self.journal.clone(),
        })?;
        if bytes.len() as u64 > LIMIT {
            return Err(invalid("legacy migration record is too large"));
        }
        let mut file = writable(&self.directory, PENDING, self.owner)?;
        file.set_len(0)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        self.check_store()?;
        rename(
            &self.directory,
            PENDING,
            RECORD,
            if self.persisted.is_some() {
                0
            } else {
                libc::RENAME_NOREPLACE
            },
        )?;
        self.directory.sync_all()?;
        self.persisted = Some(bytes);
        self.check_store()
    }

    fn check_store(&self) -> Result<()> {
        check_named_directory(self.active, DIRECTORY, &self.directory, self.owner)?;
        inventory(&self.directory, &[RECORD, PENDING])?;
        let current = optional_record(&self.directory, RECORD, self.owner)?;
        if current != self.persisted {
            return Err(invalid("legacy migration journal changed"));
        }
        // A torn unpublished write may be overwritten, but never a foreign link,
        // special file, oversized object or file outside the private directory.
        optional_record(&self.directory, PENDING, self.owner)?;
        Ok(())
    }

    fn parent(&self, action: Action) -> Result<File> {
        let path = action.path(&self.receipt.source);
        let (parent, _) = split_path(&path);
        let file = legacy_files::directory(self.root, parent, self.owner)?
            .ok_or_else(|| invalid("legacy replacement parent is missing"))?;
        if parent.is_empty() {
            if Identity::from(&file.metadata()?) != self.receipt.source.root {
                return Err(invalid("legacy source root was replaced"));
            }
            return Ok(file);
        }
        let index = self
            .receipt
            .source
            .directories
            .iter()
            .position(|d| d.path == parent)
            .expect("fixed directory inventory");
        let mut expected = self.receipt.source.directories[index].original.clone();
        for (action, step) in self.actions.iter().zip(&self.journal.steps) {
            if matches!(action, Action::Directory(i) if *i == index)
                && let Step::Done(Node::Directory(identity)) = step
            {
                expected = Some(identity.clone());
            }
        }
        if expected.as_ref() != Some(&Identity::from(&file.metadata()?)) {
            return Err(invalid("legacy replacement parent was replaced"));
        }
        Ok(file)
    }

    fn observe(&self, parent: &File, component: &str, action: Action) -> Result<Option<Node>> {
        match action {
            Action::File(i) => Ok(legacy_files::file_at(
                self.root,
                parent,
                component,
                self.owner,
                legacy_files::limit(i),
                &mut std::io::sink(),
            )?
            .map(Node::File)),
            Action::Directory(_) => {
                let file = match open_at(parent, component, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
                    Ok(file) => file,
                    Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(None);
                    }
                    Err(e) => return Err(e),
                };
                let metadata = file.metadata()?;
                owned(&metadata, true, self.owner, false)?;
                if metadata.mode() & 0o7000 != 0
                    || metadata.dev() != self.receipt.source.root.device
                    || crate::recovery_linux::opened_mount_id(&file)?
                        != crate::recovery_linux::opened_mount_id(self.root)?
                {
                    return Err(invalid("unsafe legacy replacement directory"));
                }
                Ok(Some(Node::Directory(Identity::from(&metadata))))
            }
        }
    }

    fn state(&self, index: usize, parent: &File) -> Result<(Option<Node>, Option<Node>)> {
        let action = self.actions[index];
        let path = action.path(&self.receipt.source);
        let (_, component) = split_path(&path);
        Ok((
            self.observe(parent, component, action)?,
            self.observe(
                parent,
                &scratch(self.journal.originals_sha256, index),
                action,
            )?,
        ))
    }

    fn check_tree(&self) -> Result<()> {
        let mut expected = self.receipt.source.clone();
        let current = legacy_files::inspect(self.root, self.owner)?;
        for (index, action) in self.actions.iter().copied().enumerate() {
            let path = action.path(&self.receipt.source);
            let (parent_path, _) = split_path(&path);
            let Some(parent) = legacy_files::directory(self.root, parent_path, self.owner)? else {
                continue;
            };
            let (target, temporary) = self.state(index, &parent)?;
            let old = action.original(&self.receipt.source);
            let step = self.journal.steps.get(index);
            let applied = match step {
                Some(Step::Prepared(new) | Step::Done(new))
                    if pair_matches(&target, &temporary, &Some(new.clone()), &old) =>
                {
                    true
                }
                Some(Step::Prepared(new))
                    if pair_matches(&target, &temporary, &old, &Some(new.clone())) =>
                {
                    false
                }
                Some(Step::Prepared(_)) if target == old && temporary.is_none() => false,
                Some(Step::Intent) if target == old => {
                    if temporary.is_some() {
                        self.unprepared_directory(index, &parent)?;
                    }
                    false
                }
                None if target == old && temporary.is_none() => false,
                _ => {
                    return Err(invalid(
                        "legacy migration paths conflict with their journal",
                    ));
                }
            };
            if applied {
                match (action, target) {
                    (Action::Directory(i), Some(Node::Directory(d))) => {
                        expected.directories[i].original = Some(d)
                    }
                    (Action::File(i), Some(Node::File(f))) => expected.files[i] = Some(f),
                    _ => return Err(invalid("legacy migration node type mismatch")),
                }
            }
        }
        if current != expected {
            return Err(invalid("legacy migration source changed"));
        }
        Ok(())
    }

    fn validate_journal(&self) -> Result<()> {
        if self.journal.schema != 1 || self.journal.steps.len() > self.actions.len() {
            return Err(invalid("invalid legacy migration journal"));
        }
        for (index, step) in self.journal.steps.iter().enumerate() {
            if index + 1 < self.journal.steps.len() && !matches!(step, Step::Done(_)) {
                return Err(invalid("noncontiguous legacy migration journal"));
            }
            let node = match step {
                Step::Intent => continue,
                Step::Prepared(n) | Step::Done(n) => n,
            };
            let identity = match (self.actions[index], node) {
                (Action::Directory(_), Node::Directory(d)) => d,
                (Action::File(i), Node::File(f))
                    if f.attributes.is_empty()
                        && f.contents == self.receipt.inputs.replacements[i].contents =>
                {
                    &f.identity
                }
                _ => {
                    return Err(invalid(
                        "legacy migration candidate differs from its fixed action",
                    ));
                }
            };
            if identity.device != self.receipt.source.root.device
                || identity.inode == 0
                || identity.uid != self.owner
                || identity.gid != self.receipt.source.root.gid
                || identity.mode != self.actions[index].mode()
            {
                return Err(invalid("invalid legacy migration candidate identity"));
            }
        }
        Ok(())
    }

    fn require_space(&self) -> Result<()> {
        // Retain every old inode. Credit only candidates whose named bytes
        // already exist; an anonymous candidate lost on restart must be rebuilt.
        let mut bytes = 8u64 * 1024 * 1024;
        for (index, action) in self.actions.iter().copied().enumerate() {
            if matches!(self.journal.steps.get(index), Some(Step::Done(_))) {
                continue;
            }
            if matches!(self.journal.steps.get(index), Some(Step::Prepared(_))) {
                let parent = self.parent(action)?;
                let (target, temporary) = self.state(index, &parent)?;
                if temporary.is_some() || target != action.original(&self.receipt.source) {
                    continue;
                }
            }
            bytes = bytes
                .checked_add(match action {
                    Action::Directory(_) => 4096,
                    Action::File(i) => self.receipt.inputs.replacements[i].contents.bytes,
                })
                .ok_or_else(|| invalid("legacy replacement capacity overflow"))?;
        }
        if bytes > super::backup::free_space(self.root)? {
            return Err(invalid(
                "insufficient root filesystem space to retain originals during replacement",
            ));
        }
        Ok(())
    }

    /// A crash between mkdir and recording its inode leaves only an empty
    /// candidate directory. Never adopt a nonempty directory or any file here.
    fn unprepared_directory(&self, index: usize, parent: &File) -> Result<File> {
        let action = self.actions[index];
        if !matches!(action, Action::Directory(_)) {
            return Err(invalid("unrecorded legacy temporary file"));
        }
        let file = open_at(
            parent,
            &scratch(self.journal.originals_sha256, index),
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        let metadata = file.metadata()?;
        owned(&metadata, true, self.owner, false)?;
        if metadata.dev() != self.receipt.source.root.device
            || crate::recovery_linux::opened_mount_id(&file)?
                != crate::recovery_linux::opened_mount_id(self.root)?
            || ![0o700, action.mode()].contains(&(metadata.mode() & 0o7777))
            || !super::archive::attributes(&file)?.is_empty()
            || fs::read_dir(format!("/proc/self/fd/{}", file.as_raw_fd()))?
                .next()
                .is_some()
        {
            return Err(invalid("unrecognized unfinished legacy directory"));
        }
        Ok(file)
    }
}

fn pair_matches(
    a: &Option<Node>,
    b: &Option<Node>,
    expected_a: &Option<Node>,
    expected_b: &Option<Node>,
) -> bool {
    fn matches(a: &Option<Node>, b: &Option<Node>) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => a.matches(b),
            _ => false,
        }
    }
    matches(a, expected_a) && matches(b, expected_b)
}

fn optional_record(directory: &File, component: &str, owner: u32) -> Result<Option<Vec<u8>>> {
    match super::device::private_asset(directory, component, owner) {
        Ok(file) => Ok(Some(read_file(file, owner, LIMIT)?)),
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub(super) fn started(active: &File, owner: u32) -> Result<bool> {
    let Some(directory) = super::state::optional_directory(active, DIRECTORY, owner)? else {
        return Ok(false);
    };
    // A corrupt record is not permission to start again; apply validates it.
    Ok(optional_record(&directory, RECORD, owner)?.is_some())
}

/// Caller must hold the installation lock, check the package and backups,
/// and exclude running legacy helpers and runtime sessions for the full call.
/// `check` is repeated at each durable boundary; it is not a user-supplied flag.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply(
    root: &File,
    active: &File,
    owner: u32,
    binding: &BootstrapBinding,
    inputs: &Inputs,
    originals: Sha256,
    mut sources: Vec<File>,
    mut check: impl FnMut() -> Result<()>,
    progress: &mut impl FnMut(usize, Stage) -> Result<()>,
) -> Result<LegacyReplacementReport> {
    check()?;
    let receipt = legacy_state::verified_receipt(active, owner, binding, originals)?;
    if &receipt.inputs != inputs
        || receipt.source.root.uid != owner
        || sources.len() != inputs.replacements.len()
    {
        return Err(invalid(
            "legacy migration inputs differ from the preserved originals",
        ));
    }
    for (file, replacement) in sources.iter_mut().zip(&inputs.replacements) {
        let before = file.metadata()?;
        owned(&before, false, owner, true)?;
        verify_blob(file, &replacement.contents)?;
        if !unchanged(&before, &file.metadata()?) {
            return Err(invalid("legacy replacement changed"));
        }
    }
    let directory = directory(active, DIRECTORY, owner)?;
    // The external install lock excludes other maintenance. This lock also
    // excludes independently opened transaction handles in offline rehearsals.
    if unsafe { libc::flock(directory.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } < 0 {
        let error = std::io::Error::last_os_error();
        return Err(if error.kind() == std::io::ErrorKind::WouldBlock {
            Error::Busy
        } else {
            error.into()
        });
    }
    inventory(&directory, &[RECORD, PENDING])?;
    let persisted = optional_record(&directory, RECORD, owner)?;
    let journal = if let Some(bytes) = &persisted {
        let envelope: Envelope = serde_json::from_slice(bytes)?;
        if Sha256::digest(&serde_json::to_vec(&envelope.journal)?) != envelope.checksum
            || envelope.journal.originals_sha256 != originals
        {
            return Err(invalid("legacy migration journal identity mismatch"));
        }
        envelope.journal
    } else {
        Journal {
            schema: 1,
            originals_sha256: originals,
            steps: Vec::new(),
        }
    };
    let actions = actions(&receipt.source);
    let mut transaction = Transaction {
        root,
        active,
        directory,
        owner,
        receipt,
        journal,
        persisted,
        actions,
    };
    transaction.check_store()?;
    transaction.validate_journal()?;
    transaction.check_tree()?;
    transaction.require_space()?;
    if transaction.persisted.is_none() {
        check()?;
        transaction.persist()?;
    }
    for index in 0..transaction.actions.len() {
        check()?;
        transaction.check_store()?;
        if matches!(transaction.journal.steps.get(index), Some(Step::Done(_))) {
            continue;
        }
        let action = transaction.actions[index];
        let parent = transaction.parent(action)?;
        let path = action.path(&transaction.receipt.source);
        let (_, component) = split_path(&path);
        let temporary = scratch(originals, index);
        if transaction.journal.steps.len() == index {
            transaction.journal.steps.push(Step::Intent);
            transaction.persist()?;
            progress(index, Stage::Intent)?;
        }
        let (target, temp) = transaction.state(index, &parent)?;
        let old = action.original(&transaction.receipt.source);
        let published = match &transaction.journal.steps[index] {
            Step::Prepared(new) => pair_matches(&target, &temp, &Some(new.clone()), &old),
            _ => false,
        };
        if !published {
            if target != old {
                return Err(invalid("legacy target changed before replacement"));
            }
            if temp.is_none() || matches!(transaction.journal.steps[index], Step::Intent) {
                let candidate = match action {
                    Action::Directory(_) => {
                        if temp.is_none() {
                            check()?;
                            let component = name(&temporary)?;
                            io_result(unsafe {
                                libc::mkdirat(parent.as_raw_fd(), component.as_ptr(), 0o700)
                            })?;
                            parent.sync_all()?;
                            progress(index, Stage::DirectoryCreated)?;
                        }
                        let candidate = transaction.unprepared_directory(index, &parent)?;
                        set_metadata(
                            &candidate,
                            owner,
                            transaction.receipt.source.root.gid,
                            action.mode(),
                        )?;
                        candidate.sync_all()?;
                        let node = Node::Directory(Identity::from(&candidate.metadata()?));
                        (candidate, node)
                    }
                    Action::File(i) => {
                        if temp.is_some() {
                            return Err(invalid("unrecorded legacy temporary file"));
                        }
                        check()?;
                        let mut candidate = anonymous_file(&parent)?;
                        let source = &mut sources[i];
                        let before = source.metadata()?;
                        source.rewind()?;
                        let copied = std::io::copy(
                            &mut (&mut *source).take(inputs.replacements[i].contents.bytes + 1),
                            &mut candidate,
                        )?;
                        if copied != inputs.replacements[i].contents.bytes
                            || !unchanged(&before, &source.metadata()?)
                        {
                            return Err(invalid("legacy replacement changed during copy"));
                        }
                        set_metadata(
                            &candidate,
                            owner,
                            transaction.receipt.source.root.gid,
                            action.mode(),
                        )?;
                        candidate.sync_all()?;
                        progress(index, Stage::FileWritten)?;
                        let snapshot = legacy_files::opened_file(
                            root,
                            &candidate,
                            owner,
                            legacy_files::limit(i),
                            true,
                            &mut std::io::sink(),
                        )?;
                        if snapshot.contents != inputs.replacements[i].contents
                            || !snapshot.attributes.is_empty()
                        {
                            return Err(invalid(
                                "legacy replacement readback differs from its package",
                            ));
                        }
                        (candidate, Node::File(snapshot))
                    }
                };
                check()?;
                transaction.journal.steps[index] = Step::Prepared(candidate.1);
                transaction.persist()?;
                progress(index, Stage::Prepared)?;
                if matches!(action, Action::File(_)) {
                    check()?;
                    link_anonymous(&candidate.0, &parent, &temporary)?;
                    parent.sync_all()?;
                    progress(index, Stage::Linked)?;
                }
            }
            check()?;
            transaction.check_tree()?;
            // Reopen the complete ancestry just before rename; a pinned but
            // detached parent must not receive a silently misplaced update.
            let named = transaction.parent(action)?;
            if Identity::from(&named.metadata()?) != Identity::from(&parent.metadata()?) {
                return Err(invalid("legacy replacement parent moved"));
            }
            let (target, temp) = transaction.state(index, &parent)?;
            let Step::Prepared(new) = &transaction.journal.steps[index] else {
                return Err(invalid("missing prepared legacy replacement"));
            };
            if target != old || !pair_matches(&None, &temp, &None, &Some(new.clone())) {
                return Err(invalid("legacy replacement changed before rename"));
            }
            if let Some(Node::File(original)) = &old {
                let file = open_at(&parent, component, libc::O_RDONLY, 0)?;
                if Identity::from(&file.metadata()?) != original.identity {
                    return Err(invalid("original inode changed before its final flush"));
                }
                // Retain durable original bytes, including files that were
                // dirty before the operator took the backup.
                file.sync_all()?;
            }
            rename(
                &parent,
                &temporary,
                component,
                if old.is_some() {
                    libc::RENAME_EXCHANGE
                } else {
                    libc::RENAME_NOREPLACE
                },
            )?;
            progress(index, Stage::Renamed)?;
        }
        parent.sync_all()?;
        progress(index, Stage::Flushed)?;
        check()?;
        transaction.check_tree()?;
        let Step::Prepared(new) = &transaction.journal.steps[index] else {
            return Err(invalid("missing prepared legacy replacement"));
        };
        transaction.journal.steps[index] = Step::Done(new.clone());
        transaction.persist()?;
        progress(index, Stage::Done)?;
    }
    transaction.check_tree()?;
    if legacy_state::verified_receipt(active, owner, binding, originals)? != transaction.receipt {
        return Err(invalid("legacy originals changed during replacement"));
    }
    check()?;
    transaction.check_store()?;
    Ok(LegacyReplacementReport {
        schema: 1,
        scope: "fixed fallback-root files replaced; effective privilege, service and login checks still required",
        originals_sha256: originals,
        journal_sha256: Sha256::digest(&serde_json::to_vec(&transaction.journal)?),
        files_replaced: inputs.replacements.len(),
        directories_created: transaction.actions.len() - inputs.replacements.len(),
        ready_for_trial: false,
    })
}

fn set_metadata(file: &File, owner: u32, group: u32, mode: u32) -> Result<()> {
    let metadata = file.metadata()?;
    if metadata.uid() != owner || metadata.gid() != group {
        io_result(unsafe { libc::fchown(file.as_raw_fd(), owner, group) })?;
    }
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    if !super::archive::attributes(file)?.is_empty() {
        return Err(invalid(
            "unsupported inherited attributes on legacy replacement",
        ));
    }
    Ok(())
}

fn split_path(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

fn anonymous_file(parent: &File) -> Result<File> {
    // No persistent name exists until the complete bytes, mode and inode have
    // been flushed and recorded. A crash while copying simply drops the inode.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            c".".as_ptr(),
            libc::O_TMPFILE | libc::O_RDWR | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn link_anonymous(file: &File, parent: &File, component: &str) -> Result<()> {
    let source =
        CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).expect("descriptor path");
    let destination = name(component)?;
    // The proc descriptor link is ours, not an input path. This documented
    // O_TMPFILE publication also works without CAP_DAC_READ_SEARCH in fixtures.
    io_result(unsafe {
        libc::linkat(
            libc::AT_FDCWD,
            source.as_ptr(),
            parent.as_raw_fd(),
            destination.as_ptr(),
            libc::AT_SYMLINK_FOLLOW,
        )
    })
}

fn rename(parent: &File, from: &str, to: &str, flags: u32) -> Result<()> {
    let from = name(from)?;
    let to = name(to)?;
    io_result(unsafe {
        libc::renameat2(
            parent.as_raw_fd(),
            from.as_ptr(),
            parent.as_raw_fd(),
            to.as_ptr(),
            flags,
        )
    })
}

#[cfg(test)]
#[path = "bootstrap_legacy_replace_tests.rs"]
mod tests;
