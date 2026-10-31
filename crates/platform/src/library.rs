//! Bounded settings IO with atomic replacement and explicit durability.
use archiv_core::library::LibrarySettings;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
const MAX_SETTINGS: u64 = 262_144;

/// Read a versioned file; absence is an empty library, malformed data is an error.
pub fn load(path: &Path) -> Result<LibrarySettings, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(LibrarySettings::default()),
        Err(error) => return Err(format!("Cannot read library settings: {error}")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_SETTINGS + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_SETTINGS {
        return Err("Library settings exceed 256 KiB".into());
    }
    let settings: LibrarySettings =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid library settings: {e}"))?;
    settings.validate()?;
    Ok(settings)
}

/// Write a private same-directory temporary file, sync, rename, then sync its directory.
/// A directory-sync error can mean the new file is visible but durability is uncertain.
pub fn save(path: &Path, settings: &LibrarySettings) -> Result<(), String> {
    settings.validate()?;
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_SETTINGS {
        return Err("Library settings exceed 256 KiB".into());
    }
    let parent = path.parent().ok_or("Settings path has no parent")?;
    let write = || -> io::Result<()> {
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|e| e.error)?;
        File::open(parent)?.sync_all()
    };
    write().map_err(|e| format!("Could not durably save library settings: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn round_trip_is_private_and_invalid_save_preserves_previous_file() {
        let fixture = tempfile::tempdir().unwrap();
        let path = fixture.path().join("settings/library.json");
        let settings = LibrarySettings::default();
        assert_eq!(load(&path).unwrap(), settings);
        save(&path, &settings).unwrap();
        assert_eq!(load(&path).unwrap(), settings);
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let before = fs::read(&path).unwrap();
        let mut invalid = settings;
        invalid.version = 99;
        assert!(save(&path, &invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::write(&path, br#"{"version":1,"sources":[],"collections":[],"unknown":true}"#).unwrap();
        assert!(load(&path).is_err());
        fs::write(&path, vec![b' '; 262_145]).unwrap();
        assert!(load(&path).is_err());
    }
}
