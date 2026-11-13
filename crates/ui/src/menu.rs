//! The menus as data: every item is a [`MenuAction`] with a stable id, and [`MenuState`] says
//! which are enabled and checked. The in-window menu bar and macOS's system menu bar draw the
//! same items and run them through the same [`crate::Shell::run_menu`].
//!
//! **The engine's commands are placed by the registry** (#160, swarm-core's contract: the UI
//! builds its menus and shortcuts from the registry): a command whose `MenuPlace` puts it in a
//! menu is a [`MenuAction::Command`] there, with its label, group, shortcut and what it asks for
//! (`repo.open`'s folder), enabled by the engine's own check (`engine.states`, the reason shown on
//! a greyed item). This module names no command. **The window's own items stay here**, each for a
//! reason: Open Recent's list (one item a repository `repo.recent` lists), Quit (the window's), and
//! the View menu's check items, which show and set state rather than run an action: the panes (UI
//! state, `ui.set`), the rows' grouping, collapse and merge marks (`rows.set`'s fields), folding
//! quiet time and Fit (the widget's view) and Live (`live.set`'s on).

use crate::catalog;
use swarmception_domain::{Collapse, Grouping};
use swarmception_engine::command::Shortcut;
use swarmception_engine::commands;
use swarmception_engine::repo_cmds::{MAX_RECENT, RecentRepo};

/// One menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    /// An engine command the registry places in a menu (#160), by its id: File ▸ Open…,
    /// Reload, Fetch and Reload, Open Recent ▸ Clear Menu.
    Command(&'static str),
    /// File ▸ Open Recent: the submenu of [`MenuAction::OpenRecent`]s and the commands the
    /// registry places there (#69); choosing it does nothing itself.
    Recent,
    /// File ▸ Open Recent ▸ the `n`th repository `repo.recent` lists, newest first: `repo.open`.
    OpenRecent(u8),
    /// File ▸ Quit: closes the window.
    Quit,
    /// View ▸ Timeline: shows or hides the timeline pane.
    Timeline,
    /// View ▸ Factory: shows or hides the factory pane.
    Factory,
    /// View ▸ Details: shows or hides the details pane (#47).
    Details,
    /// View ▸ Workers: shows or hides the workers pane, model and effort compared (#67).
    Workers,
    /// View ▸ Flat or By Worker: `rows.set` with that grouping.
    Grouping(Grouping),
    /// View ▸ Collapse Empty Rows and Collapse to Window (#74): `rows.set` with that collapse,
    /// or with none when it is the one on. `Collapse(Off)` is no item of the menus.
    Collapse(Collapse),
    /// View ▸ Fold Quiet Time (#86): the timeline folds the stretches where nothing happened
    /// (the widget's `view.fold`), toggled.
    FoldQuiet,
    /// View ▸ Merge Marks (#127): `rows.set`'s `mergeMarks` toggled, a green dot at each merged
    /// ticket's bar's end.
    MergeMarks,
    /// View ▸ Fit: the widget's `view.fit`.
    Fit,
    /// View ▸ Live: `live.set` toggled, watching the swarm (#54).
    Live,
}

impl MenuAction {
    /// The window's own items, in menu order (the commands the registry places are
    /// [`MenuAction::commands`]; Open Recent's repositories [`MenuAction::recent_items`]).
    pub const ALL: [Self; 14] = [
        Self::Recent,
        Self::Quit,
        Self::Timeline,
        Self::Factory,
        Self::Details,
        Self::Workers,
        Self::Grouping(Grouping::Flat),
        Self::Grouping(Grouping::Workers),
        Self::Collapse(Collapse::Empty),
        Self::Collapse(Collapse::Window),
        Self::FoldQuiet,
        Self::MergeMarks,
        Self::Fit,
        Self::Live,
    ];

    /// Every command the registry places in a menu, in the registry's order.
    #[must_use]
    pub fn commands() -> Vec<Self> {
        commands::all().iter().filter(|e| e.menu.is_some()).map(|e| Self::Command(e.id)).collect()
    }

    /// The commands the registry places at `path`, each with its group, in the registry's order.
    fn placed(path: &[&str]) -> Vec<(u8, Self)> {
        commands::all().iter().filter_map(|e| e.menu.filter(|m| m.path == path).map(|m| (m.group, Self::Command(e.id)))).collect()
    }

