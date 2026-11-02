//! macOS: the menus in the system menu bar, not inside the window (owner, 2026-10-31, #3). The
//! app menu (About, Services, Hide, Quit), then File and View, built from the shell's
//! [`MenuAction`]s: a click runs the same [`Shell::run_menu`] the in-window menu runs, and the
//! items' enabled and checked marks follow [`Shell::menu_state`] every frame.
//!
//! Automatic window tabbing is off ([`no_window_tabs`], #9): the app has one window, and with it
//! on, macOS adds Show Tab Bar and Show All Tabs to the View menu.

use std::sync::mpsc::{self, Receiver};

use eframe::egui;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use objc2::MainThreadMarker;
use objc2_app_kit::NSWindow;
use swarmception_ui::{MenuAction, MenuState, Shell};

/// Turns automatic window tabbing off for the app, so macOS adds no tab items (Show Tab Bar, Show
/// All Tabs) to the View menu. Call it on the main thread before the window and menus exist; off
/// it, it does nothing and says so.
pub fn no_window_tabs() {
    match MainThreadMarker::new() {
        Some(mtm) => NSWindow::setAllowsAutomaticWindowTabbing(false, mtm),
        None => eprintln!("swarmception: not on the main thread; window tabbing stays on"),
    }
}

/// The system menu bar, while the app runs.
pub struct NativeMenu {
    /// Kept alive: the items below are its.
    _menu: Menu,
    items: Vec<(MenuAction, Item)>,
    clicks: Receiver<MenuAction>,
    /// What the items show now; `None` after a click, which may have toggled a check mark itself.
    shown: Option<MenuState>,
}

enum Item {
    Plain(MenuItem),
    Check(CheckMenuItem),
}

impl Item {
    fn as_menu_item(&self) -> &dyn IsMenuItem {
        match self {
            Self::Plain(item) => item,
            Self::Check(item) => item,
        }
    }

    fn show(&self, state: &MenuState, action: MenuAction) {
        match self {
            Self::Plain(item) => item.set_enabled(state.enabled(action)),
            Self::Check(item) => {
                item.set_enabled(state.enabled(action));
                item.set_checked(state.checked(action).unwrap_or(false));
            }
        }
    }
}

/// The Mac shortcut of `action`.
const fn shortcut(action: MenuAction) -> Option<(Modifiers, Code)> {
    match action {
        MenuAction::Reload => Some((Modifiers::META, Code::KeyR)),
        MenuAction::Fetch => Some((Modifiers::META.union(Modifiers::SHIFT), Code::KeyR)),
        MenuAction::Quit => Some((Modifiers::META, Code::KeyQ)),
        MenuAction::Timeline | MenuAction::Factory | MenuAction::Details | MenuAction::Grouping(_) | MenuAction::Collapse | MenuAction::Fit => None,
    }
}

/// What the system menu bar calls `action`: a Mac app's Quit names the app.
const fn label(action: MenuAction) -> &'static str {
    match action {
        MenuAction::Quit => "Quit Swarmception",
        other => other.label(),
    }
}

/// What File and View hold on a Mac: Quit is the app menu's, and a separator that ends up last
/// goes with it.
fn mac_items(actions: &[Option<MenuAction>]) -> Vec<Option<MenuAction>> {
    let mut actions: Vec<Option<MenuAction>> = actions.iter().copied().filter(|a| *a != Some(MenuAction::Quit)).collect();
    while actions.last() == Some(&None) {
        actions.pop();
    }
    actions
}

/// Appends `actions`' items to `submenu`, a separator for each `None`.
fn append(submenu: &Submenu, actions: &[Option<MenuAction>], items: &[(MenuAction, Item)]) -> Result<(), muda::Error> {
    for action in actions {
        match action {
            None => submenu.append(&PredefinedMenuItem::separator())?,
            Some(action) => {
                if let Some((_, item)) = items.iter().find(|(a, _)| a == action) {
                    submenu.append(item.as_menu_item())?;
                }
            }
        }
    }
    Ok(())
}

