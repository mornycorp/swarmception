//! `swarmception-cli serve`, `mcp` and `send` as child processes: the control protocol over a
//! real socket, MCP over the child's stdio (headless, and through a live server with
//! `--control`), and `send`'s requests and scripts against a live server.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
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
        Self::with(&["mcp"])
    }

    fn with(args: &[&str]) -> Self {
        let mut child = cli().args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
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

/// A tool's error, read as the protocol's.
fn tool_error(result: Rpc<CallResult>) -> ProtocolError {
    let result = result.result.unwrap();
    assert!(result.is_error, "{result:?}");
    serde_json::from_str(&result.content[0].text).unwrap()
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
    // Headless, the window's tools aren't listed.
    assert!(!names.iter().any(|n| n.starts_with("ui_") || n.starts_with("timeline_view")), "{names:?}");

    let set: Rpc<CallResult> = mcp.send(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rows_set","arguments":{"grouping":"workers"}}}"#);
    let set = set.result.unwrap();
    assert!(!set.is_error && set.content[0].text.contains("workers"), "{set:?}");
    let get: Rpc<CallResult> = mcp.send(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"rows_get"}}"#);
    assert!(get.result.unwrap().content[0].text.contains("\"workers\""), "one session across calls");
    // A command's error is the tool's error, typed as the protocol's: here the engine's.
    let bad = tool_error(mcp.send(r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"rows_set","arguments":{"grouping":"tree"}}}"#));
    assert!(matches!(bad, ProtocolError::Engine { error: EngineError::BadParams { .. } }));
    let unknown = tool_error(mcp.send(r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"no_such"}}"#));
    assert!(matches!(unknown, ProtocolError::Engine { error: EngineError::UnknownCommand { .. } }));
    let window = tool_error(mcp.send(r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"ui_inspect"}}"#));
    assert_eq!(window, ProtocolError::NoUi, "a window's tool, headless");
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

/// `serve` as a child, its control file written: the file, and the child.
fn served(name: &str) -> (PathBuf, Killed) {
    let file = std::env::temp_dir().join(format!("worker24-{name}-{}.json", std::process::id()));
    let mut child = Killed(cli().args(["serve", "--control-file"]).arg(&file).stdout(Stdio::piped()).spawn().unwrap());
    let mut line = String::new();
    BufReader::new(child.0.stdout.take().unwrap()).read_line(&mut line).unwrap();
    (file, child)
}

/// `swarmception-cli` with `args`, run to its end in `dir`: its exit code, stdout's lines and
/// stderr.
fn finished(dir: &Path, args: &[&str]) -> (Option<i32>, Vec<String>, String) {
    let out = cli().current_dir(dir).args(args).output().unwrap();
    let lines = String::from_utf8(out.stdout).unwrap().lines().map(str::to_owned).collect();
    (out.status.code(), lines, String::from_utf8(out.stderr).unwrap())
}

#[test]
fn send_runs_a_request_or_a_script_on_a_live_server_and_its_exit_code_says_how_it_went() {
    let (file, _server) = served("send");
    let control = file.to_str().unwrap();
    let dir = std::env::temp_dir();

    let (code, out, _) = finished(&dir, &["send", "--control", control, "rows.set", r#"{"grouping":"workers"}"#]);
    assert_eq!(code, Some(0));
    assert!(out[0].contains("\"workers\""), "{out:?}");
    let (code, out, _) = finished(&dir, &["send", "--control", control, "rows.get"]);
    assert_eq!((code, out[0].contains("\"workers\"")), (Some(0), true), "one session: the server's");

    // A script: each result a line, until the first error, after which nothing is sent.
    let script = dir.join(format!("worker24-script-{}.jsonl", std::process::id()));
    let steps = [
        "# a comment, then a blank line",
        "",
        r#"{"command":"rows.set","params":{"grouping":"flat"}}"#,
        r#"{"command":"rows.get"}"#,
        r#"{"command":"ui.inspect"}"#,
        r#"{"command":"rows.set","params":{"grouping":"workers"}}"#,
    ];
    std::fs::write(&script, steps.join("\n")).unwrap();
    let (code, out, err) = finished(&dir, &["send", "--control", control, "--script", script.to_str().unwrap()]);
    assert_eq!(code, Some(1), "{err}");
    assert_eq!(out.len(), 2, "{out:?}");
    assert!(out[1].contains("\"flat\""));
    assert!(err.contains("line 5 (ui.inspect)"), "{err}");
    assert!(err.lines().any(|l| serde_json::from_str::<ProtocolError>(l).ok() == Some(ProtocolError::NoUi)), "the error, typed: {err}");
    let (_, out, _) = finished(&dir, &["send", "--control", control, "rows.get"]);
    assert!(out[0].contains("\"flat\""), "the step after the error wasn't sent");

    // A script that doesn't read is bad usage and sends nothing; no server is unreachable.
    std::fs::write(&script, "{\"command\":\"rows.set\",\"params\":{\"grouping\":\"workers\"}}\n{\"comand\":1}").unwrap();
    let (code, _, err) = finished(&dir, &["send", "--control", control, "--script", script.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}");
    assert!(err.contains("line 2"), "{err}");
    let (_, out, _) = finished(&dir, &["send", "--control", control, "rows.get"]);
    assert!(out[0].contains("\"flat\""));
    let (code, _, _) = finished(&dir, &["send", "--control", "/no/such/control.json", "rows.get"]);
    assert_eq!(code, Some(3));
    let (code, _, _) = finished(&dir, &["send", "--control", control, "rows.set", r#"{"grouping":"tree"}"#]);
    assert_eq!(code, Some(1));
    std::fs::remove_file(script).ok();
    std::fs::remove_file(file).ok();
}

#[test]
fn mcp_with_control_runs_its_tools_on_the_server() {
    let (file, server) = served("mcp");
    let mut mcp = Mcp::with(&["mcp", "--control", file.to_str().unwrap()]);
    let tools: Rpc<ToolList> = mcp.send(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    let tools = tools.result.unwrap().tools;
    assert!(tools.iter().any(|t| t.name == "factory_at"), "the server's engine commands");
    let set: Rpc<CallResult> = mcp.send(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rows_set","arguments":{"grouping":"workers"}}}"#);
    assert!(!set.result.unwrap().is_error);
    // The tool ran on the server's session, not on one of the MCP server's own.
    let listening: Listening = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let mut client = Client::connect(listening.address, &listening.token, Duration::from_secs(10)).unwrap();
    let spec: serde_json::Value = client.call("rows.get", &()).unwrap();
    assert_eq!(spec["grouping"], "workers");
    // The server gone: the tool's error says so, and MCP goes on.
    drop(server);
    let gone = tool_error(mcp.send(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rows_get"}}"#));
    assert!(matches!(gone, ProtocolError::Unavailable { .. }), "{gone:?}");
    let ping: Rpc<serde_json::Value> = mcp.send(r#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#);
    assert!(ping.result.is_some());
    std::fs::remove_file(file).ok();
}
