//! Synthetic native-egui interactions, without NAS access or a display server.

use archiv_engine::{Browser, Phase};
use archiv_platform::LocalDirectoryReader;
use archiv_ui_egui::{ArchivApp, Preferences};
use egui_kittest::{Harness, kittest::Queryable};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn harness() -> Result<Harness<'static, ArchivApp>, Box<dyn std::error::Error>> {
    let browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(|| {}))?;
    Ok(Harness::builder()
        .with_size(egui::vec2(1180.0, 760.0))
        .build_eframe(move |_cc| ArchivApp::new(browser, Preferences::default())))
}

fn settle(harness: &mut Harness<'_, ArchivApp>) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while harness.state().browser().view().phase == Phase::Loading && Instant::now() < deadline {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(1));
    }
    harness.run_steps(2);
    assert_ne!(harness.state().browser().view().phase, Phase::Loading);
}

#[test]
fn folder_dialog_filter_selection_and_navigation_use_real_read_only_engine() {
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir(fixture.path().join("Series")).unwrap();
    std::fs::write(fixture.path().join("Series/episode.mkv"), b"untouched").unwrap();
    std::fs::write(fixture.path().join("notes.txt"), b"notes").unwrap();
    let mut h = harness().unwrap();
    assert_eq!(h.state().browser().view().phase, Phase::Idle);
    h.get_by_label("Open folder").click();
    h.run_steps(3);
    h.get_by_label("Folder path").type_text(fixture.path().to_str().unwrap());
    h.run_steps(2);
    h.get_by_label("Browse folder").click();
    h.run_steps(2);
    settle(&mut h);
    assert_eq!(h.state().browser().view().entries.len(), 2);
    h.get_by_label("Filter this folder").click();
    h.run_steps(2);
    h.get_by_label("Filter this folder").type_text("Series");
    h.run_steps(2);
    assert!(h.query_by_label("notes.txt").is_none());
    h.get_by_label("Series").click();
    h.run_steps(2);
    h.get_by_label("Open selected folder").click();
    h.run_steps(2);
    settle(&mut h);
    assert_eq!(h.state().browser().view().entries.len(), 1);
    assert_eq!(h.state().browser().view().entries.first().unwrap().name, "episode.mkv");
    assert_eq!(std::fs::read(fixture.path().join("Series/episode.mkv")).unwrap(), b"untouched");
    h.get_by_label("episode.mkv").click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowUp);
    h.run_steps(2);
    settle(&mut h);
    assert!(h.state().browser().view().location.as_ref().unwrap().path.as_path().as_os_str().is_empty());
}

#[test]
fn keyboard_opens_and_cancels_the_source_dialog_without_scanning() {
    let mut h = harness().unwrap();
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::O);
    h.run_steps(3);
    h.get_by_label("Folder path");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.query_by_label("Folder path").is_none());
    assert_eq!(h.state().browser().view().phase, Phase::Idle);
    assert!(h.state().browser().sources().is_empty());
}

#[test]
fn theme_choice_applies_to_both_framework_style_branches() {
    let mut h = harness().unwrap();
    h.get_by_label("Display theme").click();
    h.run_steps(3);
    h.get_by_label("Dark").click();
    h.run_steps(3);
    for branch in [egui::Theme::Light, egui::Theme::Dark] {
        assert!(h.ctx.style_of(branch).visuals.dark_mode);
        assert_eq!(h.ctx.style_of(branch).visuals.panel_fill, egui::Color32::from_rgb(38, 38, 42));
    }
}
