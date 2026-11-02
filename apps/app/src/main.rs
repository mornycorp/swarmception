//! Swarmception, the desktop app: a swarm's tickets on a timeline, read from its repository's
//! `tracking` branch and `main`.
//!
//! `swarmception [--repo <path>] [--grouping flat|workers] [--collapse [off|empty|window]] [--panes <list>]
//! [--at <time>] [--dark | --light] [--size <w> <h>] [--screenshot <file.png|file.ppm>]`: `--repo` is any
//! directory in a swarm's clone (the current directory if not given); `--panes` the panes shown
//! (`timeline`, `factory`, `details`, `gates`, `workers`, comma-separated); `--details <what>` shows the details of
//! a ticket (`47`, `#47`), a gate run (`gate:<sub>[@<time>]`), a session
//! (`session:<instance>[@<time>]`) or a merge onto main (`merge:<commit>`), its pane open (#47);
//! `--at` puts the factory at a UTC time
//! (`2026-10-31T13:20:00Z`) and the timeline's playhead there; `--trace <issue>` traces a
//! ticket's journey through the factory; `--hover <issue>` moves the pointer onto the ticket's
//! chip once the factory has drawn it, `--hover-bar <issue>` onto its first bar on the timeline,
//! `--hover-gate <sub>` onto a gate run's bar in the Gates pane (#61), `--hover-worker <name>`
//! onto a worker's row in the Workers pane (#67: its instance, or the first row of that name);
//! `--menu file|file.recent` draws the window's own menu bar and clicks File open (and puts the
//! pointer on Open Recent), for a screenshot of it (#69); File ▸ Open… (⌘O) is AppKit's open
//! panel on macOS ([`folder`]), elsewhere the desktop's own picker (zenity, kdialog, PowerShell's
//! folder browser) as a child process with a deadline, else a path typed into the window
//! ([`picker`], #76);
//! `--step next|previous|first|last` steps the factory to a key moment from there (#33), of the
//! kinds `--steps all|claims|work|submits|gates|sendBacks|merges` names; bars always carry their tickets' titles (#34);
//! `--zoom <scale>` scales the UI (0.25 to 4: at 0.5 a 1280 × 720 window lays out as 2560 × 1440,
//! for a screenshot of a larger window than the screen holds); `--screenshot`
//! saves the window once the timeline has drawn (and the GitHub issues have come), then quits, so
//! agents can check the drawing by eye: PNG when the path ends in `.png`, else a binary PPM. `--control [<port>]` serves the control channel on
//! loopback (any free port when none is given; [`control`]): it prints where and its token as one
//! JSON line, and `--control-file <path>` writes that line to a file too. On macOS the menus are in the system menu bar
//! ([`mac_menu`]); elsewhere the window draws its own.
//!
//! The state survives a restart (#53, `swarmception_ui::saved`): the panes and their sizes, the
//! rows, the factory's settings, what the details are of, Live, the timeline's view and the window's
//! size and place, per repository, in `--state-dir <dir>`, else `$SWARMCEPTION_STATE_DIR`, else
//! the platform's app-data directory (`~/Library/Application Support/swarmception` on macOS);
//! read at the start, written when it changes and on quit. The command line goes over it;
//! `--fresh` starts without it. A run with `--screenshot` or `--control` keeps it only in a
//! directory named, so an agent's run never touches the owner's.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

mod control;
#[cfg(target_os = "macos")]
mod folder;
#[cfg(target_os = "macos")]
mod mac_menu;
// Built and tested everywhere; run off macOS only.
#[cfg_attr(target_os = "macos", allow(dead_code))]
mod picker;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use swarmception_domain::details::Subject;
use swarmception_domain::moments::Direction;
use swarmception_domain::{Collapse, Grouping};
use swarmception_engine::rows_cmds::{Set as SetRows, SetParams as SetRowsParams};
use swarmception_model::Timestamp;
use swarmception_ui::saved::{self, Loaded, StateStore};

/// `--steps`' kinds of key moment: `all`, `claims`, …
fn steps(text: &str) -> Result<swarmception_ui::Steps, String> {
    let ids = swarmception_ui::Steps::ALL.map(swarmception_ui::Steps::id);
    let steps = swarmception_ui::Steps::ALL.into_iter().find(|s| s.id() == text);
    steps.ok_or_else(|| format!("--steps takes one of {}, not {text}", ids.join(", ")))
}

