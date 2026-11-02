//! Collapse to the window (#74, G6) over a real repository's tracking branch: twelve workers,
//! each with a ticket started a minute after the last, so a window early on holds a few of them.
//! What the pane draws is what `rows.window` answers for its window; the rows follow a pan once
//! it settles; the selection, the highlight and the details' subject survive rows coming and
//! going; by worker, a worker's group with nothing in the window folds away.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use swarmception_domain::Collapse;
use swarmception_engine::Session;
use swarmception_engine::rows_cmds::WindowResult;
use swarmception_ui::control::{Answer, Inspect};
use swarmception_ui::{Lit, Shell, TimelinePane};
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::state_cmds::{Get as GetState, Set as SetState, SetParams as SetStateParams};
use timeline_engine::tracks_cmds::{Layout, LayoutParams, SetTrack, SetTrackParams};
use timeline_engine::wire::SpanWire;

const WORKERS: usize = 12;
const SIZE: (f32, f32) = (1440.0, 900.0);

fn scratch(what: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-collapse-{what}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Twelve workers on `mini`, worker-<kk> claiming ticket #<10 + k> at 03:<kk>, submitting it at
/// 03:<20 + k>, and a coordinator merging it at 03:<40 + k>: flat, #<10 + k> on `tickets/<k>`.
fn repo() -> PathBuf {
    let dir = scratch("repo");
    let mut coordinator = vec![r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#.to_owned()];
    coordinator.extend((1..=WORKERS).map(|k| format!(r#"{{"event":"merge","issue":{},"mr":{},"t":"2026-10-31T03:{:02}:00Z"}}"#, 10 + k, 100 + k, 40 + k)));
    std::fs::create_dir_all(dir.join("tracking/mini-co01")).unwrap();
    std::fs::write(dir.join("tracking/mini-co01/work.jsonl"), coordinator.join("\n") + "\n").unwrap();
    for k in 1..=WORKERS {
        let lines = [
            format!(r#"{{"event":"session.start","machine":"mini","name":"worker-{k:02}","role":"worker","t":"2026-10-31T03:00:00Z"}}"#),
            format!(r#"{{"event":"claim","issue":{},"t":"2026-10-31T03:{k:02}:00Z"}}"#, 10 + k),
            format!(r#"{{"event":"work","issue":{},"t":"2026-10-31T03:{k:02}:30Z"}}"#, 10 + k),
            format!(r#"{{"event":"submit","issue":{},"mr":{},"t":"2026-10-31T03:{:02}:00Z"}}"#, 10 + k, 100 + k, 20 + k),
        ];
        let instance = format!("mini-w{k:02}");
        std::fs::create_dir_all(dir.join("tracking").join(&instance)).unwrap();
        std::fs::write(dir.join("tracking").join(&instance).join("work.jsonl"), lines.join("\n") + "\n").unwrap();
    }
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(&dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    git(&["init", "-q", "-b", "tracking"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "events"]);
    dir
}

fn frame_with(shell: &mut Shell, ctx: &egui::Context, events: Vec<egui::Event>) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SIZE.0, SIZE.1));
    let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
}

fn frame(shell: &mut Shell, ctx: &egui::Context) {
    frame_with(shell, ctx, Vec::new());
}

fn loaded(dir: &Path) -> (egui::Context, Shell) {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, Some(dir));
    for _ in 0..500 {
        frame(&mut shell, &ctx);
        if shell.has_timeline() && !shell.loading() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(shell.has_timeline(), "the tracking branch loaded");
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    (ctx, shell)
}

fn answer(shell: &mut Shell, command: &str, params: &str) -> String {
    match shell.answer(command, params) {
        Answer::Now(Ok(text)) => text,
        other => panic!("{command} {params}: {other:?}"),
    }
}

fn set(shell: &mut Shell, params: &str) -> Inspect {
    serde_json::from_str(&answer(shell, "ui.set", params)).unwrap()
}

/// The rows the widget lays out, top to bottom, by key: what the pane draws.
fn drawn(shell: &mut Shell) -> Vec<String> {
    let layout = shell.timeline().widget().session().call::<Layout>(LayoutParams { pane: 100_000.0, scroll: 0.0 }).unwrap();
    layout.rows.into_iter().map(|row| row.key).collect()
}

/// What `rows.window` answers for the window the pane shows and the rows its selection keeps.
fn rows_window(shell: &mut Shell) -> WindowResult {
    let inspect = shell.inspect();
    let window = inspect.timeline.window.unwrap();
    let keep: Vec<&String> = inspect.timeline.selected.iter().filter(|(_, ranges)| !ranges.is_empty()).map(|(key, _)| key).collect();
    let params = serde_json::json!({"start": window.start.0, "end": window.end.0, "keep": keep});
    serde_json::from_str(&answer(shell, "rows.window", &params.to_string())).unwrap()
}

fn tickets(from: usize, to: usize) -> Vec<String> {
    (from..=to).map(|k| format!("tickets/{k}")).collect()
}

const EARLY: &str = r#"{"start": "2026-10-31T03:00:00Z", "end": "2026-10-31T03:03:10Z"}"#;

#[test]
fn collapsed_to_the_window_the_pane_draws_the_rows_rows_window_shows() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let all = drawn(&mut shell);
    assert_eq!(all.len(), 2 + WORKERS, "the two top rows and a row a ticket: {all:?}");

    let inspect = set(&mut shell, &format!(r#"{{"collapse": "window", "window": {EARLY}}}"#));
    assert_eq!(shell.menu_state().collapse, Collapse::Window, "ui.set sets it through rows.set");
    assert!(inspect.timeline.left_out.is_empty(), "the rows follow on the next frame");
    frame(&mut shell, &ctx);
    let inspect = shell.inspect();
    let mut left_out = vec!["merged".to_owned()];
    left_out.extend(tickets(4, WORKERS));
    assert_eq!(inspect.timeline.left_out, left_out, "#11 to #13 began by 03:03; the merges came after 03:40");
    assert!(inspect.timeline.collapse_us.is_some());
    let found = rows_window(&mut shell);
    assert_eq!((found.collapse, &found.left_out), (Collapse::Window, &left_out));
    assert_eq!(drawn(&mut shell), found.shown, "what the pane draws is what rows.window shows");
    assert_eq!(found.shown, tickets(1, 3));

    // Off again: every row comes back.
    set(&mut shell, r#"{"collapse": "off"}"#);
    frame(&mut shell, &ctx);
    assert!(shell.inspect().timeline.left_out.is_empty());
    assert_eq!(drawn(&mut shell).len(), 2 + WORKERS, "the main row too, empty, with collapse off");
    assert_eq!(drawn(&mut shell), rows_window(&mut shell).shown);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_rows_follow_a_pan_once_the_window_settles_and_not_while_a_button_is_held() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    set(&mut shell, &format!(r#"{{"collapse": "window", "window": {EARLY}}}"#));
    frame(&mut shell, &ctx);
    let early = shell.inspect().timeline.left_out;
    assert_eq!(early.len(), 1 + WORKERS - 3);

    // Panned to 03:10-03:12 (positions from 03:00, in nanoseconds), as a drag moves it.
    let window = SpanWire { start: 600_000_000_000, end: 720_000_000_000 };
    shell.timeline().widget().session().call::<SetState>(SetStateParams { window: Some(window), ..SetStateParams::default() }).unwrap();
    frame(&mut shell, &ctx);
    assert_eq!(shell.inspect().timeline.left_out, early, "the rows hold still while the window moves");
    let pos = egui::pos2(SIZE.0 / 2.0, SIZE.1 - 4.0);
    let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(pos), button(true)]);
    std::thread::sleep(swarmception_ui::COLLAPSE_SETTLE + Duration::from_millis(50));
    frame(&mut shell, &ctx);
    assert_eq!(shell.inspect().timeline.left_out, early, "nor while a button is held");
    frame_with(&mut shell, &ctx, vec![button(false)]);
    frame(&mut shell, &ctx);
    let inspect = shell.inspect();
    assert_eq!(inspect.timeline.left_out, ["merged"], "settled: every ticket had begun by 03:12, none merged");
    assert_eq!(drawn(&mut shell), tickets(1, WORKERS));
    assert_eq!(drawn(&mut shell), rows_window(&mut shell).shown);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_selection_highlight_and_details_survive_rows_coming_and_going() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    set(&mut shell, r#"{"collapse": "window"}"#);
    // #22 (tickets/12, and its merge on the merges row) selected, the details on it.
    set(&mut shell, r#"{"ticket": 22, "details": {"ticket": {"issue": 22}}}"#);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    let before = shell.inspect().timeline;
    assert!(before.selected.contains_key("tickets/12") && before.selected.contains_key("merged"), "{:?}", before.selected);
    assert_eq!(before.lit, Lit::Details);

    set(&mut shell, &format!(r#"{{"window": {EARLY}}}"#));
    frame(&mut shell, &ctx);
    let after = shell.inspect();
    assert_eq!(after.timeline.left_out, tickets(4, WORKERS - 1), "the rows holding the selection (tickets/12, merged) are kept");
    assert_eq!(after.timeline.selected, before.selected, "the selection as it was");
    assert_eq!((after.timeline.lit, &after.timeline.highlighted), (Lit::Details, &before.highlighted), "the highlight as it was");
    assert_eq!(after.details.subject, before_subject(), "the details' subject as it was");
    assert_eq!(drawn(&mut shell), rows_window(&mut shell).shown);
    assert!(drawn(&mut shell).contains(&"tickets/12".to_owned()));

    // The selection cleared: its rows go too.
    set(&mut shell, r#"{"clearTicket": true}"#);
    frame(&mut shell, &ctx);
    let mut left_out = vec!["merged".to_owned()];
    left_out.extend(tickets(4, WORKERS));
    assert_eq!(shell.inspect().timeline.left_out, left_out);
    assert_eq!(drawn(&mut shell), tickets(1, 3));
    std::fs::remove_dir_all(dir).ok();
}

fn before_subject() -> Option<swarmception_domain::details::Subject> {
    Some(swarmception_domain::details::Subject::Ticket { issue: 22 })
}

#[test]
fn by_worker_a_group_with_nothing_in_the_window_folds_away() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    answer(&mut shell, "rows.set", r#"{"grouping": "workers"}"#);
    set(&mut shell, &format!(r#"{{"collapse": "window", "window": {EARLY}}}"#));
    frame(&mut shell, &ctx);
    let mut left_out = vec!["merged".to_owned()];
    left_out.extend((4..=WORKERS).map(|k| format!("w/mini/worker-{k:02}")));
    assert_eq!(shell.inspect().timeline.left_out, left_out, "each worker's group whole, not its rows");
    let shown = rows_window(&mut shell).shown;
    assert_eq!(drawn(&mut shell), shown);
    assert_eq!(shown[..3], ["w/mini", "w/mini/worker-01", "w/mini/worker-01/1"]);
    assert_eq!(shown.len(), 1 + 3 * 2);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_rows_a_window_hid_are_not_saved_and_the_persons_own_stay_hidden() {
    let dir = repo();
    let mut engine = Session::new();
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    engine.execute("repo.open", &format!("{{\"path\":{path}}}")).unwrap();
    for _ in 0..500 {
        engine.poll();
        if engine.has_data().is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    engine.execute("rows.set", r#"{"collapse": "window"}"#).unwrap();
    let mut pane = TimelinePane::default();
    pane.sync(&mut engine).unwrap();
    // The person hid tickets/2 themselves.
    let hide = |pane: &mut TimelinePane, key: &str, hidden: bool| {
        pane.widget().session().call::<SetTrack>(SetTrackParams { key: key.into(), hidden: Some(hidden), ..SetTrackParams::default() }).unwrap();
    };
    hide(&mut pane, "tickets/2", true);
    let window = SpanWire { start: 60_000_000_000, end: 190_000_000_000 };
    pane.widget().session().call::<SetState>(SetStateParams { window: Some(window), ..SetStateParams::default() }).unwrap();
    assert_eq!(pane.collapse_to_window(&mut engine, Instant::now(), false), None, "the first look settles at once");
    assert_eq!(pane.left_out().len(), 1 + WORKERS - 3);
    assert!(pane.collapse_time().is_some());
    let hidden = |pane: &mut TimelinePane| -> Vec<String> {
        let state = pane.widget().session().call::<GetState>(NoParams {}).unwrap().timeline.unwrap();
        state.tracks.iter().filter(|(_, s)| s.hidden).map(|(key, _)| key.clone()).collect()
    };
    assert_eq!(hidden(&mut pane).len(), 1 + WORKERS - 3 + 1, "the window's and the person's");

    let saved = pane.view_state(&mut engine).unwrap().widget.unwrap();
    let kept: Vec<&String> = saved.tracks.iter().filter(|(_, s)| s.hidden).map(|(key, _)| key).collect();
    assert_eq!(kept, ["tickets/2"], "the saved state keeps the person's hidden row, not the window's");

    // Collapse off: the window's rows come back, the person's stays hidden.
    engine.execute("rows.set", r#"{"collapse": "empty"}"#).unwrap();
    pane.sync(&mut engine).unwrap();
    assert_eq!(pane.collapse_to_window(&mut engine, Instant::now(), false), None);
    assert!(pane.left_out().is_empty());
    assert_eq!(hidden(&mut pane), ["tickets/2"]);
    std::fs::remove_dir_all(dir).ok();
}
