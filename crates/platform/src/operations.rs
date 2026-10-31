//! Anchored file actions and explicit application launches.
use archiv_core::actions::{FileIdentity, ProtectedPath, WritePolicy};
use archiv_core::{Entry, EntryId, EntryKind, RelativePath, Source};
use rustix::fs::{self as unix, AtFlags, Mode, OFlags, RenameFlags, ResolveFlags};
use std::fs::{self, File, Metadata};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
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
fn current_parent(root: &File, path: &RelativePath, held: &File) -> Result<(), String> {
    let current = beneath(root, path.as_path(), flags())?;
    if !same(
        identity(&current.metadata().map_err(|e| e.to_string())?),
        identity(&held.metadata().map_err(|e| e.to_string())?),
    ) {
        return Err("Parent changed; file action stopped".into());
    }
    Ok(())
}
fn check_policy(policy: &WritePolicy, entry: &Entry) -> Result<(), String> {
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

const MAX_NODES: usize = 10_000;
const MAX_BYTES: u64 = 1 << 40;
fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) { Err("Cancelled; completed changes may remain".into()) } else { Ok(()) }
}
fn observed(root: &File, entry: &Entry) -> Result<File, String> {
    let held = beneath(root, entry.id.path.as_path(), OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK)?;
    if identity(&held.metadata().map_err(|e| e.to_string())?) != entry.identity {
        return Err("Item changed since listing; refresh before trying again".into());
    }
    Ok(held)
}
/// Validate a single filename, preserving exact bytes.
pub fn filename(name: &std::ffi::OsStr) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    let path = Path::new(name);
    if name.is_empty()
        || name.as_bytes().len() > 255
        || name.as_bytes().contains(&0)
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(std::path::Component::Normal(_)))
    {
        return Err("Use one filename without slashes, . or .. (up to 255 bytes)".into());
    }
    Ok(())
}
fn tree(source: &Source, top: &Entry, cancel: &AtomicBool) -> Result<Vec<Entry>, String> {
    let held_root = root(source)?;
    observed(&held_root, top)?;
    let mut nodes = vec![top.clone()];
    let mut index = 0usize;
    let mut bytes = 0u64;
    let mut seen = std::collections::HashSet::new();
    while let Some(node) = nodes.get(index).cloned() {
        cancelled(cancel)?;
        index = index.saturating_add(1);
        if node.id.path.as_path().components().count() > 64 {
            return Err("Folder depth exceeds 64".into());
        }
        let file = observed(&held_root, &node)?;
        match node.kind {
            EntryKind::File => {
                bytes = bytes.checked_add(node.bytes.ok_or("Missing file size")?).ok_or("Copy size overflow")?;
                if bytes > MAX_BYTES {
                    return Err("Operation exceeds 1 TiB limit".into());
                }
            }
            EntryKind::Directory => {
                if !seen.insert((node.identity.device, node.identity.inode)) {
                    return Err("Directory cycle refused".into());
                }
                for child in fs::read_dir(descriptor_path(&file)).map_err(|e| e.to_string())? {
                    cancelled(cancel)?;
                    if nodes.len() >= MAX_NODES {
                        return Err("Operation exceeds 10,000 items".into());
                    }
                    let child = child.map_err(|e| e.to_string())?;
                    let name = child.file_name();
                    let path = node.id.path.child(&name).map_err(|e| e.to_string())?;
                    let child =
                        beneath(&held_root, path.as_path(), OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK)?;
                    let metadata = child.metadata().map_err(|e| e.to_string())?;
                    let kind = if metadata.is_file() {
                        EntryKind::File
                    } else if metadata.is_dir() {
                        EntryKind::Directory
                    } else {
                        return Err("Links and special files are not supported by file actions".into());
                    };
                    nodes.push(Entry {
                        id: EntryId { source: source.id, path },
                        name,
                        kind,
                        bytes: metadata.is_file().then_some(metadata.len()),
                        modified: None,
                        mode: metadata.mode(),
                        identity: identity(&metadata),
                    });
                }
            }
            _ => return Err("Links and special files are not supported by file actions".into()),
        }
    }
    verify(source, &held_root)?;
    Ok(nodes)
}
/// Single-use engine deletion preview, including every physical descendant.
pub struct DeletePlan {
    /// Confirmation identity.
    pub id: u64,
    /// Physical source.
    pub source: Source,
    /// Explicit selected item.
    pub entry: Entry,
    policy: WritePolicy,
    guards: Vec<(Source, ProtectedPath)>,
    nodes: Vec<Entry>,
    root_identity: FileIdentity,
}
impl DeletePlan {
    /// Full physical scope for confirmation; no virtual expansion.
    pub fn entries(&self) -> &[Entry] {
        &self.nodes
    }
}
/// Read-only bounded preflight; rejects links, mounts, stale entries and protected descendants.
pub fn prepare_delete(
    id: u64,
    source: Source,
    entry: Entry,
    policy: WritePolicy,
    guards: Vec<(Source, ProtectedPath)>,
    cancel: &AtomicBool,
) -> Result<DeletePlan, String> {
    if source.id != entry.id.source || source.id != policy.source {
        return Err("Mismatched physical identities".into());
    }
    check_policy(&policy, &entry)?;
    check_guards(&source, &entry, &guards)?;
    let held = root(&source)?;
    let root_identity = identity(&held.metadata().map_err(|e| e.to_string())?);
    let nodes = tree(&source, &entry, cancel)?;
    for node in &nodes {
        check_policy(&policy, node)?;
    }
    Ok(DeletePlan { id, source, entry, policy, guards, nodes, root_identity })
}
/// Permanent bounded deletion. Errors/cancellation may leave a partially deleted tree, never rollback.
pub fn delete(plan: &DeletePlan, cancel: &AtomicBool) -> Result<(), String> {
    let held = root(&plan.source)?;
    if !same(identity(&held.metadata().map_err(|e| e.to_string())?), plan.root_identity) {
        return Err("Source root changed after preview".into());
    }
    check_guards(&plan.source, &plan.entry, &plan.guards)?;
    // Re-enumerate before the first unlink: changed scope cannot silently join the confirmation.
    let current = tree(&plan.source, &plan.entry, cancel)?;
    let expected: std::collections::HashMap<_, _> = plan.nodes.iter().map(|e| (&e.id, e.identity)).collect();
    if current.len() != expected.len() || current.iter().any(|e| expected.get(&e.id) != Some(&e.identity)) {
        return Err("Folder contents changed after preview; nothing was deleted".into());
    }
    for node in plan.nodes.iter().rev() {
        cancelled(cancel)?;
        check_policy(&plan.policy, node)?;
        verify(&plan.source, &held)?;
        let parent_path = node.id.path.parent().ok_or("Source root is protected")?;
        let parent = beneath(&held, parent_path.as_path(), flags())?;
        let expected_parent = plan.nodes.iter().find(|e| e.id.path == parent_path);
        if let Some(expected) = expected_parent
            && !same(identity(&parent.metadata().map_err(|e| e.to_string())?), expected.identity)
        {
            return Err("Parent changed during deletion; stopped".into());
        }
        let name = node.id.path.as_path().file_name().ok_or("Missing filename")?;
        let actual = identity(&inspect(&parent, name)?);
        if (node.kind == EntryKind::Directory && !same(actual, node.identity))
            || (node.kind != EntryKind::Directory && actual != node.identity)
        {
            return Err("Target changed; stopped deletion".into());
        }
        current_parent(&held, &parent_path, &parent)?;
        unix::unlinkat(
            &parent,
            name,
            if node.kind == EntryKind::Directory { AtFlags::REMOVEDIR } else { AtFlags::empty() },
        )
        .map_err(|e| format!("Deletion stopped: {e}; previously deleted items cannot be restored"))?;
        parent.sync_all().map_err(|e| format!("Deletion occurred but directory sync failed: {e}"))?;
    }
    Ok(())
}
/// Rename within the same directory; destinations are never overwritten.
pub fn rename(
    source: &Source,
    entry: &Entry,
    name: &std::ffi::OsStr,
    policy: &WritePolicy,
    guards: &[(Source, ProtectedPath)],
) -> Result<(), String> {
    filename(name)?;
    if policy.source != source.id || entry.id.source != source.id {
        return Err("Mismatched source identities".into());
    }
    check_policy(policy, entry)?;
    check_guards(source, entry, guards)?;
    let parent_path = entry.id.path.parent().ok_or("Source roots cannot be renamed")?;
    let path = parent_path.child(name).map_err(|e| e.to_string())?;
    let destination = Entry { id: EntryId { source: source.id, path }, identity: Default::default(), ..entry.clone() };
    check_policy(policy, &destination)?;
    check_guards(source, &destination, guards)?;
    let held = root(source)?;
    observed(&held, entry)?;
    let parent = beneath(&held, parent_path.as_path(), flags())?;
    let old = entry.id.path.as_path().file_name().ok_or("Missing filename")?;
    if identity(&inspect(&parent, old)?) != entry.identity {
        return Err("Item changed; rename refused".into());
    }
    verify(source, &held)?;
    current_parent(&held, &parent_path, &parent)?;
    unix::renameat_with(&parent, old, &parent, name, RenameFlags::NOREPLACE)
        .map_err(|e| format!("Rename refused: {e}"))?;
    parent.sync_all().map_err(|e| format!("Item renamed; durability uncertain: {e}"))?;
    if !same(identity(&inspect(&parent, name)?), entry.identity) {
        return Err("Concurrent replacement detected after rename; inspect destination".into());
    }
    Ok(())
}
/// Copy one physical file/tree into an explicit physical folder, staged then published without overwrite.
/// Failed/cancelled staging is retained at the reported path; source files are never removed.
pub fn copy(
    source: &Source,
    entry: &Entry,
    destination: &Source,
    folder: &RelativePath,
    policy: &WritePolicy,
    guards: &[(Source, ProtectedPath)],
    cancel: &AtomicBool,
) -> Result<(), String> {
    if policy.source != destination.id || entry.id.source != source.id {
        return Err("Mismatched source identities".into());
    }
    filename(&entry.name)?;
    let target = folder.child(&entry.name).map_err(|e| e.to_string())?;
    let synthetic =
        Entry { id: EntryId { source: destination.id, path: target }, identity: Default::default(), ..entry.clone() };
    check_policy(policy, &synthetic)?;
    check_guards(destination, &synthetic, guards)?;
    let nodes = tree(source, entry, cancel)?;
    let read_root = root(source)?;
    let write_root = root(destination)?;
    let parent = beneath(&write_root, folder.as_path(), flags())?;
    // Prevent copying a directory into itself, including through overlapping source definitions.
    let mut ancestor = Some(folder.clone());
    while let Some(path) = ancestor {
        let held = beneath(&write_root, path.as_path(), flags())?;
        if same(identity(&held.metadata().map_err(|e| e.to_string())?), entry.identity) {
            return Err("Cannot paste a folder into itself or its descendants".into());
        }
        ancestor = path.parent();
    }
    match unix::statat(&parent, &entry.name, AtFlags::SYMLINK_NOFOLLOW) {
        Err(e) if e == rustix::io::Errno::NOENT => {}
        Ok(_) => return Err("Destination already exists; nothing was overwritten".into()),
        Err(e) => return Err(e.to_string()),
    }
    let staging = tempfile::Builder::new()
        .prefix(".archiv-copy-")
        .tempdir_in(descriptor_path(&parent))
        .map_err(|e| e.to_string())?;
    let staging_path = staging.keep();
    let staging_name = staging_path.file_name().ok_or("Missing staging name")?;
    let stage = beneath(&parent, Path::new(staging_name), flags())?;
    let result = (|| {
        for node in &nodes {
            cancelled(cancel)?;
            let suffix = node.id.path.as_path().strip_prefix(entry.id.path.as_path()).map_err(|e| e.to_string())?;
            let relative = if suffix.as_os_str().is_empty() {
                PathBuf::from("payload")
            } else {
                Path::new("payload").join(suffix)
            };
            let input = observed(&read_root, node)?;
            if node.kind == EntryKind::Directory {
                let parent = beneath(&stage, relative.parent().unwrap_or(Path::new("")), flags())?;
                let name = relative.file_name().ok_or("Missing staged directory name")?;
                unix::mkdirat(&parent, name, Mode::from_raw_mode(0o700)).map_err(|e| e.to_string())?;
            } else {
                let mut output = File::from(
                    unix::openat2(
                        &stage,
                        &relative,
                        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC,
                        Mode::from_raw_mode(0o600),
                        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_XDEV,
                    )
                    .map_err(|e| e.to_string())?,
                );
                let mut input = input;
                let mut remaining = node.bytes.ok_or("Missing size")?;
                let mut buffer = vec![0u8; 1_048_576];
                while remaining > 0 {
                    cancelled(cancel)?;
                    let length = usize::try_from(remaining.min(buffer.len() as u64)).map_err(|e| e.to_string())?;
                    input.read_exact(&mut buffer[..length]).map_err(|e| e.to_string())?;
                    output.write_all(&buffer[..length]).map_err(|e| e.to_string())?;
                    remaining = remaining.saturating_sub(length as u64);
                }
                if identity(&input.metadata().map_err(|e| e.to_string())?) != node.identity {
                    return Err("Source changed during copy".into());
                }
                output.sync_all().map_err(|e| e.to_string())?;
            }
        }
        for node in nodes.iter().rev().filter(|n| n.kind == EntryKind::Directory) {
            let suffix = node.id.path.as_path().strip_prefix(entry.id.path.as_path()).map_err(|e| e.to_string())?;
            beneath(&stage, &Path::new("payload").join(suffix), flags())?.sync_all().map_err(|e| e.to_string())?;
        }
        cancelled(cancel)?;
        let current = tree(source, entry, cancel)?;
        let expected: std::collections::HashMap<_, _> = nodes.iter().map(|n| (&n.id, n.identity)).collect();
        if current.len() != expected.len() || current.iter().any(|n| expected.get(&n.id) != Some(&n.identity)) {
            return Err("Source tree changed during copy; copy was not published".into());
        }
        verify(source, &read_root)?;
        verify(destination, &write_root)?;
        let now = beneath(&write_root, folder.as_path(), flags())?;
        if !same(
            identity(&now.metadata().map_err(|e| e.to_string())?),
            identity(&parent.metadata().map_err(|e| e.to_string())?),
        ) {
            return Err("Destination folder changed; copy was not published".into());
        }
        unix::renameat_with(&stage, "payload", &parent, &entry.name, RenameFlags::NOREPLACE)
            .map_err(|e| e.to_string())?;
        parent.sync_all().map_err(|e| format!("Copy published but durability uncertain: {e}"))?;
        stage.sync_all().map_err(|e| e.to_string())?;
        unix::unlinkat(&parent, staging_name, AtFlags::REMOVEDIR).map_err(|e| e.to_string())?;
        Ok(())
    })();
    result.map_err(|e: String| {
        format!(
            "{e}. Partial copy may remain at {}; source was not removed",
            destination.root.join(folder.as_path()).join(staging_name).display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use archiv_core::{SourceId, actions::ProtectedPath};
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    fn setup() -> (tempfile::TempDir, Source, WritePolicy) {
        let dir = tempfile::tempdir().unwrap();
        let source = Source { id: SourceId(1), root: dir.path().to_owned(), label: "Fixture".into(), nfs_remote: None };
        let policy = WritePolicy {
            source: source.id,
            enabled: true,
            allowed: vec![RelativePath::default()],
            protected: Vec::new(),
        };
        (dir, source, policy)
    }
    fn entry(source: &Source, name: impl AsRef<Path>) -> Entry {
        let path = name.as_ref();
        let metadata = fs::symlink_metadata(source.root.join(path)).unwrap();
        Entry {
            id: EntryId { source: source.id, path: RelativePath::new(path).unwrap() },
            name: path.file_name().unwrap().to_owned(),
            kind: if metadata.is_dir() {
                EntryKind::Directory
            } else if metadata.is_file() {
                EntryKind::File
            } else {
                EntryKind::Symlink
            },
            identity: identity(&metadata),
            bytes: metadata.is_file().then_some(metadata.len()),
            modified: None,
            mode: metadata.mode(),
        }
    }
    fn cancel() -> AtomicBool {
        AtomicBool::new(false)
    }
    #[test]
    fn permanent_delete_removes_only_confirmed_tree_and_creates_no_storage() {
        let (dir, source, policy) = setup();
        fs::create_dir_all(dir.path().join("folder/sub")).unwrap();
        fs::write(dir.path().join("folder/sub/notes"), b"remove").unwrap();
        fs::write(dir.path().join("keep"), b"safe").unwrap();
        let plan = prepare_delete(1, source.clone(), entry(&source, "folder"), policy, Vec::new(), &cancel()).unwrap();
        assert_eq!(plan.entries().len(), 3);
        delete(&plan, &cancel()).unwrap();
        assert!(!dir.path().join("folder").exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(fs::read(dir.path().join("keep")).unwrap(), b"safe");
        assert!(delete(&plan, &cancel()).is_err());
    }
    #[test]
    fn delete_refuses_changed_scope_stale_target_links_mounts_and_protections() {
        let (dir, source, mut policy) = setup();
        fs::create_dir(dir.path().join("folder")).unwrap();
        fs::write(dir.path().join("folder/file"), b"safe").unwrap();
        let plan =
            prepare_delete(1, source.clone(), entry(&source, "folder"), policy.clone(), Vec::new(), &cancel()).unwrap();
        fs::write(dir.path().join("folder/new"), b"new").unwrap();
        assert!(delete(&plan, &cancel()).is_err());
        assert!(dir.path().join("folder/file").exists());
        policy.protected.push(ProtectedPath { path: RelativePath::new("folder/file").unwrap(), subtree: true });
        assert!(
            prepare_delete(
                2,
                source.clone(),
                entry(&source, "folder"),
                policy.clone(),
                vec![(source.clone(), policy.protected[0].clone())],
                &cancel()
            )
            .is_err()
        );
        symlink("/", dir.path().join("folder/link")).unwrap();
        policy.protected.clear();
        assert!(
            prepare_delete(3, source.clone(), entry(&source, "folder"), policy.clone(), Vec::new(), &cancel()).is_err()
        );
        let nfs = Source { nfs_remote: Some("invalid:/missing".into()), ..source.clone() };
        assert!(prepare_delete(4, nfs, entry(&source, "folder/file"), policy, Vec::new(), &cancel()).is_err());
    }
    #[test]
    fn renamed_parent_symlink_and_cancellation_do_not_delete_outside_data() {
        let (dir, source, policy) = setup();
        fs::create_dir(dir.path().join("parent")).unwrap();
        fs::write(dir.path().join("parent/file"), b"safe").unwrap();
        let plan =
            prepare_delete(1, source.clone(), entry(&source, "parent/file"), policy, Vec::new(), &cancel()).unwrap();
        assert!(delete(&plan, &AtomicBool::new(true)).is_err());
        fs::rename(dir.path().join("parent"), dir.path().join("moved")).unwrap();
        symlink("moved", dir.path().join("parent")).unwrap();
        assert!(delete(&plan, &cancel()).is_err());
        assert_eq!(fs::read(dir.path().join("moved/file")).unwrap(), b"safe");
    }
    #[test]
    fn rename_preserves_bytes_and_refuses_overwrites_traversal_and_changed_items() {
        let (dir, source, policy) = setup();
        let name = std::ffi::OsString::from_vec(vec![b'a', 0xff]);
        fs::write(dir.path().join(&name), b"safe").unwrap();
        let item = entry(&source, &name);
        rename(&source, &item, std::ffi::OsStr::new("renamed"), &policy, &[]).unwrap();
        assert_eq!(fs::read(dir.path().join("renamed")).unwrap(), b"safe");
        fs::write(dir.path().join("existing"), b"existing").unwrap();
        let item = entry(&source, "renamed");
        for name in ["existing", "../escape", "a/b", "", ".", ".."] {
            assert!(rename(&source, &item, std::ffi::OsStr::new(name), &policy, &[]).is_err());
        }
        fs::write(dir.path().join("renamed"), b"changed").unwrap();
        assert!(rename(&source, &item, std::ffi::OsStr::new("new"), &policy, &[]).is_err());
        assert_eq!(fs::read(dir.path().join("existing")).unwrap(), b"existing");
    }
    #[test]
    fn copies_files_and_nested_folders_without_overwrite_or_source_removal() {
        let (dir, source, policy) = setup();
        fs::create_dir_all(dir.path().join("tree/sub")).unwrap();
        fs::write(dir.path().join("tree/sub/file"), b"safe").unwrap();
        fs::create_dir(dir.path().join("destination")).unwrap();
        let folder = RelativePath::new("destination").unwrap();
        copy(&source, &entry(&source, "tree"), &source, &folder, &policy, &[], &cancel()).unwrap();
        assert_eq!(fs::read(dir.path().join("destination/tree/sub/file")).unwrap(), b"safe");
        assert!(dir.path().join("tree/sub/file").exists());
        assert!(copy(&source, &entry(&source, "tree"), &source, &folder, &policy, &[], &cancel()).is_err());
        copy(&source, &entry(&source, "tree/sub/file"), &source, &folder, &policy, &[], &cancel()).unwrap();
        assert_eq!(fs::read(dir.path().join("destination/file")).unwrap(), b"safe");
        assert!(
            fs::read_dir(dir.path().join("destination")).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".archiv-copy-"))
        );
    }
    #[test]
    fn copy_refuses_links_self_descendants_cancelled_stale_and_denied_destinations() {
        let (dir, source, mut policy) = setup();
        fs::create_dir_all(dir.path().join("tree/sub")).unwrap();
        let top = entry(&source, "tree");
        assert!(
            copy(&source, &top, &source, &RelativePath::new("tree/sub").unwrap(), &policy, &[], &cancel()).is_err()
        );
        fs::create_dir(dir.path().join("out")).unwrap();
        let out = RelativePath::new("out").unwrap();
        assert!(copy(&source, &top, &source, &out, &policy, &[], &AtomicBool::new(true)).is_err());
        symlink("/", dir.path().join("tree/link")).unwrap();
        assert!(copy(&source, &entry(&source, "tree"), &source, &out, &policy, &[], &cancel()).is_err());
        fs::write(dir.path().join("file"), b"safe").unwrap();
        let file = entry(&source, "file");
        policy.enabled = false;
        assert!(copy(&source, &file, &source, &out, &policy, &[], &cancel()).is_err());
        policy.enabled = true;
        fs::write(dir.path().join("file"), b"different").unwrap();
        assert!(copy(&source, &file, &source, &out, &policy, &[], &cancel()).is_err());
        assert_eq!(fs::read_dir(dir.path().join("out")).unwrap().count(), 0);
    }
}