/// `--panes`' list: `timeline`, `factory`, `details`, `gates`, `workers`.
fn panes(list: &str) -> Result<swarmception_ui::Panes, String> {
    let mut panes = swarmception_ui::Panes { timeline: false, factory: false, details: false, gates: false, workers: false };
    for name in list.split(',').map(str::trim) {
        match name {
            "timeline" => panes.timeline = true,
            "factory" => panes.factory = true,
            "details" => panes.details = true,
            "gates" => panes.gates = true,
            "workers" => panes.workers = true,
            other => return Err(format!("--panes takes timeline, factory, details, gates and workers, not {other}")),
        }
    }
    Ok(panes)
}

/// `--details`' subject: `47` or `#47` (or `ticket:47`), `gate:<sub>[@<time>]`,
/// `session:<instance>[@<time>]`, `merge:<commit>`.
fn subject(text: &str) -> Result<Subject, String> {
    let at = |rest: &str| -> Result<(String, Option<Timestamp>), String> {
        match rest.split_once('@') {
            Some((name, t)) => {
                Ok((name.to_owned(), Some(Timestamp::parse(t).ok_or_else(|| format!("--details' time is UTC, like 2026-10-31T13:20:00Z, not {t}"))?)))
            }
            None => Ok((rest.to_owned(), None)),
        }
    };
    let (kind, rest) = text.split_once(':').unwrap_or(("ticket", text));
    let subject = match kind {
        "ticket" => Subject::Ticket { issue: issue(rest)? },
        "gate" => at(rest).map(|(sub, at)| Subject::Gate { sub, at })?,
        "session" => at(rest).map(|(instance, at)| Subject::Session { instance, at })?,
        "merge" => Subject::Merge { commit: rest.to_owned() },
        other => return Err(format!("--details takes a ticket, gate:, session: or merge:, not {other}:")),
    };
    match &subject {
        Subject::Gate { sub: name, .. } | Subject::Session { instance: name, .. } | Subject::Merge { commit: name } if name.is_empty() => {
            Err(format!("--details {kind}: needs a name"))
        }
        _ => Ok(subject),
    }
}

/// `--step`'s direction: `next`, `previous`, `first` or `last`.
fn direction(text: &str) -> Result<Direction, String> {
    match text {
        "next" => Ok(Direction::Next),
        "previous" => Ok(Direction::Previous),
        "first" => Ok(Direction::First),
        "last" => Ok(Direction::Last),
        other => Err(format!("--step takes next, previous, first or last, not {other}")),
    }
}

/// A collapse's name (#74): `off`, `empty` or `window`.
fn collapse(text: &str) -> Option<Collapse> {
    match text {
        "off" => Some(Collapse::Off),
        "empty" => Some(Collapse::Empty),
        "window" => Some(Collapse::Window),
        _ => None,
    }
}

/// An issue number: `18` or `#18`.
fn issue(text: &str) -> Result<u64, String> {
    text.trim_start_matches('#').parse().map_err(|_| format!("an issue is a number, not {text}"))
}

const USAGE: &str = "usage: swarmception [--repo <path>] [--grouping flat|workers] [--collapse [off|empty|window]] [--panes timeline,factory,details,gates,workers] [--details <ticket|gate:<sub>|session:<instance>|merge:<commit>>] [--at <YYYY-MM-DDTHH:MM:SSZ>] [--trace <issue>] [--hover <issue>] [--hover-bar <issue>] [--hover-gate <sub>] [--hover-worker <name>] [--menu file|file.recent] [--step next|previous|first|last] [--steps all|claims|work|submits|gates|sendBacks|merges] [--dark | --light] [--size <w> <h>] [--zoom <scale>] [--screenshot <file.png|file.ppm>] [--control [<port>]] [--control-file <path>] [--state-dir <dir>] [--fresh]";
/// Frames to draw after the timeline has data, before a screenshot.
const SETTLE_FRAMES: u32 = 10;

