use super::*;
use swarmception_domain::theme::Look;

fn tabs(panes: &[Pane], active: u32) -> DockNode {
    DockNode::Tabs { panes: panes.to_vec(), active }
}

fn split(across: bool, parts: Vec<(u16, DockNode)>) -> DockNode {
    DockNode::Split { across, parts: parts.into_iter().map(|(share, node)| DockPart { share, node }).collect() }
}

/// The default layout is G18's: the timeline over the Factory (with the Workers' tab), the Details
/// at the right; it passes its own check, and builds back to itself.
#[test]
fn the_default_layout_builds_back_to_itself() {
    let layout = DockNode::default_layout();
    assert_eq!(layout.check(), Ok(()));
    assert_eq!(layout_of(&build(&layout)), Some(layout));
}

/// A layout reads back as it was given: tabs with the one shown, splits with their shares.
#[test]
fn a_layout_round_trips_through_the_tiles() {
    let layout = split(
        false,
        vec![(300, tabs(&[Pane::Details, Pane::Factory], 1)), (700, split(true, vec![(500, tabs(&[Pane::Timeline], 0)), (500, tabs(&[Pane::Workers], 0))]))],
    );
    assert_eq!(layout_of(&build(&layout)), Some(layout.clone()));
    let text = serde_json::to_string(&layout).unwrap();
    assert!(text.contains(r#""kind":"split""#) && text.contains(r#""panes":["details","factory"]"#), "{text}");
    assert_eq!(serde_json::from_str::<DockNode>(&text).unwrap(), layout);
}

/// Whatever layout it is given, the dock shows each pane once: one left out is added beside the
/// rest, one named twice is shown once.
#[test]
fn each_pane_is_shown_once() {
    let built = layout_of(&build(&tabs(&[Pane::Timeline, Pane::Timeline], 0))).unwrap();
    let mut seen = Vec::new();
    collect(&built, &mut seen);
    seen.sort();
    assert_eq!(seen, Pane::ALL.to_vec(), "{built:?}");
}

fn collect(node: &DockNode, into: &mut Vec<Pane>) {
    match node {
        DockNode::Tabs { panes, .. } => into.extend(panes),
        DockNode::Split { parts, .. } => parts.iter().for_each(|p| collect(&p.node, into)),
    }
}

/// The check refuses a layout too deep or too large, a split with no parts or a share out of
/// range, and empty tabs; a pane the dock doesn't have doesn't read.
#[test]
fn the_check_refuses_what_wont_build() {
    let mut deep = tabs(&[Pane::Timeline], 0);
    for _ in 0..DEPTH {
        deep = split(true, vec![(1, deep)]);
    }
    assert_eq!(deep.check(), Err(Refused::TooLarge), "deeper than {DEPTH}");
    let wide = split(true, (0..ITEMS).map(|_| (1, tabs(&[Pane::Timeline], 0))).collect());
    assert_eq!(wide.check(), Err(Refused::TooLarge), "over {ITEMS} items");
    assert_eq!(split(true, vec![]).check(), Err(Refused::BadSplit));
    assert_eq!(split(true, vec![(0, tabs(&[Pane::Timeline], 0))]).check(), Err(Refused::BadSplit));
    assert_eq!(split(true, vec![(1001, tabs(&[Pane::Timeline], 0))]).check(), Err(Refused::BadSplit));
    assert_eq!(tabs(&[], 0).check(), Err(Refused::EmptyTabs));
    assert!(serde_json::from_str::<DockNode>(r#"{"kind": "tabs", "panes": ["gates"], "active": 0}"#).is_err(), "no Gates pane (#128)");
    assert!(serde_json::from_str::<DockNode>(r#"{"kind": "tabs", "panes": ["timeline"], "active": 0, "x": 1}"#).is_err());
    assert!(!Refused::BadSplit.to_string().is_empty());
}

/// The View menu's panes hide their tiles, and an empty container with them; a pane shown again
/// comes to the front of its tabs. The layout doesn't change.
#[test]
fn the_view_menus_panes_hide_and_show_their_tiles() {
    let layout = DockNode::default_layout();
    let mut dock = Dock::of(&layout);
    let tile = |dock: &mut Dock, pane: Pane| dock.tree_mut().tiles.iter().find(|(_, t)| matches!(t, Tile::Pane(p) if *p == pane)).map(|(&id, _)| id).unwrap();
    dock.show_panes(Panes::default());
    let details = tile(&mut dock, Pane::Details);
    let workers = tile(&mut dock, Pane::Workers);
    assert!(dock.tree_mut().tiles.is_visible(details) && !dock.tree_mut().tiles.is_visible(workers), "the Workers off by default");
    let parent = dock.tree_mut().tiles.parent_of(details).unwrap();
    dock.show_panes(Panes { details: false, ..Panes::default() });
    assert!(!dock.tree_mut().tiles.is_visible(details) && !dock.tree_mut().tiles.is_visible(parent), "its tabs hidden with it");
    dock.show_panes(Panes { workers: true, ..Panes::default() });
    let tabs_of_workers = dock.tree_mut().tiles.parent_of(workers).unwrap();
    let active = match dock.tree_mut().tiles.get(tabs_of_workers) {
        Some(Tile::Container(Container::Tabs(t))) => t.active,
        _ => None,
    };
    assert_eq!(active, Some(workers), "shown again: in front of the Factory");
    assert_eq!(dock.changed().map(|l| matches!(l, DockNode::Split { .. })), Some(true), "the shown tab is the layout's");
    assert_eq!(dock.changed(), None, "read once");
}

/// A drag (a share changed in the tiles) reads back as the layout, once; a share is per mille, at
/// least 1.
#[test]
fn a_drag_reads_back_once() {
    let layout = split(true, vec![(500, tabs(&[Pane::Timeline, Pane::Factory, Pane::Workers], 0)), (500, tabs(&[Pane::Details], 0))]);
    let mut dock = Dock::of(&layout);
    assert_eq!(dock.changed(), None);
    let root = dock.tree_mut().root().unwrap();
    let details = dock.tree_mut().tiles.iter().find(|(_, t)| matches!(t, Tile::Pane(Pane::Details))).map(|(&id, _)| id).unwrap();
    let details_tabs = dock.tree_mut().tiles.parent_of(details).unwrap();
    if let Some(Tile::Container(Container::Linear(linear))) = dock.tree_mut().tiles.get_mut(root) {
        linear.shares.set_share(details_tabs, 1500.0);
    }
    let read = dock.changed().unwrap();
    let DockNode::Split { parts, .. } = &read else { panic!("{read:?}") };
    assert_eq!(parts.iter().map(|p| p.share).collect::<Vec<_>>(), [250, 750]);
    assert!(dock.built_from(&read) && dock.changed().is_none());
    assert_eq!((per_mille(0.0, 10.0), per_mille(f32::NAN, 1.0), per_mille(1.0, 0.0)), (1, 1, 1));
}

/// The look: the shown tab joins its pane, the others sit on the bar; a separator lights when
/// hovered or dragged; the tokens are the theme's in each look.
#[test]
fn the_dock_is_drawn_in_the_tokens() {
    for (visuals, look) in [(Visuals::dark(), Look::Dark), (Visuals::light(), Look::Light)] {
        let c = Colours::of(&visuals);
        let token = |t: theme::Token| {
            let rgb = t.of(look);
            Color32::from_rgb(rgb.r, rgb.g, rgb.b)
        };
        assert_eq!((c.tab_fill(true), c.tab_fill(false)), (token(theme::BACKGROUND), token(theme::HEADER)));
        assert_eq!((c.tab_text(true), c.tab_text(false)), (token(theme::TEXT), token(theme::MUTED)));
        assert_eq!(c.separator(ResizeState::Idle), token(theme::CHROME));
        assert_eq!((c.separator(ResizeState::Hovering), c.separator(ResizeState::Dragging)), (token(theme::ACCENT), token(theme::ACCENT)));
        assert_eq!(look::tab_outline(&c, &TabState { active: true, ..TabState::default() }).color, token(theme::FRAME));
    }
    assert_eq!((look::tab_bar_height(), look::gap()), (22.0, 4.0));
    assert!(look::simplification().all_panes_must_have_tabs);
}
