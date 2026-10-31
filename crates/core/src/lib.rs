//! Byte-preserving identities and read-only browsing models. No IO or GUI dependencies.

pub mod library;

use serde::{Deserialize, Serialize};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

/// Identity of an explicitly registered source in this session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceId(pub u64);

/// A lexical, source-relative path containing only ordinary name components.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "PathBuf", into = "PathBuf")]
pub struct RelativePath(PathBuf);

impl TryFrom<PathBuf> for RelativePath {
    type Error = InvalidRelativePath;
    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        Self::new(path)
    }
}
impl From<RelativePath> for PathBuf {
    fn from(path: RelativePath) -> Self {
        path.0
    }
}

/// A path is not a safe source-relative browsing location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidRelativePath;

impl fmt::Display for InvalidRelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Use a relative path without parent traversal or absolute components")
    }
}
impl std::error::Error for InvalidRelativePath {}

impl RelativePath {
    /// Validate without resolving or normalizing the filename bytes.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, InvalidRelativePath> {
        let path = path.into();
        if path.components().any(|part| !matches!(part, Component::Normal(_))) {
            return Err(InvalidRelativePath);
        }
        Ok(Self(path))
    }

    /// Exact relative path; empty means source root.
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Root or parent location, without leaving the source.
    pub fn parent(&self) -> Option<Self> {
        self.0.parent().map(|parent| Self(parent.to_owned()))
    }

    /// Append exactly one name, rejecting separators and navigation components.
    pub fn child(&self, name: &OsStr) -> Result<Self, InvalidRelativePath> {
        let mut components = Path::new(name).components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err(InvalidRelativePath);
        }
        Self::new(self.0.join(name))
    }
}

/// An explicit physical root; this is not a virtual collection or security permission.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Stable source identity within the session.
    pub id: SourceId,
    /// Exact absolute root path, supplied by the user.
    pub root: PathBuf,
    /// Display label, never used for filesystem operations.
    pub label: String,
    /// Expected NFS server:/export; root must be its exact mount point.
    #[serde(default)]
    pub nfs_remote: Option<String>,
}

/// A directory within one registered physical source.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Location {
    /// Owning source.
    pub source: SourceId,
    /// Relative directory path.
    pub path: RelativePath,
}

/// Stable entry selection key, independent of display labels and sorting.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntryId {
    /// Owning source, even when names collide across sources.
    pub source: SourceId,
    /// Original path bytes, relative to source root.
    pub path: RelativePath,
}

/// The kind observed without following a symbolic link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    /// Real directory, eligible for read-only navigation.
    Directory,
    /// Regular file; no application launch is performed yet.
    File,
    /// Symbolic link, displayed but never traversed.
    Symlink,
    /// Device, socket or other special entry, displayed but not opened.
    Other,
}
impl EntryKind {
    /// Human-readable file kind.
    pub fn label(self) -> &'static str {
        match self {
            Self::Directory => "Folder",
            Self::File => "File",
            Self::Symlink => "Link",
            Self::Other => "Other",
        }
    }
}

/// Metadata snapshot, never authorization to mutate or a permanent filesystem identity.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Byte-preserving physical identity.
    pub id: EntryId,
    /// Original filename bytes.
    pub name: OsString,
    /// No-follow file kind.
    pub kind: EntryKind,
    /// Size of a regular file; directories are not recursively measured.
    pub bytes: Option<u64>,
    /// Observed last modification time, if supported.
    pub modified: Option<SystemTime>,
    /// Observed Unix mode including file type bits.
    pub mode: u32,
}

/// Escape control characters for a single-line label; never use the result as an operation path.
pub fn display_name(name: &OsStr) -> String {
    let mut label = String::new();
    for ch in name.to_string_lossy().chars().take(512) {
        if ch.is_control() {
            label.extend(ch.escape_default());
        } else {
            label.push(ch);
        }
    }
    label
}

/// Sort directories first, then exact filename bytes. Duplicate names retain their source IDs.
pub fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        let a_folder = a.kind == EntryKind::Directory;
        let b_folder = b.kind == EntryKind::Directory;
        b_folder.cmp(&a_folder).then_with(|| a.name.cmp(&b.name)).then_with(|| a.id.source.cmp(&b.id.source))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn rejects_navigation_and_absolute_paths() {
        for path in ["../movie", "/etc", "movie/../../etc", ".", "./movie"] {
            assert!(RelativePath::new(path).is_err(), "accepted {path}");
        }
        assert!(RelativePath::new("").is_ok());
        let path = RelativePath::new("tv/show").unwrap();
        assert_eq!(path.parent().unwrap().as_path(), Path::new("tv"));
        assert!(path.child(OsStr::new("../outside")).is_err());
    }

    #[test]
    fn non_utf8_names_do_not_collapse_to_their_display_label() {
        let a = OsString::from_vec(vec![b'a', 0xff]);
        let b = OsString::from_vec(vec![b'a', 0xfe]);
        assert_eq!(display_name(&a), display_name(&b));
        assert_ne!(RelativePath::default().child(&a).unwrap(), RelativePath::default().child(&b).unwrap());
        assert_eq!(display_name(OsStr::new("a\nb")), "a\\nb");
    }
}
