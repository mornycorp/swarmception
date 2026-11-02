//! MCP over stdio (the Model Context Protocol's newline-delimited JSON-RPC 2.0): every command a
//! [`Handler`] runs is a tool, named with `_` for its `.` (`factory.at` is `factory_at`,
//! `timeline.view.fit` is `timeline_view_fit`), taking the command's params and answering its
//! result as JSON text. The tools are the engine's (`engine.commands`) and, when the handler has
//! a window, the window's and the widget's (`ui.commands`): over the headless session
//! ([`crate::session::SessionHandler`]) the engine's alone, over a running window
//! ([`crate::client::Remote`], `swarmception-cli mcp --control <file>`) all of them, each run in
//! the window. As the widget's `timeline_automation::mcp`, over this app's handlers. A
//! `ui_screenshot` saved as PNG is answered as MCP's image too (#76: base64, `image/png`), up to
//! [`IMAGE_MOST`] bytes; past it, [`ProtocolError::ImageTooLarge`] says where the file is.

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

/// A piece of a tool's answer: the command's JSON text, or the PNG a `ui_screenshot` saved
/// (#76).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Content {
    Text {
        text: String,
    },
    /// An image, base64.
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

/// `tools/call`'s result: the command's result as JSON text, or, with `isError`, its
/// [`ProtocolError`]'s (the engine's error as `{"problem":"engine","error":…}`). A `ui_screenshot`
/// saved as PNG answers the image too, after its text (#76).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallResult {
    pub content: Vec<Content>,
    pub is_error: bool,
}

impl CallResult {
    /// The answer's text: its first text piece's, empty when it has none.
    #[must_use]
    pub fn text(&self) -> &str {
        self.content
            .iter()
            .find_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                Content::Image { .. } => None,
            })
            .unwrap_or_default()
    }

    /// The image it carries, base64, and its type.
    #[must_use]
    pub fn image(&self) -> Option<(&str, &str)> {
        self.content.iter().find_map(|c| match c {
            Content::Image { data, mime_type } => Some((data.as_str(), mime_type.as_str())),
            Content::Text { .. } => None,
        })
    }
}

/// The most bytes of PNG a `ui_screenshot`'s answer carries as an image (#76): 3 MiB, 4 MiB as
/// base64, under the 5 MB an image may be for a model. A bigger one is
/// [`ProtocolError::ImageTooLarge`], the file still saved where it says.
pub const IMAGE_MOST: u64 = 3 * 1024 * 1024;

/// The first bytes of every PNG.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// What a `ui.screenshot` answers that the image needs: where it saved it.
#[derive(Debug, Deserialize)]
struct Shot {
    path: String,
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

/// Serves MCP on `input` and `output` until the input ends, each tool run by `handler`; a
/// screenshot's PNG answered as an image up to [`IMAGE_MOST`] bytes.
///
/// # Errors
///
/// An I/O error reading or writing.
pub fn serve(input: impl BufRead, output: impl Write, handler: &mut impl Handler) -> io::Result<()> {
    serve_with(input, output, handler, IMAGE_MOST)
}

/// As [`serve`], a screenshot's PNG answered as an image up to `image_most` bytes.
///
/// # Errors
///
/// An I/O error reading or writing.
pub fn serve_with(mut input: impl BufRead, mut output: impl Write, handler: &mut impl Handler, image_most: u64) -> io::Result<()> {
    loop {
        let reply = match read_line(&mut input, MAX_LINE)? {
            Line::End => return Ok(()),
            Line::Text(text) if text.trim().is_empty() => continue,
            Line::Text(text) => answer(handler, &text, image_most),
            Line::TooLong | Line::NotText => Some(error(&serde_json::Value::Null, PARSE_ERROR, "the line is too long or not UTF-8")),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
}

/// The reply to one message, none for a notification.
fn answer(handler: &mut impl Handler, text: &str, image_most: u64) -> Option<String> {
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
            Some(Ok(call)) => serde_json::to_value(call_tool(handler, &call, image_most)).unwrap_or_default(),
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

/// Runs the tool `call` names: its command, with its arguments. A screenshot saved as PNG is
/// answered as an image too, up to `image_most` bytes.
fn call_tool(handler: &mut impl Handler, call: &CallParams, image_most: u64) -> CallResult {
    let id = command_id(&call.name);
    let params = call.arguments.as_ref().map_or_else(String::new, ToString::to_string);
    let refused = |error: ProtocolError| CallResult {
        content: vec![Content::Text { text: serde_json::to_string(&error).unwrap_or_else(|_| error.to_string()) }],
        is_error: true,
    };
    match handler.handle(&id, &params) {
        Ok(result) if id == "ui.screenshot" => match screenshot_image(&result, image_most) {
            Ok(image) => CallResult { content: std::iter::once(Content::Text { text: result }).chain(image).collect(), is_error: false },
            Err(error) => refused(error),
        },
        Ok(result) => CallResult { content: vec![Content::Text { text: result }], is_error: false },
        Err(error) => refused(error),
    }
}

/// The PNG a `ui.screenshot` answered `result` for, as an image; none for a PPM.
///
/// # Errors
///
/// [`ProtocolError::ImageTooLarge`] past `most` bytes, [`ProtocolError::ImageUnreadable`] when it
/// can't be read back or isn't a PNG.
fn screenshot_image(result: &str, most: u64) -> Result<Option<Content>, ProtocolError> {
    let shot: Shot =
        serde_json::from_str(result).map_err(|e| ProtocolError::ImageUnreadable { path: String::new(), message: format!("the window's answer: {e}") })?;
    let path = std::path::Path::new(&shot.path);
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return Ok(None);
    }
    let bytes = read_png(path, most)?;
    Ok(Some(Content::Image { data: base64(&bytes), mime_type: "image/png".into() }))
}

/// The PNG at `path`, at most `most` bytes of it.
///
/// # Errors
///
/// [`ProtocolError::ImageTooLarge`] past `most` bytes; [`ProtocolError::ImageUnreadable`] when it
/// can't be read or doesn't start as a PNG does.
pub fn read_png(path: &std::path::Path, most: u64) -> Result<Vec<u8>, ProtocolError> {
    use std::io::Read;
    let shown = path.display().to_string();
    let unreadable = |message: String| ProtocolError::ImageUnreadable { path: shown.clone(), message };
    let too_large = |bytes: u64| ProtocolError::ImageTooLarge { path: shown.clone(), bytes, most };
    let file = std::fs::File::open(path).map_err(|e| unreadable(e.to_string()))?;
    let length = file.metadata().map_err(|e| unreadable(e.to_string()))?.len();
    if length > most {
        return Err(too_large(length));
    }
    let mut bytes = Vec::new();
    file.take(most.saturating_add(1)).read_to_end(&mut bytes).map_err(|e| unreadable(e.to_string()))?;
    let read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if read > most {
        return Err(too_large(read));
    }
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return Err(unreadable("not a PNG".into()));
    }
    Ok(bytes)
}

/// `bytes` as base64 (RFC 4648's alphabet, padded), as MCP carries an image.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let sextet = |n: u32, shift: u32| char::from(ALPHABET.get(((n >> shift) & 63) as usize).copied().unwrap_or(b'='));
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let [a, b, c] = [0, 1, 2].map(|i| u32::from(chunk.get(i).copied().unwrap_or(0)));
        let n = (a << 16) | (b << 8) | c;
        out.push(sextet(n, 18));
        out.push(sextet(n, 12));
        out.push(if chunk.len() > 1 { sextet(n, 6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(n, 0) } else { '=' });
    }
    out
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
        assert_eq!((fit.is_error, fit.text(), fit.content.len()), (false, "{\"fit\":true}", 1));
        let refused: CallResult = serde_json::from_value(lines[2]["result"].clone()).unwrap();
        assert!(refused.is_error);
        assert_eq!(serde_json::from_str::<ProtocolError>(refused.text()).unwrap(), ProtocolError::Ui { message: "no".into() });
    }

