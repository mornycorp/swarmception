//! Swarmception's engine (docs/architecture.md › How a command flows): a [`Session`] holding the
//! swarm it read and the rows it shows, and the registry of typed commands every front end
//! drives: the UI's menus and toolbar, the CLI, tests. Every action and query is a command with
//! an id (`repo.open`, `rows.set`, `rows.list`, …); `engine.commands` lists them.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

pub mod command;
pub mod commands;
pub mod engine_cmds;
pub mod error;
pub mod repo_cmds;
pub mod rows_cmds;
pub mod session;

pub use command::{Command, Entry};
pub use error::EngineError;
pub use session::Session;
