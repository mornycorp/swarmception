//! The app's state across a restart (#53): one typed, versioned document per repository
//! ([`AppState`]: the shell's [`UiState`], the rows spec, the window's size and place, and the
//! timeline's view with the widget's own `state.save`), kept under the platform's app-data
//! directory ([`platform_dir`]), keyed by the repository's canonical path ([`repo_key`]).
//!
//! [`StateStore::load`] never fails: a missing file is a fresh start; an older build's is
//! migrated (each earlier version's shape read as this one's, `read`); and a file this build can't
//! wholly read is **the person's, left alone** (#165, docs/app-architecture.md › State on disk): a
//! newer build's, a corrupt one or another repository's is refused (the window starts from the
//! defaults), and one with a section that doesn't read keeps what does. Either way
//! [`Loaded::left_alone`] holds the [`Keeper`] from writing over it this run, and a [`Loaded::note`]
//! says so. Otherwise the [`Keeper`] writes
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
use crate::{DetailsView, FactoryView, PaneSizes, Panes, UI_STATE_VERSION, UiState, WorkersView};

/// [`AppState`]'s shape. An older file is migrated (fewer sections, each read as this version's);
/// a newer one is refused and left alone (#165). 2: the rows' collapse
/// a mode's name (`off`, `empty`, `window`, #74), where 1 wrote `true` or `false`, which still
/// read as `empty` and `off`.
pub const APP_STATE_VERSION: u32 = 2;

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
    /// The tickets flat or by worker, collapsed how (#74: to the window too).
    pub rows: RowsSpec,
    /// The window's size and place on screen.
    pub window: Option<WindowPlace>,
    /// The timeline's view: the widget's own saved state and the window as clock times.
    pub timeline: Option<TimelineView>,
    /// Live (#54) as the person left it.
    #[serde(default)]
    pub live: LiveChoice,
}

/// The live watch (#54) as the person left it: on or off only when they chose it (`live.get`
/// says whether it is still the default), and its interval; restored through `live.set`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveChoice {
    /// None: nobody chose, so the default applies (on when the swarm is active).
    pub on: Option<bool>,
    /// Seconds between checks.
    pub interval_seconds: Option<u64>,
}

impl AppState {
    /// A fresh state for `repo`: every section at its default.
    #[must_use]
    pub fn new(repo: &Path) -> Self {
        Self {
            version: APP_STATE_VERSION,
            repo: repo.display().to_string(),
            ui: UiState::default(),
            rows: RowsSpec::default(),
            window: None,
            timeline: None,
            live: LiveChoice::default(),
        }
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

    /// The window on screen (#68, G17): as it is when the monitors cover it; else, partly or
    /// wholly off them (a monitor since unplugged), moved onto the monitor it overlaps most, or
    /// the nearest when it overlaps none, shrunk to fit it and held inside it, its title bar
    /// ([`TITLE_BAR`]) counted. With no monitor known, as it is.
    #[must_use]
    pub fn onto(self, monitors: &[Monitor]) -> Self {
        let window = Monitor { x: self.x, y: self.y, width: self.width, height: self.height + TITLE_BAR };
        let covered: f32 = monitors.iter().map(|m| m.overlap(&window)).sum();
        if monitors.is_empty() || covered >= window.area() - 1.0 {
            return self;
        }
        let best = monitors.iter().copied().max_by(|a, b| {
            let (oa, ob) = (a.overlap(&window), b.overlap(&window));
            oa.total_cmp(&ob).then_with(|| b.gap(&window).total_cmp(&a.gap(&window)))
        });
        let Some(m) = best else { return self };
        let width = self.width.min(m.width);
        let height = self.height.min((m.height - TITLE_BAR).max(1.0));
        let x = self.x.clamp(m.x, (m.x + m.width - width).max(m.x));
        let y = self.y.clamp(m.y, (m.y + m.height - height - TITLE_BAR).max(m.y));
        Self { x, y, width, height }
    }
}

/// The height a window's title bar adds over its inner size, as [`WindowPlace::onto`] counts it.
pub const TITLE_BAR: f32 = 28.0;

/// A monitor's area, in points, in the coordinates the window's place is in (as winit tells
/// them: its position and size over its scale factor).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Monitor {
    fn area(&self) -> f32 {
        self.width.max(0.0) * self.height.max(0.0)
    }

