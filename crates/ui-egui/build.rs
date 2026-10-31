//! Verify external font inputs without networking or committing font binaries.
#[allow(dead_code)]
mod font_inputs;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=ARCHIV_FONT_DIR");
    println!("cargo:rerun-if-changed=font_inputs.rs");
    let manifest =
        std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").ok_or("Missing manifest directory")?);
    let workspace = manifest.parent().and_then(std::path::Path::parent).ok_or("Missing workspace directory")?;
    let directory = font_inputs::directory(workspace)?;
    println!("cargo:rerun-if-changed={}", directory.display());
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or("Missing build output directory")?);
    let any = font_inputs::FONTS.iter().any(|(name, _, _)| directory.join(name).exists());
    let mut generated = String::new();
    if !any {
        println!(
            "cargo:warning=Inter inputs absent; using toolkit fallback fonts. Run cargo run -p archiv-fonts --locked, then rebuild."
        );
    }
    for (name, identifier, hash) in font_inputs::FONTS {
        if any {
            let bytes = font_inputs::verified(&directory.join(name), hash, 1_048_576)?;
            let embedded = output.join(name);
            std::fs::write(&embedded, bytes)?;
            generated
                .push_str(&format!("pub const {identifier}: Option<&[u8]> = Some(include_bytes!({:?}));\n", embedded));
        } else {
            generated.push_str(&format!("pub const {identifier}: Option<&[u8]> = None;\n"));
        }
    }
    std::fs::write(output.join("font_data.rs"), generated)?;
    Ok(())
}
