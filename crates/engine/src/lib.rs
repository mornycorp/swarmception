//! Swarmception's engine (docs/architecture.md › How a command flows): a [`Session`] holding the
//! swarm it read and the rows it shows, and the registry of typed commands every front end
//! drives: the UI's menus and toolbar, the CLI, tests. Every action and query is a command with
//! an id (`repo.open`, `rows.set`, `rows.list`, `stats.get`, `factory.at`, `details.get`, `live.set`, …); `engine.commands` lists them.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

pub mod command;
pub mod commands;
pub mod details_cmds;
pub mod engine_cmds;
pub mod error;
pub mod factory_cmds;
pub mod gates_cmds;
pub mod issues;
pub mod issues_cmds;
pub mod live;
pub mod live_cmds;
pub mod repo_cmds;
pub mod rows_cmds;
pub mod session;
pub mod stats_cmds;

pub use command::{Command, Entry};
pub use error::EngineError;
pub use session::Session;
pub use swarmception_source::Github;
