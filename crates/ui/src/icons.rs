//! Icons on menus, buttons and labels (#176, swarm-core's App UI Substrate › Icons): Font Awesome
//! Free 6.7.2's solid face ([`crate::fonts::icons`]; the font SIL OFL 1.1, the icons CC BY 4.0,
//! whose attribution the embedded font's own metadata carries).
//!
//! - **Codepoints** are the named constants here, each the icon's Font Awesome 6 name, never a
//!   literal where it is drawn; a test checks every one ([`ALL`]) is in the embedded face.
//! - **Which icon goes where is data:** a command's by its id ([`BY_COMMAND`]), so a new command
//!   needs no menu code to have one; a pane's by [`for_pane`]. A menu item without an icon keeps
//!   the empty fixed-width column ([`cell`]), so the labels line up.
//! - **Icons are drawn from their own family only**, never from a text face, and text never from
//!   theirs: as a widget's atom ([`atom`]), or a run of their own in a line of text ([`titled`]).
//! - **An icon takes its widget's text colour**, so a disabled item's icon dims with it.
//! - **Accessible names stay text.** An icon atom is built by a closure, which egui leaves out of
//!   a widget's name: a widget is named by its text, as it would be with no icon; an icon-only
//!   button gets a name of its own ([`only`]) and a tooltip.
//!
//! Until the faces are bound (a pane drawn before [`crate::theme::apply`], as a test's, or the
//! window's first pass) an icon is drawn in egui's proportional face, where it is a box: never a
//! family egui doesn't know, which would panic.

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Atom, AtomKind, Color32, FontFamily, FontId, IntoSizedResult, Response, RichText, SizedAtomKind, Ui, WidgetText, vec2};
use swarmception_engine::Command as _;
use swarmception_engine::repo_cmds::{ClearRecent, Fetch, Open, Reload};

use crate::fonts;
use crate::pane::Pane;

/// An icon: a codepoint of Font Awesome's solid face.
pub type Icon = char;

// The codepoints, by Font Awesome 6 name (fontawesome.com/icons/<name>?s=solid).
pub const ARROW_LEFT: Icon = '\u{f060}';
pub const BACKWARD_STEP: Icon = '\u{f048}';
pub const BARS_STAGGERED: Icon = '\u{f550}';
pub const CHECK: Icon = '\u{f00c}';
pub const CHEVRON_DOWN: Icon = '\u{f078}';
pub const CHEVRON_LEFT: Icon = '\u{f053}';
pub const CHEVRON_RIGHT: Icon = '\u{f054}';
pub const CIRCLE_CHECK: Icon = '\u{f058}';
pub const CIRCLE_EXCLAMATION: Icon = '\u{f06a}';
pub const CIRCLE_INFO: Icon = '\u{f05a}';
pub const CIRCLE_XMARK: Icon = '\u{f057}';
pub const CLOCK: Icon = '\u{f017}';
pub const CLOCK_ROTATE_LEFT: Icon = '\u{f1da}';
pub const CLOUD_ARROW_DOWN: Icon = '\u{f0ed}';
pub const CODE_BRANCH: Icon = '\u{f126}';
pub const CROP_SIMPLE: Icon = '\u{f565}';
pub const DOWN_LEFT_AND_UP_RIGHT_TO_CENTER: Icon = '\u{f422}';
pub const FLASK: Icon = '\u{f0c3}';
pub const FOLDER_OPEN: Icon = '\u{f07c}';
pub const FORWARD_STEP: Icon = '\u{f051}';
pub const GEAR: Icon = '\u{f013}';
pub const INDUSTRY: Icon = '\u{f275}';
pub const LINK: Icon = '\u{f0c1}';
pub const LIST: Icon = '\u{f03a}';
pub const MAXIMIZE: Icon = '\u{f31e}';
pub const MINIMIZE: Icon = '\u{f78c}';
pub const PAUSE: Icon = '\u{f04c}';
pub const PLAY: Icon = '\u{f04b}';
pub const RIGHT_FROM_BRACKET: Icon = '\u{f2f5}';
pub const ROTATE: Icon = '\u{f2f1}';
pub const SPINNER: Icon = '\u{f110}';
pub const TAG: Icon = '\u{f02b}';
pub const TICKET_SIMPLE: Icon = '\u{f3ff}';
pub const TOWER_BROADCAST: Icon = '\u{f519}';
pub const TRIANGLE_EXCLAMATION: Icon = '\u{f071}';
pub const UP_RIGHT_FROM_SQUARE: Icon = '\u{f35d}';
pub const USERS: Icon = '\u{f0c0}';
pub const XMARK: Icon = '\u{f00d}';

