use super::*;
use crate::menu::CommandsEnabled;
use swarmception_domain::{Collapse, Grouping};

fn state() -> MenuState {
    MenuState {
        commands: CommandsEnabled(u128::MAX),
        can_fit: true,
        timeline: true,
        factory: false,
        details: true,
        workers: false,
        grouping: Grouping::Workers,
        collapse: Collapse::Off,
        fold_quiet: true,
        merge_marks: false,
        live: false,
        recent: 2,
    }
}

fn labels(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|e| match e {
            Entry::Item(item) => item.label.clone(),
            Entry::Menu(menu) => format!("{} ▸", menu.label),
            Entry::System { label, .. } => format!("({label})"),
            Entry::Separator => "—".to_owned(),
        })
        .collect()
}

fn items(entries: &[Entry]) -> Vec<&Item> {
    entries
        .iter()
        .flat_map(|e| match e {
            Entry::Item(item) => vec![item],
            Entry::Menu(menu) => items(&menu.entries),
            _ => Vec::new(),
        })
        .collect()
}

/// The bar is macOS's: the application menu, File, View, Window; the application menu's own items
/// the system's, Quit the window's with ⌘Q; File without its Quit, Open Recent a submenu.
#[test]
fn the_bar_is_macoss_layout() {
    let recent = ["mc-engine — ~/dev".to_owned(), "swarmception — ~/dev".to_owned()];
    let bar = bar("Swarmception", &state(), &recent);
    let menus: Vec<(&str, Role)> = bar.iter().map(|m| (m.label.as_str(), m.role)).collect();
    assert_eq!(menus, [("Swarmception", Role::App), ("File", Role::Plain), ("View", Role::Plain), ("Window", Role::Window)]);
    assert_eq!(
        labels(&bar[0].entries),
        ["(About Swarmception)", "—", "(Services)", "—", "(Hide Swarmception)", "(Hide Others)", "(Show All)", "—", "Quit Swarmception"]
    );
    let quit = items(&bar[0].entries)[0];
    assert_eq!((quit.action, quit.accelerator.as_deref(), quit.enabled), (MenuAction::Quit, Some("Cmd+Q"), true));
    assert_eq!(labels(&bar[1].entries), ["Open…", "Open Recent ▸", "—", "Reload", "Fetch and Reload"]);
    let Entry::Menu(open_recent) = &bar[1].entries[1] else { panic!("Open Recent is a submenu") };
    assert_eq!(labels(&open_recent.entries), ["mc-engine — ~/dev", "swarmception — ~/dev", "—", "Clear Menu"]);
    assert_eq!(items(&open_recent.entries)[1].action, MenuAction::OpenRecent(1));
    assert_eq!(labels(&bar[3].entries), ["(Minimize)", "(Zoom)", "—", "(Bring All to Front)"]);
}

/// Each item is the window bar's: its action, its key, enabled and checked as the state has it.
#[test]
fn the_items_follow_the_state() {
    let mut state = state();
    let bar = bar("Swarmception", &state, &[]);
    let view = items(&bar[2].entries);
    let find = |action: MenuAction| view.iter().find(|i| i.action == action).copied().unwrap();
    assert_eq!(find(MenuAction::Factory).checked, Some(false));
    assert_eq!(find(MenuAction::Grouping(Grouping::Workers)).checked, Some(true));
    assert_eq!(find(MenuAction::FoldQuiet).checked, Some(true));
    assert_eq!((find(MenuAction::Fit).checked, find(MenuAction::Fit).enabled), (None, true));
    assert_eq!(find(MenuAction::Live).accelerator.as_deref(), Some("Cmd+L"));
    let file = items(&bar[1].entries);
    let reload = file.iter().find(|i| i.action == MenuAction::Command("repo.reload")).unwrap();
    let fetch = file.iter().find(|i| i.action == MenuAction::Command("repo.fetch")).unwrap();
    assert_eq!((reload.accelerator.as_deref(), fetch.accelerator.as_deref()), (Some("Cmd+R"), Some("Cmd+Shift+R")));
    // Nothing can run, and nothing to fit: greyed, as the window's bar greys them.
    state.commands = CommandsEnabled(0);
    state.can_fit = false;
    let bar = super::bar("Swarmception", &state, &[]);
    assert!(items(&bar[1].entries).iter().filter(|i| matches!(i.action, MenuAction::Command(_))).all(|i| !i.enabled));
    assert!(!items(&bar[2].entries).iter().find(|i| i.action == MenuAction::Fit).unwrap().enabled);
    // Every item of the window's bar is in the system's, but Quit moved to the application menu.
    let all: Vec<MenuAction> = bar.iter().flat_map(|m| items(&m.entries)).map(|i| i.action).collect();
    for action in MenuAction::file_items().into_iter().chain(MenuAction::VIEW).flatten().filter(|a| *a != MenuAction::Recent) {
        assert!(all.contains(&action), "{action:?}");
    }
}

/// A key the system's own items take keeps its item but not the key; the rest are muda's form.
#[test]
fn a_system_key_stays_the_systems() {
    assert_eq!(accelerator(Shortcut::command('o')), "Cmd+O");
    assert_eq!(accelerator(Shortcut::command_shift('R')), "Cmd+Shift+R");
    assert!(SYSTEM_KEYS.contains(&'H') && SYSTEM_KEYS.contains(&'M'));
    let bar = bar("Swarmception", &state(), &[]);
    let keys: Vec<String> = bar.iter().flat_map(|m| items(&m.entries)).filter_map(|i| i.accelerator.clone()).collect();
    assert!(keys.iter().all(|k| k != "Cmd+H" && k != "Cmd+M"), "{keys:?}");
    let mut sorted = keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), keys.len(), "each key once: {keys:?}");
}
