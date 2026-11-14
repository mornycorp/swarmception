//! The dock (#177, swarm-core's App UI Substrate › The dock): the window's panes (the timeline, the
//! Factory, the Details, the Workers) as `egui_tiles` tiles under the toolbar, tabs and splits a
//! person drags and resizes, drawn in the theme's tokens.
//!
//! **The UI state is the layout's source of truth** ([`DockNode`], [`crate::UiState`]'s `dock`,
//! which `ui.set` sets and `ui.inspect` reads). Each frame the shell builds the tiles from it when it
//! changed since they were built (a `ui.set` from an agent, a restored launch), draws them, and reads
//! the arrangement back: what a person dragged or resized goes to the state, as an agent's change
//! would. egui_tiles' own saved state is never loaded, and a layout builds whatever it holds: a pane
//! it leaves out is added beside it, one it names twice is shown once. The View menu's panes
//! ([`crate::Panes`]) show and hide their tiles; a pane shown again comes to the front of its tabs.

use egui::{Color32, Stroke, Visuals};
use egui_tiles::{Container, LinearDir, ResizeState, SimplificationOptions, TabState, Tile, TileId, Tiles, Tree};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::theme;

use crate::Panes;
use crate::density;
use crate::pane::Pane;
use crate::part::Part;

/// Between panes, and around the dock, in points.
pub const GAP: f32 = 4.0;
/// [`GAP`] as a margin's whole points.
pub const GAP_MARGIN: i8 = 4;
/// A tab bar's height: a row and a little.
pub const TAB_BAR_HEIGHT: f32 = density::ROW_HEIGHT + 4.0;
/// Inside a pane, around what it draws.
pub const PANE_MARGIN: i8 = 6;
/// How deep a layout may nest: a split in a split, to this depth.
pub const DEPTH: usize = 6;
/// The most tabs, splits and panes a layout may hold, all told.
pub const ITEMS: usize = 32;
/// The largest share a split's part may have; a part's space is its share of its split's total.
pub const SHARE: u16 = 1000;
/// A part's share as the layout keeps it: per mille of its split.
const PER_MILLE: f32 = 1000.0;

/// Where the dock's panes are: tabs of panes, or a split into parts side by side or one above
/// another. Every layout [`DockNode::check`] accepts builds a dock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DockNode {
    /// Panes as tabs, and the index of the one shown.
    Tabs { panes: Vec<Pane>, active: u32 },
    /// Parts side by side (`across`) or one above another, in order.
    Split { across: bool, parts: Vec<DockPart> },
}

/// One part of a split: its share of the split's space, and what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DockPart {
    /// Its share: its space is this over the split's total (1 to [`SHARE`]).
    pub share: u16,
    pub node: DockNode,
}

/// Why a layout is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Deeper than [`DEPTH`], or more than [`ITEMS`] tabs, splits and panes.
    TooLarge,
    /// A split with no parts, or a part's share 0 or over [`SHARE`].
    BadSplit,
    /// Tabs with no panes.
    EmptyTabs,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TooLarge => crate::catalog::THE_DOCK_S_LAYOUT_IS_TOO_LARGE,
            Self::BadSplit => crate::catalog::A_SPLIT_IN_THE_DOCK_S_LAYOUT,
            Self::EmptyTabs => crate::catalog::TABS_IN_THE_DOCK_S_LAYOUT_HAVE,
        })
    }
}

impl DockNode {
    /// Tabs of `pane` alone.
    #[must_use]
    pub fn tab(pane: Pane) -> Self {
        Self::Tabs { panes: vec![pane], active: 0 }
    }

    /// The layout the window starts with (G18's: the timeline biggest, the Factory under it, the
    /// Details at the right): the timeline over the Factory and the Workers' tab, three quarters
    /// of the width, the Details beside them.
    #[must_use]
    pub fn default_layout() -> Self {
        let part = |share, node| DockPart { share, node };
        let left = Self::Split {
            across: false,
            parts: vec![part(640, Self::tab(Pane::Timeline)), part(360, Self::Tabs { panes: vec![Pane::Factory, Pane::Workers], active: 0 })],
        };
        Self::Split { across: true, parts: vec![part(760, left), part(240, Self::tab(Pane::Details))] }
    }

    /// Why this layout is refused, if it is: deeper than [`DEPTH`], more than [`ITEMS`] items, a
    /// split with no parts or a share out of range, or tabs with no panes. (A pane the dock doesn't
    /// have doesn't read as a [`Pane`].)
    ///
    /// # Errors
    ///
    /// [`Refused`], saying which.
    pub fn check(&self) -> Result<(), Refused> {
        let mut items = 0;
        self.check_at(1, &mut items)
    }

