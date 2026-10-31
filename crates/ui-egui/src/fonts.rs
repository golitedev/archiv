//! Verified compile-time typography. No runtime filesystem access or downloads.
use egui::{FontData, FontDefinitions, FontFamily, FontId};
include!(concat!(env!("OUT_DIR"), "/font_data.rs"));
pub fn install(ctx: &egui::Context) {
    let installed = egui::Id::unique("archiv-fonts-installed");
    if ctx.data(|data| data.get_temp::<bool>(installed)).unwrap_or(false) {
        return;
    }
    let mut fonts = FontDefinitions::default();
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (name, bytes) in
        [("archiv-regular", INTER_REGULAR), ("archiv-medium", INTER_MEDIUM), ("archiv-semibold", INTER_SEMIBOLD)]
    {
        let mut stack = fallback.clone();
        if let Some(bytes) = bytes {
            fonts.font_data.insert(name.into(), FontData::from_static(bytes).into());
            stack.insert(0, name.into());
        }
        if name == "archiv-regular" {
            fonts.families.insert(FontFamily::Proportional, stack);
        } else {
            fonts.families.insert(FontFamily::Name(name.into()), stack);
        }
    }
    if let Some(bytes) = MONO {
        fonts.font_data.insert("archiv-mono".into(), FontData::from_static(bytes).into());
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "archiv-mono".into());
    }
    ctx.set_fonts(fonts);
    ctx.data_mut(|data| data.insert_temp(installed, true));
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("archiv-medium".into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("archiv-semibold".into()))
}
