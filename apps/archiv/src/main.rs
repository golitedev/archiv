//! Native Linux entry point and a read-only headless listing command.

use archiv_engine::{Browser, Command, Phase};
use archiv_platform::LocalDirectoryReader;
use archiv_ui_egui::{ArchivApp, Preferences, ThemePreference};
use std::error::Error;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const HELP: &str = "Archiv — native file manager\n\nUsage:\n  archiv [--browse PATH] [--theme system|light|dark]\n  archiv --list PATH [--search QUERY]\n  archiv --list-collection ID [--library FILE] [--search QUERY]\n  archiv [--library FILE]\n\nNo folder is scanned without an explicit path. Paths refer to this PC.\nPreferences configures sources, collections, applications and file-change safeguards. Permanent delete, rename and pasted copies are disabled until configured. Delete requires confirmation. Linux mounts NFS separately.";

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut browse = None;
    let mut list = None;
    let mut theme = None;
    let mut library_path = None;
    let mut collection = None;
    let mut search = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                writeln!(std::io::stdout().lock(), "{HELP}")?;
                return Ok(());
            }
            Some("--version" | "-V") => {
                writeln!(std::io::stdout().lock(), "archiv {}", env!("CARGO_PKG_VERSION"))?;
                return Ok(());
            }
            Some("--browse") if browse.is_none() => {
                browse = Some(absolute(args.next().ok_or("--browse requires a path")?)?)
            }
            Some("--list") if list.is_none() => list = Some(absolute(args.next().ok_or("--list requires a path")?)?),
            Some("--search") if search.is_none() => {
                search = Some(
                    args.next()
                        .ok_or("--search requires a query")?
                        .into_string()
                        .map_err(|_| "Search query must be UTF-8")?,
                );
            }
            Some("--library") if library_path.is_none() => {
                library_path = Some(absolute(args.next().ok_or("--library requires a settings file path")?)?);
            }
            Some("--list-collection") if collection.is_none() => {
                collection = Some(
                    args.next()
                        .and_then(|id| id.to_str().and_then(|id| id.parse::<u64>().ok()))
                        .ok_or("--list-collection requires a numeric collection ID")?,
                );
            }
            Some("--theme") if theme.is_none() => {
                theme = Some(match args.next().as_deref().and_then(|v| v.to_str()) {
                    Some("system") => ThemePreference::System,
                    Some("light") => ThemePreference::Light,
                    Some("dark") => ThemePreference::Dark,
                    _ => return Err("--theme requires system, light or dark".into()),
                });
            }
            _ => return Err(format!("Unknown or repeated argument: {}\n{HELP}", archiv_core_name(&arg)).into()),
        }
    }
    let library_path = match library_path {
        Some(path) => path,
        None => default_library_path()?,
    };
    let library = archiv_platform::library::load(&library_path).map_err(|e| {
        format!(
            "{}: {e}. Fix the file or use --library with another file; it has not been overwritten.",
            library_path.display()
        )
    })?;
    if collection.is_some() && list.is_some() {
        return Err("Choose --list or --list-collection".into());
    }
    if let Some(id) = collection {
        if browse.is_some() || theme.is_some() {
            return Err("--list-collection cannot be combined with GUI options".into());
        }
        return headless(Command::BrowseCollection(id), library, library_path, search);
    }
    if let Some(path) = list {
        if browse.is_some() || theme.is_some() {
            return Err("--list cannot be combined with GUI options".into());
        }
        return headless(Command::AddSource(path), library, library_path, search);
    }
    if search.is_some() {
        return Err("--search requires --list or --list-collection".into());
    }
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_app_id("archiv")
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([900.0, 580.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Archiv",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let mut browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(move || ctx.request_repaint()))?;
            browser.configure(library, library_path)?;
            let mut preferences: Preferences =
                cc.storage.and_then(|storage| eframe::get_value(storage, "archiv-presentation")).unwrap_or_default();
            if let Some(theme) = theme {
                preferences.theme = theme;
            }
            let mut app = ArchivApp::new(browser, preferences);
            if let Some(path) = browse {
                app.open_source(0, path);
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| {
        format!("Cannot start Archiv's native window: {error}. Check your Wayland/X11 session and OpenGL support.")
    })?;
    Ok(())
}

fn absolute(path: std::ffi::OsString) -> Result<PathBuf, std::io::Error> {
    let path = PathBuf::from(path);
    if path.is_absolute() { Ok(path) } else { Ok(std::env::current_dir()?.join(path)) }
}

fn archiv_core_name(value: &std::ffi::OsStr) -> String {
    value.to_string_lossy().escape_debug().to_string()
}

fn headless(
    command: Command,
    library: archiv_core::library::LibrarySettings,
    library_path: PathBuf,
    search: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let (wake_tx, wake_rx) = std::sync::mpsc::sync_channel(1);
    let mut browser = Browser::new(
        Arc::new(LocalDirectoryReader),
        Arc::new(move || {
            let _ = wake_tx.try_send(());
        }),
    )?;
    browser.configure(library, library_path)?;
    for (request, command) in (1..=2).zip(std::iter::once(command).chain(search.into_iter().map(Command::Search))) {
        browser.execute(request, command)?;
        while browser.view().phase == Phase::Loading {
            browser.poll();
            if browser.view().phase != Phase::Loading {
                break;
            }
            if wake_rx.recv_timeout(Duration::from_secs(30)).is_err() {
                return Err("Listing timed out; source unchanged".into());
            }
        }
    }
    if let Phase::Failed(error) = &browser.view().phase {
        return Err(format!("{error} {}", browser.view().errors.join("; ")).into());
    }
    let mut out = std::io::stdout().lock();
    for entry in &browser.view().entries {
        let locations = browser.view().contributors.get(&entry.id).cloned().unwrap_or_else(|| vec![entry.id.clone()]);
        for id in locations {
            if let Some(source) = browser.source(id.source) {
                writeln!(
                    out,
                    "{}\t{}\t{}\t{}",
                    entry.kind.label(),
                    archiv_core_name(&entry.name),
                    source.label,
                    archiv_core_name(source.root.join(id.path.as_path()).as_os_str())
                )?;
            }
        }
    }
    if let Some(summary) = &browser.view().summary
        && (summary.skipped > 0 || summary.truncated)
    {
        return Err(format!(
            "Partial listing: {} skipped; entry limit reached: {}",
            summary.skipped, summary.truncated
        )
        .into());
    }
    Ok(())
}

fn default_library_path() -> Result<PathBuf, Box<dyn Error>> {
    let base = if let Some(path) = std::env::var_os("XDG_CONFIG_HOME").filter(|p| !p.is_empty()) {
        PathBuf::from(path)
    } else {
        PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unavailable; specify --library PATH")?).join(".config")
    };
    if !base.is_absolute() {
        return Err("Configuration directory must be absolute; specify --library PATH".into());
    }
    Ok(base.join("archiv/library.json"))
}
