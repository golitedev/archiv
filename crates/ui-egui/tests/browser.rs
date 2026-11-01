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
    h.get_by_label("File").click();
    h.run_steps(3);
    h.get_by_label("Open folder…").click();
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
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowLeft);
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
    preferences(&mut h);
    h.get_by_label("Choose Dark theme").click();
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
    preferences(&mut h);
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
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowLeft);
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
    preferences(&mut h);
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
fn delete_confirmation_cancels_safely_then_permanently_deletes_only_the_selected_item() {
    use archiv_core::actions::{ProtectedPath, WritePolicy};
    use archiv_core::library::LibrarySettings;
    use archiv_core::{RelativePath, Source, SourceId};
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("disk");
    std::fs::create_dir_all(root.join("media/Title")).unwrap();
    std::fs::write(root.join("media/Title/notes.txt"), b"safe").unwrap();
    let source = Source { id: SourceId(1), root: root.clone(), label: "Fixture".into(), nfs_remote: None, sftp: None };
    let config = LibrarySettings {
        sources: vec![source],
        writes: vec![WritePolicy {
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
            h.get_by_label("Delete…").click();
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
            h.get_by_label("Delete permanently").click_accesskit();
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
    assert!(!root.join("media/Title").exists());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
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

#[test]
fn whole_theme_cards_select_a_theme_and_cancel_restores_appearance() {
    let mut h = harness().unwrap();
    preferences(&mut h);
    h.run_steps(20);
    h.get_by_label("Choose Dark theme").click();
    h.run_steps(5);
    assert!(h.ctx.style_of(egui::Theme::Dark).visuals.dark_mode);
    h.get_by_label("Cancel").click_accesskit();
    h.run_steps(5);
    assert!(!h.ctx.style_of(egui::Theme::Dark).visuals.dark_mode);
    preferences(&mut h);
    h.run_steps(20);
    h.get_by_label("Choose Dark theme").click_accesskit();
    h.run_steps(5);
    assert!(h.ctx.style_of(egui::Theme::Dark).visuals.dark_mode);
    h.key_press(egui::Key::Escape);
    h.run_steps(5);
    assert!(!h.ctx.style_of(egui::Theme::Dark).visuals.dark_mode);
}

#[test]
fn failed_apply_does_not_commit_appearance_cancel_baseline() {
    for fail in [false, true] {
        let fixture = tempfile::tempdir().unwrap();
        let path = if fail {
            std::fs::write(fixture.path().join("not-a-directory"), b"occupied").unwrap();
            fixture.path().join("not-a-directory/library.json")
        } else {
            fixture.path().join("settings/library.json")
        };
        let mut browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(|| {})).unwrap();
        browser.configure(Default::default(), path).unwrap();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1180.0, 760.0))
            .build_eframe(move |_cc| ArchivApp::new(browser, Preferences::default()));
        preferences(&mut h);
        h.run_steps(20);
        h.get_by_label("Choose Dark theme").click_accesskit();
        h.run_steps(5);
        h.get_by_label("Applications").click_accesskit();
        h.run_steps(10);
        h.get_by_label("Save library").click_accesskit();
        h.run_steps(3);
        let deadline = Instant::now() + Duration::from_secs(3);
        while h.state().browser().settings_pending() && Instant::now() < deadline {
            h.run_steps(2);
            std::thread::sleep(Duration::from_millis(1));
        }
        h.run_steps(5);
        h.get_by_label("Cancel").click_accesskit();
        h.run_steps(5);
        assert_eq!(h.ctx.style_of(egui::Theme::Dark).visuals.dark_mode, !fail);
    }
}

fn preferences(h: &mut Harness<'_, ArchivApp>) {
    h.get_by_label("Edit").click();
    h.run_steps(3);
    h.get_by_label("Preferences…").click();
    h.run_steps(20);
}

#[test]
fn bottom_address_and_menu_navigation_replace_the_toolbar() {
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir(fixture.path().join("child")).unwrap();
    let mut h = harness().unwrap();
    assert!(h.query_by_label("Display theme").is_none());
    assert!(h.query_by_label("Show hidden files").is_none());
    assert!(h.query_by_label("Refresh").is_none());
    assert!(h.query_by_label("Preferences…").is_none());
    h.state_mut().open_source(0, fixture.path().to_owned());
    settle(&mut h);
    h.get_by_label("View").click();
    h.run_steps(3);
    h.get_by_label("Enter a location…    Ctrl+L").click();
    h.run_steps(3);
    let address = h.get_by_label("Location address");
    assert!(address.rect().top() > 700.0);
    address.click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label("Location address").type_text(fixture.path().join("child").to_str().unwrap());
    h.run_steps(3);
    assert_eq!(h.get_by_label("Location address").value().as_deref(), fixture.path().join("child").to_str());
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    settle(&mut h);
    assert_eq!(h.state().browser().view().location.as_ref().unwrap().path.as_path(), std::path::Path::new("child"));
    h.get_by_label("View").click();
    h.run_steps(3);
    h.get_by_label("Show hidden files").click();
    h.run_steps(3);
    h.get_by_label("View").click();
    h.run_steps(3);
    h.get_by_label("Back Alt+Left").click();
    h.run_steps(3);
    settle(&mut h);
    assert_eq!(h.state().browser().view().location.as_ref().unwrap().path.as_path(), std::path::Path::new(""));
    assert_eq!(h.state().browser().view().entries.first().unwrap().name, "child");
}

#[test]
fn header_drag_changes_only_adjacent_columns_and_stays_put_on_idle_frames() {
    let fixture = tempfile::tempdir().unwrap();
    std::fs::write(fixture.path().join("notes.txt"), b"notes").unwrap();
    let mut h = harness().unwrap();
    h.state_mut().open_source(0, fixture.path().to_owned());
    settle(&mut h);
    let divider = h.get_by_label("Resize Name and Kind columns").rect().center();
    let size = h.get_by_label("SIZE").rect().left();
    h.hover_at(divider);
    h.run_steps(2);
    h.drag_at(divider);
    h.run_steps(2);
    h.hover_at(divider - egui::vec2(25.0, 0.0));
    h.run_steps(4);
    let moved = h.get_by_label("Resize Name and Kind columns").rect().center();
    assert!((moved.x - (divider.x - 25.0)).abs() < 2.0);
    assert!((h.get_by_label("SIZE").rect().left() - size).abs() < 1.0);
    h.drop_at(moved);
    h.run_steps(5);
    assert!((h.get_by_label("Resize Name and Kind columns").rect().center().x - moved.x).abs() < 1.0);
}

#[test]
fn details_splitter_tracks_the_pointer_with_empty_and_selected_content() {
    let fixture = tempfile::tempdir().unwrap();
    let name = "a-long-synthetic-filename-that-must-wrap-within-the-details-pane-without-moving-its-divider.txt";
    std::fs::write(fixture.path().join(name), b"notes").unwrap();
    let mut h = harness().unwrap();
    h.state_mut().open_source(0, fixture.path().to_owned());
    settle(&mut h);
    for selected in [false, true] {
        if selected {
            h.get_by_label(name).click();
            h.run_steps(3);
        }
        let initial = h.get_by_label("DETAILS").rect().left();
        let start = egui::pos2(initial - 17.0, 250.0);
        h.hover_at(start);
        h.run_steps(2);
        h.drag_at(start);
        h.run_steps(2);
        for distance in [12.0, 24.0, 36.0, 24.0, 12.0, 0.0] {
            h.hover_at(start - egui::vec2(distance, 0.0));
            h.run_steps(3);
            let left = h.get_by_label("DETAILS").rect().left();
            assert!((left - (initial - distance)).abs() < 3.0, "initial {initial}, distance {distance}, left {left}");
        }
        h.drop_at(start);
        h.run_steps(3);
        assert!((h.get_by_label("DETAILS").rect().left() - initial).abs() < 3.0);
    }
}

#[test]
fn context_menus_copy_paste_and_rename_the_clicked_physical_item() {
    use archiv_core::{RelativePath, Source, SourceId, actions::WritePolicy, library::LibrarySettings};
    let fixture = tempfile::tempdir().unwrap();
    std::fs::write(fixture.path().join("notes.txt"), b"safe").unwrap();
    std::fs::create_dir(fixture.path().join("out")).unwrap();
    let source = Source {
        id: SourceId(1),
        root: fixture.path().to_owned(),
        label: "Fixture".into(),
        nfs_remote: None,
        sftp: None,
    };
    let config = LibrarySettings {
        sources: vec![source],
        writes: vec![WritePolicy {
            source: SourceId(1),
            enabled: true,
            allowed: vec![RelativePath::default()],
            protected: Vec::new(),
        }],
        ..Default::default()
    };
    let mut browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(|| {})).unwrap();
    browser.configure(config, fixture.path().join("config/library.json")).unwrap();
    let root = fixture.path().to_owned();
    let mut h = Harness::builder().with_size(egui::vec2(1180.0, 760.0)).build_eframe(move |_cc| {
        let mut app = ArchivApp::new(browser, Preferences::default());
        app.open_source(0, root);
        app
    });
    settle(&mut h);
    h.get_by_label("notes.txt").click_secondary();
    h.run_steps(4);
    h.get_by_label("Copy").click();
    h.run_steps(3);
    h.get_by_label("out").click_secondary();
    h.run_steps(4);
    h.get_by_label("Paste Ctrl+V").click();
    h.run_steps(3);
    let deadline = Instant::now() + Duration::from_secs(3);
    while h.state().browser().operation_pending() && Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(Duration::from_millis(1));
    }
    settle(&mut h);
    assert_eq!(std::fs::read(fixture.path().join("out/notes.txt")).unwrap(), b"safe");
    h.get_by_label("notes.txt").click_secondary();
    h.run_steps(4);
    h.get_by_label("Rename… F2").click();
    h.run_steps(4);
    h.get_by_label("New name").click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label("New name").type_text("renamed.txt");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let deadline = Instant::now() + Duration::from_secs(3);
    while h.state().browser().operation_pending() && Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(Duration::from_millis(1));
    }
    settle(&mut h);
    assert!(fixture.path().join("renamed.txt").exists());
    assert!(!fixture.path().join("notes.txt").exists());
    assert_eq!(std::fs::read(fixture.path().join("out/notes.txt")).unwrap(), b"safe");
}

