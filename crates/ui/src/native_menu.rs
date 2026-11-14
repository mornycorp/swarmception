//! The system menu bar (#179, swarm-core's App UI Substrate › Menus): what macOS's bar holds, made
//! from the same menu items the window's own bar draws ([`MenuAction`], [`MenuState`]), as plain
//! data. It names no toolkit and no platform API, so it is tested on every platform; the windowed
//! app hands it to AppKit (`apps/app/src/mac_menu.rs`, with `muda`), and a click comes back as the
//! item's [`MenuAction`], which the shell runs exactly as a click in the window's bar
//! ([`crate::Shell::run_menu`]), so the engine's own check and its reasons hold the same.
//!
//! The bar ([`bar`]), left to right:
//!
//! - **The application menu**, named for the product: About (the system's panel, with the name, the
//!   version and the credits the embedded faces ask for), Services, Hide, Hide Others, Show All,
//!   and Quit (⌘Q), which closes the window as its close button does, so the state is saved.
//! - **The app's menus**, as the window's bar has them: File (its Quit moved to the application
//!   menu, Open Recent a submenu of the repositories `repo.recent` lists) and View.
//! - **The Window menu**: Minimize, Zoom and Bring All to Front; the system lists the app's windows
//!   there. The app has no Help menu.
//!
//! An item's shortcut is its key equivalent ([`accelerator`]), unless one of the system's own items
//! takes that key ([`SYSTEM_KEYS`]): then the item stays and the key is the system's. Each item is
//! enabled, and checked, as the window's bar has it, set again as the state changes.

use swarmception_engine::command::Shortcut;

use crate::catalog;
use crate::menu::{MenuAction, MenuState};

/// The keys the system's own items take on macOS (Hide, Minimize): a command's shortcut that is
/// one of them keeps its item but not the key. (Quit's ⌘Q is the window's own Quit, which the
/// application menu holds; Hide Others' ⌥⌘H has an Option no shortcut here has.)
pub const SYSTEM_KEYS: [char; 2] = ['H', 'M'];

/// The About panel's line under the name.
pub const ABOUT_COMMENTS: &str = catalog::A_SWARM_S_TICKETS_ON_A;
/// The About panel's credits: what the embedded faces' licences ask for (Font Awesome's icons are
/// CC BY 4.0).
pub const ABOUT_CREDITS: &str = catalog::ABOUT_CREDITS;

/// A menu of the bar, or a submenu in one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub label: String,
    pub role: Role,
    pub entries: Vec<Entry>,
}

/// What the platform makes of a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The application menu, named for the product.
    App,
    /// One of the app's menus, or a submenu.
    Plain,
    /// The platform's window menu, which lists the app's windows.
    Window,
}

/// One line of a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// An item the app runs.
    Item(Item),
    /// A submenu (Open Recent).
    Menu(Menu),
    /// One of the system's own items, which the platform runs itself.
    System {
        kind: System,
        label: String,
    },
    Separator,
}

/// An item the app runs when it is clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub action: MenuAction,
    /// The key that runs it, in macOS's menus' form ([`accelerator`]: `Cmd+Shift+R`).
    pub accelerator: Option<String>,
    /// Whether it can run now.
    pub enabled: bool,
    /// For an item that shows a state: whether it is on.
    pub checked: Option<bool>,
}

/// The system's own items, which the platform runs: the app only places them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    About,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Minimize,
    Zoom,
    BringAllToFront,
}

/// The system menu bar for the product `product`, as `state` and Open Recent's `recent` labels have
/// it now: the application menu, File, View, and Window.
#[must_use]
pub fn bar(product: &str, state: &MenuState, recent: &[String]) -> Vec<Menu> {
    let app = vec![
        system(System::About, catalog::about(&product)),
        Entry::Separator,
        system(System::Services, catalog::SERVICES.to_owned()),
        Entry::Separator,
        system(System::Hide, catalog::hide(&product)),
        system(System::HideOthers, catalog::HIDE_OTHERS.to_owned()),
        system(System::ShowAll, catalog::SHOW_ALL.to_owned()),
        Entry::Separator,
        Entry::Item(Item { label: catalog::quit(&product), ..item(MenuAction::Quit, state) }),
    ];
    let file = trimmed(MenuAction::file_items().into_iter().filter(|a| *a != Some(MenuAction::Quit)).map(|a| entry(a, state, recent)).collect());
    let view = MenuAction::VIEW.into_iter().map(|a| entry(a, state, recent)).collect();
    let window = vec![
        system(System::Minimize, catalog::MINIMIZE_2.to_owned()),
        system(System::Zoom, catalog::ZOOM.to_owned()),
        Entry::Separator,
        system(System::BringAllToFront, catalog::BRING_ALL_TO_FRONT.to_owned()),
    ];
    vec![
        Menu { label: product.to_owned(), role: Role::App, entries: app },
        Menu { label: catalog::FILE.to_owned(), role: Role::Plain, entries: file },
        Menu { label: catalog::VIEW.to_owned(), role: Role::Plain, entries: view },
        Menu { label: catalog::WINDOW.to_owned(), role: Role::Window, entries: window },
    ]
}

/// Open Recent's submenu as `state` and its `recent` labels have it: each repository, then the
/// commands the registry places there (Clear Menu).
#[must_use]
pub fn recent_menu(state: &MenuState, recent: &[String]) -> Menu {
    let entries = MenuAction::recent_items(recent.len()).into_iter().map(|a| entry(a, state, recent)).collect();
    Menu { label: MenuAction::Recent.label().to_owned(), role: Role::Plain, entries }
}

fn system(kind: System, label: String) -> Entry {
    Entry::System { kind, label }
}

/// The entry for one of the window bar's items, `None` a separator.
fn entry(action: Option<MenuAction>, state: &MenuState, recent: &[String]) -> Entry {
    match action {
        None => Entry::Separator,
        Some(MenuAction::Recent) => Entry::Menu(recent_menu(state, recent)),
        Some(action @ MenuAction::OpenRecent(n)) => Entry::Item(Item { label: recent.get(usize::from(n)).cloned().unwrap_or_default(), ..item(action, state) }),
        Some(action) => Entry::Item(item(action, state)),
    }
}

fn item(action: MenuAction, state: &MenuState) -> Item {
    Item {
        label: action.label().to_owned(),
        action,
        accelerator: action.shortcut().filter(|s| action == MenuAction::Quit || !SYSTEM_KEYS.contains(&s.key.to_ascii_uppercase())).map(accelerator),
        enabled: state.enabled(action),
        checked: state.checked(action),
    }
}

/// `entries` without the separators at their ends.
fn trimmed(mut entries: Vec<Entry>) -> Vec<Entry> {
    while entries.last() == Some(&Entry::Separator) {
        entries.pop();
    }
    while entries.first() == Some(&Entry::Separator) {
        entries.remove(0);
    }
    entries
}

/// A shortcut as macOS's menus take it, `muda`'s accelerator text: `Cmd+R`, `Cmd+Shift+R`.
#[must_use]
pub fn accelerator(shortcut: Shortcut) -> String {
    let key = shortcut.key.to_ascii_uppercase();
    if shortcut.shift { format!("Cmd+Shift+{key}") } else { format!("Cmd+{key}") }
}

#[cfg(test)]
mod tests;