    /// The area it shares with `other`.
    fn overlap(&self, other: &Self) -> f32 {
        let w = (self.x + self.width).min(other.x + other.width) - self.x.max(other.x);
        let h = (self.y + self.height).min(other.y + other.height) - self.y.max(other.y);
        w.max(0.0) * h.max(0.0)
    }

    /// How far it is from `other`: 0 when they touch or overlap.
    fn gap(&self, other: &Self) -> f32 {
        let dx = (self.x - (other.x + other.width)).max(other.x - (self.x + self.width)).max(0.0);
        let dy = (self.y - (other.y + other.height)).max(other.y - (self.y + self.height)).max(0.0);
        dx.hypot(dy)
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
    /// The file is there but this build didn't wholly read it (a newer build's, corrupt, another
    /// repository's, or a section that doesn't read): it is never written over this run (#165).
    pub left_alone: bool,
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
        let ignored = |why: String| Some(format!("The saved state {why}: starting afresh, and {LEFT_ALONE}"));
        let text = match std::fs::metadata(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded { file, state: None, note: None, left_alone: false },
            Err(e) => Err(e.to_string()),
            Ok(meta) if meta.len() > MAX_FILE => Err(format!("is over {MAX_FILE} bytes")),
            Ok(_) => std::fs::read_to_string(&file).map_err(|e| e.to_string()),
        };
        let (state, note) = match text {
            Ok(text) => read(&text, repo).map_or_else(|why| (None, ignored(why)), |(state, note)| (Some(state), note)),
            Err(why) => (None, ignored(format!("didn't read ({why})"))),
        };
        // Wholly read, or the person's to fix: a note means something in it didn't read.
        let left_alone = note.is_some();
        Loaded { file, state, note, left_alone }
    }

    /// Writes `state` to its repository's file, whole or not at all.
    ///
    /// # Errors
    ///
    /// Why it couldn't be written, as text to show.
    pub fn save(&self, state: &AppState) -> Result<PathBuf, String> {
        let file = self.file(Path::new(&state.repo));
        let text = serde_json::to_string_pretty(state).map_err(|e| format!("can't save the state: {e}"))?;
        write_whole(&file, &text).map_err(|e| format!("can't save the state to {}: {e}", file.display()))?;
        Ok(file)
    }

    /// The app-wide file of the repositories opened last (#69): `recent.json`, beside the
    /// repositories' own states.
    #[must_use]
    pub fn recent_file(&self) -> PathBuf {
        self.dir.join("recent.json")
    }

    /// The repositories opened last, newest first, as File ▸ Open Recent lists them; none when
    /// there is no file, or it isn't one this build reads (#165: corrupt, or a newer build's, and
    /// then never written over, [`Self::save_recent`]). An older version's reads its list.
    #[must_use]
    pub fn load_recent(&self) -> Vec<PathBuf> {
        let repos = match self.read_recent() {
            Ok(Some(recent)) => recent.repos.unwrap_or_default(),
            Ok(None) | Err(_) => Vec::new(),
        };
        repos.into_iter().filter(|r| !r.is_empty()).take(swarmception_engine::repo_cmds::MAX_RECENT).map(PathBuf::from).collect()
    }

    /// The recent file's list as this build reads it: none when there is no file; why not when it
    /// is there but not this build's to read (and so not its to write over).
    fn read_recent(&self) -> Result<Option<RecentSections>, String> {
        let file = self.recent_file();
        let text = match std::fs::metadata(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
            Ok(meta) if meta.len() > MAX_FILE => return Err(format!("it is over {MAX_FILE} bytes")),
            Ok(_) => std::fs::read_to_string(&file).map_err(|e| e.to_string())?,
        };
        let recent: RecentSections = serde_json::from_str(&text).map_err(|e| format!("it is corrupt ({e})"))?;
        match recent.version.unwrap_or(0) {
            newer if newer > RECENT_VERSION => Err(format!("it is a newer Swarmception's (version {newer}; this one reads {RECENT_VERSION})")),
            _ if recent.repos.is_none() => Err("it has no list of repositories".to_owned()),
            _ => Ok(Some(recent)),
        }
    }

