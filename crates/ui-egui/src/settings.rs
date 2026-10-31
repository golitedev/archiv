//! Native library editor. All persistence and filesystem checks go through the engine.
use archiv_core::library::{Collection, LibrarySettings};
use archiv_core::{Location, RelativePath, Source, SourceId};
use archiv_engine::Command;
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) struct Editor {
    draft: LibrarySettings,
    paths: HashMap<(u64, SourceId), String>,
    error: Option<String>,
}
impl Editor {
    pub fn new(draft: LibrarySettings) -> Self {
        let paths = draft
            .collections
            .iter()
            .flat_map(|c| {
                c.members.iter().map(move |m| ((c.id, m.source), m.path.as_path().to_string_lossy().into_owned()))
            })
            .collect();
        Self { draft, paths, error: None }
    }
    pub fn show(&mut self, ui: &mut egui::Ui, pending: bool, status: Option<&str>) -> Option<Command> {
        ui.heading("Library settings");
        ui.label("Sources are saved, but never scanned automatically at startup.");
        ui.label("Mount NFS shares in Linux first. Archiv does not install exports or mount disks.");
        ui.small("For NFS, use the exact mount point and server:/export shown by findmnt on this PC. An SSH alias may not be a network hostname.");
        let mut command = None;
        egui::ScrollArea::vertical().max_height((ui.ctx().content_rect().height() - 280.0).clamp(100.0, 440.0)).show(
            ui,
            |ui| {
                ui.add_enabled_ui(!pending, |ui| {
                    ui.heading("Physical sources");
                    let mut remove = None;
                    for source in &mut self.draft.sources {
                        ui.push_id(source.id, |ui| {
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(format!("Source {}", source.id.0));
                                if ui.button("Remove source").clicked() {
                                    remove = Some(source.id);
                                }
                            });
                            field(ui, "Source name", &mut source.label, 128);
                            let mut root = source.root.to_string_lossy().into_owned();
                            if field(ui, "Local mount / folder", &mut root, 16384) {
                                source.root = PathBuf::from(root);
                            }
                            let mut nfs = source.nfs_remote.is_some();
                            if ui.checkbox(&mut nfs, "Require expected NFS mount").changed() {
                                source.nfs_remote = nfs.then(|| "server:/export".into());
                            }
                            if let Some(remote) = &mut source.nfs_remote {
                                field(ui, "Expected NFS export", remote, 1024);
                            }
                        });
                    }
                    if let Some(id) = remove {
                        self.draft.sources.retain(|s| s.id != id);
                        for collection in &mut self.draft.collections {
                            collection.members.retain(|m| m.source != id);
                        }
                    }
                    if self.draft.sources.len() < 32 && ui.button("Add source").clicked() {
                        let id = self.draft.sources.iter().map(|s| s.id.0).max().unwrap_or(0).saturating_add(1);
                        self.draft.sources.push(Source {
                            id: SourceId(id),
                            label: format!("Disk {id}"),
                            root: PathBuf::new(),
                            nfs_remote: Some("server:/export".into()),
                        });
                    }
                    ui.add_space(12.0);
                    ui.heading("Optional merged collections");
                    ui.label("Choose contributing sources and a relative folder on each. Nothing is merged on disk.");
                    let mut remove = None;
                    for collection in &mut self.draft.collections {
                        ui.push_id(("collection", collection.id), |ui| {
                            ui.separator();
                            field(ui, "Collection name", &mut collection.label, 128);
                            for source in &self.draft.sources {
                                ui.push_id(source.id, |ui| {
                                    let mut enabled = collection.members.iter().any(|m| m.source == source.id);
                                    if ui.checkbox(&mut enabled, format!("Include {}", source.label)).changed() {
                                        if enabled {
                                            collection
                                                .members
                                                .push(Location { source: source.id, path: RelativePath::default() });
                                        } else {
                                            collection.members.retain(|m| m.source != source.id);
                                        }
                                    }
                                    if enabled {
                                        let path = self.paths.entry((collection.id, source.id)).or_default();
                                        field(ui, "Relative folder", path, 16384);
                                        ui.small("For example: data/media/movies. Empty means the source root.");
                                    }
                                });
                            }
                            if ui.button("Remove collection").clicked() {
                                remove = Some(collection.id);
                            }
                        });
                    }
                    if let Some(id) = remove {
                        self.draft.collections.retain(|c| c.id != id);
                    }
                    if self.draft.collections.len() < 16 && ui.button("Add collection").clicked() {
                        let id = self.draft.collections.iter().map(|c| c.id).max().unwrap_or(0).saturating_add(1);
                        self.draft.collections.push(Collection { id, label: "Movies".into(), members: Vec::new() });
                    }
                });
            },
        );
        if let Some(error) = &self.error {
            ui.label(error);
        }
        if let Some(status) = status {
            ui.label(status);
        }
        if ui.add_enabled(!pending, egui::Button::new("Save library")).clicked() {
            let mut settings = self.draft.clone();
            let result = (|| {
                for collection in &mut settings.collections {
                    for member in &mut collection.members {
                        member.path = RelativePath::new(
                            self.paths.get(&(collection.id, member.source)).cloned().unwrap_or_default(),
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
                settings.validate()
            })();
            match result {
                Ok(()) => {
                    self.error = None;
                    command = Some(Command::SaveLibrary(settings));
                }
                Err(error) => self.error = Some(error),
            }
        }
        command
    }
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String, limit: usize) -> bool {
    ui.label(format!("{label}:"));
    ui.add(egui::TextEdit::singleline(value).char_limit(limit).desired_width(f32::INFINITY))
        .accessible_name(label)
        .changed()
}