/// The command line.
#[derive(Debug, Default, PartialEq)]
struct Options {
    repo: Option<PathBuf>,
    rows: SetRowsParams,
    panes: Option<swarmception_ui::Panes>,
    details: Option<Subject>,
    at: Option<Timestamp>,
    trace: Option<u64>,
    hover: Option<u64>,
    hover_bar: Option<u64>,
    hover_gate: Option<String>,
    hover_worker: Option<String>,
    /// `--menu`: the window draws its own menu bar and opens File (and Open Recent), for a
    /// screenshot of it (#69).
    menu: Option<MenuOpen>,
    step: Option<Direction>,
    steps: Option<swarmception_ui::Steps>,
    theme: Option<egui::Theme>,
    size: Option<(f32, f32)>,
    /// `--zoom`: the UI's scale (egui's zoom factor), 0.25 to 4.
    zoom: Option<f32>,
    screenshot: Option<PathBuf>,
    /// `--control`: the port to serve the control channel on, 0 for any free one.
    control: Option<u16>,
    control_file: Option<PathBuf>,
    /// `--state-dir`: where the state is kept, over `SWARMCEPTION_STATE_DIR` and the platform's.
    state_dir: Option<PathBuf>,
    /// `--fresh`: start without the saved state, saving afresh.
    fresh: bool,
}

