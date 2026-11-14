//! `--control`: the control channel (docs/architecture.md › Automation). The app listens on
//! loopback with a fresh token (`swarmception_automation::tcp`), prints where and the token as one
//! JSON line (and writes it to `--control-file` when given), and answers each request on the UI
//! thread between frames through [`swarmception_ui::Shell::answer`]: the engine's commands, the
//! window's `ui.*` and the widget's `timeline.<id>`. A screenshot is taken a few frames after it
//! is asked for, so what a `ui.set` moved has settled, and answered once it is saved (PNG or
//! PPM, [`swarmception_ui::shot`]); `ui.loaded` is answered once the window has loaded.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Instant;

use eframe::egui;
use swarmception_automation::channel::{Call, ChannelHandler, Responder};
use swarmception_automation::protocol::ProtocolError;
use swarmception_automation::tcp::{Listening, Shared, bind, serve, write_listening};
use swarmception_ui::Shell;
use swarmception_ui::control::{Answer, Saved, Screenshot, ShellError};

/// Frames drawn after a screenshot is asked for, before it is taken.
pub const SCREENSHOT_SETTLE: u32 = 8;

/// The control channel's requests, and the answers waiting on a later frame.
pub struct Control {
    calls: Receiver<Call>,
    pending: Vec<Pending>,
}

/// What a request waits for before its answer, and where the answer goes.
struct Pending {
    wait: Wait,
    reply: Responder,
}

enum Wait {
    /// A screenshot: frames still to draw first, and the file.
    Shot { frames: u32, path: PathBuf },
    /// `ui.loaded`: the window loaded, or this time passed.
    Loaded { until: Instant },
    /// `repo.open` (#161): its folder's check landed, or this time passed.
    Opening { until: Instant },
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
    /// Answers the requests that came since the last frame, and those waiting that are due:
    /// before the shell draws, so this frame draws what they did.
    pub fn answer(&mut self, shell: &mut Shell, ctx: &egui::Context) {
        while let Ok(call) = self.calls.try_recv() {
            let Call { command, params, reply } = call;
            let wait = match shell.answer(&command, &params) {
                Answer::Now(result) => {
                    reply.send(result.map_err(protocol_error));
                    continue;
                }
                Answer::Screenshot(Screenshot { path }) => Wait::Shot { frames: SCREENSHOT_SETTLE, path: path.into() },
                Answer::Loaded(loaded) => Wait::Loaded { until: Instant::now() + loaded.wait() },
                Answer::Opening => Wait::Opening { until: Instant::now() + swarmception_ui::control::Loaded::default().wait() },
            };
            self.pending.push(Pending { wait, reply });
        }
        if self.pending.is_empty() {
            return;
        }
        let settled = self.pending.iter().any(|p| matches!(p.wait, Wait::Loaded { .. })) && shell.settled();
        let now = Instant::now();
        let mut waiting = Vec::new();
        for pending in std::mem::take(&mut self.pending) {
            if let Some(still) = step(pending, shell, ctx, settled, now) {
                waiting.push(still);
            }
        }
        self.pending = waiting;
        ctx.request_repaint();
    }
}

/// A waiting request this frame: answered (none back), or still waiting.
fn step(mut pending: Pending, shell: &mut Shell, ctx: &egui::Context, settled: bool, now: Instant) -> Option<Pending> {
    match &mut pending.wait {
        Wait::Shot { frames, .. } if *frames > 1 => *frames -= 1,
        Wait::Shot { path, .. } => {
            take_screenshot(ctx, std::mem::take(path), pending.reply);
            return None;
        }
        Wait::Loaded { .. } if settled => {
            pending.reply.send(inspect(shell));
            return None;
        }
        Wait::Loaded { until } if now >= *until => {
            pending.reply.send(Err(ProtocolError::Ui { message: "the window is still loading".into() }));
            return None;
        }
        Wait::Loaded { .. } => {}
        Wait::Opening { until } => match shell.open_answer() {
            Some(answer) => {
                pending.reply.send(match answer {
                    Ok(status) => serde_json::to_string(&status).map_err(|e| ProtocolError::Ui { message: e.to_string() }),
                    Err(error) => Err(protocol_error(ShellError::Engine(error))),
                });
                return None;
            }
            None if now >= *until => {
                pending.reply.send(Err(ProtocolError::Ui { message: "the folder is still being checked".into() }));
                return None;
            }
            None => {}
        },
    }
    Some(pending)
}

