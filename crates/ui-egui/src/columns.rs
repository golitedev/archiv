//! Presentation-only column geometry; adjacent columns share each header divider.
use crate::{theme::Tokens, widgets};
use egui::{Key, Sense};

#[derive(Default)]
pub(crate) struct Columns {
    widths: Option<[f32; 4]>,
    drag: Option<(usize, f32, [f32; 4])>,
}
impl Columns {
    pub fn header(&mut self, ui: &mut egui::Ui, width: f32, search: bool, t: &Tokens) -> [f32; 4] {
        let mut widths = self.widths.unwrap_or_else(|| {
            let location = if search { 200.0 } else { 96.0 };
            [(width - 141.0 - location).max(90.0), 63.0, 78.0, location]
        });
        // Window/panel resizing changes Name first; user-sized metadata columns stay stable.
        widths[0] = (widths[0] + width - widths.iter().sum::<f32>()).max(90.0);
        let total = widths.iter().sum::<f32>();
        if total > width {
            // Very small windows keep all columns inside the workspace.
            for cell in &mut widths {
                *cell *= width.max(0.0) / total;
            }
        }
        let (header, _) = ui.allocate_exact_size(egui::vec2(width, 26.0), Sense::hover());
        let mut offset = 0.0;
        for (index, label) in ["NAME", "KIND", "SIZE", "LOCATION"].into_iter().enumerate() {
            let cell = egui::Rect::from_min_size(header.min + egui::vec2(offset, 0.0), egui::vec2(widths[index], 26.0));
            widgets::cell(ui, cell.shrink2(egui::vec2(4.0, 0.0)), ("header", label), label, t.muted, true);
            offset += widths[index];
        }
        let mut boundary = header.left();
        for index in 0..3 {
            boundary += widths[index];
            let handle = egui::Rect::from_center_size(egui::pos2(boundary, header.center().y), egui::vec2(8.0, 26.0));
            let response = ui
                .interact(handle, ui.scope_id().with(("column-divider", index)), Sense::click_and_drag())
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            let name =
                ["Resize Name and Kind columns", "Resize Kind and Size columns", "Resize Size and Location columns"]
                    [index];
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Splitter, true, name));
            let color =
                if response.hovered() || response.dragged() || response.has_focus() { t.accent } else { t.border };
            ui.painter().line_segment(
                [egui::pos2(boundary, header.top() + 3.0), egui::pos2(boundary, header.bottom() - 3.0)],
                egui::Stroke::new(1.0, color),
            );
            if response.dragged()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let (_, anchor, initial) = *self.drag.get_or_insert_with(|| {
                    (index, pointer.x - response.total_drag_delta().map_or(0.0, |delta| delta.x), widths)
                });
                // Absolute origin, not an accumulated frame delta: layout replay is harmless.
                widths = resized(initial, index, pointer.x - anchor);
                ui.ctx().request_repaint();
            }
            if response.has_focus() {
                let delta = ui.input_mut(|input| {
                    if input.consume_key(egui::Modifiers::NONE, Key::ArrowLeft) {
                        -8.0
                    } else if input.consume_key(egui::Modifiers::NONE, Key::ArrowRight) {
                        8.0
                    } else {
                        0.0
                    }
                });
                if delta != 0.0 {
                    widths = resized(widths, index, delta);
                    ui.ctx().request_repaint();
                }
            }
        }
        if !ui.input(|input| input.pointer.primary_down()) {
            self.drag = None;
        }
        self.widths = Some(widths);
        widths
    }
}
fn resized(mut widths: [f32; 4], index: usize, delta: f32) -> [f32; 4] {
    if index >= 3 {
        return widths;
    }
    let minimum = [90.0, 48.0, 48.0, 60.0];
    let low = (minimum[index] - widths[index]).min(0.0);
    let high = (widths[index + 1] - minimum[index + 1]).max(0.0);
    let delta = delta.clamp(low, high);
    widths[index] += delta;
    widths[index + 1] -= delta;
    widths
}