impl Options {
    /// Frames to draw once ready before a screenshot: a hover waits a little longer, for its
    /// tooltip.
    fn settle(&self) -> u32 {
        if self.hover.or(self.hover_bar).is_some() || self.hover_gate.is_some() || self.hover_worker.is_some() || self.menu.is_some() {
            45
        } else {
            SETTLE_FRAMES
        }
    }

    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.iter().peekable();
        while let Some(arg) = args.next() {
            let mut value = || args.next().cloned().ok_or_else(|| format!("{arg} needs a value"));
            match arg.as_str() {
                "--repo" => options.repo = Some(value()?.into()),
                "--grouping" => {
                    options.rows.grouping = Some(match value()?.as_str() {
                        "flat" => Grouping::Flat,
                        "workers" => Grouping::Workers,
                        other => return Err(format!("--grouping takes flat or workers, not {other}")),
                    });
                }
                // Alone, the empty rows left out; a mode may follow (#74).
                "--collapse" => options.rows.collapse = args.next_if(|next| collapse(next).is_some()).and_then(|m| collapse(m)).or(Some(Collapse::Empty)),
                "--panes" => options.panes = Some(panes(&value()?)?),
                "--details" => options.details = Some(subject(&value()?)?),
                "--at" => {
                    let text = value()?;
                    options.at = Some(Timestamp::parse(&text).ok_or_else(|| format!("--at takes a UTC time like 2026-10-31T13:20:00Z, not {text}"))?);
                }
                "--trace" => options.trace = Some(issue(&value()?)?),
                "--hover" => options.hover = Some(issue(&value()?)?),
                "--hover-bar" => options.hover_bar = Some(issue(&value()?)?),
                "--hover-gate" => options.hover_gate = Some(value()?),
                "--hover-worker" => options.hover_worker = Some(value()?),
                "--menu" => options.menu = Some(MenuOpen::parse(&value()?)?),
                "--step" => options.step = Some(direction(&value()?)?),
                "--steps" => options.steps = Some(steps(&value()?)?),
                "--dark" => options.theme = Some(egui::Theme::Dark),
                "--light" => options.theme = Some(egui::Theme::Light),
                "--size" => {
                    let number = |v: String| {
                        v.parse::<f32>().ok().filter(|n| n.is_finite() && *n >= 200.0).ok_or_else(|| format!("--size takes two widths of 200 or more, not {v}"))
                    };
                    let w = number(value()?)?;
                    let h = number(value()?)?;
                    options.size = Some((w, h));
                }
                "--zoom" => {
                    let text = value()?;
                    let zoom = text.parse::<f32>().ok().filter(|z| (0.25..=4.0).contains(z));
                    options.zoom = Some(zoom.ok_or_else(|| format!("--zoom takes a scale from 0.25 to 4, not {text}"))?);
                }
                "--screenshot" => options.screenshot = Some(value()?.into()),
                "--control" => {
                    // The port is optional: the next argument when it is a number.
                    let port = args.next_if(|next| !next.starts_with("--"));
                    let port = port.map(|text| text.parse::<u16>().map_err(|_| format!("--control takes a port (0 to 65535), not {text}"))).transpose()?;
                    options.control = Some(port.unwrap_or(0));
                }
                "--control-file" => options.control_file = Some(value()?.into()),
                "--state-dir" => options.state_dir = Some(value()?.into()),
                "--fresh" => options.fresh = true,
                "-h" | "--help" => return Err(String::new()),
                other => return Err(format!("unknown argument `{other}`")),
            }
        }
        Ok(options)
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match Options::parse(&args) {
        Ok(options) => options,
        Err(message) if message.is_empty() => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("swarmception: {message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    // Before the window and the menu bar: macOS would add its tab items to View (#9).
    #[cfg(target_os = "macos")]
    mac_menu::no_window_tabs();
    let repo = options.repo.clone().or_else(|| std::env::current_dir().ok());
    let keep = keeping(&options, repo.as_deref());
    let (viewport, place) = viewport(&options, &keep);
    let native = eframe::NativeOptions { viewport, ..Default::default() };
    let start = move |cc: &eframe::CreationContext<'_>| -> Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(place) = place {
            onto_monitors(cc, place);
        }
        style(&cc.egui_ctx, &options);
        let mut shell = swarmception_ui::Shell::new(&cc.egui_ctx, repo.as_deref());
        // The saved state first, so the command line's choices go over it.
        keep.give(&mut shell);
        set_up(&mut shell, &options)?;
        let control = options.control.map(|port| control::start(port, options.control_file.as_deref(), &cc.egui_ctx)).transpose()?.map(|c| c.0);
        #[cfg(target_os = "macos")]
        let menu = mac_menus(&cc.egui_ctx, &mut shell, options.menu);
        // Elsewhere the desktop's own folder picker, as a child process (#76).
        #[cfg(not(target_os = "macos"))]
        shell.set_folder_picker(Box::new(picker::pick));
        Ok(Box::new(App {
            menu_open: options.menu.map(MenuDrive::new),
            shell,
            screenshot: options.screenshot.clone().map(|path| Screenshot::new(path, options.settle())),
            hover: options.hover,
            hover_bar: options.hover_bar,
            hover_gate: options.hover_gate,
            hover_worker: options.hover_worker,
            pointer: None,
            control,
            #[cfg(target_os = "macos")]
            menu,
        }))
    };
    match eframe::run_native("Swarmception", native, Box::new(start)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("swarmception: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The window: its size and place as it was left (unless `--size` says otherwise), and that
/// place; the saved state's note said on stderr.
fn viewport(options: &Options, keep: &Keep) -> (egui::ViewportBuilder, Option<saved::WindowPlace>) {
    if let Keep::Kept(_, _, loaded) = keep
        && let Some(note) = &loaded.note
    {
        eprintln!("swarmception: {note} ({})", loaded.file.display());
    }
    // The window as it was left, unless `--size` says otherwise.
    let place = match keep {
        Keep::Kept(_, _, loaded) if options.size.is_none() => loaded.state.as_ref().and_then(|s| s.window).and_then(|w| w.sane(MIN_SIZE)),
        _ => None,
    };
    let (width, height) = options.size.or(place.map(|p| (p.width, p.height))).unwrap_or((1_400.0, 860.0));
    let mut viewport =
        egui::ViewportBuilder::default().with_title("Swarmception").with_inner_size([width, height]).with_min_inner_size([MIN_SIZE.0, MIN_SIZE.1]);
    if let Some(place) = place {
        viewport = viewport.with_position([place.x, place.y]);
    }
    (viewport, place)
}

/// The fonts, the theme and the UI's scale as the command line asks.
fn style(ctx: &egui::Context, options: &Options) {
    swarmception_ui::fonts::install(ctx);
    // Ctrl + +, Ctrl + − and Ctrl + 0 are the timeline's zoom keys, not egui's.
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    if let Some(theme) = options.theme {
        ctx.set_theme(theme);
    }
    if let Some(zoom) = options.zoom {
        ctx.set_zoom_factor(zoom);
    }
}

/// A window restored at `place` (#68, G17), moved onto the monitors winit tells of when it is
/// partly or wholly off them (a monitor since unplugged): onto the one it overlaps most, else the
/// nearest, shrunk to fit (`saved::WindowPlace::onto`); the primary one when winit lists none.
fn onto_monitors(cc: &eframe::CreationContext<'_>, place: saved::WindowPlace) {
    let Some(window) = cc.winit_window() else { return };
    // winit's monitors in points: their pixels over their scale factors.
    let mut monitors: Vec<saved::Monitor> = window
        .available_monitors()
        .map(|m| {
            let (position, size) = (m.position().to_logical::<f32>(m.scale_factor()), m.size().to_logical::<f32>(m.scale_factor()));
            saved::Monitor { x: position.x, y: position.y, width: size.width, height: size.height }
        })
        .collect();
    if monitors.is_empty() {
        monitors.extend(window.primary_monitor().map(|m| {
            let (position, size) = (m.position().to_logical::<f32>(m.scale_factor()), m.size().to_logical::<f32>(m.scale_factor()));
            saved::Monitor { x: position.x, y: position.y, width: size.width, height: size.height }
        }));
    }
    let moved = place.onto(&monitors);
    if moved != place {
        eprintln!("swarmception: the window was left off the screen: moved onto a monitor");
        cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(moved.width, moved.height)));
        cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(moved.x, moved.y)));
    }
}

