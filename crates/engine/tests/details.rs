//! `details.get` (#47) as a front end runs it: by id with JSON params, the reply read back as its
//! result type, over a real repository whose tracking branch holds a ticket sent back once and
//! merged on its second gate run, with GitHub's issues from a `gh` of the test's own; and over
//! this clone's own tracking branch, when it has one.

use std::path::{Path, PathBuf};

use swarmception_domain::Node;
use swarmception_domain::details::{Details, PageKind, Subject};
use swarmception_domain::factory::FactoryScene;
use swarmception_domain::tooltip::{FAILED, PASSED};
use swarmception_engine::Session;
use swarmception_engine::factory_cmds::TicketBarsResult;
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_engine::rows_cmds::ListResult;
use swarmception_source::Github;

mod support;

fn temp(what: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-details-{what}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

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

/// #7 (its PR #8): claimed, worked on with a `ready` check, submitted, sent back by gate-pr8's
/// first run, worked on again, submitted, and merged by its second run; main merges PR #8.
fn repo() -> PathBuf {
    let dir = temp("repo");
    let files = [
        (
            "mini-aa1111",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-1","model":"claude-opus-5-5","effort":"high","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:02:00Z"}"#,
                r#"{"event":"sub.start","name":"ready","note":"ci --changed","sub":"ready-abc","t":"2026-10-31T03:20:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0","sub":"ready-abc","t":"2026-10-31T03:25:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:26:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:50:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T04:00:00Z"}"#,
            ],
        ),
        (
            "mini-cc3333",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7); kind=smoke","sub":"gate-pr8","t":"2026-10-31T03:30:00Z"}"#,
                r#"{"event":"sendback","issue":7,"mr":8,"note":"cause=test: a test failed","sub":"gate-pr8","t":"2026-10-31T03:40:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=1; cause=test","sub":"gate-pr8","t":"2026-10-31T03:41:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7); kind=smoke","sub":"gate-pr8","t":"2026-10-31T04:05:00Z"}"#,
                r#"{"event":"merge","issue":7,"mr":8,"sub":"gate-pr8","t":"2026-10-31T04:10:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0; merged #8","sub":"gate-pr8","t":"2026-10-31T04:11:00Z"}"#,
            ],
        ),
    ];
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
    git_at(&dir, early, &["remote", "add", "origin", "https://github.com/o/n.git"]);
    dir
}

/// A `gh` that prints #7 and its PR #8 as `gh api` would, for a session reading the issues of
/// `repo`: run by `sh`, never exec'd (`support::fake_gh`).
#[cfg(unix)]
fn fake_gh(dir: &Path, repo: &Path) -> PathBuf {
    let pages = r#"[
      {"number":7,"title":"Seven: the ticket's own title","state":"closed","labels":[{"name":"type::task"},{"name":"area::ui"}],"user":{"login":"owner"},
       "closed_at":"2026-10-31T04:10:00Z","html_url":"https://github.com/o/n/issues/7"},
      {"number":8,"title":"The fix for seven","state":"closed","body":"Closes #7","html_url":"https://github.com/o/n/pull/8",
       "pull_request":{"merged_at":"2026-10-31T04:10:00Z"}}
    ]"#;
    std::fs::write(dir.join("pages.json"), pages).unwrap();
    support::fake_gh(repo, &format!("cat '{}/pages.json'\n", dir.display()))
}

fn call<T: serde::de::DeserializeOwned>(session: &mut Session, id: &str, params: &str) -> T {
    let reply = session.execute(id, params).unwrap_or_else(|e| panic!("{id} {params}: {e}"));
    serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{id}'s reply {reply}: {e}"))
}

fn open(session: &mut Session, dir: &Path) -> StatusResult {
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    call(session, "repo.open", &format!("{{\"path\":{path}}}"))
}

fn whats(details: &Details) -> Vec<&str> {
    details.entries.iter().map(|e| e.what.as_str()).collect()
}

#[cfg(unix)]
fn session_over_repo() -> (Session, Vec<PathBuf>) {
    let (dir, tools) = (repo(), temp("gh"));
    let mut session = Session::new();
    session.set_github(Github { gh: fake_gh(&tools, &dir), cache_dir: None });
    let status = open(&mut session, &dir);
    assert_eq!((status.error, status.issues.issues), (None, 2), "{:?}", status.issues);
    (session, vec![dir, tools])
}

