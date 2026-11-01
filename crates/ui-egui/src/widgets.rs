//! Shared native controls, licensed vector icons and semantic file-type colors.
use crate::theme::Tokens;
use archiv_core::{Entry, EntryKind};
use egui::{Color32, Response, RichText};
/// Shared single-line fields keep their text/caret centered when the layout adds height.
pub fn text_field(value: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(value)
        .align(egui::Align2::LEFT_CENTER)
        .min_size(egui::vec2(0.0, 30.0))
        .margin(egui::Margin::symmetric(10, 6))
}
pub fn pill(ui: &mut egui::Ui, text: &str, primary: bool, t: &Tokens) -> Response {
    let background = ui.painter().add(egui::Shape::Noop);
    let button = egui::Button::new(RichText::new(text).color(if primary { Color32::WHITE } else { t.text }))
        .corner_radius(14)
        .fill(Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE);
    let response = ui.add(button);
    let duration = ui.style().animation_time;
    let hover = ui.ctx().animate_bool_with_time_and_easing(
        response.id.with("pill-hover"),
        response.hovered(),
        duration,
        egui::emath::easing::quadratic_out,
    );
    let down = ui.ctx().animate_bool_with_time_and_easing(
        response.id.with("pill-press"),
        response.is_pointer_button_down_on(),
        duration * 0.5,
        egui::emath::easing::quadratic_out,
    );
    let (rest, hot, pressed) = if primary {
        (t.accent, t.accent.gamma_multiply(1.08), t.accent.gamma_multiply(0.88))
    } else {
        (t.card, t.hover, t.pressed)
    };
    let fill = rest.lerp_to_gamma(hot, hover).lerp_to_gamma(pressed, down);
    let fill = if ui.is_enabled() { fill } else { fill.gamma_multiply(0.5) };
    ui.painter().set(background, egui::Shape::rect_filled(response.rect, 14, fill));
    if !primary {
        ui.painter().rect_stroke(response.rect, 14, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            16,
            egui::Stroke::new(1.5, t.accent),
            egui::StrokeKind::Outside,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
pub fn navigation_button(ui: &mut egui::Ui, name: &str, icon_name: &str, enabled: bool, t: &Tokens) -> Response {
    let response = ui
        .add_enabled(
            enabled,
            egui::Button::image(icon(icon_name, 17.0, t.text)).frame(false).min_size(egui::vec2(30.0, 30.0)),
        )
        .accessible_name(name)
        .on_hover_text(if name == "Back" { "Back · Alt+Left" } else { "Forward · Alt+Right" })
        .on_disabled_hover_text(if name == "Back" { "No previous location" } else { "No next location" });
    if response.has_focus() {
        ui.painter().rect_stroke(response.rect, 6, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    response
}
pub fn card(t: &Tokens) -> egui::Frame {
    egui::Frame::new().fill(t.card).stroke(egui::Stroke::new(1.0, t.border)).corner_radius(12).inner_margin(18)
}
pub fn icon(name: &str, size: f32, tint: Color32) -> egui::Image<'static> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static ICONS: OnceLock<HashMap<&'static str, egui::ImageSource<'static>>> = OnceLock::new();
    let icons = ICONS.get_or_init(|| {
        macro_rules! asset {
            ($file:literal) => {
                include_str!(concat!("../../../assets/icons/", $file, ".svg"))
            };
        }
        [
            ("folder", asset!("folder")),
            ("folder-open", asset!("folder-open")),
            ("text", asset!("file-text")),
            ("image", asset!("image")),
            ("media", asset!("film")),
            ("music", asset!("file-text")),
            ("right", asset!("chevron-right")),
            ("left", asset!("chevron-left")),
            ("down", asset!("chevron-down")),
            ("search", asset!("search")),
            ("source", asset!("folder")),
            ("collection", asset!("layers")),
            ("shield", asset!("shield-check")),
            ("file", asset!("file-text")),
        ]
        .into_iter()
        .map(|(name, svg)| {
            let svg = svg.replace("currentColor", "#ffffff").replace("stroke-width=\"2\"", "stroke-width=\"1.75\"");
            (
                name,
                egui::ImageSource::Bytes {
                    uri: format!("bytes://archiv/{name}.svg").into(),
                    bytes: svg.into_bytes().into(),
                },
            )
        })
        .collect()
    });
    let source = icons
        .get(name)
        .or_else(|| icons.get("file"))
        .cloned()
        .unwrap_or(egui::ImageSource::Uri("bytes://missing-icon".into()));
    egui::Image::new(source).fit_to_exact_size(egui::vec2(size, size)).tint(tint)
}
pub fn file_icon(entry: &Entry, open: bool, t: &Tokens) -> (&'static str, Color32) {
    if entry.kind == EntryKind::Directory {
        return (if open { "folder-open" } else { "folder" }, t.folder);
    }
    let extension = entry.id.path.as_path().extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "svg" | "webp" | "gif" | "avif" => ("image", t.image),
        "mp4" | "mkv" | "webm" | "mov" | "avi" => ("media", t.media),
        "mp3" | "flac" | "ogg" | "wav" => ("music", t.media),
        "txt" | "md" | "rs" | "toml" | "json" | "srt" | "log" | "yaml" | "conf" => ("text", t.document),
        _ => ("file", t.muted),
    }
}

pub fn cell(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &str,
    color: Color32,
    small: bool,
) -> Response {
    let response = ui.interact(rect, ui.scope_id().with(id), egui::Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), text));
    ui.painter().with_clip_rect(rect).text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(if small { 11.0 } else { 13.0 }),
        color,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn single_line_text_stays_centered_in_vertical_and_horizontal_layouts() {
        for scale in [1.0, 1.25, 2.0] {
            let ctx = egui::Context::default();
            crate::fonts::install(&ctx);
            ctx.set_pixels_per_point(scale);
            for horizontal in [false, true] {
                for height in [30.0, 42.0] {
                    let mut value = "Synthetic path gyp".to_owned();
                    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                        crate::theme::apply(ui.ctx(), crate::ThemePreference::Dark, true);
                        let layout = if horizontal { egui::Layout::left_to_right(egui::Align::Center) }
                            else { egui::Layout::top_down(egui::Align::Min) };
                        ui.with_layout(layout, |ui| {
                            let output = text_field(&mut value).min_size(egui::vec2(0.0, height)).show(ui);
                            let text_center = output.galley_pos.y + output.galley.size().y * 0.5;
                            let field_center = output.response.rect.center().y;
                            assert!((text_center - field_center).abs() <= 0.6 / scale,
                                "scale {scale}, horizontal {horizontal}, height {height}: text {text_center}, field {field_center}");
                        });
                    });
                    output.textures_delta.clear();
                }
            }
        }
    }
}
