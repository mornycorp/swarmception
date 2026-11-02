//! `swarmception-cli serve` and `swarmception-cli mcp` as child processes: the control protocol
//! over a real socket, and MCP over the child's stdio.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use serde::Deserialize;
use swarmception_automation::client::{Client, ClientError};
use swarmception_automation::mcp::{CallResult, ToolList};
use swarmception_automation::protocol::ProtocolError;
use swarmception_automation::tcp::Listening;
use swarmception_engine::EngineError;
use swarmception_engine::engine_cmds::CommandsResult;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_swarmception-cli"))
}

/// Kills the child when the test ends, however it ends.
struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn serve_speaks_the_control_protocol_headless() {
    let file = std::env::temp_dir().join(format!("worker18-listening-{}.json", std::process::id()));
    let mut child = Killed(cli().args(["serve", "--control-file"]).arg(&file).stdout(Stdio::piped()).spawn().unwrap());
    let mut line = String::new();
    BufReader::new(child.0.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let listening: Listening = serde_json::from_str(&line).unwrap();
    assert!(listening.address.ip().is_loopback());
    let written: Listening = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(written, listening);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o077, 0, "the token's file is its owner's alone");
    }

    let mut client = Client::connect(listening.address, &listening.token, Duration::from_secs(10)).unwrap();
    let list: CommandsResult = client.call("engine.commands", &()).unwrap();
    assert!(list.commands.iter().any(|c| c.id == "stats.get"));
    let refused = |result: Result<serde_json::Value, ClientError>| match result {
        Err(ClientError::Protocol(error)) => error,
        other => panic!("{other:?}"),
    };
    assert_eq!(refused(client.call("ui.inspect", &())), ProtocolError::NoUi);
    assert!(matches!(
        refused(client.call("rows.set", &serde_json::json!({"grouping": "tree"}))),
        ProtocolError::Engine { error: EngineError::BadParams { .. } }
    ));
    let mut stranger = Client::connect(listening.address, "guess", Duration::from_secs(10)).unwrap();
    assert_eq!(refused(stranger.call("engine.commands", &())), ProtocolError::BadToken);
    std::fs::remove_file(file).ok();
}

/// A JSON-RPC reply, its result read as `T`.
#[derive(Debug, Deserialize)]
struct Rpc<T> {
    id: serde_json::Value,
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Initialized {
    protocol_version: String,
    server_info: ServerInfo,
}

#[derive(Debug, Deserialize)]
struct ServerInfo {
    name: String,
}

struct Mcp {
    _child: Killed,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Mcp {
    fn start() -> Self {
        let mut child = cli().arg("mcp").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self { _child: Killed(child), input, output }
    }

    fn send<T: for<'de> Deserialize<'de>>(&mut self, line: &str) -> Rpc<T> {
        writeln!(self.input, "{line}").unwrap();
        self.input.flush().unwrap();
        let mut reply = String::new();
        self.output.read_line(&mut reply).unwrap();
        serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{e}: {reply}"))
    }
}

#[test]
fn mcp_serves_the_engines_commands_as_tools() {
    let mut mcp = Mcp::start();
    let init: Rpc<Initialized> = mcp.send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#);
    let init = init.result.unwrap();
    assert_eq!((init.protocol_version.as_str(), init.server_info.name.as_str()), ("2025-06-18", "swarmception"));
    // A notification has no answer: the next line answers the next request.
    writeln!(mcp.input, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
    let tools: Rpc<ToolList> = mcp.send(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
    assert_eq!(tools.id, serde_json::json!(2));
    let tools = tools.result.unwrap().tools;
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    for name in ["engine_commands", "repo_open", "rows_set", "factory_at", "factory_step", "stats_get", "timeline_placeAt"] {
        assert!(names.contains(&name), "{name} in {names:?}");
    }
    assert!(
        tools.iter().all(|t| serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(t.input_schema.clone()).is_ok()),
        "each tool's input is a schema object"
    );

    let set: Rpc<CallResult> = mcp.send(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rows_set","arguments":{"grouping":"workers"}}}"#);
    let set = set.result.unwrap();
    assert!(!set.is_error && set.content[0].text.contains("workers"), "{set:?}");
    let get: Rpc<CallResult> = mcp.send(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"rows_get"}}"#);
    assert!(get.result.unwrap().content[0].text.contains("\"workers\""), "one session across calls");
    // A command's error is the tool's error, typed as the engine's.
    let bad: Rpc<CallResult> = mcp.send(r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"rows_set","arguments":{"grouping":"tree"}}}"#);
    let bad = bad.result.unwrap();
    assert!(bad.is_error);
    assert!(matches!(serde_json::from_str::<EngineError>(&bad.content[0].text).unwrap(), EngineError::BadParams { .. }));
    let unknown: Rpc<CallResult> = mcp.send(r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"no_such"}}"#);
    assert!(matches!(serde_json::from_str::<EngineError>(&unknown.result.unwrap().content[0].text).unwrap(), EngineError::UnknownCommand { .. }));
    // Bad lines and methods are JSON-RPC errors, and the server goes on.
    let garbage: Rpc<serde_json::Value> = mcp.send("{not json");
    assert_eq!(garbage.error.map(|e| e.code), Some(-32_700));
    let nope: Rpc<serde_json::Value> = mcp.send(r#"{"jsonrpc":"2.0","id":7,"method":"resources/list"}"#);
    assert_eq!(nope.error.map(|e| e.code), Some(-32_601));
    let missing: Rpc<serde_json::Value> = mcp.send(r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"arguments":{}}}"#);
    assert_eq!(missing.error.map(|e| e.code), Some(-32_602));
    let ping: Rpc<serde_json::Value> = mcp.send(r#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#);
    assert!(ping.result.is_some());
}
