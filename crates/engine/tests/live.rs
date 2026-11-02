//! Watching a live swarm (#54): a clone of a local bare remote whose tracking branch and main
//! gain events between checks; `live.check` reads again only when one moved, `live.get` says
//! what each found and cost, errors back off and leave the timeline alone, and
//! `timeline.carryBars` finds the bars of the rows before in the rows after.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use swarmception_engine::factory_cmds::BarRef;
use swarmception_engine::live::LiveStatus;
use swarmception_engine::live_cmds::CarryResult;
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_engine::{EngineError, Github, Session};
use swarmception_model::Timestamp;

fn temp(what: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("swarmception-live-{what}-{}-{nanos}-{n}", std::process::id()))
}

fn git(dir: &Path, args: &[&str]) {
    let mut all = vec!["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"];
    all.extend_from_slice(args);
    let out = std::process::Command::new("git").current_dir(dir).args(&all).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// An event line at `t`.
fn line(t: Timestamp, rest: &str) -> String {
    format!("{{{rest},\"t\":\"{t}\"}}\n")
}

/// A swarm's repository (`work`), a bare remote cloned from it, and a clone of that, the one
/// the session opens. The swarm's worker began at `start`, claimed #7 and began work on it.
struct Swarm {
    work: PathBuf,
    remote: PathBuf,
    clone: PathBuf,
    start: Timestamp,
}

impl Swarm {
    fn new(start: Timestamp) -> Self {
        let work = temp("work");
        std::fs::create_dir_all(&work).unwrap();
        git(&work, &["init", "-q", "-b", "main"]);
        git(&work, &["commit", "-q", "--allow-empty", "-m", "start"]);
        let swarm = Self { remote: work.with_extension("git"), clone: work.with_extension("clone"), work, start };
        swarm.merge(7);
        git(&swarm.work, &["switch", "-q", "--orphan", "tracking"]);
        std::fs::create_dir_all(swarm.work.join("tracking/mini-aa1111")).unwrap();
        let events = [
            line(start, r#""event":"session.start","machine":"mini","name":"worker-1","role":"worker""#),
            line(swarm.at(60), r#""event":"claim","issue":7,"mr":8"#),
            line(swarm.at(120), r#""event":"work","issue":7"#),
        ];
        std::fs::write(swarm.file(), events.concat()).unwrap();
        git(&swarm.work, &["add", "."]);
        git(&swarm.work, &["commit", "-q", "-m", "events"]);
        git(&swarm.work, &["clone", "-q", "--bare", swarm.work.to_str().unwrap(), swarm.remote.to_str().unwrap()]);
        git(&swarm.work, &["clone", "-q", "--no-checkout", swarm.remote.to_str().unwrap(), swarm.clone.to_str().unwrap()]);
        swarm
    }

    fn at(&self, seconds: i64) -> Timestamp {
        Timestamp(self.start.0 + seconds)
    }

    fn file(&self) -> PathBuf {
        self.work.join("tracking/mini-aa1111/work.jsonl")
    }

    /// A merge onto `work`'s main of PR `pr`, from wherever `work` is.
    fn merge(&self, pr: u64) {
        let topic = format!("topic-{pr}");
        git(&self.work, &["switch", "-q", "-c", &topic, "main"]);
        git(&self.work, &["commit", "-q", "--allow-empty", "-m", "a change"]);
        git(&self.work, &["switch", "-q", "main"]);
        git(&self.work, &["merge", "-q", "--no-ff", "-m", &format!("Merge pull request #{pr} from them/{topic}"), &topic]);
    }

    /// The swarm logs `rest` at `seconds` and pushes its tracking branch.
    fn log(&self, seconds: i64, rest: &str) {
        let mut text = std::fs::read_to_string(self.file()).unwrap();
        text.push_str(&line(self.at(seconds), rest));
        std::fs::write(self.file(), text).unwrap();
        git(&self.work, &["commit", "-q", "-am", "an event"]);
        git(&self.work, &["push", "-q", self.remote.to_str().unwrap(), "tracking"]);
    }

    /// A merge onto main, pushed; back on the tracking branch.
    fn land(&self, pr: u64) {
        self.merge(pr);
        git(&self.work, &["push", "-q", self.remote.to_str().unwrap(), "main"]);
        git(&self.work, &["switch", "-q", "tracking"]);
    }

    fn open(&self, session: &mut Session) -> StatusResult {
        // No `gh`, no cache: the issues stay out of the way (the remote isn't on GitHub anyway).
        session.set_github(Github { gh: self.work.join("no-gh"), cache_dir: None });
        let path = serde_json::to_string(&self.clone.display().to_string()).unwrap();
        call(session, "repo.open", &format!("{{\"path\":{path}}}"))
    }
}

impl Drop for Swarm {
    fn drop(&mut self) {
        for dir in [&self.work, &self.remote, &self.clone] {
            std::fs::remove_dir_all(dir).ok();
        }
    }
}

fn call<T: serde::de::DeserializeOwned>(session: &mut Session, id: &str, params: &str) -> T {
    let reply = session.execute(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{id}'s reply {reply}: {e}"))
}

fn old() -> Timestamp {
    Timestamp::parse("2026-10-31T03:00:00Z").unwrap()
}

fn now() -> Timestamp {
    swarmception_source::github::now()
}

#[test]
fn a_live_check_reads_again_only_when_tracking_or_main_moved() {
    let swarm = Swarm::new(old());
    let mut session = Session::new();
    let opened = swarm.open(&mut session);
    assert_eq!((opened.events, opened.main_merges), (3, 1));
    assert_eq!(opened.reference.as_deref(), Some("refs/remotes/origin/tracking"));
    let generation = session.revision().0;

    let quiet: LiveStatus = call(&mut session, "live.check", "");
    assert_eq!((quiet.checks, quiet.reloads, quiet.error.clone()), (1, 0, None));
    let cost = quiet.cost.unwrap();
    assert_eq!((cost.read_ms, cost.build_ms, cost.new_events), (None, None, None), "nothing moved: nothing read");
    assert_eq!(session.revision().0, generation, "the rows stay as they were");

    // The swarm submits #7: the tracking branch moves.
    swarm.log(600, r#""event":"submit","issue":7,"mr":8"#);
    let moved: LiveStatus = call(&mut session, "live.check", "");
    assert_eq!((moved.checks, moved.reloads, moved.failures), (2, 1, 0));
    let cost = moved.cost.unwrap();
    assert!(cost.read_ms.is_some() && cost.build_ms.is_some());
    assert_eq!(cost.new_events, Some(1));
    assert!(moved.last_new.is_some());
    assert_ne!(session.revision().0, generation, "new rows");
    let status: StatusResult = call(&mut session, "repo.status", "");
    assert_eq!(status.events, 4);

    // A merge onto main, and nothing on the tracking branch: main moved, so it reads again.
    swarm.land(8);
    let merged: LiveStatus = call(&mut session, "live.check", "");
    assert_eq!((merged.checks, merged.reloads), (3, 2));
    assert_eq!(merged.cost.unwrap().new_events, Some(0));
    let status: StatusResult = call(&mut session, "repo.status", "");
    assert_eq!((status.events, status.main_merges), (4, 2));
}

#[test]
fn live_is_on_by_default_only_for_a_swarm_at_work() {
    let fresh = Swarm::new(Timestamp(now().0 - 10 * 60));
    let mut session = Session::new();
    fresh.open(&mut session);
    let live: LiveStatus = call(&mut session, "live.get", "");
    assert!(live.on && live.default, "the latest event is minutes old: {live:?}");
    assert_eq!((live.interval_seconds, live.next_in_seconds.map(|s| s <= 30)), (30, Some(true)));

    let stale = Swarm::new(old());
    let mut session = Session::new();
    stale.open(&mut session);
    let live: LiveStatus = call(&mut session, "live.get", "");
    assert!(!live.on && live.default);
    assert_eq!(live.next_in_seconds, None);

    // A choice stands over the default, and across opening another repository.
    let chosen: LiveStatus = call(&mut session, "live.set", r#"{"on": true, "intervalSeconds": 10}"#);
    assert!(chosen.on && !chosen.default);
    assert_eq!((chosen.interval_seconds, chosen.next_in_seconds), (10, Some(0)), "turned on: at once");
    let _: LiveStatus = call(&mut session, "live.set", r#"{"on": false}"#);
    fresh.open(&mut session);
    let live: LiveStatus = call(&mut session, "live.get", "");
    assert!(!live.on && !live.default && live.interval_seconds == 10);
}

#[test]
fn a_failed_check_backs_off_and_leaves_the_timeline_alone() {
    let swarm = Swarm::new(old());
    let mut session = Session::new();
    swarm.open(&mut session);
    let generation = session.revision().0;
    // The remote goes away.
    std::fs::remove_dir_all(&swarm.remote).unwrap();
    let failed: LiveStatus = call(&mut session, "live.check", "");
    assert_eq!((failed.checks, failed.reloads, failed.failures), (1, 0, 1));
    assert!(failed.error.as_deref().is_some_and(|e| e.contains("fetch")), "{failed:?}");
    let status: StatusResult = call(&mut session, "repo.status", "");
    assert_eq!((status.error, status.events), (None, 3), "the timeline as it was, no error of its own");
    assert_eq!(session.revision().0, generation);
    let _: LiveStatus = call(&mut session, "live.set", r#"{"on": true}"#);
    let again: LiveStatus = call(&mut session, "live.check", "");
    assert_eq!(again.failures, 2);
    // On, two failures in a row: the next check is four intervals on.
    assert!(again.next_in_seconds.is_some_and(|s| (110..=120).contains(&s)), "{again:?}");

    // No repository: the check can't run; a bad interval or field is refused, never a panic.
    let mut empty = Session::new();
    assert!(matches!(empty.execute("live.check", ""), Err(EngineError::Disabled { .. })));
    for bad in [r#"{"intervalSeconds": 1}"#, r#"{"intervalSeconds": 99999}"#] {
        assert!(matches!(empty.execute("live.set", bad), Err(EngineError::Invalid { .. })), "{bad}");
    }
    assert!(matches!(empty.execute("live.set", r#"{"often": true}"#), Err(EngineError::BadParams { .. })));
    let live: LiveStatus = call(&mut empty, "live.set", r#"{"on": true}"#);
    assert!(live.on, "on with no repository: it waits for one");
}

#[test]
fn a_session_with_a_waker_checks_on_the_interval_in_the_background() {
    let swarm = Swarm::new(old());
    let woken = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = std::sync::Arc::clone(&woken);
    let mut session = Session::with_waker(std::sync::Arc::new(move || {
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }));
    swarm.open(&mut session);
    let wait = |session: &mut Session| {
        let deadline = Instant::now() + Duration::from_secs(20);
        while session.job().is_some() && Instant::now() < deadline {
            session.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(session.job().is_none(), "the read landed");
    };
    wait(&mut session);
    assert_eq!(session.live_wait(Instant::now()), None, "off by default: an old swarm");

    swarm.log(600, r#""event":"submit","issue":7,"mr":8"#);
    let _: LiveStatus = call(&mut session, "live.set", r#"{"on": true, "intervalSeconds": 5}"#);
    assert_eq!(session.live_wait(Instant::now()), Some(Duration::ZERO));
    assert!(session.poll(), "due at once: a check starts");
    let checking: LiveStatus = call(&mut session, "live.get", "");
    assert!(checking.checking && checking.next_in_seconds.is_none());
    // A Fetch and Reload stays possible while a live check goes on, and takes its place.
    let fetch = session.execute("repo.fetch", "");
    assert!(fetch.is_ok(), "{fetch:?}");
    assert_eq!(session.job(), Some(swarmception_engine::session::Job::Fetch));
    wait(&mut session);
    assert!(woken.load(std::sync::atomic::Ordering::Relaxed) > 0);
    let status: StatusResult = call(&mut session, "repo.status", "");
    assert_eq!(status.events, 4, "the fetch brought the new event");

    // The next check, one interval on.
    let later = Instant::now() + Duration::from_secs(6);
    assert!(!session.tick(Instant::now()), "not due yet");
    swarm.log(700, r#""event":"stop","issue":7,"reason":"done""#);
    assert!(session.tick(later));
    wait(&mut session);
    let live: LiveStatus = call(&mut session, "live.get", "");
    assert_eq!((live.checks, live.reloads, live.error.clone()), (1, 1, None));
    let status: StatusResult = call(&mut session, "repo.status", "");
    assert_eq!(status.events, 5);
}

#[test]
fn bars_carry_across_a_reload_and_a_regrouping() {
    let swarm = Swarm::new(old());
    let mut session = Session::new();
    swarm.open(&mut session);
    let first = session.revision().0;
    let bars: swarmception_engine::factory_cmds::TicketBarsResult = call(&mut session, "timeline.ticketBars", r#"{"issue": 7}"#);
    assert!(!bars.bars.is_empty());
    let asked = serde_json::json!({ "generation": first, "bars": bars.bars }).to_string();
    let same: CarryResult = call(&mut session, "timeline.carryBars", &asked);
    assert_eq!((same.generation, &same.bars), (first, &bars.bars), "the same rows: the same bars");

    // Another ticket, claimed before #7's work in the log's time but arriving later.
    swarm.log(30, r#""event":"claim","issue":5,"mr":6"#);
    let _: LiveStatus = call(&mut session, "live.check", "");
    let after: swarmception_engine::factory_cmds::TicketBarsResult = call(&mut session, "timeline.ticketBars", r#"{"issue": 7}"#);
    let carried: CarryResult = call(&mut session, "timeline.carryBars", &asked);
    assert_eq!(carried.generation, session.revision().0);
    assert_eq!(carried.bars, after.bars, "#7's bars, wherever the new ticket put them");

    // By worker: the same bars on other rows.
    let _: serde_json::Value = call(&mut session, "rows.set", r#"{"grouping": "workers"}"#);
    let by_worker: swarmception_engine::factory_cmds::TicketBarsResult = call(&mut session, "timeline.ticketBars", r#"{"issue": 7}"#);
    let carried: CarryResult = call(&mut session, "timeline.carryBars", &asked);
    assert_eq!(carried.bars, by_worker.bars);
    assert_ne!(by_worker.bars, bars.bars, "other rows");

    // A bar the rows never had is left out; rows long gone are refused.
    let missing = serde_json::json!({ "generation": session.revision().0, "bars": [BarRef { track: "nope".into(), range: 0 }] }).to_string();
    let none: CarryResult = call(&mut session, "timeline.carryBars", &missing);
    assert!(none.bars.is_empty());
    for _ in 0..10 {
        let _: serde_json::Value = call(&mut session, "rows.set", r#"{"collapse": true}"#);
        let _: serde_json::Value = call(&mut session, "rows.set", r#"{"collapse": false}"#);
    }
    assert!(matches!(session.execute("timeline.carryBars", &asked), Err(EngineError::Invalid { .. })));
}
