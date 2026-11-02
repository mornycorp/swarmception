//! A typed client of the control channel: what tests and agents written in Rust drive a server
//! with, reading each reply as its result type.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::protocol::{PROTOCOL_VERSION, ProtocolError, Reply, Request};

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
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        let request = Request { v: PROTOCOL_VERSION, token: self.token.clone(), id, command: command.to_owned(), params: Some(params) };
        let line = serde_json::to_string(&request).map_err(|e| ClientError::BadReply(e.to_string()))?;
        let reply: Reply<T> = serde_json::from_str(&self.send_line(&line)?).map_err(|e| ClientError::BadReply(e.to_string()))?;
        match (reply.result, reply.error) {
            (_, Some(error)) => Err(ClientError::Protocol(error)),
            (Some(result), None) => Ok(result),
            (None, None) => Err(ClientError::BadReply("neither a result nor an error".into())),
        }
    }
}
