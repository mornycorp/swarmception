//! The menus as data: every item is a [`MenuAction`] with a stable id, and [`MenuState`] says
//! which are enabled and checked. The in-window menu bar and macOS's system menu bar draw the
//! same items and run them through the same [`crate::Shell::run_menu`], so both reach the same
//! engine commands (`repo.reload`, `repo.fetch`, `rows.set`, `live.set`) and widget commands
//! (`view.fit`); View lists the panes, the timeline, the factory, the details, the gates and the
//! workers, and Live.

use swarmception_domain::Grouping;

/// One menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
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
    /// View ▸ Collapse Empty Rows: `rows.set` with collapse toggled.
    Collapse,
    /// View ▸ Fit: the widget's `view.fit`.
    Fit,
    /// View ▸ Live: `live.set` toggled, watching the swarm (#54).
    Live,
}

impl MenuAction {
    /// Every item, in menu order.
    pub const ALL: [Self; 13] = [
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
        Self::Collapse,
        Self::Fit,
        Self::Live,
    ];

    /// The File menu's items, separated where `None` is.
    pub const FILE: [Option<Self>; 4] = [Some(Self::Reload), Some(Self::Fetch), None, Some(Self::Quit)];

    /// The View menu's items, separated where `None` is.
    pub const VIEW: [Option<Self>; 13] = [
        Some(Self::Timeline),
        Some(Self::Factory),
        Some(Self::Details),
        Some(Self::Gates),
        Some(Self::Workers),
        None,
        Some(Self::Grouping(Grouping::Flat)),
        Some(Self::Grouping(Grouping::Workers)),
        None,
        Some(Self::Collapse),
        Some(Self::Fit),
        None,
        Some(Self::Live),
    ];

    /// A stable id, for a native menu's items.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
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
            Self::Collapse => "view.collapse",
            Self::Fit => "view.fit",
            Self::Live => "view.live",
        }
    }

    /// The item with id `id`.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }

    /// What the menu shows.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
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
            Self::Collapse => "Collapse Empty Rows",
            Self::Fit => "Fit",
            Self::Live => "Live",
        }
    }

    /// Its Font Awesome glyph, for the in-window menu.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        use crate::icons;
        match self {
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
            Self::Collapse => icons::COLLAPSE,
            Self::Fit => icons::FIT,
            Self::Live => icons::LIVE,
        }
    }

    /// Whether choosing it closes the menu: a check item leaves it open.
    #[must_use]
    pub const fn closes_menu(self) -> bool {
        matches!(self, Self::Reload | Self::Fetch | Self::Quit | Self::Fit)
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
    pub collapse: bool,
    /// The live watch is on (#54).
    pub live: bool,
}

impl MenuState {
    /// Whether `action` can run now.
    #[must_use]
    pub const fn enabled(&self, action: MenuAction) -> bool {
        match action {
            MenuAction::Reload | MenuAction::Fetch => self.can_reload,
            MenuAction::Fit => self.can_fit,
            MenuAction::Quit
            | MenuAction::Timeline
            | MenuAction::Factory
            | MenuAction::Details
            | MenuAction::Gates
            | MenuAction::Workers
            | MenuAction::Grouping(_)
            | MenuAction::Collapse
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
            MenuAction::Collapse => Some(self.collapse),
            MenuAction::Live => Some(self.live),
            MenuAction::Reload | MenuAction::Fetch | MenuAction::Quit | MenuAction::Fit => None,
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
            collapse: false,
            live: true,
        };
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
        assert_eq!(state.checked(MenuAction::Collapse), Some(false));
        assert_eq!(state.checked(MenuAction::Live), Some(true));
        assert!(state.enabled(MenuAction::Live), "on or off whatever is being read");

        assert_eq!(state.checked(MenuAction::Reload), None);
    }
}
