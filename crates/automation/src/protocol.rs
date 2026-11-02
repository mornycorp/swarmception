//! The control protocol: JSON lines, one [`Request`] a line in and one [`Reply`] a line out, each
//! request carrying the protocol's version and the server's token. A line is at most [`MAX_LINE`]
//! bytes. Anything that isn't a valid request is answered with a typed [`ProtocolError`] and
//! changes nothing.
//!
//! A request's `command` is one of:
//!
//! - an engine command's id (`engine.commands` lists them with their schemas), run on the
//!   session;
//! - `ui.commands`, `ui.inspect`, `ui.set`, `ui.screenshot`: the window's own (none headless);
//! - `timeline.<id>`, where `<id>` is the timeline widget's own command (`timeline.view.fit`,
//!   `timeline.state.get`, `timeline.engine.commands` lists them): forwarded to the widget (none
//!   headless). The engine's own `timeline.*` ids have one dot, the widget's two.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_engine::EngineError;

/// The request's shape is the widget's protocol's: `{"v", "token", "id", "command", "params"}`.
pub use timeline_automation::protocol::{MAX_LINE, Request};

/// The version of the protocol this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// The prefix that forwards a command to the timeline widget.
pub const WIDGET_PREFIX: &str = "timeline.";

/// The widget's command a `timeline.<id>` request forwards, if it is one: `timeline.view.fit` is
/// the widget's `view.fit`; `timeline.placeAt` is the engine's own.
#[must_use]
pub fn widget_command(command: &str) -> Option<&str> {
    command.strip_prefix(WIDGET_PREFIX).filter(|rest| rest.contains('.'))
}

/// Whether `command` is the window's own (`ui.*`) or the widget's (`timeline.<id>`): none of
/// them runs headless.
#[must_use]
pub fn needs_window(command: &str) -> bool {
    command.starts_with("ui.") || widget_command(command).is_some()
}

/// Why a request wasn't run, or what the command refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "problem", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProtocolError {
    /// The line isn't a request: not UTF-8, not JSON, or not a request's shape.
    BadRequest { message: String },
    /// The line is longer than [`MAX_LINE`].
    TooLong { limit: usize },
    /// The request speaks another version.
    WrongVersion { got: u32, speaks: u32 },
    /// The token isn't the server's.
    BadToken,
    /// A `ui.*` or `timeline.<id>` command on a server with no window.
    NoUi,
    /// The server couldn't hand the request to the window, or got no answer in time.
    Unavailable { message: String },
    /// The server holds as many connections as it takes: close one, or try again later.
    Busy { most: usize },
    /// The engine's own error: an unknown command, bad params, disabled, a refusal. A `ui.*`
    /// command's bad params are this too.
    Engine { error: EngineError },
    /// The timeline widget's error, for a `timeline.<id>` command.
    Widget { error: timeline_engine::EngineError },
    /// The window couldn't do it: a screenshot that couldn't be saved, say.
    Ui { message: String },
}

impl ProtocolError {
    /// The stable code a reply carries, as `problem`.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::BadRequest { .. } => "bad_request",
            Self::TooLong { .. } => "too_long",
            Self::WrongVersion { .. } => "wrong_version",
            Self::BadToken => "bad_token",
            Self::NoUi => "no_ui",
            Self::Unavailable { .. } => "unavailable",
            Self::Busy { .. } => "busy",
            Self::Engine { .. } => "engine",
            Self::Widget { .. } => "widget",
            Self::Ui { .. } => "ui",
        }
    }
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest { message } => write!(f, "not a request: {message}"),
            Self::TooLong { limit } => write!(f, "a line over {limit} bytes"),
            Self::WrongVersion { got, speaks } => write!(f, "protocol version {got}; this server speaks {speaks}"),
            Self::BadToken => write!(f, "not the server's token"),
            Self::NoUi => write!(f, "no window here: ui.* and timeline.<id> need the app's --control"),
            Self::Unavailable { message } | Self::Ui { message } => write!(f, "{message}"),
            Self::Busy { most } => write!(f, "busy: {most} connections already"),
            Self::Engine { error } => write!(f, "{}: {error}", error.code()),
            Self::Widget { error } => write!(f, "the timeline widget: {}: {error}", error.code()),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<EngineError> for ProtocolError {
    fn from(error: EngineError) -> Self {
        Self::Engine { error }
    }
}

/// A reply as a client reads it: the request's id (none when the line wasn't a request), and a
/// result or an error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reply<T> {
    pub id: Option<u64>,
    /// Missing when there is an error (an `Option` field reads as none when it is missing).
    pub result: Option<T>,
    pub error: Option<ProtocolError>,
}

/// What runs a request's command: the headless session, or the window's.
pub trait Handler {
    /// Runs `command` with `params`, JSON text (empty for none), and answers its result as JSON
    /// text.
    ///
    /// # Errors
    ///
    /// A [`ProtocolError`] when the command can't run or refuses.
    fn handle(&mut self, command: &str, params: &str) -> Result<String, ProtocolError>;
}

