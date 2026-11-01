//! What Swarmception computes over the tracking events (docs/architecture.md › Layers, L1): each
//! agent instance's sessions, pauses, work on issues, sub-agents and marks ([`activity`]); each
//! ticket's life, from claim to merge ([`tickets`]); the rows the timeline shows, the merges onto
//! main and the tickets, flat or by worker ([`rows`]); the timeline widget's host data built
//! from those rows ([`timeline`]); and the swarm's statistics over a window, tickets landed per
//! active hour first ([`stats`]). Pure: no I/O, no clock.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

pub mod activity;
pub mod rows;
pub mod stats;
pub mod tickets;
pub mod timeline;

pub use activity::Activity;
pub use rows::{Grouping, Node, RowsSpec};
pub use stats::{Stats, Window, stats};
pub use tickets::{Phase, Ticket, tickets};
