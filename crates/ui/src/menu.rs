//! The menus as data: every item is a [`MenuAction`] with a stable id, and [`MenuState`] says
//! which are enabled and checked. The in-window menu bar and macOS's system menu bar draw the
//! same items and run them through the same [`crate::Shell::run_menu`], so both reach the same
//! engine commands (`repo.open`, `repo.recent`, `repo.clearRecent`, `repo.reload`, `repo.fetch`,
//! `rows.set`, `live.set`) and widget commands (`view.fit`); File opens a repository (Open…,
//! Open Recent, #69), View lists the panes, the timeline, the factory, the details, the gates and
//! the workers, and Live.

use swarmception_domain::{Collapse, Grouping};
use swarmception_engine::repo_cmds::{MAX_RECENT, RecentRepo};

/// One menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    /// File ▸ Open…: a folder chosen (the host's dialog), then `repo.open` (#69).
    Open,
    /// File ▸ Open Recent: the submenu of [`MenuAction::OpenRecent`]s and
    /// [`MenuAction::ClearRecent`] (#69); choosing it does nothing itself.
    Recent,
    /// File ▸ Open Recent ▸ the `n`th repository `repo.recent` lists, newest first: `repo.open`.
    OpenRecent(u8),
    /// File ▸ Open Recent ▸ Clear Menu: `repo.clearRecent`.
    ClearRecent,
    /// File ▸ Reload: `repo.reload`.
    Reload,
    /// File ▸ Fetch and Reload: `repo.fetch`.
    Fetch,
    /// File ▸ Quit: closes the window.
    Quit,
    /// View ▸ Timeline: shows or hides the timeline pane.
    Timeline,
    /// View ▸ Factory: shows or hides the factory pane.
    Factory,
    /// View ▸ Details: shows or hides the details pane (#47).
    Details,
    /// View ▸ Gates: shows or hides the gates pane (#61).
    Gates,
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
    /// View ▸ Fit: the widget's `view.fit`.
    Fit,
    /// View ▸ Live: `live.set` toggled, watching the swarm (#54).
    Live,
}

impl MenuAction {
    /// Every item of the menus themselves, in menu order (Open Recent's own are
    /// [`MenuAction::recent_items`]).
    pub const ALL: [Self; 17] = [
        Self::Open,
        Self::Recent,
        Self::Reload,
        Self::Fetch,
        Self::Quit,
        Self::Timeline,
        Self::Factory,
        Self::Details,
        Self::Gates,
        Self::Workers,
        Self::Grouping(Grouping::Flat),
        Self::Grouping(Grouping::Workers),
        Self::Collapse(Collapse::Empty),
        Self::Collapse(Collapse::Window),
        Self::FoldQuiet,
        Self::Fit,
        Self::Live,
    ];

    /// The File menu's items, separated where `None` is.
    pub const FILE: [Option<Self>; 7] = [Some(Self::Open), Some(Self::Recent), None, Some(Self::Reload), Some(Self::Fetch), None, Some(Self::Quit)];

    /// Open Recent's items for `count` repositories: each, then (separated) Clear Menu.
    #[must_use]
    pub fn recent_items(count: usize) -> Vec<Option<Self>> {
        let repos = (0..count.min(MAX_RECENT)).filter_map(|n| u8::try_from(n).ok()).map(|n| Some(Self::OpenRecent(n)));
        repos.chain([None, Some(Self::ClearRecent)]).collect()
    }

    /// The View menu's items, separated where `None` is.
    pub const VIEW: [Option<Self>; 15] = [
        Some(Self::Timeline),
        Some(Self::Factory),
        Some(Self::Details),
        Some(Self::Gates),
        Some(Self::Workers),
        None,
        Some(Self::Grouping(Grouping::Flat)),
        Some(Self::Grouping(Grouping::Workers)),
        None,
        Some(Self::Collapse(Collapse::Empty)),
        Some(Self::Collapse(Collapse::Window)),
        Some(Self::FoldQuiet),
        Some(Self::Fit),
        None,
        Some(Self::Live),
    ];

