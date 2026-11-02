//! The details pane (#47) over a real repository's tracking branch: it draws what `details.get`
//! returns for what is selected (AGENTS.md rule 6), and follows the selection: a bar chosen on
//! the timeline, a ticket traced in the factory.

use std::path::{Path, PathBuf};

use swarmception_domain::details::Subject;
use swarmception_engine::details_cmds::Get;
use swarmception_engine::factory_cmds::{TicketBars, TicketParams};
use swarmception_ui::{Shell, details_params};
use timeline_engine::ranges_cmds::{RangeParams, Toggle};

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
    for _ in 0..500 {
        frame(&mut shell, &ctx);
        if shell.has_timeline() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
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
