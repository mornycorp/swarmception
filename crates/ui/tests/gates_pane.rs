//! The gate over a real repository's tracking branch. Its runs have no rows on the timeline
//! (#117, reversing #84): a run naming a ticket is the ticket's *in the gate* stretch on its bar,
//! and its story is told from the ticket's (or the Factory's Gate list). The Gates pane (#61)
//! draws `gates.list`'s figures, causes and hours for the timeline's window, lined up under it,
//! and asks again only when the window or the data moves.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swarmception_domain::details::Subject;
use swarmception_domain::gates::{GatesList, Lane};
use swarmception_domain::rows::{BarRef, MERGED_KEY};
use swarmception_domain::{Collapse, Node};
use swarmception_engine::factory_cmds::{TicketBars, TicketParams};
use swarmception_engine::gates_cmds::List;
use swarmception_engine::rows_cmds::{BarParams, GetBar, List as Rows};
use swarmception_engine::stats_cmds::{At, GetParams};
use swarmception_ui::{Lit, MenuAction, Shell};

mod support;

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
    support::until("the tracking branch loaded", || {
        frame(&mut shell, &ctx);
        shell.has_timeline()
    });
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
    assert_eq!(list.runs.len(), 5);
    let pct = |rate: Option<f64>| swarmception_ui::percent(rate);
    assert_eq!(drawn.figures.first().map(String::as_str), Some(format!("gate pass {}", pct(list.gate.pass_rate)).as_str()));
    assert!(drawn.figures.contains(&format!("ready pass {}", swarmception_ui::lane_text(&list.ready))), "{:?}", drawn.figures);
    assert!(drawn.figures.contains(&format!("longest queue {}", list.longest_queue.count)));
    assert_eq!(drawn.causes, ["clippy 0 1", "conflict 1 0"], "the failures by cause, gate and ready");
    assert_eq!(drawn.hours, list.hours.len());
    // Under the timeline: the window's ends where the timeline's are.
    let axis = shell.timeline().axis().expect("the timeline drew");
    assert_eq!(drawn.axis, Some(axis));
    // ui.inspect reports what it drew.
    let inspect = shell.inspect();
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
    let subs: Vec<&str> = narrow.runs.iter().map(|r| r.sub.as_str()).collect();
    assert_eq!(subs, ["gate-8", "gate-10", "gate-8b"], "the runs the window overlaps");
    std::fs::remove_dir_all(dir).ok();
}

/// #117: the timeline has no Gate runs or Ready checks rows (#84's): the merges rows, then the
/// tickets'. A run's time is its ticket's *in the gate* stretch, drawn on the ticket's strip.
#[test]
fn the_timeline_has_no_gate_rows_a_run_is_its_tickets_in_the_gate() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let inspect = shell.inspect();
    assert_eq!(inspect.timeline.in_view.get(..2).map(<[String]>::to_vec), Some(vec!["main".into(), "merged".into()]));
    assert!(!inspect.timeline.in_view.iter().any(|k| k == "gates" || k == "ready"), "{:?}", inspect.timeline.in_view);
    let rows = shell.engine().call::<Rows>(swarmception_engine::engine_cmds::NoParams {}).unwrap().rows;
    assert!(!rows.iter().flat_map(Node::leaves).any(|n| matches!(n.key(), "gates" | "ready")));
    // #7's bar's strip, as the widget draws it: in the gate twice, gate-8's and gate-8b's.
    let seven = shell.engine().call::<TicketBars>(TicketParams { issue: 7 }).unwrap().bars;
    let bar = seven.iter().find(|b| b.track != MERGED_KEY).expect("#7's bar").clone();
    let track = {
        let session = shell.timeline().widget().session();
        let tree = session.call::<timeline_engine::tracks_cmds::Rows>(timeline_engine::engine_cmds::NoParams {}).unwrap();
        tree.rows.iter().any(|r| r.key == bar.track)
    };
    assert!(track, "its row is drawn");
    let gating = bar_stages(&mut shell, &bar).into_iter().filter(|label| label == "in the gate").count();
    assert_eq!(gating, 2, "gate-8's and gate-8b's");
    // Collapsed to the window, the rows come and go as before, the gate's none of them.
    let set = r#"{"collapse": "window", "window": {"start": "2026-10-31T04:05:00Z", "end": "2026-10-31T04:20:00Z"}}"#;
    assert!(matches!(shell.answer("ui.set", set), swarmception_ui::control::Answer::Now(Ok(_))));
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(shell.menu_state().collapse, Collapse::Window);
    assert!(!shell.inspect().timeline.in_view.iter().any(|k| k == "gates" || k == "ready"));
    std::fs::remove_dir_all(dir).ok();
}

/// The labels of `bar`'s strip's segments, as the timeline's data holds them.
fn bar_stages(shell: &mut Shell, bar: &BarRef) -> Vec<String> {
    let found = shell.engine().call::<GetBar>(BarParams { track: bar.track.clone(), range: bar.range }).unwrap().bar;
    found.segments.iter().map(|s| s.stage.name().to_owned()).collect()
}