    /// A stable id, for a native menu's items.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Open => "file.open",
            Self::Recent => "file.recent",
            Self::OpenRecent(n) => recent_id(n),
            Self::ClearRecent => "file.recent.clear",
            Self::Reload => "file.reload",
            Self::Fetch => "file.fetch",
            Self::Quit => "file.quit",
            Self::Timeline => "view.timeline",
            Self::Factory => "view.factory",
            Self::Details => "view.details",
            Self::Gates => "view.gates",
            Self::Workers => "view.workersPane",
            Self::Grouping(Grouping::Flat) => "view.flat",
            Self::Grouping(Grouping::Workers) => "view.workers",
            Self::Collapse(Collapse::Empty) => "view.collapse",
            Self::Collapse(Collapse::Window) => "view.collapseWindow",
            Self::Collapse(Collapse::Off) => "view.collapseOff",
            Self::FoldQuiet => "view.foldQuiet",
            Self::Fit => "view.fit",
            Self::Live => "view.live",
        }
    }

    /// The item with id `id`.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().chain(Self::recent_items(MAX_RECENT).into_iter().flatten()).find(|a| a.id() == id)
    }

    /// What the menu shows.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "Open…",
            Self::Recent => "Open Recent",
            // Its repository's name and where it is: [`recent_label`].
            Self::OpenRecent(_) => "Recent Repository",
            Self::ClearRecent => "Clear Menu",
            Self::Reload => "Reload",
            Self::Fetch => "Fetch and Reload",
            Self::Quit => "Quit",
            Self::Timeline => "Timeline",
            Self::Factory => "Factory",
            Self::Details => "Details",
            Self::Gates => "Gates",
            Self::Workers => "Workers",
            Self::Grouping(Grouping::Flat) => "Flat",
            Self::Grouping(Grouping::Workers) => "By Worker",
            Self::Collapse(Collapse::Empty) => "Collapse Empty Rows",
            Self::Collapse(Collapse::Window) => "Collapse to Window",
            Self::Collapse(Collapse::Off) => "Every Row",
            Self::FoldQuiet => "Fold Quiet Time",
            Self::Fit => "Fit",
            Self::Live => "Live",
        }
    }

    /// Its Font Awesome glyph, for the in-window menu.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        use crate::icons;
        match self {
            Self::Open | Self::OpenRecent(_) => icons::FOLDER,
            Self::Recent => icons::RECENT,
            Self::ClearRecent => icons::CLEAR,
            Self::Reload => icons::RELOAD,
            Self::Fetch => icons::FETCH,
            Self::Quit => icons::QUIT,
            Self::Timeline => icons::TIMELINE,
            Self::Factory => icons::FACTORY,
            Self::Details => icons::INFO,
            Self::Gates => icons::GATES,
            Self::Workers => icons::WORKERS,
            Self::Grouping(Grouping::Flat) => icons::FLAT,
            Self::Grouping(Grouping::Workers) => icons::WORKERS,
            Self::Collapse(Collapse::Empty | Collapse::Off) => icons::COLLAPSE,
            Self::Collapse(Collapse::Window) => icons::COLLAPSE_WINDOW,
            Self::FoldQuiet => icons::FOLD,
            Self::Fit => icons::FIT,
            Self::Live => icons::LIVE,
        }
    }

    /// Whether choosing it closes the menu: a check item leaves it open.
    #[must_use]
    pub const fn closes_menu(self) -> bool {
        matches!(self, Self::Open | Self::OpenRecent(_) | Self::ClearRecent | Self::Reload | Self::Fetch | Self::Quit | Self::Fit)
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
    let gone = if repo.missing { " (missing)" } else { "" };
    match parent {
        Some(parent) if !parent.is_empty() => format!("{} — {parent}{gone}", repo.name),
        _ => format!("{}{gone}", repo.name),
    }
}

