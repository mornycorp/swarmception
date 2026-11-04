//! The timeline pane over a real repository's tracking branch: what it loads into the widget, and
//! what it keeps when the engine's rows move on.

use std::path::PathBuf;

use swarmception_engine::Session;
use swarmception_ui::TimelinePane;
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::rulers_cmds::{Add, AddParams, Set, SetParams};
use timeline_engine::state_cmds::Get;
use timeline_engine::view_cmds::{Show, ShowParams};
use timeline_engine::wire::{OriginWire, RulerDomainWire};

mod support;

/// A repository in a temporary directory whose tracking branch holds one ticket.
fn repo() -> PathBuf {
    repo_with("2026-10-31T03:30:00Z")
}

/// As [`repo`], #7 submitted at `submit`: claimed at 03:01, working until then.
fn repo_with(submit: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "swarmception-ui-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        // Two tests can start within one tick of the clock: a counter keeps their directories apart.
        {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        }
    ));
    std::fs::create_dir_all(dir.join("tracking/mini-aa1111")).unwrap();
    std::fs::write(
        dir.join("tracking/mini-aa1111/work.jsonl"),
        concat!(
            r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
            "\n",
            r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
            "\n",
        )
        .to_owned()
            + &format!(r#"{{"event":"submit","issue":7,"mr":8,"t":"{submit}"}}"#)
            + "\n",
    )
    .unwrap();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    run(&["init", "-q", "-b", "tracking"]);
    run(&["add", "."]);
    run(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "events"]);
    dir
}

fn rulers(pane: &mut TimelinePane) -> Vec<RulerDomainWire> {
    let state = pane.widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
    state.rulers.iter().map(|r| r.domain).collect()
}

