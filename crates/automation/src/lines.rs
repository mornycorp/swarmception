//! The line loop every transport runs: read a line of at most [`MAX_LINE`] bytes (the widget's
//! bounded reader), answer it, until the input ends.

use std::io::{self, BufRead, Write};

pub use timeline_automation::lines::{Line, read_line};

use crate::protocol::{Handler, MAX_LINE, ProtocolError, error_line, respond};

/// Answers each line of `input` on `output` with `handler`, for a server whose token is `token`,
/// until the input ends. Blank lines are skipped.
///
/// # Errors
///
/// An I/O error reading or writing.
pub fn serve_lines(mut input: impl BufRead, mut output: impl Write, token: &str, handler: &mut impl Handler) -> io::Result<()> {
    loop {
        let reply = match read_line(&mut input, MAX_LINE)? {
            Line::End => return Ok(()),
            Line::Text(text) if text.trim().is_empty() => continue,
            Line::Text(text) => respond(&text, token, handler),
            unreadable => refusal(&unreadable),
        };
        writeln!(output, "{reply}")?;
        output.flush()?;
    }
}

/// The error reply to a line that can't be read as a request: too long, or not UTF-8.
#[must_use]
pub fn refusal(line: &Line) -> String {
    match line {
        Line::TooLong => error_line(None, ProtocolError::TooLong { limit: MAX_LINE }),
        Line::Text(_) | Line::NotText | Line::End => error_line(None, ProtocolError::BadRequest { message: "the line isn't UTF-8".into() }),
    }
}
