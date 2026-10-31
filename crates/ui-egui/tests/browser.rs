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

#[test]
fn settings_creates_saved_optional_collection_and_browses_it() {
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(fixture.path().join("disk/movies/Title")).unwrap();
    std::fs::write(fixture.path().join("disk/movies/Title/movie.mkv"), b"untouched").unwrap();
    let path = fixture.path().join("config/library.json");
    let mut browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(|| {})).unwrap();
    browser.configure(Default::default(), path.clone()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1180.0, 760.0))
        .build_eframe(move |_cc| ArchivApp::new(browser, Preferences::default()));
    h.get_by_label("Preferences").click();
    h.run_steps(3);
    h.run_steps(20);
    h.get_by_label("Sources").click_accesskit();
    h.run_steps(5);
    h.get_by_label("Add source").click();
    h.run_steps(3);
    h.run_steps(20);
    h.get_by_label("Local mount / folder").scroll_to_me();
    h.run_steps(10);
    h.get_by_label("Local mount / folder").click();
    h.run_steps(2);
    assert!(h.get_by_label("Local mount / folder").is_focused());
    h.get_by_label("Local mount / folder").type_text(fixture.path().join("disk").to_str().unwrap());
    h.run_steps(2);
    h.get_by_label("Collections").click_accesskit();
    h.run_steps(5);
    h.get_by_label("Add collection").scroll_to_me();
    h.run_steps(10);
    h.get_by_label("Add collection").click();
    h.run_steps(3);
    h.get_by_label("Include Source 1").scroll_to_me();
    h.run_steps(10);
    h.get_by_label("Include Source 1").click();
    h.run_steps(3);
    h.get_by_label("Relative folder").scroll_to_me();
    h.run_steps(10);
    h.get_by_label("Relative folder").click();
    h.run_steps(2);
    h.get_by_label("Relative folder").type_text("movies");
    h.run_steps(2);
    h.run_steps(20);
    h.get_by_label("Save library").click();
    h.run_steps(3);
    let deadline = Instant::now() + Duration::from_secs(3);
    while h.state().browser().settings_pending() && Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(Duration::from_millis(1));
    }
    h.run_steps(3);
    assert_eq!(h.state().browser().view().phase, Phase::Idle);
    assert_eq!(archiv_platform::library::load(&path).unwrap().collections.len(), 1);
    h.get_by_label("OK").click();
    h.run_steps(3);
    h.get_by_label("Collection").click();
    h.run_steps(3);
    settle(&mut h);
    assert_eq!(h.state().browser().view().collection, Some(1));
    h.get_by_label("Title").click();
    h.run_steps(2);
    h.get_by_label("Open selected folder").click();
    h.run_steps(2);
    settle(&mut h);
    assert_eq!(h.state().browser().view().entries[0].name, "movie.mkv");
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowUp);
    h.run_steps(2);
    settle(&mut h);
    assert_eq!(h.state().browser().view().entries[0].name, "Title");
}

#[test]
fn folder_disclosure_expands_inline_and_search_preserves_parent_context() {
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir(fixture.path().join("Series")).unwrap();
    std::fs::write(fixture.path().join("Series/notes.txt"), b"safe").unwrap();
    let browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(|| {})).unwrap();
    let root = fixture.path().to_owned();
    let mut h = Harness::builder().with_size(egui::vec2(1180.0, 760.0)).build_eframe(move |_cc| {
        let mut app = ArchivApp::new(browser, Preferences::default());
        app.open_source(0, root);
        app
    });
    settle(&mut h);
    let location = h.state().browser().view().location.clone();
    h.get_by_label("Expand Series").click();
    h.run_steps(3);
    let deadline = Instant::now() + Duration::from_secs(3);
    while h.query_by_label("notes.txt").is_none() && Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(Duration::from_millis(1));
    }
    h.get_by_label("notes.txt");
    assert_eq!(h.state().browser().view().location, location);
    h.get_by_label("Filter this folder").click();
    h.run_steps(2);
    h.get_by_label("Filter this folder").type_text("notes");
    h.run_steps(3);
    h.get_by_label("Series");
    h.get_by_label("notes.txt");
    h.get_by_label("Collapse Series").click();
    h.run_steps(3);
    assert!(h.query_by_label("notes.txt").is_none());
}

