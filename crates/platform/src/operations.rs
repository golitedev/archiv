//! Descriptor-relative recovery, durable manifests and explicit external application launches.
use archiv_core::actions::{FileIdentity, ProtectedPath, RECOVERY_DIR, RecoveryPolicy};
use archiv_core::{Entry, EntryKind, RelativePath, Source};
use rustix::fs::{self as unix, AtFlags, Mode, OFlags, RenameFlags, ResolveFlags};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, Metadata};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::{
    ffi::{OsStrExt, OsStringExt},
    fs::MetadataExt,
};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Identity of a no-follow metadata snapshot.
pub fn identity(metadata: &Metadata) -> FileIdentity {
    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        changed: metadata.ctime(),
        changed_ns: metadata.ctime_nsec(),
    }
}
fn same(a: FileIdentity, b: FileIdentity) -> bool {
    a.device == b.device && a.inode == b.inode
}
fn filesystem_id(root: &File) -> Result<u64, String> {
    let id = unix::fstatvfs(root).map_err(|e| e.to_string())?.f_fsid;
    if id == 0 {
        return Err("Filesystem does not expose a stable recovery identity".into());
    }
    Ok(id)
}
fn descriptor_path(file: &File) -> PathBuf {
    format!("/proc/self/fd/{}", file.as_raw_fd()).into()
}
fn flags() -> OFlags {
    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW
}
fn root(source: &Source) -> Result<File, String> {
    let root = File::from(
        unix::openat2(unix::CWD, &source.root, flags(), Mode::empty(), ResolveFlags::NO_SYMLINKS)
            .map_err(|e| e.to_string())?,
    );
    verify(source, &root)?;
    Ok(root)
}

fn verify(source: &Source, held: &File) -> Result<(), String> {
    if let Some(remote) = &source.nfs_remote {
        crate::mount::verify(&source.root, remote, held).map_err(|e| e.to_string())?;
    }
    let current = File::from(
        unix::openat2(unix::CWD, &source.root, flags(), Mode::empty(), ResolveFlags::NO_SYMLINKS)
            .map_err(|e| e.to_string())?,
    );
    if !same(
        identity(&held.metadata().map_err(|e| e.to_string())?),
        identity(&current.metadata().map_err(|e| e.to_string())?),
    ) {
        return Err("Source root changed; no fallback location is allowed".into());
    }
    Ok(())
}
fn beneath(root: &File, path: &Path, open: OFlags) -> Result<File, String> {
    let path = if path.as_os_str().is_empty() { Path::new(".") } else { path };
    unix::openat2(
        root,
        path,
        open,
        Mode::empty(),
        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_XDEV,
    )
    .map(File::from)
    .map_err(|e| format!("Cannot resolve physical target without links or mount changes: {e}"))
}
fn inspect(parent: &File, name: &std::ffi::OsStr) -> Result<Metadata, String> {
    fs::symlink_metadata(descriptor_path(parent).join(name)).map_err(|e| e.to_string())
}
fn check_policy(policy: &RecoveryPolicy, entry: &Entry) -> Result<(), String> {
    policy.permits(&entry.id.path, entry.kind)
}

fn check_guards(source: &Source, entry: &Entry, guards: &[(Source, ProtectedPath)]) -> Result<(), String> {
    let target_root = root(source)?;
    let mut ancestor_ids = Vec::new();
    let mut current = entry.id.path.parent();
    while let Some(path) = current {
        if ancestor_ids.len() >= 64 {
            return Err("Target depth exceeds mutation limit".into());
        }
        let directory = beneath(&target_root, path.as_path(), flags())?;
        ancestor_ids.push(identity(&directory.metadata().map_err(|e| e.to_string())?));
        current = path.parent();
    }
    for (guard_source, guard) in guards {
        let held_root = root(guard_source).map_err(|e| format!("Cannot revalidate configured protections: {e}"))?;
        // Lexical checks catch structural roots even before filesystem identity lookup.
        let guarded = guard_source.root.join(guard.path.as_path());
        let target = source.root.join(entry.id.path.as_path());
        if guarded.starts_with(&target) || (guard.subtree && target.starts_with(&guarded)) {
            return Err("A configured protected folder would be affected".into());
        }
        let mut current = Some(guard.path.clone());
        let mut depth = 0usize;
        while let Some(path) = current {
            depth = depth.saturating_add(1);
            if depth > 64 {
                return Err("Protection depth exceeds limit".into());
            }
            let held = beneath(&held_root, path.as_path(), flags())?;
            let observed = identity(&held.metadata().map_err(|e| e.to_string())?);
            if same(observed, entry.identity)
                || (path == guard.path
                    && guard.subtree
                    && ancestor_ids.iter().any(|ancestor| same(*ancestor, observed)))
            {
                return Err("Protected filesystem identity or ancestor would be affected through another source".into());
            }
            current = path.parent();
        }
        verify(guard_source, &held_root)?;
    }
    Ok(())
}