/// Asks egui for the window's image, saved to `path` once it comes and answered with its size.
fn take_screenshot(ctx: &egui::Context, path: PathBuf, reply: Responder) {
    ctx.request_screenshot(move |image| {
        let [width, height] = image.size;
        let saved = swarmception_ui::shot::save(&path, &image)
            .map_err(|e| ProtocolError::Ui { message: format!("can't save {}: {e}", path.display()) })
            .map(|()| Saved { path: path.display().to_string(), width, height })
            .and_then(|saved| serde_json::to_string(&saved).map_err(|e| ProtocolError::Ui { message: e.to_string() }));
        reply.send(saved);
    });
}

/// `ui.inspect`'s answer.
fn inspect(shell: &mut Shell) -> Result<String, ProtocolError> {
    match shell.answer("ui.inspect", "") {
        Answer::Now(result) => result.map_err(protocol_error),
        Answer::Screenshot(_) | Answer::Loaded(_) | Answer::Opening => Err(ProtocolError::Ui { message: "ui.inspect waited".into() }),
    }
}

/// The shell's error as the protocol carries it.
fn protocol_error(error: ShellError) -> ProtocolError {
    match error {
        ShellError::Engine(error) => ProtocolError::from(error),
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

    /// The window's size in the tests, in points at one pixel a point.
    const SCREEN: [usize; 2] = [1400, 900];

    /// The image a screenshot of the test's window gets: no renderer here, so a pattern of its
    /// size, as an integration hands the callback the frame it drew.
    fn drawn_image() -> egui::ColorImage {
        let [width, height] = SCREEN;
        let pixels = (0..width * height).map(|i| egui::Color32::from_rgb((i % width % 256) as u8, (i / width % 256) as u8, 0x2a)).collect();
        egui::ColorImage::new(SCREEN, pixels)
    }

    fn frame(shell: &mut Shell, ctx: &egui::Context, control: &mut Control) {
        control.answer(shell, ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SCREEN[0] as f32, SCREEN[1] as f32));
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| shell.ui(ui));
        output.textures_delta.clear();
        // The screenshots asked for this frame, completed as eframe does after drawing it.
        for viewport in output.viewport_output.into_values() {
            for command in viewport.commands {
                if let egui::ViewportCommand::Screenshot(callback) = command {
                    callback.complete(std::sync::Arc::new(drawn_image()));
                }
            }
        }
    }

    /// Runs `ask` on a client thread over a real socket while this thread draws frames and
    /// answers its requests, as the app's UI thread does: what `ask` returned.
    fn drive<T: Send + 'static>(shell: &mut Shell, ctx: &egui::Context, ask: impl FnOnce(&mut Client) -> T + Send + 'static) -> T {
        drive_at(shell, ctx, move |listening| ask(&mut Client::connect(listening.address, &listening.token, Duration::from_secs(20)).unwrap()))
    }

    /// As [`drive`], `ask` given where the window listens and its token.
    fn drive_at<T: Send + 'static>(shell: &mut Shell, ctx: &egui::Context, ask: impl FnOnce(Listening) -> T + Send + 'static) -> T {
        let (mut control, handler) = channel(ctx);
        let (listener, listening) = bind(0, None).unwrap();
        let token = listening.token.clone();
        std::thread::spawn(move || serve(&listener, &token, &Shared::new(handler)));
        let client = std::thread::spawn(move || ask(listening));
        while !client.is_finished() {
            frame(shell, ctx, &mut control);
            std::thread::sleep(Duration::from_millis(5));
        }
        client.join().unwrap()
    }

    /// #161: `repo.open` over the socket is answered once the folder's check lands in the
    /// background (the UI's thread drawing meanwhile): a refusal as the error it always was, an
    /// opened repository as its status.
    #[test]
    fn repo_open_over_the_socket_answers_once_the_folder_is_checked() {
        let dir = repo();
        let plain = std::env::temp_dir().join(format!("swarmception-control-plain-{}", std::process::id()));
        std::fs::create_dir_all(&plain).unwrap();
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, None);
        let (asked_plain, asked_repo) = (plain.display().to_string(), dir.display().to_string());
        let (refused, opened) = drive(&mut shell, &ctx, move |client| {
            let refused: Result<swarmception_engine::repo_cmds::StatusResult, ClientError> =
                client.call("repo.open", &serde_json::json!({ "path": asked_plain }));
            let opened: swarmception_engine::repo_cmds::StatusResult = client.call("repo.open", &serde_json::json!({ "path": asked_repo })).unwrap();
            (refused, opened)
        });
        match refused {
            Err(ClientError::Protocol(ProtocolError::Engine { error, .. })) => assert!(error.to_string().contains("is not in a git repository"), "{error}"),
            other => panic!("{other:?}"),
        }
        let top = std::fs::canonicalize(&dir).unwrap().display().to_string();
        assert_eq!((opened.repo.as_deref(), opened.opening, opened.refused), (Some(top.as_str()), None, None));
        std::fs::remove_dir_all(plain).ok();
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn what_ui_inspect_reports_over_the_socket_is_what_the_pane_drew() {
        let dir = repo();
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, Some(&dir));
        let t = Timestamp::parse("2026-10-31T03:42:00Z").unwrap();
        let (set, inspect, wrong) = drive(&mut shell, &ctx, move |client| {
            // Wait for the read to land, then put the factory at 03:42 with #7 selected. A
            // deadline only a failure reaches (#96): a count of tries shrinks with a loaded machine.
            let deadline = Instant::now() + Duration::from_secs(120);
            while !client.call::<Inspect>("ui.inspect", &()).unwrap().timeline.has_data {
                assert!(Instant::now() < deadline, "the read never landed");
                std::thread::sleep(Duration::from_millis(10));
            }
            let set: Inspect = client.call("ui.set", &serde_json::json!({"at": "2026-10-31T03:42:00Z", "ticket": 7})).unwrap();
            std::thread::sleep(Duration::from_millis(200));
            let inspect: Inspect = client.call("ui.inspect", &()).unwrap();
            let wrong: Result<Inspect, ClientError> = client.call("ui.set", &serde_json::json!({"at": "noon"}));
            (set, inspect, wrong)
        });
        assert_eq!(set.factory.t, Some(t));
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
        let asked = shell.engine().call::<At>(AtParams { t: Some(t), recent: Some(shown.recent), ticket: Some(7), run: None }).unwrap();
        assert_eq!(reported.gate, asked.gate.crates.iter().map(|c| c.issue).collect::<Vec<_>>(), "#7 is in the gate at 03:42");
        assert_eq!(reported.gate, [7]);
        assert!(!reported.journey.is_empty(), "#7's journey is traced");
        // #7's bars are selected on the timeline, and the playhead is at 03:42's item.
        assert!(inspect.timeline.selected.values().any(|ranges| !ranges.is_empty()), "{:?}", inspect.timeline.selected);
        assert!(inspect.timeline.playhead_item.is_some());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn ui_set_opens_sorts_and_docks_the_workers_pane_over_the_socket() {
        use swarmception_ui::{WorkersColumn, WorkersView};

        let dir = repo();
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, Some(&dir));
        let (set, inspect, wrong, closed) = drive(&mut shell, &ctx, |client| {
            let _: Inspect = client.call("ui.loaded", &serde_json::json!({"within": 15})).unwrap();
            let set: Inspect = client
                .call(
                    "ui.set",
                    &serde_json::json!({"workers": {"open": true, "sort": "landed", "wholeRun": true},
                        "dock": {"kind": "split", "across": false, "parts": [
                            {"share": 400, "node": {"kind": "tabs", "panes": ["timeline"], "active": 0}},
                            {"share": 600, "node": {"kind": "tabs", "panes": ["workers", "factory", "details"], "active": 0}}]}}),
                )
                .unwrap();
            // Drawn on the frames after.
            std::thread::sleep(Duration::from_millis(300));
            let inspect: Inspect = client.call("ui.inspect", &()).unwrap();
            let wrong: Result<Inspect, ClientError> = client.call("ui.set", &serde_json::json!({"dock": {"kind": "tabs", "panes": [], "active": 0}}));
            let closed: Inspect = client.call("ui.set", &serde_json::json!({"workers": {"open": false}})).unwrap();
            (set, inspect, wrong, closed)
        });
        let asked = WorkersView { whole_run: true, sort: Some(WorkersColumn::Landed), ascending: false };
        assert_eq!((set.state.panes.workers, set.state.workers), (true, asked));
        // Read back: the state as set, and the pane drawn in its tile (#177), its rows what the
        // pane drew.
        assert_eq!(inspect.state.workers, asked);
        assert_eq!(Some(&inspect.dock), set.state.dock.as_ref(), "the dock as set");
        let rect = inspect.workers.rect.expect("the pane drawn");
        assert!(rect.width > 200 && rect.height > 100, "{rect:?}");
        assert!(inspect.workers.rows.iter().any(|row| row.starts_with("mini-aa1111 | worker-1")), "{:?}", inspect.workers.rows);
        assert!(matches!(wrong, Err(ClientError::Protocol(ProtocolError::Engine { .. }))), "{wrong:?}");
        assert!(!closed.state.panes.workers);
        assert_eq!(closed.state.workers, asked, "closed, its sort kept");
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
            let shot: Result<serde_json::Value, ClientError> = client.call("ui.screenshot", &serde_json::json!({"path": "x.jpg"}));
            (commands, widget, unknown, shot)
        });
        assert!(commands.commands.iter().any(|c| c.id == "factory.at"));
        assert!(matches!(widget, Err(ClientError::Protocol(ProtocolError::Widget { .. }))));
        assert!(matches!(unknown, Err(ClientError::Protocol(ProtocolError::Engine { .. }))));
        assert!(matches!(shot, Err(ClientError::Protocol(ProtocolError::Engine { .. }))));
    }

    /// One JSON-RPC line a request.
    fn rpc(id: u64, method: &str, params: &serde_json::Value) -> String {
        serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
    }

    fn call(id: u64, tool: &str, arguments: &serde_json::Value) -> String {
        rpc(id, "tools/call", &serde_json::json!({ "name": tool, "arguments": arguments }))
    }

    #[test]
    fn mcp_through_the_window_runs_every_tool_there_and_saves_a_png() {
        use swarmception_automation::client::Remote;
        use swarmception_automation::mcp::{CallResult, ToolList};

        let dir = repo();
        let png = dir.join("worker24-window.png");
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, Some(&dir));
        let requests = [
            rpc(1, "initialize", &serde_json::json!({})),
            rpc(2, "tools/list", &serde_json::json!({})),
            call(3, "ui_loaded", &serde_json::json!({"within": 15})),
            call(4, "ui_set", &serde_json::json!({"at": "2026-10-31T03:42:00Z", "ticket": 7})),
            call(5, "ui_screenshot", &serde_json::json!({ "path": png.display().to_string() })),
            call(6, "timeline_view_fit", &serde_json::json!({})),
            call(7, "factory_at", &serde_json::json!({"t": Timestamp::parse("2026-10-31T03:42:00Z").unwrap()})),
            call(8, "ui_set", &serde_json::json!({"at": "noon"})),
        ]
        .join("\n");
        // MCP's stdio, over the window: each tool a request on its control channel.
        let output = drive_at(&mut shell, &ctx, move |listening| {
            let mut output = Vec::new();
            swarmception_automation::mcp::serve(requests.as_bytes(), &mut output, &mut Remote::new(listening, Duration::from_secs(25))).unwrap();
            String::from_utf8(output).unwrap()
        });
        let replies: Vec<serde_json::Value> = output.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(replies.len(), 8, "{output}");
        let result = |n: usize| -> CallResult { serde_json::from_value(replies[n]["result"].clone()).unwrap() };

        // The tools: the engine's, the window's and the widget's, each with its schema.
        let tools: ToolList = serde_json::from_value(replies[1]["result"].clone()).unwrap();
        let names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_str()).collect();
        for name in [
            "engine_commands",
            "factory_at",
            "timeline_placeAt",
            "ui_commands",
            "ui_inspect",
            "ui_set",
            "ui_loaded",
            "ui_screenshot",
            "timeline_view_fit",
            "timeline_state_get",
        ] {
            assert!(names.contains(&name), "{name} in {names:?}");
        }
        assert!(tools.tools.iter().all(|t| t.input_schema.is_object()), "each tool's input is a schema");
        let ui_set = tools.tools.iter().find(|t| t.name == "ui_set").unwrap();
        assert_eq!(ui_set.input_schema, serde_json::to_value(schemars::schema_for!(swarmception_ui::control::UiSet)).unwrap());

        let loaded: Inspect = serde_json::from_str(result(2).text()).unwrap();
        assert!(!loaded.loading && loaded.timeline.has_data, "ui.loaded answers once the timeline has its data");
        let set: Inspect = serde_json::from_str(result(3).text()).unwrap();
        assert_eq!((set.factory.t, set.state.factory.ticket), (Timestamp::parse("2026-10-31T03:42:00Z"), Some(7)));

        // The screenshot, saved by the window as PNG: it reads back as the frame it was handed.
        let saved: Saved = serde_json::from_str(result(4).text()).unwrap();
        assert_eq!((saved.path.as_str(), saved.width, saved.height), (png.to_str().unwrap(), SCREEN[0], SCREEN[1]));
        let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&png).unwrap())).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width as usize, info.height as usize, info.color_type), (SCREEN[0], SCREEN[1], png::ColorType::Rgb));
        let expected: Vec<u8> = drawn_image().pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
        assert!(pixels[..info.buffer_size()] == expected[..], "the PNG's pixels are the frame's");
        // And it came back as MCP's image too (#76), the file's bytes.
        let file = std::fs::read(&png).unwrap();
        assert_eq!(result(4).image(), Some((swarmception_automation::mcp::base64(&file).as_str(), "image/png")));

        assert!(!result(5).is_error, "the widget's command, through the window: {:?}", result(5));
        assert!(!result(6).is_error && result(6).text().contains("\"gate\""), "the engine's, on the window's session: {:?}", result(6));
        let refused = result(7);
        assert!(refused.is_error);
        let error: ProtocolError = serde_json::from_str(refused.text()).unwrap();
        assert!(matches!(error, ProtocolError::Engine { .. }), "{error:?}");
        std::fs::remove_dir_all(dir).ok();
    }
}
