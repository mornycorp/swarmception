//! Swarmception, the desktop app: a swarm's tickets on a timeline, read from its repository's
//! `tracking` branch and `main`.
//!
//! `swarmception [--repo <path>] [--grouping flat|workers] [--collapse]
//! [--dark | --light] [--size <w> <h>] [--screenshot <file.ppm>]`: `--repo` is any directory in a
//! swarm's clone (the current directory if not given); `--screenshot` saves the window once the
//! timeline has drawn, then quits, so agents can check the drawing by eye. On macOS the menus are
//! in the system menu bar ([`mac_menu`]); elsewhere the window draws its own.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

#[cfg(target_os = "macos")]
mod mac_menu;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use swarmception_domain::Grouping;
use swarmception_engine::rows_cmds::{Set as SetRows, SetParams as SetRowsParams};

const USAGE: &str = "usage: swarmception [--repo <path>] [--grouping flat|workers] [--collapse] [--dark | --light] [--size <w> <h>] [--screenshot <file.ppm>]";
/// Frames to draw after the timeline has data, before a screenshot.
const SETTLE_FRAMES: u32 = 10;

/// The command line.
#[derive(Debug, Default, PartialEq)]
struct Options {
    repo: Option<PathBuf>,
    rows: SetRowsParams,
    theme: Option<egui::Theme>,
    size: Option<(f32, f32)>,
    screenshot: Option<PathBuf>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.iter();
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
                "--collapse" => options.rows.collapse = Some(true),
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
                "--screenshot" => options.screenshot = Some(value()?.into()),
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
    let repo = options.repo.clone().or_else(|| std::env::current_dir().ok());
    let (width, height) = options.size.unwrap_or((1_400.0, 860.0));
    let viewport = egui::ViewportBuilder::default().with_title("Swarmception").with_inner_size([width, height]).with_min_inner_size([480.0, 320.0]);
    let native = eframe::NativeOptions { viewport, ..Default::default() };
    let start = move |cc: &eframe::CreationContext<'_>| -> Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
        swarmception_ui::fonts::install(&cc.egui_ctx);
        // Ctrl + +, Ctrl + − and Ctrl + 0 are the timeline's zoom keys, not egui's.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        if let Some(theme) = options.theme {
            cc.egui_ctx.set_theme(theme);
        }
        let mut shell = swarmception_ui::Shell::new(&cc.egui_ctx, repo.as_deref());
        if options.rows != SetRowsParams::default() {
            shell.engine().call::<SetRows>(options.rows).map_err(|e| e.to_string())?;
        }
        #[cfg(target_os = "macos")]
        let menu = match mac_menu::NativeMenu::install(&cc.egui_ctx, &shell.menu_state()) {
            Ok(menu) => {
                shell.set_native_menu(true);
                Some(menu)
            }
            Err(e) => {
                eprintln!("swarmception: no system menu bar ({e}); the window shows its own");
                None
            }
        };
        Ok(Box::new(App {
            shell,
            screenshot: options.screenshot.map(Screenshot::new),
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

struct App {
    shell: swarmception_ui::Shell,
    screenshot: Option<Screenshot>,
    #[cfg(target_os = "macos")]
    menu: Option<mac_menu::NativeMenu>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "macos")]
        if let Some(menu) = &mut self.menu {
            menu.run_clicks(&mut self.shell, ui.ctx());
        }
        self.shell.ui(ui);
        #[cfg(target_os = "macos")]
        if let Some(menu) = &mut self.menu {
            menu.show(&mut self.shell);
        }
        if let Some(shot) = &mut self.screenshot {
            let ready = !self.shell.loading() && self.shell.has_timeline();
            shot.step(ui.ctx(), ready);
        }
    }
}

/// `--screenshot`: once the timeline has drawn and settled, saves the window and quits.
struct Screenshot {
    path: PathBuf,
    frames: u32,
    requested: bool,
    saved: Arc<AtomicBool>,
}

impl Screenshot {
    fn new(path: PathBuf) -> Self {
        Self { path, frames: 0, requested: false, saved: Arc::default() }
    }

    fn step(&mut self, ctx: &egui::Context, ready: bool) {
        if self.saved.load(Ordering::Acquire) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ready {
            self.frames += 1;
        }
        if !self.requested && self.frames >= SETTLE_FRAMES {
            let (path, saved) = (self.path.clone(), Arc::clone(&self.saved));
            ctx.request_screenshot(move |image| {
                if let Err(e) = save_ppm(&path, &image) {
                    eprintln!("swarmception: can't save {}: {e}", path.display());
                }
                saved.store(true, Ordering::Release);
            });
            self.requested = true;
        }
        ctx.request_repaint();
    }
}

/// `image` as a binary PPM.
fn save_ppm(path: &std::path::Path, image: &egui::ColorImage) -> std::io::Result<()> {
    use std::io::Write;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(out, "P6\n{} {}\n255\n", image.size[0], image.size[1])?;
    for pixel in &image.pixels {
        out.write_all(&[pixel.r(), pixel.g(), pixel.b()])?;
    }
    out.flush()
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
        assert_eq!(options.rows, SetRowsParams { grouping: Some(Grouping::Workers), collapse: Some(true) });
        assert_eq!(options.theme, Some(egui::Theme::Dark));
        assert!(parse(&["--repo"]).is_err());
        assert!(parse(&["--grouping", "tree"]).is_err());
        assert!(parse(&["--size", "10", "10"]).is_err());
        assert!(parse(&["--frobnicate"]).is_err());
        assert_eq!(parse(&["--help"]), Err(String::new()));
    }
}
