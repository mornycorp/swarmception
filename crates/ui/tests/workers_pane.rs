//! The Workers pane (#67) over a real repository's tracking branch: it draws what
//! `workers.compare` returns for the timeline's window or the whole run (AGENTS.md rule 6), asks
//! again only when the window or the data moves, sorts by a column, lights a hovered worker's
//! tickets' bars on the timeline, and makes a clicked worker's session the Details pane's
//! subject.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swarmception_domain::details::Subject;
use swarmception_domain::rows::BarRef;
use swarmception_domain::workers::Comparison;
use swarmception_engine::factory_cmds::{TicketBars, TicketParams};
use swarmception_engine::stats_cmds::{At, GetParams};
use swarmception_engine::workers_cmds::Compare;
use swarmception_ui::{Lit, MenuAction, Shell, WorkerRowKind, WorkersColumn, worker_cells};

mod support;

/// worker-1 (opus, high) lands #7 after a send back and #9; worker-2 (opus, low) lands #11 after
/// a failed `ready` check and its rerun; the coordinator merges them.
fn repo() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-workers-pane-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [
        (
            "mini-aa1111",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","model":"claude-opus-5-5","effort":"high","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:02:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:12:00Z"}"#,
                r#"{"event":"claim","issue":9,"mr":10,"t":"2026-10-31T03:13:00Z"}"#,
                r#"{"event":"work","issue":9,"t":"2026-10-31T03:14:00Z"}"#,
                r#"{"event":"submit","issue":9,"mr":10,"t":"2026-10-31T03:24:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:25:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:30:00Z"}"#,
                r#"{"event":"session.stop","t":"2026-10-31T03:50:00Z"}"#,
            ],
        ),
        (
            "mini-bb2222",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-2","role":"worker","model":"claude-opus-5-5","effort":"low","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":11,"mr":12,"t":"2026-10-31T03:02:00Z"}"#,
                r#"{"event":"work","issue":11,"t":"2026-10-31T03:03:00Z"}"#,
                r#"{"event":"sub.start","name":"ready","sub":"ready-b1","t":"2026-10-31T03:15:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=1; cause=clippy","sub":"ready-b1","t":"2026-10-31T03:18:00Z"}"#,
                r#"{"event":"sub.start","name":"ready","sub":"ready-b1-rerun","t":"2026-10-31T03:20:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0","sub":"ready-b1-rerun","t":"2026-10-31T03:25:00Z"}"#,
                r#"{"event":"submit","issue":11,"mr":12,"t":"2026-10-31T03:26:00Z"}"#,
                r#"{"event":"session.stop","t":"2026-10-31T03:50:00Z"}"#,
            ],
        ),
        (
            "mini-cc3333",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"sendback","issue":7,"mr":8,"note":"cause=conflict: src/lib.rs","t":"2026-10-31T03:20:00Z"}"#,
                r#"{"event":"merge","issue":9,"mr":10,"t":"2026-10-31T03:33:00Z"}"#,
                r#"{"event":"merge","issue":7,"mr":8,"t":"2026-10-31T03:40:00Z"}"#,
                r#"{"event":"merge","issue":11,"mr":12,"t":"2026-10-31T03:45:00Z"}"#,
            ],
        ),
    ];
    for (instance, lines) in files {
        std::fs::create_dir_all(dir.join("tracking").join(instance)).unwrap();
        std::fs::write(dir.join("tracking").join(instance).join("work.jsonl"), lines.join("\n") + "\n").unwrap();
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

/// One frame of the shell, 1500 × 900 points, with `events`.
fn frame_with(shell: &mut Shell, ctx: &egui::Context, events: Vec<egui::Event>) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1500.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
}

fn frame(shell: &mut Shell, ctx: &egui::Context) {
    frame_with(shell, ctx, Vec::new());
}