#[test]
fn preferences_ok_saves_application_configuration_before_closing() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("config/library.json");
    let mut browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(|| {})).unwrap();
    browser.configure(Default::default(), path.clone()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1180.0, 760.0))
        .build_eframe(move |_cc| ArchivApp::new(browser, Preferences::default()));
    h.get_by_label("Preferences").click();
    h.run_steps(20);
    h.get_by_label("Applications").click_accesskit();
    h.run_steps(10);
    h.get_by_label("Text editor").click();
    h.run_steps(2);
    h.get_by_label("Text editor").type_text("kwrite");
    h.run_steps(3);
    h.get_by_label("OK").click_accesskit();
    h.run_steps(3);
    let deadline = Instant::now() + Duration::from_secs(3);
    while h.state().browser().settings_pending() && Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(Duration::from_millis(1));
    }
    h.run_steps(5);
    assert_eq!(archiv_platform::library::load(&path).unwrap().openers.text, "kwrite");
    assert!(h.query_by_label("Text editor").is_none());
}

#[test]
fn recovery_confirmation_cancels_safely_then_moves_only_the_selected_item() {
    use archiv_core::actions::{ProtectedPath, RecoveryPolicy};
    use archiv_core::library::LibrarySettings;
    use archiv_core::{RelativePath, Source, SourceId};
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("disk");
    std::fs::create_dir_all(root.join("media/Title")).unwrap();
    std::fs::write(root.join("media/Title/notes.txt"), b"safe").unwrap();
    let source = Source { id: SourceId(1), root: root.clone(), label: "Fixture".into(), nfs_remote: None };
    let config = LibrarySettings {
        sources: vec![source],
        recovery: vec![RecoveryPolicy {
            source: SourceId(1),
            enabled: true,
            allowed: vec![RelativePath::default()],
            protected: vec![ProtectedPath { path: RelativePath::new("media").unwrap(), subtree: false }],
        }],
        ..Default::default()
    };
    let mut browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(|| {})).unwrap();
    browser.configure(config, fixture.path().join("config/library.json")).unwrap();
    let start = root.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1180.0, 760.0)).build_eframe(move |_cc| {
        let mut app = ArchivApp::new(browser, Preferences::default());
        app.open_source(0, start);
        app
    });
    settle(&mut h);
    h.get_by_label("media").click();
    h.run_steps(2);
    h.get_by_label("Open selected folder").click();
    h.run_steps(2);
    settle(&mut h);
    h.get_by_label("Title").click();
    h.run_steps(2);
    for attempt in 0..2 {
        if attempt == 0 {
            h.key_press(egui::Key::Delete);
        } else {
            h.get_by_label("Move to recovery…").click();
        }
        h.run_steps(3);
        let deadline = Instant::now() + Duration::from_secs(3);
        while h.state().browser().operation_pending() && Instant::now() < deadline {
            h.run_steps(2);
            std::thread::sleep(Duration::from_millis(1));
        }
        h.run_steps(10);
        assert!(root.join("media/Title").exists());
        if attempt == 0 {
            h.get_by_label("Cancel").click_accesskit();
            h.run_steps(5);
            assert!(root.join("media/Title").exists());
        } else {
            h.get_by_label("Move to recovery").click_accesskit();
            h.run_steps(3);
            let deadline = Instant::now() + Duration::from_secs(3);
            while h.state().browser().operation_pending() && Instant::now() < deadline {
                h.run_steps(2);
                std::thread::sleep(Duration::from_millis(1));
            }
            settle(&mut h);
            assert!(!root.join("media/Title").exists());
        }
    }
    let job = std::fs::read_dir(root.join(".archiv-recovery")).unwrap().next().unwrap().unwrap();
    assert_eq!(std::fs::read(job.path().join("payload/notes.txt")).unwrap(), b"safe");
    assert!(root.join("media").is_dir());
}

#[test]
fn manual_scaling_keeps_menus_and_folder_dialog_accessible() {
    for scale in [100, 125, 150, 200] {
        let browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(|| {})).unwrap();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1180.0, 760.0))
            .build_eframe(move |_cc| ArchivApp::new(browser, Preferences { scale: Some(scale), ..Default::default() }));
        h.run_steps(10);
        h.get_by_label("File");
        h.get_by_label("Edit");
        h.get_by_label("View");
        h.get_by_label("Filter this folder");
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::O);
        h.run_steps(10);
        h.get_by_label("Folder path");
        h.get_by_label("Cancel");
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(h.query_by_label("Folder path").is_none());
    }
}