/// Every icon: what the test checks is in the embedded face.
pub const ALL: [Icon; 38] = [
    ARROW_LEFT,
    BACKWARD_STEP,
    BARS_STAGGERED,
    CHECK,
    CHEVRON_DOWN,
    CHEVRON_LEFT,
    CHEVRON_RIGHT,
    CIRCLE_CHECK,
    CIRCLE_EXCLAMATION,
    CIRCLE_INFO,
    CIRCLE_XMARK,
    CLOCK,
    CLOCK_ROTATE_LEFT,
    CLOUD_ARROW_DOWN,
    CODE_BRANCH,
    CROP_SIMPLE,
    DOWN_LEFT_AND_UP_RIGHT_TO_CENTER,
    FLASK,
    FOLDER_OPEN,
    FORWARD_STEP,
    GEAR,
    INDUSTRY,
    LINK,
    LIST,
    MAXIMIZE,
    MINIMIZE,
    PAUSE,
    PLAY,
    RIGHT_FROM_BRACKET,
    ROTATE,
    SPINNER,
    TAG,
    TICKET_SIMPLE,
    TOWER_BROADCAST,
    TRIANGLE_EXCLAMATION,
    UP_RIGHT_FROM_SQUARE,
    USERS,
    XMARK,
];

/// Each command's icon, by its id: its menu item and its button show it.
pub const BY_COMMAND: &[(&str, Icon)] = &[(Open::ID, FOLDER_OPEN), (ClearRecent::ID, CIRCLE_XMARK), (Reload::ID, ROTATE), (Fetch::ID, CLOUD_ARROW_DOWN)];

/// The icons' size: the body's.
pub const SIZE: f32 = fonts::BODY_SIZE;

/// The width of a menu's icon column, in points: Font Awesome's widest glyphs are 1.25 em.
pub const COLUMN: f32 = 16.0;

/// The icon of the command `id`, if it has one.
#[must_use]
pub fn for_command(id: &str) -> Option<Icon> {
    BY_COMMAND.iter().find(|(command, _)| *command == id).map(|&(_, icon)| icon)
}

/// Each pane's icon: its View menu item, its title.
#[must_use]
pub const fn for_pane(pane: Pane) -> Icon {
    match pane {
        Pane::Timeline => BARS_STAGGERED,
        Pane::Factory => INDUSTRY,
        Pane::Details => CIRCLE_INFO,
        Pane::Workers => USERS,
    }
}

/// The family icons are drawn from in `ctx`: Font Awesome's alone once it is bound, egui's
/// proportional face before (where an icon is a box, but nothing panics).
#[must_use]
pub fn family(ctx: &egui::Context) -> FontFamily {
    if fonts::bound(ctx) { fonts::icons() } else { FontFamily::Proportional }
}

/// An icon as text in the icons' family, in the widget's own colour.
#[must_use]
pub fn glyph(ctx: &egui::Context, icon: Icon) -> RichText {
    RichText::new(icon.to_string()).family(family(ctx)).size(SIZE)
}

/// An icon at `size`, for a painter.
#[must_use]
pub fn font(ctx: &egui::Context, size: f32) -> FontId {
    FontId::new(size, family(ctx))
}

/// `text` after `icon`, as one line: the icon a run in its family, the text in `font`, both in
/// `colour` (`Color32::PLACEHOLDER` for the widget's own text colour). What a label, a window's
/// title or a dock tab draws when an icon goes with its words, so it is named by its text.
#[must_use]
pub fn line(ctx: &egui::Context, icon: Icon, text: &str, font: FontId, colour: Color32) -> LayoutJob {
    let format = |font_id: FontId| TextFormat { font_id, color: colour, valign: Align::Center, ..Default::default() };
    let mut job = LayoutJob::default();
    job.append(&icon.to_string(), 0.0, format(FontId::new(font.size, family(ctx))));
    job.append(text, crate::density::spacing().icon_spacing, format(font));
    job
}

/// Whether `c` is an icon: Font Awesome's codepoints are in Unicode's Private Use Area, which no
/// text face draws.
#[must_use]
pub const fn is_icon(c: char) -> bool {
    matches!(c, '\u{e000}'..='\u{f8ff}')
}

