//! The Gates pane (#61) over a real repository's tracking branch: it draws what `gates.list`
//! returns for the timeline's window (AGENTS.md rule 6), lines its runs up under the timeline,
//! asks again only when the window or the data moves, lights a hovered run's tickets' bars on
//! the timeline, and makes a clicked run the Details pane's subject.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swarmception_domain::details::Subject;
use swarmception_domain::gates::{GatesList, Lane};
use swarmception_domain::rows::BarRef;
use swarmception_engine::factory_cmds::{TicketBars, TicketParams};
use swarmception_engine::gates_cmds::List;
use swarmception_engine::stats_cmds::{At, GetParams};
use swarmception_ui::{Lit, MenuAction, Shell};

/// #7 (PR #8) worked on by worker-1, with a failed `ready` check and its rerun; gate-8 sends it
/// back on a conflict, gate-8b merges it; #9 (PR #10) worked on by worker-2 and merged by gate-10;
/// main merges PRs #8 and #10.
fn repo() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-gates-pane-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [
        (
            "mini-aa1111",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:02:00Z"}"#,
                r#"{"event":"sub.start","name":"ready","sub":"ready-abc","t":"2026-10-31T03:15:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=1; cause=clippy","sub":"ready-abc","t":"2026-10-31T03:18:00Z"}"#,
                r#"{"event":"sub.start","name":"ready","sub":"ready-abc-rerun","t":"2026-10-31T03:20:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0","sub":"ready-abc-rerun","t":"2026-10-31T03:25:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:26:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:50:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T04:00:00Z"}"#,
            ],
        ),
        (
            "mini-bb2222",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-2","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":9,"mr":10,"t":"2026-10-31T03:05:00Z"}"#,
                r#"{"event":"work","issue":9,"t":"2026-10-31T03:06:00Z"}"#,
                r#"{"event":"submit","issue":9,"mr":10,"t":"2026-10-31T03:40:00Z"}"#,
            ],
        ),
        (
            "mini-cc3333",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7); kind=smoke","sub":"gate-8","t":"2026-10-31T03:30:00Z"}"#,
                r#"{"event":"sendback","issue":7,"mr":8,"note":"cause=conflict","sub":"gate-8","t":"2026-10-31T03:40:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=1; cause=conflict","sub":"gate-8","t":"2026-10-31T03:41:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #10 (#9); kind=smoke","sub":"gate-10","t":"2026-10-31T03:45:00Z"}"#,
                r#"{"event":"merge","issue":9,"mr":10,"sub":"gate-10","t":"2026-10-31T03:52:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0; merged #10","sub":"gate-10","t":"2026-10-31T03:53:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7); kind=smoke","sub":"gate-8b","t":"2026-10-31T04:05:00Z"}"#,
                r#"{"event":"merge","issue":7,"mr":8,"sub":"gate-8b","t":"2026-10-31T04:10:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0; merged #8","sub":"gate-8b","t":"2026-10-31T04:11:00Z"}"#,
            ],
        ),
    ];
    for (instance, lines) in files {
        std::fs::create_dir_all(dir.join("tracking").join(instance)).unwrap();
        std::fs::write(dir.join("tracking").join(instance).join("work.jsonl"), lines.join("\n") + "\n").unwrap();
    }
    let git = |at: &str, args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(&dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_AUTHOR_DATE", at)
            .env("GIT_COMMITTER_DATE", at)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    let early = "2026-10-31T02:00:00Z";
    git(early, &["init", "-q", "-b", "tracking"]);
    git(early, &["add", "."]);
    git(early, &["commit", "-q", "-m", "events"]);
    git(early, &["checkout", "-q", "--orphan", "main"]);
    git(early, &["rm", "-rqf", "."]);
    git(early, &["commit", "-q", "--allow-empty", "-m", "start"]);
    for (pr, at) in [(10, "2026-10-31T03:52:00Z"), (8, "2026-10-31T04:10:00Z")] {
        git(early, &["checkout", "-q", "-b", &format!("pr-{pr}"), "main"]);
        git(at, &["commit", "-q", "--allow-empty", "-m", "work"]);
        git(early, &["checkout", "-q", "main"]);
        git(at, &["merge", "-q", "--no-ff", "-m", &format!("Merge pull request #{pr} from mornycorp/pr-{pr}"), &format!("pr-{pr}")]);
    }
    git(early, &["checkout", "-q", "tracking"]);
    dir
}

