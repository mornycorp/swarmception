//! The factory's Gate list with the workers' ready checks (#95): told apart from gate runs, in
//! time order with them, what `factory.at` returns; a ready check's bar chosen on the timeline
//! selects its line, a click on its line tells its story and lights its bar; and a run selected
//! that ends or starts after the factory's *t* is drawn selected, running or not yet run.

use std::path::PathBuf;

use swarmception_domain::details::Subject;
use swarmception_domain::factory::{GateRun, Outcome};
use swarmception_domain::gates::Lane;
use swarmception_engine::factory_cmds::{At, AtParams};
use swarmception_model::Timestamp;
use swarmception_ui::{FactorySelected, Shell};

mod support;

/// worker-1 claims #7 (PR #8): a ready check fails (03:20–03:22, `cause=test`), a second passes
/// (03:25–03:26), it submits at 03:30; the coordinator's gate-pr8 runs 03:40–03:46 and merges it.
fn repo() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "swarmception-gate-list-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let files = [
        (
            "mini-aa1111",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
                r#"{"event":"work","issue":7,"mr":8,"t":"2026-10-31T03:02:00Z"}"#,
                r#"{"event":"sub.start","name":"ready","sub":"ready-a","t":"2026-10-31T03:20:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=1; cause=test; step=test","sub":"ready-a","t":"2026-10-31T03:22:00Z"}"#,
                r#"{"event":"sub.start","name":"ready","sub":"ready-b","t":"2026-10-31T03:25:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0","sub":"ready-b","t":"2026-10-31T03:26:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:30:00Z"}"#,
            ],
        ),
        (
            "mini-cc3333",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7)","sub":"gate-pr8","t":"2026-10-31T03:40:00Z"}"#,
                r#"{"event":"merge","issue":7,"mr":8,"sub":"gate-pr8","t":"2026-10-31T03:45:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0; merged #8","sub":"gate-pr8","t":"2026-10-31T03:46:00Z"}"#,
                r#"{"event":"session.stop","t":"2026-10-31T04:00:00Z"}"#,
            ],
        ),
    ];
    for (instance, lines) in files {
        std::fs::create_dir_all(dir.join("tracking").join(instance)).unwrap();
        std::fs::write(dir.join("tracking").join(instance).join("work.jsonl"), lines.join("\n") + "\n").unwrap();
    }
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(&dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_AUTHOR_DATE", "2026-10-31T05:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-10-31T05:00:00Z")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    run(&["init", "-q", "-b", "tracking"]);
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "events"]);
    dir
}

fn at(text: &str) -> Timestamp {
    Timestamp::parse(text).unwrap()
}

/// One frame with `events`, 1400 × 900: the text it drew.
fn frame_with(shell: &mut Shell, ctx: &egui::Context, events: Vec<egui::Event>) -> Vec<String> {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn frames(shell: &mut Shell, ctx: &egui::Context) -> Vec<String> {
    let mut texts = Vec::new();
    for _ in 0..3 {
        texts = frame_with(shell, ctx, Vec::new());
    }
    texts
}

fn loaded(dir: &std::path::Path, details: bool) -> (egui::Context, Shell) {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, Some(dir));
    let mut state = shell.ui_state();
    state.panes.details = details;
    shell.set_ui_state(state);
    support::until("the tracking branch loaded", || {
        frame_with(&mut shell, &ctx, Vec::new());
        shell.has_timeline()
    });
    assert!(shell.has_timeline(), "the tracking branch loaded");
    (ctx, shell)
}

