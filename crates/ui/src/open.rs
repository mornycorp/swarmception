//! Opening a repository from the window (#69, G8): File ▸ Open… asks the host's folder picker
//! ([`FolderPicker`]: AppKit's open panel on macOS; without one, a path typed into the window)
//! and opens the folder through the engine's `repo.open`; File ▸ Open Recent lists `repo.recent`
//! (kept app-wide in the state directory's `recent.json`, [`crate::saved::StateStore::load_recent`])
//! with Clear Menu (`repo.clearRecent`). A folder the engine refuses (missing, not a git
//! repository, no tracking branch) is a message in the window, and the repository open stays.
//!
//! Whatever opens a repository (the menus, the control channel's `repo.open`, MCP), the shell
//! notices the engine's [`Session::opens`] move and switches to it: the old repository's state
//! saved first (when the shell opened it), the panes afresh, then the new one's own saved state
//! restored (#53 keys it per repository), Live included.
//!
//! [`Session::opens`]: swarmception_engine::Session::opens

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_engine::EngineError;
use swarmception_engine::engine_cmds::NoParams;
use swarmception_engine::repo_cmds::{ClearRecent, Open, OpenParams, Recent, RecentRepo, StatusResult};

use crate::menu::{MenuAction, MenuState, recent_label};
use crate::{DetailsPane, DetailsView, FactoryPane, FactoryView, Shell, TimelinePane, UiState, WorkersPane, icons, saved};

/// The host's folder picker: shown starting in a folder (the open repository's parent), what it
/// did. The app's is AppKit's open panel on macOS, elsewhere the desktop's own picker as a child
/// process (#76: zenity, kdialog, PowerShell's folder browser); a test's answers at once.
pub type FolderPicker = Box<dyn FnMut(Option<&Path>) -> Picked>;

/// What the host's folder picker did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    /// The folder chosen.
    Folder(PathBuf),
    /// The person cancelled it: nothing opens, nothing is said.
    Cancelled,
    /// No picker could be shown (none installed, no display): the window asks for a path itself.
    NoPicker,
}

/// A folder `repo.open` refused, as the window says it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Refused {
    /// The folder asked for.
    pub path: String,
    /// Why, as the engine says it.
    pub why: String,
}

/// What `ui.inspect` says of opening (#69): the refusal shown, the path being typed, the recent
/// list as Open Recent shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenInspect {
    pub refused: Option<Refused>,
    /// The window's own path field (no folder picker), while it shows.
    pub prompt: Option<String>,
    /// Open Recent's items, newest first.
    pub recent: Vec<String>,
    /// Where the recent list is kept, when it is.
    pub recent_file: Option<String>,
}

/// The shell's part in opening repositories.
#[derive(Default)]
pub(crate) struct Opening {
    picker: Option<FolderPicker>,
    /// `repo.recent`, as last read.
    recent: Vec<RecentRepo>,
    /// What `recent.json` holds: the list last written or read.
    written: Option<Vec<String>>,
    /// The engine's [`swarmception_engine::Session::opens`] the shell has switched to.
    pub(crate) opens: u64,
    refused: Option<Refused>,
    prompt: Option<String>,
    /// Where the recent list is kept: the state's directory, when the shell keeps state.
    store: Option<saved::StateStore>,
    home: Option<PathBuf>,
    menu: MenuDrawn,
}

/// Where the window's own menu bar drew File and, while File was open, its Open Recent item:
/// for a host to put the pointer there (the app's `--menu`, a screenshot of the menu open).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MenuDrawn {
    pub file: Option<egui::Rect>,
    pub recent: Option<egui::Rect>,
}

impl Shell {
    /// Has File ▸ Open… ask `picker` for a folder (the app's AppKit open panel); without one the
    /// window asks for a path itself.
    pub fn set_folder_picker(&mut self, picker: FolderPicker) {
        self.opening.picker = Some(picker);
    }

    /// The repositories Open Recent lists, newest first (`repo.recent`, as last read).
    #[must_use]
    pub fn recent(&self) -> &[RecentRepo] {
        &self.opening.recent
    }

    /// Open Recent's item for its `n`th repository, as the menus show it.
    #[must_use]
    pub fn recent_label(&self, n: u8) -> Option<String> {
        self.opening.recent.get(usize::from(n)).map(|repo| recent_label(repo, self.opening.home.as_deref()))
    }

    /// Where the window's own menu bar drew File and Open Recent last.
    #[must_use]
    pub const fn menu_drawn(&self) -> MenuDrawn {
        self.opening.menu
    }

