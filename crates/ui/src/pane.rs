//! The window's panes, by id (#176): what the View menu shows and hides, what each pane's icon
//! is ([`crate::icons::for_pane`]), and, with the dock (#177), what its tiles hold.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A pane of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Pane {
    /// The swarm's tickets on swarm-timeline's widget.
    Timeline,
    /// The Factory: the stations and what is at each at the playhead.
    Factory,
    /// The details of what is selected.
    Details,
    /// Model and effort compared per worker.
    Workers,
}

impl Pane {
    /// Every pane, in the View menu's order.
    pub const ALL: [Self; 4] = [Self::Timeline, Self::Factory, Self::Details, Self::Workers];

    /// Its name: its View menu item's, its tab's title.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Timeline => crate::catalog::TIMELINE,
            Self::Factory => crate::catalog::FACTORY,
            Self::Details => crate::catalog::DETAILS_2,
            Self::Workers => crate::catalog::WORKERS,
        }
    }
}
