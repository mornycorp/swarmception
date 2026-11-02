//! Folding the timeline's quiet time (#86) over a real repository's tracking branch: two bursts
//! of work hours apart. Fold Quiet Time (View ▸, the toolbar's checkbox, `ui.set`'s `foldQuiet`)
//! has the widget fold the stretch where nothing happened, by the swarm's own quiet gap (15 min)
//! unless `foldAfterSeconds` says; it holds across a reload and is kept in the saved state; and the
//! Gates band's hours line up under the timeline's x through the fold.

use std::path::{Path, PathBuf};

use swarmception_domain::rows::GATES_KEY;
use swarmception_domain::{Collapse, Node};
use swarmception_engine::rows_cmds::List as Rows;
use swarmception_model::Timestamp;
use swarmception_ui::control::Answer;
use swarmception_ui::{MenuAction, Shell, UiState};
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::fold_cmds::QueryFolds;
use timeline_engine::query_cmds::{Bars, BarsParams};

/// Worker-1 works on #7 (PR #8) from 03:01, gate-8 merges it at 03:28; nothing then until
/// worker-2 claims #9 (PR #10) at 09:01, gate-10 merging it at 09:28.
fn repo() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-fold-quiet-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [
        (
            "mini-aa1111",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:02:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:20:00Z"}"#,
                r#"{"event":"session.stop","t":"2026-10-31T03:30:00Z"}"#,
            ],
        ),
        (
            "mini-bb2222",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-2","role":"worker","t":"2026-10-31T09:00:00Z"}"#,
                r#"{"event":"claim","issue":9,"mr":10,"t":"2026-10-31T09:01:00Z"}"#,
                r#"{"event":"work","issue":9,"t":"2026-10-31T09:02:00Z"}"#,
                r#"{"event":"submit","issue":9,"mr":10,"t":"2026-10-31T09:20:00Z"}"#,
            ],
        ),
        (
            "mini-cc3333",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7); kind=smoke","sub":"gate-8","t":"2026-10-31T03:25:00Z"}"#,
                r#"{"event":"merge","issue":7,"mr":8,"sub":"gate-8","t":"2026-10-31T03:28:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0; merged #8","sub":"gate-8","t":"2026-10-31T03:29:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #10 (#9); kind=smoke","sub":"gate-10","t":"2026-10-31T09:25:00Z"}"#,
                r#"{"event":"merge","issue":9,"mr":10,"sub":"gate-10","t":"2026-10-31T09:28:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0; merged #10","sub":"gate-10","t":"2026-10-31T09:29:00Z"}"#,
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
    for (pr, at) in [(8, "2026-10-31T03:28:00Z"), (10, "2026-10-31T09:28:00Z")] {
        git(early, &["checkout", "-q", "-b", &format!("pr-{pr}"), "main"]);
        git(at, &["commit", "-q", "--allow-empty", "-m", "work"]);
        git(early, &["checkout", "-q", "main"]);
        git(at, &["merge", "-q", "--no-ff", "-m", &format!("Merge pull request #{pr} from mornycorp/pr-{pr}"), &format!("pr-{pr}")]);
    }
    git(early, &["checkout", "-q", "tracking"]);
    dir
}

fn frame(shell: &mut Shell, ctx: &egui::Context) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
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

fn set(shell: &mut Shell, ctx: &egui::Context, params: &str) {
    assert!(matches!(shell.answer("ui.set", params), Answer::Now(Ok(_))), "{params}");
    for _ in 0..3 {
        frame(shell, ctx);
    }
}

fn t(text: &str) -> Timestamp {
    Timestamp::parse(text).unwrap()
}

