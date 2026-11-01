//! Native configurable file manager. Rendering dispatches typed, deduplicated engine commands.
mod columns;
mod fonts;
mod settings;
mod theme;
mod widgets;
use archiv_core::{EntryId, EntryKind, Location, RelativePath, display_name};
use archiv_engine::{Browser, Command, FrameCommands, Phase};
use egui::RichText;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub use theme::ThemePreference;

/// Presentation preferences; source definitions and permissions use the atomic library store.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Follow desktop or use a fixed theme.
    pub theme: ThemePreference,
    /// Disable optional animation.
    pub reduced_motion: bool,
    /// Manual percentage relative to native desktop scale; None follows the desktop.
    pub scale: Option<u16>,
}
/// Native shell; all filesystem and process effects are admitted by its engine.
pub struct ArchivApp {
    browser: Browser,
    commands: FrameCommands,
    preferences: Preferences,
    query: String,
    show_hidden: bool,
    selected: Option<EntryId>,
    add_source: bool,
    source_input: String,
    focus_source: bool,
    error: Option<String>,
    settings: Option<settings::Editor>,
    settings_initial: Option<Preferences>,
    settings_apply: Option<Preferences>,
    rename: Option<(EntryId, String)>,
    focus_rename: bool,
    about: bool,
    rows: Vec<(EntryId, usize)>,
    row_key: Option<(u64, String, bool)>,
    close_after_save: bool,
    focused_preview: Option<u64>,
    location_input: String,
    edit_location: bool,
    focus_location: bool,
    columns: columns::Columns,
}
impl ArchivApp {
    /// Assemble without scanning any implicit source.
    pub fn new(browser: Browser, preferences: Preferences) -> Self {
        Self {
            browser,
            commands: Default::default(),
            preferences,
            query: String::new(),
            show_hidden: false,
            selected: None,
            add_source: false,
            source_input: String::new(),
            focus_source: false,
            error: None,
            settings: None,
            settings_initial: None,
            settings_apply: None,
            rename: None,
            focus_rename: false,
            about: false,
            rows: Vec::new(),
            row_key: None,
            close_after_save: false,
            focused_preview: None,
            location_input: String::new(),
            edit_location: false,
            focus_location: false,
            columns: Default::default(),
        }
    }
    /// Headless engine inspection for tests.
    pub fn browser(&self) -> &Browser {
        &self.browser
    }
    /// Explicit startup navigation, using the shared dispatch path.
    pub fn open_source(&mut self, frame: u64, path: PathBuf) {
        self.dispatch(frame, Command::AddSource(path));
    }
    fn dispatch(&mut self, frame: u64, command: Command) {
        let searching = matches!(&command, Command::Search(_));
        let revisiting = matches!(&command, Command::Back | Command::Forward);
        let navigation = matches!(
            command,
            Command::AddSource(_)
                | Command::BrowsePath(_)
                | Command::Navigate(_)
                | Command::BrowseCollection(_)
                | Command::Open(_)
                | Command::Back
                | Command::Forward
                | Command::Refresh
                | Command::Search(_)
        );
        match self.commands.dispatch(frame, command, &mut self.browser) {
            Ok(()) => {
                self.error = None;
                if navigation {
                    if revisiting {
                        self.query = self.browser.search_query().unwrap_or_default().to_owned();
                    } else if !searching {
                        self.query.clear();
                    }
                    self.edit_location = false;
                    self.focus_location = false;
                    self.selected = None;
                }
            }
            Err(error) => {
                self.error = Some(error.to_string());
                self.close_after_save = false;
                self.settings_apply = None;
            }
        }
    }
    fn open_preferences(&mut self) {
        self.settings_initial = Some(self.preferences.clone());
        self.settings = Some(settings::Editor::new(self.browser.library().clone()));
    }
    fn open_dialog(&mut self) {
        self.add_source = true;
        self.focus_source = true;
    }
    fn edit_address(&mut self) {
        let physical = self.browser.view().location.as_ref().and_then(|location| {
            self.browser.source(location.source).map(|source| source.root.join(location.path.as_path()))
        });
        if let Some(path) = physical {
            if let Some(text) = path.to_str() {
                self.location_input = text.to_owned();
            } else {
                self.error = Some(
                    "This path contains non-UTF-8 bytes; use folder navigation instead of editing its display label."
                        .into(),
                );
                return;
            }
        } else {
            self.location_input.clear();
        }
        self.edit_location = true;
        self.focus_location = true;
    }
    fn open_command(&self) -> Option<Command> {
        let id = self.selected.as_ref()?;
        let entry = self.browser.entry(id)?;
        match entry.kind {
            EntryKind::Directory => Some(Command::Open(id.clone())),
            EntryKind::File => Some(Command::Launch(id.clone())),
            _ => None,
        }
    }
    fn refresh_rows(&mut self) {
        let key = (self.browser.view().revision(), self.query.clone(), self.show_hidden);
        if self.row_key.as_ref() != Some(&key) {
            self.rows = self.browser.rows(&self.query, self.show_hidden);
            self.row_key = Some(key);
        }
        if self.selected.as_ref().is_some_and(|id| !self.rows.iter().any(|(visible, _)| visible == id)) {
            self.selected = None;
        }
    }
    /// Draw only cached state. Commands enter bounded engine workers after rendering.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        fonts::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        ctx.set_zoom_factor(self.preferences.scale.map_or(1.0, |s| f32::from(s.clamp(75, 200)) / 100.0));
        if !self.browser.settings_pending()
            && let Some(applied) = self.settings_apply.take()
            && self.settings.is_some()
            && self.browser.settings_status().is_some_and(|s| s.starts_with("Library settings saved"))
        {
            self.settings_initial = Some(applied);
        }
        if self.close_after_save && !self.browser.settings_pending() {
            if self.browser.settings_status().is_some_and(|s| s.starts_with("Library settings saved")) {
                self.settings = None;
                self.settings_initial = None;
            }
            self.close_after_save = false;
        }
        if let Some(text) = self.browser.take_clipboard_text() {
            ctx.copy_text(text);
        }
        let t = theme::apply(&ctx, self.preferences.theme, self.preferences.reduced_motion);
        let frame = ctx.cumulative_frame_nr();
        self.refresh_rows();
        let mut command = None;
        let modal = self.add_source
            || self.settings.is_some()
            || self.browser.delete_preview().is_some()
            || self.rename.is_some()
            || self.about;
        if !modal {
            let back = ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft)
                    || i.pointer.button_released(egui::PointerButton::Extra1)
            });
            let forward = ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowRight)
                    || i.pointer.button_released(egui::PointerButton::Extra2)
            });
            if back && self.browser.can_back() {
                command = Some(Command::Back);
            } else if forward && self.browser.can_forward() {
                command = Some(Command::Forward);
            }
            if !ctx.text_edit_focused() {
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::C))
                    && let Some(id) = &self.selected
                {
                    command = Some(Command::Copy(id.clone()));
                }
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::V))
                    && let Some(location) = self.paste_location()
                {
                    command = Some(Command::Paste(location));
                }
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2))
                    && let Some(id) = self.selected.clone()
                {
                    self.begin_rename(&id);
                }
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::L)) {
                self.edit_address();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
                self.open_dialog();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
                command = Some(Command::Refresh);
            }
            if !self.edit_location
                && !ctx.text_edit_focused()
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
                && let Some(id) = &self.selected
            {
                command = Some(Command::PreviewDelete(id.clone()));
            }
        }
        egui::Panel::top("menus")
            .frame(egui::Frame::new().fill(t.panel).inner_margin(egui::Margin::symmetric(16, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("archiv").size(20.0));
                    ui.add_space(14.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(180.0, 30.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            egui::MenuBar::new().ui(ui, |ui| {
                                ui.menu_button("File", |ui| {
                                    if ui.button("Open folder…").clicked() {
                                        self.open_dialog();
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(self.open_command().is_some(), egui::Button::new("Open selected"))
                                        .on_disabled_hover_text("Select a regular file or folder first")
                                        .clicked()
                                    {
                                        command = self.open_command();
                                        ui.close();
                                    }
                                    ui.menu_button("Collections", |ui| {
                                        for collection in &self.browser.library().collections {
                                            if ui.button(&collection.label).clicked() {
                                                command = Some(Command::BrowseCollection(collection.id));
                                                ui.close();
                                            }
                                        }
                                    });
                                    ui.menu_button("Sources", |ui| {
                                        for source in self.browser.sources() {
                                            if ui.button(&source.label).clicked() {
                                                command = Some(Command::Navigate(Location {
                                                    source: source.id,
                                                    path: RelativePath::default(),
                                                }));
                                                ui.close();
                                            }
                                        }
                                    });
                                    ui.separator();
                                    if ui.button("Quit").clicked() {
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                });
                                ui.menu_button("Edit", |ui| {
                                    if ui.button("Preferences…").clicked() {
                                        self.open_preferences();
                                        ui.close();
                                    }
                                    self.edit_actions(ui, &mut command);
                                });
                                ui.menu_button("View", |ui| {
                                    if ui
                                        .add_enabled(
                                            self.browser.can_back(),
                                            egui::Button::new("Back").shortcut_text("Alt+Left"),
                                        )
                                        .on_disabled_hover_text("No previous location")
                                        .clicked()
                                    {
                                        command = Some(Command::Back);
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            self.browser.can_forward(),
                                            egui::Button::new("Forward").shortcut_text("Alt+Right"),
                                        )
                                        .on_disabled_hover_text("No next location")
                                        .clicked()
                                    {
                                        command = Some(Command::Forward);
                                        ui.close();
                                    }
                                    ui.separator();
                                    if ui.button("Enter a location…    Ctrl+L").clicked() {
                                        self.edit_address();
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            self.browser.is_loading() || self.browser.operation_pending(),
                                            egui::Button::new("Cancel operation"),
                                        )
                                        .on_disabled_hover_text("No scan or file operation is running")
                                        .clicked()
                                    {
                                        command = Some(Command::Cancel);
                                        ui.close();
                                    }
                                    ui.separator();
                                    if ui.button("Refresh").clicked() {
                                        command = Some(Command::Refresh);
                                        ui.close();
                                    }
                                    ui.checkbox(&mut self.show_hidden, "Show hidden files");
                                    ui.small("Use folder arrows to expand in place.");
                                });
                                ui.menu_button("Help", |ui| {
                                    if ui.button("About Archiv").clicked() {
                                        self.about = true;
                                        ui.close();
                                    }
                                });
                            });
                        },
                    );
                    self.search_toolbar(ui, &ctx, modal, &t, &mut command);
                });
            });
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(t.panel).inner_margin(egui::Margin::symmetric(16, 7)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2((ui.available_width() - 210.0).max(100.0), 30.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| self.location_bar(ui, &mut command),
                    );
                    ui.separator();
                    let status = match self.browser.view().phase {
                        Phase::Idle => "No folder opened",
                        Phase::Loading => "Reading…",
                        Phase::Ready => "Ready",
                        Phase::Cancelled => "Cancelled · partial results",
                        Phase::Failed(_) => "Incomplete results",
                    };
                    ui.small(status);
                    ui.separator();
                    ui.small(format!("{} visible items", self.rows.len()));
                    if let Some(summary) = &self.browser.view().summary {
                        if summary.truncated {
                            ui.small("50,000-entry limit reached");
                        }
                        if summary.skipped > 0 {
                            ui.small(format!("{} entries skipped", summary.skipped));
                        }
                    }
                    if let Some(status) = self.browser.operation_status() {
                        ui.add(egui::Label::new(status).truncate()).on_hover_text(status);
                    }
                });
            });
        if ctx.content_rect().width() >= 650.0 {
            egui::Panel::left("sources")
                .default_size(210.0)
                .min_size(165.0)
                .frame(egui::Frame::new().fill(t.panel).inner_margin(14))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.small("COLLECTIONS");
                        ui.add_space(6.0);
                        if self.browser.library().collections.is_empty() {
                            ui.weak("Create a collection in Preferences.");
                        }
                        for collection in &self.browser.library().collections {
                            ui.push_id(("collection", collection.id), |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(widgets::icon("collection", 17.0, t.media));
                                    if ui
                                        .add(
                                            egui::Button::selectable(
                                                self.browser.view().collection == Some(collection.id),
                                                (),
                                            )
                                            .left_text(&collection.label)
                                            .truncate(),
                                        )
                                        .on_hover_text(&collection.label)
                                        .clicked()
                                    {
                                        command = Some(Command::BrowseCollection(collection.id));
                                    }
                                });
                            });
                        }
                        ui.add_space(22.0);
                        ui.small("SOURCES");
                        ui.add_space(6.0);
                        if self.browser.sources().is_empty() {
                            ui.weak("Add a local or mounted folder.");
                        }
                        for source in self.browser.sources() {
                            ui.push_id(source.id, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(widgets::icon("source", 17.0, t.folder));
                                    let current =
                                        self.browser.view().location.as_ref().is_some_and(|l| l.source == source.id);
                                    if ui
                                        .add(egui::Button::selectable(current, ()).left_text(&source.label).truncate())
                                        .on_hover_text(format!(
                                            "{}\n{}",
                                            source.label,
                                            display_name(source.root.as_os_str())
                                        ))
                                        .clicked()
                                    {
                                        command = Some(Command::Navigate(Location {
                                            source: source.id,
                                            path: RelativePath::default(),
                                        }));
                                    }
                                });
                            });
                        }
                    });
                });
        }
        if ctx.content_rect().width() >= 1000.0 {
            egui::Panel::right("details")
                .default_size(225.0)
                .min_size(185.0)
                .max_size(360.0)
                .frame(egui::Frame::new().fill(t.panel).inner_margin(16))
                .show(ui, |ui| {
                    // Fill the requested pane width; text must wrap inside that geometry.
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        ui.small("DETAILS");
                        ui.add_space(12.0);
                        if let Some(entry) = self.selected.as_ref().and_then(|id| self.browser.entry(id)) {
                            ui.heading(display_name(&entry.name));
                            ui.label(entry.kind.label());
                            if let Some(size) = entry.bytes {
                                ui.label(format_size(size));
                            }
                            ui.add_space(12.0);
                            ui.small("PHYSICAL LOCATION");
                            let ids =
                                self.browser.contributors(&entry.id).cloned().unwrap_or_else(|| vec![entry.id.clone()]);
                            if self.browser.view().collection.is_some()
                                && !self.browser.view().search
                                && entry.kind == EntryKind::Directory
                            {
                                ui.label(format!("Virtual folder · {} locations", ids.len()));
                            }
                            for id in ids {
                                if let Some(source) = self.browser.source(id.source) {
                                    ui.label(&source.label);
                                    ui.label(
                                        RichText::new(display_name(source.root.join(id.path.as_path()).as_os_str()))
                                            .monospace(),
                                    );
                                    if self.browser.view().collection.is_some()
                                        && !self.browser.view().search
                                        && entry.kind == EntryKind::Directory
                                    {
                                        ui.push_id(("physical-location", &id), |ui| {
                                            if ui.button(format!("Browse {}", source.label)).clicked() {
                                                command = Some(Command::Navigate(Location {
                                                    source: id.source,
                                                    path: id.path.clone(),
                                                }));
                                            }
                                        });
                                    }
                                    ui.add_space(6.0);
                                }
                            }
                            ui.add_space(12.0);
                            let label = if entry.kind == EntryKind::Directory {
                                "Open selected folder"
                            } else {
                                "Open with application"
                            };
                            if ui
                                .add_enabled(self.open_command().is_some(), egui::Button::new(label))
                                .on_disabled_hover_text("Links and special files are not opened")
                                .clicked()
                            {
                                command = self.open_command();
                            }
                            let reason = self.browser.write_availability(&entry.id);
                            if ui
                                .add_enabled(
                                    reason.is_ok() && !self.browser.operation_pending(),
                                    egui::Button::new("Delete…"),
                                )
                                .on_disabled_hover_text(
                                    reason.err().unwrap_or_else(|| "Wait for the current operation".into()),
                                )
                                .clicked()
                            {
                                command = Some(Command::PreviewDelete(entry.id.clone()));
                            }
                            ui.small(
                                "Safeguards apply to Archiv operations. Filesystem permissions remain authoritative.",
                            );
                        } else {
                            ui.label("Select an item");
                            ui.weak("Its physical path and available actions appear here.");
                        }
                    });
                });
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(t.workspace).inner_margin(18)).show(ui, |ui| {
            if let Some(error) = &self.error {
                ui.label(error);
            }
            for error in &self.browser.view().errors {
                ui.label(error);
            }
            self.refresh_rows();
            if self.browser.view().phase == Phase::Idle {
                ui.label(RichText::new("Welcome to Archiv").font(fonts::semibold(24.0)));
                ui.weak("Browse your folders and build the collections you need.");
                ui.add_space(18.0);
                widgets::card(&t).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Start browsing").font(fonts::semibold(13.0)));
                    ui.label("Choose a local folder or a mounted network source.");
                    ui.add_space(10.0);
                    if widgets::pill(ui, "Open a folder…", true, &t).clicked() {
                        self.open_dialog();
                    }
                });
                ui.add_space(18.0);
                widgets::card(&t).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Your workspace").font(fonts::semibold(13.0)));
                    ui.label("Optional collections combine folders visually while preserving every physical location.");
                    ui.add_space(10.0);
                    if widgets::pill(ui, "Configure sources and collections", false, &t).clicked() {
                        self.open_preferences();
                    }
                });
            } else {
                self.file_list(ui, &t, &mut command);
            }
        });
        if self.add_source {
            let response = egui::Modal::new(egui::Id::unique("add-source")).frame(widgets::card(&t)).show(&ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 64.0).clamp(240.0, 480.0));
                ui.heading("Open a folder");
                ui.label("Absolute local path, including folders mounted from a NAS.");
                let input = ui
                    .add(widgets::text_field(&mut self.source_input).desired_width(f32::INFINITY).char_limit(16384))
                    .accessible_name("Folder path");
                if self.focus_source {
                    input.request_focus();
                    self.focus_source = false;
                }
                ui.horizontal(|ui| {
                    if widgets::pill(ui, "Cancel", false, &t).clicked() {
                        self.add_source = false;
                    }
                    if ui
                        .add_enabled_ui(PathBuf::from(&self.source_input).is_absolute(), |ui| {
                            widgets::pill(ui, "Browse folder", true, &t)
                        })
                        .inner
                        .clicked()
                    {
                        command = Some(Command::AddSource(self.source_input.clone().into()));
                        self.add_source = false;
                    }
                });
            });
            if response.should_close() {
                self.add_source = false;
            }
        }
        if let Some(editor) = &mut self.settings {
            let mut close = false;
            let response =
                egui::Modal::new(egui::Id::unique("preferences")).frame(widgets::card(&t)).show(&ctx, |ui| {
                    ui.set_width((ctx.content_rect().width() - 80.0).clamp(300.0, 680.0));
                    ui.heading("Preferences");
                    if let Some(save) = editor.show(
                        ui,
                        &mut self.preferences,
                        self.browser.settings_pending() || self.browser.operation_pending(),
                        self.browser.settings_status(),
                        &t,
                    ) {
                        self.settings_apply = Some(self.preferences.clone());
                        command = Some(save);
                    }
                    if let Some(error) = &self.error {
                        ui.label(error);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled_ui(
                                !self.browser.settings_pending() && !self.browser.operation_pending(),
                                |ui| widgets::pill(ui, "OK", true, &t),
                            )
                            .inner
                            .clicked()
                            && let Some(save) = editor.save_command()
                        {
                            command = Some(save);
                            self.close_after_save = true;
                        }
                        if widgets::pill(ui, "Cancel", false, &t).clicked() {
                            close = true;
                        }
                    });
                });
            if close || response.should_close() {
                self.settings = None;
                if let Some(initial) = self.settings_initial.take() {
                    self.preferences = initial;
                }
                ctx.request_repaint();
            }
        }
        if let Some(plan) = self.browser.delete_preview() {
            let id = plan.id;
            let focus_cancel = self.focused_preview != Some(id);
            self.focused_preview = Some(id);
            let response =
                egui::Modal::new(egui::Id::unique("confirm-delete")).frame(widgets::card(&t)).show(&ctx, |ui| {
                    ui.set_width((ctx.content_rect().width() - 64.0).clamp(240.0, 500.0));
                    ui.heading("Permanently delete this item?");
                    if let Some(status) = self.browser.operation_status() {
                        ui.label(status);
                    }
                    ui.label(&plan.source.label);
                    ui.label(display_name(plan.source.root.join(plan.entry.id.path.as_path()).as_os_str()));
                    ui.label("This permanently deletes the listed files and folders. It cannot be undone.");
                    ui.label(format!("{} physical items, including the selected item", plan.entries().len()));
                    egui::ScrollArea::vertical().max_height(180.0).show_rows(
                        ui,
                        24.0,
                        plan.entries().len(),
                        |ui, range| {
                            for index in range {
                                if let Some(entry) = plan.entries().get(index) {
                                    let path = display_name(plan.source.root.join(entry.id.path.as_path()).as_os_str());
                                    ui.add(egui::Label::new(&path).truncate()).on_hover_text(path);
                                }
                            }
                        },
                    );
                    ui.horizontal(|ui| {
                        let cancel = widgets::pill(ui, "Cancel", false, &t);
                        if focus_cancel {
                            cancel.request_focus();
                        }
                        if cancel.clicked() {
                            command = Some(Command::DismissDelete);
                        }
                        if ui
                            .add_enabled_ui(!self.browser.operation_pending(), |ui| {
                                widgets::pill(ui, "Delete permanently", true, &t)
                            })
                            .inner
                            .clicked()
                        {
                            command = Some(Command::ConfirmDelete(id));
                        }
                    });
                });
            if response.should_close() {
                command = Some(Command::DismissDelete);
            }
        }
        if let Some((id, name)) = &mut self.rename {
            let mut close = false;
            let response = egui::Modal::new(egui::Id::unique("rename")).frame(widgets::card(&t)).show(&ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 64.0).clamp(240.0, 480.0));
                ui.heading("Rename");
                if let Some(source) = self.browser.source(id.source) {
                    ui.label(display_name(source.root.join(id.path.as_path()).as_os_str()));
                }
                let field = ui.add(widgets::text_field(name).char_limit(255)).accessible_name("New name");
                if self.focus_rename {
                    field.request_focus();
                    self.focus_rename = false;
                }
                let submit = (field.has_focus() || field.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.small("Existing files are never overwritten.");
                ui.horizontal(|ui| {
                    if widgets::pill(ui, "Cancel", false, &t).clicked() {
                        close = true;
                    }
                    if widgets::pill(ui, "Rename", true, &t).clicked() || submit {
                        command = Some(Command::Rename(id.clone(), name.clone().into()));
                        close = true;
                    }
                });
            });
            if close || response.should_close() {
                self.rename = None;
            }
        }
        if self.about {
            let response = egui::Modal::new(egui::Id::unique("about")).frame(widgets::card(&t)).show(&ctx, |ui| {
                ui.heading("Archiv");
                ui.label("Native Linux file browsing and configurable visual collections.");
                ui.label("No telemetry. Lucide icons: ISC / MIT. Code: 0BSD.");
                ui.label(if fonts::INTER_REGULAR.is_some() {
                    "Typography: Inter 4.1 and JetBrains Mono 2.304"
                } else {
                    "Typography: toolkit fallback; verified font build inputs are unavailable"
                });
                ui.collapsing("Font licenses", |ui| {
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        ui.label(include_str!("../../../licenses/fonts/OFL-Inter.txt"));
                        ui.separator();
                        ui.label(include_str!("../../../licenses/fonts/OFL-JetBrainsMono.txt"));
                    });
                });
                if ui.button("Close").clicked() {
                    self.about = false;
                }
            });
            if response.should_close() {
                self.about = false;
            }
        }
        if let Some(command) = command {
            self.dispatch(frame, command);
            ctx.request_repaint();
        }
    }
    fn begin_rename(&mut self, id: &EntryId) {
        if self.browser.write_availability(id).is_ok() {
            let name = self.browser.entry(id).and_then(|e| e.name.to_str()).unwrap_or("").to_owned();
            self.rename = Some((id.clone(), name));
            self.focus_rename = true;
        }
    }
    fn paste_location(&self) -> Option<Location> {
        if let Some(id) = &self.selected
            && let Ok((_, entry)) = self.browser.physical(id)
            && entry.kind == EntryKind::Directory
        {
            return Some(Location { source: id.source, path: id.path.clone() });
        }
        if self.browser.view().collection.is_none() && !self.browser.view().search {
            return self.browser.view().location.clone();
        }
        None
    }
    fn edit_actions(&mut self, ui: &mut egui::Ui, command: &mut Option<Command>) {
        if let Some(id) = self.selected.clone() {
            self.item_actions(ui, &id, command);
        } else {
            ui.weak("Select a file or folder for actions.");
            self.paste_action(ui, command);
        }
    }
    fn paste_action(&self, ui: &mut egui::Ui, command: &mut Option<Command>) {
        let location = self.paste_location();
        let permitted = location
            .as_ref()
            .ok_or_else(|| "Choose an explicit physical destination folder".to_owned())
            .and_then(|location| self.browser.paste_availability(location));
        if ui
            .add_enabled(permitted.is_ok(), egui::Button::new("Paste").shortcut_text("Ctrl+V"))
            .on_disabled_hover_text(permitted.err().unwrap_or_default())
            .clicked()
        {
            if let Some(location) = location {
                *command = Some(Command::Paste(location));
            }
            ui.close();
        }
    }
    fn item_actions(&mut self, ui: &mut egui::Ui, id: &EntryId, command: &mut Option<Command>) {
        let physical = self.browser.physical(id);
        let kind = self.browser.entry(id).map(|e| e.kind);
        if ui.button("Open").clicked() {
            *command = Some(if kind == Some(EntryKind::Directory) {
                Command::Open(id.clone())
            } else {
                Command::Launch(id.clone())
            });
            ui.close();
        }
        ui.separator();
        for (label, action) in [("Copy", Command::Copy(id.clone())), ("Copy path", Command::CopyPath(id.clone()))] {
            if ui
                .add_enabled(physical.is_ok(), egui::Button::new(label))
                .on_disabled_hover_text(physical.as_ref().err().map(ToString::to_string).unwrap_or_default())
                .clicked()
            {
                *command = Some(action);
                ui.close();
            }
        }
        self.paste_action(ui, command);
        ui.separator();
        let permitted = self.browser.write_availability(id);
        let reason = permitted.as_ref().err().cloned().unwrap_or_else(|| "Wait for the current operation".into());
        if ui
            .add_enabled(
                permitted.is_ok() && !self.browser.operation_pending(),
                egui::Button::new("Rename…").shortcut_text("F2"),
            )
            .on_disabled_hover_text(&reason)
            .clicked()
        {
            self.begin_rename(id);
            ui.close();
        }
        if ui
            .add_enabled(
                permitted.is_ok() && !self.browser.operation_pending(),
                egui::Button::new("Delete…").shortcut_text("Delete"),
            )
            .on_disabled_hover_text(&reason)
            .clicked()
        {
            *command = Some(Command::PreviewDelete(id.clone()));
            ui.close();
        }
    }
    fn location_bar(&mut self, ui: &mut egui::Ui, command: &mut Option<Command>) {
        if self.edit_location {
            let address = ui
                .add(
                    widgets::text_field(&mut self.location_input)
                        .char_limit(16384)
                        .desired_width(ui.available_width())
                        .hint_text("/absolute/folder"),
                )
                .accessible_name("Location address");
            if self.focus_location {
                address.request_focus();
                self.focus_location = false;
            }
            if (address.has_focus() || address.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                *command = Some(Command::BrowsePath(self.location_input.clone().into()));
                self.edit_location = false;
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.edit_location = false;
            }
        } else {
            let label = if let Some(id) = self.browser.view().collection {
                self.browser
                    .library()
                    .collections
                    .iter()
                    .find(|c| c.id == id)
                    .map_or_else(|| "Merged view".into(), |c| format!("{} / merged view", c.label))
            } else if let Some(location) = &self.browser.view().location {
                self.browser
                    .source(location.source)
                    .map_or(String::new(), |source| display_name(source.root.join(location.path.as_path()).as_os_str()))
            } else {
                "Enter a location…  Ctrl+L".into()
            };
            if ui
                .add_sized([ui.available_width(), 30.0], egui::Button::new(label).truncate())
                .accessible_name("Current location · Ctrl+L")
                .on_hover_text("Edit location · Ctrl+L")
                .clicked()
            {
                self.edit_address();
            }
        }
    }
    fn search_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        modal: bool,
        t: &theme::Tokens,
        command: &mut Option<Command>,
    ) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let search = ui
                .scope(|ui| {
                    let style = ui.style_mut();
                    for widget in [
                        &mut style.visuals.widgets.inactive,
                        &mut style.visuals.widgets.hovered,
                        &mut style.visuals.widgets.active,
                    ] {
                        widget.corner_radius = egui::CornerRadius::same(16);
                    }
                    ui.add(
                        widgets::text_field(&mut self.query)
                            .id(egui::Id::unique("filename-filter"))
                            .hint_text("Search…  Enter for subfolders")
                            .desired_width((ui.available_width() - 76.0).clamp(70.0, 240.0))
                            .char_limit(4096)
                            .margin(egui::Margin::symmetric(12, 7)),
                    )
                    .accessible_name("Filter this folder")
                })
                .inner;
            if !modal && search.has_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                *command = Some(Command::Search(self.query.clone()));
            }
            if !modal && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F)) {
                search.request_focus();
            }
            if widgets::navigation_button(ui, "Forward", "right", !modal && self.browser.can_forward(), t).clicked() {
                *command = Some(Command::Forward);
            }
            if widgets::navigation_button(ui, "Back", "left", !modal && self.browser.can_back(), t).clicked() {
                *command = Some(Command::Back);
            }
        });
    }
    fn file_list(&mut self, ui: &mut egui::Ui, t: &theme::Tokens, command: &mut Option<Command>) {
        let background =
            ui.interact(ui.available_rect_before_wrap(), ui.scope_id().with("file-background"), egui::Sense::click());
        if background.secondary_clicked() {
            self.selected = None;
        }
        background.context_menu(|ui| {
            self.paste_action(ui, command);
            if ui.button("Refresh").clicked() {
                *command = Some(Command::Refresh);
                ui.close();
            }
        });
        let width = ui.available_width();
        let [name_width, kind_width, size_width, location_width] =
            self.columns.header(ui, width, self.browser.view().search, t);
        let kind_offset = name_width;
        let size_offset = name_width + kind_width;
        let location_offset = size_offset + size_width;
        ui.separator();
        if self.rows.is_empty() {
            ui.weak(if self.browser.view().phase == Phase::Loading {
                "Reading folders…"
            } else if self.query.is_empty() {
                "This folder has no visible items."
            } else {
                "No matching names. Press Enter to search subfolders."
            });
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 26.0, self.rows.len(), |ui, range| {
            for index in range {
                let Some((id, depth)) = self.rows.get(index).cloned() else { continue };
                let Some(entry) = self.browser.entry(&id).cloned() else { continue };
                let selected = self.selected.as_ref() == Some(&id);
                let branch = self.browser.branch(&id);
                let expanded = branch.is_some_and(|b| b.expanded);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::hover());
                let mut response = ui.interact(rect, ui.scope_id().with(("row", &id)), egui::Sense::click());
                // Reserve the background below child controls, then resolve its hover as one row.
                let background = ui.painter().add(egui::Shape::Noop);
                // The shared bottom edge belongs to the next row, not both rows.
                let row_hovered = response.container_hovered()
                    && ui.input(|input| input.pointer.interact_pos().is_some_and(|pos| pos.y < rect.bottom()));
                let mut disclosure_hovered = false;
                let row_clicked = response.clicked();
                let name_rect = egui::Rect::from_min_size(rect.min, egui::vec2(name_width, 26.0));
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt(("entry", &id))
                        .max_rect(name_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                child.spacing_mut().item_spacing.x = 4.0;
                child.spacing_mut().interact_size.y = 22.0;
                child.add_space(((depth as f32) * 16.0).min((name_width - 70.0).max(0.0)));
                if entry.kind == EntryKind::Directory {
                    let arrow = child
                        .add(
                            egui::Button::image(widgets::icon(if expanded { "down" } else { "right" }, 13.0, t.muted))
                                .frame(false)
                                .min_size(egui::vec2(18.0, 22.0)),
                        )
                        .accessible_name(format!(
                            "{} {}",
                            if expanded { "Collapse" } else { "Expand" },
                            display_name(&entry.name)
                        ));
                    disclosure_hovered = arrow.container_hovered();
                    if arrow.clicked() {
                        *command = Some(Command::Expand(id.clone()));
                    }
                    response = response.union(arrow);
                } else {
                    child.add_space(22.0);
                }
                let (icon, color) = widgets::file_icon(&entry, expanded, t);
                child.add(widgets::icon(icon, 17.0, color));
                let label = display_name(&entry.name);
                let name = child.add(egui::Button::selectable(selected, ()).left_text(label).frame(false).truncate());
                if name.clicked() {
                    self.selected = Some(id.clone());
                    ui.ctx().request_repaint();
                }
                if name.double_clicked() || (name.has_focus() && child.input(|i| i.key_pressed(egui::Key::Enter))) {
                    *command = Some(if entry.kind == EntryKind::Directory {
                        Command::Open(id.clone())
                    } else {
                        Command::Launch(id.clone())
                    });
                }
                response = response.union(name);
                for (offset, cell_width, text) in [
                    (kind_offset, kind_width, entry.kind.label().to_owned()),
                    (size_offset, size_width, entry.bytes.map(format_size).unwrap_or_else(|| "—".into())),
                ] {
                    widgets::cell(
                        ui,
                        egui::Rect::from_min_size(rect.min + egui::vec2(offset, 0.0), egui::vec2(cell_width, 26.0)),
                        ("column", &id, offset.to_bits()),
                        &text,
                        t.muted,
                        false,
                    );
                }
                let location = if let Some(branch) = branch.filter(|b| b.expanded) {
                    match &branch.view.phase {
                        Phase::Loading => "Reading…".into(),
                        Phase::Failed(_) => "Unavailable".into(),
                        Phase::Cancelled => "Cancelled".into(),
                        _ if branch.view.summary.as_ref().is_some_and(|s| s.truncated || s.skipped > 0) => {
                            "Partial listing".into()
                        }
                        _ => self
                            .browser
                            .contributors(&id)
                            .map_or_else(|| "Folder".into(), |ids| format!("{} locations", ids.len())),
                    }
                } else if let Some(ids) = self.browser.contributors(&id) {
                    format!("{} locations", ids.len())
                } else {
                    self.browser.source(id.source).map_or(String::new(), |s| {
                        if self.browser.view().search {
                            format!(
                                "{} · {}",
                                s.label,
                                display_name(id.path.parent().unwrap_or_default().as_path().as_os_str())
                            )
                        } else {
                            s.label.clone()
                        }
                    })
                };
                let location_response = widgets::cell(
                    ui,
                    egui::Rect::from_min_size(
                        rect.min + egui::vec2(location_offset, 0.0),
                        egui::vec2(location_width, 26.0),
                    ),
                    ("location", &id),
                    &location,
                    t.muted,
                    false,
                );
                if let Some(branch) = branch.filter(|b| !b.view.errors.is_empty()) {
                    location_response.on_hover_text(branch.view.errors.join("\n"));
                } else if let Some(summary) =
                    branch.and_then(|b| b.view.summary.as_ref()).filter(|s| s.truncated || s.skipped > 0)
                {
                    location_response.on_hover_text(format!(
                        "Listing limit reached: {}. Unreadable entries: {}. Refresh to retry.",
                        summary.truncated, summary.skipped
                    ));
                } else if let Some(source) = self.browser.source(id.source) {
                    location_response.on_hover_text(display_name(source.root.join(id.path.as_path()).as_os_str()));
                }
                if selected || (row_hovered && !disclosure_hovered) {
                    ui.painter().set(
                        background,
                        egui::Shape::rect_filled(
                            rect.shrink2(egui::vec2(0.0, 1.0)),
                            6,
                            if selected { t.selected } else { t.hover },
                        ),
                    );
                }
                if row_clicked {
                    self.selected = Some(id.clone());
                    ui.ctx().request_repaint();
                }
                if response.secondary_clicked() {
                    self.selected = Some(id.clone());
                }
                response.context_menu(|ui| self.item_actions(ui, &id, command));
            }
        });
    }
}
impl eframe::App for ArchivApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.browser.poll() {
            ctx.request_repaint();
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "archiv-presentation", &self.preferences);
    }
}
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
