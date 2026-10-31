//! Centralized Archiv presentation tokens and preferences.

use egui::{Color32, CornerRadius, FontId, Stroke};
use serde::{Deserialize, Serialize};

/// Saved desktop appearance preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemePreference {
    /// Follow the desktop; use light if no preference is available.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}
impl ThemePreference {
    /// Human-readable preference name.
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
    /// Resolve the preference against the last known desktop theme.
    pub fn dark(self, system: Option<egui::Theme>) -> bool {
        match self {
            Self::System => system == Some(egui::Theme::Dark),
            Self::Light => false,
            Self::Dark => true,
        }
    }
}

pub struct Tokens {
    pub panel: Color32,
    pub workspace: Color32,
    pub card: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub selected: Color32,
    pub accent: Color32,
    pub field: Color32,
}
impl Tokens {
    pub fn new(dark: bool) -> Self {
        let rgb = Color32::from_rgb;
        if dark {
            Self {
                panel: rgb(38, 38, 42),
                workspace: rgb(25, 25, 28),
                card: rgb(46, 46, 51),
                border: rgb(60, 60, 67),
                text: rgb(236, 236, 239),
                muted: rgb(174, 174, 182),
                hover: rgb(52, 52, 58),
                pressed: rgb(62, 62, 69),
                selected: rgb(36, 54, 87),
                accent: rgb(75, 139, 245),
                field: rgb(30, 30, 34),
            }
        } else {
            Self {
                panel: Color32::WHITE,
                workspace: rgb(241, 241, 243),
                card: Color32::WHITE,
                border: rgb(218, 218, 222),
                text: rgb(34, 34, 38),
                muted: rgb(94, 94, 102),
                hover: rgb(240, 240, 243),
                pressed: rgb(228, 228, 233),
                selected: rgb(227, 236, 253),
                accent: rgb(27, 99, 224),
                field: Color32::WHITE,
            }
        }
    }
}

pub fn apply(ctx: &egui::Context, preference: ThemePreference, reduced_motion: bool) -> Tokens {
    let dark = preference.dark(ctx.system_theme());
    let t = Tokens::new(dark);
    let mut visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    visuals.panel_fill = t.panel;
    visuals.window_fill = t.card;
    visuals.window_stroke = Stroke::new(1.0, t.border);
    visuals.extreme_bg_color = t.field;
    visuals.faint_bg_color = t.hover;
    visuals.override_text_color = Some(t.text);
    visuals.weak_text_color = Some(t.muted);
    visuals.selection.bg_fill = t.selected;
    visuals.selection.stroke = Stroke::new(1.5, t.accent);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 38 }),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 90 } else { 30 }),
    };
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::same(6);
        widget.fg_stroke = Stroke::new(1.0, t.text);
    }
    visuals.widgets.inactive.bg_fill = t.field;
    visuals.widgets.inactive.weak_bg_fill = t.field;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, t.border);
    visuals.widgets.hovered.bg_fill = t.hover;
    visuals.widgets.hovered.weak_bg_fill = t.hover;
    visuals.widgets.active.bg_fill = t.pressed;
    visuals.widgets.active.weak_bg_fill = t.pressed;
    // Keep framework theme branches synchronized for popups while retaining system notifications.
    ctx.set_theme(egui::ThemePreference::System);
    for branch in [egui::Theme::Light, egui::Theme::Dark] {
        ctx.style_mut_of(branch, |style| {
            style.visuals = visuals.clone();
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 5.0);
            style.spacing.interact_size.y = 30.0;
            style.spacing.menu_margin = egui::Margin::same(6);
            style.spacing.scroll.bar_width = 8.0;
            style.spacing.scroll.floating = true;
            style.spacing.scroll.floating_width = 3.0;
            style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.0));
            style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.0));
            style.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.0));
            style.text_styles.insert(egui::TextStyle::Heading, FontId::proportional(17.0));
            style.interaction.tooltip_delay = 0.35;
            style.animation_time = if reduced_motion { 0.0 } else { 0.12 };
        });
    }
    t
}
