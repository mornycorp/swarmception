//! The gate over a real repository's tracking branch. Its runs are rows of the timeline (#84):
//! the Gate runs and Ready checks rows draw `gates.list`'s runs for the window (AGENTS.md rule 6),
//! a hovered run lights its tickets' bars and a clicked one is the Details pane's subject. The
//! Gates pane (#61) keeps what isn't a lane: it draws `gates.list`'s figures, causes and hours for
//! the timeline's window, lined up under it, and asks again only when the window or the data
//! moves.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swarmception_domain::details::Subject;
use swarmception_domain::gates::{GatesList, Lane};
use swarmception_domain::rows::{BarRef, GATES_KEY, READY_KEY};
use swarmception_domain::{Collapse, Node};
use swarmception_engine::factory_cmds::{TicketBars, TicketParams};
use swarmception_engine::gates_cmds::List;
use swarmception_engine::rows_cmds::List as Rows;
use swarmception_engine::stats_cmds::{At, GetParams};
use swarmception_ui::{Lit, MenuAction, Shell};
use timeline_engine::query_cmds::{Bars, BarsParams};

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
    assert_eq!(drawn_runs(&mut shell, &narrow), runs_of(&narrow), "the timeline's rows draw them");
    std::fs::remove_dir_all(dir).ok();
}

/// Each run as `gates.list` gives it: its row, id, start, end, colour and tooltip.
type RunAt = (String, String, swarmception_model::Timestamp, swarmception_model::Timestamp, [u8; 3], Vec<String>);

fn runs_of(list: &GatesList) -> Vec<RunAt> {
    let mut runs = Vec::new();
    for (lane, key) in [(Lane::Gate, GATES_KEY), (Lane::Ready, READY_KEY)] {
        runs.extend(list.runs.iter().filter(|r| r.lane == lane).map(|r| {
            let end = r.end.unwrap_or(swarmception_model::Timestamp(r.start.0 + r.duration_seconds));
            (key.to_owned(), r.sub.clone(), r.start, end, r.colour, r.tooltip.iter().map(|l| l.text()).collect())
        }));
    }
    runs
}

/// The bars of the timeline's Gate runs and Ready checks rows (`rows.list`) in `list`'s window.
fn drawn_runs(shell: &mut Shell, list: &GatesList) -> Vec<RunAt> {
    let rows = shell.engine().call::<Rows>(swarmception_engine::engine_cmds::NoParams {}).unwrap().rows;
    let mut drawn = Vec::new();
    for key in [GATES_KEY, READY_KEY] {
        let bars = rows.iter().find_map(|n| match n {
            Node::Ranges { key: k, bars, .. } if k == key => Some(bars.clone()),
            _ => None,
        });
        for bar in bars.unwrap_or_default().into_iter().filter(|b| b.start <= list.end && b.end >= list.start) {
            let sub = bar.run.as_ref().map(|r| r.sub.clone()).unwrap_or_default();
            drawn.push((key.to_owned(), sub, bar.start, bar.end, bar.colour, bar.tooltip.clone()));
        }
    }
    drawn
}

/// #84: the timeline gains the Gate runs and Ready checks rows under the merges rows, and they
/// draw `gates.list`'s runs for its window: the widget draws a bar a run, in its result's colour,
/// labelled by its tickets (a failure's cause too), and collapsed to the window they come and go
/// as any row.
#[test]
fn the_timelines_gate_rows_draw_gates_lists_runs_for_its_window() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let list = answer(&mut shell);
    assert_eq!(drawn_runs(&mut shell, &list), runs_of(&list));
    let inspect = shell.inspect();
    assert_eq!(inspect.timeline.in_view.get(..4).map(<[String]>::to_vec), Some(vec!["main".into(), "merged".into(), GATES_KEY.into(), READY_KEY.into()]));
    // The widget draws them: a bar a run, labelled.
    let drawn = shell.timeline().widget().session().call::<Bars>(BarsParams { tracks: Some(vec![GATES_KEY.into(), READY_KEY.into()]), label_char_width: Some(7.0) }).unwrap();
    let ranges: Vec<usize> = drawn.tracks.iter().map(|t| t.ranges.iter().map(|r| r.count.max(1)).sum()).collect();
    assert_eq!(ranges, [3, 2], "gate-8, gate-10, gate-8b; ready-abc and its rerun");
    // Collapsed to the window: 04:30 to 05:00 holds no ready check, so that row goes.
    let set = r#"{"collapse": "window", "window": {"start": "2026-10-31T04:05:00Z", "end": "2026-10-31T04:20:00Z"}}"#;
    assert!(matches!(shell.answer("ui.set", set), swarmception_ui::control::Answer::Now(Ok(_))));
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    let inspect = shell.inspect();
    assert_eq!(shell.menu_state().collapse, Collapse::Window);
    assert!(inspect.timeline.left_out.iter().any(|k| k == READY_KEY), "{:?}", inspect.timeline.left_out);
    assert!(!inspect.timeline.left_out.iter().any(|k| k == GATES_KEY), "gate-8b ran then: {:?}", inspect.timeline.left_out);
    let narrow = answer(&mut shell);
    assert_eq!(drawn_runs(&mut shell, &narrow), runs_of(&narrow));
    std::fs::remove_dir_all(dir).ok();
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

/// #84: hovering a run's bar on the timeline lights its tickets' bars; a click on it selects it
/// and the Details pane tells its story (a ready check's, its ticket's).
#[test]
fn hovering_a_run_on_the_timeline_lights_its_tickets_and_a_click_tells_its_story() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let eight = shell.run_point("gate-8b").expect("gate-8b's bar drawn");
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(eight)]);
    frame(&mut shell, &ctx);
    let hovered = shell.inspect().timeline.hovered_run;
    assert_eq!(hovered.as_deref().map(|h| h.starts_with("gate-8b@")), Some(true), "{hovered:?}");
    let seven = shell.engine().call::<TicketBars>(TicketParams { issue: 7 }).unwrap().bars;
    assert_eq!((shell.timeline().lit(), shell.timeline().highlighted()), (Lit::HoveredRun, by_row(&seven)), "its ticket's bars lit");

    // A click: the gate run selected, and in the Details pane.
    let press = |pressed| egui::Event::PointerButton { pos: eight, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&mut shell, &ctx, vec![press(true)]);
    frame_with(&mut shell, &ctx, vec![press(false)]);
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    let start = swarmception_model::Timestamp::parse("2026-10-31T04:05:00Z");
    assert_eq!(shell.ui_state().details.subject, Some(Subject::Gate { sub: "gate-8b".into(), at: start }));
    assert_eq!(shell.details().details().map(|d| d.title.as_str()), Some("gate run gate-8b"));
    assert_eq!(shell.inspect().timeline.selected.get(GATES_KEY), Some(&vec![2]), "its bar selected");

    // A ready check: its ticket's story.
    let ready = shell.run_point("ready-abc").expect("ready-abc's bar drawn");
    let click = |pressed| egui::Event::PointerButton { pos: ready, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(ready), click(true)]);
    frame_with(&mut shell, &ctx, vec![click(false)]);
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.ui_state().details.subject, Some(Subject::Ticket { issue: 7 }));

    // The pointer gone: the details' bars have the highlight back.
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerGone]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.timeline().lit(), Lit::Details);
    assert_eq!(shell.inspect().timeline.hovered_run, None);
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
