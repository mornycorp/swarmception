//! Swarmception's UI shell (docs/architecture.md › The UI): the menu bar (File, View, as data in
//! [`menu`]; on macOS the app puts it in the system menu bar instead), the toolbar, the panes (the
//! timeline, and under it the factory) and the status bar, over the engine's commands.
//! Every action runs as an engine command (`repo.reload`, `rows.set`, …) or as one of the
//! timeline widget's (`view.fit`; the zoom buttons are the widget's toolbar's since #35, which
//! shows only [`TIMELINE_TOOLBAR`]); what the shell keeps of its own, the panes shown
//! and the factory's time, is [`UiState`], plain serde data.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

mod factory;
mod factory_chip;
mod factory_paint;
mod factory_stats;
mod factory_tip;
pub mod fonts;
pub mod icons;
pub mod menu;
mod timeline;

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use swarmception_domain::timeline::Link;
use swarmception_domain::{Grouping, RowsSpec};
use swarmception_engine::{Github, Session};

use swarmception_engine::engine_cmds::NoParams;
use swarmception_engine::factory_cmds::TicketParams;
use swarmception_engine::issues_cmds::Urls;
use swarmception_engine::repo_cmds::{Fetch, Open, OpenParams, Reload, Status, StatusResult};
use swarmception_engine::rows_cmds::{Get as GetRows, Set as SetRows, SetParams as SetRowsParams};

pub use factory::{FactoryPane, FactoryView, Jump, Source as FactorySource, Speed, Steps};
pub use factory_chip::{Look, Size, look};
pub use factory_paint::{Drawn, Marks};
pub use factory_stats::{StatsStrip, StripDrawn, percent, rate};
pub use menu::{MenuAction, MenuState};
pub use timeline::{Looking, Selection, TOOLBAR as TIMELINE_TOOLBAR, TimelinePane};

/// [`UiState`]'s shape; an older one still reads. 2: the factory pane and its time.
pub const UI_STATE_VERSION: u32 = 2;

/// The panes the View menu shows or hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Panes {
    pub timeline: bool,
    /// The factory, under the timeline when both show.
    pub factory: bool,
}

impl Default for Panes {
    fn default() -> Self {
        Self { timeline: true, factory: true }
    }
}

/// What the shell keeps between frames: plain data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct UiState {
    pub version: u32,
    pub panes: Panes,
    pub factory: FactoryView,
}

impl Default for UiState {
    fn default() -> Self {
        Self { version: UI_STATE_VERSION, panes: Panes::default(), factory: FactoryView::default() }
    }
}

/// The app's window: the engine's session, the timeline pane over it, and the shell's state.
pub struct Shell {
    engine: Session,
    timeline: TimelinePane,
    factory: FactoryPane,
    /// A time put on the factory (`--at`) that the timeline's playhead goes to once it has data.
    playhead_to: Option<swarmception_model::Timestamp>,
    /// The ticket whose bars the timeline has selected because it is hovered in the factory.
    lit: Option<u64>,
    ui: UiState,
    /// The engine's revision `status` was read at.
    status_at: Option<(u64, u64)>,
    status: StatusResult,
    spec: RowsSpec,
    /// The last command's error, shown in the status bar until the next succeeds.
    error: Option<String>,
    /// The app shows the menus in the system menu bar, so the window draws none.
    native_menu: bool,
    /// The pages opened from context menus, newest last: what was handed to `ctx.open_url`.
    opened: Vec<String>,
}

impl Shell {
    /// The shell over `repo`'s tracking branch, read in the background; `ctx` is woken when the
    /// read lands. Its GitHub issues come through `gh` on the path, cached in the user's cache
    /// directory.
    #[must_use]
    pub fn new(ctx: &egui::Context, repo: Option<&Path>) -> Self {
        Self::with_github(ctx, repo, Github::default())
    }

