//! The app's state across a restart (#53): one typed, versioned document per repository
//! ([`AppState`]: the shell's [`UiState`], the rows spec, the window's size and place, and the
//! timeline's view with the widget's own `state.save`), kept under the platform's app-data
//! directory ([`platform_dir`]), keyed by the repository's canonical path ([`repo_key`]).
//!
//! [`StateStore::load`] never fails: a missing file is a fresh start, and a corrupt one, another
//! repository's, or an older or newer build's is read where it can be, section by section, the
//! rest left at its defaults, with a [`Loaded::note`] saying what was left out. [`Keeper`] writes
//! the state when it changes, once it has held still for [`QUIET`] seconds (at most every
//! [`MOST`] seconds while it keeps changing), and the app writes it once more on quit; each write
//! is whole or not at all (a temporary file renamed over the old).

use std::path::{Path, PathBuf};
use std::time::Duration;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use swarmception_domain::RowsSpec;

use crate::timeline::TimelineView;
use crate::{DetailsView, FactoryView, PaneSizes, Panes, UI_STATE_VERSION, UiState};

/// [`AppState`]'s shape. A file of another version still reads: an older one has fewer
/// sections, a newer one's sections this build doesn't know are skipped.
pub const APP_STATE_VERSION: u32 = 1;

/// The environment variable that names the directory the state is kept in, over the
/// platform's (an agent's or a test's scratch directory, so the owner's state is left alone).
pub const STATE_DIR_VAR: &str = "SWARMCEPTION_STATE_DIR";

/// Seconds the state holds still before it is written.
pub const QUIET: f64 = 1.0;
/// Seconds a state that keeps changing (play running) waits at most before it is written.
pub const MOST: f64 = 5.0;
/// The largest state file read.
const MAX_FILE: u64 = 4 * 1024 * 1024;

/// What the app keeps of a repository between runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    /// [`APP_STATE_VERSION`] when this build wrote it.
    pub version: u32,
    /// The repository's canonical path ([`repo_key`]): a file whose path differs is another's.
    pub repo: String,
    /// The panes shown and their sizes, the factory's settings and what the details are of.
    /// The factory's *t* is kept only when it is not following the timeline, and play is off.
    pub ui: UiState,
    /// The tickets flat or by worker, collapsed or not.
    pub rows: RowsSpec,
    /// The window's size and place on screen.
    pub window: Option<WindowPlace>,
    /// The timeline's view: the widget's own saved state and the window as clock times.
    pub timeline: Option<TimelineView>,
}

impl AppState {
    /// A fresh state for `repo`: every section at its default.
    #[must_use]
    pub fn new(repo: &Path) -> Self {
        Self { version: APP_STATE_VERSION, repo: repo.display().to_string(), ui: UiState::default(), rows: RowsSpec::default(), window: None, timeline: None }
    }
}

/// The window on screen, in points: its outer top-left corner and its inner size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WindowPlace {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl WindowPlace {
    /// The size and place, when they are ones a window can take: finite, at least `min` and at
    /// most 16384 points across, its corner within ±32768 points.
    #[must_use]
    pub fn sane(self, min: (f32, f32)) -> Option<Self> {
        let size = |v: f32, least: f32| v.is_finite() && (least..=16_384.0).contains(&v);
        let place = |v: f32| v.is_finite() && (-32_768.0..=32_768.0).contains(&v);
        (size(self.width, min.0) && size(self.height, min.1) && place(self.x) && place(self.y)).then_some(self)
    }
}

/// The repository `path` is in, as the state is keyed: its canonical path, up to the directory
/// that holds its `.git` (a worktree's own); the canonical path itself when none does.
#[must_use]
pub fn repo_key(path: &Path) -> PathBuf {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    path.ancestors().find(|dir| dir.join(".git").exists()).map_or(path.clone(), Path::to_path_buf)
}

/// The platform's directory for Swarmception's state: `~/Library/Application Support/swarmception`
/// on macOS, `%APPDATA%\swarmception` on Windows, `$XDG_STATE_HOME/swarmception` (else
/// `~/.local/state/swarmception`) elsewhere; none when the variables it needs aren't set.
#[must_use]
pub fn platform_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Application Support"))
    } else if cfg!(windows) {
        var("APPDATA")
    } else {
        var("XDG_STATE_HOME").or_else(|| var("HOME").map(|h| h.join(".local/state")))
    };
    base.map(|b| b.join("swarmception"))
}