    fn check_at(&self, depth: usize, items: &mut usize) -> Result<(), Refused> {
        *items = items.saturating_add(1);
        if depth > DEPTH || *items > ITEMS {
            return Err(Refused::TooLarge);
        }
        match self {
            Self::Tabs { panes, .. } => {
                if panes.is_empty() {
                    return Err(Refused::EmptyTabs);
                }
                *items = items.saturating_add(panes.len());
                if *items > ITEMS { Err(Refused::TooLarge) } else { Ok(()) }
            }
            Self::Split { parts, .. } => {
                if parts.is_empty() || parts.iter().any(|p| p.share == 0 || p.share > SHARE) {
                    return Err(Refused::BadSplit);
                }
                parts.iter().try_for_each(|part| part.node.check_at(depth + 1, items))
            }
        }
    }
}

/// The tiles the dock draws, the layout they were built from, the layout as last read back, the
/// panes shown when they were last drawn, and the pane a person last clicked in (its tab is
/// underlined).
#[derive(Debug)]
pub struct Dock {
    built_from: DockNode,
    last: Option<DockNode>,
    tree: Tree<Pane>,
    shown: Option<Panes>,
    focused: Option<Pane>,
}

impl Dock {
    /// The tiles of `layout`.
    #[must_use]
    pub fn of(layout: &DockNode) -> Self {
        let tree = build(layout);
        Self { built_from: layout.clone(), last: layout_of(&tree), tree, shown: None, focused: None }
    }

    /// The layout the tiles show now.
    #[must_use]
    pub fn layout(&self) -> Option<DockNode> {
        layout_of(&self.tree)
    }

    /// Whether these tiles were built from `layout` (or read back as it since).
    #[must_use]
    pub fn built_from(&self, layout: &DockNode) -> bool {
        self.built_from == *layout
    }

    /// The tiles, to draw or to change as a drag would (a test's).
    pub const fn tree_mut(&mut self) -> &mut Tree<Pane> {
        &mut self.tree
    }

    /// The pane last clicked in.
    #[must_use]
    pub const fn focused(&self) -> Option<Pane> {
        self.focused
    }

    /// The pane last clicked in, as the shell saw it.
    pub const fn set_focused(&mut self, pane: Option<Pane>) {
        self.focused = pane;
    }

    /// Shows the panes `panes` shows and hides the rest (a tabs container left with nothing to
    /// show hidden too); a pane shown since the last frame comes to the front of its tabs.
    pub fn show_panes(&mut self, panes: Panes) {
        let before = self.shown.replace(panes);
        let tiles: Vec<(TileId, Pane)> =
            self.tree.tiles.iter().filter_map(|(&id, tile)| if let Tile::Pane(p) = tile { Some((id, *p)) } else { None }).collect();
        for &(tile, pane) in &tiles {
            let on = shows(panes, pane);
            self.tree.tiles.set_visible(tile, on);
            let newly = on && before.is_some_and(|b| !shows(b, pane));
            if let Some(parent) = self.tree.tiles.parent_of(tile)
                && let Some(Tile::Container(Container::Tabs(tabs))) = self.tree.tiles.get_mut(parent)
                && newly
            {
                tabs.set_active(tile);
            }
        }
        // A container with nothing visible in it is hidden, bottom up, so its split gives the room
        // to its neighbours.
        let containers: Vec<TileId> = self.tree.tiles.iter().filter(|(_, t)| t.is_container()).map(|(&id, _)| id).collect();
        for _ in 0..DEPTH {
            for &id in &containers {
                let any = match self.tree.tiles.get(id) {
                    Some(Tile::Container(c)) => c.children().any(|&child| self.tree.tiles.is_visible(child)),
                    _ => true,
                };
                self.tree.tiles.set_visible(id, any);
            }
        }
        // The shown tab of each tabs container is one that shows.
        for &id in &containers {
            let visible: Vec<TileId> = match self.tree.tiles.get(id) {
                Some(Tile::Container(Container::Tabs(tabs))) => tabs.children.iter().copied().filter(|&c| self.tree.tiles.is_visible(c)).collect(),
                _ => continue,
            };
            if let Some(Tile::Container(Container::Tabs(tabs))) = self.tree.tiles.get_mut(id)
                && !tabs.active.is_some_and(|a| visible.contains(&a))
                && let Some(&first) = visible.first()
            {
                tabs.set_active(first);
            }
        }
    }

