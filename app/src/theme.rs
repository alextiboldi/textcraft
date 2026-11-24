//! The theme: colours, icon buttons, fonts and the logo.

use craft_ui::buttons::{ButtonVisuals, IconButton, IconButtonStyle};
use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, Vec2};
use lucide_icons::Icon;
use std::sync::Arc;
use textcraft_core::Tok;

// ---------- palette: chosen when TextCraft was first generated, before any picture was available; not sampled from a screen ----------
pub const BG: Color32 = Color32::from_rgb(0x1e, 0x21, 0x27);
pub const BG_SIDE: Color32 = Color32::from_rgb(0x17, 0x19, 0x1e);
pub const BG_TABBAR: Color32 = Color32::from_rgb(0x14, 0x16, 0x1a);
pub const BG_LINE: Color32 = Color32::from_rgb(0x26, 0x2a, 0x32);
pub const FG: Color32 = Color32::from_rgb(0xd4, 0xd7, 0xdd);
pub const FG_DIM: Color32 = Color32::from_rgb(0x6b, 0x72, 0x80);
pub const ACCENT: Color32 = Color32::from_rgb(0xf2, 0xa6, 0x3b);
pub const C_KEYWORD: Color32 = Color32::from_rgb(0xe0, 0x6c, 0x9f);
pub const C_STRING: Color32 = Color32::from_rgb(0xb5, 0xd3, 0x6b);
pub const C_NUMBER: Color32 = Color32::from_rgb(0xc3, 0x9a, 0xf2);
pub const C_COMMENT: Color32 = Color32::from_rgb(0x6f, 0x77, 0x86);
pub const C_FUNC: Color32 = Color32::from_rgb(0x6c, 0xc4, 0xe8);
pub const C_TYPE: Color32 = Color32::from_rgb(0xf0, 0xc6, 0x74);
pub const C_MATCH: Color32 = Color32::from_rgba_premultiplied(0x60, 0x50, 0x10, 0xa0);

pub fn icon(i: Icon) -> String {
    i.unicode().to_string()
}

/// Frameless icon buttons (tab close, new tab, side bar toggle).
pub const PLAIN_ICON: IconButtonStyle = {
    let normal = ButtonVisuals { fill: Color32::TRANSPARENT, stroke: Stroke::NONE, icon: FG_DIM };
    IconButtonStyle {
        normal,
        hovered: ButtonVisuals { fill: BG_LINE, icon: FG, ..normal },
        pressed: None,
        selected: ButtonVisuals { icon: FG, ..normal },
        corner_radius: egui::CornerRadius::same(4),
        focus_stroke: Stroke { width: 1.0, color: FG },
    }
};

/// Icon buttons framed like egui's own buttons, for rows of framed buttons (find bar).
pub fn framed_icon(ui: &egui::Ui) -> IconButtonStyle {
    let (w, focus) = (&ui.visuals().widgets, ui.visuals().selection.stroke);
    let v = |s: &egui::style::WidgetVisuals| ButtonVisuals { fill: s.weak_bg_fill, stroke: s.bg_stroke, icon: s.fg_stroke.color };
    IconButtonStyle {
        normal: v(&w.inactive),
        hovered: v(&w.hovered),
        pressed: Some(v(&w.active)),
        selected: v(&w.active),
        corner_radius: w.inactive.corner_radius,
        focus_stroke: focus,
    }
}

/// An icon-only button: `glyph` (a Lucide font glyph) drawn centred, `label` for screen readers.
pub fn icon_button(ui: &mut egui::Ui, style: &IconButtonStyle, label: &str, glyph: Icon, size: f32) -> egui::Response {
    let side = ui.spacing().interact_size.y;
    IconButton::new(label, Vec2::splat(side), style).show(ui, |ui, rect, tint| {
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon(glyph), FontId::proportional(size), tint);
    })
}

pub fn tok_color(t: Tok) -> Color32 {
    match t {
        Tok::Plain => FG,
        Tok::Keyword => C_KEYWORD,
        Tok::Str => C_STRING,
        Tok::Num => C_NUMBER,
        Tok::Comment => C_COMMENT,
        Tok::Func => C_FUNC,
        Tok::Type => C_TYPE,
    }
}

pub fn setup_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("lucide".into(), Arc::new(egui::FontData::from_static(lucide_icons::LUCIDE_FONT_BYTES)));
    for fam in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().push("lucide".into());
    }
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|s| {
        s.visuals = egui::Visuals::dark();
        s.visuals.panel_fill = BG_SIDE;
        s.visuals.extreme_bg_color = BG_TABBAR;
        s.visuals.selection.bg_fill = Color32::from_rgb(0x3a, 0x4a, 0x66);
        s.visuals.selection.stroke = Stroke::new(1.0, FG);
        s.visuals.text_cursor.stroke = Stroke::new(2.0, ACCENT);
        s.visuals.override_text_color = None;
        s.spacing.item_spacing = Vec2::new(6.0, 4.0);
        s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BG_LINE);
    });
}

/// The TextCraft logo (`assets/logo.webp`), as `size`×`size` RGBA pixels. `None` if it can't be
/// decoded; the app then runs without a logo rather than failing.
pub fn logo_rgba(size: u32) -> Option<Vec<u8>> {
    let img = image::load_from_memory_with_format(include_bytes!("../assets/logo.webp"), image::ImageFormat::WebP).ok()?;
    Some(image::imageops::resize(&img.to_rgba8(), size, size, image::imageops::FilterType::Lanczos3).into_raw())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_decodes() {
        // Guards the embedded file and the `image` crate's `webp` feature together.
        assert!(logo_rgba(64).is_some_and(|px| px.len() == 64 * 64 * 4));
    }
}