/// The directory named by [`STATE_DIR_VAR`], when it is set.
#[must_use]
pub fn dir_from_env() -> Option<PathBuf> {
    std::env::var_os(STATE_DIR_VAR).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// 64-bit FNV-1a: a hash that stays the same from build to build, unlike std's.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3))
}

/// Where the states are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateStore {
    pub dir: PathBuf,
}

/// What [`StateStore::load`] found.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// The file it was read from, or would have been.
    pub file: PathBuf,
    /// What read; none when there was no file, or nothing in it read.
    pub state: Option<AppState>,
    /// What was left out and why, for the status bar to say once; none when it all read.
    pub note: Option<String>,
}

impl StateStore {
    #[must_use]
    pub const fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The file `repo`'s state is kept in: `state/<its name>-<a hash of its path>.json`.
    #[must_use]
    pub fn file(&self, repo: &Path) -> PathBuf {
        let name = repo.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let name: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')).take(40).collect();
        let hash = fnv1a(repo.as_os_str().as_encoded_bytes());
        self.dir.join("state").join(format!("{}{hash:016x}.json", if name.is_empty() { String::new() } else { format!("{name}-") }))
    }

    /// `repo`'s saved state, read where it can be.
    #[must_use]
    pub fn load(&self, repo: &Path) -> Loaded {
        let file = self.file(repo);
        let ignored = |why: String| Some(format!("The saved state {why}: starting afresh"));
        let text = match std::fs::metadata(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded { file, state: None, note: None },
            Err(e) => Err(e.to_string()),
            Ok(meta) if meta.len() > MAX_FILE => Err(format!("is over {MAX_FILE} bytes")),
            Ok(_) => std::fs::read_to_string(&file).map_err(|e| e.to_string()),
        };
        let (state, note) = match text {
            Ok(text) => read(&text, repo).map_or_else(|why| (None, ignored(why)), |(state, note)| (Some(state), note)),
            Err(why) => (None, ignored(format!("didn't read ({why})"))),
        };
        Loaded { file, state, note }
    }

    /// Writes `state` to its repository's file, whole or not at all.
    ///
    /// # Errors
    ///
    /// Why it couldn't be written, as text to show.
    pub fn save(&self, state: &AppState) -> Result<PathBuf, String> {
        let file = self.file(Path::new(&state.repo));
        let failed = |e: std::io::Error| format!("can't save the state to {}: {e}", file.display());
        let text = serde_json::to_string_pretty(state).map_err(|e| format!("can't save the state: {e}"))?;
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        let temporary = file.with_extension(format!("json.{}.tmp", std::process::id()));
        std::fs::write(&temporary, text + "\n").map_err(failed)?;
        std::fs::rename(&temporary, &file).map_err(|e| {
            let _ = std::fs::remove_file(&temporary);
            failed(e)
        })?;
        Ok(file)
    }
}

/// A saved state's sections, each read on its own so one that doesn't read loses only itself.
/// Not `deny_unknown_fields`: a newer build's sections are skipped.
#[derive(Deserialize)]
struct Sections {
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    ui: Option<serde_json::Value>,
    #[serde(default)]
    rows: Option<serde_json::Value>,
    #[serde(default)]
    window: Option<serde_json::Value>,
    #[serde(default)]
    timeline: Option<serde_json::Value>,
}

/// The shell's state's sections, read as [`Sections`] are.
#[derive(Deserialize)]
struct UiSections {
    #[serde(default)]
    panes: Option<serde_json::Value>,
    #[serde(default)]
    factory: Option<serde_json::Value>,
    #[serde(default)]
    details: Option<serde_json::Value>,
    #[serde(default)]
    sizes: Option<serde_json::Value>,
}

/// `value` as a `T`, its default when it is missing or doesn't read (`name` added to `skipped`).
fn section<T: DeserializeOwned + Default>(value: Option<serde_json::Value>, name: &'static str, skipped: &mut Vec<&'static str>) -> T {
    match value.map(serde_json::from_value::<T>) {
        None => T::default(),
        Some(Ok(read)) => read,
        Some(Err(_)) => {
            skipped.push(name);
            T::default()
        }
    }
}