/// The window's smallest inner size.
const MIN_SIZE: (f32, f32) = (480.0, 320.0);

/// How a run keeps its state across a restart (#53).
#[derive(Debug, PartialEq)]
enum Keep {
    /// Neither read nor written.
    Off,
    /// Started without it, written as it goes: `--fresh`.
    Fresh(StateStore, PathBuf),
    /// Read at the start (what was found), written as it goes.
    Kept(StateStore, PathBuf, Box<Loaded>),
}

impl Keep {
    /// Has `shell` keep the state as this says: restored from what was read, then written.
    fn give(self, shell: &mut swarmception_ui::Shell) {
        match self {
            Self::Off => {}
            Self::Fresh(store, key) => shell.keep_state(store, &key, None),
            Self::Kept(store, key, loaded) => shell.keep_state(store, &key, Some(&*loaded)),
        }
    }
}

/// How this run keeps the state of `repo`: in `--state-dir`, else `SWARMCEPTION_STATE_DIR`'s
/// directory, else the platform's app-data directory. An agent's run (`--screenshot`,
/// `--control`) keeps it only in a directory named, so it never reads or writes the owner's.
fn keeping(options: &Options, repo: Option<&Path>) -> Keep {
    let named = options.state_dir.clone().or_else(saved::dir_from_env);
    let automated = options.screenshot.is_some() || options.control.is_some();
    if automated && named.is_none() {
        return Keep::Off;
    }
    let (Some(dir), Some(repo)) = (named.or_else(saved::platform_dir), repo) else { return Keep::Off };
    let (store, key) = (StateStore::new(dir), saved::repo_key(repo));
    if options.fresh {
        return Keep::Fresh(store, key);
    }
    let loaded = store.load(&key);
    Keep::Kept(store, key, Box::new(loaded))
}

/// The shell as the command line asks: the rows, the panes, the factory's time and its traced
/// ticket.
fn set_up(shell: &mut swarmception_ui::Shell, options: &Options) -> Result<(), String> {
    if options.rows != SetRowsParams::default() {
        shell.engine().call::<SetRows>(options.rows).map_err(|e| e.to_string())?;
    }
    if let Some(panes) = options.panes {
        shell.set_ui_state(swarmception_ui::UiState { panes, ..shell.ui_state() });
    }
    if let Some(at) = options.at {
        shell.set_factory_at(at);
    }
    if let Some(subject) = &options.details {
        let state = shell.ui_state();
        shell.set_ui_state(swarmception_ui::UiState { panes: swarmception_ui::Panes { details: true, ..state.panes }, ..state });
        shell.select(subject.clone());
    }
    if let Some(ticket) = options.trace {
        let state = shell.ui_state();
        shell.set_ui_state(swarmception_ui::UiState { factory: swarmception_ui::FactoryView { ticket: Some(ticket), ..state.factory }, ..state });
    }
    if let Some(steps) = options.steps {
        let state = shell.ui_state();
        shell.set_ui_state(swarmception_ui::UiState { factory: swarmception_ui::FactoryView { steps, ..state.factory }, ..state });
    }
    if let Some(direction) = options.step {
        shell.jump_factory(swarmception_ui::Jump::Step(direction));
    }
    Ok(())
}

