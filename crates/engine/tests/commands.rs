//! The engine's commands, run as a front end runs them: by id with JSON params, the replies read
//! back as their result types, over a real repository's tracking branch.

use std::path::PathBuf;

use swarmception_domain::Stats;
use swarmception_engine::Session;
use swarmception_engine::engine_cmds::CommandsResult;
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_engine::rows_cmds::ListResult;

/// A repository in a temporary directory whose tracking branch holds two agents.
fn repo() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "swarmception-engine-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        // Two tests can start within one tick of the clock: a counter keeps their directories apart.
        {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        }
    ));
    std::fs::create_dir_all(dir.join("tracking/mini-aa1111")).unwrap();
    std::fs::create_dir_all(dir.join("tracking/mini-bb2222")).unwrap();
    std::fs::write(
        dir.join("tracking/mini-aa1111/work.jsonl"),
        concat!(
            r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
            "\n",
            r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
            "\n",
            r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:30:00Z"}"#,
            "\n",
            r#"{"event":"stop","issue":7,"reason":"done","t":"2026-10-31T03:31:00Z"}"#,
            "\n",
            r#"{"event":"session.stop","t":"2026-10-31T03:32:00Z"}"#,
            "\n",
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("tracking/mini-bb2222/work.jsonl"),
        concat!(
            r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
            "\n",
            r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7)","sub":"gate-pr8","t":"2026-10-31T03:40:00Z"}"#,
            "\n",
            r#"{"event":"merge","issue":7,"mr":8,"sub":"gate-pr8","t":"2026-10-31T03:45:00Z"}"#,
            "\n",
            r#"{"event":"sub.stop","note":"rc=0; merged #8","sub":"gate-pr8","t":"2026-10-31T03:46:00Z"}"#,
            "\n",
        ),
    )
    .unwrap();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    run(&["init", "-q", "-b", "tracking"]);
    run(&["add", "."]);
    run(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "events"]);
    dir
}