    /// `items` (each with its group) in group order, a separator between groups.
    fn grouped(mut items: Vec<(u8, Self)>) -> Vec<Option<Self>> {
        // Stable: within a group, the order they came in.
        items.sort_by_key(|(group, _)| *group);
        let mut out = Vec::new();
        let mut last = None;
        for (group, item) in items {
            if last.is_some_and(|l| l != group) {
                out.push(None);
            }
            last = Some(group);
            out.push(Some(item));
        }
        out
    }

    /// The File menu's items, separated where `None` is: the commands the registry places there,
    /// with the window's Open Recent (after the first group's) and Quit (last).
    #[must_use]
    pub fn file_items() -> Vec<Option<Self>> {
        let mut items = Self::placed(&["File"]);
        let first = items.iter().map(|(group, _)| *group).min().unwrap_or(0);
        items.push((first, Self::Recent));
        items.push((u8::MAX, Self::Quit));
        Self::grouped(items)
    }

    /// Open Recent's items for `count` repositories: each, then (separated) the commands the
    /// registry places in Open Recent (Clear Menu).
    #[must_use]
    pub fn recent_items(count: usize) -> Vec<Option<Self>> {
        let repos = (0..count.min(MAX_RECENT)).filter_map(|n| u8::try_from(n).ok()).map(|n| Some(Self::OpenRecent(n)));
        let placed = Self::grouped(Self::placed(&["File", "Open Recent"]));
        repos.chain(std::iter::once(None)).chain(placed).collect()
    }

    /// The View menu's items, separated where `None` is.
    pub const VIEW: [Option<Self>; 15] = [
        Some(Self::Timeline),
        Some(Self::Factory),
        Some(Self::Details),
        Some(Self::Workers),
        None,
        Some(Self::Grouping(Grouping::Flat)),
        Some(Self::Grouping(Grouping::Workers)),
        None,
        Some(Self::Collapse(Collapse::Empty)),
        Some(Self::Collapse(Collapse::Window)),
        Some(Self::FoldQuiet),
        Some(Self::MergeMarks),
        Some(Self::Fit),
        None,
        Some(Self::Live),
    ];

    /// A stable id, for a native menu's items.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Command(id) => id,
            Self::Recent => "file.recent",
            Self::OpenRecent(n) => recent_id(n),
            Self::Quit => "file.quit",
            Self::Timeline => "view.timeline",
            Self::Factory => "view.factory",
            Self::Details => "view.details",
            Self::Workers => "view.workersPane",
            Self::Grouping(Grouping::Flat) => "view.flat",
            Self::Grouping(Grouping::Workers) => "view.workers",
            Self::Collapse(Collapse::Empty) => "view.collapse",
            Self::Collapse(Collapse::Window) => "view.collapseWindow",
            Self::Collapse(Collapse::Off) => "view.collapseOff",
            Self::FoldQuiet => "view.foldQuiet",
            Self::MergeMarks => "view.mergeMarks",
            Self::Fit => "view.fit",
            Self::Live => "view.live",
        }
    }

    /// The item with id `id`.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().chain(Self::commands()).chain(Self::recent_items(MAX_RECENT).into_iter().flatten()).find(|a| a.id() == id)
    }

    /// What the menu shows: a command's from the registry.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Command(id) => commands::find(id).and_then(|e| e.menu).map_or(id, |m| m.label),
            Self::Recent => catalog::OPEN_RECENT,
            // Its repository's name and where it is: [`recent_label`].
            Self::OpenRecent(_) => catalog::RECENT_REPOSITORY,
            Self::Quit => catalog::QUIT,
            Self::Timeline => catalog::TIMELINE,
            Self::Factory => catalog::FACTORY,
            Self::Details => catalog::DETAILS_2,
            Self::Workers => catalog::WORKERS,
            Self::Grouping(Grouping::Flat) => catalog::FLAT,
            Self::Grouping(Grouping::Workers) => catalog::BY_WORKER,
            Self::Collapse(Collapse::Empty) => catalog::COLLAPSE_EMPTY_ROWS,
            Self::Collapse(Collapse::Window) => catalog::COLLAPSE_TO_WINDOW,
            Self::Collapse(Collapse::Off) => catalog::EVERY_ROW,
            Self::FoldQuiet => catalog::FOLD_QUIET_TIME,
            Self::MergeMarks => catalog::MERGE_MARKS,
            Self::Fit => catalog::FIT,
            Self::Live => catalog::LIVE,
        }
    }

    /// Its shortcut: a command's from the registry; the window's own Quit and Live.
    #[must_use]
    pub fn shortcut(self) -> Option<Shortcut> {
        match self {
            Self::Command(id) => commands::find(id).and_then(|e| e.menu).and_then(|m| m.shortcut),
            Self::Quit => Some(Shortcut::command('Q')),
            Self::Live => Some(Shortcut::command('L')),
            _ => None,
        }
    }

    /// Its Font Awesome glyph, for the in-window menu: the window's look, a command's by its id
    /// (none for a command it has no glyph for).
    #[must_use]
    pub fn icon(self) -> &'static str {
        use crate::icons;
        match self {
            Self::Command(id) => match id {
                "repo.open" => icons::FOLDER,
                "repo.clearRecent" => icons::CLEAR,
                "repo.reload" => icons::RELOAD,
                "repo.fetch" => icons::FETCH,
                _ => "",
            },
            Self::OpenRecent(_) => icons::FOLDER,
            Self::Recent => icons::RECENT,
            Self::Quit => icons::QUIT,
            Self::Timeline => icons::TIMELINE,
            Self::Factory => icons::FACTORY,
            Self::Details => icons::INFO,
            Self::Workers => icons::WORKERS,
            Self::Grouping(Grouping::Flat) => icons::FLAT,
            Self::Grouping(Grouping::Workers) => icons::WORKERS,
            Self::Collapse(Collapse::Empty | Collapse::Off) => icons::COLLAPSE,
            Self::Collapse(Collapse::Window) => icons::COLLAPSE_WINDOW,
            Self::FoldQuiet => icons::FOLD,
            Self::MergeMarks => icons::MERGE_MARKS,
            Self::Fit => icons::FIT,
            Self::Live => icons::LIVE,
        }
    }

    /// Whether choosing it closes the menu: a check item leaves it open.
    #[must_use]
    pub const fn closes_menu(self) -> bool {
        matches!(self, Self::Command(_) | Self::OpenRecent(_) | Self::Quit | Self::Fit)
    }
}

