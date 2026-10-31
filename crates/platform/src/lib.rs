//! Linux directory enumeration, settings and explicit recovery/process adapters.

pub mod library;
mod mount;
pub mod operations;

use archiv_core::{Entry, EntryId, EntryKind, RelativePath, Source};
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Number of entries emitted at once, bounding channel payloads.
pub const BATCH_SIZE: usize = 128;
/// Maximum entries retained from one directory. No recursive enumeration is performed.
pub const MAX_ENTRIES: usize = 50_000;

/// A read-only enumeration request for an explicit source.
#[derive(Clone, Debug)]
pub struct ReadRequest {
    /// Physical source root.
    pub source: Source,
    /// Location under that root.
    pub path: RelativePath,
}

/// Completion facts, including partial enumeration limitations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReadSummary {
    /// Entries observed before cancellation or the listing limit.
    pub entries: usize,
    /// Entries that disappeared or could not be read.
    pub skipped: usize,
    /// Enumeration was stopped at the documented limit.
    pub truncated: bool,
    /// Work was cancelled; partial results are not a complete directory.
    pub cancelled: bool,
}

/// A filesystem or directory-open failure with its physical context.
#[derive(Debug)]
pub struct ReadError {
    path: PathBuf,
    error: io::Error,
}
impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cannot browse {}: {}", archiv_core::display_name(self.path.as_os_str()), self.error)
    }
}
impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Worker-facing enumeration boundary; implementations must honor cancellation between entries.
pub trait DirectoryReader: Send + Sync + 'static {
    /// Emit bounded batches. A false callback return cancels delivery.
    fn read(
        &self,
        request: &ReadRequest,
        cancelled: &AtomicBool,
        emit: &mut dyn FnMut(Vec<Entry>) -> bool,
    ) -> Result<ReadSummary, ReadError>;
}

/// Linux local/mounted-directory adapter, using descriptor-relative no-symlink resolution.
#[derive(Default)]
pub struct LocalDirectoryReader;