/// The reply to `line` from a server whose token is `token`.
pub fn respond(line: &str, token: &str, handler: &mut impl Handler) -> String {
    answer(line, token, handler).0
}

/// [`respond`]'s reply, and whether the line carried the server's token: a connection whose
/// first line doesn't is closed.
pub fn answer(line: &str, token: &str, handler: &mut impl Handler) -> (String, bool) {
    let request: Request = match serde_json::from_str(line) {
        Ok(request) => request,
        Err(e) => return (error_line(None, ProtocolError::BadRequest { message: e.to_string() }), false),
    };
    let id = Some(request.id);
    if request.v != PROTOCOL_VERSION {
        return (error_line(id, ProtocolError::WrongVersion { got: request.v, speaks: PROTOCOL_VERSION }), false);
    }
    if !same(&request.token, token) {
        return (error_line(id, ProtocolError::BadToken), false);
    }
    let params = request.params.as_ref().map_or_else(String::new, ToString::to_string);
    let reply = match handler.handle(&request.command, &params) {
        // The result is the command's JSON text, written straight into the reply.
        Ok(result) => format!("{{\"id\":{},\"result\":{result}}}", request.id),
        Err(error) => error_line(id, error),
    };
    (reply, true)
}

/// An error reply.
#[must_use]
pub fn error_line(id: Option<u64>, error: ProtocolError) -> String {
    let reply: Reply<()> = Reply { id, result: None, error: Some(error) };
    serde_json::to_string(&reply).unwrap_or_else(|_| r#"{"id":null,"error":{"problem":"bad_request","message":"unreportable"}}"#.to_owned())
}

/// Whether `a` and `b` are the same token, looking at every byte whatever the first difference.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;

    impl Handler for Echo {
        fn handle(&mut self, command: &str, params: &str) -> Result<String, ProtocolError> {
            match command {
                "echo" => Ok(if params.is_empty() { "null".into() } else { params.to_owned() }),
                other => Err(EngineError::UnknownCommand { command: other.to_owned() }.into()),
            }
        }
    }

    fn reply(line: &str) -> Reply<serde_json::Value> {
        serde_json::from_str(&respond(line, "tok", &mut Echo)).unwrap()
    }

    #[test]
    fn requests_are_answered_and_bad_ones_are_typed_errors() {
        let ok = reply(r#"{"v":1,"token":"tok","id":3,"command":"echo","params":{"a":1}}"#);
        assert_eq!((ok.id, ok.result, ok.error), (Some(3), Some(serde_json::json!({"a": 1})), None));
        let error = |line: &str| reply(line).error.unwrap();
        assert!(matches!(error("nope"), ProtocolError::BadRequest { .. }));
        assert!(matches!(error(r#"{"v":1,"token":"tok","id":1,"command":"echo","extra":1}"#), ProtocolError::BadRequest { .. }));
        assert_eq!(error(r#"{"v":2,"token":"tok","id":1,"command":"echo"}"#), ProtocolError::WrongVersion { got: 2, speaks: 1 });
        assert_eq!(error(r#"{"v":1,"token":"tak","id":1,"command":"echo"}"#), ProtocolError::BadToken);
        assert_eq!(error(r#"{"v":1,"token":"to","id":1,"command":"echo"}"#), ProtocolError::BadToken);
        assert_eq!(
            error(r#"{"v":1,"token":"tok","id":1,"command":"x.y"}"#),
            ProtocolError::Engine { error: EngineError::UnknownCommand { command: "x.y".into() } }
        );
    }

    #[test]
    fn the_widgets_commands_have_two_dots() {
        assert_eq!(widget_command("timeline.view.fit"), Some("view.fit"));
        assert_eq!(widget_command("timeline.engine.commands"), Some("engine.commands"));
        assert_eq!(widget_command("timeline.placeAt"), None);
        assert_eq!(widget_command("factory.at"), None);
        assert!(needs_window("ui.inspect") && needs_window("timeline.state.get"));
        assert!(!needs_window("timeline.timeAt") && !needs_window("engine.commands"));
    }

    #[test]
    fn every_error_reads_back_with_its_code() {
        let errors = [
            ProtocolError::BadToken,
            ProtocolError::NoUi,
            ProtocolError::Busy { most: 8 },
            ProtocolError::Ui { message: "m".into() },
            ProtocolError::Widget { error: timeline_engine::EngineError::UnknownCommand { id: "a.b".into() } },
            ProtocolError::Engine { error: EngineError::BadParams { message: "m".into() } },
        ];
        for error in errors {
            let text = serde_json::to_string(&error).unwrap();
            assert!(text.contains(&format!("\"problem\":\"{}\"", error.code())), "{text}");
            assert_eq!(serde_json::from_str::<ProtocolError>(&text).unwrap(), error);
            assert!(!error.to_string().is_empty());
        }
    }
}
