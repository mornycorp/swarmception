//! What the UI's tests share (#96): frames until something holds, with a deadline only a
//! failure reaches. Each test binary takes in this module.

use std::time::{Duration, Instant};

/// How long a repository's background reads may take before a test fails: far longer than any
/// takes, even on a machine loaded with builds (a count of frames was not: frames slow down with
/// the machine). Under nextest's slow-timeout kill.
pub const LOAD_WITHIN: Duration = Duration::from_secs(120);

/// Runs `step` (a frame, say, and whether what the test waits for holds) until it says so, every
/// 10 ms; past [`LOAD_WITHIN`] the test fails saying `what`.
pub fn until(what: &str, mut step: impl FnMut() -> bool) {
    let deadline = Instant::now() + LOAD_WITHIN;
    while !step() {
        assert!(Instant::now() < deadline, "{what}: not within {} s", LOAD_WITHIN.as_secs());
        std::thread::sleep(Duration::from_millis(10));
    }
}