impl NativeMenu {
    /// Builds the menus and makes them the app's main menu; a click wakes `ctx`.
    ///
    /// # Errors
    ///
    /// What muda ran into building them; the window keeps its own menu bar then.
    pub fn install(ctx: &egui::Context, state: &MenuState) -> Result<Self, muda::Error> {
        let mut items = Vec::new();
        for action in MenuAction::ALL {
            let accelerator = shortcut(action).map(|(mods, code)| Accelerator::new(mods, code));
            let item = match state.checked(action) {
                Some(checked) => Item::Check(CheckMenuItem::with_id(action.id(), label(action), state.enabled(action), checked, accelerator)),
                None => Item::Plain(MenuItem::with_id(action.id(), label(action), state.enabled(action), accelerator)),
            };
            items.push((action, item));
        }
        let about = AboutMetadata {
            name: Some("Swarmception".to_owned()),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            comments: Some("A swarm's tickets on a timeline, from its tracking branch".to_owned()),
            ..AboutMetadata::default()
        };
        let app = Submenu::new("Swarmception", true);
        for predefined in [
            PredefinedMenuItem::about(Some("About Swarmception"), Some(about)),
            PredefinedMenuItem::separator(),
            PredefinedMenuItem::services(None),
            PredefinedMenuItem::separator(),
            PredefinedMenuItem::hide(None),
            PredefinedMenuItem::hide_others(None),
            PredefinedMenuItem::show_all(None),
            PredefinedMenuItem::separator(),
        ] {
            app.append(&predefined)?;
        }
        append(&app, &[Some(MenuAction::Quit)], &items)?;
        let file = Submenu::new("File", true);
        append(&file, &mac_items(&MenuAction::FILE), &items)?;
        let view = Submenu::new("View", true);
        append(&view, &mac_items(&MenuAction::VIEW), &items)?;

        let menu = Menu::new();
        menu.append_items(&[&app, &file, &view])?;
        menu.init_for_nsapp();

        let (send, clicks) = mpsc::channel();
        let waker = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some(action) = MenuAction::from_id(event.id.as_ref()) {
                // The receiver goes only when the app does.
                let _ = send.send(action);
                waker.request_repaint();
            }
        }));
        Ok(Self { _menu: menu, items, clicks, shown: Some(*state) })
    }

    /// Runs the items clicked since the last frame.
    pub fn run_clicks(&mut self, shell: &mut Shell, ctx: &egui::Context) {
        while let Ok(action) = self.clicks.try_recv() {
            shell.run_menu(ctx, action);
            self.shown = None;
        }
    }

    /// Brings the items' enabled and checked marks in step with the shell.
    pub fn show(&mut self, shell: &mut Shell) {
        let state = shell.menu_state();
        if self.shown == Some(state) {
            return;
        }
        for (action, item) in &self.items {
            item.show(&state, *action);
        }
        self.shown = Some(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_are_the_mac_ones_and_distinct() {
        assert_eq!(shortcut(MenuAction::Quit), Some((Modifiers::META, Code::KeyQ)));
        assert_eq!(shortcut(MenuAction::Reload), Some((Modifiers::META, Code::KeyR)));
        let all: Vec<_> = MenuAction::ALL.into_iter().filter_map(shortcut).collect();
        for (i, a) in all.iter().enumerate() {
            assert!(!all[i + 1..].contains(a), "{a:?} twice");
        }
        assert_eq!(label(MenuAction::Quit), "Quit Swarmception");
        assert_eq!(label(MenuAction::Fit), "Fit");
    }

    #[test]
    fn window_tabbing_goes_off_and_never_panics() {
        // Off the main thread (where the harness usually runs a test) it only says so.
        no_window_tabs();
        if let Some(mtm) = MainThreadMarker::new() {
            assert!(!NSWindow::allowsAutomaticWindowTabbing(mtm));
        }
    }

    #[test]
    fn quit_moves_to_the_app_menu() {
        assert_eq!(mac_items(&MenuAction::FILE), [Some(MenuAction::Reload), Some(MenuAction::Fetch)]);
        assert_eq!(mac_items(&MenuAction::VIEW), MenuAction::VIEW);
    }
}
