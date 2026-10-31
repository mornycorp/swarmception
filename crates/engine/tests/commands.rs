//! The engine's commands, run as a front end runs them: by id with JSON params, the replies read
//! back as their result types, over a real repository's tracking branch.

use std::path::PathBuf;

use swarmception_engine::Session;
use swarmception_engine::engine_cmds::CommandsResult;
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_engine::rows_cmds::ListResult;

/// A repository in a temporary directory whose tracking branch holds two agents.
fn repo() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "swarmception-engine-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
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
    assert_eq!(names, ["Merges to main", "Tickets merged", "#7 · PR #8"]);
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
    assert!(commands.commands.len() >= 9);
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