/// Open Recent's `n`th item's id; one past [`MAX_RECENT`] is never listed.
const fn recent_id(n: u8) -> &'static str {
    match n {
        0 => "file.recent.0",
        1 => "file.recent.1",
        2 => "file.recent.2",
        3 => "file.recent.3",
        4 => "file.recent.4",
        5 => "file.recent.5",
        6 => "file.recent.6",
        7 => "file.recent.7",
        8 => "file.recent.8",
        9 => "file.recent.9",
        _ => "file.recent.none",
    }
}

/// What Open Recent shows for `repo`: its name, then the folder it is in (the home directory as
/// `~`), and that it is gone when it is.
#[must_use]
pub fn recent_label(repo: &RecentRepo, home: Option<&std::path::Path>) -> String {
    let path = std::path::Path::new(&repo.path);
    let parent = path.parent().map(|p| {
        home.and_then(|h| p.strip_prefix(h).ok()).map_or_else(
            || p.display().to_string(),
            |rest| {
                if rest.as_os_str().is_empty() { "~".to_owned() } else { format!("~/{}", rest.display()) }
            },
        )
    });
    let gone = if repo.missing { catalog::MISSING } else { "" };
    match parent {
        Some(parent) if !parent.is_empty() => format!("{} — {parent}{gone}", repo.name),
        _ => format!("{}{gone}", repo.name),
    }
}

/// Which of the registry's commands can run now (`engine.states`, #160): bit `i` its `i`th
/// command's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommandsEnabled(pub u128);

impl CommandsEnabled {
    /// From the engine's states, in the registry's order.
    #[must_use]
    pub fn of(states: &[swarmception_engine::engine_cmds::CommandState]) -> Self {
        Self(states.iter().take(128).enumerate().filter(|(_, s)| s.enabled).fold(0, |bits, (i, _)| bits | (1 << i)))
    }

    /// Whether the command `id` can run now.
    #[must_use]
    pub fn enabled(self, id: &str) -> bool {
        commands::all().iter().position(|e| e.id == id).is_some_and(|i| i < 128 && self.0 & (1 << i) != 0)
    }
}

