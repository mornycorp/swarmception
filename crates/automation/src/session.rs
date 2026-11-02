//! The headless handler: a session of its own, no window.

use swarmception_engine::Session;

use crate::protocol::{Handler, ProtocolError, needs_window};

/// Runs requests on a headless session: every engine command; `ui.*` and the widget's
/// `timeline.<id>` have no window to answer.
#[derive(Default)]
pub struct SessionHandler {
    pub session: Session,
}

impl SessionHandler {
    #[must_use]
    pub const fn new(session: Session) -> Self {
        Self { session }
    }
}

impl std::fmt::Debug for SessionHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandler").finish_non_exhaustive()
    }
}

impl Handler for SessionHandler {
    fn handle(&mut self, command: &str, params: &str) -> Result<String, ProtocolError> {
        if needs_window(command) {
            return Err(ProtocolError::NoUi);
        }
        self.session.execute(command, params).map_err(ProtocolError::from)
    }
}
