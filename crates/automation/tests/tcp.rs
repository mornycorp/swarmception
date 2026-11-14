//! The control channel over a real socket, headless: the engine's commands, typed errors for
//! bad lines, a wrong token, unknown commands and hostile params, and the connection limits.

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::time::Duration;

use swarmception_automation::client::{Client, ClientError};
use swarmception_automation::protocol::{MAX_LINE, ProtocolError, Reply};
use swarmception_automation::session::SessionHandler;
use swarmception_automation::tcp::{Limits, Listening, Shared, bind, serve_within};
use swarmception_domain::{Grouping, RowsSpec};
use swarmception_engine::EngineError;
use swarmception_engine::engine_cmds::CommandsResult;
use swarmception_engine::error::Unavailable;

const WAIT: Duration = Duration::from_secs(10);

/// A headless server on a free loopback port, within `limits`.
fn server(limits: Limits) -> Listening {
    let (listener, listening) = bind(0, None).unwrap();
    assert!(listening.address.ip().is_loopback());
    assert_eq!(listening.token.len(), 32);
    let token = listening.token.clone();
    std::thread::spawn(move || serve_within(&listener, &token, &Shared::new(SessionHandler::default()), limits));
    listening
}

fn client(listening: &Listening) -> Client {
    Client::connect(listening.address, &listening.token, WAIT).unwrap()
}

fn problem(result: Result<serde_json::Value, ClientError>) -> ProtocolError {
    match result {
        Err(ClientError::Protocol(error)) => error,
        other => panic!("not a protocol error: {other:?}"),
    }
}

#[test]
fn the_engines_commands_run_over_the_socket() {
    let listening = server(Limits::DEFAULT);
    let mut client = client(&listening);
    let list: CommandsResult = client.call("engine.commands", &serde_json::json!({})).unwrap();
    assert!(list.commands.iter().any(|c| c.id == "factory.at"));
    let _: RowsSpec = client.call("rows.set", &serde_json::json!({"grouping": "workers"})).unwrap();
    let spec: RowsSpec = client.call("rows.get", &serde_json::json!({})).unwrap();
    assert_eq!(spec.grouping, Grouping::Workers);
    // A second connection shares the session.
    let spec: RowsSpec = self::client(&listening).call("rows.get", &()).unwrap();
    assert_eq!(spec.grouping, Grouping::Workers);
    // No repository: disabled, and why.
    let error = problem(client.call("repo.reload", &()));
    assert_eq!(error, ProtocolError::from(EngineError::Disabled { command: "repo.reload".into(), reason: Unavailable::NoRepo }));
    let ProtocolError::Engine { remediation, .. } = &error else { panic!("{error:?}") };
    assert_eq!(remediation, "open a repository first (repo.open)", "every failure says what to do");
}

