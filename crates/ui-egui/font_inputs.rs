//! Pinned, openly licensed build inputs. Shared by the explicit fetch tool and UI build script.
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
/// Inter upstream release; the static faces match the PdfCraft reference hashes.
pub const INTER_URL: &str = "https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip";
/// Archive integrity pin.
pub const INTER_ARCHIVE_SHA: &str = "9883fdd4a49d4fb66bd8177ba6625ef9a64aa45899767dde3d36aa425756b11e";
/// JetBrains Mono upstream release input.
pub const MONO_URL: &str =
    "https://raw.githubusercontent.com/JetBrains/JetBrainsMono/v2.304/fonts/ttf/JetBrainsMono-Regular.ttf";
/// Face names, generated identifiers and exact SHA-256 values.
pub const FONTS: &[(&str, &str, &str)] = &[
    ("Inter-Regular.ttf", "INTER_REGULAR", "40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82"),
    ("Inter-Medium.ttf", "INTER_MEDIUM", "97ad806f526e41546d46365bb3a393145f75b7b1568913db74549ad8b8dba872"),
    ("Inter-SemiBold.ttf", "INTER_SEMIBOLD", "78a843fade9d4612a5567302fb595b56976eb5fcebf4fea5a5912d638bafcde3"),
    ("JetBrainsMono-Regular.ttf", "MONO", "a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f"),
];
/// Cache chosen explicitly or under the repository's ignored artifacts directory.
pub fn directory(workspace: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let directory = std::env::var_os("ARCHIV_FONT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("artifacts/font-inputs"));
    if !directory.is_absolute() {
        return Err("ARCHIV_FONT_DIR must be an absolute directory".into());
    }
    Ok(directory)
}

/// Verify bounded input before embedding or installing; never trust a filename alone.
pub fn verified(path: &Path, expected: &str, limit: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut data = Vec::new();
    std::fs::File::open(path)?.take(limit.saturating_add(1)).read_to_end(&mut data)?;
    if data.len() as u64 > limit {
        return Err(format!("Font input exceeds limit: {}", path.display()).into());
    }
    if format!("{:x}", Sha256::digest(&data)) != expected {
        return Err(format!("Font input integrity mismatch: {}", path.display()).into());
    }
    Ok(data)
}