    /// Writes the repositories opened last, whole or not at all; never over a file this build
    /// doesn't read (#165).
    ///
    /// # Errors
    ///
    /// Why it couldn't be written, as text to show; or why the file there is left alone.
    pub fn save_recent(&self, repos: &[String]) -> Result<PathBuf, String> {
        let file = self.recent_file();
        if let Err(why) = self.read_recent() {
            return Err(format!("left the recent repositories' file alone ({why}): {}", file.display()));
        }
        let recent = RecentFile { version: RECENT_VERSION, repos: repos.to_vec() };
        let text = serde_json::to_string_pretty(&recent).map_err(|e| format!("can't save the recent repositories: {e}"))?;
        write_whole(&file, &text).map_err(|e| format!("can't save the recent repositories to {}: {e}", file.display()))?;
        Ok(file)
    }
}

/// [`RecentFile`]'s shape.
pub const RECENT_VERSION: u32 = 1;

/// The repositories opened last, app-wide (#69): what File ▸ Open Recent lists across runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecentFile {
    /// [`RECENT_VERSION`] when this build wrote it.
    pub version: u32,
    /// Their top levels, newest first.
    pub repos: Vec<String>,
}

/// [`RecentFile`] as this and any older version reads: its version and its list, the rest skipped.
#[derive(Deserialize)]
struct RecentSections {
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    repos: Option<Vec<String>>,
}

/// What a note adds when the file is left alone.
const LEFT_ALONE: &str = "the file is left as it is: this run's state isn't saved over it";