#[test]
fn bad_requests_are_typed_errors_and_never_end_the_server() {
    let listening = server(Limits::DEFAULT);
    let mut client = client(&listening);
    // A good first line, then bad ones on the same connection: each answered, none fatal.
    let _: CommandsResult = client.call("engine.commands", &()).unwrap();
    let read = |line: String| serde_json::from_str::<Reply<serde_json::Value>>(&line).unwrap().error.unwrap();
    assert!(matches!(read(client.send_line("{not json").unwrap()), ProtocolError::BadRequest { .. }));
    assert!(matches!(read(client.send_line(r#"{"v":1}"#).unwrap()), ProtocolError::BadRequest { .. }));
    let wrong = format!(r#"{{"v":7,"token":"{}","id":1,"command":"rows.get"}}"#, listening.token);
    assert_eq!(read(client.send_line(&wrong).unwrap()), ProtocolError::WrongVersion { got: 7, speaks: 1 });
    assert_eq!(read(client.send_line(r#"{"v":1,"token":"nope","id":2,"command":"rows.get"}"#).unwrap()), ProtocolError::BadToken);
    let long = "x".repeat(MAX_LINE + 10);
    assert_eq!(read(client.send_line(&long).unwrap()), ProtocolError::TooLong { limit: MAX_LINE });
    assert!(matches!(problem(client.call("nope.nope", &())), ProtocolError::Engine { error: EngineError::UnknownCommand { .. }, .. }));
    // The window's commands have no window here.
    assert_eq!(problem(client.call("ui.inspect", &())), ProtocolError::NoUi);
    assert_eq!(problem(client.call("timeline.view.fit", &())), ProtocolError::NoUi);
    // Hostile params: wrong types, unknown fields, huge numbers, deep nesting, a list.
    let hostile = [
        serde_json::json!({"grouping": 7}),
        serde_json::json!({"grouping": "workers", "evil": true}),
        serde_json::json!({"collapse": 1e308}),
        serde_json::json!([1, 2, 3]),
        serde_json::from_str(&format!("{}{}", "[".repeat(64), "]".repeat(64))).unwrap(),
    ];
    for params in hostile {
        assert!(matches!(problem(client.call("rows.set", &params)), ProtocolError::Engine { error: EngineError::BadParams { .. }, .. }), "{params}");
    }
    // Still serving.
    let _: RowsSpec = client.call("rows.get", &()).unwrap();
}

#[test]
fn a_first_line_without_the_token_closes_the_connection() {
    let listening = server(Limits::DEFAULT);
    let mut stranger = Client::connect(listening.address, "not-the-token", WAIT).unwrap();
    assert_eq!(problem(stranger.call("rows.get", &())), ProtocolError::BadToken);
    // Closed: the next line gets no answer.
    assert!(stranger.send_line(r#"{"v":1,"token":"x","id":2,"command":"rows.get"}"#).is_err());
    // Garbage first: closed too.
    let mut garbage = Client::connect(listening.address, &listening.token, WAIT).unwrap();
    assert!(garbage.send_line("hello").unwrap().contains("bad_request"));
    assert!(garbage.send_line("hello").is_err());
}

#[test]
fn connections_over_the_limit_are_told_they_are_busy() {
    let listening = server(Limits { connections: 1, ..Limits::DEFAULT });
    let mut first = client(&listening);
    let _: RowsSpec = first.call("rows.get", &()).unwrap();
    let second = TcpStream::connect(listening.address).unwrap();
    second.set_read_timeout(Some(WAIT)).unwrap();
    let mut line = String::new();
    BufReader::new(&second).read_line(&mut line).unwrap();
    let reply: Reply<serde_json::Value> = serde_json::from_str(&line).unwrap();
    assert_eq!(reply.error, Some(ProtocolError::Busy { most: 1 }));
    drop(second);
    // The first is still served.
    let _: RowsSpec = first.call("rows.get", &()).unwrap();
}

#[test]
fn every_command_with_hostile_params_is_an_answer_not_a_crash() {
    let listening = server(Limits::DEFAULT);
    let mut client = client(&listening);
    let list: CommandsResult = client.call("engine.commands", &()).unwrap();
    let hostile = [
        serde_json::json!({}),
        serde_json::json!(null),
        serde_json::json!({"t": -9_223_372_036_854_775_808_i64, "issue": u64::MAX, "time": i64::MAX, "start": "x", "end": -1}),
        serde_json::json!({"path": "/nonexistent/\u{0}"}),
        serde_json::json!("text"),
    ];
    for command in &list.commands {
        for params in &hostile {
            let result: Result<serde_json::Value, ClientError> = client.call(&command.id, params);
            assert!(!matches!(result, Err(ClientError::Io(_) | ClientError::BadReply(_))), "{} {params}: {result:?}", command.id);
            let caught = matches!(&result, Err(ClientError::Protocol(ProtocolError::Engine { error: EngineError::Internal { .. }, remediation })) if remediation == swarmception_engine::error::PANICKED);
            assert!(!caught, "{} {params}: a panic the dispatch caught", command.id);
        }
    }
}
