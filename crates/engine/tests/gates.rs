//! `gates.list` (#61) as a front end runs it: by id with JSON params, the reply read back as its
//! result type, over a real repository whose tracking branch holds a gate run that sends a ticket
//! back, its rerun that merges it, and the worker's `ready` checks; over this clone's own
//! tracking branch, when it has one; and over the swarms `SWARMCEPTION_GATES_REPOS` names
//! (paths, `:`-separated), printing each one's figures.

use std::path::{Path, PathBuf};

use swarmception_domain::details::{Details, Subject};
use swarmception_domain::gates::{GateResult, GatesList, Lane};
use swarmception_domain::rows::{MERGED_KEY, Stage};
use swarmception_domain::stats::Gates;
use swarmception_domain::{Node, Stats};
use swarmception_engine::Session;
use swarmception_engine::factory_cmds::TicketBarsResult;
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_engine::rows_cmds::{BarResult, ListResult};
use swarmception_source::Github;

fn git_at(dir: &Path, at: &str, args: &[&str]) {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_AUTHOR_DATE", at)
        .env("GIT_COMMITTER_DATE", at)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// #7 (PR #8): worked on with a failed `ready` check and its rerun, submitted, sent back by
/// gate-8 on a conflict, submitted again and merged by gate-8b; main merges PR #8.
fn repo() -> PathBuf {
    make(files())
}

/// [`repo`]'s, and a second coordinator's two gates that logged no `sub.start`: gate-pr21 merges
/// #11 at 04:20, gate-pr22 sends #12 back on a conflict at 04:30.
fn repo_inferred() -> PathBuf {
    let mut files = files();
    files.push((
        "mini-dd4444",
        vec![
            r#"{"event":"session.start","machine":"mini","name":"coordinator-2","role":"coordinator","t":"2026-10-31T04:15:00Z"}"#,
            r#"{"event":"merge","issue":11,"mr":21,"sub":"gate-pr21","t":"2026-10-31T04:20:00Z"}"#,
            r#"{"event":"sendback","issue":12,"mr":22,"note":"cause=conflict","sub":"gate-pr22","t":"2026-10-31T04:30:00Z"}"#,
        ],
    ));
    make(files)
}

type Files = Vec<(&'static str, Vec<&'static str>)>;

fn files() -> Files {
    vec![
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
            "mini-cc3333",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7); kind=smoke","sub":"gate-8","t":"2026-10-31T03:30:00Z"}"#,
                r#"{"event":"sendback","issue":7,"mr":8,"note":"cause=conflict","sub":"gate-8","t":"2026-10-31T03:40:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=1; cause=conflict","sub":"gate-8","t":"2026-10-31T03:41:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7); kind=smoke","sub":"gate-8b","t":"2026-10-31T04:05:00Z"}"#,
                r#"{"event":"merge","issue":7,"mr":8,"sub":"gate-8b","t":"2026-10-31T04:10:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0; merged #8","sub":"gate-8b","t":"2026-10-31T04:11:00Z"}"#,
            ],
        ),
    ]
}

fn make(files: Files) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-gates-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (instance, lines) in files {
        std::fs::create_dir_all(dir.join("tracking").join(instance)).unwrap();
        std::fs::write(dir.join("tracking").join(instance).join("work.jsonl"), lines.join("\n") + "\n").unwrap();
    }
    let early = "2026-10-31T02:00:00Z";
    git_at(&dir, early, &["init", "-q", "-b", "tracking"]);
    git_at(&dir, early, &["add", "."]);
    git_at(&dir, early, &["commit", "-q", "-m", "events"]);
    git_at(&dir, early, &["checkout", "-q", "--orphan", "main"]);
    git_at(&dir, early, &["rm", "-rqf", "."]);
    git_at(&dir, early, &["commit", "-q", "--allow-empty", "-m", "start"]);
    git_at(&dir, early, &["checkout", "-q", "-b", "pr-8"]);
    let merged = "2026-10-31T04:10:00Z";
    git_at(&dir, merged, &["commit", "-q", "--allow-empty", "-m", "work"]);
    git_at(&dir, early, &["checkout", "-q", "main"]);
    git_at(&dir, merged, &["merge", "-q", "--no-ff", "-m", "Merge pull request #8 from mornycorp/pr-8", "pr-8"]);
    git_at(&dir, early, &["checkout", "-q", "tracking"]);
    dir
}

fn call<T: serde::de::DeserializeOwned>(session: &mut Session, id: &str, params: &str) -> T {
    let reply = session.execute(id, params).unwrap_or_else(|e| panic!("{id} {params}: {e}"));
    serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{id}'s reply {reply}: {e}"))
}