/// Writes `text` to `file`, whole or not at all, through the one file writer below the UI (#164).
fn write_whole(file: &Path, text: &str) -> std::io::Result<()> {
    swarmception_source::files::write_atomic(file, format!("{text}\n").as_bytes())
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
    #[serde(default)]
    live: Option<serde_json::Value>,
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
    #[serde(default)]
    folded: Option<serde_json::Value>,
    workers: Option<serde_json::Value>,
    /// Folding the timeline's quiet time (#86, version 10): off in an older state.
    #[serde(default)]
    fold: Option<serde_json::Value>,
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

/// The factory's section without the time of its own an older state kept (#85, `t` up to
/// version 8 of the shell's state): the timeline's playhead, saved with the timeline, is its time.
fn without_time(mut factory: serde_json::Value) -> serde_json::Value {
    if let Some(fields) = factory.as_object_mut() {
        fields.remove("t");
    }
    factory
}

/// A pane's section without the gates pane an older state kept (#128, `gates` in the panes, the
/// sizes and the folds up to version 11 of the shell's state): it is gone.
fn without_gates(mut section: serde_json::Value) -> serde_json::Value {
    if let Some(fields) = section.as_object_mut() {
        fields.remove("gates");
    }
    section
}

/// The state in `text`, saved for `repo`, and a note on what was left out; an error saying why
/// none of it is read: corrupt, another repository's, or a newer build's (#165: refused, not read
/// in part). An older version is migrated: each section read as this version's, the shapes it
/// had read as what they became (version 1's collapse a bool, the shell's state's factory time up
/// to its version 8, its gates pane up to 11).
fn read(text: &str, repo: &Path) -> Result<(AppState, Option<String>), String> {
    let sections: Sections = serde_json::from_str(text).map_err(|e| format!("is corrupt ({e})"))?;
    let key = repo.display().to_string();
    if let Some(theirs) = sections.repo.as_ref().filter(|theirs| **theirs != key) {
        return Err(format!("is another repository's ({theirs})"));
    }
    let version = sections.version.unwrap_or(0);
    if version > APP_STATE_VERSION {
        return Err(format!("is a newer Swarmception's (version {version}; this one reads {APP_STATE_VERSION})"));
    }
    let mut skipped = Vec::new();
    let ui: Option<UiSections> = section(sections.ui, "ui", &mut skipped);
    let ui = ui.map_or_else(UiState::default, |ui| UiState {
        version: UI_STATE_VERSION,
        panes: section::<Panes>(ui.panes.map(without_gates), "panes", &mut skipped),
        factory: section::<FactoryView>(ui.factory.map(without_time), "factory", &mut skipped),
        details: section::<DetailsView>(ui.details, "details", &mut skipped),
        sizes: section::<PaneSizes>(ui.sizes.map(without_gates), "sizes", &mut skipped),
        folded: section::<crate::layout::Folded>(ui.folded.map(without_gates), "folded", &mut skipped),
        workers: section::<WorkersView>(ui.workers, "workers", &mut skipped),
        fold: section::<crate::FoldView>(ui.fold, "fold", &mut skipped),
    });
    let state = AppState {
        version: APP_STATE_VERSION,
        repo: key,
        ui,
        rows: section(sections.rows, "rows", &mut skipped),
        window: section(sections.window, "window", &mut skipped),
        timeline: section(sections.timeline, "timeline", &mut skipped),
        live: section(sections.live, "live", &mut skipped),
    };
    let note = (!skipped.is_empty()).then(|| format!("The saved state's {} didn't read: left at the defaults, and {LEFT_ALONE}", skipped.join(", ")));
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
    /// The file there isn't this build's to write over (#165: a newer build's, corrupt, another
    /// repository's, or a section that didn't read): read where it could be, never written.
    LeftAlone,
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
            keeping: match loaded {
                Some(loaded) if loaded.left_alone => Keeping::LeftAlone,
                Some(_) => Keeping::Kept,
                None => Keeping::Fresh,
            },
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

    /// Where it keeps it.
    #[must_use]
    pub const fn store(&self) -> &StateStore {
        &self.store
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
        if self.keeping == Keeping::LeftAlone || self.written.as_ref() == Some(&state) {
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

    /// Writes `state` now (on quit), whether or not it changed; the file it went to, none when
    /// the file is left alone (#165).
    ///
    /// # Errors
    ///
    /// Why it couldn't be written.
    pub fn write_now(&mut self, state: AppState) -> Result<Option<PathBuf>, String> {
        self.pending = None;
        if self.keeping == Keeping::LeftAlone {
            return Ok(None);
        }
        let written = self.store.save(&state);
        self.landed(state, written.as_ref().err().cloned());
        written.map(Some)
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
        assert_eq!(store.load(repo), Loaded { file: store.file(repo), state: None, note: None, left_alone: false }, "no file: a fresh start, nothing said");
        let mut state = AppState::new(repo);
        state.ui.panes.factory = false;
        state.rows.grouping = swarmception_domain::Grouping::Workers;
        state.window = Some(WindowPlace { x: 40.0, y: 60.0, width: 1200.0, height: 800.0 });
        let file = store.save(&state).unwrap();
        assert_eq!(file, store.file(repo));
        assert!(file.starts_with(dir.join("state")) && file.file_name().unwrap().to_string_lossy().starts_with("mc-engine-"), "{}", file.display());
        assert_eq!(store.load(repo), Loaded { file: file.clone(), state: Some(state), note: None, left_alone: false });
        assert_eq!(std::fs::read_dir(dir.join("state")).unwrap().count(), 1, "no temporary file left");
        // Another repository has its own file.
        assert_ne!(store.file(Path::new("/Volumes/Data/dev/swarmception")), file);
        assert_ne!(store.file(Path::new("/elsewhere/mc-engine")), file);
        std::fs::remove_dir_all(dir).ok();
    }

    /// #69: the recent list is app-wide, one file beside the repositories' states.
    #[test]
    fn the_recent_repositories_are_kept_app_wide() {
        let dir = scratch("recent");
        let store = StateStore::new(dir.clone());
        assert!(store.load_recent().is_empty(), "no file: none");
        let repos = vec!["/r/one".to_owned(), "/r/two".to_owned()];
        let file = store.save_recent(&repos).unwrap();
        assert_eq!(file, dir.join("recent.json"));
        assert_eq!(store.load_recent(), [PathBuf::from("/r/one"), PathBuf::from("/r/two")]);
        let read: RecentFile = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!((read.version, read.repos), (RECENT_VERSION, repos.clone()));
        // An older build's (no version) reads its list.
        std::fs::write(&file, r#"{"repos": ["/r/three", ""]}"#).unwrap();
        assert_eq!(store.load_recent(), [PathBuf::from("/r/three")]);
        // #165: a newer build's or a corrupt one reads as none, and is never written over.
        let newer = r#"{"version": 4, "repos": ["/r/three"], "pinned": ["/r/one"]}"#;
        for kept in [newer, "", "{", r#"{"repos": "/r/one"}"#, r#"{"version": 1}"#, "[1]"] {
            std::fs::write(&file, kept).unwrap();
            assert!(store.load_recent().is_empty(), "{kept:?}");
            let refused = store.save_recent(&repos).unwrap_err();
            assert!(refused.starts_with("left the recent repositories' file alone"), "{kept:?}: {refused}");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), kept, "untouched");
        }
        std::fs::remove_file(&file).unwrap();
        // At most ten.
        let many: Vec<String> = (0..30).map(|n| format!("/r/{n}")).collect();
        store.save_recent(&many).unwrap();
        assert_eq!(store.load_recent().len(), 10);
        std::fs::remove_dir_all(dir).ok();
    }

    /// #165: an older build's file is migrated, each earlier version read as this one's; a file
    /// this build can't wholly read is left alone (a newer, corrupt or foreign one refused, one
    /// with a section that doesn't read kept in part), and says so.
    #[test]
    fn older_files_migrate_and_any_other_is_left_alone() {
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
            assert_eq!((loaded.state.as_ref(), loaded.left_alone), (None, true), "{corrupt:?}");
            assert!(
                loaded.note.as_deref().is_some_and(|n| n.contains("is corrupt") && n.contains("starting afresh") && n.contains("left as it is")),
                "{corrupt:?}: {:?}",
                loaded.note
            );
        }
        let foreign = load(r#"{"version": 1, "repo": "/r/other"}"#);
        assert!(foreign.state.is_none() && foreign.left_alone && foreign.note.unwrap().contains("another repository's"));

        // An older file (no sizes, no window, no timeline) reads with their defaults, silently.
        let older = load(r#"{"version": 0, "repo": "/r/swarm", "ui": {"version": 3, "panes": {"factory": false}}, "rows": {"collapse": true}}"#);
        let state = older.state.unwrap();
        assert_eq!((state.ui.panes.factory, state.ui.panes.timeline, state.rows.collapse), (false, true, swarmception_domain::Collapse::Empty));
        assert!(!older.left_alone, "migrated: written as this version from now on");
        // Version 1's collapse was a bool (#74): it keeps the choice it was.
        let one = load(r#"{"version": 1, "repo": "/r/swarm", "rows": {"grouping": "workers", "collapse": false}}"#);
        assert_eq!((one.state.unwrap().rows.collapse, one.note, one.left_alone), (swarmception_domain::Collapse::Off, None, false));
        let two = load(r#"{"version": 2, "repo": "/r/swarm", "rows": {"collapse": "window"}}"#);
        assert_eq!(two.state.unwrap().rows.collapse, swarmception_domain::Collapse::Window);
        assert_eq!((state.ui.sizes, state.window, older.note), (PaneSizes::default(), None, None));
        // Version 8 of the shell's state kept the factory's own time (#85): it reads on without it,
        // its other settings kept, nothing said.
        let eight = load(
            r#"{"version": 2, "repo": "/r/swarm", "ui": {"version": 8, "factory": {"follow": false, "playing": false, "speed": "hour", "t": 1793000000, "ticket": 7, "steps": "merges"}}}"#,
        );
        let factory = FactoryView { follow: false, speed: crate::Speed::Hour, ticket: Some(7), steps: crate::Steps::Merges, ..FactoryView::default() };
        assert_eq!((eight.state.unwrap().ui.factory, eight.note), (factory, None));
        // Version 11 kept the gates pane (#128): its choices are dropped, the rest kept, nothing said.
        let eleven = load(
            r#"{"version": 2, "repo": "/r/swarm", "ui": {"version": 11, "panes": {"gates": false, "details": false},
                "sizes": {"gates": 160, "factory": 300}, "folded": {"gates": true, "factory": true}}}"#,
        );
        let ui = eleven.state.unwrap().ui;
        assert_eq!((ui.panes.details, ui.sizes.factory, ui.folded.factory, eleven.note), (false, Some(300), Some(true), None));

        // A newer one: refused whole, not read in part, and left alone.
        let newer = load(
            r#"{"version": 9, "repo": "/r/swarm", "ui": {"panes": {"details": false}, "factory": {"speed": "fortnight"}, "lighting": 3},
                "rows": {"grouping": "workers"}, "window": "maximised", "plugins": [1, 2]}"#,
        );
        assert_eq!((newer.state.as_ref(), newer.left_alone), (None, true));
        let note = newer.note.unwrap();
        assert!(note.contains("is a newer Swarmception's (version 9; this one reads 2)") && note.contains("starting afresh"), "{note}");
        // This version's, a section that doesn't read: the rest kept, the file left alone.
        let part = load(r#"{"version": 2, "repo": "/r/swarm", "ui": {"panes": {"details": false}, "factory": {"speed": "fortnight"}}, "window": "maximised"}"#);
        let state = part.state.unwrap();
        assert!(!state.ui.panes.details && part.left_alone);
        assert_eq!((state.ui.factory, state.window), (FactoryView::default(), None));
        let note = part.note.unwrap();
        assert!(note.contains("factory, window didn't read") && note.contains("left as it is"), "{note}");
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
        // Changing every frame: written within MOST seconds all the same.
        for (n, t) in (0..).zip([10.0, 11.0, 12.0, 13.0, 14.0, 15.0]) {
            state.ui.factory.ticket = Some(n);
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
        assert!(keeper.write_now(state.clone()).is_err());

        // #165: a file left alone is never written, as it changes or on quit.
        let file = store.file(repo);
        std::fs::write(&file, r#"{"version": 99, "repo": "/r/keeper"}"#).unwrap();
        let loaded = store.load(repo);
        let mut keeper = Keeper::new(store.clone(), repo, Some(&loaded));
        assert_eq!(keeper.inspect().keeping, Keeping::LeftAlone);
        state.ui.panes.timeline = false;
        assert!(keeper.observe(state.clone(), 0.0).is_none() && keeper.observe(state.clone(), 9.0).is_none());
        assert_eq!(keeper.write_now(state), Ok(None));
        assert_eq!((keeper.inspect().writes, std::fs::read_to_string(&file).unwrap().as_str()), (0, r#"{"version": 99, "repo": "/r/keeper"}"#));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_window_off_every_monitor_comes_onto_the_nearest() {
        // A laptop's screen, and an external one to its right (since unplugged, or not).
        let laptop = Monitor { x: 0.0, y: 0.0, width: 1440.0, height: 900.0 };
        let external = Monitor { x: 1440.0, y: -200.0, width: 2560.0, height: 1440.0 };
        let on_external = WindowPlace { x: 2000.0, y: 100.0, width: 1400.0, height: 860.0 };
        // Both there: it stays, on one monitor or across the two.
        assert_eq!(on_external.onto(&[laptop, external]), on_external);
        let across = WindowPlace { x: 1000.0, y: 50.0, width: 1000.0, height: 700.0 };
        assert_eq!(across.onto(&[laptop, external]), across, "covered by the two");
        // The external one unplugged: onto the laptop's, shrunk to fit, inside it.
        let moved = on_external.onto(&[laptop]);
        assert_eq!(moved, WindowPlace { x: 40.0, y: 12.0, width: 1400.0, height: 860.0 });
        let big = WindowPlace { x: 1500.0, y: 0.0, width: 2400.0, height: 1300.0 };
        assert_eq!(big.onto(&[laptop]), WindowPlace { x: 0.0, y: 0.0, width: 1440.0, height: 900.0 - TITLE_BAR });
        // Partly off: pulled in wholly, on the monitor it overlaps most.
        let partly = WindowPlace { x: 1200.0, y: 600.0, width: 800.0, height: 600.0 };
        assert_eq!(partly.onto(&[laptop]), WindowPlace { x: 640.0, y: 272.0, width: 800.0, height: 600.0 });
        // Off every monitor: onto the nearest.
        let far_left = WindowPlace { x: -5000.0, y: 300.0, width: 800.0, height: 600.0 };
        assert_eq!(far_left.onto(&[external, laptop]), WindowPlace { x: 0.0, y: 272.0, ..far_left });
        let far_right = WindowPlace { x: 9000.0, y: 300.0, ..far_left };
        assert_eq!(far_right.onto(&[laptop, external]), WindowPlace { x: 4000.0 - 800.0, y: 300.0, ..far_left });
        // No monitor known: as it is.
        assert_eq!(far_left.onto(&[]), far_left);
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