#[cfg(unix)]
#[test]
fn a_ticket_sent_back_once_tells_its_whole_story_in_order() {
    let (mut session, dirs) = session_over_repo();
    let d: Details = call(&mut session, "details.get", r#"{"ticket":7}"#);
    assert_eq!(d.subject, Subject::Ticket { issue: 7 });
    assert_eq!((d.title.as_str(), d.mark.as_str()), ("Seven: the ticket's own title", "#7"));
    assert_eq!(d.labels, ["type::task", "area::ui"]);
    assert_eq!(
        d.lines.iter().map(swarmception_domain::tooltip::TipLine::text).collect::<Vec<_>>(),
        ["closed · opened by owner", "PR #8 The fix for seven (merged)"]
    );
    assert_eq!(
        whats(&d),
        [
            "claimed",
            "working",
            "ready check passed",
            "waiting for merge",
            "gate run failed",
            "sent back",
            "working",
            "waiting for merge",
            "gate run passed",
            "merged"
        ]
    );
    let durations: Vec<Option<&str>> = d.entries.iter().map(|e| e.duration.as_deref()).collect();
    assert_eq!(
        durations,
        [
            Some("1 min 00 s"),
            Some("24 min 00 s"),
            Some("5 min 00 s"),
            Some("14 min 00 s"),
            Some("11 min 00 s"),
            Some("10 min 00 s"),
            Some("10 min 00 s"),
            Some("10 min 00 s"),
            Some("6 min 00 s"),
            None
        ]
    );
    let sent = &d.entries[5];
    assert_eq!((sent.colour, sent.notes[0].text().as_str()), (FAILED, "cause=test: a test failed"));
    assert_eq!(sent.notes[0].colour_of("cause=test: a test failed"), Some(FAILED), "the cause in red");
    assert_eq!(d.entries[4].notes[0].text(), "test");
    assert_eq!(d.entries[8].goes_to, Some(Subject::Gate { sub: "gate-pr8".into(), at: Some(d.entries[8].t) }));
    let merged = &d.entries[9];
    assert_eq!((merged.colour, merged.text().as_str()), (PASSED, "merged PR #8 in gate-pr8 · coordinator"));
    assert!(merged.notes[0].text().starts_with("onto main "), "{:?}", merged.notes);
    let facts: Vec<(&str, &str)> = d.facts.iter().map(|f| (f.label.as_str(), f.value.as_str())).collect();
    assert_eq!(facts[0], ("claim → merge", "1 h 09 min"));
    assert!(facts.contains(&("gate runs", "2 (1 passed, 1 failed)")) && facts.contains(&("sent back", "1×")), "{facts:?}");
    let pages: Vec<(PageKind, Option<&str>)> = d.pages.iter().map(|p| (p.kind, p.url.as_deref())).collect();
    assert_eq!(pages, [(PageKind::Issue, Some("https://github.com/o/n/issues/7")), (PageKind::Pull, Some("https://github.com/o/n/pull/8"))]);
    dirs.into_iter().for_each(|d| drop(std::fs::remove_dir_all(d)));
}

#[cfg(unix)]
#[test]
fn a_gate_run_by_its_id_and_a_time_in_it() {
    let (mut session, dirs) = session_over_repo();
    let first: Details = call(&mut session, "details.get", r#"{"gate":"gate-pr8"}"#);
    assert_eq!(whats(&first), ["started", "sent back #7", "failed"]);
    assert_eq!(first.colour, FAILED);
    assert_eq!(first.entries[2].detail.text(), "test");
    let rerun: Details = call(&mut session, "details.get", r#"{"gate":"gate-pr8","at":"2026-10-31T04:06:00Z"}"#);
    assert_eq!(whats(&rerun), ["started", "merged #7", "onto main", "passed"]);
    assert_eq!(rerun.entries[0].notes[0].text(), "local gate: #8 (#7); kind=smoke");
    assert_eq!(rerun.entries[1].goes_to, Some(Subject::Ticket { issue: 7 }));
    assert_eq!(rerun.related.iter().map(|r| r.mark.as_str()).collect::<Vec<_>>(), ["#7"]);
    assert_eq!(rerun.pages.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(), ["PR #8", "Issue #7"]);
    let worker: Details = call(&mut session, "details.get", r#"{"session":"mini-aa1111"}"#);
    assert_eq!(worker.title, "worker-1 · mini");
    assert!(whats(&worker).starts_with(&["session started", "claimed", "working on"]), "{:?}", whats(&worker));
    dirs.into_iter().for_each(|d| drop(std::fs::remove_dir_all(d)));
}

#[cfg(unix)]
#[test]
fn a_bar_on_the_timeline_is_its_ticket_or_its_merge_onto_main() {
    let (mut session, dirs) = session_over_repo();
    let found: TicketBarsResult = call(&mut session, "timeline.ticketBars", r#"{"issue":7}"#);
    for bar in &found.bars {
        let d: Details = call(&mut session, "details.get", &format!(r#"{{"track":"{}","range":{}}}"#, bar.track, bar.range));
        assert_eq!(d.subject, Subject::Ticket { issue: 7 }, "{bar:?}");
    }
    let list: ListResult = call(&mut session, "rows.list", "");
    let Some(Node::Ranges { key, bars, .. }) = list.rows.first() else { panic!("the main row first") };
    assert_eq!((key.as_str(), bars.len()), ("main", 1));
    let d: Details = call(&mut session, "details.get", r#"{"track":"main","range":0}"#);
    assert!(matches!(&d.subject, Subject::Merge { commit } if commit.len() == 40), "{:?}", d.subject);
    assert_eq!(whats(&d), ["#7 submitted", "gate run passed", "#7 merged", "onto main"]);
    assert_eq!(d.title, "PR #8 The fix for seven");
    dirs.into_iter().for_each(|d| drop(std::fs::remove_dir_all(d)));
}

#[cfg(unix)]
#[test]
fn details_name_their_bars_on_the_rows_the_timeline_shows() {
    let (mut session, dirs) = session_over_repo();
    for grouping in ["flat", "workers"] {
        let _: swarmception_domain::RowsSpec = call(&mut session, "rows.set", &format!(r#"{{"grouping":"{grouping}"}}"#));
        let mut seven: TicketBarsResult = call(&mut session, "timeline.ticketBars", r#"{"issue":7}"#);
        seven.bars.sort();
        let ticket: Details = call(&mut session, "details.get", r#"{"ticket":7}"#);
        assert_eq!(ticket.timeline.bars, seven.bars, "{grouping}: a ticket's bars are timeline.ticketBars'");
        assert!(ticket.timeline.start < ticket.timeline.end);
        let merge: Details = call(&mut session, "details.get", r#"{"track":"main","range":0}"#);
        assert!(merge.timeline.bars.iter().any(|b| b.track == "main" && b.range == 0), "{grouping}: the merge onto main's own bar");
        assert!(seven.bars.iter().all(|b| merge.timeline.bars.contains(b)), "{grouping}: and its ticket's");
        // Every bar named is one the rows have.
        for bar in &merge.timeline.bars {
            let _: Details = call(&mut session, "details.get", &format!(r#"{{"track":"{}","range":{}}}"#, bar.track, bar.range));
        }
    }
    dirs.into_iter().for_each(|d| drop(std::fs::remove_dir_all(d)));
}

#[cfg(unix)]
#[test]
fn what_isnt_there_or_isnt_one_thing_is_refused() {
    let (mut session, dirs) = session_over_repo();
    for (params, code) in [
        ("", "bad_params"),
        ("{}", "bad_params"),
        (r#"{"ticket":7,"gate":"gate-pr8"}"#, "bad_params"),
        (r#"{"track":"main"}"#, "bad_params"),
        (r#"{"ticket":"seven"}"#, "bad_params"),
        (r#"{"ticket":99}"#, "invalid"),
        (r#"{"gate":"gate-nope"}"#, "invalid"),
        (r#"{"session":"nobody-1"}"#, "invalid"),
        (r#"{"merge":"abc"}"#, "invalid"),
        (r#"{"track":"main","range":5}"#, "invalid"),
        (r#"{"gate":"gate-pr8","at":"teatime"}"#, "invalid"),
    ] {
        assert_eq!(session.execute("details.get", params).unwrap_err().code(), code, "{params}");
    }
    let error = session.execute("details.get", r#"{"ticket":99}"#).unwrap_err();
    assert_eq!(error.to_string(), "the swarm has no ticket #99");
    assert_eq!(Session::new().execute("details.get", r#"{"ticket":7}"#).unwrap_err().code(), "disabled", "no repository");
    dirs.into_iter().for_each(|d| drop(std::fs::remove_dir_all(d)));
}

/// This clone's own tracking branch, when it has one: every ticket's and every recent gate run's
/// story reads, oldest first, each duration its seconds.
#[test]
fn this_clones_own_swarm_tells_every_tickets_story() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let has = std::process::Command::new("git").current_dir(&root).args(["rev-parse", "--verify", "-q", "refs/remotes/origin/tracking"]).output();
    if !has.is_ok_and(|o| o.status.success()) {
        eprintln!("no origin/tracking here: nothing to read");
        return;
    }
    let mut session = Session::new();
    session.set_github(Github { gh: PathBuf::from("/no/such/gh"), cache_dir: None });
    let status = open(&mut session, &root);
    assert_eq!(status.error, None);
    let list: ListResult = call(&mut session, "rows.list", "");
    let mut issues: Vec<u64> = list
        .rows
        .iter()
        .flat_map(Node::leaves)
        .filter_map(|l| if let Node::Ranges { bars, .. } = l { Some(bars) } else { None })
        .flatten()
        .filter_map(|b| b.ticket)
        .collect();
    issues.sort_unstable();
    issues.dedup();
    assert!(!issues.is_empty(), "the swarm has tickets");
    let scene: FactoryScene = call(&mut session, "factory.at", "");
    let gates = scene.gate.recent.iter().chain(&scene.gate.running).map(|r| format!(r#"{{"gate":"{}","at":{}}}"#, r.sub, r.start.0));
    for params in issues.iter().map(|n| format!(r#"{{"ticket":{n}}}"#)).chain(gates) {
        let d: Details = call(&mut session, "details.get", &params);
        assert!(!d.entries.is_empty(), "{params}");
        assert!(d.entries.windows(2).all(|w| w[0].t <= w[1].t), "{params}: oldest first");
        for entry in &d.entries {
            assert_eq!(entry.duration.as_deref().map(str::to_owned), entry.seconds.map(swarmception_domain::rows::duration), "{params}: {}", entry.text());
        }
    }
}