fn call<T: serde::de::DeserializeOwned>(session: &mut Session, id: &str, params: &str) -> T {
    let reply = session.execute(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{id}'s reply {reply}: {e}"))
}

#[test]
fn open_a_repo_and_list_its_rows() {
    let dir = repo();
    let mut session = Session::new();
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    let status: StatusResult = call(&mut session, "repo.open", &format!("{{\"path\":{path}}}"));
    assert_eq!(status.reference.as_deref(), Some("refs/heads/tracking"));
    assert_eq!((status.machines, status.workers, status.instances, status.tickets, status.merged, status.events), (1, 2, 2, 1, 1, 9));
    assert_eq!((status.main, status.main_merges), (None, 0), "no main branch");
    let list: ListResult = call(&mut session, "rows.list", "");
    let names: Vec<&str> = list.rows.iter().map(swarmception_domain::Node::name).collect();
    assert_eq!(names, ["Merges to main", "Tickets merged", "Tickets 1"]);
    assert!(session.widget_timeline().unwrap().is_some());

    let before = session.revision();
    let _: serde_json::Value = call(&mut session, "rows.set", r#"{"grouping":"workers","collapse":true}"#);
    assert_ne!(session.revision().0, before.0, "new rows, a new generation");
    let list: ListResult = call(&mut session, "rows.list", "");
    let names: Vec<&str> = list.rows.iter().map(swarmception_domain::Node::name).collect();
    assert_eq!(names, ["Tickets merged", "mini"], "the empty main row collapsed; the ticket under its worker");

    let at: swarmception_engine::rows_cmds::TimeAtResult = call(&mut session, "timeline.timeAt", r#"{"position":60000000000}"#);
    assert_eq!(at.text, "2026-10-31T03:01:00Z");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_reload_before_an_open_is_disabled() {
    let mut session = Session::new();
    let error = session.execute("repo.reload", "").unwrap_err();
    assert_eq!(error.code(), "disabled");
    let error = session.execute("repo.open", r#"{"path":"/no/such/dir"}"#).unwrap_err();
    assert_eq!(error.code(), "invalid");
}

#[test]
fn every_command_takes_hostile_params_without_panicking() {
    let commands: CommandsResult = call(&mut Session::new(), "engine.commands", "");
    assert!(commands.commands.len() >= 13);
    let hostile = [
        "",
        "null",
        "{}",
        "[]",
        "1",
        "\"x\"",
        "{\"nope\":1}",
        "{\"path\":5}",
        "{\"position\":-9223372036854775808}",
        "{\"by\":\"martians\"}",
        "{",
        &"[".repeat(10_000),
    ];
    for command in &commands.commands {
        for params in hostile {
            let mut session = Session::new();
            if let Err(error) = session.execute(&command.id, params) {
                assert_ne!(error.code(), "panicked", "{} with {params:.40}", command.id);
            }
        }
    }
    assert_eq!(Session::new().execute("no.such", "").unwrap_err().code(), "unknown_command");
}

#[test]
fn stats_over_a_repo_and_a_window() {
    let dir = repo();
    let mut session = Session::new();
    assert_eq!(session.execute("stats.get", "").unwrap_err().code(), "disabled", "no repository yet");
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    let _: StatusResult = call(&mut session, "repo.open", &format!("{{\"path\":{path}}}"));
    let stats: Stats = call(&mut session, "stats.get", "");
    assert_eq!((stats.start.to_string(), stats.end.to_string()), ("2026-10-31T03:00:00Z".to_owned(), "2026-10-31T03:46:00Z".to_owned()));
    assert_eq!(stats.on_shift_seconds, 32 * 60, "worker-1's session; the coordinator's isn't a worker's");
    assert_eq!(stats.active_seconds, 3 * 60, "03:01 to 03:30 has no event: quiet");
    assert_eq!((stats.landed, stats.landed_per_active_hour), (1, Some(20.0)));
    assert_eq!((stats.gates.runs, stats.gates.passed), (1, 1));
    assert_eq!(stats.median_dock_wait_seconds, Some(15 * 60));
    let early: Stats = call(&mut session, "stats.get", r#"{"start":"2026-10-31T03:00:00Z","end":1793416200}"#);
    assert_eq!((early.end.to_string().as_str(), early.landed, early.active_seconds), ("2026-10-31T03:10:00Z", 0, 60));
    for bad in [r#"{"start":"yesterday"}"#, r#"{"start":"2026-10-31T04:00:00Z","end":"2026-10-31T03:00:00Z"}"#] {
        assert_eq!(session.execute("stats.get", bad).unwrap_err().code(), "invalid", "{bad}");
    }
    assert_eq!(session.execute("stats.get", r#"{"start":true}"#).unwrap_err().code(), "bad_params");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_factory_at_a_moment_and_the_timeline_place_of_a_time() {
    use swarmception_domain::factory::{BenchStatus, CrateState, FactoryScene, Outcome};
    use swarmception_engine::factory_cmds::PlaceAtResult;
    let dir = repo();
    let mut session = Session::new();
    assert_eq!(session.execute("factory.at", "").unwrap_err().code(), "disabled", "no repository");
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    let _: StatusResult = call(&mut session, "repo.open", &format!("{{\"path\":{path}}}"));

    // 03:10: worker-1 holds #7 (claimed); 03:42: #7 waits in the gate; the default: the last event.
    let t = |text: &str| swarmception_model::Timestamp::parse(text).unwrap().0;
    let busy: FactoryScene = call(&mut session, "factory.at", &format!("{{\"t\":{}}}", t("2026-10-31T03:10:00Z")));
    assert_eq!(busy.text, "2026-10-31T03:10:00Z");
    let bench = busy.benches.iter().find(|b| b.actor.name == "worker-1").unwrap();
    assert_eq!((bench.status, bench.crates.len()), (BenchStatus::Claimed, 1));
    let gating: FactoryScene = call(&mut session, "factory.at", &format!("{{\"t\":{},\"recent\":60}}", t("2026-10-31T03:42:00Z")));
    assert_eq!(gating.recent, 60);
    assert_eq!(gating.gate.crates.iter().map(|c| (c.issue, c.state)).collect::<Vec<_>>(), [(7, CrateState::Gating)]);
    assert_eq!(gating.gate.running[0].outcome, Outcome::Running);
    let end: FactoryScene = call(&mut session, "factory.at", "");
    assert_eq!(end.text, "2026-10-31T03:46:00Z");
    assert_eq!((end.counts.merged, end.shipping.landed, end.gate.passed), (1, 1, 1));

    let place: PlaceAtResult = call(&mut session, "timeline.placeAt", &format!("{{\"time\":{}}}", t("2026-10-31T03:01:30Z")));
    assert_eq!(place.position, 90_000_000_000, "90 s past the origin, 03:00");
    let at: swarmception_engine::rows_cmds::TimeAtResult = call(&mut session, "timeline.timeAt", &format!("{{\"position\":{}}}", place.position));
    assert_eq!(at.text, "2026-10-31T03:01:30Z", "the other way round");
    assert_eq!(place.item, Some(0), "in 03:01's item, the first");
    // #7's journey, and its bars on the timeline: claimed, then waiting for merge.
    let traced: FactoryScene = call(&mut session, "factory.at", r#"{"ticket":7}"#);
    assert_eq!(traced.journey.map(|j| j.text).as_deref(), Some("bench 29m → dock 10m → gate 5m → main"));
    let bars: swarmception_engine::factory_cmds::TicketBarsResult = call(&mut session, "timeline.ticketBars", r#"{"issue":7}"#);
    let bars: Vec<(String, usize)> = bars.bars.into_iter().map(|b| (b.track, b.range)).collect();
    assert_eq!(bars, [("tickets/1".to_owned(), 0), ("tickets/1".to_owned(), 1)]);
    let none: swarmception_engine::factory_cmds::TicketBarsResult = call(&mut session, "timeline.ticketBars", r#"{"issue":70}"#);
    assert!(none.bars.is_empty(), "#70 is not #7");
    let before: PlaceAtResult = call(&mut session, "timeline.placeAt", r#"{"time":0}"#);
    assert_eq!(before.item, None);

    // Hostile values on a loaded session: errors or answers, never a panic.
    for params in [r#"{"t":-9223372036854775808,"recent":9223372036854775807}"#, r#"{"t":9223372036854775807,"recent":-1}"#, r#"{"t":"soon"}"#, r#"{"at":1}"#] {
        if let Err(error) = session.execute("factory.at", params) {
            assert_eq!(error.code(), "bad_params", "{params}");
        }
    }
    for params in [r#"{"time":-9223372036854775808}"#, r#"{"time":9223372036854775807}"#] {
        let _: PlaceAtResult = call(&mut session, "timeline.placeAt", params);
    }
    std::fs::remove_dir_all(dir).ok();
}