/// One frame of the shell, 1400 × 900 points, with `events`.
fn frame_with(shell: &mut Shell, ctx: &egui::Context, events: Vec<egui::Event>) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
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
        if shell.has_timeline() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(shell.has_timeline(), "the tracking branch loaded");
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    (ctx, shell)
}

/// What `gates.list` returns for the timeline's window now.
fn answer(shell: &mut Shell) -> GatesList {
    let window = shell.inspect().timeline.window.expect("the timeline's window");
    let params = GetParams { start: Some(At::Seconds(window.start.0)), end: Some(At::Seconds(window.end.0)) };
    shell.engine().call::<List>(params).unwrap()
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
fn the_pane_draws_what_gates_list_returns_under_the_timeline() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let list = answer(&mut shell);
    assert_eq!(shell.gates().list(), Some(&list), "the pane holds what gates.list returns for the timeline's window");
    let drawn = shell.gates().drawn().clone();
    assert_eq!(drawn.window, Some((list.start, list.end)));
    // The coordinator's lane, then the workers': each in `gates.list`'s order.
    let lanes = || list.runs.iter().filter(|r| r.lane == Lane::Gate).chain(list.runs.iter().filter(|r| r.lane == Lane::Ready));
    let runs: Vec<(String, Lane)> = lanes().map(|r| (r.sub.clone(), r.lane)).collect();
    assert_eq!(drawn.runs.iter().map(|r| (r.sub.clone(), r.lane)).collect::<Vec<_>>(), runs, "a bar a run, in its lane");
    assert_eq!(runs.len(), 5);
    let pct = |rate: Option<f64>| swarmception_ui::percent(rate);
    assert_eq!(drawn.figures.first().map(String::as_str), Some(format!("gate pass {}", pct(list.gate.pass_rate)).as_str()));
    assert!(drawn.figures.contains(&format!("ready pass {}", swarmception_ui::lane_text(&list.ready))), "{:?}", drawn.figures);
    assert!(drawn.figures.contains(&format!("longest queue {}", list.longest_queue.count)));
    assert_eq!(drawn.causes, ["clippy 0 1", "conflict 1 0"], "the failures by cause, gate and ready");
    assert_eq!(drawn.hours, list.hours.len());
    // Under the timeline: the window's ends where the timeline's are, and each run as long as it took.
    let axis = shell.timeline().axis().expect("the timeline drew");
    assert_eq!(drawn.axis, Some(axis));
    let (x0, x1) = axis;
    let span = list.end.0.saturating_sub(list.start.0) as f32;
    for (run, bar) in lanes().zip(&drawn.runs) {
        let x = x0 + (x1 - x0) * run.start.0.saturating_sub(list.start.0) as f32 / span;
        assert!((bar.rect.left() - x).abs() < 1.0, "{}: drawn from {} where its start is {x}", run.sub, bar.rect.left());
        let width = (x1 - x0) * run.duration_seconds as f32 / span;
        assert!((bar.rect.width() - width.max(2.0)).abs() < 1.0, "{}: {} wide for {width}", run.sub, bar.rect.width());
    }
    // ui.inspect reports what it drew.
    let inspect = shell.inspect();
    assert_eq!(inspect.gates.runs, lanes().map(|r| format!("{}@{}", r.sub, r.start)).collect::<Vec<_>>());
    assert_eq!((inspect.gates.figures, inspect.gates.causes), (drawn.figures.clone(), drawn.causes.clone()));
    assert!(inspect.state.panes.gates);

    // Asked again only when the window moves.
    let asks = shell.gates().asks();
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.gates().asks(), asks, "nothing moved: not asked again");
    let set = r#"{"window": {"start": "2026-10-31T03:35:00Z", "end": "2026-10-31T04:06:00Z"}}"#;
    assert!(matches!(shell.answer("ui.set", set), swarmception_ui::control::Answer::Now(Ok(_))));
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert!(shell.gates().asks() > asks);
    let narrow = answer(&mut shell);
    assert_eq!(shell.gates().list(), Some(&narrow));
    let subs: Vec<&str> = shell.gates().drawn().runs.iter().map(|r| r.sub.as_str()).collect();
    assert_eq!(subs, ["gate-8", "gate-10", "gate-8b"], "the runs the window overlaps");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn hovering_a_run_lights_its_tickets_and_a_click_tells_its_story() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let rect = |shell: &Shell, sub: &str| shell.gates().drawn().runs.iter().find(|r| r.sub == sub).map(|r| r.rect).unwrap();
    let eight = rect(&shell, "gate-8b");
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(eight.center())]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.gates().drawn().hovered.as_ref().map(|(sub, _)| sub.as_str()), Some("gate-8b"));
    let seven = shell.engine().call::<TicketBars>(TicketParams { issue: 7 }).unwrap().bars;
    assert_eq!((shell.timeline().lit(), shell.timeline().highlighted()), (Lit::HoveredRun, by_row(&seven)), "its ticket's bars lit");
    assert_eq!(shell.inspect().gates.hovered.as_deref().map(|h| h.starts_with("gate-8b@")), Some(true));

    // A click: the gate run in the Details pane.
    let press = |pressed| egui::Event::PointerButton { pos: eight.center(), button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&mut shell, &ctx, vec![press(true)]);
    frame_with(&mut shell, &ctx, vec![press(false)]);
    frame(&mut shell, &ctx);
    let start = swarmception_model::Timestamp::parse("2026-10-31T04:05:00Z");
    assert_eq!(shell.ui_state().details.subject, Some(Subject::Gate { sub: "gate-8b".into(), at: start }));
    assert_eq!(shell.details().details().map(|d| d.title.as_str()), Some("gate run gate-8b"));

    // A ready check: its ticket's story.
    let ready = rect(&shell, "ready-abc");
    let click = |pressed| egui::Event::PointerButton { pos: ready.center(), button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(ready.center()), click(true)]);
    frame_with(&mut shell, &ctx, vec![click(false)]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.ui_state().details.subject, Some(Subject::Ticket { issue: 7 }));

    // The pointer gone: the details' bars have the highlight back.
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerGone]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.timeline().lit(), Lit::Details);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn view_gates_shows_and_hides_the_pane() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    assert!(shell.menu_state().gates && !shell.gates().drawn().runs.is_empty(), "shown by default");
    shell.run_menu(&ctx, MenuAction::Gates);
    frame(&mut shell, &ctx);
    assert!(!shell.menu_state().gates && !shell.ui_state().panes.gates);
    assert!(shell.gates().drawn().runs.is_empty(), "hidden: nothing drawn");
    assert_eq!(shell.menu_state().checked(MenuAction::Gates), Some(false));
    shell.run_menu(&ctx, MenuAction::Gates);
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert!(!shell.gates().drawn().runs.is_empty());
    // Its height the window's default (#68): no drag, so none of the person's.
    assert_eq!(shell.ui_state().sizes.gates, None);
    let gates = shell.layout().bands.iter().find(|b| b.name == "gates").cloned().unwrap();
    assert!(gates.size >= 90 && !gates.folded, "{gates:?}");

    // Alone, without the timeline: the whole run, on its own axis.
    let mut state = shell.ui_state();
    state.panes = swarmception_ui::Panes { timeline: false, factory: false, details: false, gates: true, workers: false };
    shell.set_ui_state(state);
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    let whole = shell.engine().call::<List>(GetParams::default()).unwrap();
    assert_eq!(shell.gates().list(), Some(&whole));
    assert_eq!(shell.gates().drawn().runs.len(), whole.runs.len());
    std::fs::remove_dir_all(dir).ok();
}
