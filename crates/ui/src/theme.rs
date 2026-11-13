//! egui drawn in the theme (#158, swarm-core#100's `theme::apply`): each of egui's looks gets
//! visuals whose colours are the theme's tokens ([`swarmception_domain::theme`]: the palette the
//! swarm's tools share), so the window, the Factory's cards and cells and the timeline widget's
//! background are the swarm's other tools' colours, not egui's greys. Everything that reads egui's
//! visuals (the Factory's `Palette`, the timeline widget's `Theme::from_visuals`) follows.

use egui::{Color32, Stroke, Visuals};
use swarmception_domain::theme::{self, Look, Rgb, Token};

/// egui's visuals in `look`: egui's own look for it, its colours the theme's tokens.
#[must_use]
pub fn visuals(look: Look) -> Visuals {
    let token = |t: Token| colour(t.of(look));
    let mut v = match look {
        Look::Dark => Visuals::dark(),
        Look::Light => Visuals::light(),
    };
    v.panel_fill = token(theme::BACKGROUND);
    v.window_fill = token(theme::PANEL);
    v.extreme_bg_color = token(theme::BASE);
    v.code_bg_color = token(theme::BASE);
    v.faint_bg_color = token(theme::ALT_BASE);
    v.window_stroke = Stroke::new(1.0, token(theme::BORDER));
    v.hyperlink_color = token(theme::LINK);
    v.selection.bg_fill = token(theme::SELECTION);
    v.selection.stroke = Stroke::new(1.0, token(theme::SELECTED_TEXT));
    v.warn_fg_color = token(theme::WARNING);
    v.error_fg_color = token(theme::ERROR);
    v.override_text_color = None;
    v.weak_text_color = Some(token(theme::MUTED));
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.open] {
        w.fg_stroke.color = token(theme::TEXT);
    }
    // egui's strong text is its active widgets' text.
    v.widgets.active.fg_stroke.color = token(theme::STRONG);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, token(theme::BORDER));
    // egui draws resting widgets with no edge and a fill barely off the panel.
    v.widgets.inactive.bg_fill = token(theme::CONTROL);
    v.widgets.inactive.weak_bg_fill = token(theme::CONTROL);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, token(theme::CONTROL_BORDER));
    v.widgets.open.bg_stroke = Stroke::new(1.0, token(theme::CONTROL_BORDER));
    v
}

/// Styles `ctx` in the theme: each of egui's looks gets [`visuals`] for it, and egui keeps choosing
/// between them (the system's, or `--dark` / `--light`). Called once, when the window starts.
pub fn apply(ctx: &egui::Context) {
    for (appearance, look) in [(egui::Theme::Dark, Look::Dark), (egui::Theme::Light, Look::Light)] {
        let visuals = visuals(look);
        ctx.style_mut_of(appearance, |style| style.visuals = visuals);
    }
}

/// The edge a filled colour (a brick in its ticket's colour, a red corner) is outlined in: the
/// theme's [`theme::darker`], each channel × 0.8, the shared palette's border for a filled entry.
#[must_use]
pub fn border(fill: Color32) -> Color32 {
    colour(theme::darker(Rgb::new(fill.r(), fill.g(), fill.b())))
}

const fn colour(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.r, rgb.g, rgb.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **egui draws in the theme** (#158): once applied, each of egui's looks has the tokens for
    /// its surfaces, text, selection, widgets and states.
    #[test]
    fn egui_draws_with_the_tokens_once_applied() {
        let ctx = egui::Context::default();
        apply(&ctx);
        for (appearance, look) in [(egui::Theme::Dark, Look::Dark), (egui::Theme::Light, Look::Light)] {
            let v = ctx.style_of(appearance).visuals.clone();
            assert_eq!(v.dark_mode, look == Look::Dark, "{look:?}: egui's own look underneath");
            let t = |token: Token| colour(token.of(look));
            let drawn = [
                ("background", v.panel_fill, t(theme::BACKGROUND)),
                ("panel", v.window_fill, t(theme::PANEL)),
                ("base", v.extreme_bg_color, t(theme::BASE)),
                ("alt_base", v.faint_bg_color, t(theme::ALT_BASE)),
                ("border", v.window_stroke.color, t(theme::BORDER)),
                ("text", v.text_color(), t(theme::TEXT)),
                ("strong", v.strong_text_color(), t(theme::STRONG)),
                ("muted", v.weak_text_color(), t(theme::MUTED)),
                ("link", v.hyperlink_color, t(theme::LINK)),
                ("selection", v.selection.bg_fill, t(theme::SELECTION)),
                ("selected_text", v.selection.stroke.color, t(theme::SELECTED_TEXT)),
                ("control", v.widgets.inactive.bg_fill, t(theme::CONTROL)),
                ("control_border", v.widgets.inactive.bg_stroke.color, t(theme::CONTROL_BORDER)),
                ("warning", v.warn_fg_color, t(theme::WARNING)),
                ("error", v.error_fg_color, t(theme::ERROR)),
            ];
            for (name, egui, ours) in drawn {
                assert_eq!(egui, ours, "{look:?} {name}");
            }
        }
    }

    #[test]
    fn a_filled_colours_border_is_the_themes_darker() {
        assert_eq!(border(Color32::from_rgb(0xFF, 0x7F, 0x1C)), Color32::from_rgb(0xCC, 0x65, 0x16), "the shared palette's darker #FF7F1C");
    }
}
