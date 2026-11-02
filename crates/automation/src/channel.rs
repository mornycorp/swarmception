//! A handler for a session that lives on another thread, the window's on its UI thread: each
//! request is sent there as a [`Call`], the owner is woken, and its answer is waited for. The
//! owner may answer later (a screenshot lands a frame or two after it is asked for) by keeping
//! the call's [`Responder`].

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use crate::protocol::{Handler, ProtocolError};

/// How long a request waits for the owner's answer by default.
pub const ANSWER_WITHIN: Duration = Duration::from_secs(20);

/// Where a call's answer goes, to answer it now or later; a requester that has gone is no error.
#[derive(Debug, Clone)]
pub struct Responder(Sender<Result<String, ProtocolError>>);

impl Responder {
    pub fn send(&self, answer: Result<String, ProtocolError>) {
        let _ = self.0.send(answer);
    }
}

/// One request for the session's owner to answer.
#[derive(Debug)]
pub struct Call {
    pub command: String,
    pub params: String,
    pub reply: Responder,
}

impl Call {
    /// Answers the call with what `answer` returns for its command and params.
    pub fn answer(self, answer: impl FnOnce(&str, &str) -> Result<String, ProtocolError>) {
        let result = answer(&self.command, &self.params);
        self.reply.send(result);
    }
}

/// Sends each request to the owner as a [`Call`] and waits for its answer.
pub struct ChannelHandler {
    calls: Sender<Call>,
    /// Wakes the owner: a UI thread repaints, so it answers.
    wake: Box<dyn Fn() + Send>,
    within: Duration,
}

impl ChannelHandler {
    /// A handler whose calls arrive on the returned receiver; `wake` is run after each is sent.
    #[must_use]
    pub fn new(wake: impl Fn() + Send + 'static) -> (Self, Receiver<Call>) {
        Self::within(wake, ANSWER_WITHIN)
    }

    /// As [`ChannelHandler::new`], each request waiting at most `within`.
    #[must_use]
    pub fn within(wake: impl Fn() + Send + 'static, within: Duration) -> (Self, Receiver<Call>) {
        let (calls, receiver) = channel();
        (Self { calls, wake: Box::new(wake), within }, receiver)
    }
}

impl std::fmt::Debug for ChannelHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelHandler").finish_non_exhaustive()
    }
}

impl Handler for ChannelHandler {
    fn handle(&mut self, command: &str, params: &str) -> Result<String, ProtocolError> {
        let gone = || ProtocolError::Unavailable { message: "the window is gone".into() };
        let (reply, answer) = channel();
        self.calls.send(Call { command: command.to_owned(), params: params.to_owned(), reply: Responder(reply) }).map_err(|_| gone())?;
        (self.wake)();
        match answer.recv_timeout(self.within) {
            Ok(result) => result,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(gone()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                Err(ProtocolError::Unavailable { message: format!("no answer within {} s", self.within.as_secs()) })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_reach_the_owner_and_its_answers_come_back() {
        let (mut handler, calls) = ChannelHandler::within(|| {}, Duration::from_millis(300));
        let owner = std::thread::spawn(move || {
            for call in calls.iter().take(3) {
                match call.command.as_str() {
                    "echo" => call.answer(|_, params| Ok(params.to_owned())),
                    // Dropped unanswered.
                    "never" => drop(call),
                    _ => call.answer(|_, _| Err(ProtocolError::NoUi)),
                }
            }
        });
        assert_eq!(handler.handle("echo", "{\"a\":1}"), Ok("{\"a\":1}".to_owned()));
        // Never answered, and dropped: an error, not a hang.
        assert!(matches!(handler.handle("never", ""), Err(ProtocolError::Unavailable { .. })));
        assert_eq!(handler.handle("ui.inspect", ""), Err(ProtocolError::NoUi));
        owner.join().unwrap();
        assert!(matches!(handler.handle("echo", ""), Err(ProtocolError::Unavailable { .. })));
    }

    #[test]
    fn a_kept_responder_answers_later() {
        let (mut handler, calls) = ChannelHandler::new(|| {});
        let owner = std::thread::spawn(move || {
            let call = calls.recv().unwrap();
            std::thread::sleep(Duration::from_millis(50));
            call.reply.send(Ok("\"saved\"".into()));
        });
        assert_eq!(handler.handle("ui.screenshot", "{}"), Ok("\"saved\"".to_owned()));
        owner.join().unwrap();
    }
}
