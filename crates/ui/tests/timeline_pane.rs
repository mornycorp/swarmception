//! The timeline pane over a real repository's tracking branch: what it loads into the widget, and
//! what it keeps when the engine's rows move on.

use std::path::PathBuf;

use swarmception_engine::Session;
use swarmception_ui::TimelinePane;
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::rulers_cmds::{Add, AddParams, Set, SetParams};
use timeline_engine::state_cmds::Get;
use timeline_engine::view_cmds::{Show, ShowParams};
use timeline_engine::wire::{OriginWire, RulerDomainWire};

/// A repository in a temporary directory whose tracking branch holds one ticket.
fn repo() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "swarmception-ui-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        // Two tests can start within one tick of the clock: a counter keeps their directories apart.
        {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        }
    ));
    std::fs::create_dir_all(dir.join("tracking/mini-aa1111")).unwrap();
    std::fs::write(
        dir.join("tracking/mini-aa1111/work.jsonl"),
        concat!(
            r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
            "\n",
            r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
            "\n",
            r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:30:00Z"}"#,
            "\n",
        ),
    )
    .unwrap();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    run(&["init", "-q", "-b", "tracking"]);
    run(&["add", "."]);
    run(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "events"]);
    dir
}

fn rulers(pane: &mut TimelinePane) -> Vec<RulerDomainWire> {
    let state = pane.widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
    state.rulers.iter().map(|r| r.domain).collect()
}

