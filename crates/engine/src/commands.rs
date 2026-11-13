//! The registry: every command, one line each.

use std::sync::LazyLock;

use crate::command::Entry;
use crate::{details_cmds, engine_cmds, factory_cmds, gates_cmds, issues_cmds, live_cmds, repo_cmds, rows_cmds, stats_cmds, theme_cmds, workers_cmds};

static REGISTRY: LazyLock<Vec<Entry>> = LazyLock::new(|| {
    vec![
        Entry::of::<engine_cmds::Commands>(),
        Entry::of::<repo_cmds::Open>(),
        Entry::of::<repo_cmds::Reload>(),
        Entry::of::<repo_cmds::Fetch>(),
        Entry::of::<repo_cmds::Status>(),
        Entry::of::<repo_cmds::Recent>(),
        Entry::of::<repo_cmds::ClearRecent>(),
        Entry::of::<rows_cmds::Get>(),
        Entry::of::<rows_cmds::Set>(),
        Entry::of::<rows_cmds::List>(),
        Entry::of::<rows_cmds::WindowRows>(),
        Entry::of::<rows_cmds::TimeAt>(),
        Entry::of::<rows_cmds::GetBar>(),
        Entry::of::<stats_cmds::Get>(),
        Entry::of::<factory_cmds::PlaceAt>(),
        Entry::of::<factory_cmds::At>(),
        Entry::of::<factory_cmds::TicketBars>(),
        Entry::of::<factory_cmds::Moments>(),
        Entry::of::<factory_cmds::StepTo>(),
        Entry::of::<issues_cmds::Urls>(),
        Entry::of::<details_cmds::Get>(),
        Entry::of::<gates_cmds::List>(),
        Entry::of::<workers_cmds::Compare>(),
        Entry::of::<live_cmds::Get>(),
        Entry::of::<live_cmds::Set>(),
        Entry::of::<live_cmds::Check>(),
        Entry::of::<live_cmds::CarryBars>(),
        Entry::of::<theme_cmds::Get>(),
    ]
});

/// Every command, in the registry's order.
#[must_use]
pub fn all() -> &'static [Entry] {
    &REGISTRY
}

/// The command `id`.
#[must_use]
pub fn find(id: &str) -> Option<&'static Entry> {
    REGISTRY.iter().find(|e| e.id == id)
}
