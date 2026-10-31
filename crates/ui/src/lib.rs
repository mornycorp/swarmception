//! Swarmception's UI shell (docs/architecture.md › The UI): the menu bar (File, View, as data in
//! [`menu`]; on macOS the app puts it in the system menu bar instead), the toolbar, the panes (the timeline, for now) and the status bar, over the engine's commands.
//! Every action runs as an engine command (`repo.reload`, `rows.set`, …) or as one of the
//! timeline widget's (`view.fit`, `view.zoom`); what the shell keeps of its own, the panes shown,
//! is [`UiState`], plain serde data.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

pub mod fonts;
pub mod icons;
pub mod menu;
mod timeline;

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use swarmception_domain::{Grouping, RowsSpec};
use swarmception_engine::Session;
use swarmception_engine::engine_cmds::NoParams;
use swarmception_engine::repo_cmds::{Fetch, Open, OpenParams, Reload, Status, StatusResult};
use swarmception_engine::rows_cmds::{Get as GetRows, Set as SetRows, SetParams as SetRowsParams};

pub use menu::{MenuAction, MenuState};
pub use timeline::TimelinePane;

/// [`UiState`]'s shape; an older one still reads.
pub const UI_STATE_VERSION: u32 = 1;

/// The panes the View menu shows or hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Panes {
    pub timeline: bool,
}

impl Default for Panes {
    fn default() -> Self {
        Self { timeline: true }
    }
}

/// What the shell keeps between frames: plain data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct UiState {
    pub version: u32,
    pub panes: Panes,
}

impl Default for UiState {
    fn default() -> Self {
        Self { version: UI_STATE_VERSION, panes: Panes::default() }
    }
}

/// The app's window: the engine's session, the timeline pane over it, and the shell's state.
pub struct Shell {
    engine: Session,
    timeline: TimelinePane,
    ui: UiState,
    /// The engine's revision `status` was read at.
    status_at: Option<(u64, u64)>,
    status: StatusResult,
    spec: RowsSpec,
    /// The last command's error, shown in the status bar until the next succeeds.
    error: Option<String>,
    /// The app shows the menus in the system menu bar, so the window draws none.
    native_menu: bool,
}

impl Shell {
    /// The shell over `repo`'s tracking branch, read in the background; `ctx` is woken when the
    /// read lands.
    #[must_use]
    pub fn new(ctx: &egui::Context, repo: Option<&Path>) -> Self {
        let waker = ctx.clone();
        let engine = Session::with_waker(Arc::new(move || waker.request_repaint()));
        let mut shell = Self {
            engine,
            timeline: TimelinePane::default(),
            ui: UiState::default(),
            status_at: None,
            status: StatusResult::default(),
            spec: RowsSpec::default(),
            error: None,
            native_menu: false,
        };
        if let Some(repo) = repo {
            let result = shell.engine.call::<Open>(OpenParams { path: repo.display().to_string() });
            shell.note(result);
        }
        shell
    }

    /// The shell's own state.
    #[must_use]
    pub const fn ui_state(&self) -> UiState {
        self.ui
    }

    pub const fn set_ui_state(&mut self, state: UiState) {
        self.ui = state;
    }

    /// The engine's session, to run commands on.
    pub const fn engine(&mut self) -> &mut Session {
        &mut self.engine
    }

    /// Whether a read is still going.
    #[must_use]
    pub const fn loading(&self) -> bool {
        self.status.job.is_some()
    }

    /// Whether the window leaves its menu bar out because the app shows the menus in the system
    /// menu bar.
    pub const fn set_native_menu(&mut self, native: bool) {
        self.native_menu = native;
    }

    /// What the menus show now.
    #[must_use]
    pub fn menu_state(&mut self) -> MenuState {
        self.sync();
        MenuState {
            can_reload: self.status.repo.is_some() && !self.loading(),
            can_fit: self.timeline.has_data(),
            timeline: self.ui.panes.timeline,
            grouping: self.spec.grouping,
            collapse: self.spec.collapse,
        }
    }

