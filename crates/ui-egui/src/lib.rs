//! Native read-only desktop browsing. Widgets dispatch the same headless engine commands.

mod theme;
pub use theme::ThemePreference;

use archiv_core::{EntryId, EntryKind, Location, RelativePath, display_name};
use archiv_engine::{Browser, Command, FrameCommands, Phase};
use egui::RichText;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Presentation preferences only; sources and personal inventories are not persisted.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// System, light or dark appearance.
    pub theme: ThemePreference,
    /// Disable optional animation. Desktop motion integration is deferred.
    pub reduced_motion: bool,
}

/// Desktop shell backed by a read-only engine; it exposes no mutation controls.
pub struct ArchivApp {
    browser: Browser,
    commands: FrameCommands,
    preferences: Preferences,
    query: String,
    show_hidden: bool,
    selected: Option<EntryId>,
    filtered: Vec<usize>,
    filter_key: Option<(u64, String, bool)>,
    add_source: bool,
    source_input: String,
    focus_source: bool,
    error: Option<String>,
}

impl ArchivApp {
    /// Assemble a UI with an existing engine. Startup performs no implicit scans.
    pub fn new(browser: Browser, preferences: Preferences) -> Self {
        Self {
            browser,
            commands: FrameCommands::default(),
            preferences,
            query: String::new(),
            show_hidden: false,
            selected: None,
            filtered: Vec::new(),
            filter_key: None,
            add_source: false,
            source_input: String::new(),
            focus_source: false,
            error: None,
        }
    }

    /// Read the underlying engine for tests/headless inspection.
    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    /// Dispatch a startup source through the same route as the Open folder dialog.
    pub fn open_source(&mut self, frame: u64, path: PathBuf) {
        self.dispatch(frame, Command::AddSource(path));
    }

