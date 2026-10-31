//! Deterministic application selection and deny-by-default recovery policy.
use crate::{EntryKind, RelativePath, SourceId};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Reserved same-filesystem recovery directory; never part of browsing or selection.
pub const RECOVERY_DIR: &str = ".archiv-recovery";
/// User-chosen programs, each an executable name/path without shell arguments.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Openers {
    /// Image viewer, e.g. imv.
    pub images: String,
    /// Video/audio player, e.g. mpv.
    pub media: String,
    /// Text editor, e.g. kwrite.
    pub text: String,
    /// Optional handler for other regular files.
    pub other: String,
}
impl Openers {
    /// Select a configured handler; never execute the selected file itself.
    pub fn program(&self, path: &Path) -> Option<&str> {
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let program = match extension.as_str() {
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tiff" | "avif" | "svg" => &self.images,
            "mkv" | "mp4" | "webm" | "avi" | "mov" | "m4v" | "mp3" | "flac" | "ogg" | "wav" | "m4a" => &self.media,
            "txt" | "md" | "rs" | "toml" | "json" | "yaml" | "yml" | "log" | "srt" | "ass" | "csv" | "xml" | "conf" => {
                &self.text
            }
            _ => &self.other,
        };
        (!program.is_empty()).then_some(program.as_str())
    }
    /// Validate executable values without interpreting a shell command line.
    pub fn validate(&self) -> Result<(), String> {
        for value in [&self.images, &self.media, &self.text, &self.other] {
            if value.len() > 4096 || value.chars().any(char::is_control) || value.starts_with('-') {
                return Err("Application executable is invalid or too long".into());
            }
        }
        Ok(())
    }
}
/// A folder protected against recovery/removal; optionally protects its entire subtree.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedPath {
    /// Source-relative path.
    pub path: RelativePath,
    /// Also deny operations on descendants. The folder and its ancestors are always protected.
    pub subtree: bool,
}
/// Explicit opt-in per source. This policy controls Archiv only, not other programs or NAS users.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPolicy {
    /// Configured physical source.
    pub source: SourceId,
    /// Explicitly allow recovery moves for this source.
    pub enabled: bool,
    /// Allowed subtrees, still intersected with protections.
    pub allowed: Vec<RelativePath>,
    /// Structural or subtree protections.
    pub protected: Vec<ProtectedPath>,
}
impl RecoveryPolicy {
    /// Deny source roots, recovery storage, non-files/directories, and ambiguous/protected targets.
    pub fn permits(&self, path: &RelativePath, kind: EntryKind) -> Result<(), String> {
        let path = path.as_path();
        if !self.enabled {
            return Err("Recovery is disabled for this source in Safeguards".into());
        }
        if path.as_os_str().is_empty() || path.components().any(|c| c.as_os_str() == RECOVERY_DIR) {
            return Err("Source roots and recovery storage are protected".into());
        }
        if !matches!(kind, EntryKind::File | EntryKind::Directory) {
            return Err("Links and special files cannot be moved to recovery".into());
        }
        if !self.allowed.iter().any(|root| path.starts_with(root.as_path())) {
            return Err("This path is outside the configured allowed folders".into());
        }
        for protected in &self.protected {
            if protected.path.as_path().starts_with(path)
                || (protected.subtree && path.starts_with(protected.path.as_path()))
            {
                return Err("A protected folder or subtree would be affected".into());
            }
        }
        Ok(())
    }
}
/// Observed Linux identity, used to reject stale previews without treating labels as paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileIdentity {
    /// Device identity.
    pub device: u64,
    /// Inode identity.
    pub inode: u64,
    /// Observed change time in seconds.
    pub changed: i64,
    /// Observed change-time nanoseconds.
    pub changed_ns: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(text: &str) -> RelativePath {
        RelativePath::new(text).unwrap()
    }
    #[test]
    fn protected_structure_denies_ancestors_but_can_allow_children() {
        let mut policy = RecoveryPolicy {
            source: SourceId(1),
            enabled: true,
            allowed: vec![path("")],
            protected: vec![ProtectedPath { path: path("data/media"), subtree: false }],
        };
        for target in ["", "data", "data/media", ".archiv-recovery/item/payload"] {
            assert!(policy.permits(&path(target), EntryKind::Directory).is_err(), "{target}");
        }
        assert!(policy.permits(&path("data/media/Title"), EntryKind::Directory).is_ok());
        assert!(policy.permits(&path("data/media2/file"), EntryKind::File).is_ok());
        policy.protected[0].subtree = true;
        assert!(policy.permits(&path("data/media/Title"), EntryKind::Directory).is_err());
        assert!(policy.permits(&path("link"), EntryKind::Symlink).is_err());
        policy.enabled = false;
        assert!(policy.permits(&path("file"), EntryKind::File).is_err());
    }
    #[test]
    fn application_selection_never_executes_the_selected_path() {
        let openers =
            Openers { images: "imv".into(), media: "mpv".into(), text: "kwrite".into(), other: String::new() };
        assert_eq!(openers.program(Path::new("Title.MKV")), Some("mpv"));
        assert_eq!(openers.program(Path::new("$(touch file).txt")), Some("kwrite"));
        assert_eq!(openers.program(Path::new("program.sh")), None);
    }
}