/// Off by default; on, the stretch from the first gate run's end to #9's claim is folded, by the
/// swarm's own quiet gap; View ▸ Fold Quiet Time turns it off again, and the threshold is kept.
#[test]
fn fold_quiet_time_folds_the_stretch_where_nothing_happened() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let before = shell.inspect();
    assert!(!before.state.fold.on && before.timeline.folds.is_empty(), "off by default");
    assert!(!shell.menu_state().fold_quiet);

    set(&mut shell, &ctx, r#"{"foldQuiet": true}"#);
    let inspect = shell.inspect();
    assert!(inspect.state.fold.on);
    assert_eq!(inspect.version, 12);
    let folds = inspect.timeline.folds.clone();
    assert_eq!(folds.len(), 1, "{folds:?}");
    assert_eq!((folds[0].start, folds[0].end), (t("2026-10-31T03:29:00Z"), t("2026-10-31T09:01:00Z")), "{folds:?}");
    assert_eq!(inspect.timeline.window, before.timeline.window, "the window stays the same span of time");
    assert!(shell.menu_state().fold_quiet);
    // The widget folds it, its marker 12 px wide.
    let drawn = shell.timeline().widget().session().call::<QueryFolds>(NoParams {}).unwrap();
    assert_eq!(drawn.folds.len(), 1);
    assert!((drawn.folds[0].x1 - drawn.folds[0].x0 - 12.0).abs() < 0.01, "{:?}", drawn.folds);
    assert!(drawn.folds[0].lines[0].starts_with("quiet 5 h 32 min"), "{:?}", drawn.folds[0].lines);

    // A threshold over the stretch folds nothing; kept while off and on again.
    set(&mut shell, &ctx, r#"{"foldAfterSeconds": 21600}"#);
    assert!(shell.inspect().timeline.folds.is_empty());
    shell.run_menu(&ctx, MenuAction::FoldQuiet);
    frame(&mut shell, &ctx);
    let off = shell.inspect();
    assert!(!off.state.fold.on && off.timeline.folds.is_empty());
    assert_eq!(off.state.fold.after_seconds, Some(21_600));
    set(&mut shell, &ctx, r#"{"foldQuiet": true, "foldAfterSeconds": 900}"#);
    assert_eq!(shell.inspect().timeline.folds.len(), 1);
    // Out of range: refused, nothing changed.
    assert!(matches!(shell.answer("ui.set", r#"{"foldAfterSeconds": 0}"#), Answer::Now(Err(_))));
    assert_eq!(shell.inspect().state.fold.after_seconds, Some(900));
    std::fs::remove_dir_all(dir).ok();
}

/// Folding and collapse compose (#74): collapsed to the window and folded, both hold; a reload
/// (the rows built again, the widget loaded anew) keeps the fold.
#[test]
fn folding_holds_with_collapse_and_across_a_reload() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    set(&mut shell, &ctx, r#"{"foldQuiet": true, "collapse": "window"}"#);
    let inspect = shell.inspect();
    assert_eq!(shell.menu_state().collapse, Collapse::Window);
    assert_eq!(inspect.timeline.folds.len(), 1);
    // The rows built again: grouped by worker, then flat, each a fresh load into the widget.
    shell.run_menu(&ctx, MenuAction::Grouping(swarmception_domain::Grouping::Workers));
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(shell.inspect().timeline.folds.len(), 1, "a load folds again");
    shell.run_menu(&ctx, MenuAction::Collapse(Collapse::Window));
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(shell.inspect().timeline.folds.len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

/// The Gates band's hours sit under the timeline's x for the same time through the fold: each
/// gate run's bar on the timeline's Gate runs row lies over its hour's column in the band.
#[test]
fn the_gates_hours_line_up_under_the_timeline_through_the_fold() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    set(&mut shell, &ctx, r#"{"foldQuiet": true}"#);
    let drawn = shell.gates().drawn().clone();
    let (x0, _) = drawn.axis.expect("the band lines up under the timeline");
    let bars = shell.timeline().widget().session().call::<Bars>(BarsParams { tracks: Some(vec![GATES_KEY.into()]), label_char_width: None }).unwrap();
    let runs: Vec<(f32, f32)> = bars.tracks[0].ranges.iter().map(|r| (x0 + r.x, x0 + r.x + r.width)).collect();
    assert_eq!(runs.len(), 2, "gate-8 and gate-10");
    let hours: Vec<(Timestamp, egui::Rect)> = drawn.hour_columns.iter().step_by(2).map(|c| (c.start, c.rect)).collect();
    let rows = shell.engine().call::<Rows>(swarmception_engine::engine_cmds::NoParams {}).unwrap().rows;
    let gates = rows.iter().find_map(|n| match n {
        Node::Ranges { key, bars, .. } if key == GATES_KEY => Some(bars.clone()),
        _ => None,
    });
    let gates = gates.expect("the Gate runs row");
    for (bar, (left, right)) in gates.iter().zip(&runs) {
        let hour = Timestamp(bar.start.0 - bar.start.0.rem_euclid(3_600));
        let (_, column) = hours.iter().find(|(start, _)| *start == hour).unwrap_or_else(|| panic!("no column for {hour}: {hours:?}"));
        // The run's bar on the timeline, over its hour's column in the band (the column's inset
        // a few points either side).
        assert!(column.left() - 8.0 <= *left && *right <= column.right() + 8.0, "{hour}: bar {left}..{right}, column {column:?}");
    }
    // Folded, the second hour's column starts just past the fold's marker, not where 09:00
    // falls on an even axis (most of the width).
    let (x0, x1) = drawn.axis.unwrap();
    let late = hours.iter().find(|(start, _)| *start == t("2026-10-31T09:00:00Z")).unwrap().1;
    let even = x0 + (x1 - x0) * (6.0 * 3_600.0) / (6.5 * 3_600.0);
    assert!(late.left() < even - 200.0, "{late:?} vs an even {even}");
    std::fs::remove_dir_all(dir).ok();
}

/// The fold is the shell's state: kept as UiState version 10, an older state reading as off.
#[test]
fn folding_is_kept_in_the_shells_state() {
    assert_eq!(swarmception_ui::UI_STATE_VERSION, 10);
    let older: UiState = serde_json::from_str(r#"{"version": 9}"#).unwrap();
    assert!(!older.fold.on && older.fold.after_seconds.is_none());
    let on = UiState { fold: swarmception_ui::FoldView { on: true, after_seconds: Some(1_200) }, ..UiState::default() };
    let text = serde_json::to_string(&on).unwrap();
    assert!(text.contains(r#""fold":{"on":true,"afterSeconds":1200}"#), "{text}");
    assert_eq!(serde_json::from_str::<UiState>(&text).unwrap(), on);
    assert_eq!(on.fold.threshold(), Some(1_200));
    assert_eq!(UiState::default().fold.threshold(), None);
    assert_eq!(swarmception_ui::FoldView { on: true, after_seconds: None }.threshold(), Some(15 * 60), "stats.get's quietAfterSeconds");
}
