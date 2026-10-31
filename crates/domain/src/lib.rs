//! What Swarmception computes over the tracking events (docs/architecture.md › Layers, L1): each
//! agent instance's sessions, pauses, work on issues, sub-agents and marks ([`activity`]); the
//! rows the timeline shows, by worker or by ticket, as a tree or flat ([`rows`]); and the
//! timeline widget's host data built from those rows ([`timeline`]). Pure: no I/O, no clock.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

pub mod activity;
pub mod rows;
pub mod timeline;

pub use activity::Activity;
pub use rows::{Grouping, Node, RowsBy, RowsSpec};