#[test]
fn the_first_load_reads_the_clock_and_a_reload_keeps_the_rulers() {
    let dir = repo();
    let mut engine = Session::new();
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    engine.execute("repo.open", &format!("{{\"path\":{path}}}")).unwrap();
    let mut pane = TimelinePane::default();
    pane.sync(&engine).unwrap();
    assert_eq!(rulers(&mut pane), [RulerDomainWire::Clock], "the scrub ruler reads the time of day");
    let text = pane.clock_text(&mut engine).unwrap();
    assert!(text.starts_with("2026-10-31 03:") && text.contains(" UTC"), "{text}");

    // The person picks the Time ruler and adds a Clock ruler under it; new rows keep both.
    let session = pane.widget().session();
    session.call::<Set>(SetParams { ruler: 0, domain: Some(RulerDomainWire::Axis), origin: None }).unwrap();
    session.call::<Add>(AddParams { domain: RulerDomainWire::Clock, origin: Some(OriginWire::Absolute) }).unwrap();
    engine.execute("rows.set", r#"{"grouping":"workers"}"#).unwrap();
    pane.sync(&engine).unwrap();
    assert_eq!(rulers(&mut pane), [RulerDomainWire::Axis, RulerDomainWire::Clock]);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_toolbar_shows_only_what_swarmception_needs_and_the_rest_is_fixed_by_command() {
    use timeline_engine::menu_cmds::Toolbar;
    use timeline_engine::minimap_cmds::VisibilityWire;
    use timeline_engine::state_cmds::{Set as SetState, SetParams as SetStateParams};
    use timeline_engine::toolbar_cmds::{GetToolbar, ToolbarControl as C};
    use timeline_engine::wire::ClickModeWire;

    // The spec, before any data and after: the navigate items, the zoom buttons, the measurement.
    let wanted = [C::Fit, C::ZoomToSelection, C::UndoZoom, C::ZoomIn, C::ZoomOut, C::ZoomFit, C::Measurement];
    assert_eq!(swarmception_ui::TIMELINE_TOOLBAR, wanted);
    let mut pane = TimelinePane::default();
    assert_eq!(pane.widget().session().call::<GetToolbar>(NoParams {}).unwrap().show, wanted);

    let dir = repo();
    let mut engine = Session::new();
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    engine.execute("repo.open", &format!("{{\"path\":{path}}}")).unwrap();
    pane.sync(&engine).unwrap();
    let fixed = |pane: &mut TimelinePane| {
        let state = pane.widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
        (state.scale, state.origin, state.minimap, state.click)
    };
    let expected = ("Time".to_owned(), OriginWire::Absolute, VisibilityWire::Always, ClickModeWire::Nearest);
    assert_eq!(fixed(&mut pane), expected, "set by command on load");
    let bar = pane.widget().session().call::<Toolbar>(NoParams {}).unwrap();
    assert_eq!(bar.show, wanted, "the toolbar draws only these");
    assert_eq!(bar.navigate.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Fit", "Zoom to Selection", "Undo Zoom"]);
    assert_eq!(bar.zoom.iter().map(|b| b.control).collect::<Vec<_>>(), [C::ZoomIn, C::ZoomOut, C::ZoomFit], "the zoom buttons are the widget's");
    assert!(bar.zoom.iter().all(|b| b.item.enabled), "{:?}", bar.zoom);

    // Something moves them (the keyboard, a ruler's menu); the next load fixes them again and
    // the spec holds.
    let session = pane.widget().session();
    session.call::<SetState>(SetStateParams { scale: Some("Unit".to_owned()), origin: Some(OriginWire::Viewport), ..SetStateParams::default() }).unwrap();
    engine.execute("rows.set", r#"{"grouping":"workers"}"#).unwrap();
    pane.sync(&engine).unwrap();
    assert_eq!(fixed(&mut pane), expected);
    assert_eq!(pane.widget().session().call::<GetToolbar>(NoParams {}).unwrap().show, wanted);
    std::fs::remove_dir_all(dir).ok();
}

/// One frame of the shell, 1000 × 700 points, with `events`.
fn frame(shell: &mut swarmception_ui::Shell, ctx: &egui::Context, events: Vec<egui::Event>) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
    let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
}

#[test]
fn ctrl_drag_through_the_shell_pans_the_timeline() {
    // macOS's Ctrl is egui's `ctrl` alone, its Cmd `command`: both pan, and the shell takes neither.
    let ctrl = egui::Modifiers { ctrl: true, ..Default::default() };
    let cmd = egui::Modifiers { command: true, mac_cmd: true, ..Default::default() };
    let dir = repo();
    for modifiers in [ctrl, cmd] {
        let ctx = egui::Context::default();
        let mut shell = swarmception_ui::Shell::new(&ctx, Some(&dir));
        for _ in 0..500 {
            frame(&mut shell, &ctx, Vec::new());
            if shell.has_timeline() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(shell.has_timeline(), "the tracking branch loaded");
        // A height in the timeline's rows, under the minimap's strip (always shown since #35):
        // ticket 7's bar's, while the whole swarm shows.
        frame(&mut shell, &ctx, Vec::new());
        let y = shell.bar_point(7).expect("ticket 7's bar is drawn").y;
        let window = |shell: &mut swarmception_ui::Shell| {
            let state = shell.timeline().widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
            (state.window.start, state.window.end)
        };
        // A window inside the data, so a pan has room either way.
        let session = shell.timeline().widget().session();
        session.call::<Show>(ShowParams { start: 600_000_000_000, end: 1_200_000_000_000 }).unwrap();
        frame(&mut shell, &ctx, Vec::new());
        let before = window(&mut shell);
        frame(&mut shell, &ctx, vec![egui::Event::ModifiersChanged(modifiers)]);
        let button = |at, pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers };
        let start = egui::pos2(600.0, y);
        frame(&mut shell, &ctx, vec![egui::Event::PointerMoved(start), button(start, true)]);
        for x in [570.0, 520.0, 450.0, 400.0] {
            frame(&mut shell, &ctx, vec![egui::Event::PointerMoved(egui::pos2(x, y))]);
        }
        frame(&mut shell, &ctx, vec![button(egui::pos2(400.0, y), false)]);
        let after = window(&mut shell);
        assert!(after.0 > before.0, "{modifiers:?}: the content followed the pointer left: {before:?} → {after:?}");
        assert_eq!(after.1 - after.0, before.1 - before.0, "{modifiers:?}: a pan keeps the span");
        let selection = shell.timeline().widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap().selection;
        assert!(selection.is_empty(), "{modifiers:?}: a pan selects nothing");
    }
    std::fs::remove_dir_all(dir).ok();
}