    pub(crate) fn note_menu(&mut self, file: Option<egui::Rect>, recent: Option<egui::Rect>) {
        self.opening.menu.file = file.or(self.opening.menu.file);
        self.opening.menu.recent = recent.or(self.opening.menu.recent);
    }

    /// The folder `repo.open` last refused, while the window says so.
    #[must_use]
    pub const fn refused(&self) -> Option<&Refused> {
        self.opening.refused.as_ref()
    }

    /// What `ui.inspect` says of opening.
    #[must_use]
    pub fn open_inspect(&self) -> OpenInspect {
        OpenInspect {
            refused: self.opening.refused.clone(),
            prompt: self.opening.prompt.clone(),
            recent: (0..self.opening.recent.len()).filter_map(|n| u8::try_from(n).ok()).filter_map(|n| self.recent_label(n)).collect(),
            recent_file: self.opening.store.as_ref().map(|s| s.recent_file().display().to_string()),
        }
    }

    /// Opens the repository `path` is in (`repo.open`), the open one's state saved first; a
    /// refusal is said in the window and changes nothing.
    ///
    /// # Errors
    ///
    /// `repo.open`'s refusal.
    pub fn open_repo(&mut self, path: &str) -> Result<StatusResult, EngineError> {
        self.opening.prompt = None;
        if let Err(why) = self.save_state() {
            self.error = Some(why);
        }
        let result = self.engine.call::<Open>(OpenParams { path: path.to_owned() });
        match &result {
            Ok(_) => {
                self.opening.refused = None;
                self.sync();
            }
            Err(error) => self.opening.refused = Some(Refused { path: path.to_owned(), why: error.to_string() }),
        }
        result
    }

    /// File ▸ Open…: the host's picker, from the open repository's parent; else, or when it
    /// couldn't show one, the window's path field.
    pub(crate) fn ask_open(&mut self) {
        let start = self.engine.repo().and_then(Path::parent).map(Path::to_path_buf);
        let picked = self.opening.picker.as_mut().map_or(Picked::NoPicker, |pick| pick(start.as_deref()));
        match picked {
            Picked::Folder(folder) => {
                let _ = self.open_repo(&folder.display().to_string());
            }
            Picked::Cancelled => {}
            Picked::NoPicker => self.opening.prompt = Some(start.map(|s| format!("{}/", s.display())).unwrap_or_default()),
        }
    }

    /// File ▸ Open Recent ▸ its `n`th repository.
    pub(crate) fn open_recent(&mut self, n: u8) {
        if let Some(repo) = self.opening.recent.get(usize::from(n)).cloned() {
            let _ = self.open_repo(&repo.path);
        }
    }

    /// File ▸ Open Recent ▸ Clear Menu.
    pub(crate) fn clear_recent(&mut self) {
        let result = self.engine.call::<ClearRecent>(NoParams {});
        self.note(result);
        self.sync();
    }

    /// Where the recent list is kept from now on (the state's directory), what it held put after
    /// the repositories opened this run.
    pub(crate) fn keep_recent(&mut self, store: &saved::StateStore) {
        if self.opening.store.as_ref() != Some(store) {
            let kept = store.load_recent();
            self.opening.written = Some(kept.iter().map(|p| p.display().to_string()).collect());
            self.engine.remember_all(kept);
            self.opening.store = Some(store.clone());
        }
        self.opening.home = std::env::var_os("HOME").map(PathBuf::from);
        self.status_at = None;
    }

    /// In [`Shell::sync`], when the engine's revision moved: another repository opened (switch to
    /// it), and the recent list read again and written when it changed.
    pub(crate) fn sync_opening(&mut self) {
        if self.engine.opens() != self.opening.opens {
            self.switch_repo();
        }
        if let Ok(recent) = self.engine.call::<Recent>(NoParams {}) {
            self.opening.recent = recent.repos;
        }
        let paths: Vec<String> = self.opening.recent.iter().map(|r| r.path.clone()).collect();
        if let Some(store) = &self.opening.store
            && self.opening.written.as_ref() != Some(&paths)
        {
            if let Err(why) = store.save_recent(&paths) {
                self.error = Some(why);
            }
            self.opening.written = Some(paths);
        }
    }

