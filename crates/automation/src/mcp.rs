//! MCP over stdio (the Model Context Protocol's newline-delimited JSON-RPC 2.0): every command a
//! [`Handler`] runs is a tool, named with `_` for its `.` (`factory.at` is `factory_at`,
//! `timeline.view.fit` is `timeline_view_fit`), taking the command's params and answering its
//! result as JSON text. The tools are the engine's (`engine.commands`) and, when the handler has
//! a window, the window's and the widget's (`ui.commands`): over the headless session
//! ([`crate::session::SessionHandler`]) the engine's alone, over a running window
//! ([`crate::client::Remote`], `swarmception-cli mcp --control <file>`) all of them, each run in
//! the window. As the widget's `timeline_automation::mcp`, over this app's handlers.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use swarmception_engine::engine_cmds::CommandsResult;

use crate::lines::{Line, read_line};
use crate::protocol::{Handler, MAX_LINE, ProtocolError};

/// The MCP version this server speaks when the client names none.
pub const MCP_VERSION: &str = "2025-06-18";

/// The server's name in `initialize`'s answer.
pub const SERVER_NAME: &str = "swarmception";

/// A JSON-RPC message from the client.
#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct InitializeParams {
    #[serde(default)]
    protocol_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CallParams {
    name: String,
    #[serde(default)]
    arguments: Option<serde_json::Value>,
}

/// A tool, as `tools/list` answers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// `tools/list`'s result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolList {
    pub tools: Vec<Tool>,
}

/// A piece of a tool's answer: here always its JSON text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Text {
    pub r#type: String,
    pub text: String,
}

/// `tools/call`'s result: the command's result as JSON text, or, with `isError`, its
/// [`ProtocolError`]'s (the engine's error as `{"problem":"engine","error":…}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallResult {
    pub content: Vec<Text>,
    pub is_error: bool,
}

/// JSON-RPC's error codes this server answers with.
pub const PARSE_ERROR: i64 = -32_700;
pub const METHOD_NOT_FOUND: i64 = -32_601;
pub const INVALID_PARAMS: i64 = -32_602;

/// The tool's name for the command `id`.
#[must_use]
pub fn tool_name(id: &str) -> String {
    id.replace('.', "_")
}

/// The command's id for the tool `name`: ids, the engine's, the window's and the widget's, have
/// no `_`.
#[must_use]
pub fn command_id(name: &str) -> String {
    name.replace('_', ".")
}