fn open(session: &mut Session, dir: &Path) -> StatusResult {
    session.set_github(Github { gh: PathBuf::from("/no/such/gh"), cache_dir: None });
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    call(session, "repo.open", &format!("{{\"path\":{path}}}"))
}

#[test]
fn every_run_its_result_and_the_figures() {
    let dir = repo();
    let mut session = Session::new();
    assert_eq!(session.execute("gates.list", "").unwrap_err().code(), "disabled", "no repository yet");
    open(&mut session, &dir);
    let list: GatesList = call(&mut session, "gates.list", "");
    type Row<'a> = (&'a str, Lane, GateResult, Option<&'a str>, Option<&'a str>);
    let runs: Vec<Row<'_>> = list.runs.iter().map(|r| (r.sub.as_str(), r.lane, r.result, r.cause.as_deref(), r.rerun_of.as_deref())).collect();
    assert_eq!(
        runs,
        [
            ("ready-abc", Lane::Ready, GateResult::Failed, Some("clippy"), None),
            ("ready-abc-rerun", Lane::Ready, GateResult::Passed, None, Some("ready-abc")),
            ("gate-8", Lane::Gate, GateResult::Failed, Some("conflict"), None),
            ("gate-8b", Lane::Gate, GateResult::Passed, None, Some("gate-8")),
        ]
    );
    let merged = &list.runs[3];
    assert_eq!((merged.tickets.as_slice(), merged.prs.as_slice(), merged.duration_seconds), (&[7][..], &[8][..], 6 * 60));
    let commit = merged.merges.first().and_then(|m| m.commit.clone()).expect("main's merge commit of PR #8");
    assert_eq!(list.runs[2].sendbacks, [7]);
    assert_eq!((list.gate.runs, list.gate.pass_rate, list.gate.median_seconds, list.gate.busy_seconds), (2, Some(0.5), Some((11 * 60 + 6 * 60) / 2), 17 * 60));
    assert_eq!(list.gate.causes.iter().map(|c| (c.cause.as_str(), c.count)).collect::<Vec<_>>(), [("conflict", 1)]);
    assert_eq!((list.ready.runs, list.ready.pass_rate, list.ready.reruns), (2, Some(0.5), 1));
    assert_eq!((list.longest_queue.count, list.longest_queue.tickets.as_slice()), (1, &[7][..]));
    // The pass rates are `stats.get`'s.
    let stats: Stats = call(&mut session, "stats.get", "");
    assert_eq!((list.gate.pass_rate, list.ready.pass_rate), (stats.gates.pass_rate, stats.ready.pass_rate));
    // A run's subject is what the details pane tells: the gate run, with its merge's commit.
    let details: Details = call(&mut session, "details.get", r#"{"gate":"gate-8b"}"#);
    assert_eq!(details.subject, merged.subject);
    assert!(details.entries.iter().any(|e| e.goes_to == Some(Subject::Merge { commit: commit.clone() })), "{commit}");
    let check = &list.runs[0];
    assert_eq!(check.subject, Subject::Gate { sub: check.sub.clone(), at: Some(check.start) }, "a ready check: its own story (#95)");
    let told: Details = call(&mut session, "details.get", &format!(r#"{{"gate":"{}"}}"#, check.sub));
    assert_eq!((told.subject, told.title.as_str()), (check.subject.clone(), "ready check ready-abc"));

    // A window: the runs it overlaps, the figures of those it started.
    let window: GatesList = call(&mut session, "gates.list", r#"{"start":"2026-10-31T03:35:00Z","end":"2026-10-31T04:06:00Z"}"#);
    let subs: Vec<&str> = window.runs.iter().map(|r| r.sub.as_str()).collect();
    assert_eq!(subs, ["gate-8", "gate-8b"]);
    assert_eq!((window.gate.runs, window.gate.busy_seconds, window.span_seconds), (1, 6 * 60 + 60, 31 * 60));
    for bad in [r#"{"start":"teatime"}"#, r#"{"start":"2026-10-31T04:00:00Z","end":"2026-10-31T03:00:00Z"}"#] {
        assert_eq!(session.execute("gates.list", bad).unwrap_err().code(), "invalid", "{bad}");
    }
    assert_eq!(session.execute("gates.list", r#"{"nope":1}"#).unwrap_err().code(), "bad_params");
    std::fs::remove_dir_all(dir).ok();
}

/// `gates.list` and `stats.get` over the same window give the same gate-run counts (#75), the
/// runs read from merges and send backs among them and said to be.
#[test]
fn gates_list_and_stats_get_agree_on_every_window() {
    let dir = repo_inferred();
    let mut session = Session::new();
    open(&mut session, &dir);
    let windows = [
        "{}",
        r#"{"start":"2026-10-31T03:00:00Z","end":"2026-10-31T04:00:00Z"}"#,
        r#"{"start":"2026-10-31T04:00:00Z","end":"2026-10-31T04:25:00Z"}"#,
        r#"{"start":"2026-10-31T04:25:00Z"}"#,
        r#"{"end":"2026-10-31T03:35:00Z"}"#,
        r#"{"start":"2026-11-30T00:00:00Z"}"#,
    ];
    for window in windows {
        let list: GatesList = call(&mut session, "gates.list", window);
        let stats: Stats = call(&mut session, "stats.get", window);
        assert_eq!((Gates::from(&list.gate), Gates::from(&list.ready)), (stats.gates.clone(), stats.ready.clone()), "{window}");
        assert_eq!((list.start, list.end), (stats.start, stats.end), "{window}: the same window");
    }
    let whole: Stats = call(&mut session, "stats.get", "{}");
    assert_eq!((whole.gates.runs, whole.gates.inferred, whole.gates.passed, whole.gates.failed), (4, 2, 2, 2), "gate-8, gate-8b, gate-pr21, gate-pr22");
    assert_eq!(whole.gates.pass_rate, Some(0.5));
    let late: GatesList = call(&mut session, "gates.list", r#"{"start":"2026-10-31T04:25:00Z"}"#);
    assert_eq!((late.gate.runs, late.gate.inferred, late.gate.failed), (1, 1, 1));
    assert_eq!(late.runs.iter().map(|r| (r.sub.as_str(), r.inferred)).collect::<Vec<_>>(), [("gate-pr22", true)]);
    check_swarm(&dir);
    std::fs::remove_dir_all(dir).ok();
}

/// A swarm's gate as `gates.list` reads it: consistent with itself and with `stats.get`.
fn check_swarm(dir: &Path) -> (GatesList, Stats) {
    let mut session = Session::new();
    let status = open(&mut session, dir);
    assert_eq!(status.error, None, "{}", dir.display());
    let list: GatesList = call(&mut session, "gates.list", "");
    let stats: Stats = call(&mut session, "stats.get", "");
    for (figures, lane) in [(&list.gate, Lane::Gate), (&list.ready, Lane::Ready)] {
        assert_eq!(figures.runs, figures.passed + figures.failed + figures.no_result + figures.running, "{lane:?}");
        assert_eq!(figures.causes.iter().map(|c| c.count).sum::<usize>(), figures.failed, "{lane:?}");
        assert_eq!(figures.runs, list.runs.iter().filter(|r| r.lane == lane).count(), "the whole run: every run started in it");
        assert!(figures.busy_seconds <= list.span_seconds);
        let hours: usize = list.hours.iter().map(|h| if lane == Lane::Gate { h.gate.runs } else { h.ready.runs }).sum();
        assert_eq!(hours, figures.runs, "{lane:?}: each run in one hour");
    }
    // One definition of a gate run (#75): the same counts from both, inferred runs said.
    assert_eq!((Gates::from(&list.gate), Gates::from(&list.ready)), (stats.gates.clone(), stats.ready.clone()), "{}", dir.display());
    assert_eq!(list.gate.inferred, list.runs.iter().filter(|r| r.inferred).count());
    for run in &list.runs {
        assert!(run.duration_seconds >= 0 && run.end.is_none_or(|end| end >= run.start), "{}", run.sub);
        assert!(!run.tooltip.is_empty());
    }
    (list, stats)
}

/// #117 (reversing #84): the gate's runs have no rows on the timeline: `rows.list` has none,
/// `timeline.runBars` is gone and no bar is a run's. A run is its ticket's *in the gate* stretch,
/// on its bar's strip and in its tooltip, and `details.get` of a run lights its tickets' bars and
/// the merges onto main it made; `gates.list` still lists every run.
#[test]
fn a_gate_run_has_no_row_its_time_is_its_tickets_in_the_gate() {
    let dir = repo_inferred();
    let mut session = Session::new();
    open(&mut session, &dir);
    let rows: ListResult = call(&mut session, "rows.list", "");
    let names: Vec<&str> = rows.rows.iter().map(Node::name).collect();
    assert_eq!(names.get(..2), Some(&["Merges to main", "Tickets merged"][..]));
    assert!(!names.iter().any(|n| *n == "Gate runs" || *n == "Ready checks"), "{names:?}");
    let keys: Vec<&str> = rows.rows.iter().flat_map(Node::leaves).map(Node::key).collect();
    assert!(!keys.iter().any(|k| *k == "gates" || *k == "ready"), "{keys:?}");
    assert_eq!(session.execute("timeline.runBars", r#"{"sub":"gate-8b"}"#).unwrap_err().code(), "unknown_command");
    assert_eq!(session.execute("timeline.bar", r#"{"track":"gates","range":0}"#).unwrap_err().code(), "invalid");

    // #7: sent back by gate-8 (03:30 to 03:40), merged by gate-8b (04:05 to 04:10).
    let bars: TicketBarsResult = call(&mut session, "timeline.ticketBars", r#"{"issue":7}"#);
    let on_row = bars.bars.iter().find(|b| b.track != MERGED_KEY).expect("#7's bar");
    let bar: BarResult =
        call(&mut session, "timeline.bar", &serde_json::to_string(&serde_json::json!({"track": on_row.track, "range": on_row.range})).unwrap());
    let gating: Vec<(String, String)> =
        bar.bar.segments.iter().filter(|s| s.stage == Stage::Gating).map(|s| (s.start.to_string(), s.end.to_string())).collect();
    let at = |t: &str| t.to_owned();
    assert_eq!(gating, [(at("2026-10-31T03:30:00Z"), at("2026-10-31T03:40:00Z")), (at("2026-10-31T04:05:00Z"), at("2026-10-31T04:10:00Z"))]);
    for sub in ["gate-8", "gate-8b"] {
        let line = bar.bar.tooltip_lines.iter().find(|l| l.text().ends_with(&format!(") {sub}"))).unwrap_or_else(|| panic!("{sub}: {:?}", bar.bar.tooltip));
        assert_eq!(line.colour_of("in the gate"), Some(Stage::Gating.colour()), "{sub}");
    }

    // A run's details light its ticket's bars and its merge onto main, no bar of its own.
    let details: Details = call(&mut session, "details.get", r#"{"gate":"gate-8b"}"#);
    assert!(matches!(&details.subject, Subject::Gate { sub, .. } if sub == "gate-8b"));
    assert!(!details.timeline.bars.is_empty() && details.timeline.bars.iter().all(|b| b.track != "gates" && b.track != "ready"), "{:?}", details.timeline.bars);
    assert!(bars.bars.iter().all(|b| details.timeline.bars.contains(b)), "#7's bars lit");
    // `gates.list` keeps every run.
    let list: GatesList = call(&mut session, "gates.list", "{}");
    let subs: Vec<&str> = list.runs.iter().map(|r| r.sub.as_str()).collect();
    for sub in ["gate-8", "gate-8b", "ready-abc", "gate-pr21", "gate-pr22"] {
        assert!(subs.contains(&sub), "{sub}: {subs:?}");
    }
    std::fs::remove_dir_all(dir).ok();
}

fn print_figures(name: &str, list: &GatesList) {
    let pct = |v: Option<f64>| v.map_or_else(|| "–".to_owned(), |v| format!("{:.1}%", v * 100.0));
    for (lane, f) in [("gate", &list.gate), ("ready", &list.ready)] {
        let causes: Vec<String> = f.causes.iter().map(|c| format!("{} {}", c.cause, c.count)).collect();
        eprintln!(
            "{name} {lane}: {} runs, {} passed, {} failed, {} no result, {} running · pass {} · median {:?} s · busy {} · reruns {} · causes: {}",
            f.runs,
            f.passed,
            f.failed,
            f.no_result,
            f.running,
            pct(f.pass_rate),
            f.median_seconds,
            pct(f.busy),
            f.reruns,
            causes.join(", ")
        );
    }
    eprintln!("{name} longest queue: {} at {:?} {:?}", list.longest_queue.count, list.longest_queue.at.map(|t| t.to_string()), list.longest_queue.tickets);
}

/// This clone's own swarm, when it has a tracking branch.
#[test]
fn this_clones_own_gate_reads() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let has = std::process::Command::new("git").current_dir(&root).args(["rev-parse", "--verify", "-q", "refs/remotes/origin/tracking"]).output();
    if !has.is_ok_and(|o| o.status.success()) {
        eprintln!("no origin/tracking here: nothing to read");
        return;
    }
    let (list, _) = check_swarm(&root);
    print_figures("swarmception", &list);
}

/// The swarms `SWARMCEPTION_GATES_REPOS` names (mc-engine, swarm-timeline: their gates' causes
/// and pass rates), when it is set.
#[test]
fn named_swarms_gates_read() {
    let Ok(repos) = std::env::var("SWARMCEPTION_GATES_REPOS") else {
        eprintln!("SWARMCEPTION_GATES_REPOS unset: nothing to read");
        return;
    };
    for repo in repos.split(':').filter(|r| !r.is_empty()) {
        let (list, _) = check_swarm(Path::new(repo));
        assert!(!list.runs.is_empty(), "{repo}: a swarm with a gate");
        print_figures(repo, &list);
    }
}