#[test]
fn the_gate_list_holds_the_ready_checks_in_time_order_with_its_runs() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir, false);
    let t = at("2026-10-31T03:50:00Z");
    shell.set_factory_at(t);
    let texts = frames(&mut shell, &ctx);

    let shown = shell.factory().scene().cloned().unwrap();
    let asked = shell.engine().call::<At>(AtParams { t: Some(t), recent: Some(shown.recent), ticket: None, run: None }).unwrap();
    assert_eq!(shown, asked);
    /// A check as the test reads it: its id, lane, outcome, worker and cause.
    type Check<'a> = (&'a str, Lane, Outcome, Option<&'a str>, Option<&'a str>);
    let checks: Vec<Check<'_>> = asked.gate.checks.iter().map(|c| (c.sub.as_str(), c.lane, c.outcome, c.worker.as_deref(), c.cause.as_deref())).collect();
    assert_eq!(
        checks,
        [("ready-b", Lane::Ready, Outcome::Passed, Some("worker-1"), None), ("ready-a", Lane::Ready, Outcome::Failed, Some("worker-1"), Some("test"))],
        "the last ended first; the cause as a gate run's"
    );
    assert!(asked.gate.recent.iter().all(|r: &GateRun| r.lane == Lane::Gate));
    assert_eq!((asked.gate.passed, asked.gate.failed), (1, 0), "the gate's own counts");

    // In time order with the gate's runs: the last ended first.
    let drawn = shell.factory().drawn().clone();
    assert_eq!(drawn.runs, ["gate-pr8", "ready-b", "ready-a"]);
    assert_eq!(drawn.run_lines.len(), 3);
    // A check reads as its worker's, its ticket and its cause; a run by its id.
    assert!(texts.iter().any(|text| text == "worker-1  #7  test"), "{texts:?}");
    assert!(texts.iter().any(|text| text == "gate-pr8  #7"), "{texts:?}");
    assert!(texts.iter().any(|text| *text == swarmception_ui::icons::FLASK.to_string()), "the ready checks' mark: {texts:?}");

    // While the second runs, it is first, running.
    shell.set_factory_at(at("2026-10-31T03:25:30Z"));
    frames(&mut shell, &ctx);
    assert_eq!(shell.factory().drawn().runs, ["ready-b", "ready-a"]);
    assert_eq!(shell.factory().scene().unwrap().gate.checks[0].outcome, Outcome::Running);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_click_on_a_ready_checks_line_selects_it_and_tells_its_story() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir, true);
    shell.set_factory_at(at("2026-10-31T03:50:00Z"));
    frames(&mut shell, &ctx);

    // #117: no ready check has a bar on the timeline to select its line (#95's, gone); a check
    // chosen in the details still selects its line.
    let ready_a = Subject::Gate { sub: "ready-a".into(), at: Some(at("2026-10-31T03:20:00Z")) };
    shell.select(ready_a.clone());
    frames(&mut shell, &ctx);
    assert_eq!(shell.ui_state().details.subject, Some(ready_a));
    assert_eq!(shell.factory().selected(), Some(&FactorySelected::Run { sub: "ready-a".into(), at: Some(at("2026-10-31T03:20:00Z")) }));
    assert_eq!(shell.factory().drawn().selected, ["run ready-a"]);
    assert_eq!(shell.ui_state().factory.ticket, None, "a check traces no ticket");
    let inspect = shell.inspect().factory;
    assert_eq!(inspect.drawn.selected, ["run ready-a"]);

    // The list → the details: a click on ready-b's line.
    let drawn = shell.factory().drawn().clone();
    let line = drawn.runs.iter().position(|sub| sub == "ready-b").map(|n| drawn.run_lines[n]).unwrap();
    let on = line.center();
    let button = |pressed| egui::Event::PointerButton { pos: on, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(on), button(true)]);
    frame_with(&mut shell, &ctx, vec![button(false)]);
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerGone]);
    frames(&mut shell, &ctx);
    let ready_b = Subject::Gate { sub: "ready-b".into(), at: Some(at("2026-10-31T03:25:00Z")) };
    assert_eq!(shell.ui_state().details.subject, Some(ready_b));
    assert_eq!(shell.factory().drawn().selected, ["run ready-b"]);
    let details = shell.details().details().cloned().unwrap();
    assert_eq!((details.mark.as_str(), details.title.as_str()), ("ready", "ready check ready-b"));
    // No bar of its own lit on the timeline: the gate's rows are gone (#117).
    let lit = shell.timeline().highlighted();
    assert!(!lit.contains_key("ready") && !lit.contains_key("gates"), "{lit:?}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_run_selected_is_drawn_selected_though_it_ends_or_starts_after_t() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir, false);
    let gate = Subject::Gate { sub: "gate-pr8".into(), at: Some(at("2026-10-31T03:40:00Z")) };
    shell.select(gate);

    // 03:43: it runs on past t, selected.
    shell.set_factory_at(at("2026-10-31T03:43:00Z"));
    frames(&mut shell, &ctx);
    assert_eq!(shell.factory().drawn().runs.first().map(String::as_str), Some("gate-pr8"));
    assert_eq!(shell.factory().drawn().selected, ["run gate-pr8"]);

    // 03:35: it hasn't begun; not yet run, selected, first in the list.
    shell.set_factory_at(at("2026-10-31T03:35:00Z"));
    let texts = frames(&mut shell, &ctx);
    assert_eq!(shell.factory().drawn().selected, ["run gate-pr8"]);
    assert!(!shell.factory().drawn().runs.contains(&"gate-pr8".to_owned()), "nothing after t told");
    assert!(texts.iter().any(|text| text == "gate-pr8  not yet run"), "{texts:?}");
    std::fs::remove_dir_all(dir).ok();
}
