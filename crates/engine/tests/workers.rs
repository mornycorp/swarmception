//! `workers.compare` (#67) as a front end runs it: by id with JSON params, the reply read back as
//! its result type, over a real repository whose tracking branch holds workers at two efforts;
//! over this clone's own tracking branch, when it has one; and over the swarms
//! `SWARMCEPTION_WORKERS_REPOS` names (paths, `:`-separated), printing each one's comparison.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swarmception_domain::Stats;
use swarmception_domain::details::{Details, Subject};
use swarmception_domain::workers::{Comparison, Figures, Measure, THIN};
use swarmception_engine::Session;
use swarmception_engine::repo_cmds::StatusResult;
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

/// worker-1 (opus, high) lands #7 after a send back and #9; worker-2 (opus, low) lands #11 after
/// a failed `ready` check and its rerun.
fn repo() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-workers-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [
        (
            "mini-aa1111",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","model":"claude-opus-5-5","effort":"high","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:02:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:12:00Z"}"#,
                r#"{"event":"claim","issue":9,"mr":10,"t":"2026-10-31T03:13:00Z"}"#,
                r#"{"event":"work","issue":9,"t":"2026-10-31T03:14:00Z"}"#,
                r#"{"event":"submit","issue":9,"mr":10,"t":"2026-10-31T03:24:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:25:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:30:00Z"}"#,
                r#"{"event":"session.stop","t":"2026-10-31T03:50:00Z"}"#,
            ],
        ),
        (
            "mini-bb2222",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-2","role":"worker","model":"claude-opus-5-5","effort":"low","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":11,"mr":12,"t":"2026-10-31T03:02:00Z"}"#,
                r#"{"event":"work","issue":11,"t":"2026-10-31T03:03:00Z"}"#,
                r#"{"event":"sub.start","name":"ready","sub":"ready-b1","t":"2026-10-31T03:15:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=1; cause=clippy","sub":"ready-b1","t":"2026-10-31T03:18:00Z"}"#,
                r#"{"event":"sub.start","name":"ready","sub":"ready-b1-rerun","t":"2026-10-31T03:20:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0","sub":"ready-b1-rerun","t":"2026-10-31T03:25:00Z"}"#,
                r#"{"event":"submit","issue":11,"mr":12,"t":"2026-10-31T03:26:00Z"}"#,
                r#"{"event":"session.stop","t":"2026-10-31T03:50:00Z"}"#,
            ],
        ),
        (
            "mini-cc3333",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"sendback","issue":7,"mr":8,"note":"cause=conflict: src/lib.rs","t":"2026-10-31T03:20:00Z"}"#,
                r#"{"event":"merge","issue":9,"mr":10,"t":"2026-10-31T03:33:00Z"}"#,
                r#"{"event":"merge","issue":7,"mr":8,"t":"2026-10-31T03:40:00Z"}"#,
                r#"{"event":"merge","issue":11,"mr":12,"t":"2026-10-31T03:45:00Z"}"#,
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
fn each_worker_and_each_model_and_effort() {
    let dir = repo();
    let mut session = Session::new();
    assert_eq!(session.execute("workers.compare", "").unwrap_err().code(), "disabled", "no repository yet");
    open(&mut session, &dir);
    let all: Comparison = call(&mut session, "workers.compare", "");
    let labels: Vec<&str> = all.groups.iter().map(|g| g.label.as_str()).collect();
    assert_eq!(labels, ["opus 5.5 · high", "opus 5.5 · low"], "more effort first; no coordinator");
    let high = &all.groups[0].workers[0];
    assert_eq!((high.name.as_str(), high.model.as_deref(), high.effort.as_deref()), ("worker-1", Some("claude-opus-5-5"), Some("high")));
    let f = &high.figures;
    assert_eq!((f.claimed, f.landed, f.sendbacks, f.ready_runs), (2, 2, 1, 0));
    assert_eq!(f.sendback_causes.iter().map(|c| (c.cause.as_str(), c.count)).collect::<Vec<_>>(), [("conflict", 1)]);
    assert_eq!((f.on_shift_seconds, f.active_seconds), (50 * 60, 30 * 60), "idle from its last submit at 03:30 to its stop");
    assert_eq!(f.landed_per_active_hour, Measure { value: Some(4.0), samples: 2, thin: true }, "2 in 30 active minutes, thin");
    assert_eq!(f.ready_pass_rate, Measure { value: None, samples: 0, thin: true });
    assert_eq!(high.tickets, [7, 9]);
    let low = &all.groups[1].workers[0].figures;
    assert_eq!((low.landed, low.ready_runs, low.ready_passed, low.ready_failed, low.ready_pass_rate.value), (1, 2, 1, 1, Some(0.5)));
    assert_eq!(all.all.landed, 3);
    assert_eq!(all.thin_below, THIN);

    // The pane's click: the worker's session tells its story.
    let Subject::Session { instance, .. } = &high.subject else { panic!("a session: {:?}", high.subject) };
    let story: Details = call(&mut session, "details.get", &format!("{{\"session\":\"{instance}\"}}"));
    assert!(matches!(&story.subject, Subject::Session { instance: told, .. } if told == instance), "{:?}", story.subject);

    // A window: only #9's merge is in it.
    let early: Comparison = call(&mut session, "workers.compare", r#"{"start":"2026-10-31T03:00:00Z","end":"2026-10-31T03:35:00Z"}"#);
    let landed: Vec<usize> = early.groups.iter().flat_map(|g| &g.workers).map(|w| w.figures.landed).collect();
    assert_eq!(landed, [1, 0]);

    for bad in [r#"{"start":"teatime"}"#, r#"{"start":"2026-10-31T04:00:00Z","end":"2026-10-31T03:00:00Z"}"#] {
        assert_eq!(session.execute("workers.compare", bad).unwrap_err().code(), "invalid", "{bad}");
    }
    assert_eq!(session.execute("workers.compare", r#"{"nope":1}"#).unwrap_err().code(), "bad_params");
    std::fs::remove_dir_all(dir).ok();
}

/// A swarm's workers as `workers.compare` reads them: consistent with itself and with
/// `stats.get`'s workers.
fn check_swarm(dir: &Path) -> Comparison {
    let mut session = Session::new();
    let status = open(&mut session, dir);
    assert_eq!(status.error, None, "{}", dir.display());
    let compared: Comparison = call(&mut session, "workers.compare", "");
    let stats: Stats = call(&mut session, "stats.get", "");
    let mut sum = Figures::default();
    let mut by_name: BTreeMap<(String, String), usize> = BTreeMap::new();
    for group in &compared.groups {
        let figures = &group.figures;
        assert_eq!(figures.workers, group.workers.len(), "{}", group.label);
        assert_eq!(figures.landed, group.workers.iter().map(|w| w.figures.landed).sum::<usize>(), "{}", group.label);
        assert_eq!(figures.active_seconds, group.workers.iter().map(|w| w.figures.active_seconds).sum::<i64>(), "{}", group.label);
        for measure in [figures.landed_per_active_hour, figures.claim_to_merge_seconds, figures.working_seconds, figures.ready_pass_rate] {
            assert_eq!(measure.thin, measure.samples < THIN, "{}", group.label);
        }
        for worker in &group.workers {
            let f = &worker.figures;
            assert!(f.active_seconds <= f.on_shift_seconds, "{}", worker.instance);
            assert_eq!(f.sendback_causes.iter().map(|c| c.count).sum::<usize>(), f.sendbacks, "{}", worker.instance);
            assert!(f.ready_runs >= f.ready_passed + f.ready_failed, "{}", worker.instance);
            assert!(f.claim_to_merge_seconds.samples <= f.landed && f.working_seconds.samples <= f.landed, "{}", worker.instance);
            *by_name.entry((worker.machine.clone(), worker.name.clone())).or_default() += f.landed;
        }
        sum.landed += figures.landed;
        sum.ready_runs += figures.ready_runs;
        sum.sendbacks += figures.sendbacks;
    }
    assert_eq!((sum.landed, sum.ready_runs, sum.sendbacks), (compared.all.landed, compared.all.ready_runs, compared.all.sendbacks));
    for worker in &stats.workers {
        assert_eq!(by_name.get(&(worker.machine.clone(), worker.name.clone())).copied().unwrap_or(0), worker.landed, "{}: its instances' landed", worker.name);
    }
    // A coordinator's own `ready` checks (its progress PRs) and a ticket only a coordinator held
    // are no worker's.
    assert!(compared.all.landed <= stats.landed && compared.all.sendbacks <= stats.sendbacks && compared.all.ready_runs <= stats.ready.runs);
    compared
}

fn print_comparison(name: &str, compared: &Comparison) {
    let m = |m: Measure, scale: f64, unit: &str| {
        let thin = if m.thin { " thin" } else { "" };
        m.value.map_or_else(|| format!("– (n={})", m.samples), |v| format!("{:.2}{unit} (n={}{thin})", v * scale, m.samples))
    };
    for group in &compared.groups {
        let f = &group.figures;
        eprintln!(
            "{name} {}: {} workers · claimed {} landed {} · {} /active h · claim→merge {} · working {} · sendbacks/landed {} · ready pass {} · abandoned {} · active {:.2} h of {:.2} h on shift",
            group.label,
            f.workers,
            f.claimed,
            f.landed,
            m(f.landed_per_active_hour, 1.0, ""),
            m(f.claim_to_merge_seconds, 1.0 / 60.0, " min"),
            m(f.working_seconds, 1.0 / 60.0, " min"),
            m(f.sendbacks_per_landed, 1.0, ""),
            m(f.ready_pass_rate, 100.0, "%"),
            f.abandoned,
            f.active_seconds as f64 / 3_600.0,
            f.on_shift_seconds as f64 / 3_600.0,
        );
    }
}

/// This clone's own swarm, when it has a tracking branch.
#[test]
fn this_clones_own_workers_compare() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let has = std::process::Command::new("git").current_dir(&root).args(["rev-parse", "--verify", "-q", "refs/remotes/origin/tracking"]).output();
    if !has.is_ok_and(|o| o.status.success()) {
        eprintln!("no origin/tracking here: nothing to read");
        return;
    }
    print_comparison("swarmception", &check_swarm(&root));
}

/// The swarms `SWARMCEPTION_WORKERS_REPOS` names (mc-engine: opus at high, medium and low;
/// swarm-timeline), when it is set.
#[test]
fn named_swarms_workers_compare() {
    let Ok(repos) = std::env::var("SWARMCEPTION_WORKERS_REPOS") else {
        eprintln!("SWARMCEPTION_WORKERS_REPOS unset: nothing to read");
        return;
    };
    for repo in repos.split(':').filter(|r| !r.is_empty()) {
        let compared = check_swarm(Path::new(repo));
        assert!(!compared.groups.is_empty(), "{repo}: a swarm with workers");
        print_comparison(repo, &compared);
    }
}
