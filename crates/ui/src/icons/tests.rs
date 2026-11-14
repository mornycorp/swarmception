use super::*;
use egui::IntoAtoms as _;

/// Every named icon is in the embedded Font Awesome face, by its own character map (in a family of
/// one face every character resolves to that face, so egui's `has_glyph` can't say).
#[test]
fn every_icon_is_in_the_embedded_face() {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    let has = |c: char| ctx.fonts_mut(|f| f.characters(&fonts::icons()).contains_key(&c));
    let missing: Vec<String> = ALL.iter().filter(|&&icon| !has(icon)).map(|icon| format!("U+{:04X}", u32::from(*icon))).collect();
    assert!(missing.is_empty(), "not in fa-solid-900.ttf: {missing:?}");
    assert!(!has('\u{1e9e}'), "a character it lacks is not found");
    let mut sorted = ALL;
    sorted.sort_unstable();
    assert!(sorted.windows(2).all(|w| w[0] != w[1]), "each codepoint named once");
}

/// A command's icon is chosen by its id, and one with none has none; a pane has its own.
#[test]
fn icons_are_chosen_by_command_id_and_pane() {
    assert_eq!(for_command(Reload::ID), Some(ROTATE));
    assert_eq!(for_command(Open::ID), Some(FOLDER_OPEN));
    assert_eq!(for_command("repo.nothing"), None);
    for (id, icon) in BY_COMMAND {
        assert!(ALL.contains(icon), "{id}'s icon is named");
        assert!(swarmception_engine::commands::find(id).is_some(), "{id} is a command");
    }
    let mut seen: Vec<Icon> = Pane::ALL.iter().map(|&p| for_pane(p)).collect();
    seen.dedup();
    assert_eq!(seen.len(), Pane::ALL.len(), "each pane its own icon");
}

/// A widget with an icon is named by its text alone: the icon's atom is left out of the name.
#[test]
fn an_icon_is_left_out_of_a_widgets_name() {
    assert_eq!((atom(PLAY), "Run").into_atoms().text().as_deref(), Some("Run"));
    assert_eq!((cell(Some(CIRCLE_INFO)), "Details").into_atoms().text().as_deref(), Some("Details"));
    assert_eq!((cell(None), "Quit").into_atoms().text().as_deref(), Some("Quit"));
}

/// An icon and its words are one line whose text is the words after the icon; before the faces
/// are bound the icon is in egui's face, never an unknown family.
#[test]
fn a_line_is_the_icon_then_its_words() {
    let ctx = egui::Context::default();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        let job = line(ui.ctx(), CLOCK, "12:00", FontId::proportional(12.0), Color32::PLACEHOLDER);
        assert_eq!(job.text, format!("{CLOCK}12:00"));
        assert_eq!(job.sections[0].format.font_id.family, FontFamily::Proportional, "unbound: egui's face");
        ui.label(titled(ui.ctx(), CLOCK, "12:00"));
    })
    .textures_delta
    .clear();
    crate::theme::apply(&ctx);
    ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        let job = line(ui.ctx(), CLOCK, "12:00", FontId::proportional(12.0), Color32::PLACEHOLDER);
        assert_eq!(job.sections[0].format.font_id.family, fonts::icons(), "bound: the icons' own");
        assert_eq!(job.sections[1].format.font_id.family, FontFamily::Proportional, "the words in text's");
    })
    .textures_delta
    .clear();
}

/// A line made with icons in it draws each icon from the icons' family and the words from text's.
#[test]
fn a_lines_icons_are_runs_of_their_own() {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        let line = format!("{TOWER_BROADCAST}  live · failed{TRIANGLE_EXCLAMATION}");
        let laid = job(ui.ctx(), &line, FontId::proportional(12.0), Color32::PLACEHOLDER);
        assert_eq!(laid.text, line, "the same text");
        let runs: Vec<(&str, bool)> = laid
            .sections
            .iter()
            .map(|s| (laid.text.get(s.byte_range.start.0..s.byte_range.end.0).unwrap_or_default(), s.format.font_id.family == fonts::icons()))
            .collect();
        let want = [(TOWER_BROADCAST.to_string(), true), ("  live · failed".to_owned(), false), (TRIANGLE_EXCLAMATION.to_string(), true)];
        let want: Vec<(&str, bool)> = want.iter().map(|(t, i)| (t.as_str(), *i)).collect();
        assert_eq!(runs, want);
        assert!(job(ui.ctx(), "", FontId::proportional(12.0), Color32::PLACEHOLDER).sections.is_empty());
        assert!(ALL.iter().all(|&i| is_icon(i)) && !is_icon('a') && !is_icon('·'));
    })
    .textures_delta
    .clear();
}
