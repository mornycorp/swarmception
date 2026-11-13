//! Swarmception's UI shell (docs/architecture.md › The UI): the menu bar (File, View, as data in
//! [`menu`]; on macOS the app puts it in the system menu bar instead), the toolbar, the panes (the
//! timeline, under it the factory, and beside them the details of what is
//! selected) and the status bar, over the engine's commands.
//! Every action runs as an engine command (`repo.reload`, `rows.set`, …) or as one of the
//! timeline widget's (`view.fit`; the zoom buttons are the widget's toolbar's since #35, which
//! shows only [`TIMELINE_TOOLBAR`]); what the shell keeps of its own, the panes shown, the
//! factory's time and what the details are of, is [`UiState`], plain serde data.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

pub mod control;
mod details;
mod factory;
mod factory_chip;
mod factory_paint;
mod factory_stats;
mod factory_tip;
pub mod fonts;
pub mod icons;
pub mod layout;
pub mod menu;
mod open;
pub mod saved;
pub mod shot;
pub mod theme;
mod timeline;
mod workers;

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use swarmception_domain::timeline::Link;
use swarmception_domain::{Collapse, Grouping, RowsSpec};
use swarmception_engine::{Github, Session};

use swarmception_domain::details::Subject;
use swarmception_engine::details_cmds::{DetailsParams, Get as GetDetails};
use swarmception_engine::engine_cmds::NoParams;
use swarmception_engine::factory_cmds::TicketParams;
use swarmception_engine::issues_cmds::Urls;
use swarmception_engine::live::LiveStatus;
use swarmception_engine::live_cmds::{Get as GetLive, Set as SetLive, SetParams as SetLiveParams};
use swarmception_engine::repo_cmds::{Fetch, Open, OpenParams, Reload, Status, StatusResult};
use swarmception_engine::rows_cmds::{Get as GetRows, Set as SetRows, SetParams as SetRowsParams};
use swarmception_engine::session::Job;

pub use details::{DetailsDrawn, DetailsPane, DetailsView, params as details_params};
pub use factory::{FactoryPane, FactoryView, HeaderDrawn, Jump, Source as FactorySource, Speed, Steps};
pub use factory_chip::{Look, Size, look};
pub use factory_paint::{Drawn, Marks, Selected as FactorySelected};
pub use factory_stats::{StatsStrip, StripDrawn, percent, rate};
pub use menu::{MenuAction, MenuState};
pub use open::{FolderPicker, MenuDrawn, OpenInspect, Picked, Refused};
pub use timeline::{Hover, Lit, Looking, SETTLE as COLLAPSE_SETTLE, Selection, TOOLBAR as TIMELINE_TOOLBAR, TimeAxis, TimelinePane, TimelineView};
pub use workers::{
    Column as WorkersColumn, Place as WorkersPlace, RowDrawn as WorkerRowDrawn, RowKind as WorkerRowKind, WorkersDrawn, WorkersPane, WorkersView,
    cells as worker_cells,
};

/// [`UiState`]'s shape; an older one still reads. 2: the factory pane and its time; 3: the
/// details pane and what it is about; 4: the panes' sizes (#53); 5: the gates pane and its
/// height (#61); 6: the folded panes (#68); 7: the workers pane, its window and its sort (#67);
/// 8: where the workers' window is (#76); 9: the factory keeps no time of its own (#85: the
/// factory's `t` is gone, and an older state's is dropped as it reads); 10: folding the timeline's
/// quiet time (#86, `fold`; an older state's is off); 11: the factory's benches off shift opened
/// out of their fold (#106, `factory.off_shift`; an older state's are folded); 12: the gates pane
/// gone (#128: an older state's `gates` is dropped as it reads).
pub const UI_STATE_VERSION: u32 = 12;

/// Folding the timeline's quiet time (#86): View ▸ Fold Quiet Time, the toolbar's checkbox and
/// `ui.set`'s `foldQuiet` turn it on; the widget then folds each stretch where no row has
/// anything for longer than the threshold.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct FoldView {
    pub on: bool,
    /// The threshold in seconds (`ui.set`'s `foldAfterSeconds`); none: the swarm's own quiet gap,
    /// `stats.get`'s `quietAfterSeconds` (15 minutes).
    pub after_seconds: Option<i64>,
}

impl FoldView {
    /// The least threshold `ui.set` takes, in seconds.
    pub const LEAST: i64 = 1;
    /// The greatest, a week.
    pub const MOST: i64 = 7 * 24 * 3_600;

    /// The threshold in seconds the timeline folds by while on; none while off.
    #[must_use]
    pub fn threshold(self) -> Option<i64> {
        self.on.then(|| self.after_seconds.unwrap_or(swarmception_domain::stats::QUIET).clamp(Self::LEAST, Self::MOST))
    }
}

/// The panes the View menu shows or hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Panes {
    pub timeline: bool,
    /// The factory, under the timeline when both show.
    pub factory: bool,
    /// The details of what is selected, beside the others (#47).
    pub details: bool,
    /// Model and effort compared per worker, a window over the others (#67); off by default.
    pub workers: bool,
}

impl Default for Panes {
    fn default() -> Self {
        Self { timeline: true, factory: true, details: true, workers: false }
    }
}

/// The panes' sizes, in whole points, their margins included (#53): the details pane's width as
/// last drawn; the factory's height under the timeline as the person
/// dragged them (#68: they give way when the timeline needs the room, and come back when it
/// doesn't). None: the default for the window's size ([`layout::defaults`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PaneSizes {
    pub details: Option<u32>,
    pub factory: Option<u32>,
}

/// What the shell keeps between frames: plain data, kept across a restart ([`saved`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct UiState {
    pub version: u32,
    pub panes: Panes,
    pub factory: FactoryView,
    /// What the details pane tells the story of.
    pub details: DetailsView,
    /// The panes' sizes (#53).
    pub sizes: PaneSizes,
    /// Which panes are folded (#68): the person's choice, else the window's size decides.
    pub folded: layout::Folded,
    /// What the workers pane compares over, and its sort (#67); where its window is (#76).
    pub workers: WorkersView,
    /// Folding the timeline's quiet time (#86): off by default.
    pub fold: FoldView,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            version: UI_STATE_VERSION,
            panes: Panes::default(),
            factory: FactoryView::default(),
            details: DetailsView::default(),
            sizes: PaneSizes::default(),
            folded: layout::Folded::default(),
            workers: WorkersView::default(),
            fold: FoldView::default(),
        }
    }
}