/// The state in `text`, saved for `repo`, and a note on what was left out; an error saying why
/// none of it reads.
fn read(text: &str, repo: &Path) -> Result<(AppState, Option<String>), String> {
    let sections: Sections = serde_json::from_str(text).map_err(|e| format!("is corrupt ({e})"))?;
    let key = repo.display().to_string();
    if let Some(theirs) = sections.repo.as_ref().filter(|theirs| **theirs != key) {
        return Err(format!("is another repository's ({theirs})"));
    }
    let mut skipped = Vec::new();
    let ui: Option<UiSections> = section(sections.ui, "ui", &mut skipped);
    let ui = ui.map_or_else(UiState::default, |ui| UiState {
        version: UI_STATE_VERSION,
        panes: section::<Panes>(ui.panes, "panes", &mut skipped),
        factory: section::<FactoryView>(ui.factory, "factory", &mut skipped),
        details: section::<DetailsView>(ui.details, "details", &mut skipped),
        sizes: section::<PaneSizes>(ui.sizes, "sizes", &mut skipped),
    });
    let state = AppState {
        version: APP_STATE_VERSION,
        repo: key,
        ui,
        rows: section(sections.rows, "rows", &mut skipped),
        window: section(sections.window, "window", &mut skipped),
        timeline: section(sections.timeline, "timeline", &mut skipped),
    };
    let version = sections.version.unwrap_or(0);
    let newer =
        (version > APP_STATE_VERSION).then(|| format!("The saved state is a newer Swarmception's (version {version}; this one reads {APP_STATE_VERSION})"));
    let left = (!skipped.is_empty()).then(|| skipped.join(", "));
    let note = match (newer, left) {
        (Some(newer), Some(left)) => Some(format!("{newer}: kept what this one reads, left out {left}")),
        (Some(newer), None) => Some(format!("{newer}: kept what this one reads")),
        (None, Some(left)) => Some(format!("The saved state's {left} didn't read: left at the defaults")),
        (None, None) => None,
    };
    Ok((state, note))
}

/// How a run keeps its state, as `ui.inspect` reports it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Keeping {
    /// Neither read nor written: an agent's run (`--screenshot`, `--control`) with no state
    /// directory named, or a shell no state was given.
    #[default]
    Off,
    /// Started without it (`--fresh`), written as it goes.
    Fresh,
    /// Read at the start, written as it goes.
    Kept,
}

/// Where the state is kept, as `ui.inspect` reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SavedInspect {
    pub keeping: Keeping,
    /// The file it is written to.
    pub file: Option<String>,
    /// The file it was read from at the start; none when it started afresh.
    pub loaded_from: Option<String>,
    /// What the status bar says about it: what was left out of the file, or why it couldn't be
    /// written.
    pub note: Option<String>,
    /// How many times it was written this run.
    pub writes: u32,
}

/// Keeps a shell's state in its file: written when it changes and holds still, and on quit.
#[derive(Debug)]
pub struct Keeper {
    store: StateStore,
    repo: PathBuf,
    keeping: Keeping,
    loaded_from: Option<PathBuf>,
    /// What the file holds: the state last written or read.
    written: Option<AppState>,
    /// A state not written yet, and when it was first seen, and when it last changed.
    pending: Option<(AppState, f64, f64)>,
    note: Option<String>,
    writes: u32,
}

impl Keeper {
    /// Keeps `repo`'s state in `store`; `loaded` is what was read at the start (none for a fresh
    /// start).
    #[must_use]
    pub fn new(store: StateStore, repo: &Path, loaded: Option<&Loaded>) -> Self {
        Self {
            store,
            repo: repo.to_path_buf(),
            keeping: if loaded.is_some() { Keeping::Kept } else { Keeping::Fresh },
            loaded_from: loaded.filter(|l| l.state.is_some()).map(|l| l.file.clone()),
            written: loaded.and_then(|l| l.state.clone()),
            pending: None,
            note: loaded.and_then(|l| l.note.clone()),
            writes: 0,
        }
    }

    /// The repository it keeps the state of.
    #[must_use]
    pub fn repo(&self) -> &Path {
        &self.repo
    }

    /// What the status bar says, once: what was left out of the file, or why a write failed.
    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// The status bar's note was read: it goes.
    pub fn dismiss(&mut self) {
        self.note = None;
    }

