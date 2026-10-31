//! Native Linux entry point and a read-only headless listing command.

use archiv_engine::{Browser, Command, Phase};
use archiv_platform::LocalDirectoryReader;
use archiv_ui_egui::{ArchivApp, Preferences, ThemePreference};
use std::error::Error;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const HELP: &str = "Archiv — read-only native file browser\n\nUsage:\n  archiv [--browse PATH] [--theme system|light|dark]\n  archiv --list PATH\n\nNo folder is scanned without an explicit path. Paths refer to this PC.\nFile mutation, merged libraries, transfers and NAS configuration are not implemented yet.";

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut browse = None;
    let mut list = None;
    let mut theme = None;
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
    if let Some(path) = list {
        if browse.is_some() || theme.is_some() {
            return Err("--list cannot be combined with GUI options".into());
        }
        return headless(path);
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
            let browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(move || ctx.request_repaint()))?;
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

fn headless(path: PathBuf) -> Result<(), Box<dyn Error>> {
    let (wake_tx, wake_rx) = std::sync::mpsc::sync_channel(1);
    let mut browser = Browser::new(
        Arc::new(LocalDirectoryReader),
        Arc::new(move || {
            let _ = wake_tx.try_send(());
        }),
    )?;
    browser.execute(1, Command::AddSource(path))?;
    while browser.view().phase == Phase::Loading {
        browser.poll();
        if browser.view().phase != Phase::Loading {
            break;
        }
        if wake_rx.recv_timeout(Duration::from_secs(30)).is_err() {
            return Err("Listing timed out; source unchanged".into());
        }
    }
    if let Phase::Failed(error) = &browser.view().phase {
        return Err(error.clone().into());
    }
    let mut out = std::io::stdout().lock();
    for entry in &browser.view().entries {
        writeln!(out, "{}\t{}", entry.kind.label(), archiv_core_name(&entry.name))?;
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