/// The app's window: the engine's session, the timeline pane over it, and the shell's state.
pub struct Shell {
    engine: Session,
    timeline: TimelinePane,
    factory: FactoryPane,
    details: DetailsPane,
    workers: WorkersPane,
    /// What the details followed last: the timeline's chosen bar, the factory's traced ticket.
    followed: (Option<(String, usize)>, Option<u64>),
    /// The details' subject whose bars were last brought into view on the timeline (#55).
    revealed: Option<Subject>,
    /// A time put on the factory (`--at`) that the timeline's playhead goes to once it has data.
    playhead_to: Option<swarmception_model::Timestamp>,
    ui: UiState,
    /// The engine's revision `status` was read at.
    status_at: Option<(u64, u64)>,
    status: StatusResult,
    /// The live watch (#54), as `live.get` said at `status_at`.
    live: LiveStatus,
    spec: RowsSpec,
    /// The last command's error, shown in the status bar until the next succeeds.
    error: Option<String>,
    /// The app shows the menus in the system menu bar, so the window draws none.
    native_menu: bool,
    /// The pages opened from context menus, newest last: what was handed to `ctx.open_url`.
    opened: Vec<String>,
    /// Keeps the state in its file across a restart (#53); none: neither read nor written.
    keeper: Option<saved::Keeper>,
    /// The window's size and place on screen, as last drawn (or as restored).
    window_place: Option<saved::WindowPlace>,
    /// How the panes shared the window last frame (#68).
    layout: layout::LayoutDrawn,
    /// Opening repositories (#69): the folder picker, the recent list, a refusal shown.
    opening: open::Opening,
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
            details: DetailsPane::default(),
            workers: WorkersPane::default(),
            followed: (None, None),
            revealed: None,
            playhead_to: None,
            ui: UiState::default(),
            status_at: None,
            status: StatusResult::default(),
            live: LiveStatus::default(),
            spec: RowsSpec::default(),
            error: None,
            native_menu: false,
            opened: Vec::new(),
            keeper: None,
            window_place: None,
            layout: layout::LayoutDrawn::default(),
            opening: open::Opening::default(),
        };
        if let Some(repo) = repo {
            let result = shell.engine.call::<Open>(OpenParams { path: repo.display().to_string() });
            shell.note(result);
        }
        // The repository opened at the start is the shell's from the first frame.
        shell.opening.opens = shell.engine.opens();
        shell
    }

    /// The shell's own state.
    #[must_use]
    pub fn ui_state(&self) -> UiState {
        self.ui.clone()
    }

    pub fn set_ui_state(&mut self, state: UiState) {
        self.ui = state;
    }

    /// Keeps the state of `repo` (its [`saved::repo_key`]) in `store` from now on (#53): restored
    /// from what `loaded` read, when it read anything (none: a fresh start, `--fresh`), then
    /// written when it changes and holds still, and by [`Self::save_state`] on quit.
    pub fn keep_state(&mut self, store: saved::StateStore, repo: &Path, loaded: Option<&saved::Loaded>) {
        if let Some(state) = loaded.and_then(|l| l.state.as_ref()) {
            self.restore(state);
        }
        // The recent list is app-wide, kept beside the repositories' states (#69).
        self.keep_recent(&store);
        self.keeper = Some(saved::Keeper::new(store, repo, loaded));
    }

    /// Puts the shell as `state` has it: the panes and their sizes, the factory's settings and
    /// *t*, what the details are of, the rows (`rows.set`), the window's place, and the timeline's
    /// view once it has data.
    pub fn restore(&mut self, state: &saved::AppState) {
        self.ui = UiState { version: UI_STATE_VERSION, ..state.ui.clone() };
        // The traced ticket was followed already: the details stay on what they were of.
        self.followed.1 = self.ui.factory.ticket;
        // Its subject was in view as it was left: the restored window stays, not moved to it.
        self.revealed.clone_from(&self.ui.details.subject);
        self.set_rows(SetRowsParams { grouping: Some(state.rows.grouping), collapse: Some(state.rows.collapse), merge_marks: Some(state.rows.merge_marks) });
        if let Some(view) = &state.timeline {
            self.timeline.restore_view(view.clone());
        }
        self.window_place = state.window;
        self.restore_live(state.live);
    }

    /// Live as the person left it (#54), through `live.set`; an interval this build refuses is
    /// left at its default, the choice of on or off kept.
    fn restore_live(&mut self, live: saved::LiveChoice) {
        if live == saved::LiveChoice::default() {
            return;
        }
        let params = SetLiveParams { on: live.on, interval_seconds: live.interval_seconds };
        if self.engine.call::<SetLive>(params).is_err() && live.on.is_some() {
            self.set_live(SetLiveParams { on: live.on, interval_seconds: None });
        }
    }

    /// The state as it would be saved now; none when the shell keeps none. Play off; the
    /// factory's time is the timeline's playhead's, saved with the timeline (#85).
    pub fn app_state(&mut self) -> Option<saved::AppState> {
        let repo = self.keeper.as_ref()?.repo().to_path_buf();
        let mut ui = self.ui.clone();
        ui.factory.playing = false;
        Some(saved::AppState {
            ui,
            rows: self.spec,
            window: self.window_place,
            timeline: self.timeline.view_state(&mut self.engine),
            live: self.engine.call::<GetLive>(NoParams {}).map_or_else(
                |_| saved::LiveChoice::default(),
                |live| saved::LiveChoice { on: (!live.default).then_some(live.on), interval_seconds: Some(live.interval_seconds) },
            ),
            ..saved::AppState::new(&repo)
        })
    }

    /// Writes the state now (on quit); the file it went to, none when the shell keeps none.
    ///
    /// # Errors
    ///
    /// Why it couldn't be written.
    pub fn save_state(&mut self) -> Result<Option<std::path::PathBuf>, String> {
        self.sync();
        let Some(state) = self.app_state() else { return Ok(None) };
        self.keeper.as_mut().map(|keeper| keeper.write_now(state)).transpose()
    }

    /// Where the state is kept, read from and what was said of it (`ui.inspect`'s `saved`).
    #[must_use]
    pub fn saved(&self) -> saved::SavedInspect {
        self.keeper.as_ref().map(saved::Keeper::inspect).unwrap_or_default()
    }

    /// The window's size and place to save, as the app reads them from its viewport.
    fn note_window(&mut self, ctx: &egui::Context) {
        let place = ctx.input(|i| {
            let v = i.viewport();
            if v.minimized == Some(true) || v.fullscreen == Some(true) {
                return None;
            }
            let (inner, outer) = (v.inner_rect?, v.outer_rect?);
            Some(saved::WindowPlace { x: outer.min.x, y: outer.min.y, width: inner.width(), height: inner.height() })
        });
        if place.is_some() {
            self.window_place = place;
        }
    }

    /// Hands this frame's state to the keeper, which writes it once it holds still; a frame is
    /// asked for when a write waits.
    fn keep(&mut self, ctx: &egui::Context) {
        if self.keeper.is_none() {
            return;
        }
        self.note_window(ctx);
        let Some(state) = self.app_state() else { return };
        let now = ctx.input(|i| i.time);
        if let Some(wait) = self.keeper.as_mut().and_then(|keeper| keeper.observe(state, now)) {
            ctx.request_repaint_after(wait);
        }
    }

    /// The details pane, to read what it shows.
    #[must_use]
    pub const fn details(&self) -> &DetailsPane {
        &self.details
    }

    /// The workers pane, to read what it shows (#67).
    #[must_use]
    pub const fn workers(&self) -> &WorkersPane {
        &self.workers
    }

    /// Shows the details of `subject`, as a selection does; ← goes back to what was before.
    pub fn select(&mut self, subject: swarmception_domain::details::Subject) {
        self.details.select(&mut self.ui.details, subject);
    }

    /// Back to what the details showed before, as the pane's ← does.
    pub fn details_back(&mut self) {
        self.details.back(&mut self.ui.details);
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

    /// Puts the timeline's playhead at `t` (once it has data), and the factory, following it, at
    /// `t` (#85: the timeline is the factory's time); play off.
    pub fn set_factory_at(&mut self, t: swarmception_model::Timestamp) {
        self.factory.put(&mut self.ui.factory, t);
        self.playhead_to = Some(t);
    }

    /// Steps the factory to a key moment, or jumps it to the first or last event (#33), as ⏮ / ⏭
    /// and ← / → do: on the next frame that has a scene, the timeline's playhead following.
    pub const fn jump_factory(&mut self, jump: Jump) {
        self.factory.ask(jump);
    }

    /// Whether a read is still going: a Reload's or a Fetch's, not a live check's, which goes on
    /// unseen until it brings something (#54).
    #[must_use]
    pub fn loading(&self) -> bool {
        self.status.job.is_some_and(|job| job != Job::Live)
    }

    /// The live watch (#54), as `live.get` said last.
    #[must_use]
    pub const fn live(&self) -> &LiveStatus {
        &self.live
    }

    /// Turns the live watch on or off, or sets its interval (`live.set`).
    pub fn set_live(&mut self, params: SetLiveParams) {
        let result = self.engine.call::<SetLive>(params);
        self.note(result);
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

    /// Where on screen the timeline's `n`th fold's marker was drawn last frame (#86), from 0.
    pub fn fold_point(&mut self, n: usize) -> Option<egui::Pos2> {
        self.timeline.fold_point(n)
    }

    /// Folds the timeline's quiet time or not (#86): the app's `--fold`.
    pub fn fold_quiet(&mut self, on: bool) {
        self.set_fold(on);
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
            details: self.ui.panes.details,
            workers: self.ui.panes.workers,
            grouping: self.spec.grouping,
            collapse: self.spec.collapse,
            fold_quiet: self.ui.fold.on,
            merge_marks: self.spec.merge_marks,
            live: self.live.on,
            recent: u8::try_from(self.recent().len()).unwrap_or(u8::MAX),
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
            MenuAction::Open => self.ask_open(),
            // A submenu: its items run.
            MenuAction::Recent => {}
            MenuAction::OpenRecent(n) => self.open_recent(n),
            MenuAction::ClearRecent => self.clear_recent(),
            MenuAction::Reload => self.reload(),
            MenuAction::Fetch => self.fetch(),
            MenuAction::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                // The close lands on the next frame: a click in the system menu bar brings none.
                ctx.request_repaint();
            }
            MenuAction::Timeline => self.ui.panes.timeline = !self.ui.panes.timeline,
            MenuAction::Factory => self.ui.panes.factory = !self.ui.panes.factory,
            MenuAction::Details => self.ui.panes.details = !self.ui.panes.details,
            MenuAction::Workers => self.ui.panes.workers = !self.ui.panes.workers,
            MenuAction::Grouping(grouping) => self.set_rows(SetRowsParams { grouping: Some(grouping), ..SetRowsParams::default() }),
            MenuAction::Collapse(collapse) => {
                let collapse = if self.spec.collapse == collapse { Collapse::Off } else { collapse };
                self.set_rows(SetRowsParams { collapse: Some(collapse), ..SetRowsParams::default() });
            }
            MenuAction::FoldQuiet => self.set_fold(!self.ui.fold.on),
            MenuAction::MergeMarks => self.set_rows(SetRowsParams { merge_marks: Some(!self.spec.merge_marks), ..SetRowsParams::default() }),
            MenuAction::Fit => self.timeline.fit(),
            MenuAction::Live => self.set_live(SetLiveParams { on: Some(!self.live.on), interval_seconds: None }),
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
            // Another repository opened (#69): switched to first.
            self.sync_opening();
            let revision = self.engine.revision();
            self.status_at = Some(revision);
            if let Ok(status) = self.engine.call::<Status>(NoParams {}) {
                self.status = status;
            }
            if let Ok(spec) = self.engine.call::<GetRows>(NoParams {}) {
                self.spec = spec;
            }
            if let Ok(live) = self.engine.call::<GetLive>(NoParams {}) {
                self.live = live;
            }
        }
        if let Err(error) = self.timeline.sync(&mut self.engine) {
            self.error = Some(error);
        }
        self.timeline.restore_pending(&mut self.engine);
        // What the person chose, told again after a load or restore (#86).
        self.timeline.fold_quiet(self.ui.fold.threshold());
    }

    /// Folds the timeline's quiet time or not (#86), as View ▸ Fold Quiet Time and the toolbar's
    /// checkbox do.
    fn set_fold(&mut self, on: bool) {
        self.ui.fold.on = on;
        self.timeline.fold_quiet(self.ui.fold.threshold());
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
        self.follow_look(ui.visuals());
        self.collapse_to_window(ui.ctx());
        // Woken for the live watch's next check (#54), which the next frame's poll starts.
        if let Some(wait) = self.engine.live_wait(std::time::Instant::now()) {
            ui.ctx().request_repaint_after(wait);
        }
        if !self.native_menu {
            egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
            // ⌘O / Ctrl+O, as the system menu bar's File ▸ Open… has it.
            if ui.ctx().input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O))) {
                self.ask_open();
            }
        }
        self.opening_ui(ui.ctx());
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        if self.ui.panes.factory {
            self.step_factory(ui.ctx());
        }
        self.details.sync(&mut self.engine, &self.ui.details);
        let defaults = layout::defaults(ui.ctx().content_rect().size());
        self.layout = layout::LayoutDrawn::default();
        if self.ui.panes.details {
            self.details_ui(ui, &defaults);
        }
        egui::CentralPanel::default().show(ui, |ui| self.central(ui, &defaults));
        self.workers_window(ui.ctx());
        if let Some(hour) = self.factory.strip().drawn().chosen {
            self.show_hour(hour);
            ui.ctx().request_repaint();
        }
        let mut links = self.timeline.take_links();
        links.extend(self.factory.drawn().open);
        links.extend(self.details.drawn().open.filter(|_| self.ui.panes.details));
        for link in links {
            self.open_link(ui.ctx(), link);
        }
        self.follow(ui.ctx());
        self.reveal_details(ui.ctx());
        self.light_timeline(ui.ctx());
        self.keep(ui.ctx());
    }

    /// The timeline built in the look egui draws in (#154): `theme.set` when it changes, so its
    /// bars, strips and tooltips take the theme's light colours on a light look.
    fn follow_look(&mut self, visuals: &egui::Visuals) {
        let look = factory_chip::shade(visuals);
        if self.engine.look() != look {
            let _ = self.engine.call::<swarmception_engine::theme_cmds::Set>(swarmception_engine::theme_cmds::LookParams { look });
        }
    }

    /// Collapsed to the window (#74): the timeline's rows follow it once it settles, before the
    /// frame draws them; a frame asked for when it should look again.
    fn collapse_to_window(&mut self, ctx: &egui::Context) {
        if !self.ui.panes.timeline {
            return;
        }
        let held = ctx.input(|i| i.pointer.any_down());
        if let Some(wait) = self.timeline.collapse_to_window(&mut self.engine, std::time::Instant::now(), held) {
            ctx.request_repaint_after(wait);
        }
    }

    /// What the details are of, following the selection (#47): a bar newly chosen on the
    /// timeline (the person's own selection, not a factory hover's), a ticket newly traced or a
    /// gate run clicked in the factory, or a line, name or chip clicked in the pane, or its ←.
    fn follow(&mut self, ctx: &egui::Context) {
        let before = self.ui.details.subject.clone();
        self.follow_timeline();
        self.follow_factory();
        if let Some(subject) = self.workers.drawn().chosen.clone().filter(|_| self.ui.panes.workers) {
            self.select(subject);
        }
        if self.ui.panes.details {
            let drawn = self.details.drawn().clone();
            if drawn.back {
                self.details.back(&mut self.ui.details);
            } else if let Some(subject) = drawn.chosen {
                self.select(subject);
            }
        }
        if self.ui.details.subject != before {
            self.trace_subject();
            self.details.sync(&mut self.engine, &self.ui.details);
            ctx.request_repaint();
        }
    }

    /// What was chosen in the factory: a ticket newly traced (a chip clicked), a gate run's line
    /// or a merge's brick clicked (#85).
    fn follow_factory(&mut self) {
        let traced = self.ui.factory.ticket;
        if traced != self.followed.1 {
            self.followed.1 = traced;
            if let Some(issue) = traced {
                self.select(Subject::Ticket { issue });
            }
        }
        if !self.ui.panes.factory {
            return;
        }
        if let Some((sub, at)) = self.factory.drawn().run_clicked.clone() {
            self.select(Subject::Gate { sub, at: Some(at) });
        }
        if let Some(commit) = self.factory.drawn().merge_clicked.clone() {
            self.select(Subject::Merge { commit });
        }
    }

    /// A new selection traced in the factory (#85): a ticket's journey traced, whoever chose it
    /// (a bar on the timeline, a chip, the details), and none for anything else; followed already,
    /// so it chooses nothing again.
    fn trace_subject(&mut self) {
        self.ui.factory.ticket = match &self.ui.details.subject {
            Some(Subject::Ticket { issue }) => Some(*issue),
            _ => None,
        };
        self.followed.1 = self.ui.factory.ticket;
    }

    /// A bar newly chosen on the timeline: the details of what `details.get` reads it as (its
    /// ticket, or its merge onto main).
    fn follow_timeline(&mut self) {
        let bar = self.timeline.chosen_bar();
        if bar == self.followed.0 {
            return;
        }
        self.followed.0.clone_from(&bar);
        let Some((track, range)) = bar else { return };
        let params = DetailsParams { track: Some(track), range: Some(range), ..DetailsParams::default() };
        if let Ok(found) = self.engine.call::<GetDetails>(params) {
            self.select(found.subject);
        }
    }

    /// The hour starting at `start`, its bar clicked in the factory's sparkline (#31): the
    /// timeline's window on it, and its playhead (the factory following it) at the hour's last
    /// second, so the factory shows what the hour ended with; the strip keeps its range, the hour
    /// marked (#85).
    fn show_hour(&mut self, start: swarmception_model::Timestamp) {
        let end = swarmception_model::Timestamp(start.0.saturating_add(3_600));
        let t = swarmception_model::Timestamp(end.0.saturating_sub(1));
        self.factory.keep_strip(start);
        self.timeline.show_span(&mut self.engine, start, end);
        self.set_factory_at(t);
        self.timeline.put_playhead(&mut self.engine, t);
        self.playhead_to = None;
    }

    /// The timeline's highlight, apart from the person's selection: the ticket hovered in the
    /// factory while it is (#48), else a worker's hovered in the Workers pane (#67), else what the
    /// details pane shows (#55, `details.get`'s `timeline`); the pane tells the widget only when
    /// it changes.
    fn light_timeline(&mut self, ctx: &egui::Context) {
        let factory = if self.ui.panes.factory { self.factory.hovered() } else { None };
        let worker = if self.ui.panes.workers { self.workers.hovered().clone() } else { Hover::Nothing };
        let hovered = factory.map_or(worker, Hover::Ticket);
        let details = self.details.on_timeline(&self.ui.details).filter(|_| self.ui.panes.details).map(|on| on.bars.as_slice()).unwrap_or_default();
        if self.timeline.light_hover(&mut self.engine, hovered, details) {
            ctx.request_repaint();
        }
    }

    /// The Details pane on the right (#47): resizable, or folded to a strip (#68).
    fn details_ui(&mut self, ui: &mut egui::Ui, defaults: &layout::Defaults) {
        let folded = self.ui.folded.details.or(defaults.folded.details).unwrap_or(false);
        let wanted = self.ui.sizes.details.map_or(defaults.details, points_f32);
        let shown = if folded {
            let shown = egui::Panel::right("details-strip").resizable(false).exact_size(layout::FOLDED_STRIP).show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    let chevron = ui.add(egui::Button::new(icons::CHEVRON_LEFT).frame(false)).on_hover_text("Open the Details");
                    let icon =
                        ui.add(egui::Label::new(egui::RichText::new(icons::INFO).strong()).sense(egui::Sense::click())).on_hover_text("Open the Details");
                    chevron.clicked() || icon.clicked()
                })
                .inner
            });
            if shown.inner {
                self.ui.folded.details = Some(false);
            }
            shown.response.rect.width()
        } else {
            let width = ui.available_width();
            let shown = egui::Panel::right("details")
                .resizable(true)
                .default_size(wanted)
                .size_range(220.0..=(width - layout::CENTRAL_LEAST_WIDTH).max(220.0))
                .show(ui, |ui| self.details.ui(ui, &self.ui.details));
            self.ui.sizes.details = Some(whole_points(shown.response.rect.width()));
            if self.details.drawn().fold {
                self.ui.folded.details = Some(true);
            }
            shown.response.rect.width()
        };
        self.layout.details = Some(layout::BandDrawn {
            name: "details".into(),
            size: whole_points(shown),
            wanted: whole_points(wanted),
            folded,
            default_fold: self.ui.folded.details.is_none(),
        });
    }

    /// The workers pane (#67), a window over the others when it shows: `workers.compare` for the
    /// timeline's window (the whole run when asked, or with no timeline), when the window or the
    /// data moved on; its × closes it as View ▸ Workers does.
    fn workers_window(&mut self, ctx: &egui::Context) {
        if !self.ui.panes.workers {
            self.workers.leave();
            return;
        }
        let windowed = self.ui.panes.timeline && self.timeline.has_data();
        let window = if windowed && !self.ui.workers.whole_run { self.timeline.looking(&mut self.engine).and_then(|l| l.window) } else { None };
        self.workers.sync(&mut self.engine, window);
        let mut open = true;
        let before = self.ui.workers;
        self.workers.show(ctx, &mut self.ui.workers, &mut open, windowed, self.ui.details.subject.as_ref());
        self.ui.panes.workers = open;
        if self.ui.workers != before {
            ctx.request_repaint();
        }
    }

    /// The central area (#68's rule, [`layout`]): the timeline, under it the bands (the factory),
    /// each as shown, folded or not, sharing the height by
    /// [`layout::share`]: the timeline keeps its least while the bands can give way. Without the
    /// timeline the last open band fills the rest.
    fn central(&mut self, ui: &mut egui::Ui, defaults: &layout::Defaults) {
        let panes = self.ui.panes;
        if !panes.timeline && !panes.factory {
            ui.centered_and_justified(|ui| ui.weak("No pane is shown: View ▸ Timeline or Factory shows one."));
            return;
        }
        let folded = self.ui.folded.or(defaults.folded);
        let mut bands: Vec<BandKind> = BandKind::ALL.into_iter().filter(|kind| kind.shown(panes)).collect();
        // What fills the rest: the timeline, else the last open band.
        let filler = if panes.timeline { None } else { bands.iter().rposition(|kind| !kind.folded(folded)).map(|at| bands.remove(at)) };
        let least = if panes.timeline { self.timeline.least() } else { filler.map_or(0.0, BandKind::least) };
        let specs: Vec<layout::Band> = bands
            .iter()
            .map(|kind| layout::Band {
                wanted: kind.size(self.ui.sizes).map_or(kind.default(defaults), points_f32),
                least: kind.least(),
                folded: kind.folded(folded),
            })
            .collect();
        let heights = layout::share(ui.available_height(), least, &specs);
        let mut drawn = Vec::new();
        // From the bottom up: each band a panel at its share, under the one before.
        for ((kind, spec), height) in bands.iter().zip(&specs).zip(&heights).rev() {
            let shown = self.band(ui, *kind, *height, spec);
            drawn.push(layout::BandDrawn {
                name: kind.name().into(),
                size: whole_points(shown),
                wanted: whole_points(spec.wanted),
                folded: spec.folded,
                default_fold: kind.chosen(self.ui.folded).is_none(),
            });
        }
        drawn.reverse();
        self.layout.timeline_least = whole_points(least);
        if panes.timeline {
            self.layout.timeline = Some(whole_points(ui.available_height()));
            self.timeline.ui(ui, &self.status);
        } else if let Some(kind) = filler {
            let rest = whole_points(ui.available_height());
            drawn.push(layout::BandDrawn {
                name: kind.name().into(),
                size: rest,
                wanted: rest,
                folded: false,
                default_fold: kind.chosen(self.ui.folded).is_none(),
            });
            self.band_contents(ui, kind);
        }
        self.layout.bands = drawn;
    }

    /// Band `kind` as a panel `height` high at the bottom of what is left: its contents, or its
    /// header alone when folded; open, its top edge drags its height. Its height drawn.
    fn band(&mut self, ui: &mut egui::Ui, kind: BandKind, height: f32, spec: &layout::Band) -> f32 {
        let shown = egui::Panel::bottom(kind.name()).resizable(false).exact_size(height).show(ui, |ui| {
            if spec.folded {
                ui.horizontal_centered(|ui| {
                    if layout::fold_title(ui, kind.icon(), kind.title(), layout::Chevron::Folded).clicked() {
                        kind.set_folded(&mut self.ui.folded, false);
                    }
                });
            } else {
                self.band_contents(ui, kind);
            }
        });
        let rect = shown.response.rect;
        if !spec.folded {
            let edge = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() - 3.0), egui::pos2(rect.right(), rect.top() + 3.0));
            let response = ui.interact(edge, ui.scope_id().with(("band-edge", kind.name())), egui::Sense::drag());
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                ui.painter().hline(rect.x_range(), rect.top(), ui.visuals().widgets.hovered.fg_stroke);
            }
            if response.dragged() && response.drag_delta().y != 0.0 {
                let wanted = (rect.height() - response.drag_delta().y).max(spec.least);
                kind.set_size(&mut self.ui.sizes, Some(whole_points(wanted)));
            }
        }
        rect.height()
    }

    /// What band `kind` draws open; a click on its title folds it.
    fn band_contents(&mut self, ui: &mut egui::Ui, kind: BandKind) {
        let fold = match kind {
            BandKind::Factory => {
                self.factory.ui(ui, &mut self.ui.factory, &self.status);
                self.factory.fold_asked()
            }
        };
        if fold {
            kind.set_folded(&mut self.ui.folded, true);
        }
    }

    /// How the panes shared the window last frame (#68, `ui.inspect`'s `layout`).
    #[must_use]
    pub const fn layout(&self) -> &layout::LayoutDrawn {
        &self.layout
    }

    /// A new subject in the details pane (from the pane, the factory, the timeline, `ui.set` or
    /// `--details`): its bars brought into view on the timeline when none is (#55), once its
    /// details have come; the selection untouched.
    fn reveal_details(&mut self, ctx: &egui::Context) {
        if self.ui.details.subject.is_none() {
            self.revealed = None;
        }
        if !self.ui.panes.details || !self.timeline.has_data() || self.ui.details.subject == self.revealed {
            return;
        }
        let Some(on) = self.details.on_timeline(&self.ui.details).cloned() else { return };
        self.revealed.clone_from(&self.ui.details.subject);
        self.timeline.reveal(&mut self.engine, &on);
        ctx.request_repaint();
    }

    /// The factory's time this frame (#85: the timeline's playhead, which a step, play or `--at`
    /// moves first), what it has selected (the details' subject), and the scene at that time.
    fn step_factory(&mut self, ctx: &egui::Context) {
        let jumped = self.factory.jump(&mut self.engine, &mut self.ui.factory);
        if jumped.is_some() {
            ctx.request_repaint();
        }
        if let Some(t) = jumped.or_else(|| self.playhead_to.filter(|_| self.timeline.has_data())) {
            self.timeline.put_playhead(&mut self.engine, t);
            self.playhead_to = None;
        }
        let looking = self.timeline.looking(&mut self.engine);
        if let Some(t) = self.factory.step(ctx, &mut self.engine, &mut self.ui.factory, looking) {
            self.timeline.put_playhead(&mut self.engine, t);
        }
        self.factory.select(self.ui.details.subject.as_ref().and_then(factory_paint::Selected::of));
        let recent = factory::recent(&self.ui.factory, looking);
        self.factory.sync(&mut self.engine, &self.ui.factory, recent);
        self.factory.sync_stats(&mut self.engine, looking);
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let state = self.menu_state();
        egui::MenuBar::new().ui(ui, |ui| {
            let file = ui.menu_button("File", |ui| self.menu_items(ui, state, &MenuAction::FILE));
            self.note_menu(Some(file.response.rect), None);
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
            if action == MenuAction::Recent {
                let shown = ui.menu_button(format!("{}  {}", action.icon(), action.label()), |ui| self.recent_menu(ui, state));
                self.note_menu(None, Some(shown.response.rect));
                continue;
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
            self.live_toggle(ui);
            ui.separator();
            let mut spec = self.spec;
            ui.selectable_value(&mut spec.grouping, Grouping::Flat, icons::FLAT).on_hover_text("Tickets packed onto shared rows, in the order they began");
            ui.selectable_value(&mut spec.grouping, Grouping::Workers, icons::WORKERS).on_hover_text("Tickets by worker: machine / worker / its rows");
            ui.separator();
            let mut collapse = |ui: &mut egui::Ui, mode: Collapse, icon: &str, hover: &str| {
                let on = spec.collapse == mode;
                if ui.selectable_label(on, icon).on_hover_text(hover).clicked() {
                    spec.collapse = if on { Collapse::Off } else { mode };
                }
            };
            collapse(ui, Collapse::Empty, icons::COLLAPSE, "Collapse: leave out rows with no events at all");
            collapse(ui, Collapse::Window, icons::COLLAPSE_WINDOW, "Collapse to the window: leave out rows with nothing in the visible window, as it settles");
            if spec != self.spec {
                self.set_rows(SetRowsParams { grouping: Some(spec.grouping), collapse: Some(spec.collapse), merge_marks: Some(spec.merge_marks) });
            }
            let mut fold = self.ui.fold.on;
            let minutes = self.ui.fold.after_seconds.unwrap_or(swarmception_domain::stats::QUIET) / 60;
            let hover =
                format!("Fold quiet time: each stretch where nothing happened for over {minutes} min drawn as a narrow marker; the ruler jumps over it");
            if ui.checkbox(&mut fold, format!("{}  Fold quiet time", icons::FOLD)).on_hover_text(hover).changed() {
                self.set_fold(fold);
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
            // First, where it is seen however narrow the window: it goes once read.
            self.saved_note(ui);
            let s = &self.status;
            match &s.repo {
                Some(repo) => ui.label(format!("{}  {repo}", icons::FOLDER)),
                None => ui.weak("No repository: File ▸ Open… (⌘O) opens a swarm's clone"),
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
            self.live_note(ui);
            self.issues_note(ui);
            let s = &self.status;
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

/// The intervals the toolbar offers a live watch, in seconds.
pub const LIVE_INTERVALS: [u64; 5] = [10, 30, 60, 120, 300];

/// An interval as the toolbar shows it: `30 s`, `2 min`.
#[must_use]
pub fn interval_text(seconds: u64) -> String {
    if seconds >= 60 && seconds.is_multiple_of(60) { format!("{} min", seconds / 60) } else { format!("{seconds} s") }
}

/// A time of day, `HH:MM:SS`, as the status bar shows it (UTC).
fn time_of_day(t: swarmception_model::Timestamp) -> String {
    let text = t.to_string();
    text.get(11..19).map_or(text.clone(), str::to_owned)
}

impl Shell {
    /// The toolbar's Live toggle and, while it is on, its interval (#54): `live.set`.
    fn live_toggle(&mut self, ui: &mut egui::Ui) {
        let mut on = self.live.on;
        let hover = format!(
            "Live: fetch the remote's tracking and main every {}, off the UI thread, and reload when either moved; the view stays, or follows the end when it was there",
            interval_text(self.live.interval_seconds)
        );
        if ui.add_enabled(self.status.repo.is_some(), egui::Button::selectable(on, format!("{}  Live", icons::LIVE))).on_hover_text(hover).clicked() {
            on = !on;
            self.set_live(SetLiveParams { on: Some(on), interval_seconds: None });
        }
        if !self.live.on {
            return;
        }
        let mut interval = self.live.interval_seconds;
        egui::ComboBox::from_id_salt("live-interval").width(56.0).selected_text(interval_text(interval)).show_ui(ui, |ui| {
            for seconds in LIVE_INTERVALS {
                ui.selectable_value(&mut interval, seconds, interval_text(seconds));
            }
        });
        if interval != self.live.interval_seconds {
            self.set_live(SetLiveParams { on: None, interval_seconds: Some(interval) });
        }
    }

    /// The status bar's word on the live watch (#54): live and when it last checked, paused, or
    /// why its last check failed and when it tries again; one segment, the error on hover.
    fn live_note(&self, ui: &mut egui::Ui) {
        let live = &self.live;
        if self.status.repo.is_none() {
            return;
        }
        ui.separator();
        let checked = live.last_check.map(|t| format!(" · checked {}", time_of_day(t))).unwrap_or_default();
        let mut hover = vec![format!("Every {}: fetches tracking and main, and reloads when either moved.", interval_text(live.interval_seconds))];
        hover.extend(live.last_new.map(|t| format!("Last new events: {} UTC", time_of_day(t))));
        if let Some(cost) = live.cost {
            let mut parts = vec![format!("fetch {} ms", cost.fetch_ms)];
            parts.extend(cost.read_ms.map(|ms| format!("read {ms} ms")));
            parts.extend(cost.build_ms.map(|ms| format!("build {ms} ms")));
            parts.extend(cost.new_events.map(|n| format!("{n} new events")));
            hover.push(format!("Last check: {}", parts.join(", ")));
        }
        let label = match (&live.error, live.on) {
            (Some(error), true) => {
                let retry = live
                    .last_check
                    .zip(live.next_in_seconds)
                    .map(|(t, s)| format!(" · retry at {}", time_of_day(swarmception_model::Timestamp(t.0.saturating_add(i64::try_from(s).unwrap_or(0))))));
                hover.push(error.clone());
                let failed = if live.failures > 1 { format!("check failed ×{}", live.failures) } else { "check failed".to_owned() };
                ui.colored_label(ui.visuals().warn_fg_color, format!("{}  live · {failed}{}  {}", icons::LIVE, retry.unwrap_or_default(), icons::WARNING))
            }
            (_, true) => ui.label(format!("{}  live{checked}", icons::LIVE)),
            (_, false) => ui.weak(format!("{}  paused{checked}", icons::LIVE)),
        };
        label.on_hover_text(hover.join("\n"));
    }

    /// The status bar's word on the saved state (#53), said once: what of the file was left out
    /// and why, or why it couldn't be written; a click puts it away.
    fn saved_note(&mut self, ui: &mut egui::Ui) {
        let Some(note) = self.keeper.as_ref().and_then(saved::Keeper::note) else { return };
        let short: String = note.chars().take(120).collect();
        let short = if short.len() < note.len() { format!("{short}…") } else { short };
        let file = self.saved().file.unwrap_or_default();
        let full = format!("{note}\n{file}\n(click to dismiss)");
        let text = egui::RichText::new(format!("{}  {short}", icons::WARNING)).color(ui.visuals().warn_fg_color);
        if ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text(full).clicked()
            && let Some(keeper) = &mut self.keeper
        {
            keeper.dismiss();
        }
        ui.separator();
    }

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

/// A size in whole points, as [`PaneSizes`] keeps it.
/// A band under the timeline (#68), top to bottom in [`BandKind::ALL`]: a pane that slots in
/// under the timeline adds itself here, with its least height, default, fold and size, and
/// [`layout::share`] gives it its room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BandKind {
    Factory,
}

impl BandKind {
    const ALL: [Self; 1] = [Self::Factory];

    const fn name(self) -> &'static str {
        match self {
            Self::Factory => "factory",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Factory => "Factory",
        }
    }

    const fn icon(self) -> &'static str {
        match self {
            Self::Factory => icons::FACTORY,
        }
    }

    const fn shown(self, panes: Panes) -> bool {
        match self {
            Self::Factory => panes.factory,
        }
    }

    /// The least it shrinks to while open: the factory's header and a floor.
    const fn least(self) -> f32 {
        match self {
            Self::Factory => 140.0,
        }
    }

    const fn default(self, defaults: &layout::Defaults) -> f32 {
        match self {
            Self::Factory => defaults.factory,
        }
    }

    const fn size(self, sizes: PaneSizes) -> Option<u32> {
        match self {
            Self::Factory => sizes.factory,
        }
    }

    const fn set_size(self, sizes: &mut PaneSizes, size: Option<u32>) {
        match self {
            Self::Factory => sizes.factory = size,
        }
    }

    /// The person's choice of fold, if they made one.
    const fn chosen(self, folded: layout::Folded) -> Option<bool> {
        match self {
            Self::Factory => folded.factory,
        }
    }

    /// Whether it is folded, the choices and defaults merged in `folded`.
    fn folded(self, folded: layout::Folded) -> bool {
        self.chosen(folded).unwrap_or(false)
    }

    const fn set_folded(self, folded: &mut layout::Folded, fold: bool) {
        match self {
            Self::Factory => folded.factory = Some(fold),
        }
    }
}

fn whole_points(size: f32) -> u32 {
    if size.is_finite() { size.round().clamp(0.0, 16_384.0) as u32 } else { 0 }
}

/// A [`PaneSizes`] size as egui takes it.
const fn points_f32(size: u32) -> f32 {
    size as f32
}

fn icon_button(icon: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(icon).size(15.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_state_reads_back_and_defaults() {
        let state = UiState { panes: Panes { timeline: false, factory: true, details: false, workers: true }, ..UiState::default() };
        let text = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<UiState>(&text).unwrap(), state);
        assert_eq!(serde_json::from_str::<UiState>("{}").unwrap(), UiState::default());
        assert!(serde_json::from_str::<UiState>(r#"{"nope":1}"#).is_err());
        // Version 1, before the factory, still reads, with the factory's defaults.
        let old: UiState = serde_json::from_str(r#"{"version":1,"panes":{"timeline":false}}"#).unwrap();
        assert_eq!((old.panes.timeline, old.panes.factory, old.factory), (false, true, FactoryView::default()));
        assert!(old.panes.details && old.details.subject.is_none(), "the details pane, with nothing selected");
        let selected: UiState = serde_json::from_str(r#"{"version":3,"details":{"subject":{"ticket":{"issue":47}}}}"#).unwrap();
        assert_eq!(selected.details.subject, Some(Subject::Ticket { issue: 47 }));
        // Version 4: the factory's height kept.
        let four: UiState = serde_json::from_str(r#"{"version":4,"panes":{"details":false},"sizes":{"factory":300}}"#).unwrap();
        assert_eq!(four.sizes.factory, Some(300));
        // Version 6, before the workers pane (#67): hidden, over the timeline's window, unsorted.
        let five: UiState = serde_json::from_str(r#"{"version":6,"panes":{"details":false}}"#).unwrap();
        assert_eq!((five.panes.workers, five.workers), (false, WorkersView::default()));
        let sorted = UiState {
            panes: Panes { workers: true, ..Panes::default() },
            workers: WorkersView { whole_run: true, sort: Some(WorkersColumn::Rate), ascending: false, place: Some(WorkersPlace { x: 40, y: -12 }) },
            ..UiState::default()
        };
        assert_eq!(serde_json::from_str::<UiState>(&serde_json::to_string(&sorted).unwrap()).unwrap(), sorted);
        // Version 7, before the window's place (#76): at its default place, its sort kept.
        let seven: UiState = serde_json::from_str(r#"{"version":7,"workers":{"whole_run":true,"sort":"landed","ascending":true}}"#).unwrap();
        assert_eq!(seven.workers, WorkersView { whole_run: true, sort: Some(WorkersColumn::Landed), ascending: true, place: None });
        let hidden = UiState {
            panes: Panes { factory: false, ..Panes::default() },
            sizes: PaneSizes { factory: Some(180), ..PaneSizes::default() },
            ..UiState::default()
        };
        assert_eq!(serde_json::from_str::<UiState>(&serde_json::to_string(&hidden).unwrap()).unwrap(), hidden);
    }

    #[test]
    fn menu_items_run_the_same_commands_the_state_shows() {
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, None);
        let state = shell.menu_state();
        assert_eq!(
            state,
            MenuState {
                can_reload: false,
                can_fit: false,
                timeline: true,
                factory: true,
                details: true,
                workers: false,
                grouping: Grouping::Flat,
                collapse: Collapse::Off,
                fold_quiet: false,
                merge_marks: false,
                live: false,
                recent: 0
            }
        );
        // Live is `live.set`, and the menu's mark is what `live.get` says.
        shell.run_menu(&ctx, MenuAction::Live);
        assert!(shell.menu_state().live && shell.live().on && !shell.live().default);
        shell.run_menu(&ctx, MenuAction::Live);
        assert!(!shell.menu_state().live);

        shell.run_menu(&ctx, MenuAction::Timeline);
        shell.run_menu(&ctx, MenuAction::Factory);
        shell.run_menu(&ctx, MenuAction::Grouping(Grouping::Workers));
        shell.run_menu(&ctx, MenuAction::Collapse(Collapse::Empty));
        let state = shell.menu_state();
        assert!(!state.timeline);
        assert!(!state.factory);
        assert_eq!(state.checked(MenuAction::Grouping(Grouping::Workers)), Some(true));
        assert_eq!(state.checked(MenuAction::Collapse(Collapse::Empty)), Some(true));
        assert_eq!(state.checked(MenuAction::Collapse(Collapse::Window)), Some(false));
        assert!(!shell.ui_state().panes.timeline);
        // What the menu shows is what `rows.get` returns.
        let spec = shell.engine().call::<GetRows>(NoParams {}).unwrap();
        assert_eq!((spec.grouping, spec.collapse), (state.grouping, state.collapse));

        // To the window (#74): the one item checked; choosing the checked one turns it off.
        shell.run_menu(&ctx, MenuAction::Collapse(Collapse::Window));
        let state = shell.menu_state();
        assert_eq!(shell.engine().call::<GetRows>(NoParams {}).unwrap().collapse, Collapse::Window);
        assert_eq!(state.checked(MenuAction::Collapse(Collapse::Empty)), Some(false));
        assert_eq!(state.checked(MenuAction::Collapse(Collapse::Window)), Some(true));
        shell.run_menu(&ctx, MenuAction::Collapse(Collapse::Window));
        assert_eq!(shell.menu_state().collapse, Collapse::Off);

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