/// A line of text that may hold icons (a catalog entry made with one, `"{icon}  Reading…"`), as
/// runs: each icon in the icons' family, the words between in `font`, all in `colour`
/// (`Color32::PLACEHOLDER` for the widget's own text colour). So an icon is drawn from its own
/// face wherever its line came from, and text never is.
#[must_use]
pub fn job(ctx: &egui::Context, text: &str, font: FontId, colour: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    append(ctx, &mut job, text, font, colour);
    job
}

/// [`job`]'s runs, appended to `job`.
pub fn append(ctx: &egui::Context, job: &mut LayoutJob, text: &str, font: FontId, colour: Color32) {
    let glyphs = FontId::new(font.size, family(ctx));
    let format = |font_id: FontId| TextFormat { font_id, color: colour, valign: Align::Center, ..Default::default() };
    for (run, icon) in runs(text) {
        job.append(run, 0.0, format(if icon { glyphs.clone() } else { font.clone() }));
    }
}

/// `text` cut into runs of icons and runs of words, in order, each with whether it is icons.
#[must_use]
pub fn runs(text: &str) -> Vec<(&str, bool)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut icon = None;
    for (at, c) in text.char_indices() {
        let this = is_icon(c);
        if icon.is_some_and(|was| was != this) {
            out.push((text.get(start..at).unwrap_or_default(), !this));
            start = at;
        }
        icon = Some(this);
    }
    if let Some(icon) = icon {
        out.push((text.get(start..).unwrap_or_default(), icon));
    }
    out
}

/// A line that may hold icons ([`job`]) in the body's font and the widget's own colour: what a
/// label, a button or a window's title draws.
#[must_use]
pub fn text(ctx: &egui::Context, text: &str) -> WidgetText {
    job(ctx, text, FontId::proportional(fonts::BODY_SIZE), Color32::PLACEHOLDER).into()
}

/// A line that may hold icons ([`job`]) in the body's font and `colour` (a warning's, the weak
/// text's, the strong's).
#[must_use]
pub fn text_in(ctx: &egui::Context, text: &str, colour: Color32) -> WidgetText {
    job(ctx, text, FontId::proportional(fonts::BODY_SIZE), colour).into()
}

/// `text` after `icon` in the body's font and the widget's colour (a label's, a title's).
#[must_use]
pub fn titled(ctx: &egui::Context, icon: Icon, text: &str) -> WidgetText {
    line(ctx, icon, text, FontId::proportional(fonts::BODY_SIZE), Color32::PLACEHOLDER).into()
}

/// `text` after `icon` in `colour` (a warning's, an error's).
#[must_use]
pub fn titled_in(ctx: &egui::Context, icon: Icon, text: &str, colour: Color32) -> WidgetText {
    line(ctx, icon, text, FontId::proportional(fonts::BODY_SIZE), colour).into()
}

/// An icon as a widget's atom: laid out and painted as the widget's text is (in its colour), but
/// built by a closure, which egui leaves out of the widget's accessible name.
#[must_use]
pub fn atom(icon: Icon) -> Atom<'static> {
    Atom::from(AtomKind::closure(move |ui, args| {
        let galley = WidgetText::from(glyph(ui.ctx(), icon)).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, args.fallback_font);
        IntoSizedResult { intrinsic_size: galley.size(), sized: SizedAtomKind::Text(galley) }
    }))
}

/// A menu's icon column: `icon` in [`COLUMN`], or the column empty, so the labels line up.
#[must_use]
pub fn cell(icon: Option<Icon>) -> Atom<'static> {
    let size = Some(vec2(COLUMN, SIZE));
    match icon {
        Some(icon) => Atom { size, ..atom(icon) },
        None => Atom { size, ..Atom::default() },
    }
}

/// A button with `icon` before `text`.
pub fn text_button(ui: &mut Ui, icon: Icon, text: &str) -> Response {
    ui.add(egui::Button::new((atom(icon), text)))
}

/// An icon-only button, named `name` for whatever reads the window, and with `name` as its tooltip
/// (the App UI Substrate › Icons: an icon-only button gets a name and a tooltip of its own).
pub fn only(icon: Icon) -> egui::Button<'static> {
    egui::Button::new(atom(icon))
}

/// Gives an icon-only widget's response its name, for screen readers and `ui.inspect`, and its
/// tooltip.
pub fn named(response: Response, name: &str) -> Response {
    let enabled = response.enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, name));
    response.on_hover_text(name)
}

#[cfg(test)]
mod tests;