    /// Runs a menu item, from the in-window menu or the system menu bar; `ctx` takes Quit.
    pub fn run_menu(&mut self, ctx: &egui::Context, action: MenuAction) {
        self.sync();
        match action {
            MenuAction::Reload => self.reload(),
            MenuAction::Fetch => self.fetch(),
            MenuAction::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                // The close lands on the next frame: a click in the system menu bar brings none.
                ctx.request_repaint();
            }
            MenuAction::Timeline => self.ui.panes.timeline = !self.ui.panes.timeline,
            MenuAction::Grouping(grouping) => self.set_rows(SetRowsParams { grouping: Some(grouping), collapse: None }),
            MenuAction::Collapse => self.set_rows(SetRowsParams { grouping: None, collapse: Some(!self.spec.collapse) }),
            MenuAction::Fit => self.timeline.fit(),
        }
    }

    /// Whether the timeline shows data.
    #[must_use]
    pub fn has_timeline(&mut self) -> bool {
        self.timeline.has_data()
    }

    fn note<T>(&mut self, result: Result<T, swarmception_engine::EngineError>) {
        match result {
            Ok(_) => self.error = None,
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Takes in what moved in the engine: a read that landed, new rows.
    fn sync(&mut self) {
        self.engine.poll();
        let revision = self.engine.revision();
        if self.status_at != Some(revision) {
            self.status_at = Some(revision);
            if let Ok(status) = self.engine.call::<Status>(NoParams {}) {
                self.status = status;
            }
            if let Ok(spec) = self.engine.call::<GetRows>(NoParams {}) {
                self.spec = spec;
            }
        }
        if let Err(error) = self.timeline.sync(&self.engine) {
            self.error = Some(error);
        }
    }

    fn set_rows(&mut self, params: SetRowsParams) {
        let result = self.engine.call::<SetRows>(params);
        self.note(result);
    }

    fn reload(&mut self) {
        let result = self.engine.call::<Reload>(NoParams {});
        self.note(result);
    }

    fn fetch(&mut self) {
        let result = self.engine.call::<Fetch>(NoParams {});
        self.note(result);
    }

    /// Draws the window's frame.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.sync();
        if !self.native_menu {
            egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        }
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            if self.ui.panes.timeline {
                self.timeline.ui(ui, &self.status);
            } else {
                ui.centered_and_justified(|ui| ui.weak("No pane is shown: View ▸ Timeline shows the timeline."));
            }
        });
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let state = self.menu_state();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| self.menu_items(ui, state, &MenuAction::FILE));
            ui.menu_button("View", |ui| self.menu_items(ui, state, &MenuAction::VIEW));
        });
    }

    fn menu_items(&mut self, ui: &mut egui::Ui, state: MenuState, items: &[Option<MenuAction>]) {
        for item in items {
            let Some(action) = *item else {
                ui.separator();
                continue;
            };
            if action == MenuAction::Grouping(Grouping::Flat) {
                ui.label("Tickets");
            }
            let label = format!("{}  {}", action.icon(), action.label());
            let enabled = state.enabled(action);
            let clicked = match state.checked(action) {
                Some(checked) if matches!(action, MenuAction::Grouping(_)) => ui.add_enabled(enabled, egui::RadioButton::new(checked, label)).clicked(),
                Some(mut checked) => ui.add_enabled(enabled, egui::Checkbox::new(&mut checked, label)).clicked(),
                None => ui.add_enabled(enabled, egui::Button::new(label)).clicked(),
            };
            if clicked {
                self.run_menu(ui.ctx(), action);
                if action.closes_menu() {
                    ui.close();
                }
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let can = self.status.repo.is_some() && !self.loading();
            if ui.add_enabled(can, icon_button(icons::RELOAD)).on_hover_text("Reload the tracking branch").clicked() {
                self.reload();
            }
            if ui.add_enabled(can, icon_button(icons::FETCH)).on_hover_text("Fetch the remote's tracking branch, then reload").clicked() {
                self.fetch();
            }
            ui.separator();
            let mut spec = self.spec;
            ui.selectable_value(&mut spec.grouping, Grouping::Flat, icons::FLAT).on_hover_text("Tickets packed onto shared rows, in the order they began");
            ui.selectable_value(&mut spec.grouping, Grouping::Workers, icons::WORKERS).on_hover_text("Tickets by worker: machine / worker / its rows");
            ui.separator();
            ui.toggle_value(&mut spec.collapse, icons::COLLAPSE).on_hover_text("Collapse: leave out rows with nothing in them");
            if spec != self.spec {
                self.set_rows(SetRowsParams { grouping: Some(spec.grouping), collapse: Some(spec.collapse) });
            }
            ui.separator();
            let has = self.timeline.has_data();
            if ui.add_enabled(has, icon_button(icons::FIT)).on_hover_text("Fit: show the whole swarm").clicked() {
                self.timeline.fit();
            }
            if ui.add_enabled(has, icon_button(icons::ZOOM_IN)).on_hover_text("Zoom in").clicked() {
                self.timeline.zoom(2.0);
            }
            if ui.add_enabled(has, icon_button(icons::ZOOM_OUT)).on_hover_text("Zoom out").clicked() {
                self.timeline.zoom(0.5);
            }
            if self.loading() {
                ui.separator();
                ui.label(format!("{}  reading the tracking branch…", icons::SPINNER));
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let s = &self.status;
            match &s.repo {
                Some(repo) => ui.label(format!("{}  {repo}", icons::FOLDER)),
                None => ui.weak("No repository: start with --repo <path>"),
            };
            if let (Some(reference), Some(commit)) = (&s.reference, &s.commit) {
                ui.separator();
                let short = commit.get(..8).unwrap_or(commit);
                ui.label(format!("{}  {} @ {short}", icons::BRANCH, reference.trim_start_matches("refs/")));
                ui.separator();
                ui.label(format!(
                    "{}  {} tickets · {} merged · {} merges to main · {} workers · {} events",
                    icons::TICKETS,
                    s.tickets,
                    s.merged,
                    s.main_merges,
                    s.workers,
                    s.events
                ));
            }
            if let Some(text) = self.timeline.clock_text(&mut self.engine) {
                ui.separator();
                ui.label(format!("{}  {text}", icons::CLOCK));
            }
            let problem = self.error.as_ref().or(s.error.as_ref());
            if let Some(problem) = problem {
                ui.separator();
                ui.colored_label(ui.visuals().error_fg_color, format!("{}  {problem}", icons::WARNING));
            } else if s.line_errors > 0 {
                ui.separator();
                let lines: Vec<String> = s.errors.iter().map(|e| format!("{} line {}: {}", e.instance, e.line, e.message)).collect();
                ui.colored_label(ui.visuals().warn_fg_color, format!("{}  {} lines didn't read", icons::WARNING, s.line_errors))
                    .on_hover_text(lines.join("\n"));
            }
        });
    }
}

