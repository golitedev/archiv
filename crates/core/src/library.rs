//! Optional virtual collections and versioned, credential-free library configuration.
use crate::{Entry, EntryId, EntryKind, Location, Source};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A user-created virtual folder; members remain independent physical locations.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collection {
    /// Stable configured identity.
    pub id: u64,
    /// User-visible name.
    pub label: String,
    /// Explicit contributing roots; no implicit source expansion.
    pub members: Vec<Location>,
}

/// Saved source definitions, loaded without scanning or reconnecting.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibrarySettings {
    /// Supported schema version (currently 1).
    pub version: u32,
    /// Configurable external applications.
    #[serde(default)]
    pub openers: crate::actions::Openers,
    /// Deny-by-default file-change permissions and protections.
    #[serde(default)]
    pub writes: Vec<crate::actions::WritePolicy>,
    /// Physical roots, at most 32.
    pub sources: Vec<Source>,
    /// Optional merged folders, at most 16.
    pub collections: Vec<Collection>,
}
impl Default for LibrarySettings {
    fn default() -> Self {
        Self {
            version: 1,
            openers: Default::default(),
            writes: Vec::new(),
            sources: Vec::new(),
            collections: Vec::new(),
        }
    }
}
impl LibrarySettings {
    /// Reject malformed, ambiguous or unbounded definitions before use or persistence.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.sources.len() > 32 || self.collections.len() > 16 {
            return Err("Unsupported library version or too many sources/collections".into());
        }
        self.openers.validate()?;
        let mut ids = HashSet::new();
        let mut roots = HashSet::new();
        for source in &self.sources {
            if source.id.0 == 0
                || source.id.0 == u64::MAX
                || !ids.insert(source.id)
                || !roots.insert(&source.root)
                || !source.root.is_absolute()
                || source.root.as_os_str().len() > 16384
                || source.root.components().any(|c| matches!(c, std::path::Component::ParentDir))
                || source.label.trim().is_empty()
                || source.label.len() > 128
                || source.label.chars().any(char::is_control)
            {
                return Err("Sources need unique IDs/absolute roots and a name (up to 128 bytes)".into());
            }
            if let Some(remote) = &source.nfs_remote
                && (remote.len() > 1024 || !remote.contains(":/") || remote.chars().any(char::is_whitespace))
            {
                return Err("NFS source needs an exact server:/export identity".into());
            }
        }
        let mut policies = HashSet::new();
        if self.writes.len() > 32 {
            return Err("Too many file-change policies".into());
        }
        for policy in &self.writes {
            if !ids.contains(&policy.source)
                || !policies.insert(policy.source)
                || policy.allowed.len() > 128
                || policy.protected.len() > 128
            {
                return Err("Invalid, duplicate or oversized file-change policy".into());
            }
        }
        let mut collection_ids = HashSet::new();
        for collection in &self.collections {
            let mut members = HashSet::new();
            if collection.id == 0
                || !collection_ids.insert(collection.id)
                || collection.label.trim().is_empty()
                || collection.label.len() > 128
                || collection.label.chars().any(char::is_control)
                || collection.members.is_empty()
                || collection.members.len() > 32
            {
                return Err("Collections need unique IDs, a name, and 1–32 explicit folders".into());
            }
            for member in &collection.members {
                if !ids.contains(&member.source)
                    || !members.insert(member)
                    || member.path.as_path().as_os_str().len() > 16384
                {
                    return Err("Collection references an unknown source or duplicate/oversized folder".into());
                }
            }
        }
        Ok(())
    }
}

/// Collapse exact-byte directory names only; keep colliding files and every contributor.
/// The returned keys are representative physical IDs, never operation targets for a virtual node.
pub fn merge_directories(entries: &mut Vec<Entry>) -> HashMap<EntryId, Vec<EntryId>> {
    crate::sort_entries(entries);
    let mut folders = HashMap::new();
    let mut contributors: HashMap<EntryId, Vec<EntryId>> = HashMap::new();
    entries.retain(|entry| {
        if entry.kind != EntryKind::Directory {
            return true;
        }
        let representative = folders.entry(entry.name.clone()).or_insert_with(|| entry.id.clone());
        contributors.entry(representative.clone()).or_default().push(entry.id.clone());
        representative == &entry.id
    });
    contributors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntryKind, RelativePath, SourceId};
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    fn entry(source: u64, name: OsString, kind: EntryKind) -> Entry {
        Entry {
            id: EntryId { source: SourceId(source), path: RelativePath::default().child(&name).unwrap() },
            name,
            kind,
            bytes: None,
            modified: None,
            mode: 0,
            identity: Default::default(),
        }
    }
    #[test]
    fn exact_bytes_only_and_files_never_collapse() {
        let mut entries = vec![
            entry(1, "Title".into(), EntryKind::Directory),
            entry(2, "Title".into(), EntryKind::Directory),
            entry(3, "title".into(), EntryKind::Directory),
            entry(1, OsString::from_vec(vec![b'a', 0xfe]), EntryKind::Directory),
            entry(2, OsString::from_vec(vec![b'a', 0xff]), EntryKind::Directory),
            entry(1, "movie.mkv".into(), EntryKind::File),
            entry(2, "movie.mkv".into(), EntryKind::File),
            entry(3, "Title".into(), EntryKind::Symlink),
        ];
        let contributors = merge_directories(&mut entries);
        assert_eq!(entries.len(), 7);
        assert_eq!(contributors.values().map(Vec::len).max(), Some(2));
        assert_eq!(entries.iter().filter(|e| e.kind == EntryKind::File).count(), 2);
    }
    #[test]
    fn invalid_references_duplicate_roots_and_versions_are_rejected() {
        let mut config = LibrarySettings::default();
        config.sources.push(Source {
            id: SourceId(1),
            label: "Disk".into(),
            root: "/synthetic/disk".into(),
            nfs_remote: None,
        });
        config.collections.push(Collection {
            id: 1,
            label: "Movies".into(),
            members: vec![Location { source: SourceId(2), path: RelativePath::default() }],
        });
        assert!(config.validate().is_err());
        config.collections[0].members[0].source = SourceId(1);
        assert!(config.validate().is_ok());
        config.sources.push(Source { id: SourceId(2), ..config.sources[0].clone() });
        assert!(config.validate().is_err());
        config.sources.pop();
        config.version = 99;
        assert!(config.validate().is_err());
    }
}
