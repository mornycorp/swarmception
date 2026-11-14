//! macOS: the menus in the system menu bar, not inside the window (owner, 2026-10-31, #3; #179,
//! swarm-core's App UI Substrate › Menus). What the bar holds is the UI crate's
//! ([`native_menu::bar`], plain data tested on every platform: the application menu, File, View and
//! Window); this module only hands it to AppKit with `muda`. A click comes back as the item's
//! [`MenuAction`], which the shell runs as a click in the window's bar ([`Shell::run_menu`]); the
//! items' enabled and checked marks, and Open Recent's repositories, follow the shell's state every
//! frame. A shortcut is the item's key equivalent alone: AppKit takes it before the window sees the
//! key, and the window's own bar (none here) handles none.
//!
//! Automatic window tabbing is off ([`no_window_tabs`], #9): the app has one window, and with it
//! on, macOS adds Show Tab Bar and Show All Tabs to the View menu.

use std::sync::mpsc::{self, Receiver};

use eframe::egui;
use muda::accelerator::Accelerator;
use muda::{AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use objc2::MainThreadMarker;
use objc2_app_kit::NSWindow;
use swarmception_ui::native_menu::{self, Entry, Role, System};
use swarmception_ui::{MenuAction, MenuState, Shell};

/// The product's name, as the application menu and its items say it.
const PRODUCT: &str = "Swarmception";

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
    /// Each item the app runs, by its action, to set its marks as the state changes.
    items: Vec<(MenuAction, Item)>,
    /// File ▸ Open Recent (#69): rebuilt when the repositories it lists change.
    recent: Submenu,
    /// What Open Recent lists now.
    recent_shown: Option<native_menu::Menu>,
    clicks: Receiver<MenuAction>,
    /// What the items show now; `None` after a click, which may have toggled a check mark itself.
    shown: Option<MenuState>,
}

enum Item {
    Plain(MenuItem),
    Check(CheckMenuItem),
}

impl Item {
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

impl NativeMenu {
    /// Builds the bar from the shell's menus and makes it the app's main menu; a click wakes `ctx`.
    ///
    /// # Errors
    ///
    /// What muda ran into building it; the window keeps its own menu bar then.
    pub fn install(ctx: &egui::Context, shell: &mut Shell) -> Result<Self, muda::Error> {
        let state = shell.menu_state();
        let recent_labels = labels(shell, &state);
        let about = AboutMetadata {
            name: Some(PRODUCT.to_owned()),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            comments: Some(native_menu::ABOUT_COMMENTS.to_owned()),
            credits: Some(native_menu::ABOUT_CREDITS.to_owned()),
            ..AboutMetadata::default()
        };
        let menu = Menu::new();
        let mut built = Built { about, items: Vec::new(), recent: None };
        let mut roles = Vec::new();
        for bar_menu in native_menu::bar(PRODUCT, &state, &recent_labels) {
            let submenu = built.submenu(&bar_menu)?;
            menu.append(&submenu)?;
            roles.push((bar_menu.role, submenu));
        }
        menu.init_for_nsapp();
        for (role, submenu) in &roles {
            if *role == Role::Window {
                submenu.set_as_windows_menu_for_nsapp();
            }
        }
        let recent = built.recent.unwrap_or_else(|| Submenu::new(MenuAction::Recent.label(), true));
        let (send, clicks) = mpsc::channel();
        let waker = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some(action) = MenuAction::from_id(event.id.as_ref()) {
                // The receiver goes only when the app does.
                let _ = send.send(action);
                waker.request_repaint();
            }
        }));
        Ok(Self { _menu: menu, items: built.items, recent, recent_shown: Some(native_menu::recent_menu(&state, &recent_labels)), clicks, shown: Some(state) })
    }

    /// Runs the items clicked since the last frame.
    pub fn run_clicks(&mut self, shell: &mut Shell, ctx: &egui::Context) {
        while let Ok(action) = self.clicks.try_recv() {
            shell.run_menu(ctx, action);
            self.shown = None;
        }
    }

    /// Brings the items' enabled and checked marks, and Open Recent's list, in step with the shell.
    pub fn show(&mut self, shell: &mut Shell) {
        let state = shell.menu_state();
        let recent = native_menu::recent_menu(&state, &labels(shell, &state));
        if self.recent_shown.as_ref() != Some(&recent) {
            while self.recent.remove_at(0).is_some() {}
            let mut built = Built { about: AboutMetadata::default(), items: Vec::new(), recent: None };
            if let Err(e) = built.fill(&self.recent, &recent) {
                eprintln!("swarmception: Open Recent's items: {e}");
            }
            self.recent_shown = Some(recent);
        }
        if self.shown == Some(state) {
            return;
        }
        for (action, item) in &self.items {
            item.show(&state, *action);
        }
        self.shown = Some(state);
    }
}

