//! Opening a repository (#69, G8): `repo.open` refuses a missing folder, a file, a plain folder
//! and a repository with no tracking branch before anything changes (the repository open stays
//! open), and `repo.recent` lists what was opened, newest first, each once, at most ten.

use std::path::{Path, PathBuf};

use swarmception_engine::error::Refusal;
use swarmception_engine::repo_cmds::{MAX_RECENT, RecentResult, StatusResult};
use swarmception_engine::{EngineError, Github, Session};

mod support;
use support::Wakes;

fn temp(what: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("worker29-open-{what}-{}-{nanos}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // The top level as git says it (on macOS the temporary directory is behind a symlink).
    let dir = std::fs::canonicalize(&dir).unwrap();
    // Git for Windows prints DOS paths with forward slashes, not Rust's verbatim prefix.
    #[cfg(windows)]
    let dir = PathBuf::from(dir.to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/"));
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let mut all = vec!["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"];
    all.extend_from_slice(args);
    let out = std::process::Command::new("git").current_dir(dir).args(&all).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A swarm's clone: a tracking branch with one worker's session and claim of `issue`.
fn swarm(what: &str, issue: u64) -> PathBuf {
    let dir = temp(what);
    std::fs::create_dir_all(dir.join("tracking/mini-aa1111")).unwrap();
    std::fs::write(
        dir.join("tracking/mini-aa1111/work.jsonl"),
        format!(
            "{}\n{}\n",
            r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
            format_args!(r#"{{"event":"claim","issue":{issue},"mr":8,"t":"2026-10-31T03:01:00Z"}}"#)
        ),
    )
    .unwrap();
    git(&dir, &["init", "-q", "-b", "tracking"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "events"]);
    dir
}

fn session() -> Session {
    let mut session = Session::new();
    session.set_github(Github { gh: PathBuf::from("/no/such/gh"), cache_dir: None });
    session
}

fn open(session: &mut Session, path: &Path) -> Result<StatusResult, EngineError> {
    let params = serde_json::json!({ "path": path.display().to_string() }).to_string();
    session.execute("repo.open", &params).map(|reply| serde_json::from_str(&reply).unwrap())
}

fn recent(session: &mut Session) -> RecentResult {
    serde_json::from_str(&session.execute("repo.recent", "").unwrap()).unwrap()
}

fn refusal(result: Result<StatusResult, EngineError>) -> Refusal {
    match result {
        Err(EngineError::Invalid { refusal }) => refusal,
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn a_folder_that_isnt_a_swarms_clone_is_refused_and_the_open_one_stays() {
    let good = swarm("good", 7);
    let mut session = session();
    let opened = open(&mut session, &good).unwrap();
    assert_eq!(opened.tickets, 1);
    let revision = session.revision();
    let opens = session.opens();

    let plain = temp("plain");
    let file = plain.join("a-file.txt");
    std::fs::write(&file, "not a folder").unwrap();
    let untracked = temp("untracked");
    git(&untracked, &["init", "-q", "-b", "main"]);
    git(&untracked, &["commit", "-q", "--allow-empty", "-m", "start"]);
    let missing = plain.join("gone");

    assert!(matches!(refusal(open(&mut session, &missing)), Refusal::NoSuchFolder { .. }));
    assert!(matches!(refusal(open(&mut session, &file)), Refusal::NotADirectory { .. }));
    let not_repo = refusal(open(&mut session, &plain));
    assert!(matches!(&not_repo, Refusal::NotARepository { path, .. } if *path == plain.display().to_string()), "{not_repo:?}");
    assert!(not_repo.to_string().contains("is not in a git repository"), "{not_repo}");
    let no_branch = refusal(open(&mut session, &untracked));
    assert!(matches!(&no_branch, Refusal::NoTrackingBranch { path } if *path == untracked.display().to_string()), "{no_branch:?}");
    assert!(no_branch.to_string().contains("has no tracking branch"), "{no_branch}");
    assert!(matches!(refusal(open(&mut session, Path::new(""))), Refusal::NoSuchFolder { .. }));

    // Nothing changed: the same repository, read, its rows, no new open, the recent list as it was.
    assert_eq!((session.revision(), session.opens()), (revision, opens));
    let status: StatusResult = serde_json::from_str(&session.execute("repo.status", "").unwrap()).unwrap();
    assert_eq!((status.repo.as_deref(), status.tickets, status.error), (Some(good.display().to_string().as_str()), 1, None));
    assert_eq!(recent(&mut session).repos.iter().map(|r| r.path.clone()).collect::<Vec<_>>(), [good.display().to_string()]);
    for dir in [good, plain, untracked] {
        std::fs::remove_dir_all(dir).ok();
    }
}

#[test]
fn another_repository_replaces_the_one_open_and_goes_first_in_the_recent_list() {
    let (a, b) = (swarm("a", 7), swarm("b", 9));
    let mut session = session();
    assert!(recent(&mut session).repos.is_empty());
    // A folder inside a clone opens the clone, by its top level.
    std::fs::create_dir_all(a.join("tracking/deeper")).unwrap();
    let inside = open(&mut session, &a.join("tracking/deeper")).unwrap();
    assert_eq!(inside.repo.as_deref(), Some(a.display().to_string().as_str()));
    assert_eq!(session.repo(), Some(a.as_path()));
    let opens = session.opens();
    let opened = open(&mut session, &b).unwrap();
    assert_eq!(session.opens(), opens + 1);
    assert_eq!((opened.repo.as_deref(), opened.tickets), (Some(b.display().to_string().as_str()), 1));
    let bars: swarmception_engine::factory_cmds::TicketBarsResult =
        serde_json::from_str(&session.execute("timeline.ticketBars", r#"{"issue": 9}"#).unwrap()).unwrap();
    assert!(!bars.bars.is_empty(), "b's ticket");
    let list = recent(&mut session);
    assert_eq!(list.repos.iter().map(|r| r.path.clone()).collect::<Vec<_>>(), [b.display().to_string(), a.display().to_string()]);
    assert_eq!(list.repos[0].name, b.file_name().unwrap().to_string_lossy());
    assert!(!list.repos[0].missing);
    // a again: first, once.
    open(&mut session, &a).unwrap();
    assert_eq!(recent(&mut session).repos.iter().map(|r| r.path.clone()).collect::<Vec<_>>(), [a.display().to_string(), b.display().to_string()]);

    // What the app kept from before goes after, each once, ten at most; a folder since gone is
    // listed as missing, and opening it is refused.
    let kept: Vec<PathBuf> = std::iter::once(b.clone()).chain((0..20).map(|n| PathBuf::from(format!("/no/such/swarm-{n}")))).collect();
    session.remember_all(kept);
    let list = recent(&mut session);
    assert_eq!(list.repos.len(), MAX_RECENT);
    assert_eq!((list.repos[0].path.as_str(), list.repos[1].path.as_str()), (a.display().to_string().as_str(), b.display().to_string().as_str()));
    assert_eq!(list.repos[2].path, "/no/such/swarm-0");
    assert!(list.repos[2].missing && list.repos[2].name == "swarm-0");
    assert!(matches!(refusal(open(&mut session, Path::new(&list.repos[2].path))), Refusal::NoSuchFolder { .. }));

    // Clear Menu: empty, the repository still open.
    let cleared: RecentResult = serde_json::from_str(&session.execute("repo.clearRecent", "").unwrap()).unwrap();
    assert!(cleared.repos.is_empty() && recent(&mut session).repos.is_empty());
    assert_eq!(session.repo(), Some(a.as_path()));
    assert!(session.execute("repo.recent", r#"{"all": true}"#).is_err(), "unknown params refused");
    for dir in [a, b] {
        std::fs::remove_dir_all(dir).ok();
    }
}

/// A session with a waker (the app's) switching repositories while a read of the old one goes
/// on: the old read is dropped, the new one lands.
#[test]
fn an_open_while_a_read_goes_on_lands_the_new_repository() {
    let (a, b) = (swarm("busy-a", 7), swarm("busy-b", 9));
    let (mut session, wakes) = Wakes::session();
    session.set_github(Github { gh: PathBuf::from("/no/such/gh"), cache_dir: None });
    open(&mut session, &a).unwrap();
    open(&mut session, &b).unwrap();
    wakes.settle(&mut session);
    let status: StatusResult = serde_json::from_str(&session.execute("repo.status", "").unwrap()).unwrap();
    assert_eq!(status.repo.as_deref(), Some(b.display().to_string().as_str()));
    let bars: swarmception_engine::factory_cmds::TicketBarsResult =
        serde_json::from_str(&session.execute("timeline.ticketBars", r#"{"issue": 9}"#).unwrap()).unwrap();
    assert!(!bars.bars.is_empty(), "b's rows, not a's");
    for dir in [a, b] {
        std::fs::remove_dir_all(dir).ok();
    }
}

/// #161: with a waker (a front end's session), `repo.open` checks the folder in the background:
/// the answer comes at once saying `opening`, the repository open stays open meanwhile, and the
/// check lands through `poll`, opening the repository or keeping why not in `repo.status`'s
/// `refused`. A second `repo.open` before the first lands takes its place.
#[test]
fn a_front_ends_open_checks_the_folder_in_the_background() {
    let (a, b) = (swarm("bg-a", 7), swarm("bg-b", 9));
    let plain = temp("bg-plain");
    let (mut session, wakes) = Wakes::session();
    session.set_github(Github { gh: PathBuf::from("/no/such/gh"), cache_dir: None });
    let status = |session: &mut Session| -> StatusResult { serde_json::from_str(&session.execute("repo.status", "").unwrap()).unwrap() };

    let asked = open(&mut session, &a).unwrap();
    assert_eq!((asked.opening.as_deref(), asked.repo, session.opens()), (Some(a.display().to_string().as_str()), None, 0), "nothing opened yet");
    wakes.settle(&mut session);
    let opened = status(&mut session);
    assert_eq!((opened.repo.as_deref(), opened.tickets, opened.opening, opened.refused), (Some(a.display().to_string().as_str()), 1, None, None));
    assert_eq!(session.opens(), 1);

    // Refused once it lands: a stays open, the refusal kept until the next open.
    let asked = open(&mut session, &plain).unwrap();
    assert!(asked.opening.is_some() && asked.refused.is_none());
    wakes.settle(&mut session);
    let refused = status(&mut session);
    assert_eq!((refused.repo.as_deref(), refused.tickets, session.opens()), (Some(a.display().to_string().as_str()), 1, 1));
    let kept = refused.refused.unwrap();
    assert_eq!(kept.path, plain.display().to_string());
    assert!(matches!(kept.refusal, Refusal::NotARepository { .. }), "{kept:?}");

    // Two asked one after the other: the second takes the first's place.
    open(&mut session, &plain).unwrap();
    let asked = open(&mut session, &b).unwrap();
    assert_eq!((asked.opening.as_deref(), asked.refused), (Some(b.display().to_string().as_str()), None));
    wakes.settle(&mut session);
    let now = status(&mut session);
    assert_eq!((now.repo.as_deref(), now.refused, session.opens()), (Some(b.display().to_string().as_str()), None, 2));
    for dir in [a, b, plain] {
        std::fs::remove_dir_all(dir).ok();
    }
}
