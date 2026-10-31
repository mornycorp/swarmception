//! The fonts: egui's own, with Font Awesome's solid face behind each family, so an icon's glyph
//! draws inside any label or button.

/// Font Awesome Free 5.15.4, solid (assets/README.md).
const FONT_AWESOME: &[u8] = include_bytes!("../assets/fa-solid-900.ttf");
const NAME: &str = "fa-solid-900";

/// egui's fonts with Font Awesome as the last fallback of every family.
#[must_use]
pub fn definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(NAME.to_owned(), std::sync::Arc::new(egui::FontData::from_static(FONT_AWESOME)));
    for family in fonts.families.values_mut() {
        family.push(NAME.to_owned());
    }
    fonts
}

/// Installs [`definitions`] in `ctx`.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_family_falls_back_to_font_awesome() {
        let fonts = definitions();
        assert!(fonts.font_data.contains_key(NAME));
        assert!(fonts.families.values().all(|f| f.last().map(String::as_str) == Some(NAME)));
    }
}