#[test]
fn sources_splitter_tracks_small_drags_with_long_navigation_labels() {
    use archiv_core::{
        Location, RelativePath, Source, SourceId,
        library::{Collection, LibrarySettings},
    };
    let fixture = tempfile::tempdir().unwrap();
    let mut browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(|| {})).unwrap();
    let source = Source {
        id: SourceId(1),
        root: fixture.path().to_owned(),
        label: "A long synthetic physical source name that must not force the sidebar width".into(),
        nfs_remote: None,
        sftp: None,
    };
    let config = LibrarySettings {
        sources: vec![source],
        collections: vec![Collection {
            id: 1,
            label: "A long synthetic collection name that must not force the sidebar width".into(),
            members: vec![Location { source: SourceId(1), path: RelativePath::default() }],
        }],
        ..Default::default()
    };
    browser.configure(config, fixture.path().join("library.json")).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1180.0, 760.0))
        .build_eframe(move |_cc| ArchivApp::new(browser, Preferences::default()));
    h.state_mut().open_source(0, fixture.path().to_owned());
    settle(&mut h);
    let initial = h.get_by_label("NAME").rect().left();
    assert!(initial < 280.0, "Navigation labels forced an oversized sidebar: {initial}");
    let start = egui::pos2(initial - 22.0, 300.0);
    h.hover_at(start);
    h.run_steps(2);
    h.drag_at(start);
    h.run_steps(2);
    for distance in [4.0, 8.0, 12.0, 24.0, 40.0, 24.0, 8.0, 0.0, -8.0, -16.0, 0.0] {
        h.hover_at(start + egui::vec2(distance, 0.0));
        h.run_steps(3);
        let actual = h.get_by_label("NAME").rect().left();
        assert!((actual - (initial + distance)).abs() < 2.0, "initial {initial}, distance {distance}, actual {actual}");
    }
    h.drop_at(start);
    h.run_steps(5);
    assert!((h.get_by_label("NAME").rect().left() - initial).abs() < 2.0);
}