/// The shell over `dir`, loaded, the workers pane opened from View ▸ Workers.
fn loaded(dir: &Path) -> (egui::Context, Shell) {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, Some(dir));
    support::until("the tracking branch loaded", || {
        frame(&mut shell, &ctx);
        shell.has_timeline()
    });
    assert!(shell.has_timeline(), "the tracking branch loaded");
    assert!(!shell.menu_state().workers, "hidden by default");
    shell.run_menu(&ctx, MenuAction::Workers);
    for _ in 0..4 {
        frame(&mut shell, &ctx);
    }
    (ctx, shell)
}

/// What `workers.compare` returns for the timeline's window now.
fn answer(shell: &mut Shell) -> Comparison {
    let window = shell.inspect().timeline.window.expect("the timeline's window");
    let params = GetParams { start: Some(At::Seconds(window.start.0)), end: Some(At::Seconds(window.end.0)) };
    shell.engine().call::<Compare>(params).unwrap()
}

/// The rows `compared` should be drawn as, in its order: each model × effort, its workers, then
/// every worker's together; each its key, its kind and its cells.
fn rows_of(compared: &Comparison) -> Vec<(String, WorkerRowKind, Vec<String>)> {
    let mut rows = Vec::new();
    let with_name = |name: &str, cells: Vec<String>| std::iter::once(name.to_owned()).chain(cells).collect::<Vec<_>>();
    for group in &compared.groups {
        rows.push((group.label.clone(), WorkerRowKind::Group, with_name(&group.label, worker_cells(&group.figures))));
        for worker in &group.workers {
            rows.push((worker.instance.clone(), WorkerRowKind::Worker, with_name(&worker.name, worker_cells(&worker.figures))));
        }
    }
    rows.push(("all".to_owned(), WorkerRowKind::All, with_name("All workers", worker_cells(&compared.all))));
    rows
}

fn drawn_rows(shell: &Shell) -> Vec<(String, WorkerRowKind, Vec<String>)> {
    shell.workers().drawn().rows.iter().map(|r| (r.key.clone(), r.kind, r.cells.clone())).collect()
}

fn by_row(bars: &[BarRef]) -> BTreeMap<String, Vec<usize>> {
    let mut rows = BTreeMap::<String, Vec<usize>>::new();
    for bar in bars {
        rows.entry(bar.track.clone()).or_default().push(bar.range);
    }
    for ranges in rows.values_mut() {
        ranges.sort_unstable();
        ranges.dedup();
    }
    rows
}

