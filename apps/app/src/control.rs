//! `--control`: the control channel (docs/architecture.md › Automation). The app listens on
//! loopback with a fresh token (`swarmception_automation::tcp`), prints where and the token as one
//! JSON line (and writes it to `--control-file` when given), and answers each request on the UI
//! thread between frames through [`swarmception_ui::Shell::answer`]: the engine's commands, the
//! window's `ui.*` and the widget's `timeline.<id>`. A screenshot is taken a few frames after it
//! is asked for, so what a `ui.set` moved has settled, and answered once it is saved.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use eframe::egui;
use swarmception_automation::channel::{Call, ChannelHandler, Responder};
use swarmception_automation::protocol::ProtocolError;
use swarmception_automation::tcp::{Listening, Shared, bind, serve, write_listening};
use swarmception_ui::Shell;
use swarmception_ui::control::{Answer, Saved, Screenshot, ShellError};

/// Frames drawn after a screenshot is asked for, before it is taken.
pub const SCREENSHOT_SETTLE: u32 = 8;

/// The control channel's requests, and the screenshots waiting to be taken.
pub struct Control {
    calls: Receiver<Call>,
    pending: Vec<Pending>,
}

/// A screenshot asked for: frames still to draw first, the file, and where its answer goes.
struct Pending {
    frames: u32,
    path: PathBuf,
    reply: Responder,
}

/// Listens on loopback at `port` (0 for any free one) and serves the channel on a thread of its
/// own, each request waking `ctx`; prints where and the token, and writes them to `file`.
///
/// # Errors
///
/// Why it can't listen or write the file.
pub fn start(port: u16, file: Option<&Path>, ctx: &egui::Context) -> Result<(Control, Listening), String> {
    let (listener, listening) = bind(port, None).map_err(|e| format!("can't listen on 127.0.0.1:{port}: {e}"))?;
    // The file first: a caller that read the line finds the file written.
    if let Some(file) = file {
        write_listening(file, &listening).map_err(|e| format!("can't write {}: {e}", file.display()))?;
    }
    let line = serde_json::to_string(&listening).map_err(|e| e.to_string())?;
    println!("{line}");
    let (control, handler) = channel(ctx);
    let token = listening.token.clone();
    std::thread::Builder::new()
        .name("control channel".into())
        .spawn(move || serve(&listener, &token, &Shared::new(handler)))
        .map_err(|e| format!("can't start the control channel: {e}"))?;
    Ok((control, listening))
}

/// A control whose requests come through the returned handler, each waking `ctx`.
pub fn channel(ctx: &egui::Context) -> (Control, ChannelHandler) {
    let waker = ctx.clone();
    let (handler, calls) = ChannelHandler::new(move || waker.request_repaint());
    (Control { calls, pending: Vec::new() }, handler)
}

impl Control {
    /// Answers the requests that came since the last frame, and takes the screenshots that are
    /// due: before the shell draws, so this frame draws what they did.
    pub fn answer(&mut self, shell: &mut Shell, ctx: &egui::Context) {
        while let Ok(call) = self.calls.try_recv() {
            let Call { command, params, reply } = call;
            match shell.answer(&command, &params) {
                Answer::Now(result) => reply.send(result.map_err(protocol_error)),
                Answer::Screenshot(Screenshot { path }) => self.pending.push(Pending { frames: SCREENSHOT_SETTLE, path: path.into(), reply }),
            }
        }
        if self.pending.is_empty() {
            return;
        }
        for shot in &mut self.pending {
            shot.frames = shot.frames.saturating_sub(1);
        }
        let (due, waiting): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut self.pending).into_iter().partition(|p| p.frames == 0);
        self.pending = waiting;
        for shot in due {
            ctx.request_screenshot(move |image| {
                let [width, height] = image.size;
                let saved = crate::save_ppm(&shot.path, &image)
                    .map_err(|e| ProtocolError::Ui { message: format!("can't save {}: {e}", shot.path.display()) })
                    .map(|()| Saved { path: shot.path.display().to_string(), width, height })
                    .and_then(|saved| serde_json::to_string(&saved).map_err(|e| ProtocolError::Ui { message: e.to_string() }));
                shot.reply.send(saved);
            });
        }
        ctx.request_repaint();
    }
}

