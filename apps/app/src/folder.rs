//! macOS: File ▸ Open…'s folder picker (#69), AppKit's own open panel, folders only, starting in
//! the open repository's parent. A thin shell: what it returns goes to the shell's
//! `open_repo`, the typed `repo.open` that tests drive directly.

use std::path::{Path, PathBuf};

use objc2::MainThreadMarker;
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
use objc2_foundation::{NSString, NSURL};
use swarmception_ui::Picked;

/// Shows the open panel, modal, from `start`: the folder chosen, or cancelled; off the main
/// thread, where AppKit can't show it, no picker (the window asks for a path).
pub fn pick(start: Option<&Path>) -> Picked {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("swarmception: not on the main thread; no folder panel");
        return Picked::NoPicker;
    };
    shown(mtm, start).map_or(Picked::Cancelled, Picked::Folder)
}

/// The panel shown on the main thread: the folder chosen, or none when cancelled.
fn shown(mtm: MainThreadMarker, start: Option<&Path>) -> Option<PathBuf> {
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setAllowsMultipleSelection(false);
    panel.setCanCreateDirectories(false);
    panel.setMessage(Some(&NSString::from_str("Open a swarm's clone: a git repository with a tracking branch")));
    panel.setPrompt(Some(&NSString::from_str("Open")));
    if let Some(start) = start {
        panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(&NSString::from_str(&start.display().to_string()))));
    }
    if panel.runModal() != NSModalResponseOK {
        return None;
    }
    let path = panel.URL()?.path()?;
    Some(PathBuf::from(path.to_string()))
}