/// What the menus show now: which items are enabled and which are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuState {
    /// Which commands can run now: the engine's own check (#160).
    pub commands: CommandsEnabled,
    /// The timeline shows data.
    pub can_fit: bool,
    pub timeline: bool,
    pub factory: bool,
    pub details: bool,
    pub workers: bool,
    pub grouping: Grouping,
    pub collapse: Collapse,
    /// The timeline folds its quiet time (#86).
    pub fold_quiet: bool,
    /// Merged tickets' bars end in a green dot (#127).
    pub merge_marks: bool,
    /// The live watch is on (#54).
    pub live: bool,
    /// How many repositories Open Recent lists (#69, `repo.recent`).
    pub recent: u8,
}

impl MenuState {
    /// Whether `action` can run now.
    #[must_use]
    pub fn enabled(&self, action: MenuAction) -> bool {
        match action {
            MenuAction::Command(id) => self.commands.enabled(id),
            MenuAction::Fit => self.can_fit,
            MenuAction::OpenRecent(n) => n < self.recent,
            MenuAction::Recent
            | MenuAction::Quit
            | MenuAction::Timeline
            | MenuAction::Factory
            | MenuAction::Details
            | MenuAction::Workers
            | MenuAction::Grouping(_)
            | MenuAction::Collapse(_)
            | MenuAction::FoldQuiet
            | MenuAction::MergeMarks
            | MenuAction::Live => true,
        }
    }

    /// Whether `action`'s check mark shows, for a check item; `None` for a plain one.
    #[must_use]
    pub fn checked(&self, action: MenuAction) -> Option<bool> {
        match action {
            MenuAction::Timeline => Some(self.timeline),
            MenuAction::Factory => Some(self.factory),
            MenuAction::Details => Some(self.details),
            MenuAction::Workers => Some(self.workers),
            MenuAction::Grouping(grouping) => Some(self.grouping == grouping),
            MenuAction::Collapse(collapse) => Some(self.collapse == collapse),
            MenuAction::FoldQuiet => Some(self.fold_quiet),
            MenuAction::MergeMarks => Some(self.merge_marks),
            MenuAction::Live => Some(self.live),
            MenuAction::Command(_) | MenuAction::Recent | MenuAction::OpenRecent(_) | MenuAction::Quit | MenuAction::Fit => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_read_back() {
        let all: Vec<MenuAction> = MenuAction::ALL.into_iter().chain(MenuAction::commands()).collect();
        for action in &all {
            assert_eq!(MenuAction::from_id(action.id()), Some(*action));
            assert_eq!(all.iter().filter(|a| a.id() == action.id()).count(), 1);
        }
        assert_eq!(MenuAction::from_id("file.nope"), None);
        // Open Recent's items too, each its own id.
        let recent: Vec<MenuAction> = MenuAction::recent_items(MAX_RECENT).into_iter().flatten().collect();
        assert_eq!(recent.len(), MAX_RECENT + 1);
        for action in &recent {
            assert_eq!(MenuAction::from_id(action.id()), Some(*action));
        }
        assert_eq!(MenuAction::from_id("file.recent.3"), Some(MenuAction::OpenRecent(3)));
        assert_eq!(MenuAction::from_id("file.recent.10"), None);
        assert_eq!(MenuAction::recent_items(30).len(), MAX_RECENT + 2, "ten at most, then Clear Menu");
        assert_eq!(MenuAction::recent_items(0), [None, Some(MenuAction::Command("repo.clearRecent"))], "Clear Menu, placed by the registry");
    }

    /// #160: File is built from the registry's places: each placed command with its label and
    /// shortcut, in its group, the window's Open Recent after the first group and Quit last.
    #[test]
    fn the_file_menu_is_the_registrys_places() {
        let file = MenuAction::file_items();
        let ids: Vec<Option<&str>> = file.iter().map(|a| a.map(MenuAction::id)).collect();
        assert_eq!(ids, [Some("repo.open"), Some("file.recent"), None, Some("repo.reload"), Some("repo.fetch"), None, Some("file.quit")]);
        let labels: Vec<&str> = file.iter().flatten().map(|a| a.label()).collect();
        assert_eq!(labels, ["Open…", "Open Recent", "Reload", "Fetch and Reload", "Quit"]);
        assert_eq!(MenuAction::Command("repo.open").shortcut(), Some(Shortcut::command('O')));
        assert_eq!(MenuAction::Command("repo.fetch").shortcut(), Some(Shortcut::command_shift('R')));
        // Every placed command is in a menu: File, or Open Recent.
        let shown: Vec<MenuAction> = file.into_iter().chain(MenuAction::recent_items(0)).flatten().collect();
        for command in MenuAction::commands() {
            assert!(shown.contains(&command), "{command:?} placed but not shown");
        }
        // Shortcuts are distinct.
        let mut keys: Vec<Shortcut> = MenuAction::ALL.into_iter().chain(MenuAction::commands()).filter_map(MenuAction::shortcut).collect();
        let count = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), count);
    }