struct App {
    shell: swarmception_ui::Shell,
    screenshot: Option<Screenshot>,
    /// `--menu`: File clicked open, then the pointer on Open Recent.
    menu_open: Option<MenuDrive>,
    /// `--hover`: the ticket the pointer is put on.
    hover: Option<u64>,
    /// `--hover-bar`: the ticket whose bar on the timeline the pointer is put on.
    hover_bar: Option<u64>,
    /// `--hover-gate`: the gate run whose bar in the Gates pane the pointer is put on (#61).
    hover_gate: Option<String>,
    /// `--hover-worker`: the worker whose row in the Workers pane the pointer is put on (#67).
    hover_worker: Option<String>,
    /// Where `--hover`, `--hover-bar`, `--hover-gate` or `--hover-worker` put the pointer last.
    pointer: Option<egui::Pos2>,
    /// `--control`: the control channel's requests.
    control: Option<control::Control>,
    #[cfg(target_os = "macos")]
    menu: Option<mac_menu::NativeMenu>,
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let bar = self.hover_bar.and_then(|issue| self.shell.bar_point(issue));
        let chip = self.hover.and_then(|issue| self.shell.factory().drawn().chips.iter().find(|(i, _)| *i == issue).map(|(_, rect)| rect.center()));
        let gate = self.hover_gate.as_ref().and_then(|sub| self.shell.gates().drawn().runs.iter().find(|r| &r.sub == sub).map(|r| r.rect.center()));
        let rows = &self.shell.workers().drawn().rows;
        let worker = self.hover_worker.as_ref().and_then(|name| {
            let row = rows
                .iter()
                .find(|r| &r.key == name)
                .or_else(|| rows.iter().find(|r| r.kind == swarmception_ui::WorkerRowKind::Worker && r.cells.first() == Some(name)));
            // On its name, clear of the figures' own tooltips.
            row.map(|r| egui::pos2(r.rect.left() + 60.0, r.rect.center().y))
        });
        // Moved only when the point moves: each move restarts egui's tooltip delay.
        if let Some(point) = chip.or(bar).or(gate).or(worker).filter(|p| self.pointer.is_none_or(|last| last.distance(*p) > 1.0)) {
            raw_input.events.push(egui::Event::PointerMoved(point));
            self.pointer = Some(point);
        }
        if let Some(drive) = &mut self.menu_open {
            drive.step(self.shell.menu_drawn(), raw_input);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "macos")]
        if let Some(menu) = &mut self.menu {
            menu.run_clicks(&mut self.shell, ui.ctx());
        }
        if let Some(control) = &mut self.control {
            control.answer(&mut self.shell, ui.ctx());
        }
        self.shell.ui(ui);
        #[cfg(target_os = "macos")]
        if let Some(menu) = &mut self.menu {
            menu.show(&mut self.shell);
        }
        if let Some(shot) = &mut self.screenshot {
            let ready = !self.shell.loading() && !self.shell.fetching_issues() && self.shell.has_timeline();
            shot.step(ui.ctx(), ready);
        }
    }

    /// On quit (File ▸ Quit, the window closed, ⌘Q), the state once more (#53).
    fn on_exit(&mut self) {
        if let Err(e) = self.shell.save_state() {
            eprintln!("swarmception: {e}");
        }
    }
}

/// macOS: File ▸ Open…'s folder from AppKit's open panel (#69), and the menus in the system menu
/// bar, unless `--menu` asks for the window's own (to open it in a screenshot).
#[cfg(target_os = "macos")]
fn mac_menus(ctx: &egui::Context, shell: &mut swarmception_ui::Shell, open: Option<MenuOpen>) -> Option<mac_menu::NativeMenu> {
    shell.set_folder_picker(Box::new(folder::pick));
    if open.is_some() {
        return None;
    }
    match mac_menu::NativeMenu::install(ctx, &shell.menu_state()) {
        Ok(menu) => {
            shell.set_native_menu(true);
            Some(menu)
        }
        Err(e) => {
            eprintln!("swarmception: no system menu bar ({e}); the window shows its own");
            None
        }
    }
}

/// `--menu`'s menu: File alone, or File with Open Recent open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuOpen {
    File,
    Recent,
}

impl MenuOpen {
    fn parse(text: &str) -> Result<Self, String> {
        match text {
            "file" => Ok(Self::File),
            "file.recent" => Ok(Self::Recent),
            other => Err(format!("--menu takes file or file.recent, not {other}")),
        }
    }
}

/// Opens the window's own File menu with the pointer, as a person would (#69): a click on File
/// once it is drawn, then, for [`MenuOpen::Recent`], the pointer onto Open Recent, which opens
/// its submenu on hover.
#[derive(Debug)]
struct MenuDrive {
    open: MenuOpen,
    /// 0: File not clicked yet; 1: pressed; 2: released; 3: on Open Recent.
    stage: u8,
}

impl MenuDrive {
    const fn new(open: MenuOpen) -> Self {
        Self { open, stage: 0 }
    }