    /// The layout read back after a frame, when it changed since the last: what a person dragged.
    pub fn changed(&mut self) -> Option<DockNode> {
        let now = layout_of(&self.tree);
        if now == self.last {
            return None;
        }
        self.last.clone_from(&now);
        let layout = now?;
        self.built_from = layout.clone();
        Some(layout)
    }
}

/// Whether `panes` shows `pane`.
#[must_use]
pub const fn shows(panes: Panes, pane: Pane) -> bool {
    match pane {
        Pane::Timeline => panes.timeline,
        Pane::Factory => panes.factory,
        Pane::Details => panes.details,
        Pane::Workers => panes.workers,
    }
}

/// The tiles of `layout`: each pane it names once, in its tabs and splits, and each it leaves out
/// beside them.
#[must_use]
pub fn build(layout: &DockNode) -> Tree<Pane> {
    let mut tiles = Tiles::default();
    let mut placed = Vec::new();
    let root = insert(&mut tiles, layout, &mut placed);
    let missing: Vec<TileId> = Pane::ALL
        .into_iter()
        .filter(|pane| !placed.contains(pane))
        .map(|pane| {
            let tile = tiles.insert_pane(pane);
            tiles.insert_tab_tile(vec![tile])
        })
        .collect();
    let root = match root {
        Some(root) if missing.is_empty() => root,
        root => tiles.insert_horizontal_tile(root.into_iter().chain(missing).collect()),
    };
    Tree::new(Part::Dock.id(), root, tiles)
}

fn insert(tiles: &mut Tiles<Pane>, node: &DockNode, placed: &mut Vec<Pane>) -> Option<TileId> {
    match node {
        DockNode::Tabs { panes, active } => insert_tabs(tiles, panes, *active, placed),
        DockNode::Split { across, parts } => insert_split(tiles, *across, parts, placed),
    }
}

fn insert_tabs(tiles: &mut Tiles<Pane>, panes: &[Pane], active: u32, placed: &mut Vec<Pane>) -> Option<TileId> {
    let mut children = Vec::new();
    for &pane in panes {
        if !placed.contains(&pane) {
            placed.push(pane);
            children.push(tiles.insert_pane(pane));
        }
    }
    let shown = usize::try_from(active).ok().and_then(|at| children.get(at)).copied();
    if children.is_empty() {
        return None;
    }
    let tabs = tiles.insert_tab_tile(children);
    if let (Some(child), Some(Tile::Container(Container::Tabs(t)))) = (shown, tiles.get_mut(tabs)) {
        t.set_active(child);
    }
    Some(tabs)
}

fn insert_split(tiles: &mut Tiles<Pane>, across: bool, parts: &[DockPart], placed: &mut Vec<Pane>) -> Option<TileId> {
    let parts: Vec<(TileId, u16)> = parts.iter().filter_map(|part| Some((insert(tiles, &part.node, placed)?, part.share))).collect();
    if let [(only, _)] = parts.as_slice() {
        return Some(*only);
    }
    let children: Vec<TileId> = parts.iter().map(|&(tile, _)| tile).collect();
    if children.is_empty() {
        return None;
    }
    let split = if across { tiles.insert_horizontal_tile(children) } else { tiles.insert_vertical_tile(children) };
    if let Some(Tile::Container(Container::Linear(linear))) = tiles.get_mut(split) {
        for &(tile, share) in &parts {
            linear.shares.set_share(tile, f32::from(share));
        }
    }
    Some(split)
}

/// The layout `tree` shows: its tabs and splits, each part's share per mille of its split.
#[must_use]
pub fn layout_of(tree: &Tree<Pane>) -> Option<DockNode> {
    tree.root().and_then(|root| node_of(&tree.tiles, root))
}

fn node_of(tiles: &Tiles<Pane>, tile: TileId) -> Option<DockNode> {
    match tiles.get(tile)? {
        Tile::Pane(pane) => Some(DockNode::tab(*pane)),
        Tile::Container(Container::Tabs(tabs)) => tabs_of(tiles, &tabs.children, tabs.active),
        Tile::Container(Container::Linear(linear)) => {
            let shares: Vec<(TileId, f32)> = linear.children.iter().map(|&child| (child, linear.shares[child])).collect();
            split_of(tiles, linear.dir == LinearDir::Horizontal, &shares)
        }
        Tile::Container(grid) => {
            let shares: Vec<(TileId, f32)> = grid.children().map(|&child| (child, 1.0)).collect();
            split_of(tiles, true, &shares)
        }
    }
}