/// Both lanes' hours (#75): each hour's gate runs and `ready` checks drawn as `gates.list`
/// counts them, mirrored about one baseline, each as tall as its runs on its lane's scale, under
/// its hour on the timeline; `ui.inspect` reads them.
#[test]
fn both_lanes_hours_are_drawn_as_gates_list_counts_them() {
    let dir = repo();
    let (_ctx, mut shell) = loaded(&dir);
    let list = answer(&mut shell);
    let drawn = shell.gates().drawn().clone();
    let per_hour: Vec<(String, usize, usize)> = list.hours.iter().map(|h| (h.start.to_string(), h.gate.runs, h.ready.runs)).collect();
    assert_eq!(per_hour, [("2026-10-31T03:00:00Z".to_owned(), 2, 2), ("2026-10-31T04:00:00Z".to_owned(), 1, 0)]);
    assert_eq!(drawn.hour_columns.len(), 2 * list.hours.len(), "two columns an hour");
    let (x0, x1) = drawn.axis.expect("drawn");
    let span = list.end.0.saturating_sub(list.start.0) as f32;
    let x = |t: i64| x0 + (x1 - x0) * (t.clamp(list.start.0, list.end.0) - list.start.0) as f32 / span;
    let mut units = [Vec::new(), Vec::new()];
    for (hour, pair) in list.hours.iter().zip(drawn.hour_columns.chunks(2)) {
        let [gate, ready] = pair else { panic!("{pair:?}") };
        for (column, lane, figures) in [(gate, Lane::Gate, &hour.gate), (ready, Lane::Ready, &hour.ready)] {
            assert_eq!((column.start, column.lane), (hour.start, lane));
            assert_eq!(
                (column.runs, column.passed, column.failed, column.no_result),
                (figures.runs, figures.passed, figures.failed, figures.no_result),
                "{lane:?} {}",
                hour.start
            );
            // Under its hour on the timeline.
            assert!(
                column.rect.left() >= x(hour.start.0) - 0.5 && column.rect.right() <= x(hour.start.0 + 3_600) + 0.5,
                "{lane:?} {}: {:?}",
                hour.start,
                column.rect
            );
            if figures.runs == 0 {
                assert!(column.rect.height().abs() < 0.01, "none tall with none");
            } else {
                units[usize::from(lane == Lane::Ready)].push(column.rect.height() / figures.runs as f32);
            }
        }
        assert_eq!((gate.rect.left(), gate.rect.right()), (ready.rect.left(), ready.rect.right()), "one column an hour");
        assert!((ready.rect.top() - gate.rect.bottom() - 2.0).abs() < 0.01, "mirrored about the baseline: gate up, ready down");
    }
    for lane in &units {
        assert!(!lane.is_empty() && lane.iter().all(|u| (u - lane[0]).abs() < 0.01 && *u > 1.0), "one scale a lane: {units:?}");
    }
    let inspect = shell.inspect();
    assert_eq!(inspect.gates.hours, ["2026-10-31T03:00:00Z gate 2 ready 2", "2026-10-31T04:00:00Z gate 1 ready 0"]);
    std::fs::remove_dir_all(dir).ok();
}

/// #117: with no run's bar on the timeline, hovering a ticket's bar lights nothing of a run's
/// (#84's `hoveredRun`, gone), a click on it tells the ticket's story, and its gate runs' lines
/// there tell each run's.
#[test]
fn a_tickets_bar_tells_its_story_and_its_gate_runs_lines_theirs() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let seven = shell.bar_point(7).expect("#7's bar drawn");
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(seven)]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.timeline().lit(), Lit::Nothing, "a hover on the timeline lights nothing of a run's");
    let press = |pressed| egui::Event::PointerButton { pos: seven, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&mut shell, &ctx, vec![press(true)]);
    frame_with(&mut shell, &ctx, vec![press(false)]);
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.ui_state().details.subject, Some(Subject::Ticket { issue: 7 }));
    let details = shell.details().details().cloned().expect("#7's story");
    let runs: Vec<Option<Subject>> = details.entries.iter().filter(|e| e.what.starts_with("gate run")).map(|e| e.goes_to.clone()).collect();
    let at = |t: &str| swarmception_model::Timestamp::parse(t);
    assert_eq!(
        runs,
        [
            Some(Subject::Gate { sub: "gate-8".into(), at: at("2026-10-31T03:30:00Z") }),
            Some(Subject::Gate { sub: "gate-8b".into(), at: at("2026-10-31T04:05:00Z") })
        ]
    );
    // A run's story, as its line there goes to it.
    shell.select(Subject::Gate { sub: "gate-8b".into(), at: at("2026-10-31T04:05:00Z") });
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.details().details().map(|d| d.title.as_str()), Some("gate run gate-8b"));
    let lit = shell.timeline().highlighted();
    assert!(!lit.contains_key("gates") && !lit.contains_key("ready"), "{lit:?}");
    assert_eq!(lit, by_row(&shell.details().details().unwrap().timeline.bars.clone()), "its ticket's bars and its merge onto main lit");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn view_gates_shows_and_hides_the_pane() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    assert!(shell.menu_state().gates && shell.gates().drawn().hours > 0, "shown by default");
    shell.run_menu(&ctx, MenuAction::Gates);
    frame(&mut shell, &ctx);
    assert!(!shell.menu_state().gates && !shell.ui_state().panes.gates);
    assert_eq!(shell.gates().drawn().hours, 0, "hidden: nothing drawn");
    assert_eq!(shell.menu_state().checked(MenuAction::Gates), Some(false));
    shell.run_menu(&ctx, MenuAction::Gates);
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert!(shell.gates().drawn().hours > 0);
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
    assert_eq!(shell.gates().drawn().hours, whole.hours.len());
    std::fs::remove_dir_all(dir).ok();
}