#[test]
fn navigation_buttons_keyboard_and_mouse_side_buttons_share_history() {
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir(fixture.path().join("child")).unwrap();
    let mut h = harness().unwrap();
    h.state_mut().open_source(0, fixture.path().to_owned());
    settle(&mut h);
    assert!(!h.state().browser().can_back());
    assert!(!h.state().browser().can_forward());
    h.get_by_label("child").click();
    h.run_steps(2);
    h.get_by_label("Open selected folder").click();
    h.run_steps(2);
    settle(&mut h);
    assert!(h.state().browser().can_back());
    h.get_by_label("Back").click();
    h.run_steps(3);
    settle(&mut h);
    assert!(h.state().browser().view().location.as_ref().unwrap().path.as_path().as_os_str().is_empty());
    h.get_by_label("Forward").click();
    h.run_steps(3);
    settle(&mut h);
    assert_eq!(h.state().browser().view().location.as_ref().unwrap().path.as_path(), std::path::Path::new("child"));
    for (button, child) in [(egui::PointerButton::Extra1, false), (egui::PointerButton::Extra2, true)] {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: egui::pos2(500.0, 400.0),
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run_steps(3);
        settle(&mut h);
        assert_eq!(
            h.state().browser().view().location.as_ref().unwrap().path.as_path() == std::path::Path::new("child"),
            child
        );
    }
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::O);
    h.run_steps(3);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: egui::pos2(500.0, 400.0),
            button: egui::PointerButton::Extra1,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(3);
    assert_eq!(h.state().browser().view().location.as_ref().unwrap().path.as_path(), std::path::Path::new("child"));
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowLeft);
    h.run_steps(3);
    settle(&mut h);
    assert!(h.state().browser().view().location.as_ref().unwrap().path.as_path().as_os_str().is_empty());
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowRight);
    h.run_steps(3);
    settle(&mut h);
    assert_eq!(h.state().browser().view().location.as_ref().unwrap().path.as_path(), std::path::Path::new("child"));
}

