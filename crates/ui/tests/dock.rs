//! The dock (#177, swarm-core's App UI Substrate › The dock): the window's panes as tiles the UI
//! state's layout places. `ui.set` sets the layout and `ui.inspect` reads it back with each pane
//! drawn and its size (AGENTS.md rule 6: what is drawn is what the state says); a bad layout is
//! refused and changes nothing; each pane is shown once whatever the layout; the View menu hides
//! a tile; the layout is saved and comes back.

use swarmception_ui::control::{Answer, Inspect};
use swarmception_ui::pane::Pane;
use swarmception_ui::{DockNode, MenuAction, Shell};

fn frame(shell: &mut Shell, ctx: &egui::Context) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1500.0, 900.0));
    let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| shell.ui(ui));
    output.textures_delta.clear();
}

fn frames(shell: &mut Shell, ctx: &egui::Context) -> Inspect {
    for _ in 0..3 {
        frame(shell, ctx);
    }
    shell.inspect()
}

fn set(shell: &mut Shell, params: &str) -> Result<(), String> {
    match shell.answer("ui.set", params) {
        Answer::Now(Ok(_)) => Ok(()),
        Answer::Now(Err(e)) => Err(format!("{e:?}")),
        _ => Err("not now".into()),
    }
}

fn size(inspect: &Inspect, pane: Pane) -> Option<(u32, u32)> {
    inspect.panes.iter().find(|p| p.pane == pane).map(|p| (p.width, p.height))
}

#[test]
fn the_default_layout_is_g18s() {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, None);
    let inspect = frames(&mut shell, &ctx);
    assert_eq!(inspect.dock, DockNode::default_layout());
    let drawn: Vec<Pane> = inspect.panes.iter().map(|p| p.pane).collect();
    assert_eq!(drawn.len(), 3, "the Workers hidden by default: {drawn:?}");
    let (timeline, factory, details) =
        (size(&inspect, Pane::Timeline).unwrap(), size(&inspect, Pane::Factory).unwrap(), size(&inspect, Pane::Details).unwrap());
    assert!(timeline.1 > factory.1, "the timeline biggest: {timeline:?} over {factory:?}");
    assert!(timeline.0 > details.0 * 2 && details.1 > timeline.1, "the Details a column at the right: {details:?}");
}

#[test]
fn ui_set_places_the_panes_and_inspect_reads_them_back() {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, None);
    frames(&mut shell, &ctx);
    // The Details over the timeline and the Factory side by side, a quarter of the height.
    let layout = r#"{"dock": {"kind": "split", "across": false, "parts": [
        {"share": 250, "node": {"kind": "tabs", "panes": ["details"], "active": 0}},
        {"share": 750, "node": {"kind": "split", "across": true, "parts": [
            {"share": 500, "node": {"kind": "tabs", "panes": ["timeline"], "active": 0}},
            {"share": 500, "node": {"kind": "tabs", "panes": ["factory", "workers"], "active": 0}}]}}]}}"#;
    set(&mut shell, layout).unwrap();
    let inspect = frames(&mut shell, &ctx);
    let want: serde_json::Value = serde_json::from_str(layout).unwrap();
    assert_eq!(serde_json::to_value(&inspect.dock).unwrap(), want["dock"], "read back as set");
    assert_eq!(Some(&inspect.dock), shell.ui_state().dock.as_ref());
    let (details, timeline, factory) =
        (size(&inspect, Pane::Details).unwrap(), size(&inspect, Pane::Timeline).unwrap(), size(&inspect, Pane::Factory).unwrap());
    assert!(details.0 > timeline.0 && timeline.1 > details.1 * 2, "the Details across the top: {details:?} {timeline:?}");
    assert!(timeline.0.abs_diff(factory.0) <= 2 && timeline.1 == factory.1, "side by side, halves: {timeline:?} {factory:?}");
    // Back to the default.
    set(&mut shell, r#"{"defaultDock": true}"#).unwrap();
    assert_eq!(frames(&mut shell, &ctx).dock, DockNode::default_layout());
}

#[test]
fn a_bad_layout_is_refused_and_changes_nothing() {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, None);
    frames(&mut shell, &ctx);
    let before = shell.ui_state();
    let deep = (0..7).fold(r#"{"kind": "tabs", "panes": ["timeline"], "active": 0}"#.to_owned(), |inner, _| {
        format!(r#"{{"kind": "split", "across": true, "parts": [{{"share": 1, "node": {inner}}}]}}"#)
    });
    for bad in [
        r#"{"kind": "tabs", "panes": [], "active": 0}"#.to_owned(),
        r#"{"kind": "split", "across": true, "parts": []}"#.to_owned(),
        r#"{"kind": "split", "across": true, "parts": [{"share": 0, "node": {"kind": "tabs", "panes": ["timeline"], "active": 0}}]}"#.to_owned(),
        r#"{"kind": "split", "across": true, "parts": [{"share": 1001, "node": {"kind": "tabs", "panes": ["timeline"], "active": 0}}]}"#.to_owned(),
        r#"{"kind": "tabs", "panes": ["gates"], "active": 0}"#.to_owned(),
        deep,
    ] {
        let refused = set(&mut shell, &format!(r#"{{"dock": {bad}, "follow": false}}"#));
        assert!(refused.is_err(), "{bad}");
    }
    assert_eq!(shell.ui_state(), before, "nothing changed");
}

#[test]
fn each_pane_is_shown_once_and_the_view_menu_hides_its_tile() {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, None);
    // The timeline twice, the rest left out: each pane once.
    set(&mut shell, r#"{"dock": {"kind": "tabs", "panes": ["timeline", "timeline"], "active": 0}, "panes": {"workers": true}}"#).unwrap();
    let inspect = frames(&mut shell, &ctx);
    let mut named = Vec::new();
    collect(&inspect.dock, &mut named);
    named.sort();
    assert_eq!(named, Pane::ALL.to_vec(), "{:?}", inspect.dock);
    // View ▸ Details hides its tile, and gives its room to the rest.
    let shown = inspect.panes.len();
    shell.run_menu(&ctx, MenuAction::Details);
    let hidden = frames(&mut shell, &ctx);
    assert_eq!(hidden.panes.len(), shown - 1);
    assert!(hidden.panes.iter().all(|p| p.pane != Pane::Details));
    assert_eq!(hidden.dock, inspect.dock, "hiding a pane leaves the layout as it is");
    // Nothing shown: the window says how to show one.
    set(&mut shell, r#"{"panes": {"timeline": false, "factory": false, "details": false, "workers": false}}"#).unwrap();
    assert!(frames(&mut shell, &ctx).panes.is_empty());
}

fn collect(node: &DockNode, into: &mut Vec<Pane>) {
    match node {
        DockNode::Tabs { panes, .. } => into.extend(panes),
        DockNode::Split { parts, .. } => parts.iter().for_each(|p| collect(&p.node, into)),
    }
}