#[test]
fn the_first_load_reads_the_clock_and_a_reload_keeps_the_rulers() {
    let dir = repo();
    let mut engine = Session::new();
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    engine.execute("repo.open", &format!("{{\"path\":{path}}}")).unwrap();
    let mut pane = TimelinePane::default();
    pane.sync(&mut engine).unwrap();
    assert_eq!(rulers(&mut pane), [RulerDomainWire::Clock], "the scrub ruler reads the time of day");
    let text = pane.clock_text(&mut engine).unwrap();
    assert!(text.starts_with("2026-10-31 03:") && text.contains(" UTC"), "{text}");

    // The person picks the Time ruler and adds a Clock ruler under it; new rows keep both.
    let session = pane.widget().session();
    session.call::<Set>(SetParams { ruler: 0, domain: Some(RulerDomainWire::Axis), origin: None }).unwrap();
    session.call::<Add>(AddParams { domain: RulerDomainWire::Clock, origin: Some(OriginWire::Absolute) }).unwrap();
    engine.execute("rows.set", r#"{"grouping":"workers"}"#).unwrap();
    pane.sync(&mut engine).unwrap();
    assert_eq!(rulers(&mut pane), [RulerDomainWire::Axis, RulerDomainWire::Clock]);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_toolbar_shows_only_what_swarmception_needs_and_the_rest_is_fixed_by_command() {
    use timeline_engine::menu_cmds::Toolbar;
    use timeline_engine::minimap_cmds::VisibilityWire;
    use timeline_engine::state_cmds::{Set as SetState, SetParams as SetStateParams};
    use timeline_engine::toolbar_cmds::{GetToolbar, ToolbarControl as C};
    use timeline_engine::wire::ClickModeWire;

    // The spec, before any data and after: the navigate items, the zoom buttons, the measurement.
    let wanted = [C::Fit, C::ZoomToSelection, C::UndoZoom, C::ZoomIn, C::ZoomOut, C::ZoomFit, C::Measurement];
    assert_eq!(swarmception_ui::TIMELINE_TOOLBAR, wanted);
    let mut pane = TimelinePane::default();
    assert_eq!(pane.widget().session().call::<GetToolbar>(NoParams {}).unwrap().show, wanted);

    let dir = repo();
    let mut engine = Session::new();
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    engine.execute("repo.open", &format!("{{\"path\":{path}}}")).unwrap();
    pane.sync(&mut engine).unwrap();
    let fixed = |pane: &mut TimelinePane| {
        let state = pane.widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
        (state.scale, state.origin, state.minimap, state.minimap_strips, state.click)
    };
    // #134: the minimap draws the tickets' strips (swarm-timeline#222).
    let expected = ("Time".to_owned(), OriginWire::Absolute, VisibilityWire::Always, true, ClickModeWire::Nearest);
    assert_eq!(fixed(&mut pane), expected, "set by command on load");
    let bar = pane.widget().session().call::<Toolbar>(NoParams {}).unwrap();
    assert_eq!(bar.show, wanted, "the toolbar draws only these");
    assert_eq!(bar.navigate.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Fit", "Zoom to Selection", "Undo Zoom"]);
    assert_eq!(bar.zoom.iter().map(|b| b.control).collect::<Vec<_>>(), [C::ZoomIn, C::ZoomOut, C::ZoomFit], "the zoom buttons are the widget's");
    assert!(bar.zoom.iter().all(|b| b.item.enabled), "{:?}", bar.zoom);

    // Something moves them (the keyboard, a ruler's menu); the next load fixes them again and
    // the spec holds.
    let session = pane.widget().session();
    session.call::<SetState>(SetStateParams { scale: Some("Unit".to_owned()), origin: Some(OriginWire::Viewport), ..SetStateParams::default() }).unwrap();
    engine.execute("rows.set", r#"{"grouping":"workers"}"#).unwrap();
    pane.sync(&mut engine).unwrap();
    assert_eq!(fixed(&mut pane), expected);
    assert_eq!(pane.widget().session().call::<GetToolbar>(NoParams {}).unwrap().show, wanted);
    std::fs::remove_dir_all(dir).ok();
}

/// The pane over a repository whose #7 was submitted at `submit`, and the engine under it.
fn opened(submit: &str) -> (PathBuf, Session, TimelinePane) {
    let dir = repo_with(submit);
    let mut engine = Session::new();
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    engine.execute("repo.open", &format!("{{\"path\":{path}}}")).unwrap();
    let mut pane = TimelinePane::default();
    pane.sync(&mut engine).unwrap();
    (dir, engine, pane)
}

#[test]
fn a_hovered_tickets_highlight_leaves_the_selection_and_comes_back_after_a_reload() {
    use swarmception_engine::factory_cmds::{TicketBars, TicketParams};
    let (dir, mut engine, mut pane) = opened("2026-10-31T03:30:00Z");
    let bars = engine.call::<TicketBars>(TicketParams { issue: 7 }).unwrap().bars;
    let mut want = std::collections::BTreeMap::<String, Vec<usize>>::new();
    for bar in bars {
        want.entry(bar.track).or_default().push(bar.range);
    }
    let before = pane.selection().unwrap();
    assert!(pane.light_ticket(&mut engine, Some(7)), "the widget is told");
    assert_eq!(pane.highlighted(), want);
    assert!(!pane.light_ticket(&mut engine, Some(7)), "and not again while nothing moved");
    assert_eq!(pane.selection().unwrap(), before, "the selection untouched");
    // New rows: the widget's load cleared its highlight; the pane highlights the bars again.
    engine.execute("rows.set", r#"{"grouping":"workers"}"#).unwrap();
    pane.sync(&mut engine).unwrap();
    assert!(pane.highlighted().is_empty(), "a load clears the widget's highlight");
    assert!(pane.light_ticket(&mut engine, Some(7)));
    assert!(!pane.highlighted().is_empty());
    assert!(pane.light_ticket(&mut engine, None));
    assert!(pane.highlighted().is_empty());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_highlight_has_one_owner_a_hover_while_it_lasts_then_the_details() {
    use swarmception_domain::rows::BarRef;
    use swarmception_engine::factory_cmds::{TicketBars, TicketParams};
    use swarmception_ui::Lit;
    let (dir, mut engine, mut pane) = opened("2026-10-31T03:30:00Z");
    let by_row = |bars: &[BarRef]| {
        let mut rows = std::collections::BTreeMap::<String, Vec<usize>>::new();
        for bar in bars {
            rows.entry(bar.track.clone()).or_default().push(bar.range);
        }
        rows
    };
    let seven = engine.call::<TicketBars>(TicketParams { issue: 7 }).unwrap().bars;
    // The details' bars: #7's first, as a details pane on it would hand them.
    let details = vec![seven[0].clone()];
    let before = pane.selection().unwrap();
    assert_eq!(pane.lit(), Lit::Nothing);
    assert!(pane.light(&mut engine, None, &details), "the widget is told");
    assert_eq!((pane.highlighted(), pane.lit()), (by_row(&details), Lit::Details));
    assert!(!pane.light(&mut engine, None, &details), "and not again while nothing moved");
    // A hover wins while it lasts, whatever the details hold.
    assert!(pane.light(&mut engine, Some(7), &details));
    assert_eq!((pane.highlighted(), pane.lit()), (by_row(&seven), Lit::Hovered { ticket: 7 }));
    assert!(!pane.light(&mut engine, Some(7), &[]), "the details moving under a hover tell the widget nothing");
    // Its leave gives the details their highlight back.
    assert!(pane.light(&mut engine, None, &details));
    assert_eq!((pane.highlighted(), pane.lit()), (by_row(&details), Lit::Details));
    assert_eq!(pane.selection().unwrap(), before, "the selection never touched");
    // Bars the rows no longer have: nothing highlighted, never a panic.
    assert!(pane.light(&mut engine, None, &[BarRef { track: "nope".into(), range: 3 }]));
    assert!(pane.highlighted().is_empty());
    assert!(pane.light(&mut engine, None, &[]));
    assert_eq!((pane.highlighted().is_empty(), pane.lit()), (true, Lit::Nothing));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn bar_labels_and_tooltips_read_durations_in_hours_and_minutes() {
    use timeline_engine::query_cmds::{Bars, BarsParams, Tooltip};
    use timeline_engine::state_cmds::{Set as SetState, SetParams as SetStateParams};
    use timeline_engine::tracks_cmds::{HitParams, Layout, LayoutParams};
    // #7 works from 03:01 to 04:59:30: 1 h 58 min 30 s, read as a person reads it.
    let (dir, _engine, mut pane) = opened("2026-10-31T04:59:30Z");
    let session = pane.widget().session();
    session.call::<SetState>(SetStateParams { width: Some(1_600.0), ..SetStateParams::default() }).unwrap();
    let layout = session.call::<Layout>(LayoutParams { pane: 600.0, scroll: 0.0 }).unwrap();
    let bars = session.call::<Bars>(BarsParams { tracks: None, label_char_width: Some(7.0) }).unwrap();
    let labels: Vec<&str> = bars.tracks.iter().flat_map(|t| t.labels.iter().map(|l| l.text.as_str())).collect();
    assert!(labels.iter().any(|l| l.ends_with("(1 h 58 min)")), "{labels:?}");
    assert!(!labels.iter().any(|l| l.contains(",") && l.ends_with(" s)")), "no seconds by the thousand: {labels:?}");
    // Its tooltip, over the bar's middle.
    let (track, bar) = bars.tracks.iter().find_map(|t| t.ranges.iter().max_by(|a, b| a.width.total_cmp(&b.width)).map(|b| (t, b))).unwrap();
    let row = layout.rows.iter().find(|r| r.key == track.key).unwrap();
    let point = HitParams { pane: 600.0, scroll: 0.0, x: layout.label_width + bar.x + bar.width / 2.0, y: row.y + row.height / 2.0 };
    let tip = session.call::<Tooltip>(point).unwrap();
    let lines: Vec<String> = tip.lines.iter().map(|l| l.text()).collect();
    assert!(lines.iter().any(|l| l.contains("1 h 58 min")), "{lines:?}");
    std::fs::remove_dir_all(dir).ok();
}

/// One frame of the shell, 1000 × 700 points, with `events`.
fn frame(shell: &mut swarmception_ui::Shell, ctx: &egui::Context, events: Vec<egui::Event>) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
    let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
}

#[test]
fn ctrl_drag_through_the_shell_pans_the_timeline() {
    // macOS's Ctrl is egui's `ctrl` alone, its Cmd `command`: both pan, and the shell takes neither.
    let ctrl = egui::Modifiers { ctrl: true, ..Default::default() };
    let cmd = egui::Modifiers { command: true, mac_cmd: true, ..Default::default() };
    let dir = repo();
    for modifiers in [ctrl, cmd] {
        let ctx = egui::Context::default();
        let mut shell = swarmception_ui::Shell::new(&ctx, Some(&dir));
        support::until("the tracking branch loaded", || {
            frame(&mut shell, &ctx, Vec::new());
            shell.has_timeline()
        });
        assert!(shell.has_timeline(), "the tracking branch loaded");
        // A height in the timeline's rows, under the minimap's strip (always shown since #35):
        // ticket 7's bar's, while the whole swarm shows.
        frame(&mut shell, &ctx, Vec::new());
        let y = shell.bar_point(7).expect("ticket 7's bar is drawn").y;
        let window = |shell: &mut swarmception_ui::Shell| {
            let state = shell.timeline().widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
            (state.window.start, state.window.end)
        };
        // A window inside the data, so a pan has room either way.
        let session = shell.timeline().widget().session();
        session.call::<Show>(ShowParams { start: 600_000_000_000, end: 1_200_000_000_000 }).unwrap();
        frame(&mut shell, &ctx, Vec::new());
        let before = window(&mut shell);
        frame(&mut shell, &ctx, vec![egui::Event::ModifiersChanged(modifiers)]);
        let button = |at, pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers };
        let start = egui::pos2(600.0, y);
        frame(&mut shell, &ctx, vec![egui::Event::PointerMoved(start), button(start, true)]);
        for x in [570.0, 520.0, 450.0, 400.0] {
            frame(&mut shell, &ctx, vec![egui::Event::PointerMoved(egui::pos2(x, y))]);
        }
        frame(&mut shell, &ctx, vec![button(egui::pos2(400.0, y), false)]);
        let after = window(&mut shell);
        assert!(after.0 > before.0, "{modifiers:?}: the content followed the pointer left: {before:?} → {after:?}");
        assert_eq!(after.1 - after.0, before.1 - before.0, "{modifiers:?}: a pan keeps the span");
        let selection = shell.timeline().widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap().selection;
        assert!(selection.is_empty(), "{modifiers:?}: a pan selects nothing");
    }
    std::fs::remove_dir_all(dir).ok();
}

/// #110: a ticket is one bar in its own colour, its stages the strip along its foot, as
/// `rows.list` answers them: the widget draws exactly those (`query.bars`' strip), and its
/// tooltip names the stage under the pointer with how long it took.
#[test]
fn a_tickets_one_bar_draws_its_stages_as_its_strip() {
    use swarmception_domain::rows::{Node, Stage};
    use timeline_engine::query_cmds::{Bars, BarsParams, Tooltip};
    use timeline_engine::state_cmds::{Set as SetState, SetParams as SetStateParams};
    use timeline_engine::tracks_cmds::{HitParams, Layout, LayoutParams};
    // #7 claimed at 03:01, submitted at 03:30, the last event: waiting since, for no time yet.
    let (dir, mut engine, mut pane) = opened("2026-10-31T03:30:00Z");
    let list: swarmception_engine::rows_cmds::ListResult = serde_json::from_str(&engine.execute("rows.list", "").unwrap()).unwrap();
    let (key, bar) = list
        .rows
        .iter()
        .flat_map(Node::leaves)
        .find_map(|leaf| match leaf {
            Node::Ranges { key, bars, .. } if key != "merged" => bars.iter().position(|b| b.ticket == Some(7)).map(|i| (key.clone(), (i, bars[i].clone()))),
            _ => None,
        })
        .unwrap();
    let (place, bar) = bar;
    assert_eq!(bar.colour, swarmception_domain::colours::ticket_colour(7), "in its own colour");
    assert_eq!(bar.segments.iter().map(|s| s.stage).collect::<Vec<_>>(), [Stage::Claimed]);
    let session = pane.widget().session();
    session.call::<SetState>(SetStateParams { width: Some(1_600.0), ..SetStateParams::default() }).unwrap();
    let bars = session.call::<Bars>(BarsParams { tracks: Some(vec![key.clone()]), label_char_width: Some(7.0) }).unwrap();
    let track = &bars.tracks[0];
    let at = track.ranges.iter().position(|r| r.range == place).unwrap();
    let strip = track.strips.iter().find(|s| s.bar == at).expect("its strip");
    let [r, g, b] = bar.segments[0].colour;
    let colours: Vec<[u8; 4]> = strip.segments.iter().map(|s| s.colour).collect();
    assert_eq!(colours, [[r, g, b, 255]], "claimed, all along");
    // The tooltip over the claimed stretch names it.
    let layout = session.call::<Layout>(LayoutParams { pane: 600.0, scroll: 0.0 }).unwrap();
    let row = layout.rows.iter().find(|r| r.key == key).unwrap();
    let piece = strip.segments[0];
    let point = HitParams { pane: 600.0, scroll: 0.0, x: layout.label_width + piece.x + piece.width / 2.0, y: row.y + row.height / 2.0 };
    let tip = session.call::<Tooltip>(point).unwrap();
    assert_eq!(tip.lines.last().map(timeline_engine::wire::TooltipLineWire::text).as_deref(), Some("■ claimed · 29 min 00 s"));
    std::fs::remove_dir_all(dir).ok();
}

/// As [`opened`] with #7 submitted at 03:30, and `events` logged after.
fn opened_then(events: &[&str]) -> (PathBuf, Session, TimelinePane) {
    let dir = repo_with("2026-10-31T03:30:00Z");
    let log = dir.join("tracking/mini-aa1111/work.jsonl");
    let lines: String = events.iter().map(|e| format!("{e}\n")).collect();
    std::fs::write(&log, std::fs::read_to_string(&log).unwrap() + &lines).unwrap();
    let out = std::process::Command::new("git")
        .current_dir(&dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "-am", "more"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let mut engine = Session::new();
    let path = serde_json::to_string(&dir.display().to_string()).unwrap();
    engine.execute("repo.open", &format!("{{\"path\":{path}}}")).unwrap();
    let mut pane = TimelinePane::default();
    pane.sync(&mut engine).unwrap();
    (dir, engine, pane)
}

/// #119 (swarm-timeline#211): a merge is a marker of no length (#115), and its tooltip, as the
/// widget answers it, is the merge's own lines, as `rows.list` gives them, with no duration
/// line: a merge at an instant has none, so no "duration 0 ns".
#[test]
fn a_merge_markers_tooltip_has_no_duration_line() {
    use swarmception_domain::rows::{MERGED_KEY, Node};
    use timeline_engine::query_cmds::{Bars, BarsParams, Tooltip};
    use timeline_engine::state_cmds::{Set as SetState, SetParams as SetStateParams};
    use timeline_engine::tracks_cmds::{HitParams, Layout, LayoutParams};
    // #7 claimed at 03:01, submitted at 03:30, merged at 03:45.
    let (dir, mut engine, mut pane) = opened_then(&[r#"{"event":"merge","issue":7,"mr":8,"t":"2026-10-31T03:45:00Z"}"#]);
    // The merge's own lines, as the engine's rows give them.
    let list: swarmception_engine::rows_cmds::ListResult = serde_json::from_str(&engine.execute("rows.list", "").unwrap()).unwrap();
    let merge = list
        .rows
        .iter()
        .flat_map(Node::leaves)
        .find_map(|leaf| match leaf {
            Node::Ranges { key, bars, .. } if key == MERGED_KEY => bars.first().cloned(),
            _ => None,
        })
        .expect("#7's merge");
    assert_eq!(merge.start, merge.end, "a marker of no length (#115)");
    // Hovered at the middle of its 3 px marker.
    let session = pane.widget().session();
    session.call::<SetState>(SetStateParams { width: Some(1_600.0), ..SetStateParams::default() }).unwrap();
    let bars = session.call::<Bars>(BarsParams { tracks: Some(vec![MERGED_KEY.to_owned()]), label_char_width: Some(7.0) }).unwrap();
    let bar = bars.tracks[0].ranges.iter().find(|b| b.range == 0 && b.count == 1).expect("the merge's marker");
    assert!(bar.packed && bar.width >= 3.0, "drawn as a marker: {bar:?}");
    let layout = session.call::<Layout>(LayoutParams { pane: 600.0, scroll: 0.0 }).unwrap();
    let row = layout.rows.iter().find(|r| r.key == MERGED_KEY).unwrap();
    let point = HitParams { pane: 600.0, scroll: 0.0, x: layout.label_width + bar.x + bar.width / 2.0, y: row.y + row.height / 2.0 };
    let tip = session.call::<Tooltip>(point).unwrap();
    let lines: Vec<String> = tip.lines.iter().map(timeline_engine::wire::TooltipLineWire::text).collect();
    assert_eq!(lines, swarmception_domain::tooltip::texts(&merge.tooltip_lines), "the merge's lines alone");
    assert!(!lines.iter().any(|l| l.starts_with("duration")), "no duration line: {lines:?}");
    std::fs::remove_dir_all(dir).ok();
}

/// #121 (swarm-timeline#213): a ticket's tooltip over a short timeline pane near the screen's
/// bottom is drawn whole, inside the screen, past the pane's own rows: the widget no longer
/// clips it to them.
#[test]
fn a_tickets_tooltip_over_a_short_pane_is_whole() {
    use swarmception_domain::rows::Node;
    use swarmception_engine::repo_cmds::Status;
    // #7 claimed at 03:01, working from 03:05, submitted at 03:30, merged at 03:45: a tall tooltip.
    let (dir, mut engine, mut pane) =
        opened_then(&[r#"{"event":"work","issue":7,"t":"2026-10-31T03:05:00Z"}"#, r#"{"event":"merge","issue":7,"mr":8,"t":"2026-10-31T03:45:00Z"}"#]);
    // #7's bar's lines, as the engine's rows give them: each plain line whole, each coloured one
    // its runs (the widget draws a run at a time).
    let list: swarmception_engine::rows_cmds::ListResult = serde_json::from_str(&engine.execute("rows.list", "").unwrap()).unwrap();
    let bar = list
        .rows
        .iter()
        .flat_map(Node::leaves)
        .find_map(|leaf| match leaf {
            Node::Ranges { key, bars, .. } if key != "merged" => bars.iter().find(|b| b.ticket == Some(7)).cloned(),
            _ => None,
        })
        .unwrap();
    let wanted: Vec<String> = bar
        .tooltip_lines
        .iter()
        .flat_map(|line| if line.is_plain() { vec![line.text()] } else { line.spans.iter().map(|s| s.text.clone()).collect() })
        .filter(|text| !text.trim().is_empty())
        .collect();
    assert!(bar.tooltip_lines.len() >= 9, "a tall tooltip: {:?}", bar.tooltip);
    let status = engine.call::<Status>(swarmception_engine::engine_cmds::NoParams {}).unwrap();
    // The pane 125 px tall at the bottom of a 1000 × 700 screen, its rows about 100 px: under the
    // tooltip. Alt held, so the tooltip shows at once.
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
    let area = egui::Rect::from_min_max(egui::pos2(0.0, 575.0), egui::pos2(1000.0, 700.0));
    let alt = egui::Modifiers { alt: true, ..Default::default() };
    let run = |pane: &mut TimelinePane, mut events: Vec<egui::Event>| {
        events.insert(0, egui::Event::ModifiersChanged(alt));
        let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                ui.set_clip_rect(area);
                pane.ui(ui, &status);
            });
        });
        output.textures_delta.clear();
        output.shapes
    };
    run(&mut pane, Vec::new());
    // Left of the bar's middle, so the box (right of the pointer) has the screen's width, and in
    // the visible top of its row, the last, which the pane's bottom cuts.
    let middle = pane.bar_point(&mut engine, 7).expect("#7's bar is drawn");
    let at = egui::pos2(middle.x - 150.0, middle.y.min(area.bottom() - 20.0));
    assert!(area.contains(at), "{at:?} in the pane");
    run(&mut pane, vec![egui::Event::PointerMoved(at)]);
    let shapes = run(&mut pane, vec![egui::Event::PointerMoved(at)]);
    let drawn: Vec<(String, egui::Rect, egui::Rect)> = shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.visual_bounding_rect(), clipped.clip_rect)),
            _ => None,
        })
        .collect();
    // Every line drawn, and every copy of it whole: inside its clip and the screen.
    for text in &wanted {
        assert!(drawn.iter().any(|(t, _, _)| t == text), "{text:?} drawn");
    }
    for (text, rect, clip) in drawn.iter().filter(|(t, _, _)| wanted.contains(t)) {
        assert!(clip.contains_rect(*rect) && screen.contains_rect(*rect), "{text:?} whole: {rect:?} in {clip:?}");
    }
    // Taller than the pane's rows: it reaches above them, over the toolbar.
    let top = drawn.iter().filter(|(t, _, _)| wanted.contains(t)).map(|(_, rect, _)| rect.top()).fold(f32::INFINITY, f32::min);
    assert!(top < area.top() + 40.0, "above the rows: {top}");
    std::fs::remove_dir_all(dir).ok();
}

/// #124 (swarm-timeline#215): a ticket's tooltip box fits its text. Its lines are long (stretches
/// with timestamps, the worker and its instance), and the box was sized at 7 px a character
/// while egui lays the text out wider, so they ran past its right edge. Each line now ends inside
/// the box, and the box is inside the screen, wherever the pointer is on the bar.
#[test]
fn a_long_ticket_tooltips_lines_end_inside_its_box() {
    use swarmception_domain::rows::Node;
    use swarmception_engine::repo_cmds::Status;
    let (dir, mut engine, mut pane) =
        opened_then(&[r#"{"event":"work","issue":7,"t":"2026-10-31T03:05:00Z"}"#, r#"{"event":"merge","issue":7,"mr":8,"t":"2026-10-31T03:45:00Z"}"#]);
    let list: swarmception_engine::rows_cmds::ListResult = serde_json::from_str(&engine.execute("rows.list", "").unwrap()).unwrap();
    let bar = list
        .rows
        .iter()
        .flat_map(Node::leaves)
        .find_map(|leaf| match leaf {
            Node::Ranges { key, bars, .. } if key != "merged" => bars.iter().find(|b| b.ticket == Some(7)).cloned(),
            _ => None,
        })
        .unwrap();
    // The bar's lines as the widget draws them: each plain line whole, each coloured one its runs.
    let wanted: Vec<String> = bar
        .tooltip_lines
        .iter()
        .flat_map(|line| if line.is_plain() { vec![line.text()] } else { line.spans.iter().map(|s| s.text.clone()).collect() })
        .filter(|text| !text.trim().is_empty())
        .collect();
    let longest = bar.tooltip_lines.iter().map(|line| line.text().chars().count()).max().unwrap_or(0);
    assert!(longest >= 40, "a long line, where 7 px a character falls short: {:?}", bar.tooltip);
    let first = bar.tooltip_lines.first().map(|line| line.text()).unwrap();
    let status = engine.call::<Status>(swarmception_engine::engine_cmds::NoParams {}).unwrap();
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
    let alt = egui::Modifiers { alt: true, ..Default::default() };
    let run = |pane: &mut TimelinePane, mut events: Vec<egui::Event>| {
        // Alt held: the tooltip shows at once.
        events.insert(0, egui::Event::ModifiersChanged(alt));
        let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            pane.ui(ui, &status);
        });
        output.textures_delta.clear();
        output.shapes
    };
    run(&mut pane, Vec::new());
    let middle = pane.bar_point(&mut engine, 7).expect("#7's bar is drawn");
    // The pointer at two places on the bar: the box goes right of it where it fits, else left of
    // it, else from the screen's left edge (a box of ~740 px on a 1000 px screen).
    for x in [middle.x.min(300.0), middle.x.max(900.0)] {
        let at = egui::pos2(x, middle.y);
        run(&mut pane, vec![egui::Event::PointerMoved(at)]);
        let shapes = run(&mut pane, vec![egui::Event::PointerMoved(at)]);
        // The tooltip's first line, the last drawn of that text (a bar's label can read the same),
        // and its box, the filled rect drawn just before it.
        let Some(start) = shapes.iter().rposition(|c| matches!(&c.shape, egui::Shape::Text(t) if t.galley.text() == first)) else {
            panic!("{x}: #7's tooltip drawn at {at:?}: {:?}", bar.tooltip);
        };
        let boxed = shapes[..start]
            .iter()
            .rev()
            .find_map(|c| match &c.shape {
                egui::Shape::Rect(rect) if rect.fill != egui::Color32::TRANSPARENT => Some(rect.rect),
                _ => None,
            })
            .expect("the tooltip's box");
        assert!(screen.contains_rect(boxed), "{x}: the box inside the screen: {boxed:?}");
        let drawn: Vec<(String, egui::Rect)> = shapes[start..]
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.visual_bounding_rect())),
                _ => None,
            })
            .collect();
        for text in &wanted {
            assert!(drawn.iter().any(|(t, _)| t == text), "{x}: {text:?} drawn: {drawn:?}");
        }
        // Every run of every line, each copy of a strong one too (runs repeat across lines: " by
        // mini-aa1111" ends each stretch's line, the longest last).
        for (text, rect) in drawn.iter().filter(|(t, _)| wanted.contains(t)) {
            assert!(boxed.contains_rect(*rect), "{x}: {text:?} ends inside the box: {rect:?} in {boxed:?}");
        }
        // And the box fits its text: no wider than its widest line and its padding (5 px a side).
        let widest = drawn.iter().filter(|(t, _)| wanted.contains(t)).map(|(_, r)| r.right()).fold(f32::MIN, f32::max);
        assert!(boxed.right() - widest <= 2.0 * 5.0 + 1.0, "{x}: the box fits its text, no wider: {widest} in {boxed:?}");
    }
    std::fs::remove_dir_all(dir).ok();
}