    /// The engine opened another repository: the panes afresh (the panes shown, their sizes and
    /// folds kept), then its own saved state restored and kept, when the shell keeps state.
    fn switch_repo(&mut self) {
        self.opening.opens = self.engine.opens();
        self.ui = UiState { factory: FactoryView::default(), details: DetailsView::default(), ..self.ui.clone() };
        self.timeline = TimelinePane::default();
        self.factory = FactoryPane::default();
        self.details = DetailsPane::default();
        self.workers = WorkersPane::default();
        self.followed = (None, None);
        self.revealed = None;
        self.playhead_to = None;
        self.error = None;
        self.status_at = None;
        let (Some(keeper), Some(repo)) = (self.keeper.take(), self.engine.repo()) else { return };
        let store = keeper.store().clone();
        let key = saved::repo_key(repo);
        let loaded = store.load(&key);
        // The window stays where it is: its place isn't another repository's to move.
        let place = self.window_place;
        self.keep_state(store, &key, Some(&loaded));
        self.window_place = place.or(self.window_place);
    }

    /// Open Recent's submenu in the window's own menu bar: each repository, then Clear Menu.
    pub(crate) fn recent_menu(&mut self, ui: &mut egui::Ui, state: MenuState) {
        if self.opening.recent.is_empty() {
            ui.add_enabled(false, egui::Label::new("No Recent Repositories"));
        }
        for item in MenuAction::recent_items(self.opening.recent.len()) {
            let Some(action) = item else {
                ui.separator();
                continue;
            };
            let label = match action {
                MenuAction::OpenRecent(n) => self.recent_label(n).unwrap_or_default(),
                other => other.label().to_owned(),
            };
            let button = ui.add_enabled(state.enabled(action), egui::Button::new(format!("{}  {label}", action.icon())));
            let path = match action {
                MenuAction::OpenRecent(n) => self.opening.recent.get(usize::from(n)).map(|r| r.path.clone()),
                _ => None,
            };
            let button = match path {
                Some(path) => button.on_hover_text(path),
                None => button,
            };
            if button.clicked() {
                self.run_menu(ui.ctx(), action);
                ui.close();
            }
        }
    }

    /// The refusal's message, and the window's own path field when there is no folder picker.
    pub(crate) fn opening_ui(&mut self, ctx: &egui::Context) {
        if let Some(refused) = self.opening.refused.clone() {
            let still = self.status.repo.clone();
            let (mut close, mut again) = (false, false);
            egui::Window::new(format!("{}  Can't open that folder", icons::WARNING))
                .id(egui::Id::unique("open-refused"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.set_max_width(460.0);
                    ui.label(egui::RichText::new(&refused.path).strong());
                    ui.add_space(4.0);
                    // The path once: "That folder is not in a git repository…".
                    let why = refused.why.strip_prefix(refused.path.as_str()).map_or_else(|| refused.why.clone(), |rest| format!("That folder{rest}."));
                    ui.label(why);
                    ui.add_space(4.0);
                    match &still {
                        Some(repo) => ui.weak(format!("{repo} stays open.")),
                        None => ui.weak("Open a swarm's clone: a git repository with a tracking branch."),
                    };
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        close = ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter));
                        again = ui.button(format!("{}  Open…", icons::FOLDER)).clicked();
                    });
                });
            if close || again {
                self.opening.refused = None;
            }
            if again {
                self.ask_open();
            }
        }
        let Some(mut text) = self.opening.prompt.clone() else { return };
        let (mut open, mut cancel) = (false, false);
        egui::Window::new(format!("{}  Open a Repository", icons::FOLDER))
            .id(egui::Id::unique("open-prompt"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("A folder in a swarm's clone (a git repository with a tracking branch):");
                let field = ui.add(egui::TextEdit::singleline(&mut text).desired_width(420.0));
                field.request_focus();
                open = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    open |= ui.button("Open").clicked();
                    cancel = ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
                });
            });
        if cancel {
            self.opening.prompt = None;
        } else if open {
            let _ = self.open_repo(text.trim());
        } else {
            self.opening.prompt = Some(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_with_no_picker_asks_in_the_window_and_cancels() {
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, None);
        shell.run_menu(&ctx, MenuAction::Open);
        assert_eq!(shell.open_inspect().prompt.as_deref(), Some(""), "no repository: an empty path to type");
        shell.opening.prompt = None;
        // A picker cancelled: nothing opens, nothing is said.
        shell.set_folder_picker(Box::new(|_| Picked::Cancelled));
        shell.run_menu(&ctx, MenuAction::Open);
        assert_eq!(shell.open_inspect(), OpenInspect::default());
        // A picker that couldn't show (none installed, no display): the window asks (#76).
        shell.set_folder_picker(Box::new(|_| Picked::NoPicker));
        shell.run_menu(&ctx, MenuAction::Open);
        assert_eq!(shell.open_inspect().prompt.as_deref(), Some(""));
    }
}