    /// Takes in the state at `now` (seconds): writes it once it has held still for [`QUIET`]
    /// seconds, or been changing for [`MOST`]. How long until it should be looked at again, when
    /// a write waits.
    pub fn observe(&mut self, state: AppState, now: f64) -> Option<Duration> {
        if self.written.as_ref() == Some(&state) {
            self.pending = None;
            return None;
        }
        let (first, changed) = match self.pending.take() {
            Some((seen, first, changed)) => (first, if seen == state { changed } else { now }),
            None => (now, now),
        };
        if now - changed >= QUIET || now - first >= MOST {
            self.write(state);
            return None;
        }
        self.pending = Some((state, first, changed));
        Some(Duration::from_secs_f64((QUIET - (now - changed)).clamp(0.05, QUIET)))
    }

    /// Writes `state` now (on quit), whether or not it changed.
    ///
    /// # Errors
    ///
    /// Why it couldn't be written.
    pub fn write_now(&mut self, state: AppState) -> Result<PathBuf, String> {
        self.pending = None;
        let written = self.store.save(&state);
        self.landed(state, written.as_ref().err().cloned());
        written
    }

    fn write(&mut self, state: AppState) {
        let failed = self.store.save(&state).err();
        self.landed(state, failed);
    }

    /// A write's outcome: a failure noted (and not tried again until the state moves on).
    fn landed(&mut self, state: AppState, failed: Option<String>) {
        match failed {
            None => self.writes = self.writes.saturating_add(1),
            Some(why) => self.note = Some(why),
        }
        self.written = Some(state);
    }

