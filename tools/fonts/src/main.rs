//! Explicit font acquisition for builds; the Archiv application never downloads resources.
#[path = "../../../crates/ui-egui/font_inputs.rs"]
mod font_inputs;
use std::path::Path;
use std::process::Command;
fn download(url: &str, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "-fsSL",
            "--connect-timeout",
            "10",
            "--max-time",
            "90",
            "--max-filesize",
            "40000000",
            "--max-redirs",
            "5",
            "--output",
        ])
        .arg(path)
        .arg(url)
        .status()?;
    if !status.success() {
        return Err(format!("Font download failed: {status}").into());
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    let check = match first.as_deref() {
        None => false,
        Some(arg) if arg == "--check" => true,
        _ => return Err("Usage: archiv-fonts [--check]".into()),
    };
    if args.next().is_some() {
        return Err("Usage: archiv-fonts [--check]".into());
    }
    let workspace =
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).ok_or("Missing workspace path")?;
    let directory = font_inputs::directory(workspace)?;
    if !check {
        std::fs::create_dir_all(&directory)?;
    }
    let missing = font_inputs::FONTS
        .iter()
        .any(|(name, _, hash)| font_inputs::verified(&directory.join(name), hash, 1_048_576).is_err());
    if check || !missing {
        for (name, _, hash) in font_inputs::FONTS {
            font_inputs::verified(&directory.join(name), hash, 1_048_576)?;
        }
    } else {
        let temporary = tempfile::tempdir_in(&directory)?;
        let archive = temporary.path().join("Inter-4.1.zip");
        download(font_inputs::INTER_URL, &archive)?;
        font_inputs::verified(&archive, font_inputs::INTER_ARCHIVE_SHA, 40_000_000)?;
        for (name, _, hash) in font_inputs::FONTS {
            let path = temporary.path().join(name);
            if name.starts_with("Inter-") {
                let file = std::fs::File::create(&path)?;
                let status = Command::new("unzip")
                    .arg("-p")
                    .arg(&archive)
                    .arg(format!("extras/ttf/{name}"))
                    .stdout(file)
                    .status()?;
                if !status.success() {
                    return Err(format!("Font extraction failed: {status}").into());
                }
            } else {
                download(font_inputs::MONO_URL, &path)?;
            }
            font_inputs::verified(&path, hash, 1_048_576)?;
        }
        // Install only after every input has passed its integrity check.
        for (name, _, _) in font_inputs::FONTS {
            std::fs::rename(temporary.path().join(name), directory.join(name))?;
        }
    }
    println!("Verified Inter 4.1 and JetBrains Mono 2.304 at {}. Rebuild Archiv to embed them.", directory.display());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_or_oversized_font_input_is_rejected() {
        let fixture = tempfile::tempdir().unwrap();
        let path = fixture.path().join("input");
        std::fs::write(&path, b"abc").unwrap();
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(font_inputs::verified(&path, hash, 3).is_ok());
        assert!(font_inputs::verified(&path, hash, 2).is_err());
        std::fs::write(&path, b"abd").unwrap();
        assert!(font_inputs::verified(&path, hash, 3).is_err());
    }
}