fn tabs_of(tiles: &Tiles<Pane>, children: &[TileId], active: Option<TileId>) -> Option<DockNode> {
    let panes: Vec<(TileId, Pane)> = children
        .iter()
        .filter_map(|&child| match tiles.get(child) {
            Some(Tile::Pane(pane)) => Some((child, *pane)),
            _ => None,
        })
        .collect();
    let shown = panes.iter().position(|&(child, _)| Some(child) == active).unwrap_or(0);
    let active = u32::try_from(shown).unwrap_or(0);
    (!panes.is_empty()).then(|| DockNode::Tabs { panes: panes.into_iter().map(|(_, pane)| pane).collect(), active })
}

fn split_of(tiles: &Tiles<Pane>, across: bool, shares: &[(TileId, f32)]) -> Option<DockNode> {
    let total: f32 = shares.iter().map(|&(_, share)| share.max(0.0)).sum();
    let parts: Vec<DockPart> =
        shares.iter().filter_map(|&(child, share)| Some(DockPart { share: per_mille(share, total), node: node_of(tiles, child)? })).collect();
    match parts.len() {
        0 => None,
        1 => parts.into_iter().next().map(|part| part.node),
        _ => Some(DockNode::Split { across, parts }),
    }
}

/// `share` of `total`, per mille, at least 1: what a part's share is kept as.
fn per_mille(share: f32, total: f32) -> u16 {
    let fraction = if total > 0.0 && share.is_finite() { share / total } else { 0.0 };
    let mille = (fraction * PER_MILLE).round().clamp(1.0, PER_MILLE);
    // In 1..=1000 once clamped: the cast neither wraps nor truncates a whole part.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to 1..=1000 just above")]
    let mille = mille as u16;
    mille
}

/// The dock's colours in one look, from the theme's tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colours {
    pub chrome: Color32,
    pub background: Color32,
    pub header: Color32,
    pub frame: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
}

impl Colours {
    /// The tokens in egui's look.
    #[must_use]
    pub fn of(visuals: &Visuals) -> Self {
        let token = |t: theme::Token| crate::theme::token(visuals, t);
        Self {
            chrome: token(theme::CHROME),
            background: token(theme::BACKGROUND),
            header: token(theme::HEADER),
            frame: token(theme::FRAME),
            text: token(theme::TEXT),
            muted: token(theme::MUTED),
            accent: token(theme::ACCENT),
        }
    }

    /// A tab's fill: the shown tab joins its pane (`background`), the others sit on the bar (`header`).
    #[must_use]
    pub const fn tab_fill(&self, active: bool) -> Color32 {
        if active { self.background } else { self.header }
    }

    /// A tab's title: `text` on the shown tab, `muted` on the others.
    #[must_use]
    pub const fn tab_text(&self, active: bool) -> Color32 {
        if active { self.text } else { self.muted }
    }

    /// A separator between panes: `chrome` at rest, `accent` when hovered or dragged.
    #[must_use]
    pub const fn separator(&self, state: ResizeState) -> Color32 {
        match state {
            ResizeState::Idle => self.chrome,
            ResizeState::Hovering | ResizeState::Dragging => self.accent,
        }
    }
}

/// The parts of egui_tiles' `Behavior` that are the dock's look, the same for every pane: an
/// implementation draws the panes and takes these.
pub mod look {
    use super::{Colours, GAP, ResizeState, SimplificationOptions, Stroke, TAB_BAR_HEIGHT, TabState};

    /// The simplification the dock asks: every pane in tabs, so each has a tab to drag by.
    #[must_use]
    pub fn simplification() -> SimplificationOptions {
        SimplificationOptions { all_panes_must_have_tabs: true, ..SimplificationOptions::default() }
    }

    pub const fn tab_bar_height() -> f32 {
        TAB_BAR_HEIGHT
    }

    pub const fn gap() -> f32 {
        GAP
    }

    pub const fn tab_outline(colours: &Colours, state: &TabState) -> Stroke {
        if state.active { Stroke { width: 1.0, color: colours.frame } } else { Stroke::NONE }
    }

    pub const fn resize(colours: &Colours, state: ResizeState) -> Stroke {
        Stroke { width: 1.0, color: colours.separator(state) }
    }
}

#[cfg(test)]
mod tests;