    #[test]
    fn base64_is_rfc_4648s() {
        let cases = [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")];
        for (plain, encoded) in cases {
            assert_eq!(base64(plain.as_bytes()), encoded, "{plain}");
        }
        assert_eq!(base64(&[0xfb, 0xff, 0xbf]), "+/+/");
        assert_eq!(base64(&PNG_SIGNATURE), "iVBORw0KGgo=");
    }

    /// A window whose screenshots write what it was given to the path asked for.
    struct Shooter(Vec<u8>);

    impl Handler for Shooter {
        fn handle(&mut self, command: &str, params: &str) -> Result<String, ProtocolError> {
            assert_eq!(command, "ui.screenshot");
            let shot: Shot = serde_json::from_str(params).unwrap();
            std::fs::write(&shot.path, &self.0).unwrap();
            Ok(serde_json::json!({ "path": shot.path, "width": 2, "height": 1 }).to_string())
        }
    }

    #[test]
    fn a_screenshots_png_comes_back_as_an_image_up_to_the_cap() {
        let dir = std::env::temp_dir().join(format!("worker32-mcp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png: Vec<u8> = PNG_SIGNATURE.iter().copied().chain(*b"IHDR and the rest").collect();
        let shoot = |bytes: &[u8], name: &str, most: u64| -> (CallResult, serde_json::Value) {
            let path = dir.join(name).display().to_string();
            let line = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "ui_screenshot", "arguments": {"path": path}}});
            let mut output = Vec::new();
            serve_with(line.to_string().as_bytes(), &mut output, &mut Shooter(bytes.to_vec()), most).unwrap();
            let reply: serde_json::Value = serde_json::from_slice(&output).unwrap();
            (serde_json::from_value(reply["result"].clone()).unwrap(), reply["result"].clone())
        };
        // Under the cap: the window's answer as text, then the PNG, base64, as MCP's image.
        let (shot, wire) = shoot(&png, "a.png", 64);
        assert!(!shot.is_error);
        assert!(shot.text().contains("a.png"));
        assert_eq!(shot.image(), Some((base64(&png).as_str(), "image/png")));
        assert_eq!(wire["content"][1], serde_json::json!({"type": "image", "data": base64(&png), "mimeType": "image/png"}));
        assert_eq!(wire["content"][0]["type"], "text");
        // At the cap still; past it, a typed error saying where the file is.
        assert!(!shoot(&png, "b.png", png.len() as u64).0.is_error);
        let (over, _) = shoot(&png, "c.png", png.len() as u64 - 1);
        let error: ProtocolError = serde_json::from_str(over.text()).unwrap();
        let path = dir.join("c.png").display().to_string();
        assert!(over.is_error);
        assert_eq!(error, ProtocolError::ImageTooLarge { path, bytes: png.len() as u64, most: png.len() as u64 - 1 });
        // Not a PNG: unreadable as one. A PPM: its text alone.
        let (garbled, _) = shoot(b"P6 not a png", "d.png", 64);
        assert!(matches!(serde_json::from_str(garbled.text()).unwrap(), ProtocolError::ImageUnreadable { .. }));
        let (ppm, _) = shoot(b"P6\n1 1\n255\n\0\0\0", "e.ppm", 64);
        assert_eq!((ppm.is_error, ppm.content.len(), ppm.image()), (false, 1, None));
        std::fs::remove_dir_all(dir).ok();
    }
}