    /// As [`Shell::new`], the issues read through `github` (its `gh`, its cache directory).
    #[must_use]
    pub fn with_github(ctx: &egui::Context, repo: Option<&Path>, github: Github) -> Self {
        let waker = ctx.clone();
        let mut engine = Session::with_waker(Arc::new(move || waker.request_repaint()));
        engine.set_github(github);
        let mut shell = Self {
            engine,
            timeline: TimelinePane::default(),
            factory: FactoryPane::default(),
            playhead_to: None,
            lit: None,
            ui: UiState::default(),
            status_at: None,
            status: StatusResult::default(),
            spec: RowsSpec::default(),
            error: None,
            native_menu: false,
            opened: Vec::new(),
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

    /// The timeline pane, to read the widget's state.
    pub const fn timeline(&mut self) -> &mut TimelinePane {
        &mut self.timeline
    }

    /// The factory pane, to read what it shows.
    #[must_use]
    pub const fn factory(&self) -> &FactoryPane {
        &self.factory
    }

    /// Puts the factory at `t`, not following the timeline, and the timeline's playhead there.
    pub const fn set_factory_at(&mut self, t: swarmception_model::Timestamp) {
        self.ui.factory.t = Some(t);
        self.ui.factory.follow = false;
        self.ui.factory.playing = false;
        self.playhead_to = Some(t);
    }

    /// Steps the factory to a key moment, or jumps it to the first or last event (#33), as ⏮ / ⏭
    /// and ← / → do: on the next frame that has a scene, the timeline's playhead following.
    pub const fn jump_factory(&mut self, jump: Jump) {
        self.factory.ask(jump);
    }

    /// Whether a read is still going.
    #[must_use]
    pub const fn loading(&self) -> bool {
        self.status.job.is_some()
    }

    /// Whether the GitHub issues are still being fetched.
    #[must_use]
    pub const fn fetching_issues(&self) -> bool {
        self.status.issues.fetching
    }

    /// Where on screen ticket `issue`'s first bar on the timeline was drawn last frame.
    pub fn bar_point(&mut self, issue: u64) -> Option<egui::Pos2> {
        self.timeline.bar_point(&mut self.engine, issue)
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
            factory: self.ui.panes.factory,
            grouping: self.spec.grouping,
            collapse: self.spec.collapse,
        }
    }

    /// The pages opened from the bars' and chips' context menus, oldest first (#17).
    #[must_use]
    pub fn opened(&self) -> &[String] {
        &self.opened
    }

    /// Opens `link`'s page in the browser: its URL from `ticket.urls`, handed to egui's
    /// `open_url` (eframe opens it; a test reads it from the frame's output instead).
    pub fn open_link(&mut self, ctx: &egui::Context, link: Link) {
        let (number, pull) = match link {
            Link::Issue(n) => (n, false),
            Link::Pull(n) => (n, true),
        };
        let result = self.engine.call::<Urls>(TicketParams { issue: number });
        let url = result.as_ref().ok().map(|urls| if pull { urls.pull_url.clone() } else { urls.issue_url.clone() });
        self.note(result);
        if let Some(url) = url {
            ctx.open_url(egui::OpenUrl::new_tab(url.clone()));
            self.opened.push(url);
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
            MenuAction::Factory => self.ui.panes.factory = !self.ui.panes.factory,
            MenuAction::Grouping(grouping) => self.set_rows(SetRowsParams { grouping: Some(grouping), ..SetRowsParams::default() }),
            MenuAction::Collapse => self.set_rows(SetRowsParams { collapse: Some(!self.spec.collapse), ..SetRowsParams::default() }),
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
        if self.ui.panes.factory {
            self.step_factory(ui.ctx());
        }
        egui::CentralPanel::default().show(ui, |ui| match (self.ui.panes.timeline, self.ui.panes.factory) {
            (true, true) => {
                let height = ui.available_height();
                egui::Panel::bottom("factory")
                    .resizable(true)
                    .default_size(height * 0.58)
                    .size_range(140.0..=(height - 100.0).max(140.0))
                    .show(ui, |ui| self.factory.ui(ui, &mut self.ui.factory, &self.status));
                self.timeline.ui(ui, &self.status);
            }
            (true, false) => self.timeline.ui(ui, &self.status),
            (false, true) => self.factory.ui(ui, &mut self.ui.factory, &self.status),
            (false, false) => {
                ui.centered_and_justified(|ui| ui.weak("No pane is shown: View ▸ Timeline or View ▸ Factory shows one."));
            }
        });
        self.light_timeline(ui.ctx());
        if let Some(hour) = self.factory.strip().drawn().chosen {
            self.show_hour(hour);
            ui.ctx().request_repaint();
        }
        let mut links = self.timeline.take_links();
        links.extend(self.factory.drawn().open);
        for link in links {
            self.open_link(ui.ctx(), link);
        }
    }

    /// The hour starting at `start`, its bar clicked in the factory's sparkline (#31): the
    /// timeline's window on it, and the factory's *t* and the timeline's playhead at its last
    /// second, so the factory shows what the hour ended with.
    fn show_hour(&mut self, start: swarmception_model::Timestamp) {
        let end = swarmception_model::Timestamp(start.0.saturating_add(3_600));
        let t = swarmception_model::Timestamp(end.0.saturating_sub(1));
        self.timeline.show_span(&mut self.engine, start, end);
        self.ui.factory.t = Some(t);
        self.timeline.put_playhead(&mut self.engine, t);
    }

    /// The ticket hovered in the factory, its bars selected on the timeline; the person's own
    /// selection put back when the pointer leaves it.
    fn light_timeline(&mut self, ctx: &egui::Context) {
        let hovered = if self.ui.panes.factory { self.factory.hovered() } else { None };
        if hovered != self.lit {
            self.timeline.light_ticket(&mut self.engine, hovered);
            self.lit = hovered;
            ctx.request_repaint();
        }
    }

    /// The factory's time this frame (a step, play, the timeline or neither), the timeline's
    /// playhead where a step, play or `--at` put it, and the scene at that time.
    fn step_factory(&mut self, ctx: &egui::Context) {
        let looking = self.timeline.looking(&mut self.engine);
        let jumped = self.factory.jump(&mut self.engine, &mut self.ui.factory);
        let to = self.factory.step(ctx, &mut self.ui.factory, looking);
        if jumped.is_some() {
            ctx.request_repaint();
        }
        if let Some(t) = jumped.or(to).or_else(|| self.playhead_to.filter(|_| self.timeline.has_data())) {
            self.timeline.put_playhead(&mut self.engine, t);
            self.playhead_to = None;
        }
        let recent = factory::recent(&self.ui.factory, looking);
        self.factory.sync(&mut self.engine, &self.ui.factory, recent);
        self.factory.sync_stats(&mut self.engine, looking);
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
            // The zoom in / out / fit buttons are the timeline widget's own toolbar's (#35).
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
            self.issues_note(ui);
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

impl Shell {
    /// The status bar's word on the GitHub issues: how many and from where, the fetch going on,
    /// or why there are none (a warning, never a failed load).
    fn issues_note(&self, ui: &mut egui::Ui) {
        let issues = &self.status.issues;
        if self.status.repo.is_none() {
            return;
        }
        ui.separator();
        let from = match issues.source {
            Some(swarmception_engine::issues::IssuesSource::Github) => "from GitHub",
            Some(swarmception_engine::issues::IssuesSource::Cache) => "cached",
            None => "",
        };
        let mut text = match (&issues.github, issues.issues) {
            (Some(repo), 0) if issues.source.is_none() => format!("{}  {repo}", icons::ISSUES),
            (Some(repo), n) => format!("{}  {repo}: {n} issues and PRs {from}", icons::ISSUES),
            (None, _) => format!("{}  no issues", icons::ISSUES),
        };
        if issues.fetching {
            text.push_str(" · fetching…");
        }
        let hover = issues.fetched.map(|t| format!("fetched {t}")).into_iter().chain(issues.cache.iter().map(|c| format!("cached in {c}")));
        let hover: Vec<String> = hover.chain(issues.error.iter().cloned()).collect();
        let label = match &issues.error {
            Some(_) => ui.colored_label(ui.visuals().warn_fg_color, format!("{text} {}", icons::WARNING)),
            None => ui.label(text),
        };
        if !hover.is_empty() {
            label.on_hover_text(hover.join("\n"));
        }
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
        let state = UiState { panes: Panes { timeline: false, factory: true }, ..UiState::default() };
        let text = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<UiState>(&text).unwrap(), state);
        assert_eq!(serde_json::from_str::<UiState>("{}").unwrap(), UiState::default());
        assert!(serde_json::from_str::<UiState>(r#"{"nope":1}"#).is_err());
        // Version 1, before the factory, still reads, with the factory's defaults.
        let old: UiState = serde_json::from_str(r#"{"version":1,"panes":{"timeline":false}}"#).unwrap();
        assert_eq!((old.panes.timeline, old.panes.factory, old.factory), (false, true, FactoryView::default()));
    }

    #[test]
    fn menu_items_run_the_same_commands_the_state_shows() {
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, None);
        let state = shell.menu_state();
        assert_eq!(state, MenuState { can_reload: false, can_fit: false, timeline: true, factory: true, grouping: Grouping::Flat, collapse: false });

        shell.run_menu(&ctx, MenuAction::Timeline);
        shell.run_menu(&ctx, MenuAction::Factory);
        shell.run_menu(&ctx, MenuAction::Grouping(Grouping::Workers));
        shell.run_menu(&ctx, MenuAction::Collapse);
        let state = shell.menu_state();
        assert!(!state.timeline);
        assert!(!state.factory);
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