    /// What `ui.inspect` says of it.
    #[must_use]
    pub fn inspect(&self) -> SavedInspect {
        SavedInspect {
            keeping: self.keeping,
            file: Some(self.store.file(&self.repo).display().to_string()),
            loaded_from: self.loaded_from.as_ref().map(|f| f.display().to_string()),
            note: self.note.clone(),
            writes: self.writes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(what: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("worker21-saved-{what}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_state_is_written_whole_and_reads_back() {
        let dir = scratch("roundtrip");
        let store = StateStore::new(dir.clone());
        let repo = Path::new("/Volumes/Data/dev/mc-engine");
        assert_eq!(store.load(repo), Loaded { file: store.file(repo), state: None, note: None }, "no file: a fresh start, nothing said");
        let mut state = AppState::new(repo);
        state.ui.panes.factory = false;
        state.rows.grouping = swarmception_domain::Grouping::Workers;
        state.window = Some(WindowPlace { x: 40.0, y: 60.0, width: 1200.0, height: 800.0 });
        let file = store.save(&state).unwrap();
        assert_eq!(file, store.file(repo));
        assert!(file.starts_with(dir.join("state")) && file.file_name().unwrap().to_string_lossy().starts_with("mc-engine-"), "{}", file.display());
        assert_eq!(store.load(repo), Loaded { file: file.clone(), state: Some(state), note: None });
        assert_eq!(std::fs::read_dir(dir.join("state")).unwrap().count(), 1, "no temporary file left");
        // Another repository has its own file.
        assert_ne!(store.file(Path::new("/Volumes/Data/dev/swarmception")), file);
        assert_ne!(store.file(Path::new("/elsewhere/mc-engine")), file);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn corrupt_foreign_older_and_newer_files_read_where_they_can() {
        let dir = scratch("odd");
        let store = StateStore::new(dir.clone());
        let repo = Path::new("/r/swarm");
        let file = store.file(repo);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let load = |text: &str| {
            std::fs::write(&file, text).unwrap();
            store.load(repo)
        };
        for corrupt in ["", "{", "[1, 2]", "null", "\u{0}\u{1}garbage"] {
            let loaded = load(corrupt);
            assert_eq!(loaded.state, None, "{corrupt:?}");
            assert!(loaded.note.as_deref().is_some_and(|n| n.contains("is corrupt") && n.contains("starting afresh")), "{corrupt:?}: {:?}", loaded.note);
        }
        let foreign = load(r#"{"version": 1, "repo": "/r/other"}"#);
        assert!(foreign.state.is_none() && foreign.note.unwrap().contains("another repository's"));

        // An older file (no sizes, no window, no timeline) reads with their defaults, silently.
        let older = load(r#"{"version": 0, "repo": "/r/swarm", "ui": {"version": 3, "panes": {"factory": false}}, "rows": {"collapse": true}}"#);
        let state = older.state.unwrap();
        assert_eq!((state.ui.panes.factory, state.ui.panes.timeline, state.rows.collapse), (false, true, true));
        assert_eq!((state.ui.sizes, state.window, older.note), (PaneSizes::default(), None, None));

        // A newer one: its new sections skipped, a section that changed shape left out, the rest kept.
        let newer = load(
            r#"{"version": 9, "repo": "/r/swarm", "ui": {"panes": {"details": false}, "factory": {"speed": "fortnight"}, "lighting": 3},
                "rows": {"grouping": "workers"}, "window": "maximised", "plugins": [1, 2]}"#,
        );
        let state = newer.state.unwrap();
        assert!(!state.ui.panes.details);
        assert_eq!(state.ui.factory, FactoryView::default(), "a speed it doesn't know: the section's defaults");
        assert_eq!((state.rows.grouping, state.window), (swarmception_domain::Grouping::Workers, None));
        let note = newer.note.unwrap();
        assert!(note.contains("newer Swarmception's (version 9") && note.contains("left out factory, window"), "{note}");
        // Too big to be one.
        let big = load(&" ".repeat(usize::try_from(MAX_FILE).unwrap() + 1));
        assert!(big.state.is_none() && big.note.unwrap().contains("over"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_keeper_writes_once_the_state_holds_still() {
        let dir = scratch("keeper");
        let store = StateStore::new(dir.clone());
        let repo = Path::new("/r/keeper");
        let mut keeper = Keeper::new(store.clone(), repo, None);
        assert_eq!(keeper.inspect().keeping, Keeping::Fresh);
        let mut state = AppState::new(repo);
        assert!(keeper.observe(state.clone(), 0.0).is_some(), "a new state waits");
        assert!(keeper.observe(state.clone(), 0.5).is_some());
        assert!(keeper.observe(state.clone(), 1.0).is_none(), "held still a second: written");
        assert_eq!((keeper.inspect().writes, store.load(repo).state.as_ref()), (1, Some(&state)));
        assert!(keeper.observe(state.clone(), 9.0).is_none(), "unchanged: nothing to write");
        assert_eq!(keeper.inspect().writes, 1);
        // Changing every frame (play): written within MOST seconds all the same.
        for (n, t) in (0..).zip([10.0, 11.0, 12.0, 13.0, 14.0, 15.0]) {
            state.ui.factory.t = Some(swarmception_model::Timestamp(n));
            keeper.observe(state.clone(), t);
        }
        assert_eq!(keeper.inspect().writes, 2);
        // On quit, whatever it is.
        state.ui.panes.details = false;
        keeper.write_now(state.clone()).unwrap();
        assert_eq!((keeper.inspect().writes, store.load(repo).state), (3, Some(state.clone())));

        // A directory it can't write: noted once, not tried again until the state moves on.
        let blocked = dir.join("blocked");
        std::fs::write(&blocked, "a file, not a directory").unwrap();
        let mut keeper = Keeper::new(StateStore::new(blocked), repo, None);
        keeper.observe(state.clone(), 0.0);
        keeper.observe(state.clone(), 2.0);
        let note = keeper.note().unwrap().to_owned();
        assert!(note.starts_with("can't save the state"), "{note}");
        keeper.dismiss();
        assert!(keeper.observe(state.clone(), 3.0).is_none() && keeper.note().is_none(), "the same state isn't tried again");
        assert!(keeper.write_now(state).is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn window_places_and_repo_keys() {
        let place = WindowPlace { x: 10.0, y: 20.0, width: 1400.0, height: 860.0 };
        assert_eq!(place.sane((480.0, 320.0)), Some(place));
        for bad in [
            WindowPlace { width: 100.0, ..place },
            WindowPlace { height: f32::NAN, ..place },
            WindowPlace { x: f32::INFINITY, ..place },
            WindowPlace { y: -1.0e6, ..place },
            WindowPlace { width: 1.0e6, ..place },
        ] {
            assert_eq!(bad.sane((480.0, 320.0)), None, "{bad:?}");
        }
        // A directory inside a repository is keyed by the repository.
        let dir = scratch("key");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::create_dir_all(dir.join("crates/ui")).unwrap();
        let top = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(repo_key(&dir.join("crates/ui")), top);
        assert_eq!(repo_key(&dir), top);
        assert_eq!(repo_key(Path::new("/no/such/place")), PathBuf::from("/no/such/place"));
        std::fs::remove_dir_all(dir).ok();
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c, "FNV-1a's published value");
    }
}