impl DirectoryReader for LocalDirectoryReader {
    fn read(
        &self,
        request: &ReadRequest,
        cancelled: &AtomicBool,
        emit: &mut dyn FnMut(Vec<Entry>) -> bool,
    ) -> Result<ReadSummary, ReadError> {
        let mut summary = ReadSummary::default();
        if cancelled.load(Ordering::Acquire) {
            summary.cancelled = true;
            return Ok(summary);
        }
        if request.path.as_path().components().any(|c| c.as_os_str() == archiv_core::actions::RECOVERY_DIR) {
            return Err(ReadError {
                path: request.source.root.clone(),
                error: io::Error::other("Recovery storage is not a browsing location"),
            });
        }
        let physical = request.source.root.join(request.path.as_path());
        let failure = |error: io::Error| ReadError { path: physical.clone(), error };
        let flags = rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOFOLLOW;
        let root = rustix::fs::open(&request.source.root, flags, rustix::fs::Mode::empty())
            .map_err(|error| failure(error.into()))?;
        let root = File::from(root);
        if let Some(remote) = &request.source.nfs_remote {
            mount::verify(&request.source.root, remote, &root).map_err(failure)?;
        }
        let relative = if request.path.as_path().as_os_str().is_empty() {
            std::path::Path::new(".")
        } else {
            request.path.as_path()
        };
        // BENEATH and NO_SYMLINKS resolve beneath the held root descriptor, not a race-prone
        // canonicalize/prefix check. Older unsupported kernels report an error; no lax fallback.
        let directory = rustix::fs::openat2(
            &root,
            relative,
            flags,
            rustix::fs::Mode::empty(),
            rustix::fs::ResolveFlags::BENEATH
                | rustix::fs::ResolveFlags::NO_SYMLINKS
                | if request.source.nfs_remote.is_some() {
                    rustix::fs::ResolveFlags::NO_XDEV
                } else {
                    rustix::fs::ResolveFlags::empty()
                },
        )
        .map_err(|error| failure(error.into()))?;
        let directory = File::from(directory);
        // Keep the resolved directory alive. Linux procfs opens this exact directory; ReadDir
        // holds its own descriptor and DirEntry::metadata does not follow symlinks on Linux.
        let fd_path = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        let listing = fs::read_dir(fd_path).map_err(failure)?;
        let mut batch = Vec::with_capacity(BATCH_SIZE);
        for item in listing {
            if cancelled.load(Ordering::Acquire) {
                summary.cancelled = true;
                break;
            }
            if summary.entries >= MAX_ENTRIES {
                summary.truncated = true;
                break;
            }
            let Ok(item) = item else {
                summary.skipped = summary.skipped.saturating_add(1);
                continue;
            };
            let Ok(metadata) = item.metadata() else {
                summary.skipped = summary.skipped.saturating_add(1);
                continue;
            };
            let name = item.file_name();
            if name == archiv_core::actions::RECOVERY_DIR {
                continue;
            }
            let Ok(path) = request.path.child(&name) else {
                summary.skipped = summary.skipped.saturating_add(1);
                continue;
            };
            let file_type = metadata.file_type();
            let kind = if file_type.is_symlink() {
                EntryKind::Symlink
            } else if file_type.is_dir() {
                EntryKind::Directory
            } else if file_type.is_file() {
                EntryKind::File
            } else {
                EntryKind::Other
            };
            batch.push(Entry {
                id: EntryId { source: request.source.id, path },
                name,
                kind,
                bytes: (kind == EntryKind::File).then_some(metadata.len()),
                modified: metadata.modified().ok(),
                mode: metadata.mode(),
                identity: operations::identity(&metadata),
            });
            summary.entries = summary.entries.saturating_add(1);
            if batch.len() == BATCH_SIZE && !emit(std::mem::replace(&mut batch, Vec::with_capacity(BATCH_SIZE))) {
                summary.cancelled = true;
                break;
            }
        }
        if !summary.cancelled && !batch.is_empty() && !emit(batch) {
            summary.cancelled = true;
        }
        if let Some(remote) = &request.source.nfs_remote {
            mount::verify(&request.source.root, remote, &root).map_err(failure)?;
        }
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archiv_core::SourceId;
    use std::os::unix::{ffi::OsStringExt, fs::symlink};

    fn request(root: &std::path::Path) -> ReadRequest {
        ReadRequest {
            source: Source { id: SourceId(1), root: root.to_owned(), label: "Fixture".into(), nfs_remote: None },
            path: RelativePath::default(),
        }
    }

    #[test]
    fn nfs_source_never_lists_a_local_mountpoint_fallback() {
        let fixture = tempfile::tempdir().unwrap();
        fs::write(fixture.path().join("local-fallback"), b"not NAS").unwrap();
        let mut req = request(fixture.path());
        req.source.nfs_remote = Some("fixture:/export".into());
        let mut emitted = false;
        let result = LocalDirectoryReader.read(&req, &AtomicBool::new(false), &mut |_| {
            emitted = true;
            true
        });
        assert!(result.is_err());
        assert!(!emitted);
    }

    #[test]
    fn lists_in_batches_without_following_links_or_changing_data() {
        let fixture = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("private"), "outside").unwrap();
        fs::create_dir(fixture.path().join("show")).unwrap();
        symlink(outside.path(), fixture.path().join("external")).unwrap();
        let odd = std::ffi::OsString::from_vec(vec![b'm', 0xff]);
        fs::write(fixture.path().join(&odd), "untouched").unwrap();
        for i in 0..300 {
            fs::write(fixture.path().join(format!("file-{i}")), "data").unwrap();
        }
        let mut entries = Vec::new();
        let summary = LocalDirectoryReader
            .read(&request(fixture.path()), &AtomicBool::new(false), &mut |batch| {
                assert!(batch.len() <= BATCH_SIZE);
                entries.extend(batch);
                true
            })
            .unwrap();
        assert_eq!(summary.entries, 303);
        assert!(entries.iter().any(|e| e.name == odd));
        assert!(entries.iter().any(|e| e.name == "external" && e.kind == EntryKind::Symlink));
        assert_eq!(fs::read(fixture.path().join(odd)).unwrap(), b"untouched");
        let mut link = request(fixture.path());
        link.path = RelativePath::new("external").unwrap();
        assert!(LocalDirectoryReader.read(&link, &AtomicBool::new(false), &mut |_| true).is_err());
        assert_eq!(fs::read(outside.path().join("private")).unwrap(), b"outside");
    }

    #[test]
    fn cancellation_does_not_open_a_missing_source() {
        let req = request(std::path::Path::new("/does-not-exist/archiv-fixture"));
        let result = LocalDirectoryReader
            .read(&req, &AtomicBool::new(true), &mut |_| panic!("cancelled scan emitted entries"))
            .unwrap();
        assert!(result.cancelled);
        assert_eq!(result.entries, 0);
    }
}