/// Serves MCP on `input` and `output` until the input ends, each tool run by `handler`.
///
/// # Errors
///
/// An I/O error reading or writing.
pub fn serve(mut input: impl BufRead, mut output: impl Write, handler: &mut impl Handler) -> io::Result<()> {
    loop {
        let reply = match read_line(&mut input, MAX_LINE)? {
            Line::End => return Ok(()),
            Line::Text(text) if text.trim().is_empty() => continue,
            Line::Text(text) => answer(handler, &text),
            Line::TooLong | Line::NotText => Some(error(&serde_json::Value::Null, PARSE_ERROR, "the line is too long or not UTF-8")),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
}

/// The reply to one message, none for a notification.
fn answer(handler: &mut impl Handler, text: &str) -> Option<String> {
    let message: Message = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(e) => return Some(error(&serde_json::Value::Null, PARSE_ERROR, &e.to_string())),
    };
    let id = message.id?;
    let result = match message.method.as_str() {
        "initialize" => {
            let asked: InitializeParams = message.params.and_then(|p| serde_json::from_value(p).ok()).unwrap_or_default();
            serde_json::json!({
                "protocolVersion": asked.protocol_version.unwrap_or_else(|| MCP_VERSION.to_owned()),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
            })
        }
        "ping" => serde_json::json!({}),
        "tools/list" => match tools(handler) {
            Ok(tools) => serde_json::to_value(tools).unwrap_or_default(),
            Err(message) => return Some(error(&id, INVALID_PARAMS, &message)),
        },
        "tools/call" => match message.params.map(serde_json::from_value::<CallParams>) {
            Some(Ok(call)) => serde_json::to_value(call_tool(handler, &call)).unwrap_or_default(),
            _ => return Some(error(&id, INVALID_PARAMS, "tools/call takes a name and arguments")),
        },
        other => return Some(error(&id, METHOD_NOT_FOUND, &format!("no method {}", other.chars().take(64).collect::<String>()))),
    };
    Some(serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string())
}

/// The commands `command` lists (`engine.commands`, `ui.commands`); none where there is no
/// window to list them.
fn listed(handler: &mut impl Handler, command: &str) -> Result<CommandsResult, String> {
    match handler.handle(command, "") {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{command}'s answer doesn't read: {e}")),
        Err(ProtocolError::NoUi) => Ok(CommandsResult { commands: Vec::new() }),
        Err(error) => Err(format!("{command}: {error}")),
    }
}

/// Every command the handler runs, as a tool: the engine's, then the window's and the widget's.
fn tools(handler: &mut impl Handler) -> Result<ToolList, String> {
    let engine = listed(handler, "engine.commands")?;
    let window = listed(handler, "ui.commands")?;
    let tools = engine
        .commands
        .into_iter()
        .chain(window.commands)
        .map(|command| Tool { name: tool_name(&command.id), description: format!("{} ({})", command.label, command.id), input_schema: command.params })
        .collect();
    Ok(ToolList { tools })
}

/// Runs the tool `call` names: its command, with its arguments.
fn call_tool(handler: &mut impl Handler, call: &CallParams) -> CallResult {
    let id = command_id(&call.name);
    let params = call.arguments.as_ref().map_or_else(String::new, ToString::to_string);
    let text = |text: String| vec![Text { r#type: "text".into(), text }];
    match handler.handle(&id, &params) {
        Ok(result) => CallResult { content: text(result), is_error: false },
        Err(error) => CallResult { content: text(serde_json::to_string(&error).unwrap_or_else(|_| error.to_string())), is_error: true },
    }
}

fn error(id: &serde_json::Value, code: i64, message: &str) -> String {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_id_round_trips_through_its_tool_name() {
        for entry in swarmception_engine::commands::all() {
            assert!(!entry.id.contains('_'), "{} has a `_`, which a tool name can't tell from its `.`", entry.id);
            assert_eq!(command_id(&tool_name(entry.id)), entry.id);
        }
        for entry in timeline_engine::commands::all() {
            let id = format!("timeline.{}", entry.id);
            assert_eq!(command_id(&tool_name(&id)), id, "the widget's, forwarded");
        }
    }

    /// A window's handler: the engine's and the window's lists, and one of the widget's.
    struct Window;

    impl Handler for Window {
        fn handle(&mut self, command: &str, _params: &str) -> Result<String, ProtocolError> {
            let list = |ids: &[&str]| {
                let commands = ids
                    .iter()
                    .map(|id| swarmception_engine::engine_cmds::CommandWire {
                        id: (*id).to_owned(),
                        label: "L".into(),
                        read_only: true,
                        params: serde_json::json!({"type": "object"}),
                        result: serde_json::json!({}),
                    })
                    .collect();
                Ok(serde_json::to_string(&CommandsResult { commands }).unwrap())
            };
            match command {
                "engine.commands" => list(&["engine.commands", "factory.at"]),
                "ui.commands" => list(&["ui.inspect", "timeline.view.fit"]),
                "timeline.view.fit" => Ok("{\"fit\":true}".into()),
                _ => Err(ProtocolError::Ui { message: "no".into() }),
            }
        }
    }

    #[test]
    fn a_windows_tools_are_the_engines_then_its_own_and_errors_are_the_protocols() {
        let input = [
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"timeline_view_fit"}}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ui_set","arguments":{}}}"#,
        ]
        .join("\n");
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output, &mut Window).unwrap();
        let lines: Vec<serde_json::Value> = String::from_utf8(output).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let tools: ToolList = serde_json::from_value(lines[0]["result"].clone()).unwrap();
        let names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["engine_commands", "factory_at", "ui_inspect", "timeline_view_fit"]);
        let fit: CallResult = serde_json::from_value(lines[1]["result"].clone()).unwrap();
        assert_eq!((fit.is_error, fit.content[0].text.as_str()), (false, "{\"fit\":true}"));
        let refused: CallResult = serde_json::from_value(lines[2]["result"].clone()).unwrap();
        assert!(refused.is_error);
        assert_eq!(serde_json::from_str::<ProtocolError>(&refused.content[0].text).unwrap(), ProtocolError::Ui { message: "no".into() });
    }
}