#[test]
fn the_pane_draws_what_workers_compare_returns() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let compared = answer(&mut shell);
    assert_eq!(shell.workers().compared(), Some(&compared), "the pane holds what workers.compare returns for the timeline's window");
    let drawn = shell.workers().drawn().clone();
    assert_eq!(drawn.window, Some((compared.start, compared.end)));
    assert_eq!(drawn_rows(&shell), rows_of(&compared), "a row a model × effort, its workers under it, then all: each cell as the figures read");
    let names: Vec<&str> = drawn.rows.iter().map(|r| r.cells[0].as_str()).collect();
    assert_eq!(names, ["opus 5.5 · high", "worker-1", "opus 5.5 · low", "worker-2", "All workers"]);
    let rate = &drawn.rows[1].cells[3];
    assert!(rate.starts_with('~') && rate.ends_with(" n=2"), "2 landed in about 30 active minutes: thin, marked, its samples said: {rate}");
    // ui.inspect reports what it drew; the open state is the UI state's, kept across a restart.
    let inspect = shell.inspect();
    assert_eq!(inspect.workers.rows.len(), drawn.rows.len());
    assert_eq!(inspect.workers.rows[1], format!("mini-aa1111 | {}", drawn.rows[1].cells.join(" | ")));
    assert!(inspect.state.panes.workers);

    // Asked again only when the window moves, or the whole run is asked for.
    let asks = shell.workers().asks();
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.workers().asks(), asks, "nothing moved: not asked again");
    let set = r#"{"window": {"start": "2026-10-31T03:00:00Z", "end": "2026-10-31T03:35:00Z"}}"#;
    assert!(matches!(shell.answer("ui.set", set), swarmception_ui::control::Answer::Now(Ok(_))));
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert!(shell.workers().asks() > asks);
    let narrow = answer(&mut shell);
    assert_eq!(shell.workers().compared(), Some(&narrow));
    let landed: Vec<usize> = narrow.groups.iter().flat_map(|g| &g.workers).map(|w| w.figures.landed).collect();
    assert_eq!(landed, [1, 0], "only #9's merge in the window");
    assert_eq!(drawn_rows(&shell), rows_of(&narrow));
    let mut state = shell.ui_state();
    state.workers.whole_run = true;
    shell.set_ui_state(state);
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    let whole = shell.engine().call::<Compare>(GetParams::default()).unwrap();
    assert_eq!(shell.workers().compared(), Some(&whole));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_header_sorts_by_its_column() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let header = |shell: &Shell, column| shell.workers().drawn().headers.iter().find(|(c, _)| *c == column).map(|(_, r)| r.center()).unwrap();
    let click = |shell: &mut Shell, at: egui::Pos2| {
        let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame_with(shell, &ctx, vec![egui::Event::PointerMoved(at), press(true)]);
        frame_with(shell, &ctx, vec![press(false)]);
        frame(shell, &ctx);
    };
    let groups =
        |shell: &Shell| -> Vec<String> { shell.workers().drawn().rows.iter().filter(|r| r.kind == WorkerRowKind::Group).map(|r| r.cells[0].clone()).collect() };
    assert_eq!(groups(&shell), ["opus 5.5 · high", "opus 5.5 · low"]);
    // Ready pass: high has no checks (none), low 50%: low first, none last either way.
    let ready = header(&shell, WorkersColumn::Ready);
    click(&mut shell, ready);
    assert_eq!(shell.ui_state().workers.sort, Some(WorkersColumn::Ready));
    assert!(!shell.ui_state().workers.ascending, "most first");
    assert_eq!(groups(&shell), ["opus 5.5 · low", "opus 5.5 · high"]);
    click(&mut shell, ready);
    assert!(shell.ui_state().workers.ascending, "again: the other way");
    assert_eq!(groups(&shell), ["opus 5.5 · low", "opus 5.5 · high"], "none stays last");
    // Landed: high's 2 before low's 1; the first column puts the command's order back.
    let landed = header(&shell, WorkersColumn::Landed);
    click(&mut shell, landed);
    assert_eq!(groups(&shell), ["opus 5.5 · high", "opus 5.5 · low"]);
    let name = header(&shell, WorkersColumn::Name);
    click(&mut shell, name);
    assert_eq!(shell.ui_state().workers.sort, None);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn hovering_a_worker_lights_its_tickets_and_a_click_tells_its_session() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let row = |shell: &Shell, key: &str| shell.workers().drawn().rows.iter().find(|r| r.key == key).map(|r| r.rect).unwrap();
    let one = row(&shell, "mini-aa1111");
    let at = egui::pos2(one.left() + 40.0, one.center().y);
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(at)]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.workers().drawn().hovered.as_deref(), Some("mini-aa1111"));
    let mut bars = Vec::new();
    for issue in [7, 9] {
        bars.extend(shell.engine().call::<TicketBars>(TicketParams { issue }).unwrap().bars);
    }
    assert_eq!((shell.timeline().lit(), shell.timeline().highlighted()), (Lit::HoveredWorker, by_row(&bars)), "its tickets' bars lit");
    assert_eq!(shell.inspect().workers.hovered.as_deref(), Some("mini-aa1111"));

    // A click: its session in the Details pane, outlined in the table.
    let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&mut shell, &ctx, vec![press(true)]);
    frame_with(&mut shell, &ctx, vec![press(false)]);
    frame(&mut shell, &ctx);
    let start = swarmception_model::Timestamp::parse("2026-10-31T03:00:00Z");
    assert_eq!(shell.ui_state().details.subject, Some(Subject::Session { instance: "mini-aa1111".into(), at: start }));
    assert!(shell.details().details().is_some_and(|d| d.title.contains("worker-1")), "{:?}", shell.details().details().map(|d| &d.title));

    // A model × effort: all its workers' tickets.
    let low = row(&shell, "opus 5.5 · low");
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(egui::pos2(low.left() + 40.0, low.center().y))]);
    frame(&mut shell, &ctx);
    let eleven = shell.engine().call::<TicketBars>(TicketParams { issue: 11 }).unwrap().bars;
    assert_eq!(shell.timeline().highlighted(), by_row(&eleven));

    // The pointer gone: the details' bars have the highlight back; the pane closed, nothing drawn.
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerGone]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.timeline().lit(), Lit::Details);
    shell.run_menu(&ctx, MenuAction::Workers);
    frame(&mut shell, &ctx);
    assert!(!shell.ui_state().panes.workers && shell.workers().drawn().rows.is_empty());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn ui_set_places_the_window_and_a_drag_moves_where_it_is_kept() {
    use swarmception_ui::control::{Answer, Inspect};
    use swarmception_ui::{WorkersPlace, WorkersView};

    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let set = |shell: &mut Shell, params: &str| -> Inspect {
        let Answer::Now(Ok(text)) = shell.answer("ui.set", params) else { panic!("ui.set {params}") };
        serde_json::from_str(&text).unwrap()
    };
    let rect = |shell: &Shell| shell.workers().drawn().rect.expect("the window drawn");
    // By default up from the bottom: its foot near the window's.
    let default = rect(&shell);
    assert!(default.bottom() > 800.0, "{default:?}");
    assert_eq!(shell.ui_state().workers.place, None);

    // Placed and sorted by `ui.set`: drawn there, its corner as asked.
    set(&mut shell, r#"{"workers": {"place": {"x": 60, "y": 90}, "sort": "landed"}}"#);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(rect(&shell).min, egui::pos2(60.0, 90.0));
    let inspect = shell.inspect();
    let drawn = inspect.workers.rect.unwrap();
    assert_eq!((drawn.x, drawn.y, drawn.width, drawn.height), (60, 90, rect(&shell).width().round() as i32, rect(&shell).height().round() as i32));
    assert_eq!(inspect.state.workers, WorkersView { sort: Some(WorkersColumn::Landed), place: Some(WorkersPlace { x: 60, y: 90 }), ..WorkersView::default() });

    // The person drags it by its title: once let go, where it went is the state's place.
    let title = egui::pos2(rect(&shell).left() + 120.0, rect(&shell).top() + 10.0);
    let to = title + egui::vec2(150.0, 70.0);
    let press = |at, pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(title), press(title, true)]);
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(title + egui::vec2(75.0, 35.0))]);
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(to)]);
    assert_eq!(shell.ui_state().workers.place, Some(WorkersPlace { x: 60, y: 90 }), "not while the button is down");
    frame_with(&mut shell, &ctx, vec![press(to, false)]);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(rect(&shell).min, egui::pos2(210.0, 160.0), "it stays where it was let go");
    assert_eq!(shell.ui_state().workers.place, Some(WorkersPlace { x: 210, y: 160 }));

    // Its default place again, then closed: nothing drawn.
    set(&mut shell, r#"{"workers": {"defaultPlace": true}}"#);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert!((rect(&shell).bottom() - default.bottom()).abs() < 1.0, "{:?} back at {default:?}", rect(&shell));
    assert_eq!(shell.ui_state().workers.place, None, "the default isn't kept as a place");
    set(&mut shell, r#"{"workers": {"open": false}}"#);
    frame(&mut shell, &ctx);
    assert_eq!((shell.workers().drawn().rect, shell.inspect().workers.rect), (None, None));
    std::fs::remove_dir_all(dir).ok();
}