#[test]
fn row_hover_is_stable_over_names_columns_and_slow_vertical_movement() {
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir(fixture.path().join("Series")).unwrap();
    for index in 0..8 {
        std::fs::create_dir(fixture.path().join(format!("Folder {index:02}"))).unwrap();
    }
    std::fs::write(fixture.path().join("notes.txt"), b"safe").unwrap();
    let mut h = harness().unwrap();
    h.state_mut().open_source(0, fixture.path().to_owned());
    settle(&mut h);
    let hover = h.ctx.style_of(egui::Theme::Light).visuals.widgets.hovered.bg_fill;
    fn has_row_fill(h: &Harness<'_, ArchivApp>, y: f32, color: egui::Color32) -> bool {
        h.output().shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Rect(rect) => {
                rect.fill == color
                    && rect.rect.width() > 300.0
                    && (rect.rect.height() - 24.0).abs() < 1.0
                    && (rect.rect.center().y - y).abs() < 1.0
            }
            _ => false,
        })
    }
    for label in ["Series", "notes.txt"] {
        let name = h.get_by_label(label).rect();
        let y = name.center().y;
        let left = h.get_by_label("NAME").rect().left() - 4.0;
        let mut points = vec![egui::pos2(left + 32.0, y), name.center()];
        for column in ["KIND", "SIZE", "LOCATION"] {
            points.push(egui::pos2(h.get_by_label(column).rect().center().x, y));
        }
        points.push(egui::pos2(h.get_by_label("KIND").rect().left() - 18.0, y));
        for point in points {
            h.hover_at(point);
            h.run_steps(3);
            assert!(has_row_fill(&h, y, hover), "Missing full-row highlight for {label} at {point:?}");
        }
        if label == "Series" {
            h.hover_at(h.get_by_label("Expand Series").rect().center());
            h.run_steps(3);
            assert!(!has_row_fill(&h, y, hover), "Chevron owns its own hover region");
        }
        h.hover_at(egui::pos2(4.0, 500.0));
        h.run_steps(3);
        assert!(!has_row_fill(&h, y, hover));
    }
    let centers: Vec<_> = (0..8).map(|index| h.get_by_label(&format!("Folder {index:02}")).rect().center()).collect();
    let start = centers.first().unwrap();
    let end = centers.last().unwrap();
    for y in (start.y as i32)..=(end.y as i32) {
        h.hover_at(egui::pos2(start.x, y as f32));
        h.run_steps(1);
        let expected = (((y as f32) - (start.y - 13.0)) / 26.0).floor() as usize;
        for (index, center) in centers.iter().enumerate() {
            assert_eq!(has_row_fill(&h, center.y, hover), index == expected, "Slow hover y={y}, folder {index}");
        }
    }
    let name = h.get_by_label("Series").rect();
    h.hover_at(name.center());
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::O);
    h.run_steps(3);
    assert!(!has_row_fill(&h, name.center().y, hover), "A modal must cover underlying row hover");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.get_by_label("Series").click();
    h.run_steps(3);
    h.hover_at(egui::pos2(4.0, 500.0));
    h.run_steps(3);
    assert!(has_row_fill(&h, name.center().y, h.ctx.style_of(egui::Theme::Light).visuals.selection.bg_fill));
}

