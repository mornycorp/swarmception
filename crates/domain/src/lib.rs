//! What Swarmception computes over the tracking events (docs/architecture.md › Layers, L1): each
//! agent instance's sessions, pauses, work on issues, sub-agents and marks ([`activity`]); each
//! ticket's life, from claim to merge ([`tickets`]), sent back by a `sendback` or a gate's failure
//! ([`sendbacks`]); the colours of tickets and phases, one source
//! for the timeline and the factory ([`colours`]); the rows the timeline shows, the merges onto
//! main and the tickets, flat or by worker ([`rows`]); the timeline widget's host data built
//! from those rows ([`timeline`]); the swarm's statistics over a window, tickets landed per
//! active hour first ([`stats`]); the gate's runs over time and their figures ([`gates`]); the swarm at one moment as a factory building the app
//! ([`factory`]), and its key moments to step through ([`moments`]); what GitHub's issues say
//! of a ticket, its title on bars and first in tooltips ([`titles`]); a tooltip's lines in
//! colour, the bars' ([`tooltip`]); the story of what is selected, in order, for the details
//! pane ([`details`]); and how each worker did, by model × effort ([`workers`]). Pure: no I/O,
//! no clock.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

pub mod activity;
pub mod colours;
pub mod details;
pub mod factory;
pub mod gates;
pub mod moments;
pub mod rows;
pub mod sendbacks;
pub mod stats;
pub mod tickets;
pub mod timeline;
pub mod titles;
pub mod tooltip;
pub mod workers;

pub use activity::Activity;
pub use rows::{Collapse, Grouping, Node, RowsSpec};
pub use stats::{Stats, Window, stats};
pub use tickets::{Phase, Ticket, tickets};
