//! A typed client of the control channel: what tests and agents written in Rust drive a server
//! with, reading each reply as its result type ([`Client`]); and [`Remote`], a [`Handler`] that
//! runs each request on a server through a client, which `swarmception-cli send` and
//! `mcp --control` drive a window with.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::protocol::{Handler, PROTOCOL_VERSION, ProtocolError, Reply, Request};
use crate::tcp::Listening;

/// How long a [`Remote`] waits for a reply: longer than the window takes to answer
/// (`channel::ANSWER_WITHIN`), so the window's own timeout is the one a caller reads.
pub const REPLY_WITHIN: Duration = Duration::from_secs(30);

/// Why a call came back with no result.
#[derive(Debug)]
pub enum ClientError {
    /// The connection failed.
    Io(io::Error),
    /// The server answered with an error.
    Protocol(ProtocolError),
    /// The reply didn't read as the result asked for.
    BadReply(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Protocol(e) => write!(f, "{e}"),
            Self::BadReply(m) => write!(f, "a reply that doesn't read: {m}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<io::Error> for ClientError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// One connection to a server.
pub struct Client {
    input: BufReader<TcpStream>,
    output: TcpStream,
    token: String,
    next: u64,
}

impl Client {
    /// Connects to `address`, whose token is `token`; each reply waited for at most `timeout`.
    ///
    /// # Errors
    ///
    /// The connection's I/O error.
    pub fn connect(address: SocketAddr, token: &str, timeout: Duration) -> io::Result<Self> {
        let output = TcpStream::connect_timeout(&address, timeout)?;
        output.set_read_timeout(Some(timeout))?;
        let input = BufReader::new(output.try_clone()?);
        Ok(Self { input, output, token: token.to_owned(), next: 1 })
    }

    /// Sends `line` as it is and reads the reply's line: for a test of what a bad line gets.
    ///
    /// # Errors
    ///
    /// The connection's I/O error, or its end.
    pub fn send_line(&mut self, line: &str) -> io::Result<String> {
        writeln!(self.output, "{line}")?;
        self.output.flush()?;
        let mut reply = String::new();
        if self.input.read_line(&mut reply)? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the server closed the connection"));
        }
        Ok(reply.trim_end().to_owned())
    }

    /// Runs `command` with `params` and reads its result as `T`.
    ///
    /// # Errors
    ///
    /// [`ClientError`]: the connection, the server's error, or a reply that isn't a `T`.
    pub fn call<T: DeserializeOwned>(&mut self, command: &str, params: &impl Serialize) -> Result<T, ClientError> {
        let params = serde_json::to_value(params).map_err(|e| ClientError::BadReply(e.to_string()))?;
        self.send(command, Some(params))
    }

    /// Runs `command` with `params`, JSON text (empty for none), and answers its result as JSON
    /// text: for a caller that passes results on rather than reading them.
    ///
    /// # Errors
    ///
    /// [`ClientError`]: the connection, the server's error, params that aren't JSON (a
    /// `bad_request`, sent nowhere) or a reply that isn't.
    pub fn request(&mut self, command: &str, params: &str) -> Result<String, ClientError> {
        let params = if params.trim().is_empty() {
            None
        } else {
            Some(serde_json::from_str(params).map_err(|e| ClientError::Protocol(ProtocolError::BadRequest { message: format!("params: {e}") }))?)
        };
        let result: serde_json::Value = self.send(command, params)?;
        Ok(result.to_string())
    }

    fn send<T: DeserializeOwned>(&mut self, command: &str, params: Option<serde_json::Value>) -> Result<T, ClientError> {
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        let request = Request { v: PROTOCOL_VERSION, token: self.token.clone(), id, command: command.to_owned(), params };
        let line = serde_json::to_string(&request).map_err(|e| ClientError::BadReply(e.to_string()))?;
        let reply: Reply<T> = serde_json::from_str(&self.send_line(&line)?).map_err(|e| ClientError::BadReply(e.to_string()))?;
        match (reply.result, reply.error) {
            (_, Some(error)) => Err(ClientError::Protocol(error)),
            (Some(result), None) => Ok(result),
            (None, None) => Err(ClientError::BadReply("neither a result nor an error".into())),
        }
    }
}

/// A server reached through a [`Client`]: each request runs there, as if handled here. It
/// connects on the first request; a connection the server closed meanwhile (after ten idle
/// minutes) is opened again and the request sent once more, since the server read none of it.
/// A `ui.screenshot`'s relative path is made this process's ([`own_paths`]).
pub struct Remote {
    listening: Listening,
    within: Duration,
    client: Option<Client>,
}

impl Remote {
    /// A remote of the server `listening` names, each reply waited for at most `within`.
    #[must_use]
    pub const fn new(listening: Listening, within: Duration) -> Self {
        Self { listening, within, client: None }
    }

    /// A remote of the server whose control file is `path` (`--control-file`), each reply
    /// waited for at most [`REPLY_WITHIN`].
    ///
    /// # Errors
    ///
    /// The file's error ([`crate::tcp::read_listening`]).
    pub fn from_file(path: &Path) -> io::Result<Self> {
        Ok(Self::new(crate::tcp::read_listening(path)?, REPLY_WITHIN))
    }

    /// Where the server listens.
    #[must_use]
    pub const fn listening(&self) -> &Listening {
        &self.listening
    }

    /// Runs one request on the open connection, opening one first; a connection that failed
    /// is dropped.
    fn once(&mut self, command: &str, params: &str) -> Result<String, ClientError> {
        let client = match &mut self.client {
            Some(client) => client,
            none => none.insert(Client::connect(self.listening.address, &self.listening.token, self.within)?),
        };
        let result = client.request(command, params);
        if matches!(result, Err(ClientError::Io(_) | ClientError::BadReply(_))) {
            self.client = None;
        }
        result
    }
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Remote").field("address", &self.listening.address).finish_non_exhaustive()
    }
}

/// Whether `error` says the server closed the connection before reading a request: sent again,
/// it runs once.
fn closed(error: &ClientError) -> bool {
    use io::ErrorKind::{BrokenPipe, ConnectionAborted, ConnectionReset, UnexpectedEof};
    matches!(error, ClientError::Io(e) if matches!(e.kind(), UnexpectedEof | ConnectionReset | BrokenPipe | ConnectionAborted))
}

impl Handler for Remote {
    fn handle(&mut self, command: &str, params: &str) -> Result<String, ProtocolError> {
        let params = own_paths(command, params);
        let reused = self.client.is_some();
        let result = match self.once(command, &params) {
            Err(error) if reused && closed(&error) => self.once(command, &params),
            other => other,
        };
        let at = self.listening.address;
        result.map_err(|error| match error {
            ClientError::Protocol(error) => error,
            ClientError::Io(e) => ProtocolError::Unavailable { message: format!("the server at {at}: {e}") },
            ClientError::BadReply(m) => ProtocolError::Unavailable { message: format!("the server at {at}: a reply that doesn't read: {m}") },
        })
    }
}

/// `params` with a `ui.screenshot`'s relative `path` made absolute against this process's
/// working directory, since the window's may be another; any other request's as they are.
#[must_use]
pub fn own_paths(command: &str, params: &str) -> String {
    if command != "ui.screenshot" {
        return params.to_owned();
    }
    let Ok(serde_json::Value::Object(mut map)) = serde_json::from_str::<serde_json::Value>(params) else {
        return params.to_owned();
    };
    let Some(serde_json::Value::String(path)) = map.get("path") else {
        return params.to_owned();
    };
    let (path, Ok(here)) = (Path::new(path), std::env::current_dir()) else {
        return params.to_owned();
    };
    if path.is_absolute() {
        return params.to_owned();
    }
    let path = here.join(path).display().to_string();
    map.insert("path".into(), serde_json::Value::String(path));
    serde_json::Value::Object(map).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screenshots_relative_path_is_made_this_processs() {
        let here = std::env::current_dir().unwrap();
        let made: serde_json::Value = serde_json::from_str(&own_paths("ui.screenshot", r#"{"path":"shots/a.png"}"#)).unwrap();
        assert_eq!(made["path"], here.join("shots/a.png").display().to_string());
        let absolute = serde_json::json!({ "path": here.join("a.png").display().to_string() }).to_string();
        assert_eq!(own_paths("ui.screenshot", &absolute), absolute);
        for (command, params) in [("ui.set", r#"{"path":"a.png"}"#), ("ui.screenshot", ""), ("ui.screenshot", "{not json"), ("ui.screenshot", r#"{"path":7}"#)]
        {
            assert_eq!(own_paths(command, params), params, "{command} {params}");
        }
    }

    #[test]
    fn a_server_that_isnt_there_is_unavailable() {
        let (listener, listening) = crate::tcp::bind(0, None).unwrap();
        drop(listener);
        let mut remote = Remote::new(listening, Duration::from_secs(2));
        assert!(matches!(remote.handle("engine.commands", ""), Err(ProtocolError::Unavailable { .. })));
        assert!(matches!(remote.handle("ui.screenshot", r#"{"path":"a.png"}"#), Err(ProtocolError::Unavailable { .. })));
    }
}