    fn step(&mut self, drawn: swarmception_ui::MenuDrawn, raw_input: &mut egui::RawInput) {
        let button = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        match (self.stage, drawn.file, drawn.recent) {
            (0, Some(file), _) => {
                raw_input.events.extend([egui::Event::PointerMoved(file.center()), button(file.center(), true)]);
                self.stage = 1;
            }
            (1, Some(file), _) => {
                raw_input.events.push(button(file.center(), false));
                self.stage = 2;
            }
            (2, _, Some(recent)) if self.open == MenuOpen::Recent => {
                raw_input.events.push(egui::Event::PointerMoved(recent.center()));
                self.stage = 3;
            }
            _ => {}
        }
    }
}

/// `--screenshot`: once the timeline has drawn and settled, saves the window and quits.
struct Screenshot {
    path: PathBuf,
    frames: u32,
    /// Frames to draw once ready, before saving.
    settle: u32,
    requested: bool,
    saved: Arc<AtomicBool>,
}

impl Screenshot {
    fn new(path: PathBuf, settle: u32) -> Self {
        Self { path, frames: 0, settle, requested: false, saved: Arc::default() }
    }

    fn step(&mut self, ctx: &egui::Context, ready: bool) {
        if self.saved.load(Ordering::Acquire) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ready {
            self.frames += 1;
        }
        if !self.requested && self.frames >= self.settle {
            let (path, saved) = (self.path.clone(), Arc::clone(&self.saved));
            ctx.request_screenshot(move |image| {
                if let Err(e) = swarmception_ui::shot::save(&path, &image) {
                    eprintln!("swarmception: can't save {}: {e}", path.display());
                }
                saved.store(true, Ordering::Release);
            });
            self.requested = true;
        }
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(&args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn options_parse() {
        let options = parse(&["--repo", "../swarm-timeline", "--grouping", "workers", "--collapse", "--dark"]).unwrap();
        assert_eq!(options.repo, Some(PathBuf::from("../swarm-timeline")));
        assert_eq!(options.rows, SetRowsParams { grouping: Some(Grouping::Workers), collapse: Some(Collapse::Empty) });
        assert_eq!(options.theme, Some(egui::Theme::Dark));
        // A mode may follow --collapse (#74).
        assert_eq!(parse(&["--collapse", "window", "--dark"]).unwrap().rows.collapse, Some(Collapse::Window));
        assert_eq!(parse(&["--collapse", "off"]).unwrap().rows.collapse, Some(Collapse::Off));
        assert_eq!(parse(&["--collapse"]).unwrap().rows.collapse, Some(Collapse::Empty));
        assert!(parse(&["--repo"]).is_err());
        assert!(parse(&["--grouping", "tree"]).is_err());
        assert!(parse(&["--size", "10", "10"]).is_err());
        assert_eq!(parse(&["--zoom", "0.5"]).unwrap().zoom, Some(0.5));
        assert!(parse(&["--zoom", "0"]).is_err() && parse(&["--zoom", "big"]).is_err());
        assert!(parse(&["--frobnicate"]).is_err());
        let options = parse(&["--panes", "factory", "--at", "2026-10-31T13:20:00Z"]).unwrap();
        assert_eq!(options.panes, Some(swarmception_ui::Panes { timeline: false, factory: true, details: false, gates: false, workers: false }));
        assert_eq!(options.at.map(|t| t.to_string()).as_deref(), Some("2026-10-31T13:20:00Z"));
        assert!(parse(&["--panes", "timeline,gantt"]).is_err());
        assert!(parse(&["--at", "teatime"]).is_err());
        let options = parse(&["--trace", "#58", "--hover", "120"]).unwrap();
        assert_eq!((options.trace, options.hover), (Some(58), Some(120)));
        assert!(parse(&["--hover", "x"]).is_err());
        let options = parse(&["--hover-bar", "#17"]).unwrap();
        assert_eq!(options.hover_bar, Some(17));
        let options = parse(&["--hover-gate", "gate-12b", "--panes", "timeline,gates"]).unwrap();
        assert_eq!(options.hover_gate.as_deref(), Some("gate-12b"));
        assert_eq!(options.panes, Some(swarmception_ui::Panes { timeline: true, factory: false, details: false, gates: true, workers: false }));
        let options = parse(&["--hover-worker", "alder", "--panes", "timeline,workers"]).unwrap();
        assert_eq!(options.hover_worker.as_deref(), Some("alder"));
        assert_eq!(options.panes, Some(swarmception_ui::Panes { timeline: true, factory: false, details: false, gates: false, workers: true }));
        assert!(parse(&["--no-titles"]).is_err(), "titles are always on (#34)");
        assert_eq!(parse(&["--menu", "file.recent"]).unwrap().menu, Some(MenuOpen::Recent));
        assert_eq!(parse(&["--menu", "file"]).unwrap().menu, Some(MenuOpen::File));
        assert!(parse(&["--menu", "view"]).is_err() && parse(&["--menu"]).is_err());
        let options = parse(&["--step", "previous", "--steps", "sendBacks"]).unwrap();
        assert_eq!((options.step, options.steps), (Some(Direction::Previous), Some(swarmception_ui::Steps::SendBacks)));
        assert!(parse(&["--step", "sideways"]).is_err());
        assert!(parse(&["--steps", "lunches"]).is_err());
        assert_eq!(parse(&["--control"]).unwrap().control, Some(0));
        let options = parse(&["--control", "7070", "--control-file", "c.json", "--dark"]).unwrap();
        assert_eq!((options.control, options.control_file, options.theme), (Some(7070), Some(PathBuf::from("c.json")), Some(egui::Theme::Dark)));
        assert_eq!(parse(&["--control", "--dark"]).unwrap().control, Some(0));
        assert!(parse(&["--control", "port"]).is_err());
        assert!(parse(&["--control", "70000"]).is_err());

        let details = |text: &str| parse(&["--details", text]).map(|o| o.details);
        assert_eq!(details("#47"), Ok(Some(Subject::Ticket { issue: 47 })));
        assert_eq!(details("ticket:47"), Ok(Some(Subject::Ticket { issue: 47 })));
        assert_eq!(details("gate:gate-pr50"), Ok(Some(Subject::Gate { sub: "gate-pr50".into(), at: None })));
        assert_eq!(
            details("session:mini-aa1111@2026-10-31T13:20:00Z"),
            Ok(Some(Subject::Session { instance: "mini-aa1111".into(), at: Timestamp::parse("2026-10-31T13:20:00Z") }))
        );
        assert_eq!(details("merge:be043bc0"), Ok(Some(Subject::Merge { commit: "be043bc0".into() })));
        for bad in ["gate:", "gate:x@teatime", "nope:1", "seven"] {
            assert!(details(bad).is_err(), "{bad}");
        }
        assert_eq!(
            parse(&["--panes", "details"]).unwrap().panes,
            Some(swarmception_ui::Panes { timeline: false, factory: false, details: true, gates: false, workers: false })
        );

        assert_eq!(parse(&["--help"]), Err(String::new()));
        let options = parse(&["--state-dir", "/tmp/s", "--fresh"]).unwrap();
        assert_eq!((options.state_dir, options.fresh), (Some(PathBuf::from("/tmp/s")), true));
        assert!(parse(&["--state-dir"]).is_err());
    }

    #[test]
    fn an_agents_run_keeps_the_state_only_in_a_directory_it_names() {
        let dir = std::env::temp_dir().join(format!("worker21-keeping-{}", std::process::id()));
        let repo = std::env::temp_dir();
        let key = saved::repo_key(&repo);
        let store = StateStore::new(dir.clone());
        let named = |args: &[&str]| {
            let mut options = parse(args).unwrap();
            options.state_dir = Some(dir.clone());
            keeping(&options, Some(&repo))
        };
        assert_eq!(named(&[]), Keep::Kept(store.clone(), key.clone(), Box::new(store.load(&key))));
        assert_eq!(named(&["--fresh"]), Keep::Fresh(store.clone(), key.clone()));
        assert_eq!(named(&["--screenshot", "a.ppm"]), Keep::Kept(store.clone(), key.clone(), Box::new(store.load(&key))), "named: kept");
        assert_eq!(named(&["--control"]), Keep::Kept(store.clone(), key.clone(), Box::new(store.load(&key))));
        assert_eq!(keeping(&parse(&[]).unwrap(), None), Keep::Off, "no repository, no state");
        // Not named (and no SWARMCEPTION_STATE_DIR): an agent's run leaves the owner's alone.
        if saved::dir_from_env().is_none() {
            assert_eq!(keeping(&parse(&["--screenshot", "a.ppm"]).unwrap(), Some(&repo)), Keep::Off);
            assert_eq!(keeping(&parse(&["--control", "--fresh"]).unwrap(), Some(&repo)), Keep::Off);
        }
    }
}