/// Immutable confirmation snapshot; the engine supplies a single-use request identity.
#[derive(Clone, Debug)]
pub struct RecoveryPlan {
    /// Single-use preview identity.
    pub id: u64,
    /// Explicit physical source.
    pub source: Source,
    /// Exact observed item; never expanded from a virtual directory.
    pub entry: Entry,
    /// Policy validated again before execution.
    pub policy: RecoveryPolicy,
    /// Held-root identity observed while preparing.
    pub root_identity: FileIdentity,
    /// Cross-source structural protections, checked by filesystem identity as well as path.
    pub guards: Vec<(Source, ProtectedPath)>,
}
/// Prepare without modifying the source; fail when the listing snapshot is stale.
pub fn prepare(
    id: u64,
    source: Source,
    entry: Entry,
    policy: RecoveryPolicy,
    guards: Vec<(Source, ProtectedPath)>,
) -> Result<RecoveryPlan, String> {
    if source.id != entry.id.source || policy.source != source.id {
        return Err("Source and policy identities do not match".into());
    }
    check_policy(&policy, &entry)?;
    check_guards(&source, &entry, &guards)?;
    let root = root(&source)?;
    let parent = entry.id.path.parent().ok_or("Source roots are protected")?;
    let directory = beneath(&root, parent.as_path(), flags())?;
    let name = entry.id.path.as_path().file_name().ok_or("Missing target name")?;
    if identity(&inspect(&directory, name)?) != entry.identity {
        return Err("Item changed since listing; refresh before trying again".into());
    }
    let root_identity = identity(&root.metadata().map_err(|e| e.to_string())?);
    Ok(RecoveryPlan { id, source, entry, policy, root_identity, guards })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    original: Vec<u8>,
    root: FileIdentity,
    filesystem: u64,
    object: FileIdentity,
    directory: bool,
    state: String,
}
#[cfg(test)]
std::thread_local! { static FAIL_JOURNAL_WRITE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
fn journal_write(directory: &File, journal: &Journal) -> Result<(), String> {
    #[cfg(test)]
    if FAIL_JOURNAL_WRITE.with(|counter| {
        let value = counter.get();
        counter.set(value.saturating_sub(1));
        value == 1
    }) {
        return Err("Injected storage-full/persistence failure".into());
    }
    let bytes = serde_json::to_vec(journal).map_err(|e| e.to_string())?;
    let path = descriptor_path(directory);
    let mut temp = tempfile::NamedTempFile::new_in(&path).map_err(|e| e.to_string())?;
    temp.write_all(&bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path.join("journal.json")).map_err(|e| e.to_string())?;
    directory.sync_all().map_err(|e| e.to_string())
}
fn journal_read(directory: &File) -> Result<Journal, String> {
    let file = beneath(
        directory,
        Path::new("journal.json"),
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
    )?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Recovery manifest is not a regular file".into());
    }
    let mut data = Vec::new();
    file.take(65537).read_to_end(&mut data).map_err(|e| e.to_string())?;
    if data.len() > 65536 {
        return Err("Recovery manifest exceeds limit".into());
    }
    let journal: Journal = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if journal.original.len() > 16384 || !matches!(journal.state.as_str(), "prepared" | "recovered" | "restored") {
        return Err("Invalid recovery manifest state or path length".into());
    }
    if journal.version != 1 {
        return Err("Unsupported recovery manifest version".into());
    }
    Ok(journal)
}
fn store(root: &File, create: bool) -> Result<File, String> {
    if create {
        match unix::mkdirat(root, RECOVERY_DIR, Mode::from_raw_mode(0o700)) {
            Ok(()) => root.sync_all().map_err(|e| e.to_string())?,
            Err(error) if error == rustix::io::Errno::EXIST => {}
            Err(error) => return Err(format!("Cannot create same-filesystem recovery storage: {error}")),
        }
    }
    let directory = beneath(root, Path::new(RECOVERY_DIR), flags())?;
    let metadata = directory.metadata().map_err(|e| e.to_string())?;
    if metadata.mode() & 0o077 != 0 {
        return Err("Recovery storage must be private (mode 0700); no permission change was attempted".into());
    }
    Ok(directory)
}
/// Recover one explicitly confirmed item. Never purge, overwrite, copy across disks or follow links.
/// Any post-rename failure keeps the object and its prewritten manifest in the recovery job.
pub fn recover(plan: &RecoveryPlan) -> Result<String, String> {
    if plan.source.id != plan.entry.id.source || plan.policy.source != plan.source.id {
        return Err("Source and policy identities do not match".into());
    }
    check_policy(&plan.policy, &plan.entry)?;
    check_guards(&plan.source, &plan.entry, &plan.guards)?;
    let root = root(&plan.source)?;
    if !same(identity(&root.metadata().map_err(|e| e.to_string())?), plan.root_identity) {
        return Err("Source changed after preview".into());
    }
    let parent_path = plan.entry.id.path.parent().ok_or("Source roots are protected")?;
    let parent = beneath(&root, parent_path.as_path(), flags())?;
    let name = plan.entry.id.path.as_path().file_name().ok_or("Missing filename")?;
    if identity(&inspect(&parent, name)?) != plan.entry.identity {
        return Err("Item changed after preview; nothing was moved".into());
    }
    let recovery = store(&root, true)?;
    let job =
        tempfile::Builder::new().prefix("item-").tempdir_in(descriptor_path(&recovery)).map_err(|e| e.to_string())?;
    let job_path = job.keep();
    let job_name = job_path.file_name().ok_or("Missing recovery identity")?.to_string_lossy().into_owned();
    let directory = beneath(&recovery, Path::new(&job_name), flags())?;
    let mut journal = Journal {
        version: 1,
        original: plan.entry.id.path.as_path().as_os_str().as_bytes().to_vec(),
        root: plan.root_identity,
        filesystem: filesystem_id(&root)?,
        object: plan.entry.identity,
        directory: plan.entry.kind == EntryKind::Directory,
        state: "prepared".into(),
    };
    journal_write(&directory, &journal)?;
    recovery.sync_all().map_err(|e| e.to_string())?;
    // Re-resolve the parent and revalidate immediately before the atomic rename.
    let current_parent = beneath(&root, parent_path.as_path(), flags())?;
    if !same(
        identity(&parent.metadata().map_err(|e| e.to_string())?),
        identity(&current_parent.metadata().map_err(|e| e.to_string())?),
    ) || identity(&inspect(&parent, name)?) != plan.entry.identity
    {
        return Err("Parent or target changed; nothing was moved".into());
    }
    verify(&plan.source, &root)?;
    unix::renameat_with(&parent, name, &directory, "payload", RenameFlags::NOREPLACE)
        .map_err(|e| format!("Recovery move refused: {e}; source was not removed by Archiv"))?;
    let complete = || -> Result<(), String> {
        parent.sync_all().map_err(|e| e.to_string())?;
        directory.sync_all().map_err(|e| e.to_string())?;
        if !same(identity(&inspect(&directory, std::ffi::OsStr::new("payload"))?), plan.entry.identity) {
            return Err("Concurrent target replacement detected; retained payload requires inspection".into());
        }
        verify(&plan.source, &root)?;
        let reachable = beneath(&root, parent_path.as_path(), flags())?;
        if !same(
            identity(&reachable.metadata().map_err(|e| e.to_string())?),
            identity(&parent.metadata().map_err(|e| e.to_string())?),
        ) {
            return Err("Parent changed during recovery; retained payload requires inspection".into());
        }
        Ok(())
    };
    if let Err(error) = complete() {
        return Err(format!(
            "{error}. Data retained in {RECOVERY_DIR}/{job_name}; do not retry blindly. Open Recovery to inspect it."
        ));
    }
    journal.state = "recovered".into();
    journal_write(&directory, &journal).map_err(|e| {
        format!("{e}. Payload retained at {RECOVERY_DIR}/{job_name}; Recovery can reconcile the prepared record.")
    })?;
    Ok(job_name)
}
/// A bounded, reconciled recovery record, including interrupted prepared moves.
#[derive(Clone, Debug)]
pub struct RecoveryRecord {
    /// Opaque job directory identity.
    pub id: String,
    /// Exact source-relative original path.
    pub original: RelativePath,
    /// True if durable completion was recorded; false means an interrupted move was reconciled.
    pub completed: bool,
}
/// Explicitly inspect at most 1000 recovery jobs. Invalid manifests produce an error, never deletion.
pub fn list_recovery(source: &Source) -> Result<Vec<RecoveryRecord>, String> {
    let root = root(source)?;
    let recovery = match beneath(&root, Path::new(RECOVERY_DIR), flags()) {
        Ok(dir) => dir,
        Err(_)
            if matches!(
                unix::statat(&root, RECOVERY_DIR, AtFlags::SYMLINK_NOFOLLOW),
                Err(rustix::io::Errno::NOENT)
            ) =>
        {
            return Ok(Vec::new());
        }
        Err(error) => return Err(error),
    };
    let mut result = Vec::new();
    for (index, item) in fs::read_dir(descriptor_path(&recovery)).map_err(|e| e.to_string())?.enumerate() {
        if index >= 1000 {
            return Err("Recovery has more than 1000 records; no records were removed".into());
        }
        let item = item.map_err(|e| e.to_string())?;
        let directory = beneath(&recovery, Path::new(&item.file_name()), flags())?;
        if matches!(unix::statat(&directory, "journal.json", AtFlags::SYMLINK_NOFOLLOW), Err(rustix::io::Errno::NOENT))
            && matches!(unix::statat(&directory, "payload", AtFlags::SYMLINK_NOFOLLOW), Err(rustix::io::Errno::NOENT))
        {
            continue;
        }
        let journal = journal_read(&directory)
            .map_err(|e| format!("Cannot inspect recovery {}: {e}", archiv_core::display_name(&item.file_name())))?;
        if journal.filesystem != filesystem_id(&root)?
            || journal.root.inode != identity(&root.metadata().map_err(|e| e.to_string())?).inode
        {
            return Err("Recovery record belongs to a different source filesystem".into());
        }
        let original = RelativePath::new(PathBuf::from(std::ffi::OsString::from_vec(journal.original)))
            .map_err(|e| e.to_string())?;
        match inspect(&directory, std::ffi::OsStr::new("payload")) {
            Ok(metadata) if identity(&metadata).inode == journal.object.inode => result.push(RecoveryRecord {
                id: item.file_name().to_string_lossy().into_owned(),
                original,
                completed: journal.state == "recovered",
            }),
            Ok(_) => return Err("A recovery payload changed identity; manual inspection is needed".into()),
            Err(_)
                if matches!(
                    unix::statat(&directory, "payload", AtFlags::SYMLINK_NOFOLLOW),
                    Err(rustix::io::Errno::NOENT)
                ) =>
            {
                if journal.state == "prepared" || journal.state == "restored" {
                    continue;
                }
                let parent_path = original.parent().ok_or("Invalid recovery original path")?;
                let parent = beneath(&root, parent_path.as_path(), flags())?;
                let name = original.as_path().file_name().ok_or("Invalid original filename")?;
                if identity(&inspect(&parent, name)?).inode != journal.object.inode {
                    return Err("Recovery payload is absent and the original location cannot be reconciled".into());
                }
            }
            Err(error) => return Err(format!("Missing recovery payload: {error}")),
        }
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    verify(source, &root)?;
    Ok(result)
}
/// Restore to the exact original location, refusing collisions, links, mount changes and stale policy.
pub fn restore(
    source: &Source,
    policy: &RecoveryPolicy,
    id: &str,
    guards: &[(Source, ProtectedPath)],
) -> Result<(), String> {
    if source.id != policy.source {
        return Err("Recovery policy belongs to another source".into());
    }
    if !id.starts_with("item-") || Path::new(id).components().count() != 1 {
        return Err("Invalid recovery identity".into());
    }
    let root = root(source)?;
    let recovery = store(&root, false)?;
    let directory = beneath(&recovery, Path::new(id), flags())?;
    let mut journal = journal_read(&directory)?;
    let original = RelativePath::new(PathBuf::from(std::ffi::OsString::from_vec(journal.original.clone())))
        .map_err(|e| e.to_string())?;
    policy.permits(&original, if journal.directory { EntryKind::Directory } else { EntryKind::File })?;
    if journal.filesystem != filesystem_id(&root)?
        || journal.root.inode != identity(&root.metadata().map_err(|e| e.to_string())?).inode
        || journal.object.inode != identity(&inspect(&directory, std::ffi::OsStr::new("payload"))?).inode
    {
        return Err("Recovery identity changed; restore refused".into());
    }
    let payload = inspect(&directory, std::ffi::OsStr::new("payload"))?;
    let target = Entry {
        id: archiv_core::EntryId { source: source.id, path: original.clone() },
        name: original.as_path().file_name().unwrap_or_default().to_owned(),
        kind: if journal.directory { EntryKind::Directory } else { EntryKind::File },
        bytes: None,
        modified: None,
        mode: payload.mode(),
        identity: identity(&payload),
    };
    check_guards(source, &target, guards)?;
    let parent_path = original.parent().ok_or("Cannot restore a source root")?;
    let parent = beneath(&root, parent_path.as_path(), flags())?;
    let name = original.as_path().file_name().ok_or("Missing original name")?;
    verify(source, &root)?;
    unix::renameat_with(&directory, "payload", &parent, name, RenameFlags::NOREPLACE)
        .map_err(|e| format!("Restore refused (nothing overwritten): {e}"))?;
    parent
        .sync_all()
        .and_then(|()| directory.sync_all())
        .map_err(|e| format!("Restored at original path, but durability is uncertain: {e}"))?;
    journal.state = "restored".into();
    journal_write(&directory, &journal).map_err(|e| format!("Restored at original path; journal update failed: {e}"))
}
/// Owned external application handle. Process status IO stays in the platform boundary.
pub struct Application {
    child: Child,
    program: String,
}
impl Application {
    /// True when reaped successfully, false while running, or an actionable exit/wait failure.
    pub fn poll(&mut self) -> Result<bool, String> {
        match self.child.try_wait().map_err(|e| format!("Cannot reap {}: {e}", self.program))? {
            None => Ok(false),
            Some(status) if status.success() => Ok(true),
            Some(status) => Err(format!("{} exited unsuccessfully: {status}", self.program)),
        }
    }
}
/// Launch an explicit regular file with separate arguments and no shell. Caller reaps the child.
pub fn launch(source: &Source, entry: &Entry, program: &str) -> Result<Application, String> {
    if entry.kind != EntryKind::File || program.is_empty() {
        return Err("Choose a regular file and configure its application in Preferences".into());
    }
    let root = root(source)?;
    let file = beneath(
        &root,
        entry.id.path.as_path(),
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
    )?;
    if identity(&file.metadata().map_err(|e| e.to_string())?) != entry.identity {
        return Err("File changed since listing; refresh before opening".into());
    }
    verify(source, &root)?;
    Command::new(program)
        .arg("--")
        .arg(source.root.join(entry.id.path.as_path()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|child| Application { child, program: program.to_owned() })
        .map_err(|e| format!("Cannot start {program}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DirectoryReader, LocalDirectoryReader, ReadRequest};
    use archiv_core::{EntryId, SourceId};
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::sync::atomic::AtomicBool;
    fn setup() -> (tempfile::TempDir, Source, RecoveryPolicy) {
        let fixture = tempfile::tempdir().unwrap();
        let source =
            Source { id: SourceId(1), root: fixture.path().to_owned(), label: "Fixture".into(), nfs_remote: None };
        let policy = RecoveryPolicy {
            source: source.id,
            enabled: true,
            allowed: vec![RelativePath::default()],
            protected: Vec::new(),
        };
        (fixture, source, policy)
    }
    fn entry(source: &Source, name: &str) -> Entry {
        let mut entries = Vec::new();
        let path = Path::new(name);
        let parent = path.parent().unwrap_or(Path::new(""));
        LocalDirectoryReader
            .read(
                &ReadRequest { source: source.clone(), path: RelativePath::new(parent).unwrap() },
                &AtomicBool::new(false),
                &mut |batch| {
                    entries.extend(batch);
                    true
                },
            )
            .unwrap();
        entries.into_iter().find(|e| e.id.path.as_path() == path).unwrap()
    }
    #[test]
    fn persistence_failures_before_and_after_rename_never_lose_payload() {
        for fail_at in [1, 2] {
            let (fixture, source, policy) = setup();
            fs::write(fixture.path().join("file"), b"safe").unwrap();
            let plan = prepare(1, source.clone(), entry(&source, "file"), policy.clone(), Vec::new()).unwrap();
            FAIL_JOURNAL_WRITE.with(|counter| counter.set(fail_at));
            assert!(recover(&plan).is_err());
            let records = list_recovery(&source).unwrap();
            if fail_at == 1 {
                assert_eq!(fs::read(fixture.path().join("file")).unwrap(), b"safe");
                assert!(records.is_empty());
            } else {
                assert!(!fixture.path().join("file").exists());
                assert_eq!(records.len(), 1);
                assert!(!records[0].completed);
                restore(&source, &policy, &records[0].id, &[]).unwrap();
                assert_eq!(fs::read(fixture.path().join("file")).unwrap(), b"safe");
            }
        }
    }
    #[test]
    fn unavailable_write_permission_leaves_original_untouched() {
        let (fixture, source, policy) = setup();
        fs::write(fixture.path().join("file"), b"safe").unwrap();
        let plan = prepare(1, source.clone(), entry(&source, "file"), policy, Vec::new()).unwrap();
        fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let result = recover(&plan);
        fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read(fixture.path().join("file")).unwrap(), b"safe");
    }
    #[test]
    fn recovery_roundtrip_preserves_directory_bytes_and_refuses_restore_collision() {
        let (fixture, source, policy) = setup();
        fs::create_dir_all(fixture.path().join("Title/Extras")).unwrap();
        fs::write(fixture.path().join("Title/Extras/movie.mkv"), b"untouched").unwrap();
        let plan = prepare(1, source.clone(), entry(&source, "Title"), policy.clone(), Vec::new()).unwrap();
        let id = recover(&plan).unwrap();
        assert!(!fixture.path().join("Title").exists());
        assert_eq!(
            fs::read(fixture.path().join(RECOVERY_DIR).join(&id).join("payload/Extras/movie.mkv")).unwrap(),
            b"untouched"
        );
        let records = list_recovery(&source).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].completed);
        fs::create_dir(fixture.path().join("Title")).unwrap();
        assert!(restore(&source, &policy, &id, &[]).is_err());
        assert!(fixture.path().join(RECOVERY_DIR).join(&id).join("payload").exists());
        fs::remove_dir(fixture.path().join("Title")).unwrap();
        restore(&source, &policy, &id, &[]).unwrap();
        assert_eq!(fs::read(fixture.path().join("Title/Extras/movie.mkv")).unwrap(), b"untouched");
        assert!(list_recovery(&source).unwrap().is_empty());
        assert!(recover(&plan).is_err());
    }
    #[test]
    fn stale_targets_parent_symlink_swaps_and_protected_aliases_fail() {
        let (fixture, source, policy) = setup();
        fs::create_dir(fixture.path().join("folder")).unwrap();
        fs::write(fixture.path().join("folder/file"), b"original").unwrap();
        let plan = prepare(1, source.clone(), entry(&source, "folder/file"), policy.clone(), Vec::new()).unwrap();
        fs::rename(fixture.path().join("folder/file"), fixture.path().join("folder/old")).unwrap();
        fs::write(fixture.path().join("folder/file"), b"replacement").unwrap();
        assert!(recover(&plan).is_err());
        assert_eq!(fs::read(fixture.path().join("folder/file")).unwrap(), b"replacement");
        let plan = prepare(2, source.clone(), entry(&source, "folder/file"), policy.clone(), Vec::new()).unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("file"), b"outside").unwrap();
        fs::rename(fixture.path().join("folder"), fixture.path().join("original-folder")).unwrap();
        symlink(outside.path(), fixture.path().join("folder")).unwrap();
        assert!(recover(&plan).is_err());
        assert_eq!(fs::read(outside.path().join("file")).unwrap(), b"outside");
        let guarded = ProtectedPath { path: RelativePath::new("original-folder").unwrap(), subtree: true };
        assert!(
            prepare(3, source.clone(), entry(&source, "original-folder/file"), policy, vec![(source.clone(), guarded)])
                .is_err()
        );
    }
    #[test]
    fn missing_mount_recovery_symlink_and_read_only_failure_preserve_source() {
        let (fixture, mut source, policy) = setup();
        fs::write(fixture.path().join("file"), b"safe").unwrap();
        let selected = entry(&source, "file");
        source.nfs_remote = Some("fixture:/missing".into());
        assert!(prepare(1, source.clone(), selected.clone(), policy.clone(), Vec::new()).is_err());
        source.nfs_remote = None;
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), fixture.path().join(RECOVERY_DIR)).unwrap();
        let plan = prepare(2, source.clone(), selected.clone(), policy.clone(), Vec::new()).unwrap();
        assert!(recover(&plan).is_err());
        assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
        fs::remove_file(fixture.path().join(RECOVERY_DIR)).unwrap();
        fs::create_dir(fixture.path().join(RECOVERY_DIR)).unwrap();
        fs::set_permissions(fixture.path().join(RECOVERY_DIR), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(recover(&plan).is_err());
        assert_eq!(fs::read(fixture.path().join("file")).unwrap(), b"safe");
        // A non-directory store simulates creation/open failure without requiring root/mount privileges.
        fs::remove_dir(fixture.path().join(RECOVERY_DIR)).unwrap();
        fs::write(fixture.path().join(RECOVERY_DIR), b"occupied").unwrap();
        assert!(recover(&plan).is_err());
        assert_eq!(fs::read(fixture.path().join("file")).unwrap(), b"safe");
    }
    #[test]
    fn interrupted_manifest_is_reconciled_and_non_utf8_original_is_preserved() {
        let (fixture, source, policy) = setup();
        let odd = std::ffi::OsString::from_vec(vec![b'f', 0xff]);
        fs::write(fixture.path().join(&odd), b"bytes").unwrap();
        let metadata = fs::symlink_metadata(fixture.path().join(&odd)).unwrap();
        let selected = Entry {
            id: EntryId { source: source.id, path: RelativePath::default().child(&odd).unwrap() },
            name: odd.clone(),
            kind: EntryKind::File,
            bytes: Some(5),
            modified: None,
            mode: metadata.mode(),
            identity: identity(&metadata),
        };
        let plan = prepare(1, source.clone(), selected, policy.clone(), Vec::new()).unwrap();
        let id = recover(&plan).unwrap();
        let root = root(&source).unwrap();
        let recovery = store(&root, false).unwrap();
        let directory = beneath(&recovery, Path::new(&id), flags()).unwrap();
        let mut journal = journal_read(&directory).unwrap();
        journal.state = "prepared".into();
        journal_write(&directory, &journal).unwrap();
        let records = list_recovery(&source).unwrap();
        assert!(!records[0].completed);
        assert_eq!(records[0].original.as_path().as_os_str(), odd);
        restore(&source, &policy, &id, &[]).unwrap();
        assert_eq!(fs::read(fixture.path().join(&odd)).unwrap(), b"bytes");
    }
    #[test]
    fn launch_uses_literal_arguments_and_revalidates_file_identity() {
        let (fixture, source, _) = setup();
        let name = "-- $(not-a-command).txt";
        fs::write(fixture.path().join(name), b"text").unwrap();
        let program = fixture.path().join("viewer");
        fs::write(&program,b"#!/usr/bin/python3\nimport json,sys,pathlib\npathlib.Path(__file__).with_suffix('.json').write_text(json.dumps(sys.argv[1:]))\n").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let selected = entry(&source, name);
        let mut child = launch(&source, &selected, program.to_str().unwrap()).unwrap();
        assert!(child.child.wait().unwrap().success());
        let args: Vec<String> = serde_json::from_slice(&fs::read(program.with_extension("json")).unwrap()).unwrap();
        assert_eq!(args, vec!["--".to_owned(), fixture.path().join(name).to_string_lossy().into_owned()]);
        fs::write(fixture.path().join(name), b"changed").unwrap();
        assert!(launch(&source, &selected, program.to_str().unwrap()).is_err());
    }
}