fn icon_button(icon: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(icon).size(15.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_state_reads_back_and_defaults() {
        let state = UiState { panes: Panes { timeline: false }, ..UiState::default() };
        let text = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<UiState>(&text).unwrap(), state);
        assert_eq!(serde_json::from_str::<UiState>("{}").unwrap(), UiState::default());
        assert!(serde_json::from_str::<UiState>(r#"{"nope":1}"#).is_err());
    }

    #[test]
    fn menu_items_run_the_same_commands_the_state_shows() {
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, None);
        let state = shell.menu_state();
        assert_eq!(state, MenuState { can_reload: false, can_fit: false, timeline: true, grouping: Grouping::Flat, collapse: false });

        shell.run_menu(&ctx, MenuAction::Timeline);
        shell.run_menu(&ctx, MenuAction::Grouping(Grouping::Workers));
        shell.run_menu(&ctx, MenuAction::Collapse);
        let state = shell.menu_state();
        assert!(!state.timeline);
        assert_eq!(state.checked(MenuAction::Grouping(Grouping::Workers)), Some(true));
        assert_eq!(state.checked(MenuAction::Collapse), Some(true));
        assert!(!shell.ui_state().panes.timeline);
        // What the menu shows is what `rows.get` returns.
        let spec = shell.engine().call::<GetRows>(NoParams {}).unwrap();
        assert_eq!((spec.grouping, spec.collapse), (state.grouping, state.collapse));

        shell.run_menu(&ctx, MenuAction::Collapse);
        assert!(!shell.menu_state().collapse);
        // No repository: Reload is an error the status bar shows, never a panic.
        shell.run_menu(&ctx, MenuAction::Reload);
        assert!(shell.error.is_some());
    }

    #[test]
    fn a_native_menu_leaves_the_window_without_one() {
        // Whether the window drew a "File" menu button.
        let drawn = |native: bool| {
            let ctx = egui::Context::default();
            let mut shell = Shell::new(&ctx, None);
            shell.set_native_menu(native);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| shell.ui(ui));
            output.textures_delta.clear();
            output.shapes.iter().any(|clipped| matches!(&clipped.shape, egui::Shape::Text(text) if text.galley.text() == "File"))
        };
        assert!(drawn(false));
        assert!(!drawn(true));
    }
}
