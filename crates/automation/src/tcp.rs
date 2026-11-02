//! The control channel over TCP: loopback only, a token, one thread a connection and at most
//! [`Limits::connections`] of them, every connection's requests run one at a time on the shared
//! handler. A connection's first line must come soon and carry the token, or it is closed.
//! The bind, the token and the limits are the widget's (`timeline_automation::tcp`).

use std::io::{self, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

pub use timeline_automation::tcp::{Limits, Listening, bind, new_token};

use crate::lines::{Line, read_line, refusal, serve_lines};
use crate::protocol::{Handler, MAX_LINE, ProtocolError, answer, error_line};

/// A handler shared by every connection, each request run under its lock.
#[derive(Debug)]
pub struct Shared<H>(pub Arc<Mutex<H>>);

impl<H> Shared<H> {
    pub fn new(handler: H) -> Self {
        Self(Arc::new(Mutex::new(handler)))
    }
}

impl<H> Clone for Shared<H> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<H: Handler> Handler for Shared<H> {
    fn handle(&mut self, command: &str, params: &str) -> Result<String, ProtocolError> {
        // A command that panicked is caught inside the engine; the lock is still good to use.
        let mut handler = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        handler.handle(command, params)
    }
}

/// Writes where a server listens and its token to `path`, one JSON line, readable by its owner
/// only where the system has permissions: for a caller that would rather not read stdout.
///
/// # Errors
///
/// The write's I/O error.
pub fn write_listening(path: &Path, listening: &Listening) -> io::Result<()> {
    let text = serde_json::to_string(listening).map_err(io::Error::other)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    writeln!(file, "{text}")?;
    file.flush()
}

/// Serves `listener`'s connections forever, each on its own thread over `handler`, within
/// [`Limits::DEFAULT`].
pub fn serve<H: Handler + Send + 'static>(listener: &TcpListener, token: &str, handler: &Shared<H>) {
    serve_within(listener, token, handler, Limits::DEFAULT);
}

/// Serves `listener`'s connections forever, each on its own thread over `handler`, within
/// `limits`.
pub fn serve_within<H: Handler + Send + 'static>(listener: &TcpListener, token: &str, handler: &Shared<H>, limits: Limits) {
    let open = Arc::new(AtomicUsize::new(0));
    for mut stream in listener.incoming().flatten() {
        if open.load(Ordering::Acquire) >= limits.connections {
            let _ = writeln!(stream, "{}", error_line(None, ProtocolError::Busy { most: limits.connections }));
            let _ = stream.shutdown(Shutdown::Write);
            continue;
        }
        let slot = Slot::take(&open);
        let (token, mut handler) = (token.to_owned(), handler.clone());
        // A thread that can't start drops its slot and its stream: the connection closes.
        let _ = thread::Builder::new().name("control connection".into()).spawn(move || {
            let _slot = slot;
            let _ = connection(&stream, &token, &mut handler, limits);
        });
    }
}

/// One open connection's place in the count, given back when it closes.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(open: &Arc<AtomicUsize>) -> Self {
        open.fetch_add(1, Ordering::AcqRel);
        Self(Arc::clone(open))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// One connection's requests, until it closes, goes idle too long, or its first line lacks the
/// token.
fn connection(stream: &TcpStream, token: &str, handler: &mut impl Handler, limits: Limits) -> io::Result<()> {
    stream.set_read_timeout(Some(limits.first_line))?;
    let mut input = BufReader::new(stream.try_clone()?);
    let (reply, trusted) = match read_line(&mut input, MAX_LINE)? {
        Line::End => return Ok(()),
        Line::Text(text) => answer(&text, token, handler),
        unreadable => (refusal(&unreadable), false),
    };
    let mut output = stream;
    writeln!(output, "{reply}")?;
    output.flush()?;
    if !trusted {
        return Ok(());
    }
    stream.set_read_timeout(Some(limits.idle))?;
    serve_lines(input, stream, token, handler)
}