    fn dispatch(&mut self, frame: u64, command: Command) {
        let navigation = !matches!(command, Command::Cancel);
        let new_location = matches!(command, Command::AddSource(_) | Command::Navigate(_) | Command::Open(_));
        match self.commands.dispatch(frame, command, &mut self.browser) {
            Ok(()) => {
                self.error = None;
                if new_location {
                    self.query.clear();
                }
                if navigation {
                    self.selected = None;
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn open_dialog(&mut self) {
        self.add_source = true;
        self.focus_source = true;
    }

    fn parent_command(&self) -> Option<Command> {
        let location = self.browser.view().location.as_ref()?;
        Some(Command::Navigate(Location { source: location.source, path: location.path.parent()? }))
    }

    fn update_filter(&mut self) {
        let key = (self.browser.view().revision(), self.query.clone(), self.show_hidden);
        if self.filter_key.as_ref() != Some(&key) {
            self.filtered = self.browser.filtered_indices(&self.query, self.show_hidden);
            self.filter_key = Some(key);
        }
        if let Some(selected) = &self.selected
            && !self.browser.view().entries.iter().any(|entry| &entry.id == selected)
        {
            self.selected = None;
        }
    }

    /// Draw the native interface. This method only reads engine snapshots and queues commands.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let frame = ctx.cumulative_frame_nr();
        let t = theme::apply(&ctx, self.preferences.theme, self.preferences.reduced_motion);
        self.update_filter();
        let mut command = None;
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
            self.open_dialog();
        }
        if !self.add_source {
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
                command = Some(Command::Refresh);
            }
            if !ctx.memory(|memory| memory.has_focus(egui::Id::unique("filename-filter")))
                && ctx.input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::ArrowUp))
            {
                command = self.parent_command();
            }
        }
        egui::Panel::top("toolbar")
            .resizable(false)
            .frame(egui::Frame::new().fill(t.panel).inner_margin(egui::Margin::symmetric(18, 12)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("archiv").size(21.0).strong());
                    ui.add_space(14.0);
                    if ui.button("Open folder").on_hover_text("Add a physical source · Ctrl+O").clicked() {
                        self.open_dialog();
                    }
                    let parent = self.parent_command();
                    if ui
                        .add_enabled(parent.is_some(), egui::Button::new("Up"))
                        .on_disabled_hover_text("Already at the source root")
                        .on_hover_text("Parent folder · Alt+Up")
                        .clicked()
                    {
                        command = parent;
                    }
                    let has_source = self.browser.view().location.is_some();
                    if ui
                        .add_enabled(has_source, egui::Button::new("Refresh"))
                        .on_disabled_hover_text("Open a folder first")
                        .on_hover_text("Refresh directory · F5")
                        .clicked()
                    {
                        command = Some(Command::Refresh);
                    }
                    if self.browser.view().phase == Phase::Loading && ui.button("Cancel scan").clicked() {
                        command = Some(Command::Cancel);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("theme")
                            .selected_text(self.preferences.theme.label())
                            .width(86.0)
                            .show_ui(ui, |ui| {
                                for value in [ThemePreference::System, ThemePreference::Light, ThemePreference::Dark] {
                                    ui.selectable_value(&mut self.preferences.theme, value, value.label());
                                }
                            })
                            .response
                            .accessible_name("Display theme");
                        ui.label(RichText::new("Read only").color(t.muted));
                    });
                });
            });
        egui::Panel::bottom("status")
            .resizable(false)
            .frame(egui::Frame::new().fill(t.panel).inner_margin(egui::Margin::symmetric(18, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let status = match &self.browser.view().phase {
                        Phase::Idle => "No folder opened",
                        Phase::Loading => "Reading folder…",
                        Phase::Ready => "Ready",
                        Phase::Cancelled => "Cancelled · results may be incomplete",
                        Phase::Failed(_) => "Could not read folder",
                    };
                    ui.label(status);
                    ui.separator();
                    ui.label(format!("{} of {} items", self.filtered.len(), self.browser.view().entries.len()));
                    if let Some(summary) = &self.browser.view().summary {
                        if summary.skipped > 0 {
                            ui.label(format!("{} unreadable entries skipped", summary.skipped));
                        }
                        if summary.truncated {
                            ui.label("Listing limited to 50,000 entries");
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small("Local browsing · no file changes");
                    });
                });
            });
        egui::Panel::left("sources")
            .default_size(220.0)
            .min_size(170.0)
            .max_size(320.0)
            .frame(egui::Frame::new().fill(t.panel).inner_margin(16))
            .show(ui, |ui| {
                ui.label(RichText::new("SOURCES").small().color(t.muted));
                ui.add_space(8.0);
                if self.browser.sources().is_empty() {
                    ui.label(RichText::new("Add a folder to start browsing.").color(t.muted));
                }
                for source in self.browser.sources() {
                    let current = self.browser.view().location.as_ref().is_some_and(|l| l.source == source.id);
                    ui.push_id(source.id, |ui| {
                        let label = format!("{} · {}", source.label, source.id.0);
                        let response = ui.add_sized(
                            [ui.available_width(), 34.0],
                            egui::Button::selectable(current, ()).left_text(label).truncate(),
                        );
                        if response.on_hover_text(display_name(source.root.as_os_str())).clicked() {
                            command =
                                Some(Command::Navigate(Location { source: source.id, path: RelativePath::default() }));
                        }
                    });
                }
                ui.add_space(24.0);
                ui.label(RichText::new("LIBRARY").small().color(t.muted));
                for label in ["Movies", "TV", "Anime"] {
                    ui.add_enabled(false, egui::Button::new(label).frame(false)).on_disabled_hover_text(
                        "Merged library views are planned; browse physical sources in this version",
                    );
                }
                ui.add_space(24.0);
                ui.separator();
                ui.small("Sources stay separate on disk.");
                ui.add_space(8.0);
                ui.checkbox(&mut self.show_hidden, "Show hidden files");
                ui.checkbox(&mut self.preferences.reduced_motion, "Reduce motion");
            });
        let selected_entry = self
            .selected
            .as_ref()
            .and_then(|id| self.browser.view().entries.iter().find(|entry| &entry.id == id))
            .cloned();
        egui::Panel::right("details")
            .default_size(240.0)
            .min_size(190.0)
            .max_size(380.0)
            .frame(egui::Frame::new().fill(t.panel).inner_margin(18))
            .show(ui, |ui| {
                ui.label(RichText::new("DETAILS").small().color(t.muted));
                ui.add_space(16.0);
                if let Some(entry) = &selected_entry {
                    ui.label(RichText::new(display_name(&entry.name)).strong().size(16.0));
                    ui.add_space(10.0);
                    ui.label(entry.kind.label());
                    if let Some(bytes) = entry.bytes {
                        ui.label(format!("{} · {bytes} bytes", format_size(bytes)));
                    }
                    ui.add_space(16.0);
                    ui.small("PHYSICAL LOCATION");
                    if let Some(source) = self.browser.source(entry.id.source) {
                        ui.label(display_name(source.root.join(entry.id.path.as_path()).as_os_str()));
                        ui.add_space(12.0);
                        ui.small(format!("Source {} · {}", source.id.0, source.label));
                    }
                    ui.add_space(10.0);
                    ui.label(format!("Observed mode: {:04o}", entry.mode & 0o7777));
                    ui.small("Metadata only; not a write permission check.");
                    ui.add_space(18.0);
                    if ui
                        .add_enabled(entry.kind == EntryKind::Directory, egui::Button::new("Open selected folder"))
                        .on_disabled_hover_text("File playback and link navigation are not enabled yet")
                        .clicked()
                    {
                        command = Some(Command::Open(entry.id.clone()));
                    }
                    if entry.kind == EntryKind::Symlink {
                        ui.label("Symbolic links are listed but never followed.");
                    }
                } else {
                    ui.label(RichText::new("Select an item").strong());
                    ui.label(RichText::new("Its physical location and metadata will appear here.").color(t.muted));
                }
                ui.add_space(24.0);
                ui.separator();
                ui.small("No delete, move, upload or permission-changing operation is available.");
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(t.workspace).inner_margin(20)).show(ui, |ui| {
            if let Some(location) = &self.browser.view().location
                && let Some(source) = self.browser.source(location.source)
            {
                ui.label(RichText::new(&source.label).size(17.0).strong());
                ui.label(
                    RichText::new(display_name(source.root.join(location.path.as_path()).as_os_str()))
                        .small()
                        .color(t.muted),
                );
            } else {
                ui.heading("Your files, in one place");
                ui.label(
                    RichText::new("Start with a folder on your PC. Nothing is scanned until you choose it.")
                        .color(t.muted),
                );
            }
            ui.add_space(14.0);
            let search = ui
                .add(
                    egui::TextEdit::singleline(&mut self.query)
                        .id(egui::Id::unique("filename-filter"))
                        .hint_text("Filter this folder…")
                        .char_limit(4096)
                        .desired_width(ui.available_width())
                        .margin(egui::Margin::symmetric(10, 7)),
                )
                .accessible_name("Filter this folder");
            if !self.add_source && ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::F)) {
                search.request_focus();
            }
            self.update_filter();
            ui.add_space(12.0);
            let error = self.error.as_ref().or(match &self.browser.view().phase {
                Phase::Failed(error) => Some(error),
                _ => None,
            });
            if let Some(error) = error {
                egui::Frame::new()
                    .fill(t.card)
                    .stroke(egui::Stroke::new(1.0, t.border))
                    .inner_margin(12)
                    .corner_radius(6)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Unable to complete request").strong());
                        ui.label(error);
                    });
                ui.add_space(10.0);
            }
            if self.browser.view().phase == Phase::Idle {
                ui.add_space(40.0);
                ui.label(RichText::new("Choose your first source").size(20.0).strong());
                ui.label("Browse folders without changing their contents.");
                ui.add_space(14.0);
                if ui.button("Open a folder…").clicked() {
                    self.open_dialog();
                }
            } else if self.filtered.is_empty() {
                ui.add_space(30.0);
                ui.label(match self.browser.view().phase {
                    Phase::Loading => "Waiting for directory entries…",
                    Phase::Failed(_) => "No entries available. Check the path and permissions, then refresh.",
                    Phase::Cancelled => "Scan cancelled. Refresh to try again.",
                    _ if !self.query.is_empty() => "No filenames match this filter.",
                    _ => "This folder has no visible items.",
                });
            } else {
                self.file_table(ui, &t, &mut command);
            }
        });
        if self.add_source {
            let response = egui::Modal::new(egui::Id::unique("add-source"))
                .frame(
                    egui::Frame::new()
                        .fill(t.card)
                        .stroke(egui::Stroke::new(1.0, t.border))
                        .corner_radius(10)
                        .inner_margin(egui::Margin::symmetric(20, 16)),
                )
                .show(&ctx, |ui| {
                    ui.set_width(480.0);
                    ui.heading("Open a folder");
                    ui.label("Enter an absolute path on this PC. Browsing does not modify files.");
                    ui.add_space(10.0);
                    let input = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.source_input)
                                .desired_width(f32::INFINITY)
                                .hint_text("/home/you/Videos")
                                .char_limit(16384)
                                .margin(egui::Margin::symmetric(10, 7)),
                        )
                        .accessible_name("Folder path");
                    if self.focus_source {
                        input.request_focus();
                        self.focus_source = false;
                    }
                    ui.add_space(12.0);
                    let mut open = false;
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.add_source = false;
                        }
                        open = ui
                            .add_enabled(
                                PathBuf::from(&self.source_input).is_absolute(),
                                egui::Button::new("Browse folder"),
                            )
                            .clicked();
                    });
                    if open {
                        command = Some(Command::AddSource(PathBuf::from(&self.source_input)));
                        self.add_source = false;
                    }
                    if !self.source_input.is_empty() && !PathBuf::from(&self.source_input).is_absolute() {
                        ui.label("Use an absolute path beginning with /.");
                    }
                });
            if response.should_close() {
                self.add_source = false;
            }
        }
        if let Some(command) = command {
            self.dispatch(frame, command);
            ctx.request_repaint();
        }
    }

    fn file_table(&mut self, ui: &mut egui::Ui, t: &theme::Tokens, command: &mut Option<Command>) {
        use egui_extras::{Column, TableBuilder};
        TableBuilder::new(ui)
            .id_salt("physical-files")
            .striped(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .resizable(true)
            .column(Column::remainder().at_least(160.0))
            .column(Column::initial(70.0).at_least(50.0))
            .column(Column::initial(85.0).at_least(60.0))
            .header(30.0, |mut header| {
                for label in ["NAME", "KIND", "SIZE"] {
                    header.col(|ui| {
                        ui.label(RichText::new(label).small().color(t.muted));
                    });
                }
            })
            .body(|body| {
                body.rows(34.0, self.filtered.len(), |mut row| {
                    let Some(index) = self.filtered.get(row.index()) else { return };
                    let Some(entry) = self.browser.view().entries.get(*index) else { return };
                    let selected = self.selected.as_ref() == Some(&entry.id);
                    row.set_selected(selected);
                    row.col(|ui| {
                        ui.push_id(&entry.id, |ui| {
                            let response = ui.add_sized(
                                [ui.available_width(), 30.0],
                                egui::Button::selectable(selected, ())
                                    .left_text(display_name(&entry.name))
                                    .frame(false)
                                    .truncate(),
                            );
                            if response.clicked() {
                                self.selected = Some(entry.id.clone());
                                ui.ctx().request_repaint();
                            }
                            if entry.kind == EntryKind::Directory
                                && (response.double_clicked()
                                    || (response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))))
                            {
                                *command = Some(Command::Open(entry.id.clone()));
                            }
                            response.on_hover_text(display_name(entry.id.path.as_path().as_os_str()));
                        });
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(entry.kind.label()).color(t.muted));
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(entry.bytes.map(format_size).unwrap_or_else(|| "—".into())).color(t.muted),
                        );
                    });
                });
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