/// What the menus show now: which items are enabled and which are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuState {
    /// A repository is open and no read is going.
    pub can_reload: bool,
    /// The timeline shows data.
    pub can_fit: bool,
    pub timeline: bool,
    pub factory: bool,
    pub details: bool,
    pub gates: bool,
    pub workers: bool,
    pub grouping: Grouping,
    pub collapse: Collapse,
    /// The timeline folds its quiet time (#86).
    pub fold_quiet: bool,
    /// The live watch is on (#54).
    pub live: bool,
    /// How many repositories Open Recent lists (#69, `repo.recent`).
    pub recent: u8,
}

impl MenuState {
    /// Whether `action` can run now.
    #[must_use]
    pub const fn enabled(&self, action: MenuAction) -> bool {
        match action {
            MenuAction::Reload | MenuAction::Fetch => self.can_reload,
            MenuAction::Fit => self.can_fit,
            MenuAction::OpenRecent(n) => n < self.recent,
            MenuAction::ClearRecent => self.recent > 0,
            MenuAction::Open
            | MenuAction::Recent
            | MenuAction::Quit
            | MenuAction::Timeline
            | MenuAction::Factory
            | MenuAction::Details
            | MenuAction::Gates
            | MenuAction::Workers
            | MenuAction::Grouping(_)
            | MenuAction::Collapse(_)
            | MenuAction::FoldQuiet
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
            MenuAction::Gates => Some(self.gates),
            MenuAction::Workers => Some(self.workers),
            MenuAction::Grouping(grouping) => Some(self.grouping == grouping),
            MenuAction::Collapse(collapse) => Some(self.collapse == collapse),
            MenuAction::FoldQuiet => Some(self.fold_quiet),
            MenuAction::Live => Some(self.live),
            MenuAction::Open
            | MenuAction::Recent
            | MenuAction::OpenRecent(_)
            | MenuAction::ClearRecent
            | MenuAction::Reload
            | MenuAction::Fetch
            | MenuAction::Quit
            | MenuAction::Fit => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_read_back() {
        for action in MenuAction::ALL {
            assert_eq!(MenuAction::from_id(action.id()), Some(action));
            assert_eq!(MenuAction::ALL.iter().filter(|a| a.id() == action.id()).count(), 1);
        }
        assert_eq!(MenuAction::from_id("file.nope"), None);
        // Open Recent's items too, each its own id.
        let recent: Vec<MenuAction> = MenuAction::recent_items(MAX_RECENT).into_iter().flatten().collect();
        assert_eq!(recent.len(), MAX_RECENT + 1);
        for action in &recent {
            assert_eq!(MenuAction::from_id(action.id()), Some(*action));
            assert!(!MenuAction::ALL.iter().any(|a| a.id() == action.id()));
        }
        assert_eq!(MenuAction::from_id("file.recent.3"), Some(MenuAction::OpenRecent(3)));
        assert_eq!(MenuAction::from_id("file.recent.10"), None);
        assert_eq!(MenuAction::recent_items(30).len(), MAX_RECENT + 2, "ten at most, then Clear Menu");
        assert_eq!(MenuAction::recent_items(0), [None, Some(MenuAction::ClearRecent)]);
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
        let listed: Vec<MenuAction> = MenuAction::FILE.iter().chain(&MenuAction::VIEW).flatten().copied().collect();
        assert_eq!(listed, MenuAction::ALL);
    }

    #[test]
    fn state_enables_and_checks() {
        let state = MenuState {
            can_reload: false,
            can_fit: true,
            timeline: true,
            factory: false,
            details: true,
            gates: false,
            workers: true,
            grouping: Grouping::Workers,
            collapse: Collapse::Window,
            fold_quiet: true,
            live: true,
            recent: 2,
        };
        assert!(state.enabled(MenuAction::Open) && state.enabled(MenuAction::Recent));
        assert!(state.enabled(MenuAction::OpenRecent(1)) && !state.enabled(MenuAction::OpenRecent(2)));
        assert!(state.enabled(MenuAction::ClearRecent));
        assert!(!MenuState { recent: 0, ..state }.enabled(MenuAction::ClearRecent), "nothing to clear");
        assert!(!state.enabled(MenuAction::Reload));
        assert!(!state.enabled(MenuAction::Fetch));
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

        assert_eq!(state.checked(MenuAction::Reload), None);
    }
}
