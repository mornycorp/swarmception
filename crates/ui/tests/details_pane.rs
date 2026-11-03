//! The details pane (#47) over a real repository's tracking branch: it draws what `details.get`
//! returns for what is selected (AGENTS.md rule 6), and follows the selection: a bar chosen on
//! the timeline, a ticket traced in the factory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swarmception_domain::details::Subject;
use swarmception_domain::rows::BarRef;
use swarmception_engine::details_cmds::{DetailsParams, Get};
use swarmception_engine::factory_cmds::{TicketBars, TicketParams};
use swarmception_model::Timestamp;
use swarmception_ui::control::{Answer, Inspect};
use swarmception_ui::{Lit, Shell, details_params};
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::ranges_cmds::{RangeParams, Toggle};
use timeline_engine::state_cmds::Get as GetState;

mod support;

/// #7 (PR #8) claimed, worked on, submitted and merged by gate-pr8; #9 claimed by another
/// worker; main merges PR #8.
fn repo() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-details-pane-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [
        (
            "mini-aa1111",
            [
                r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:02:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:30:00Z"}"#,
            ]
            .join("\n"),
        ),
        (
            "mini-bb2222",
            [
                r#"{"event":"session.start","machine":"mini","name":"worker-2","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":9,"t":"2026-10-31T03:05:00Z"}"#,
            ]
            .join("\n"),
        ),
        (
            "mini-cc3333",
            [
                r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7); kind=smoke","sub":"gate-pr8","t":"2026-10-31T03:40:00Z"}"#,
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
    let run_at = |at: &str, args: &[&str]| {
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
    run_at(early, &["init", "-q", "-b", "tracking"]);
    run_at(early, &["add", "."]);
    run_at(early, &["commit", "-q", "-m", "events"]);
    run_at(early, &["checkout", "-q", "--orphan", "main"]);
    run_at(early, &["rm", "-rqf", "."]);
    run_at(early, &["commit", "-q", "--allow-empty", "-m", "start"]);
    run_at(early, &["checkout", "-q", "-b", "pr-8"]);
    run_at("2026-10-31T03:45:00Z", &["commit", "-q", "--allow-empty", "-m", "work"]);
    run_at(early, &["checkout", "-q", "main"]);
    run_at("2026-10-31T03:45:00Z", &["merge", "-q", "--no-ff", "-m", "Merge pull request #8 from mornycorp/pr-8", "pr-8"]);
    dir
}

/// One frame of the shell, 1400 × 900 points: the text it drew.
fn frame(shell: &mut Shell, ctx: &egui::Context) -> Vec<String> {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
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

fn loaded(dir: &Path) -> (egui::Context, Shell) {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, Some(dir));
    support::until("the tracking branch loaded", || {
        frame(&mut shell, &ctx);
        shell.has_timeline()
    });
    assert!(shell.has_timeline(), "the tracking branch loaded");
    (ctx, shell)
}

#[test]
fn the_pane_draws_what_details_get_returns() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let texts = frame(&mut shell, &ctx);
    assert!(texts.iter().any(|t| t.starts_with("Select a bar on the timeline")), "nothing selected yet: a hint");
    for subject in
        [Subject::Ticket { issue: 7 }, Subject::Gate { sub: "gate-pr8".into(), at: None }, Subject::Session { instance: "mini-aa1111".into(), at: None }]
    {
        shell.select(subject.clone());
        frame(&mut shell, &ctx);
        let texts = frame(&mut shell, &ctx);
        let answer = shell.engine().call::<Get>(details_params(&subject)).unwrap();
        let pane = shell.details();
        assert_eq!(pane.details(), Some(&answer), "{subject:?}: the pane holds what details.get returns");
        let drawn = pane.drawn();
        assert_eq!((drawn.title.as_deref(), drawn.mark.as_deref()), (Some(answer.title.as_str()), Some(answer.mark.as_str())));
        assert_eq!(drawn.entries, answer.entries.iter().map(swarmception_domain::details::Entry::text).collect::<Vec<_>>(), "{subject:?}: a line an entry");
        assert_eq!(drawn.facts, answer.facts.iter().map(|f| format!("{} {}", f.label, f.value)).collect::<Vec<_>>());
        for entry in &answer.entries {
            assert!(texts.contains(&entry.what), "{subject:?}: `{}` drawn", entry.what);
            if let Some(took) = &entry.duration {
                assert!(texts.iter().any(|t| t.contains(took.as_str())), "{subject:?}: `{took}` drawn");
            }
        }
    }
    assert_eq!(shell.details().details().map(|d| d.title.as_str()), Some("worker-1 · mini"));
    std::fs::remove_dir_all(dir).ok();
}

/// `bars` by row key, as the widget's highlight holds them.
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

/// The person's own selection on the timeline, as text to compare: its ranges, instant, focus
/// and playhead.
fn person(shell: &mut Shell) -> String {
    let s = shell.timeline().widget().session().call::<GetState>(NoParams {}).unwrap().timeline.unwrap();
    format!("{:?}", (s.selection, s.selected_ranges, s.selected_instant, s.focus, s.playhead))
}

#[test]
fn the_timeline_highlights_the_details_subjects_bars_and_never_the_selection() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    // The person chose a bar of #9's on the timeline: the details are #9's, its bars highlighted.
    let nine = shell.engine().call::<TicketBars>(TicketParams { issue: 9 }).unwrap().bars;
    shell.timeline().widget().session().call::<Toggle>(RangeParams { track: nine[0].track.clone(), range: nine[0].range }).unwrap();
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.ui_state().details.subject, Some(Subject::Ticket { issue: 9 }));
    assert_eq!((shell.timeline().highlighted(), shell.timeline().lit()), (by_row(&nine), Lit::Details));
    let before = person(&mut shell);

    // Each kind of subject chosen in the pane: its bars highlighted, as details.get names them.
    let seven = shell.engine().call::<TicketBars>(TicketParams { issue: 7 }).unwrap().bars;
    let main = BarRef { track: "main".into(), range: 0 };
    let stretches: Vec<BarRef> = seven.iter().filter(|b| b.track != "merged").cloned().collect();
    let merge = shell.engine().call::<Get>(DetailsParams { track: Some("main".into()), range: Some(0), ..DetailsParams::default() }).unwrap().subject;
    let with_main = [seven.clone(), vec![main.clone()]].concat();
    // A gate run's own bar on the Gate runs row too (#84).
    let with_run = [with_main.clone(), vec![BarRef { track: "gates".into(), range: 0 }]].concat();
    for (subject, want) in [
        (Subject::Ticket { issue: 7 }, seven.clone()),
        (Subject::Gate { sub: "gate-pr8".into(), at: None }, with_run),
        (Subject::Session { instance: "mini-aa1111".into(), at: None }, stretches),
        (Subject::Session { instance: "mini-cc3333".into(), at: None }, vec![main.clone(), seven.iter().find(|b| b.track == "merged").unwrap().clone()]),
        (merge, with_main),
    ] {
        shell.select(subject.clone());
        frame(&mut shell, &ctx);
        frame(&mut shell, &ctx);
        let answer = shell.engine().call::<Get>(details_params(&subject)).unwrap().timeline;
        assert_eq!(by_row(&answer.bars), by_row(&want), "{subject:?}: details.get names its bars");
        assert_eq!(shell.timeline().highlighted(), by_row(&want), "{subject:?}: the timeline highlights them");
        let inspect = shell.inspect();
        assert_eq!((&inspect.details.subject, &inspect.details.bars, inspect.timeline.lit), (&Some(subject.clone()), &answer.bars, Lit::Details));
        assert_eq!(person(&mut shell), before, "{subject:?}: the selection never touched");
    }

    // ← back: the subject before, and its bars.
    shell.details_back();
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert!(matches!(shell.ui_state().details.subject, Some(Subject::Session { .. })));
    assert_eq!(shell.timeline().highlighted().get("main"), Some(&vec![0]));

    // Details the swarm never had: nothing highlighted, the selection as it was.
    shell.select(Subject::Ticket { issue: 99 });
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert!(shell.timeline().highlighted().is_empty());
    assert_eq!((shell.timeline().lit(), person(&mut shell)), (Lit::Nothing, before));

    // The details pane closed: its subject highlights nothing.
    shell.select(Subject::Ticket { issue: 7 });
    let mut state = shell.ui_state();
    state.panes.details = false;
    shell.set_ui_state(state);
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert!(shell.timeline().highlighted().is_empty());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn ui_set_details_reports_its_subject_and_brings_its_bars_into_view() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let set = |shell: &mut Shell, params: &str| -> Inspect {
        match shell.answer("ui.set", params) {
            Answer::Now(Ok(text)) => serde_json::from_str(&text).unwrap(),
            other => panic!("{params}: {other:?}"),
        }
    };
    let window = |shell: &mut Shell| shell.inspect().timeline.window.map(|w| (w.start, w.end)).unwrap();
    let t = |text: &str| Timestamp::parse(text).unwrap();
    let before = person(&mut shell);

    // The window after the gate run ended; its bars (its own on the Gate runs row, #84,
    // among them) nowhere in it.
    set(&mut shell, r#"{"window": {"start": "2026-10-31T03:46:05Z", "end": "2026-10-31T03:47:00Z"}}"#);
    frame(&mut shell, &ctx);
    assert_eq!(window(&mut shell), (t("2026-10-31T03:46:05Z"), t("2026-10-31T03:47:00Z")));
    let inspect = set(&mut shell, r#"{"details": {"gate": {"sub": "gate-pr8"}}}"#);
    assert_eq!(inspect.state.details.subject, Some(Subject::Gate { sub: "gate-pr8".into(), at: None }));
    assert_eq!(inspect.details.subject, inspect.state.details.subject, "ui.inspect's details report the subject");
    assert_eq!(inspect.details.title.as_deref(), Some("gate run gate-pr8"));
    let gate = shell.engine().call::<Get>(details_params(&Subject::Gate { sub: "gate-pr8".into(), at: None })).unwrap().timeline;
    assert_eq!(inspect.details.bars, gate.bars);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    // Too wide for the window: the window holds them all, from #7's claim to the merge onto main.
    let (start, end) = window(&mut shell);
    assert!(start <= t("2026-10-31T03:01:00Z") && end >= t("2026-10-31T03:45:10Z"), "{start} → {end}");
    assert_eq!(shell.timeline().highlighted(), by_row(&gate.bars));
    assert_eq!(person(&mut shell), before, "the selection untouched");

    // A subject on screen already: the window stays.
    set(&mut shell, r#"{"window": {"start": "2026-10-31T03:10:00Z", "end": "2026-10-31T03:12:00Z"}}"#);
    frame(&mut shell, &ctx);
    let stays = window(&mut shell);
    set(&mut shell, r#"{"details": {"ticket": {"issue": 7}}}"#);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(window(&mut shell), stays, "#7's working bar was in view");

    // Narrower than the window: the same width, centred on them (the coordinator's merge).
    set(&mut shell, r#"{"window": {"start": "2026-10-31T03:20:00Z", "end": "2026-10-31T03:20:50Z"}}"#);
    frame(&mut shell, &ctx);
    assert_eq!(window(&mut shell), (t("2026-10-31T03:20:00Z"), t("2026-10-31T03:20:50Z")));
    set(&mut shell, r#"{"details": {"session": {"instance": "mini-cc3333"}}}"#);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    let (start, end) = window(&mut shell);
    assert!(start <= t("2026-10-31T03:45:00Z") && end >= t("2026-10-31T03:45:10Z"), "{start} → {end}");
    assert!((end.0 - start.0 - 50).abs() <= 1, "the window's width kept: {start} → {end}");

    // Cleared: nothing reported, nothing highlighted.
    let inspect = set(&mut shell, r#"{"clearDetails": true}"#);
    assert_eq!((inspect.details.subject, inspect.details.bars.len()), (None, 0));
    frame(&mut shell, &ctx);
    assert!(shell.timeline().highlighted().is_empty());
    assert_eq!(person(&mut shell), before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_pane_follows_the_timelines_selection_and_the_factorys_traced_ticket() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    // A bar of #7's chosen on the timeline: the details are #7's.
    let bars = shell.engine().call::<TicketBars>(TicketParams { issue: 7 }).unwrap().bars;
    let bar = bars.iter().find(|b| b.track != "merged").unwrap().clone();
    shell.timeline().widget().session().call::<Toggle>(RangeParams { track: bar.track.clone(), range: bar.range }).unwrap();
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.ui_state().details.subject, Some(Subject::Ticket { issue: 7 }));
    assert_eq!(shell.details().details().map(|d| d.mark.as_str()), Some("#7"));

    // The merge onto main chosen instead: the merge's details.
    let session = shell.timeline().widget().session();
    session.call::<Toggle>(RangeParams { track: bar.track, range: bar.range }).unwrap();
    session.call::<Toggle>(RangeParams { track: "main".into(), range: 0 }).unwrap();
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert!(matches!(shell.ui_state().details.subject, Some(Subject::Merge { .. })), "{:?}", shell.ui_state().details.subject);

    // A ticket traced in the factory (a chip clicked): its details.
    let mut state = shell.ui_state();
    state.factory.ticket = Some(9);
    shell.set_ui_state(state);
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.ui_state().details.subject, Some(Subject::Ticket { issue: 9 }));
    assert_eq!(shell.details().details().map(|d| d.entries[0].what.as_str()), Some("claimed"));

    // What the swarm never had: the pane says why, never a panic.
    shell.select(Subject::Ticket { issue: 99 });
    let texts = frame(&mut shell, &ctx);
    assert!(texts.iter().any(|t| t.contains("the swarm has no ticket #99")), "{texts:?}");
    std::fs::remove_dir_all(dir).ok();
}
