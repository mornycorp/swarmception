//! Automation (docs/architecture.md › Automation): Swarmception's control protocol (JSON lines,
//! loopback only, a token; [`protocol`]), the TCP server ([`tcp`]), the handler for a session that
//! lives on the UI thread ([`channel`]), the headless handler ([`session`]), the MCP server over
//! stdio ([`mcp`]) and a typed client ([`client`]). Each is a thin transport over the engine's
//! command registry: a command is a control method and an MCP tool the moment it is registered.
//!
//! It follows swarm-timeline's `timeline-automation` and reuses its pieces that carry no types
//! of the widget's: the bounded line reader, the token, the loopback bind and the limits.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

pub mod channel;
pub mod client;
pub mod lines;
pub mod mcp;
pub mod protocol;
pub mod session;
pub mod tcp;

pub use protocol::{Handler, PROTOCOL_VERSION, ProtocolError, Reply, Request};