/// Open Recent's labels: each repository `repo.recent` lists, as the window's bar shows it.
fn labels(shell: &Shell, state: &MenuState) -> Vec<String> {
    (0..state.recent).filter_map(|n| shell.recent_label(n)).collect()
}

/// The bar's menus as muda's, and the items made on the way.
struct Built {
    about: AboutMetadata,
    items: Vec<(MenuAction, Item)>,
    /// Open Recent's submenu, once made.
    recent: Option<Submenu>,
}

impl Built {
    /// `bar_menu` as a submenu (Open Recent's by its id).
    fn submenu(&mut self, bar_menu: &native_menu::Menu) -> Result<Submenu, muda::Error> {
        let submenu = if bar_menu.label == MenuAction::Recent.label() {
            Submenu::with_id(MenuAction::Recent.id(), &bar_menu.label, true)
        } else {
            Submenu::new(&bar_menu.label, true)
        };
        self.fill(&submenu, bar_menu)?;
        Ok(submenu)
    }

    /// `bar_menu`'s entries appended to `submenu`; the commands' items kept, to update; Open
    /// Recent's repositories not (the list is rebuilt whole when it changes).
    fn fill(&mut self, submenu: &Submenu, bar_menu: &native_menu::Menu) -> Result<(), muda::Error> {
        for entry in &bar_menu.entries {
            match entry {
                Entry::Separator => submenu.append(&PredefinedMenuItem::separator())?,
                Entry::Menu(inner) => {
                    let made = self.submenu(inner)?;
                    submenu.append(&made)?;
                    if inner.label == MenuAction::Recent.label() {
                        self.recent = Some(made);
                    }
                }
                Entry::System { kind, label } => submenu.append(&system(*kind, label, &self.about))?,
                Entry::Item(item) => {
                    let accelerator = item.accelerator.as_deref().and_then(|text| text.parse::<Accelerator>().ok());
                    let id = item.action.id();
                    let made = match item.checked {
                        Some(checked) => Item::Check(CheckMenuItem::with_id(id, &item.label, item.enabled, checked, accelerator)),
                        None => Item::Plain(MenuItem::with_id(id, &item.label, item.enabled, accelerator)),
                    };
                    match &made {
                        Item::Plain(native) => submenu.append(native)?,
                        Item::Check(native) => submenu.append(native)?,
                    }
                    if !matches!(item.action, MenuAction::OpenRecent(_)) {
                        self.items.push((item.action, made));
                    }
                }
            }
        }
        Ok(())
    }
}

/// The system's own item `kind`, labelled `label`.
fn system(kind: System, label: &str, about: &AboutMetadata) -> PredefinedMenuItem {
    let label = Some(label);
    match kind {
        System::About => PredefinedMenuItem::about(label, Some(about.clone())),
        System::Services => PredefinedMenuItem::services(label),
        System::Hide => PredefinedMenuItem::hide(label),
        System::HideOthers => PredefinedMenuItem::hide_others(label),
        System::ShowAll => PredefinedMenuItem::show_all(label),
        System::Minimize => PredefinedMenuItem::minimize(label),
        System::Zoom => PredefinedMenuItem::zoom(label),
        System::BringAllToFront => PredefinedMenuItem::bring_all_to_front(label),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every accelerator the bar makes is one muda reads, so no item loses its key silently (`fill`
    /// drops one it can't parse).
    #[test]
    fn muda_reads_every_accelerator() {
        let state = MenuState { recent: 0, ..test_state() };
        let mut texts = Vec::new();
        for menu in native_menu::bar(PRODUCT, &state, &[]) {
            collect(&menu.entries, &mut texts);
        }
        assert!(texts.len() >= 5, "{texts:?}");
        for text in &texts {
            assert!(text.parse::<Accelerator>().is_ok(), "muda can't read {text}");
        }
        assert_eq!(
            "Cmd+Shift+R".parse::<Accelerator>().ok(),
            native_menu::accelerator(swarmception_engine::command::Shortcut::command_shift('R')).parse().ok()
        );
    }

    fn collect(entries: &[Entry], into: &mut Vec<String>) {
        for entry in entries {
            match entry {
                Entry::Item(item) => into.extend(item.accelerator.clone()),
                Entry::Menu(menu) => collect(&menu.entries, into),
                _ => {}
            }
        }
    }

    fn test_state() -> MenuState {
        let ctx = egui::Context::default();
        Shell::new(&ctx, None).menu_state()
    }

    #[test]
    fn window_tabbing_goes_off_and_never_panics() {
        // Off the main thread (where the harness usually runs a test) it only says so.
        no_window_tabs();
        if let Some(mtm) = MainThreadMarker::new() {
            assert!(!NSWindow::allowsAutomaticWindowTabbing(mtm));
        }
    }
}