    #[test]
    fn a_recent_repository_is_its_name_and_where_it_is() {
        let repo = |path: &str, missing: bool| RecentRepo { path: path.into(), name: path.rsplit('/').next().unwrap().into(), missing };
        let home = std::path::Path::new("/Users/me");
        assert_eq!(recent_label(&repo("/Users/me/dev/mc-engine", false), Some(home)), "mc-engine — ~/dev");
        assert_eq!(recent_label(&repo("/Users/me/swarm", false), Some(home)), "swarm — ~");
        assert_eq!(recent_label(&repo("/Volumes/Data/dev/swarm", true), Some(home)), "swarm — /Volumes/Data/dev (missing)");
        assert_eq!(recent_label(&repo("/Volumes/Data/dev/swarm", false), None), "swarm — /Volumes/Data/dev");
    }

    #[test]
    fn there_is_no_titles_item() {
        // Titles are always on the bars (#34): the View menu has no Titles on Bars.
        assert_eq!(MenuAction::from_id("view.titles"), None);
        assert!(MenuAction::ALL.iter().all(|a| a.label() != "Titles on Bars"));
    }

    #[test]
    fn the_menus_hold_every_item_once() {
        let listed: Vec<MenuAction> =
            MenuAction::file_items().iter().chain(&MenuAction::VIEW).flatten().copied().filter(|a| !matches!(a, MenuAction::Command(_))).collect();
        assert_eq!(listed, MenuAction::ALL);
    }

    #[test]
    fn state_enables_and_checks() {
        let only_open = commands::all().iter().position(|e| e.id == "repo.open").unwrap();
        let state = MenuState {
            commands: CommandsEnabled(1 << only_open),
            can_fit: true,
            timeline: true,
            factory: false,
            details: true,
            workers: true,
            grouping: Grouping::Workers,
            collapse: Collapse::Window,
            fold_quiet: true,
            merge_marks: false,
            live: true,
            recent: 2,
        };
        assert!(state.enabled(MenuAction::Command("repo.open")) && state.enabled(MenuAction::Recent));
        assert!(state.enabled(MenuAction::OpenRecent(1)) && !state.enabled(MenuAction::OpenRecent(2)));
        assert!(!state.enabled(MenuAction::Command("repo.reload")), "the engine's check, not the menu's");
        assert!(!state.enabled(MenuAction::Command("repo.fetch")));
        assert!(!state.enabled(MenuAction::Command("no.such")));
        assert!(state.enabled(MenuAction::Fit));
        assert!(state.enabled(MenuAction::Quit));
        assert_eq!(state.checked(MenuAction::Timeline), Some(true));
        assert_eq!(state.checked(MenuAction::Factory), Some(false));
        assert_eq!(state.checked(MenuAction::Details), Some(true));
        assert_eq!(state.checked(MenuAction::Workers), Some(true));
        assert_eq!(state.checked(MenuAction::Grouping(Grouping::Flat)), Some(false));
        assert_eq!(state.checked(MenuAction::Grouping(Grouping::Workers)), Some(true));
        assert_eq!(state.checked(MenuAction::Collapse(Collapse::Empty)), Some(false));
        assert_eq!(state.checked(MenuAction::Collapse(Collapse::Window)), Some(true));
        assert!(state.enabled(MenuAction::Collapse(Collapse::Window)));
        assert_eq!(state.checked(MenuAction::Live), Some(true));
        assert_eq!(state.checked(MenuAction::FoldQuiet), Some(true));
        assert!(state.enabled(MenuAction::FoldQuiet));
        assert!(state.enabled(MenuAction::Live), "on or off whatever is being read");

        assert_eq!(state.checked(MenuAction::Command("repo.reload")), None);
    }
}