#[test]
fn row_highlights_and_columns_leave_the_scrollbar_lane_clear() {
    let fixture = tempfile::tempdir().unwrap();
    for index in 0..100 {
        std::fs::write(fixture.path().join(format!("item-{index:03}.txt")), b"safe").unwrap();
    }
    let mut h = harness().unwrap();
    h.state_mut().open_source(0, fixture.path().to_owned());
    settle(&mut h);
    h.get_by_label("item-000.txt").click();
    h.run_steps(3);
    h.hover_at(h.get_by_label("item-001.txt").rect().center());
    h.run_steps(3);
    let scrollbar = h.get_by_role(egui::Role::ScrollBar).rect();
    let style = h.ctx.style_of(egui::Theme::Light);
    let selected = style.visuals.selection.bg_fill;
    let hover = style.visuals.widgets.hovered.bg_fill;
    let right = h.get_by_label("LOCATION").rect().right() + 4.0;
    let fills: Vec<_> = h
        .output()
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if (rect.fill == selected || rect.fill == hover)
                    && rect.rect.width() > 300.0
                    && (rect.rect.height() - 24.0).abs() < 1.0 =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .collect();
    assert_eq!(fills.len(), 2, "Both selection and hover must be painted");
    for rect in fills {
        assert!(
            rect.right() + style.spacing.scroll.bar_inner_margin <= scrollbar.left() + 0.5,
            "Row {rect:?} overlaps scrollbar lane {scrollbar:?}"
        );
        assert!((rect.right() - right).abs() < 0.5, "Header/row geometry must agree");
    }
    h.hover_at(egui::pos2(scrollbar.center().x, h.get_by_label("item-001.txt").rect().center().y));
    h.run_steps(3);
    assert!(!h.output().shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::Rect(rect) if rect.fill==hover && rect.rect.width()>300.0 && (rect.rect.height()-24.0).abs()<1.0)),"Hovering the scrollbar must not hover a file");
    h.get_by_label("Filter this folder").click();
    h.run_steps(2);
    h.get_by_label("Filter this folder").type_text("item-00");
    h.run_steps(5);
    assert!(
        (h.get_by_label("LOCATION").rect().right() + 4.0 - right).abs() < 0.5,
        "Filtering away overflow must not move the columns"
    );
}

#[test]
fn preferences_saves_sftp_without_connecting_and_shows_read_only_safeguards() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("library.json");
    let mut browser = Browser::new(Arc::new(LocalDirectoryReader), Arc::new(|| {})).unwrap();
    browser.configure(Default::default(), path.clone()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1180.0, 760.0))
        .build_eframe(move |_cc| ArchivApp::new(browser, Preferences::default()));
    preferences(&mut h);
    h.run_steps(3);
    h.get_by_label("Sources").click_accesskit();
    h.run_steps(3);
    h.get_by_label("Add source").click();
    h.run_steps(3);
    h.get_by_label("Direct SFTP (browse only)").click_accesskit();
    h.run_steps(3);
    h.get_by_label("Remote root folder").click();
    h.run_steps(2);
    h.get_by_label("Remote root folder").type_text("/synthetic/media");
    h.run_steps(2);
    h.get_by_label("SFTP host or SSH alias").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("SFTP host or SSH alias").click();
    h.run_steps(2);
    h.get_by_label("SFTP host or SSH alias").type_text("synthetic-nas");
    h.run_steps(2);
    h.get_by_label("Save library").click();
    h.run_steps(3);
    let deadline = Instant::now() + Duration::from_secs(3);
    while h.state().browser().settings_pending() && Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(Duration::from_millis(1));
    }
    h.run_steps(3);
    let saved = archiv_platform::library::load(&path).unwrap();
    assert_eq!(saved.sources[0].sftp.as_ref().unwrap().host, "synthetic-nas");
    assert_eq!(saved.sources[0].root, std::path::Path::new("/synthetic/media"));
    assert_eq!(h.state().browser().view().phase, Phase::Idle);
    h.get_by_label("Safeguards").click_accesskit();
    h.run_steps(3);
    h.get_by_label("SFTP · browsing only. File changes are unavailable.");
    assert!(h.query_by_label("Allow permanent delete, rename and pasted copies").is_none());
}
