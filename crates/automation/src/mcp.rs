//! MCP over stdio (the Model Context Protocol's newline-delimited JSON-RPC 2.0): every engine
//! command is a tool, named with `_` for its `.` (`factory.at` is `factory_at`), taking the
//! command's params and answering its result as JSON text. One headless session, the caller's
//! (opened on a repository or not). As the widget's `timeline_automation::mcp`, over this
//! engine's registry.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use swarmception_engine::Session;
use swarmception_engine::engine_cmds::{Commands, NoParams};

use crate::lines::{Line, read_line};
use crate::protocol::MAX_LINE;

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

/// `tools/call`'s result: the command's result as JSON text, or its error's.
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

/// The command's id for the tool `name`: ids are `area.name` with no `_`.
#[must_use]
pub fn command_id(name: &str) -> String {
    name.replace('_', ".")
}

/// Serves MCP on `input` and `output` until the input ends, over `session`.
///
/// # Errors
///
/// An I/O error reading or writing.
pub fn serve(mut input: impl BufRead, mut output: impl Write, session: &mut Session) -> io::Result<()> {
    loop {
        let reply = match read_line(&mut input, MAX_LINE)? {
            Line::End => return Ok(()),
            Line::Text(text) if text.trim().is_empty() => continue,
            Line::Text(text) => answer(session, &text),
            Line::TooLong | Line::NotText => Some(error(&serde_json::Value::Null, PARSE_ERROR, "the line is too long or not UTF-8")),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
}

/// The reply to one message, none for a notification.
fn answer(session: &mut Session, text: &str) -> Option<String> {
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
        "tools/list" => match tools(session) {
            Ok(tools) => serde_json::to_value(tools).unwrap_or_default(),
            Err(message) => return Some(error(&id, INVALID_PARAMS, &message)),
        },
        "tools/call" => match message.params.map(serde_json::from_value::<CallParams>) {
            Some(Ok(call)) => serde_json::to_value(call_tool(session, &call)).unwrap_or_default(),
            _ => return Some(error(&id, INVALID_PARAMS, "tools/call takes a name and arguments")),
        },
        other => return Some(error(&id, METHOD_NOT_FOUND, &format!("no method {}", other.chars().take(64).collect::<String>()))),
    };
    Some(serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string())
}

/// Every engine command as a tool.
fn tools(session: &mut Session) -> Result<ToolList, String> {
    let list = session.call::<Commands>(NoParams {}).map_err(|e| e.to_string())?;
    let tools = list
        .commands
        .into_iter()
        .map(|command| Tool { name: tool_name(&command.id), description: format!("{} ({})", command.label, command.id), input_schema: command.params })
        .collect();
    Ok(ToolList { tools })
}

/// Runs the tool `call` names: its command, with its arguments.
fn call_tool(session: &mut Session, call: &CallParams) -> CallResult {
    let id = command_id(&call.name);
    let params = call.arguments.as_ref().map_or_else(String::new, ToString::to_string);
    let text = |text: String| vec![Text { r#type: "text".into(), text }];
    match session.execute(&id, &params) {
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
    }
}
