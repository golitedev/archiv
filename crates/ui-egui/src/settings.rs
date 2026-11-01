//! Categorized native preferences; settings writes are admitted through the engine.
use crate::{Preferences, ThemePreference, theme::Tokens, widgets};
use archiv_core::actions::{ProtectedPath, WritePolicy};
use archiv_core::library::{Collection, LibrarySettings};
use archiv_core::{Location, RelativePath, Source, SourceId};
use archiv_engine::Command;
use std::collections::HashMap;
use std::path::PathBuf;
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Appearance,
    Sources,
    Collections,
    Applications,
    Safeguards,
}
pub(crate) struct Editor {
    draft: LibrarySettings,
    page: Page,
    members: HashMap<u64, Vec<(SourceId, String)>>,
    allowed: HashMap<SourceId, String>,
    protected: HashMap<SourceId, String>,
    subtrees: HashMap<SourceId, String>,
    error: Option<String>,
}
impl Editor {
    pub fn new(draft: LibrarySettings) -> Self {
        let members = draft
            .collections
            .iter()
            .map(|c| {
                (c.id, c.members.iter().map(|m| (m.source, m.path.as_path().to_string_lossy().into_owned())).collect())
            })
            .collect();
        let mut allowed = HashMap::new();
        let mut protected = HashMap::new();
        let mut subtrees = HashMap::new();
        for policy in &draft.writes {
            allowed.insert(
                policy.source,
                policy
                    .allowed
                    .iter()
                    .map(|p| {
                        if p.as_path().as_os_str().is_empty() {
                            std::borrow::Cow::Borrowed(".")
                        } else {
                            p.as_path().to_string_lossy()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            protected.insert(
                policy.source,
                policy
                    .protected
                    .iter()
                    .filter(|p| !p.subtree)
                    .map(|p| p.path.as_path().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            subtrees.insert(
                policy.source,
                policy
                    .protected
                    .iter()
                    .filter(|p| p.subtree)
                    .map(|p| p.path.as_path().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        Self { draft, page: Page::Appearance, members, allowed, protected, subtrees, error: None }
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        preferences: &mut Preferences,
        pending: bool,
        status: Option<&str>,
        t: &Tokens,
    ) -> Option<Command> {
        ui.horizontal_wrapped(|ui| {
            for (page, label) in [
                (Page::Appearance, "Appearance"),
                (Page::Sources, "Sources"),
                (Page::Collections, "Collections"),
                (Page::Applications, "Applications"),
                (Page::Safeguards, "Safeguards"),
            ] {
                ui.selectable_value(&mut self.page, page, label);
            }
        });
        ui.separator();
        let mut command = None;
        egui::ScrollArea::vertical().id_salt("preferences-body").max_height((ui.ctx().content_rect().height()-250.0).clamp(120.0,520.0)).show(ui,|ui|{
            ui.set_min_height(300.0);
            if self.page==Page::Appearance {
                appearance(ui,preferences,t);
            }else{ui.add_enabled_ui(!pending,|ui|match self.page{
                Page::Sources=>{
                    section(ui,"Physical sources");ui.label("Saved definitions never scan automatically at startup.");ui.small("For NFS, mount the share in Linux, then enter the local mount point and exact server:/export shown by findmnt. SFTP sources connect directly using OpenSSH; no mount is needed.");
                    let mut remove=None;
                    for source in &mut self.draft.sources{ui.push_id(source.id,|ui|{
                        ui.add_space(10.0);ui.separator();field(ui,"Source name",&mut source.label,128);
                        let mut root=source.root.to_string_lossy().into_owned();if field(ui,if source.sftp.is_some(){"Remote root folder"}else{"Local mount / folder"},&mut root,16384){source.root=PathBuf::from(root);}
                        let mut remote_source=source.sftp.is_some();
                        if ui.checkbox(&mut remote_source,"Direct SFTP (browse only)").changed(){
                            source.sftp=remote_source.then(||archiv_core::sftp::SftpConnection{host:String::new(),user:String::new(),port:None});
                            if remote_source{source.nfs_remote=None;self.draft.writes.retain(|p|p.source!=source.id);}
                        }
                        if let Some(connection)=&mut source.sftp{
                            field(ui,"SFTP host or SSH alias",&mut connection.host,253);
                            field(ui,"SFTP username (optional)",&mut connection.user,128);
                            let mut port=connection.port.unwrap_or(0);
                            ui.horizontal(|ui|{ui.label("SFTP port (0 uses SSH config)");if ui.add(egui::DragValue::new(&mut port).range(0..=65535)).changed(){connection.port=(port!=0).then_some(port);}});
                            ui.small("Uses this PC's SSH config, keys/agent and trusted known_hosts. No passwords are stored. Files are listed only; opening apps and file changes are unavailable.");
                        }
                        if source.sftp.is_none(){
                        let mut nfs=source.nfs_remote.is_some();if ui.checkbox(&mut nfs,"Require expected NFS mount").changed(){source.nfs_remote=nfs.then(||"server:/export".into());}
                        if let Some(remote)=&mut source.nfs_remote{field(ui,"Expected NFS export",remote,1024);}
                        }
                        if ui.button("Remove source").clicked(){remove=Some(source.id);}
                    });}
                    if let Some(id)=remove{self.allowed.remove(&id);self.protected.remove(&id);self.subtrees.remove(&id);self.draft.sources.retain(|s|s.id!=id);self.draft.writes.retain(|p|p.source!=id);for members in self.members.values_mut(){members.retain(|(source,_)|*source!=id);}for c in &mut self.draft.collections{c.members.retain(|m|m.source!=id);}}
                    if self.draft.sources.len()<32&&ui.button("Add source").clicked(){let id=self.draft.sources.iter().map(|s|s.id.0).max().unwrap_or(0).saturating_add(1);self.draft.sources.push(Source{id:SourceId(id),label:format!("Source {id}"),root:PathBuf::new(),nfs_remote:None,sftp:None});}
                }
                Page::Collections=>{
                    section(ui,"Optional collections");ui.label("Combine selected folders visually. Nothing is merged or renamed on disk.");let mut remove=None;
                    for collection in &mut self.draft.collections{ui.push_id(collection.id,|ui|{
                        ui.separator();field(ui,"Collection name",&mut collection.label,128);
                        let members=self.members.entry(collection.id).or_default();
                        for source in &self.draft.sources{ui.push_id(source.id,|ui|{
                            let mut included=members.iter().any(|(id,_)|*id==source.id);
                            let check=ui.add_enabled_ui(included||members.len()<32,|ui|ui.checkbox(&mut included,format!("Include {}",source.label))).inner;
                            if check.changed(){if included{members.push((source.id,String::new()));}else{members.retain(|(id,_)|*id!=source.id);}}
                            if included {
                                let mut remove_member=None;
                                for (index,(id,path)) in members.iter_mut().enumerate().filter(|(_, (id,_))|*id==source.id){
                                    ui.push_id((id,index),|ui|{
                                        field(ui,"Relative folder",path,16384);ui.small("Empty means the source root. Otherwise use a source-relative folder path.");
                                        if ui.button("Remove this folder").clicked(){remove_member=Some(index);}
                                    });
                                }
                                if let Some(index)=remove_member{members.remove(index);}
                                if ui.add_enabled(members.len()<32,egui::Button::new("Add another folder from this source")).clicked(){members.push((source.id,String::new()));}
                            }
                        });}
                        if ui.button("Remove collection").clicked(){remove=Some(collection.id);}
                    });}
                    if let Some(id)=remove{self.members.remove(&id);self.draft.collections.retain(|c|c.id!=id);}
                    if self.draft.collections.len()<16&&ui.button("Add collection").clicked(){let id=self.draft.collections.iter().map(|c|c.id).max().unwrap_or(0).saturating_add(1);self.members.insert(id,Vec::new());self.draft.collections.push(Collection{id,label:"Collection".into(),members:Vec::new()});}
                }
                Page::Applications=>{
                    section(ui,"Open files with");ui.label("Enter an executable name or absolute path, not a shell command.");ui.small("Archiv supplies -- and the exact physical filename as separate arguments. Applications use your normal user permissions.");
                    field(ui,"Image viewer",&mut self.draft.openers.images,4096);ui.small("For example: imv");field(ui,"Media player",&mut self.draft.openers.media,4096);ui.small("For example: mpv");field(ui,"Text editor",&mut self.draft.openers.text,4096);ui.small("For example: kwrite");field(ui,"Other files",&mut self.draft.openers.other,4096);ui.small("Leave empty to require an explicit application configuration.");
                }
                Page::Safeguards=>{
                    ui.horizontal(|ui|{ui.add(widgets::icon("shield",18.0,t.accent));section(ui,"File changes and protected folders");});ui.label("Delete, rename and pasted copies are disabled until explicitly allowed for a physical source.");ui.small("These safeguards apply to Archiv only. Other apps, SSH users and writable NFS clients have their own access. Source roots and collection roots are always protected. Delete is permanent and cannot be undone.");
                    for source in &self.draft.sources{ui.push_id(source.id,|ui|{
                        ui.separator();ui.strong(&source.label);
                        if source.sftp.is_some(){ui.label("SFTP · browsing only. File changes are unavailable.");return;}ui.label(source.root.to_string_lossy());
                        let mut enabled=self.draft.writes.iter().any(|p|p.source==source.id&&p.enabled);
                        if ui.checkbox(&mut enabled,"Allow permanent delete, rename and pasted copies").changed(){if let Some(policy)=self.draft.writes.iter_mut().find(|p|p.source==source.id){policy.enabled=enabled;}else{self.draft.writes.push(WritePolicy{source:source.id,enabled,allowed:Vec::new(),protected:Vec::new()});}}
                        ui.label("Allowed relative folders (one per line)");multiline(ui,"Allowed folders",self.allowed.entry(source.id).or_default());ui.small("Use . to allow the source subtree; the source root itself stays protected.");
                        ui.label("Protect folder itself (children may be changed)");multiline(ui,"Protected folders",self.protected.entry(source.id).or_default());
                        ui.label("Protect entire subtrees");multiline(ui,"Protected subtrees",self.subtrees.entry(source.id).or_default());
                        ui.small("Protecting a folder also prevents removing any ancestor that contains it. Renames and pasted copies also obey the destination protections.");
                    });}
                }
                Page::Appearance=>{}
            });}
        });
        if let Some(error) = &self.error {
            ui.label(error);
        }
        if let Some(status) = status {
            ui.label(status);
        }
        if self.page != Page::Appearance
            && ui.add_enabled_ui(!pending, |ui| widgets::pill(ui, "Save library", true, t)).inner.clicked()
        {
            command = self.save_command();
        }
        command
    }
    pub fn save_command(&mut self) -> Option<Command> {
        let mut draft = self.draft.clone();
        let result = (|| {
            for c in &mut draft.collections {
                c.members = self
                    .members
                    .get(&c.id)
                    .into_iter()
                    .flatten()
                    .map(|(source, path)| {
                        Ok(Location { source: *source, path: RelativePath::new(path).map_err(|e| e.to_string())? })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
            }
            for policy in &mut draft.writes {
                policy.allowed = parse_lines(self.allowed.get(&policy.source).map_or("", String::as_str), true)?;
                policy.protected = parse_lines(self.protected.get(&policy.source).map_or("", String::as_str), false)?
                    .into_iter()
                    .map(|path| ProtectedPath { path, subtree: false })
                    .chain(
                        parse_lines(self.subtrees.get(&policy.source).map_or("", String::as_str), false)?
                            .into_iter()
                            .map(|path| ProtectedPath { path, subtree: true }),
                    )
                    .collect();
            }
            draft.validate()
        })();
        match result {
            Ok(()) => {
                self.error = None;
                return Some(Command::SaveLibrary(draft));
            }
            Err(error) => self.error = Some(error),
        }
        None
    }
}
fn parse_lines(text: &str, root: bool) -> Result<Vec<RelativePath>, String> {
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| RelativePath::new(if root && line == "." { "" } else { line }).map_err(|e| e.to_string()))
        .collect()
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String, limit: usize) -> bool {
    ui.label(format!("{label}:"));
    ui.scope(|ui| {
        let visuals = &mut ui.style_mut().visuals;
        for widget in [&mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active] {
            widget.corner_radius = egui::CornerRadius::same(4);
        }
        ui.add(widgets::text_field(value).char_limit(limit).desired_width(f32::INFINITY))
            .accessible_name(label)
            .changed()
    })
    .inner
}

fn multiline(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.add(egui::TextEdit::multiline(value).desired_rows(2).char_limit(16384).desired_width(f32::INFINITY))
        .accessible_name(label);
}

fn section(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(crate::fonts::semibold(13.0)));
}
fn appearance(ui: &mut egui::Ui, preferences: &mut Preferences, t: &Tokens) {
    section(ui, "Appearance");
    ui.horizontal(|ui| {
        ui.label("Appearance mode");
        egui::ComboBox::from_id_salt("preferences-theme")
            .selected_text(preferences.theme.label())
            .show_ui(ui, |ui| {
                for theme in [ThemePreference::System, ThemePreference::Light, ThemePreference::Dark] {
                    ui.selectable_value(&mut preferences.theme, theme, theme.label());
                }
            })
            .response
            .accessible_name("Appearance mode");
    });
    ui.add_space(6.0);
    let width = ((ui.available_width() - 28.0) / 2.0).max(100.0);
    ui.horizontal(|ui| {
        for theme in [ThemePreference::Light, ThemePreference::Dark] {
            let active = preferences.theme.dark(ui.ctx().system_theme()) == (theme == ThemePreference::Dark);
            let preview = Tokens::new(theme == ThemePreference::Dark);
            let card = egui::Frame::new()
                .fill(t.card)
                .stroke(egui::Stroke::new(1.0, if active { t.accent } else { t.border }))
                .corner_radius(6)
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{} theme", theme.label()))
                                    .font(crate::fonts::semibold(12.5)),
                            );
                            if active {
                                ui.label(egui::RichText::new("Active").small().color(t.accent_text));
                            }
                        });
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 100.0), egui::Sense::hover());
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 3, preview.workspace);
                        painter.rect_filled(
                            egui::Rect::from_min_size(rect.min, egui::vec2(width, 12.0)),
                            0,
                            preview.titlebar,
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 12.0), egui::vec2(width, 10.0)),
                            0,
                            preview.panel,
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 23.0), egui::vec2(width * 0.19, 77.0)),
                            0,
                            preview.panel,
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                rect.min + egui::vec2(width * 0.82, 23.0),
                                egui::vec2(width * 0.18, 77.0),
                            ),
                            0,
                            preview.panel,
                        );
                        for row in 0..4 {
                            let y = 32.0 + (row as f32) * 16.0;
                            if row == 1 {
                                painter.rect_filled(
                                    egui::Rect::from_min_size(
                                        rect.min + egui::vec2(width * 0.23, y - 3.0),
                                        egui::vec2(width * 0.55, 12.0),
                                    ),
                                    3,
                                    preview.selected,
                                );
                            }
                            painter.rect_filled(
                                egui::Rect::from_min_size(rect.min + egui::vec2(width * 0.25, y), egui::vec2(6.0, 5.0)),
                                1,
                                if row < 2 { preview.folder } else { preview.document },
                            );
                            painter.rect_filled(
                                egui::Rect::from_min_size(
                                    rect.min + egui::vec2(width * 0.31, y + 1.0),
                                    egui::vec2(width * 0.35, 2.0),
                                ),
                                1,
                                preview.muted,
                            );
                        }
                        ui.label(
                            egui::RichText::new(if active { "Selected" } else { "Click to select" })
                                .small()
                                .color(t.muted),
                        );
                    });
                });
            let response = card.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    ui.is_enabled(),
                    format!("Choose {} theme", theme.label()),
                )
            });
            if response.has_focus() {
                ui.painter().rect_stroke(
                    response.rect.expand(2.0),
                    8,
                    egui::Stroke::new(1.5, t.accent),
                    egui::StrokeKind::Outside,
                );
            }
            if response.clicked() {
                preferences.theme = theme;
                ui.ctx().request_repaint();
            }
        }
    });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label("Interface size");
        egui::ComboBox::from_id_salt("interface-scale")
            .selected_text(preferences.scale.map_or_else(|| "Auto".into(), |s| format!("{s}%")))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut preferences.scale, None, "Auto");
                for scale in [75, 100, 125, 150, 200] {
                    ui.selectable_value(&mut preferences.scale, Some(scale), format!("{scale}%"));
                }
            });
    });
    ui.small("Auto follows display scaling. Manual percentages multiply the native desktop scale.");
    ui.checkbox(&mut preferences.reduced_motion, "Reduce motion");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_preserves_multiple_roots_from_one_source_and_root_allowlist() {
        let config = LibrarySettings {
            sources: vec![Source {
                id: SourceId(1),
                label: "Archive".into(),
                root: "/synthetic/archive".into(),
                nfs_remote: None,
                sftp: None,
            }],
            collections: vec![Collection {
                id: 1,
                label: "Documents".into(),
                members: vec![
                    Location { source: SourceId(1), path: RelativePath::new("one").unwrap() },
                    Location { source: SourceId(1), path: RelativePath::new("two").unwrap() },
                ],
            }],
            writes: vec![WritePolicy {
                source: SourceId(1),
                enabled: true,
                allowed: vec![RelativePath::default()],
                protected: Vec::new(),
            }],
            ..Default::default()
        };
        let mut editor = Editor::new(config.clone());
        let Some(Command::SaveLibrary(saved)) = editor.save_command() else {
            panic!("Valid multiple-root settings could not be saved")
        };
        assert_eq!(saved, config);
    }
}