/// The shell's error as the protocol carries it.
fn protocol_error(error: ShellError) -> ProtocolError {
    match error {
        ShellError::Engine(error) => ProtocolError::Engine { error },
        ShellError::Widget(error) => ProtocolError::Widget { error },
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use swarmception_automation::client::{Client, ClientError};
    use swarmception_automation::tcp::{Shared, bind, serve};
    use swarmception_engine::factory_cmds::{At, AtParams};
    use swarmception_model::Timestamp;
    use swarmception_ui::control::Inspect;

    use super::*;

    /// A repository whose tracking branch holds a worker and a coordinator's gate.
    fn repo() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("worker18-control-{}-{:?}", std::process::id(), std::thread::current().id()).replace(['(', ')'], ""));
        let _ = std::fs::remove_dir_all(&dir);
        let files = [
            (
                "mini-aa1111",
                [
                    r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
                    r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
                    r#"{"event":"work","issue":7,"t":"2026-10-31T03:02:00Z"}"#,
                    r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:30:00Z"}"#,
                    r#"{"event":"claim","issue":9,"t":"2026-10-31T03:31:00Z"}"#,
                    r#"{"event":"work","issue":9,"t":"2026-10-31T03:32:00Z"}"#,
                ]
                .join("\n"),
            ),
            (
                "mini-cc3333",
                [
                    r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
                    r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7)","sub":"gate-pr8","t":"2026-10-31T03:40:00Z"}"#,
                    r#"{"event":"merge","issue":7,"mr":8,"sub":"gate-pr8","t":"2026-10-31T03:45:00Z"}"#,
                    r#"{"event":"sub.stop","note":"rc=0; merged #8","sub":"gate-pr8","t":"2026-10-31T03:46:00Z"}"#,
                ]
                .join("\n"),
            ),
        ];
        for (instance, text) in files {
            std::fs::create_dir_all(dir.join("tracking").join(instance)).unwrap();
            std::fs::write(dir.join("tracking").join(instance).join("work.jsonl"), text + "\n").unwrap();
        }
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .current_dir(&dir)
                .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
                .args(args)
                .env("GIT_AUTHOR_DATE", "2026-10-31T02:00:00Z")
                .env("GIT_COMMITTER_DATE", "2026-10-31T02:00:00Z")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q", "-b", "tracking"]);
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "events"]);
        dir
    }

    fn frame(shell: &mut Shell, ctx: &egui::Context, control: &mut Control) {
        control.answer(shell, ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| shell.ui(ui));
        output.textures_delta.clear();
    }

    /// Runs `ask` on a client thread over a real socket while this thread draws frames and
    /// answers its requests, as the app's UI thread does: what `ask` returned.
    fn drive<T: Send + 'static>(shell: &mut Shell, ctx: &egui::Context, ask: impl FnOnce(&mut Client) -> T + Send + 'static) -> T {
        let (mut control, handler) = channel(ctx);
        let (listener, listening) = bind(0, None).unwrap();
        let token = listening.token.clone();
        std::thread::spawn(move || serve(&listener, &token, &Shared::new(handler)));
        let client = std::thread::spawn(move || ask(&mut Client::connect(listening.address, &listening.token, Duration::from_secs(20)).unwrap()));
        while !client.is_finished() {
            frame(shell, ctx, &mut control);
            std::thread::sleep(Duration::from_millis(5));
        }
        client.join().unwrap()
    }

    #[test]
    fn what_ui_inspect_reports_over_the_socket_is_what_the_pane_drew() {
        let dir = repo();
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, Some(&dir));
        let t = Timestamp::parse("2026-10-31T03:42:00Z").unwrap();
        let (set, inspect, wrong) = drive(&mut shell, &ctx, move |client| {
            // Wait for the read to land, then put the factory at 03:42 with #7 selected.
            for _ in 0..500 {
                let inspect: Inspect = client.call("ui.inspect", &()).unwrap();
                if inspect.timeline.has_data {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let set: Inspect = client.call("ui.set", &serde_json::json!({"at": "2026-10-31T03:42:00Z", "ticket": 7})).unwrap();
            std::thread::sleep(Duration::from_millis(200));
            let inspect: Inspect = client.call("ui.inspect", &()).unwrap();
            let wrong: Result<Inspect, ClientError> = client.call("ui.set", &serde_json::json!({"at": "noon"}));
            (set, inspect, wrong)
        });
        assert_eq!(set.state.factory.t, Some(t));
        assert_eq!(set.state.factory.ticket, Some(7));
        assert!(matches!(wrong, Err(ClientError::Protocol(ProtocolError::Engine { .. }))));

        // What it reported is what the pane drew, and what `factory.at` answers for that time.
        assert_eq!(inspect.factory.t, Some(t));
        let shown = shell.factory().scene().cloned().unwrap();
        assert_eq!(inspect.factory.t, Some(shown.t));
        let drawn = shell.factory().drawn().clone();
        let reported = &inspect.factory.drawn;
        assert_eq!(
            (&reported.upcoming, &reported.shelf, &reported.dock, &reported.gate, &reported.done),
            (&drawn.upcoming, &drawn.shelf, &drawn.dock, &drawn.gate, &drawn.done)
        );
        let benches: Vec<(String, Vec<u64>)> = reported.benches.iter().map(|b| (b.key.clone(), b.tickets.clone())).collect();
        assert_eq!(benches, drawn.benches);
        assert_eq!((&reported.runs, reported.blocks, &reported.journey), (&drawn.runs, drawn.blocks, &drawn.journey));
        let asked = shell.engine().call::<At>(AtParams { t: Some(t), recent: Some(shown.recent), ticket: Some(7) }).unwrap();
        assert_eq!(reported.gate, asked.gate.crates.iter().map(|c| c.issue).collect::<Vec<_>>(), "#7 is in the gate at 03:42");
        assert_eq!(reported.gate, [7]);
        assert!(!reported.journey.is_empty(), "#7's journey is traced");
        // #7's bars are selected on the timeline, and the playhead is at 03:42's item.
        assert!(inspect.timeline.selected.values().any(|ranges| !ranges.is_empty()), "{:?}", inspect.timeline.selected);
        assert!(inspect.timeline.playhead_item.is_some());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_widgets_commands_and_the_engines_go_through_the_window() {
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, None);
        let (commands, widget, unknown, shot) = drive(&mut shell, &ctx, |client| {
            let commands: swarmception_engine::engine_cmds::CommandsResult = client.call("engine.commands", &()).unwrap();
            let widget: Result<serde_json::Value, ClientError> = client.call("timeline.view.nope", &());
            let unknown: Result<serde_json::Value, ClientError> = client.call("ui.nope", &());
            let shot: Result<serde_json::Value, ClientError> = client.call("ui.screenshot", &serde_json::json!({"path": "x.png"}));
            (commands, widget, unknown, shot)
        });
        assert!(commands.commands.iter().any(|c| c.id == "factory.at"));
        assert!(matches!(widget, Err(ClientError::Protocol(ProtocolError::Widget { .. }))));
        assert!(matches!(unknown, Err(ClientError::Protocol(ProtocolError::Engine { .. }))));
        assert!(matches!(shot, Err(ClientError::Protocol(ProtocolError::Engine { .. }))));
    }
}
