//! The repository's GitHub issues (#17), read on a load as a front end loads: through a `gh` of
//! the test's own (a script printing `gh api`'s pages, or failing), a cache in a temporary
//! directory, and a real repository whose tracking branch holds one ticket. No network, and no
//! browser: `ticket.urls` answers the pages, a front end opens them. A load with a cache asks
//! only for what changed since (#62), the fake `gh` answering that request apart.

use std::path::{Path, PathBuf};

use swarmception_domain::Node;
use swarmception_engine::EngineError;
use swarmception_engine::Session;
use swarmception_engine::error::Unavailable;
use swarmception_engine::issues::{IssuesSource, RefreshKind};
use swarmception_engine::issues_cmds::TicketUrlsResult;
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_engine::rows_cmds::ListResult;
use swarmception_source::Github;

mod support;
use support::Wakes;

fn temp(what: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-issues-{what}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git").current_dir(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A repository whose tracking branch holds #7 (its PR #8) claimed, submitted and merged; its
/// `origin` is `remote` when given.
fn repo(remote: Option<&str>) -> PathBuf {
    let dir = temp("repo");
    std::fs::create_dir_all(dir.join("tracking/mini-aa1111")).unwrap();
    std::fs::write(
        dir.join("tracking/mini-aa1111/work.jsonl"),
        [
            r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
            r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
            r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:30:00Z"}"#,
            r#"{"event":"merge","issue":7,"mr":8,"t":"2026-10-31T03:45:00Z"}"#,
            r#"{"event":"claim","issue":9,"t":"2026-10-31T03:50:00Z"}"#,
        ]
        .join("\n"),
    )
    .unwrap();
    git(&dir, &["init", "-q", "-b", "tracking"]);
    git(&dir, &["add", "."]);
    git(&dir, &["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "-c", "maintenance.auto=false", "commit", "-q", "-m", "events"]);
    if let Some(remote) = remote {
        git(&dir, &["remote", "add", "origin", remote]);
    }
    dir
}

/// `gh api`'s pages for `o/n`: #7, the PR #8 that closes it, and an issue the swarm never held.
const PAGES: &str = r#"[
  {"number":7,"title":"Seven: the ticket's own title","state":"closed","labels":[{"name":"type::task"}],"user":{"login":"owner"},
   "created_at":"2026-10-31T02:00:00Z","closed_at":"2026-10-31T03:45:00Z","updated_at":"2026-10-31T03:45:00Z","html_url":"https://github.com/o/n/issues/7"},
  {"number":8,"title":"The fix for seven","state":"closed","body":"Closes #7","html_url":"https://github.com/o/n/pull/8",
   "updated_at":"2026-10-31T03:45:00Z","pull_request":{"merged_at":"2026-10-31T03:45:00Z"}}
]
[
  {"number":12,"title":"Never held","state":"open","updated_at":"2026-10-31T04:00:00Z","html_url":"https://github.com/o/n/issues/12"}
]"#;

/// The pulls' pages: each one's head branch.
const PULLS: &str = r#"[{"number":8,"head":{"ref":"7-seven"}},{"number":13,"head":{"ref":"12-never-held"}}]"#;

/// What changed since, as `gh api --include` prints it: #12 renamed, and a new PR #13 on #12's
/// branch, its body closing nothing.
const SINCE: &str = "HTTP/2.0 200 OK\nEtag: W/\"e1\"\r\n\r\n[\
  {\"number\":12,\"title\":\"Held at last\",\"state\":\"open\",\"updated_at\":\"2026-11-01T09:00:00Z\",\"html_url\":\"https://github.com/o/n/issues/12\"},\
  {\"number\":13,\"title\":\"Twelve's fix\",\"state\":\"open\",\"body\":\"\",\"updated_at\":\"2026-11-01T10:00:00Z\",\
   \"html_url\":\"https://github.com/o/n/pull/13\",\"pull_request\":{}}]";

/// A `gh` that prints `PAGES` for the issues, `PULLS` for the pulls, `SINCE` for what changed and
/// a 304 for a request with an ETag (each call's arguments a line of `<dir>/args`), or fails
/// saying `fail`, for a session reading the issues of `repo`: run by `sh`, never exec'd
/// (`support::fake_gh`).
#[cfg(unix)]
fn fake_gh(dir: &Path, repo: &Path, fail: Option<&str>) -> PathBuf {
    std::fs::write(dir.join("pages.json"), PAGES).unwrap();
    std::fs::write(dir.join("pulls.json"), PULLS).unwrap();
    std::fs::write(dir.join("since.txt"), SINCE).unwrap();
    let d = dir.display();
    let script = match fail {
        Some(why) => format!("echo '{why}' >&2\nexit 1\n"),
        None => format!(
            "echo \"$@\" >> '{d}/args'\ncase \"$*\" in\n  *If-None-Match*) echo 'gh: HTTP 304' >&2; exit 1 ;;\n  *since=*) cat '{d}/since.txt' ;;\n  *pulls*) cat '{d}/pulls.json' ;;\n  *) cat '{d}/pages.json' ;;\nesac\n"
        ),
    };
    support::fake_gh(repo, &script)
}

fn call<T: serde::de::DeserializeOwned>(session: &mut Session, id: &str, params: &str) -> T {
    let reply = session.execute(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{id}'s reply {reply}: {e}"))
}

fn open(session: &mut Session, dir: &Path) -> StatusResult {
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    call(session, "repo.open", &format!("{{\"path\":{path}}}"))
}

/// The bars of ticket `issue` on the rows.
fn bars_of(session: &mut Session, issue: u64) -> Vec<swarmception_domain::rows::Bar> {
    let list: ListResult = call(session, "rows.list", "");
    list.rows
        .iter()
        .flat_map(Node::leaves)
        .filter_map(|l| if let Node::Ranges { key, bars, .. } = l { (key != "merged").then(|| bars.clone()) } else { None })
        .flatten()
        .filter(|b| b.ticket == Some(issue))
        .collect()
}

#[test]
fn a_load_with_no_github_remote_still_loads_the_timeline() {
    let dir = repo(None);
    let cache = temp("no-github-cache");
    let mut session = Session::new();
    session.set_github(Github { gh: PathBuf::from("/no/such/gh"), cache_dir: Some(cache.clone()) });
    let status = open(&mut session, &dir);
    assert_eq!(status.error, None, "the tracking branch read");
    assert_eq!(status.tickets, 2);
    assert!(status.issues.error.as_deref().is_some_and(|e| e.contains("no origin remote")), "{:?}", status.issues);
    assert_eq!((status.issues.github, status.issues.source, status.issues.issues), (None, None, 0));
    assert!(session.widget_timeline().unwrap().is_some(), "the timeline loads all the same");
    assert_eq!(bars_of(&mut session, 7)[0].label, "#7", "no titles: its number (#110: one bar, its phases its strip)");
    let error = session.execute("ticket.urls", r#"{"issue":7}"#).unwrap_err();
    assert_eq!(error, EngineError::Disabled { command: "ticket.urls".into(), reason: Unavailable::NoGithub });
    assert!(std::fs::read_dir(&cache).unwrap().next().is_none(), "nothing cached");
    std::fs::remove_dir_all(dir).ok();
    std::fs::remove_dir_all(cache).ok();
}

#[test]
fn a_github_remote_with_no_gh_still_loads_and_gives_the_pages() {
    let dir = repo(Some("git@github.com:o/n.git"));
    let mut session = Session::new();
    session.set_github(Github { gh: PathBuf::from("/no/such/gh"), cache_dir: None });
    let status = open(&mut session, &dir);
    assert_eq!(status.error, None);
    assert_eq!(status.issues.github.as_deref(), Some("o/n"));
    assert!(status.issues.error.as_deref().is_some_and(|e| e.contains("can't run gh")), "{:?}", status.issues);
    // The pages from the remote alone: the PR its events name.
    let urls: TicketUrlsResult = call(&mut session, "ticket.urls", r#"{"issue":7}"#);
    assert_eq!(
        urls,
        TicketUrlsResult {
            issue: 7,
            title: None,
            issue_url: "https://github.com/o/n/issues/7".into(),
            pr: Some(8),
            pr_url: Some("https://github.com/o/n/pull/8".into()),
            pull_url: "https://github.com/o/n/pull/7".into()
        }
    );
    let urls: TicketUrlsResult = call(&mut session, "ticket.urls", r#"{"issue":9}"#);
    assert_eq!((urls.pr, urls.pr_url), (None, None), "#9 names no PR");
    let merge: TicketUrlsResult = call(&mut session, "ticket.urls", r#"{"issue":8}"#);
    assert_eq!(merge.pull_url, "https://github.com/o/n/pull/8", "a merge's PR, by its number");
    std::fs::remove_dir_all(dir).ok();
}

#[cfg(unix)]
#[test]
fn issues_from_gh_reach_the_tooltips_the_labels_and_the_cache() {
    let dir = repo(Some("https://github.com/o/n.git"));
    let tools = temp("gh");
    let cache = temp("cache");
    let mut session = Session::new();
    session.set_github(Github { gh: fake_gh(&tools, &dir, None), cache_dir: Some(cache.clone()) });
    let status = open(&mut session, &dir);
    assert_eq!(status.issues.error, None, "{:?}", status.issues);
    assert_eq!((status.issues.source, status.issues.issues, status.issues.pulls), (Some(IssuesSource::Github), 3, 1));
    assert!(status.issues.fetched.is_some());
    let cached = cache.join("issues/o--n.json");
    assert_eq!(status.issues.cache.as_deref(), Some(cached.display().to_string().as_str()));
    assert!(cached.is_file(), "the issues cached");

    let bars = bars_of(&mut session, 7);
    assert_eq!(bars.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(), ["#7 Seven: the ticket's own title"], "its one bar (#110)");
    assert_eq!(bars[0].tooltip[..3], ["#7 Seven: the ticket's own title", "closed · type::task · by owner", "PR #8 The fix for seven (merged)"]);
    assert!(bars[0].tooltip[3].starts_with("claimed: 2026-10-31T03:01:00Z → 2026-10-31T03:30:00Z (29 min 00 s) by "), "{:?}", bars[0].tooltip);
    let urls: TicketUrlsResult = call(&mut session, "ticket.urls", r#"{"issue":7}"#);
    assert_eq!(urls.title.as_deref(), Some("Seven: the ticket's own title"));
    assert_eq!((urls.issue_url.as_str(), urls.pr_url.as_deref()), ("https://github.com/o/n/issues/7", Some("https://github.com/o/n/pull/8")));
    let pr: TicketUrlsResult = call(&mut session, "ticket.urls", r#"{"issue":8}"#);
    assert_eq!((pr.issue_url.as_str(), pr.pr), ("https://github.com/o/n/pull/8", Some(8)), "a PR's own number opens it");

    // The factory's crates carry the title too.
    let scene: swarmception_domain::factory::FactoryScene = call(&mut session, "factory.at", r#"{"t":1793417000}"#);
    let held = scene.benches.iter().flat_map(|b| &b.crates).find(|c| c.issue == 7).unwrap();
    assert_eq!(held.title.as_deref(), Some("Seven: the ticket's own title"));
    assert_eq!(held.tooltip[1], "open · type::task · by owner", "open at t: it closed later");

    // Titles are always on: `rows.set` has no `titles` (#34), and the bars keep the title.
    assert!(session.execute("rows.set", r#"{"titles":false}"#).is_err(), "rows.set refuses titles");
    let _: swarmception_domain::RowsSpec = call(&mut session, "rows.set", r#"{"grouping":"workers","collapse":true}"#);
    assert_eq!(bars_of(&mut session, 7)[0].label, "#7 Seven: the ticket's own title");

    // Another load with gh failing: the cache's issues, and why the fetch failed.
    let mut later = Session::new();
    later.set_github(Github { gh: fake_gh(&tools, &dir, Some("error connecting to api.github.com")), cache_dir: Some(cache.clone()) });
    let status = open(&mut later, &dir);
    assert_eq!((status.issues.source, status.issues.issues), (Some(IssuesSource::Cache), 3));
    assert!(status.issues.error.as_deref().is_some_and(|e| e.contains("error connecting")), "{:?}", status.issues);
    assert_eq!(bars_of(&mut later, 7)[0].label, "#7 Seven: the ticket's own title", "titles from the cache");
    for path in [dir, tools, cache] {
        std::fs::remove_dir_all(path).ok();
    }
}

#[cfg(unix)]
#[test]
fn in_the_background_the_issues_land_through_poll() {
    let dir = repo(Some("https://github.com/o/n.git"));
    let tools = temp("gh-bg");
    let (mut session, wakes) = Wakes::session();
    session.set_github(Github { gh: fake_gh(&tools, &dir, None), cache_dir: None });
    let opened = open(&mut session, &dir);
    assert!(opened.issues.fetching, "the issues are read in the background");
    // Polled only when woken, as the app draws: the last wake comes once the reads are over, so
    // a front end sees the issues stop fetching with no input of its own (#96).
    wakes.settle(&mut session);
    let status: StatusResult = call(&mut session, "repo.status", "");
    assert!(!status.issues.fetching);
    assert_eq!(status.error, None);
    assert!(session.has_data().is_ok(), "the branch read");
    assert_eq!((status.issues.source, status.issues.issues), (Some(IssuesSource::Github), 3));
    assert!(wakes.count() >= 3, "woken for the branch, the issues and the end of their read");
    assert_eq!(bars_of(&mut session, 7)[0].label, "#7 Seven: the ticket's own title");
    std::fs::remove_dir_all(dir).ok();
    std::fs::remove_dir_all(tools).ok();
}

/// The cache's file, as JSON to change.
fn cache_json(cache: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(cache.join("issues/o--n.json")).unwrap()).unwrap()
}

fn write_cache_json(cache: &Path, json: &serde_json::Value) {
    std::fs::write(cache.join("issues/o--n.json"), json.to_string()).unwrap();
}

#[cfg(unix)]
#[test]
fn a_load_with_a_cache_merges_what_changed_since() {
    let dir = repo(Some("https://github.com/o/n.git"));
    let tools = temp("gh-since");
    let cache = temp("cache-since");
    let mut session = Session::new();
    session.set_github(Github { gh: fake_gh(&tools, &dir, None), cache_dir: Some(cache.clone()) });
    let status = open(&mut session, &dir);
    let refresh = status.issues.refresh.unwrap();
    assert_eq!((refresh.kind, refresh.why.as_deref()), (RefreshKind::Full, Some("no cache")));
    assert_eq!(cache_json(&cache)["version"], 2);
    let urls: TicketUrlsResult = call(&mut session, "ticket.urls", r#"{"issue":12}"#);
    assert_eq!((urls.title.as_deref(), urls.pr), (Some("Never held"), None));

    // A reload: only what changed since, merged into the cache.
    let status: StatusResult = call(&mut session, "repo.reload", "");
    let refresh = status.issues.refresh.unwrap();
    assert_eq!((refresh.kind, refresh.came, refresh.changed), (RefreshKind::Incremental, 2, 2), "{refresh:?}");
    assert_eq!((status.issues.source, status.issues.issues, status.issues.pulls), (Some(IssuesSource::Github), 4, 2));
    let urls: TicketUrlsResult = call(&mut session, "ticket.urls", r#"{"issue":12}"#);
    assert_eq!((urls.title.as_deref(), urls.pr), (Some("Held at last"), Some(13)), "#13 is #12's by its branch");
    assert_eq!(bars_of(&mut session, 7)[0].label, "#7 Seven: the ticket's own title", "what didn't change stays");
    let kept = cache_json(&cache);
    assert_eq!(kept["issues"].as_array().map(Vec::len), Some(4), "the merge cached");
    assert_eq!(kept["validator"]["etag"], "W/\"e1\"");

    // Another session from the cache: the same issues before GitHub answers, then nothing new.
    let mut later = Session::new();
    later.set_github(Github { gh: fake_gh(&tools, &dir, None), cache_dir: Some(cache.clone()) });
    let status = open(&mut later, &dir);
    assert_eq!((status.issues.refresh.unwrap().kind, status.issues.issues), (RefreshKind::Incremental, 4));
    // The newest change is where it was: the same request, sent with its ETag, says nothing changed.
    let status: StatusResult = call(&mut later, "repo.reload", "");
    assert_eq!((status.issues.refresh.unwrap().kind, status.issues.issues), (RefreshKind::Unchanged, 4));
    assert!(std::fs::read_to_string(tools.join("args")).unwrap().lines().last().unwrap().contains("If-None-Match: W/\"e1\""));
    assert_eq!(call::<TicketUrlsResult>(&mut later, "ticket.urls", r#"{"issue":12}"#).pr, Some(13));
    for path in [dir, tools, cache] {
        std::fs::remove_dir_all(path).ok();
    }
}

#[cfg(unix)]
#[test]
fn a_stale_old_or_corrupt_cache_is_fetched_in_full() {
    let dir = repo(Some("https://github.com/o/n.git"));
    let tools = temp("gh-full");
    let cache = temp("cache-full");
    let github = Github { gh: fake_gh(&tools, &dir, None), cache_dir: Some(cache.clone()) };
    let load = |why: &str| {
        let mut session = Session::new();
        session.set_github(github.clone());
        let status = open(&mut session, &dir);
        let refresh = status.issues.refresh.clone().unwrap();
        assert_eq!((refresh.kind, refresh.why.as_deref()), (RefreshKind::Full, Some(why)), "{:?}", status.issues);
        assert_eq!((status.issues.issues, status.issues.error.as_deref()), (3, None));
    };
    load("no cache");
    let fresh = cache_json(&cache);

    let mut stale = fresh.clone();
    stale["full"] = serde_json::json!(fresh["full"].as_i64().unwrap() - 8 * 24 * 3600);
    write_cache_json(&cache, &stale);
    load("a week since the last full fetch");

    let mut older = fresh;
    older["version"] = serde_json::json!(1);
    write_cache_json(&cache, &older);
    load("the cache is version 1");
    assert_eq!(cache_json(&cache)["version"], 2, "written again as this version");

    std::fs::write(cache.join("issues/o--n.json"), "{ not a cache").unwrap();
    load("no cache");
    assert!(cache_json(&cache)["issues"].is_array(), "the corrupt cache replaced");
    for path in [dir, tools, cache] {
        std::fs::remove_dir_all(path).ok();
    }
}

#[cfg(unix)]
#[test]
fn a_gh_error_leaves_the_cache_as_it_was() {
    let dir = repo(Some("https://github.com/o/n.git"));
    let tools = temp("gh-error");
    let cache = temp("cache-error");
    let mut session = Session::new();
    session.set_github(Github { gh: fake_gh(&tools, &dir, None), cache_dir: Some(cache.clone()) });
    let _ = open(&mut session, &dir);
    let before = std::fs::read(cache.join("issues/o--n.json")).unwrap();

    let mut failing = Session::new();
    failing.set_github(Github { gh: fake_gh(&tools, &dir, Some("HTTP 502: Bad Gateway")), cache_dir: Some(cache.clone()) });
    let status = open(&mut failing, &dir);
    assert_eq!((status.issues.source, status.issues.issues, status.issues.refresh.as_ref()), (Some(IssuesSource::Cache), 3, None), "the cache's, as it was");
    assert!(status.issues.error.as_deref().is_some_and(|e| e.contains("Bad Gateway")), "{:?}", status.issues);
    let status: StatusResult = call(&mut failing, "repo.reload", "");
    assert_eq!(status.issues.issues, 3);
    assert_eq!(std::fs::read(cache.join("issues/o--n.json")).unwrap(), before, "the cache untouched");
    for path in [dir, tools, cache] {
        std::fs::remove_dir_all(path).ok();
    }
}
