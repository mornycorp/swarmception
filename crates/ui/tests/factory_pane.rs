//! The factory pane over a real repository's tracking branch: it draws what `factory.at` returns
//! for its time, station by station, and puts the timeline's playhead at that time.

use std::path::PathBuf;

use swarmception_domain::factory::{CrateState, FactoryScene};
use swarmception_engine::factory_cmds::{At, AtParams};
use swarmception_model::Timestamp;
use swarmception_ui::{FactorySource, Shell};
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::state_cmds::Get;

mod support;

/// A repository whose tracking branch holds two workers and a coordinator's gate.
fn repo() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "swarmception-factory-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        // Two tests can start within one tick of the clock: a counter keeps their directories apart.
        {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        }
    ));
    let files = [
        (
            "mini-aa1111",
            [
                r#"{"event":"session.start","machine":"mini","name":"worker-1","model":"claude-opus-5-5","effort":"high","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:02:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:30:00Z"}"#,
                r#"{"event":"claim","issue":9,"t":"2026-10-31T03:31:00Z"}"#,
                r#"{"event":"work","issue":9,"t":"2026-10-31T03:32:00Z"}"#,
            ]
            .join("\n"),
        ),
        (
            "mini-bb2222",
            [
                r#"{"event":"session.start","machine":"mini","name":"worker-2","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":11,"mr":12,"t":"2026-10-31T03:05:00Z"}"#,
                r#"{"event":"submit","issue":11,"mr":12,"t":"2026-10-31T03:35:00Z"}"#,
                r#"{"event":"claim","issue":13,"t":"2026-10-31T04:30:00Z"}"#,
            ]
            .join("\n"),
        ),
        (
            "mini-cc3333",
            [
                r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7)","sub":"gate-pr8","t":"2026-10-31T03:40:00Z"}"#,
                r#"{"event":"merge","issue":7,"mr":8,"sub":"gate-pr8","t":"2026-10-31T03:45:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0; merged #8","sub":"gate-pr8","t":"2026-10-31T03:46:00Z"}"#,
            ]
            .join("\n"),
        ),
    ];
    for (instance, text) in files {
        std::fs::create_dir_all(dir.join("tracking").join(instance)).unwrap();
        std::fs::write(dir.join("tracking").join(instance).join("work.jsonl"), text + "\n").unwrap();
    }
    // Each git call at `at` (the commit's time), as a test user.
    let run_at = |at: &str, args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(&dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_AUTHOR_DATE", at)
            .env("GIT_COMMITTER_DATE", at)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    let run = |args: &[&str]| run_at("2026-10-31T02:00:00Z", args);
    run(&["init", "-q", "-b", "tracking"]);
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "events"]);
    // Main: a setup PR no ticket names (#13, 03:20), then #7's PR (#8, 03:45).
    run(&["checkout", "-q", "--orphan", "main"]);
    run(&["rm", "-rqf", "."]);
    run(&["commit", "-q", "--allow-empty", "-m", "start"]);
    for (pr, at) in [("13", "2026-10-31T03:20:00Z"), ("8", "2026-10-31T03:45:00Z")] {
        let branch = format!("pr-{pr}");
        run(&["checkout", "-q", "-b", &branch]);
        run_at(at, &["commit", "-q", "--allow-empty", "-m", "work"]);
        run(&["checkout", "-q", "main"]);
        run_at(at, &["merge", "-q", "--no-ff", "-m", &format!("Merge pull request #{pr} from mornycorp/{branch}"), &branch]);
    }
    dir
}

/// One frame of the shell, 1400 × 900 points: the text it drew.
fn frame(shell: &mut Shell, ctx: &egui::Context) -> Vec<String> {
    frame_with(shell, ctx, Vec::new())
}

/// One frame with `events`: the text it drew.
fn frame_with(shell: &mut Shell, ctx: &egui::Context, events: Vec<egui::Event>) -> Vec<String> {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn loaded(dir: &std::path::Path) -> (egui::Context, Shell) {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, Some(dir));
    // The factory's layout at the window's full width: the details pane (#47) closed.
    let mut state = shell.ui_state();
    state.panes.details = false;
    shell.set_ui_state(state);
    support::until("the tracking branch loaded", || {
        frame(&mut shell, &ctx);
        shell.has_timeline()
    });
    assert!(shell.has_timeline(), "the tracking branch loaded");
    (ctx, shell)
}

fn issues(crates: &[swarmception_domain::factory::Crate]) -> Vec<u64> {
    crates.iter().map(|c| c.issue).collect()
}

#[test]
fn the_pane_draws_what_factory_at_returns() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let t = Timestamp::parse("2026-10-31T03:42:00Z").unwrap();
    shell.set_factory_at(t);
    frame(&mut shell, &ctx);
    let texts = frame(&mut shell, &ctx);
    assert_eq!(shell.factory().source(), FactorySource::Playhead, "on the timeline's playhead, put there");
    assert_eq!(shell.factory().t(), Some(t));

    let shown: FactoryScene = shell.factory().scene().cloned().unwrap();
    let asked = shell.engine().call::<At>(AtParams { t: Some(t), recent: Some(shown.recent), ticket: None, run: None }).unwrap();
    assert_eq!(shown, asked, "the pane shows factory.at's answer for its time");
    // 03:42: worker-1 works on #9, #7 is in the gate, #11 waits at the dock, #13 is up next.
    assert_eq!(issues(&asked.gate.crates), [7]);
    assert_eq!(issues(&asked.dock), [11]);
    assert_eq!(issues(&asked.upcoming), [13]);
    assert_eq!(asked.benches[0].crates.first().map(|c| (c.issue, c.state)), Some((9, CrateState::Working)));

    let drawn = shell.factory().drawn().clone();
    assert_eq!(drawn.upcoming, issues(&asked.upcoming));
    assert_eq!(drawn.dock, issues(&asked.dock));
    assert_eq!(drawn.gate, issues(&asked.gate.crates));
    let benches: Vec<(String, Vec<u64>)> = asked.benches.iter().filter(|b| !b.grouped_idle()).map(|b| (b.key.clone(), issues(&b.crates))).collect();
    assert_eq!(drawn.benches, benches);
    assert_eq!(drawn.idle_benches, asked.idle.as_ref().map(|g| g.keys.clone()).unwrap_or_default());
    let runs: Vec<String> = asked.gate.running.iter().chain(&asked.gate.recent).map(|r| r.sub.clone()).collect();
    assert_eq!(drawn.runs, runs);
    assert_eq!(drawn.blocks, asked.shipping.blocks.len());
    assert_eq!(drawn.hidden, 0, "everything fits at this size");
    for label in ["#7", "#9", "#11", "#13", "worker-1", "worker-2", "running gate-pr8 · 2m", "at the timeline's playhead"] {
        assert!(texts.iter().any(|text| text.contains(label)), "{label} drawn: {texts:?}");
    }
    // #85: no clock of its own; the timeline's playhead is the time.
    assert!(!texts.iter().any(|text| text.contains("03:42:00")), "no clock: {texts:?}");
    // #31: no ticker; the key along the bottom; the gate's list of runs, no press.
    assert!(!texts.iter().any(|text| text.contains("JUST HAPPENED")), "{texts:?}");
    for key in ["border = phase · fill = ticket", "waiting for merge", "in the gate"] {
        assert!(texts.iter().any(|text| text == key), "{key} in the key: {texts:?}");
    }

    // The timeline's playhead went to the item 03:42 falls in.
    let state = shell.timeline().widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
    assert!(state.playhead.is_some());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn following_the_timeline_takes_its_window_end_and_play_moves_on() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    frame(&mut shell, &ctx);
    assert_eq!(shell.factory().source(), FactorySource::WindowEnd, "no pointer, no playhead: the window's end");
    let end = shell.factory().scene().unwrap().t;

    shell.set_factory_at(Timestamp::parse("2026-10-31T03:10:00Z").unwrap());
    let mut state = shell.ui_state();
    state.factory.playing = true;
    shell.set_ui_state(state);
    let t = |shell: &Shell| shell.factory().scene().unwrap().t;
    frame(&mut shell, &ctx);
    let first = t(&shell);
    for _ in 0..5 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(shell.factory().source(), FactorySource::Playing);
    assert!(t(&shell) >= first && t(&shell) < end, "play moves on through the window: {first:?} → {:?}", t(&shell));
    // Play moves the timeline's playhead (#85), not a clock of the factory's own.
    let playhead = shell.inspect().timeline.playhead.unwrap();
    let place = |shell: &mut Shell, time| {
        shell.engine().call::<swarmception_engine::factory_cmds::PlaceAt>(swarmception_engine::factory_cmds::PlaceAtParams { time }).unwrap().item
    };
    let now = t(&shell);
    let (on, at) = (place(&mut shell, now), place(&mut shell, playhead));
    assert_eq!(on, at, "the playhead on the item play's t falls in");
    std::fs::remove_dir_all(dir).ok();
}

/// Where ticket `issue`'s chips were drawn, at every station.
fn chips(shell: &Shell, issue: u64) -> Vec<egui::Rect> {
    shell.factory().drawn().chips.iter().filter(|(i, _)| *i == issue).map(|(_, r)| *r).collect()
}

/// The timeline's selected bars: (row key, range), in order.
fn selected(shell: &mut Shell) -> Vec<(String, usize)> {
    let state = shell.timeline().widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
    state.selected_ranges.into_iter().flat_map(|(track, ranges)| ranges.into_iter().map(move |r| (track.clone(), r))).collect()
}

#[test]
fn one_chip_its_whole_life_sized_by_station() {
    use swarmception_ui::Size;
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:42:00Z").unwrap());
    // About a second of frames: the chips that moved have glided and resized into place.
    for _ in 0..60 {
        frame(&mut shell, &ctx);
    }
    // #9 at the bench is large; #7 in the gate, #11 at the dock and #13 up next are medium.
    let size = |shell: &Shell, issue| chips(shell, issue).first().map(egui::Rect::size);
    assert_eq!(size(&shell, 9), Some(Size::Large.dims()));
    for issue in [7, 11, 13] {
        assert_eq!(size(&shell, issue), Some(Size::Medium.dims()), "#{issue}");
    }
    // At 03:50 #7 has merged: it flies from the gate to its brick in main's wall.
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:50:00Z").unwrap());
    frame(&mut shell, &ctx);
    let flying = chips(&shell, 7)[0];
    let place = |shell: &Shell| shell.factory().drawn().bricks.iter().find(|(mark, _)| mark == "#7").map(|(_, rect)| *rect).unwrap();
    assert_ne!(flying, place(&shell), "it sets off from the gate");
    for _ in 0..60 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(chips(&shell, 7)[0], place(&shell), "and lands in its place in the wall");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn hovering_a_ticket_rings_it_everywhere_and_highlights_its_bars() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:42:00Z").unwrap());
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    // #7 is in the gate and in the ticker (its submit, 12 min before, is recent at this zoom).
    let on = chips(&shell, 7)[0].center();
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(on)]);
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.factory().hovered(), Some(7));
    let drawn = shell.factory().drawn();
    let everywhere = drawn.chips.iter().filter(|(i, _)| *i == 7).count();
    assert!(everywhere >= 1);
    assert_eq!(drawn.highlighted.iter().filter(|i| **i == 7).count(), everywhere, "ringed at every station it is drawn at");
    assert!(drawn.highlighted.iter().all(|i| *i == 7), "and nothing else");
    let want = ticket_bars(&mut shell, 7);
    assert!(!want.is_empty());
    assert_eq!(shell.timeline().highlighted(), want, "its bars highlighted on the timeline");
    assert!(selected(&mut shell).is_empty(), "and none selected");

    // The pointer leaves: nothing hovered, nothing highlighted.
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerGone]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.factory().hovered(), None);
    assert!(shell.timeline().highlighted().is_empty());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn clicking_a_ticket_traces_its_journey() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:42:00Z").unwrap());
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    let click = |shell: &mut Shell, at: egui::Pos2| {
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame_with(shell, &ctx, vec![egui::Event::PointerMoved(at), button(true)]);
        frame_with(shell, &ctx, vec![button(false)]);
        frame(shell, &ctx);
        frame(shell, &ctx);
    };
    let on = chips(&shell, 7)[0].center();
    click(&mut shell, on);
    assert_eq!(shell.ui_state().factory.ticket, Some(7));
    // Claimed at 03:01, submitted at 03:30, gated from 03:40: by 03:42, 29 min, 10 min and 2 min.
    assert_eq!(shell.factory().scene().and_then(|s| s.journey.as_ref()).map(|j| j.text.as_str()), Some("bench 29m → dock 10m → gate 2m"));
    assert_eq!(shell.factory().drawn().journey, ["bench 29m", "dock 10m", "gate 2m"]);
    let on = chips(&shell, 7)[0].center();
    click(&mut shell, on);
    assert_eq!(shell.ui_state().factory.ticket, None, "a second click stops tracing");
    assert!(shell.factory().drawn().journey.is_empty());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_merge_no_ticket_names_is_a_grey_brick_marked_with_its_pr() {
    use swarmception_domain::colours::NO_TICKET as NO_TICKET_COLOUR;
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:50:00Z").unwrap());
    let mut texts = Vec::new();
    for _ in 0..3 {
        texts = frame(&mut shell, &ctx);
    }
    let blocks = shell.factory().scene().unwrap().shipping.blocks.clone();
    let marks: Vec<(&str, Option<u64>)> = blocks.iter().map(|b| (b.mark.as_str(), b.issue)).collect();
    assert_eq!(marks, [("!13", None), ("#7", Some(7))], "the setup PR, then #7's");
    assert_eq!(blocks[0].colour, NO_TICKET_COLOUR);
    assert_eq!(blocks[0].tooltip[0], "Merge pull request #13 from mornycorp/pr-13");
    assert_eq!(shell.factory().drawn().marks, ["!13", "#7"], "each brick says what it is");
    // #32: #7's brick carries its life from factory.at (claimed 03:01, merged 03:45: 44 min,
    // through a gate run), and is wider than the grey one, which has none.
    let seven = &blocks[1];
    assert_eq!(seven.took, Some(44 * 60));
    assert!(seven.segments.iter().any(|s| s.state == CrateState::Gating), "{:?}", seven.segments);
    assert_eq!(seven.segments.iter().map(|s| s.seconds).sum::<i64>(), 44 * 60);
    assert!(seven.tooltip.iter().any(|line| line == "claim → merge 44 min 00 s"), "{:?}", seven.tooltip);
    let bricks = shell.factory().drawn().bricks.clone();
    assert!(bricks[1].1.width() > bricks[0].1.width(), "{bricks:?}");
    assert!(texts.iter().any(|t| t == "!13"), "{texts:?}");
    std::fs::remove_dir_all(dir).ok();
}

/// Ticket `issue`'s bars (`timeline.ticketBars`), by row key, as the widget's highlight holds them.
fn ticket_bars(shell: &mut Shell, issue: u64) -> std::collections::BTreeMap<String, Vec<usize>> {
    let bars = shell.engine().call::<swarmception_engine::factory_cmds::TicketBars>(swarmception_engine::factory_cmds::TicketParams { issue }).unwrap().bars;
    let mut by_row = std::collections::BTreeMap::<String, Vec<usize>>::new();
    for bar in bars {
        by_row.entry(bar.track).or_default().push(bar.range);
    }
    for ranges in by_row.values_mut() {
        ranges.sort_unstable();
        ranges.dedup();
    }
    by_row
}

#[test]
fn hovering_a_chip_highlights_its_bars_and_never_touches_the_selection() {
    use timeline_engine::ranges_cmds::{RangeParams, Toggle};
    use timeline_engine::state_cmds::{Set, SetParams};
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:42:00Z").unwrap());
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    // The person's selection: a bar of #11 selected and focused, the playhead on item 0.
    let eleven = ticket_bars(&mut shell, 11);
    let (row, ranges) = eleven.iter().next().unwrap();
    let session = shell.timeline().widget().session();
    session.call::<Toggle>(RangeParams { track: row.clone(), range: ranges[0] }).unwrap();
    session.call::<Set>(SetParams { playhead: Some(0), ..SetParams::default() }).unwrap();
    let person = |shell: &mut Shell| {
        let s = shell.timeline().widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
        (s.selection, s.selected_ranges, s.selected_instant, s.focus, s.playhead)
    };
    let before = person(&mut shell);
    assert!(shell.timeline().highlighted().is_empty(), "nothing highlighted before a hover");
    frame(&mut shell, &ctx);
    // The details follow the person's own bar (#47), which a hover never moves.
    let details = shell.ui_state().details.subject;
    assert!(details.is_some(), "the person's bar chosen");
    // The factory follows the playhead the person moved (#85): its chips glide there first.
    for _ in 0..60 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(person(&mut shell), before);

    // Every frame of a hover on #7, then on #9, then away: the selection as it was, the
    // highlight on the hovered ticket's bars, then on none.
    let mut seen = Vec::new();
    for (issue, at) in [(Some(7), chips(&shell, 7)[0].center()), (Some(9), chips(&shell, 9)[0].center())] {
        frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(at)]);
        for _ in 0..3 {
            assert_eq!(person(&mut shell), before, "the selection untouched while #{issue:?} is hovered");
            frame(&mut shell, &ctx);
        }
        assert_eq!(shell.factory().hovered(), issue);
        let lit = shell.timeline().highlighted();
        assert!(!lit.is_empty());
        assert_eq!(lit, ticket_bars(&mut shell, issue.unwrap()), "the highlight follows #{issue:?}'s bars");
        // The control channel's ui.inspect reports it beside the selection.
        let inspect = shell.inspect();
        assert_eq!((inspect.timeline.highlighted, inspect.timeline.selected.clone()), (lit.clone(), before.1.clone()));
        seen.push(lit);
    }
    assert_ne!(seen[0], seen[1], "moving from ticket to ticket moves the highlight");
    // The widget is told once per change, not every frame: the revision stands while the hover does.
    let revision = shell.timeline().widget().session().revision();
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.timeline().widget().session().revision(), revision);
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerGone]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.factory().hovered(), None);
    assert!(shell.timeline().highlighted().is_empty());
    assert_eq!(person(&mut shell), before);
    assert_eq!(shell.ui_state().details.subject, details, "the details still on the person's bar");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_hover_wins_the_highlight_and_its_leave_gives_it_back_to_the_details() {
    use swarmception_domain::details::Subject;
    use swarmception_ui::Lit;
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let mut state = shell.ui_state();
    state.panes.details = true;
    shell.set_ui_state(state);
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:42:00Z").unwrap());
    let gate = Subject::Gate { sub: "gate-pr8".into(), at: None };
    shell.select(gate.clone());
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    // The details' gate run: its merge onto main and #7's bars, highlighted.
    let answer = shell.engine().call::<swarmception_engine::details_cmds::Get>(swarmception_ui::details_params(&gate)).unwrap();
    let mut gate_bars = std::collections::BTreeMap::<String, Vec<usize>>::new();
    for bar in &answer.timeline.bars {
        gate_bars.entry(bar.track.clone()).or_default().push(bar.range);
    }
    let mut want = ticket_bars(&mut shell, 7);
    let on_main = gate_bars.get("main").cloned().unwrap_or_default();
    assert_eq!(on_main.len(), 1, "one merge onto main: {gate_bars:?}");
    let params = swarmception_engine::details_cmds::DetailsParams { track: Some("main".into()), range: Some(on_main[0]), ..Default::default() };
    let merge = shell.engine().call::<swarmception_engine::details_cmds::Get>(params).unwrap();
    assert!(merge.title.starts_with("PR #8"), "the merge onto main of PR #8: {}", merge.title);
    want.insert("main".into(), on_main);
    assert_eq!(gate_bars, want, "no bar of its own: the gate's rows are gone (#117)");
    assert_eq!((shell.timeline().highlighted(), shell.timeline().lit()), (gate_bars.clone(), Lit::Details));
    let before = shell.timeline().selection().unwrap();

    // A hover on #9's chip wins while it lasts.
    let on = chips(&shell, 9)[0].center();
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(on)]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.factory().hovered(), Some(9));
    assert_eq!(shell.timeline().highlighted(), ticket_bars(&mut shell, 9));
    assert_eq!(shell.inspect().timeline.lit, Lit::Hovered { ticket: 9 });

    // Its leave gives the gate run its highlight back; the selection and the details as they were.
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerGone]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.factory().hovered(), None);
    assert_eq!((shell.timeline().highlighted(), shell.inspect().timeline.lit), (gate_bars, Lit::Details));
    assert_eq!(shell.timeline().selection().unwrap(), before, "the selection never touched");
    assert_eq!(shell.ui_state().details.subject, Some(gate));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_persons_selection_survives_hovering_a_chip_and_leaving_it() {
    use timeline_engine::ranges_cmds::{RangeParams, Toggle};
    use timeline_engine::state_cmds::{Set, SetParams};
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:42:00Z").unwrap());
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    let hover_and_leave = |shell: &mut Shell, issue: u64| {
        let on = chips(shell, issue)[0].center();
        frame_with(shell, &ctx, vec![egui::Event::PointerMoved(on)]);
        frame(shell, &ctx);
        frame(shell, &ctx);
        assert_eq!(shell.factory().hovered(), Some(issue));
        let lit = shell.timeline().highlighted();
        // Straight on to another ticket, then away.
        let next = chips(shell, 11)[0].center();
        frame_with(shell, &ctx, vec![egui::Event::PointerMoved(next)]);
        frame(shell, &ctx);
        frame_with(shell, &ctx, vec![egui::Event::PointerGone]);
        frame(shell, &ctx);
        assert_eq!(shell.factory().hovered(), None);
        lit
    };

    // The person selected #11's bars (#110: its merge and its bar) and focused the last.
    let bars = |shell: &mut Shell, issue| {
        shell.engine().call::<swarmception_engine::factory_cmds::TicketBars>(swarmception_engine::factory_cmds::TicketParams { issue }).unwrap().bars
    };
    let mine = bars(&mut shell, 11);
    assert!(!mine.is_empty());
    for bar in &mine {
        shell.timeline().widget().session().call::<Toggle>(RangeParams { track: bar.track.clone(), range: bar.range }).unwrap();
    }
    let before = shell.timeline().selection().unwrap();
    let lit = hover_and_leave(&mut shell, 7);
    assert_eq!(lit, ticket_bars(&mut shell, 7), "the hover highlighted #7's bars");
    assert_eq!(shell.timeline().selection().unwrap(), before, "and left the person's selection exactly as it was");
    assert!(shell.timeline().highlighted().is_empty(), "nothing highlighted once the pointer left");

    // A merge's span selected alone, the playhead moved to its start: put back too.
    let session = shell.timeline().widget().session();
    session.call::<timeline_engine::ranges_cmds::Clear>(NoParams {}).unwrap();
    session.call::<Toggle>(RangeParams { track: "merged".into(), range: 0 }).unwrap();
    session.call::<Set>(SetParams { playhead: Some(0), ..SetParams::default() }).unwrap();
    let before = shell.timeline().selection().unwrap();
    assert_eq!((before.ranges.get("merged").cloned(), before.playhead), (Some(vec![0]), Some(0)));
    // The factory follows the playhead (#85): its chips glide there first.
    for _ in 0..60 {
        frame(&mut shell, &ctx);
    }
    hover_and_leave(&mut shell, 9);
    assert_eq!(shell.timeline().selection().unwrap(), before);

    // Nothing selected stays nothing.
    shell.timeline().widget().session().call::<timeline_engine::ranges_cmds::Clear>(NoParams {}).unwrap();
    let before = shell.timeline().selection().unwrap();
    hover_and_leave(&mut shell, 7);
    assert_eq!(shell.timeline().selection().unwrap(), before);
    std::fs::remove_dir_all(dir).ok();
}

/// The timeline's visible window, as clock times.
fn window(shell: &mut Shell) -> (Timestamp, Timestamp) {
    use swarmception_engine::rows_cmds::{TimeAt, TimeAtParams};
    let state = shell.timeline().widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
    let mut at = |position| shell.engine().call::<TimeAt>(TimeAtParams { position }).unwrap().time;
    (at(state.window.start), at(state.window.end))
}

#[test]
fn the_strip_draws_what_stats_get_returns_for_the_window() {
    use swarmception_engine::stats_cmds::{At as StatsAt, Get as GetStats, GetParams};
    use swarmception_ui::{percent, rate};
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:42:00Z").unwrap());
    let mut texts = Vec::new();
    for _ in 0..3 {
        texts = frame(&mut shell, &ctx);
    }
    let (start, end) = window(&mut shell);
    let params = GetParams { start: Some(StatsAt::Seconds(start.0)), end: Some(StatsAt::Seconds(end.0)) };
    let asked = shell.engine().call::<GetStats>(params).unwrap();
    let strip = shell.factory().strip();
    assert_eq!(strip.stats(), Some(&asked), "the strip shows stats.get's answer for the timeline's window");
    assert_eq!(asked.landed, 1, "#7 merged in the window");
    let drawn = strip.drawn();
    assert_eq!(drawn.headline, rate(asked.landed_per_active_hour));
    assert_eq!(drawn.landed, format!("{} landed · {:.1} active h", asked.landed, asked.active_seconds as f64 / 3_600.0));
    assert_eq!(drawn.bars, asked.hours.len());
    let t = Timestamp::parse("2026-10-31T03:42:00Z").unwrap();
    let holding = asked.hours.iter().position(|h| h.start <= t && t.0 < h.start.0 + 3_600);
    assert!(holding.is_some());
    assert_eq!(drawn.marked, holding, "the hour holding t is marked");
    assert!(drawn.figures.contains(&format!("gates pass {}", percent(asked.gates.pass_rate))), "{:?}", drawn.figures);
    assert_eq!(drawn.figures.len(), 6);
    assert!(texts.contains(&drawn.headline), "the headline drawn: {texts:?}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn clicking_an_hours_bar_zooms_the_timeline_to_it_and_the_strip_keeps_its_range() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    let range = |shell: &Shell| shell.factory().strip().stats().map(|s| (s.start, s.end)).unwrap();
    let before = range(&shell);
    let click_hour = |shell: &mut Shell, hour: Timestamp| {
        let hours = shell.factory().strip().drawn().hours.clone();
        let bar = hours.iter().find(|(start, _)| *start == hour).map(|(_, rect)| rect.center()).expect("the hour's bar drawn");
        let button = |pressed| egui::Event::PointerButton { pos: bar, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame_with(shell, &ctx, vec![egui::Event::PointerMoved(bar), button(true)]);
        frame_with(shell, &ctx, vec![button(false)]);
        for _ in 0..3 {
            frame(shell, &ctx);
        }
    };
    let three = Timestamp::parse("2026-10-31T03:00:00Z").unwrap();
    click_hour(&mut shell, three);
    // The hour, as much of it as the timeline holds: its first event is at 03:00, its extent
    // starts a minute later.
    let end = Timestamp(three.0 + 3_600);
    let (start, until) = window(&mut shell);
    assert!(three <= start && start <= Timestamp(three.0 + 60) && until == end, "the timeline's window is that hour: {start:?} → {until:?}");
    let t = shell.factory().scene().unwrap().t;
    assert_eq!(t, Timestamp(end.0 - 1), "the factory at the hour's last second");
    let playhead = shell.inspect().timeline.playhead.expect("a playhead");
    assert!(start <= playhead && playhead < end, "the playhead in it: {playhead:?}");
    // #85: the strip doesn't zoom itself: its range as it was, the hour clicked marked.
    assert_eq!(range(&shell), before, "the strip keeps its range");
    let strip = shell.factory().strip();
    assert!(strip.kept());
    assert_eq!(strip.clicked(), Some(three));
    let marked = strip.drawn().clicked.and_then(|n| strip.drawn().hours.get(n)).map(|(start, _)| *start);
    assert_eq!(marked, Some(three), "its bar marked");
    assert_ne!(before, (start, until), "a range the click's window isn't");
    let inspect = shell.inspect().factory.strip;
    assert_eq!((inspect.kept, inspect.clicked, inspect.window.map(|w| (w.start, w.end))), (true, Some(three), Some(before)));

    // The same hour again from the kept strip: the range still kept.
    click_hour(&mut shell, three);
    assert_eq!((range(&shell), shell.factory().strip().clicked()), (before, Some(three)));

    // The person moves the timeline's window: the strip follows it again, nothing marked.
    shell.timeline().fit();
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    let (start, until) = window(&mut shell);
    assert_eq!(range(&shell), (start, until), "over the timeline's window again");
    assert!(!shell.factory().strip().kept() && shell.factory().strip().drawn().clicked.is_none());

    // The control channel's way in, as the click: `ui.set`'s hour.
    let before = range(&shell);
    assert!(matches!(shell.answer("ui.set", r#"{"hour": "2026-10-31T03:20:00Z"}"#), swarmception_ui::control::Answer::Now(Ok(_))));
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    let (start, until) = window(&mut shell);
    assert!(three <= start && until == end, "the timeline on 03:00's hour: {start:?} → {until:?}");
    assert_eq!((range(&shell), shell.factory().strip().clicked()), (before, Some(three)));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_strip_asks_again_only_when_the_window_or_the_data_moves() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    let asks = shell.factory().strip().asks();
    let span = |shell: &Shell| shell.factory().strip().stats().map(|s| s.end.0 - s.start.0).unwrap();
    let before = span(&shell);
    // Frames with nothing moving, t moving included: no new ask.
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:10:00Z").unwrap());
    for _ in 0..10 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(shell.factory().strip().asks(), asks, "not every frame, nor when t alone moves");
    // A zoom changes the window: one more ask, over the narrower window.
    shell.timeline().zoom(2.0);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(shell.factory().strip().asks(), asks + 1);
    assert!(span(&shell) < before, "{} then {}", before, span(&shell));
    // New rows (the data moved on): one more.
    shell.engine().execute("rows.set", r#"{"grouping":"workers"}"#).unwrap();
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(shell.factory().strip().asks(), asks + 2);
    std::fs::remove_dir_all(dir).ok();
}

/// A key pressed and let go, with no modifiers.
fn key(key: egui::Key) -> Vec<egui::Event> {
    let event = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE };
    vec![event(true), event(false)]
}

/// A click at `at`, then frames for what it set going.
fn click_at(shell: &mut Shell, ctx: &egui::Context, at: egui::Pos2) {
    let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(shell, ctx, vec![egui::Event::PointerMoved(at), button(true)]);
    frame_with(shell, ctx, vec![button(false)]);
    frame(shell, ctx);
}

/// `key` pressed, then frames for the step to land: the text drawn last.
fn press(shell: &mut Shell, ctx: &egui::Context, k: egui::Key) -> Vec<String> {
    frame_with(shell, ctx, key(k));
    frame(shell, ctx);
    frame(shell, ctx)
}

#[test]
fn stepping_by_key_moment_back_and_forth_names_each_and_moves_the_playhead() {
    use swarmception_domain::moments::{Direction, Step};
    use swarmception_engine::factory_cmds::{PlaceAt, PlaceAtParams, StepParams, StepTo};
    use swarmception_ui::{Jump, Steps};
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let at = |text: &str| Timestamp::parse(text).unwrap();
    shell.set_factory_at(at("2026-10-31T03:10:00Z"));
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    // A click on the floor away from any chip gives the factory the keyboard.
    let floor = shell.factory().floor().unwrap();
    let empty = floor.left_bottom() + egui::vec2(3.0, -3.0);
    assert!(!shell.factory().drawn().chips.iter().any(|(_, r)| r.contains(empty)));
    click_at(&mut shell, &ctx, empty);

    let t = |shell: &Shell| shell.factory().t();
    let playhead = |shell: &mut Shell| shell.timeline().widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap().playhead;
    let item = |shell: &mut Shell, t| shell.engine().call::<PlaceAt>(PlaceAtParams { time: t }).unwrap().item;
    // → from 03:10: the setup PR landing on main at 03:20, named in the header.
    let texts = press(&mut shell, &ctx, egui::Key::ArrowRight);
    let asked: Step = shell.engine().call::<StepTo>(StepParams { t: Some(at("2026-10-31T03:10:00Z")), direction: Direction::Next, kinds: Vec::new() }).unwrap();
    assert_eq!(asked.t, Some(at("2026-10-31T03:20:00Z")));
    assert_eq!(t(&shell), asked.t, "t is the step's");
    let view = shell.ui_state().factory;
    assert_eq!(shell.factory().moment(), Some(&asked), "the pane shows factory.step's answer");
    assert_eq!(shell.factory().scene().unwrap().t, at("2026-10-31T03:20:00Z"));
    assert_eq!(shell.factory().source(), FactorySource::Stepped);
    assert!(texts.iter().any(|text| text == "PR #13 merged onto main"), "named in the header: {texts:?}");
    let want = item(&mut shell, at("2026-10-31T03:20:00Z"));
    assert_eq!(playhead(&mut shell), want, "the timeline's playhead follows");
    assert!(view.follow && !view.playing, "the factory follows the playhead the step moved");

    // ← twice: #11's claim at 03:05, then #7's work at 03:02.
    let texts = press(&mut shell, &ctx, egui::Key::ArrowLeft);
    assert_eq!(t(&shell), Some(at("2026-10-31T03:05:00Z")));
    assert!(texts.iter().any(|text| text == "#11 claimed · worker-2"), "{texts:?}");
    press(&mut shell, &ctx, egui::Key::ArrowLeft);
    assert_eq!(t(&shell), Some(at("2026-10-31T03:02:00Z")));

    // Only merges onto main: 03:20, 03:45, then none, t staying.
    let mut state = shell.ui_state();
    state.factory.steps = Steps::Merges;
    shell.set_ui_state(state);
    press(&mut shell, &ctx, egui::Key::ArrowRight);
    let texts = press(&mut shell, &ctx, egui::Key::ArrowRight);
    assert_eq!(t(&shell), Some(at("2026-10-31T03:45:00Z")));
    assert!(texts.iter().any(|text| text == "#7 merged onto main (PR #8) · worker-1"), "{texts:?}");
    let texts = press(&mut shell, &ctx, egui::Key::ArrowRight);
    assert_eq!(t(&shell), Some(at("2026-10-31T03:45:00Z")), "no later merge: t stays");
    assert!(texts.iter().any(|text| text == "no later merge onto main"), "{texts:?}");

    // Home and End: the first and last events, as ⏮ / ⏭'s shift-click.
    press(&mut shell, &ctx, egui::Key::Home);
    assert_eq!(t(&shell), Some(at("2026-10-31T03:00:00Z")));
    press(&mut shell, &ctx, egui::Key::End);
    assert_eq!(t(&shell), Some(at("2026-10-31T04:30:00Z")));
    assert_eq!(shell.factory().moment(), None, "a jump names no moment");
    shell.jump_factory(Jump::Step(Direction::Previous));
    frame(&mut shell, &ctx);
    assert_eq!(t(&shell), Some(at("2026-10-31T03:45:00Z")), "the shell's own way in: the last merge before");

    // A click on the timeline takes the keyboard there: → is the timeline's, not the factory's;
    // the factory shows wherever the timeline's playhead is (#85).
    click_at(&mut shell, &ctx, egui::pos2(700.0, 150.0));
    press(&mut shell, &ctx, egui::Key::ArrowRight);
    frame(&mut shell, &ctx);
    assert_eq!(shell.factory().moment(), None, "no step of the factory's");
    let playhead = shell.inspect().timeline.playhead;
    assert_eq!(t(&shell), playhead, "the factory on the timeline's playhead");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_factory_follows_the_timelines_playhead_and_holds_when_it_doesnt() {
    use timeline_engine::state_cmds::{Set, SetParams};
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:42:00Z").unwrap());
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    // The person moves the playhead on the timeline: the factory goes with it, to its item's time.
    let item = |shell: &mut Shell, text: &str| {
        let time = Timestamp::parse(text).unwrap();
        shell.engine().call::<swarmception_engine::factory_cmds::PlaceAt>(swarmception_engine::factory_cmds::PlaceAtParams { time }).unwrap().item
    };
    let submit = item(&mut shell, "2026-10-31T03:30:00Z");
    shell.timeline().widget().session().call::<Set>(SetParams { playhead: submit, ..SetParams::default() }).unwrap();
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.factory().t(), Timestamp::parse("2026-10-31T03:30:00Z"), "the playhead's time");
    assert_eq!(shell.factory().scene().unwrap().t, Timestamp::parse("2026-10-31T03:30:00Z").unwrap());
    assert_eq!(shell.factory().source(), FactorySource::Playhead);
    assert_eq!(shell.inspect().factory.header, "at the timeline's playhead");

    // Not following: it holds where it was, and says so in a small label.
    let mut state = shell.ui_state();
    state.factory.follow = false;
    shell.set_ui_state(state);
    let merge = item(&mut shell, "2026-10-31T03:45:00Z");
    shell.timeline().widget().session().call::<Set>(SetParams { playhead: merge, ..SetParams::default() }).unwrap();
    frame(&mut shell, &ctx);
    let texts = frame(&mut shell, &ctx);
    assert_eq!(shell.factory().t(), Timestamp::parse("2026-10-31T03:30:00Z"), "held");
    assert_eq!(shell.factory().source(), FactorySource::Held);
    let label = "not following the timeline: held at 03:30:00 UTC";
    assert!(texts.iter().any(|t| t == label), "{texts:?}");
    assert_eq!(shell.inspect().factory.header, label);

    // Following again: the playhead's time.
    let mut state = shell.ui_state();
    state.factory.follow = true;
    shell.set_ui_state(state);
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_eq!(shell.factory().t(), Timestamp::parse("2026-10-31T03:45:00Z"));
    std::fs::remove_dir_all(dir).ok();
}

/// A click at `at` with `modifiers` held, then frames for what it set going.
fn click_with(shell: &mut Shell, ctx: &egui::Context, at: egui::Pos2, modifiers: egui::Modifiers) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers };
    let held = egui::Event::ModifiersChanged(modifiers);
    for events in [vec![held.clone(), egui::Event::PointerMoved(at), button(true)], vec![button(false)], Vec::new()] {
        let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
        ctx.run_ui(input, |ui| shell.ui(ui)).textures_delta.clear();
    }
    frame(shell, ctx);
}

#[test]
fn a_click_on_a_tickets_bar_shows_its_start_and_alt_click_its_end() {
    use swarmception_ui::FactorySelected;
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    frame(&mut shell, &ctx);
    let at = shell.bar_point(7).expect("#7's bar is drawn");
    click_with(&mut shell, &ctx, at, egui::Modifiers::NONE);
    assert_eq!(shell.factory().selected(), Some(&FactorySelected::Ticket { issue: 7 }));
    assert_eq!(shell.factory().t(), Timestamp::parse("2026-10-31T03:01:00Z"), "a click: the claim");
    // Alt (#137): the same ticket, the factory at its merge.
    click_with(&mut shell, &ctx, at, egui::Modifiers::ALT);
    assert_eq!(shell.factory().selected(), Some(&FactorySelected::Ticket { issue: 7 }));
    assert_eq!(shell.factory().t(), Timestamp::parse("2026-10-31T03:45:00Z"), "Alt + click: the merge");
    click_with(&mut shell, &ctx, at, egui::Modifiers::NONE);
    assert_eq!(shell.factory().t(), Timestamp::parse("2026-10-31T03:01:00Z"), "back to the start");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn dragging_crosses_ticket_rows_and_x_constrains_with_or_without_alt() {
    use swarmception_ui::FactorySelected;
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    frame(&mut shell, &ctx);
    for modifiers in [egui::Modifiers::NONE, egui::Modifiers::ALT] {
        let start = shell.bar_point(7).unwrap();
        let other = egui::pos2(start.x, shell.bar_point(11).unwrap().y);
        assert_ne!(start.y, other.y);
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers };
        let key = |pressed| egui::Event::Key { key: egui::Key::X, physical_key: None, pressed, repeat: false, modifiers };
        frame_with(&mut shell, &ctx, vec![egui::Event::ModifiersChanged(modifiers), egui::Event::PointerMoved(start), button(start, true)]);
        frame(&mut shell, &ctx);
        let initial = shell.factory().t();
        assert_eq!(shell.factory().selected(), Some(&FactorySelected::Ticket { issue: 7 }));
        frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(other)]);
        frame(&mut shell, &ctx);
        assert_eq!(shell.factory().selected(), Some(&FactorySelected::Ticket { issue: 11 }), "vertical motion crosses rows");
        frame_with(&mut shell, &ctx, vec![key(true)]);
        frame(&mut shell, &ctx);
        assert_eq!(shell.factory().selected(), Some(&FactorySelected::Ticket { issue: 7 }), "X selects in the starting row");
        assert_eq!(shell.factory().t(), initial, "Alt still determines the endpoint");
        frame_with(&mut shell, &ctx, vec![key(false)]);
        frame(&mut shell, &ctx);
        assert_eq!(shell.factory().selected(), Some(&FactorySelected::Ticket { issue: 11 }));
        frame_with(&mut shell, &ctx, vec![button(other, false), egui::Event::PointerGone]);
        frame(&mut shell, &ctx);
    }
    std::fs::remove_dir_all(dir).ok();
}

/// A bar selected on the timeline, alone, as a click selects it.
fn choose_bar(shell: &mut Shell, track: &str, range: usize) {
    use timeline_engine::ranges_cmds::{RangeParams, Toggle};
    let session = shell.timeline().widget().session();
    session.call::<timeline_engine::ranges_cmds::Clear>(NoParams {}).unwrap();
    session.call::<Toggle>(RangeParams { track: track.to_owned(), range }).unwrap();
}

/// Ticket `issue`'s first bar off the merges rows, chosen on the timeline, then frames.
fn choose_ticket(shell: &mut Shell, ctx: &egui::Context, issue: u64) {
    let bars = shell.engine().call::<swarmception_engine::factory_cmds::TicketBars>(swarmception_engine::factory_cmds::TicketParams { issue }).unwrap().bars;
    let bar = bars.iter().find(|b| b.track != "merged").unwrap().clone();
    choose_bar(shell, &bar.track, bar.range);
    for _ in 0..3 {
        frame(shell, ctx);
    }
}

#[test]
fn a_bar_selected_on_the_timeline_selects_it_in_the_factory() {
    use swarmception_domain::details::Subject;
    use swarmception_ui::FactorySelected;
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:42:00Z").unwrap());
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(shell.factory().selected(), None);

    // #11's bar on the timeline: its chip at the dock, ringed; its journey traced.
    choose_ticket(&mut shell, &ctx, 11);
    assert_eq!(shell.ui_state().details.subject, Some(Subject::Ticket { issue: 11 }));
    assert_eq!(shell.factory().selected(), Some(&FactorySelected::Ticket { issue: 11 }));
    assert_eq!(shell.factory().drawn().selected, ["dock #11"]);
    assert_eq!(shell.ui_state().factory.ticket, Some(11), "its journey traced");
    let inspect = shell.inspect().factory;
    assert_eq!((inspect.selected, inspect.drawn.selected), (Some(FactorySelected::Ticket { issue: 11 }), vec!["dock #11".to_owned()]));

    // At 03:50 #7 has merged: its bar selects its brick on main.
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:50:00Z").unwrap());
    choose_ticket(&mut shell, &ctx, 7);
    assert_eq!(shell.factory().selected(), Some(&FactorySelected::Ticket { issue: 7 }));
    assert_eq!(shell.factory().drawn().selected, ["brick #7"]);

    // A merge onto main no ticket names (the setup PR, !13): its brick.
    choose_bar(&mut shell, "main", 0);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert!(matches!(shell.ui_state().details.subject, Some(Subject::Merge { .. })), "{:?}", shell.ui_state().details.subject);
    assert!(matches!(shell.factory().selected(), Some(FactorySelected::Merge { .. })));
    assert_eq!(shell.factory().drawn().selected, ["brick !13"]);
    assert_eq!(shell.ui_state().factory.ticket, None, "a merge traces no ticket");

    // A gate run chosen in the details (#117: no bar of its own on the timeline): its line in
    // the gate's list.
    shell.select(Subject::Gate { sub: "gate-pr8".into(), at: None });
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    assert!(matches!(&shell.ui_state().details.subject, Some(Subject::Gate { sub, .. }) if sub == "gate-pr8"), "{:?}", shell.ui_state().details.subject);
    assert!(matches!(shell.factory().selected(), Some(FactorySelected::Run { sub, .. }) if sub == "gate-pr8"));
    assert_eq!(shell.factory().drawn().selected, ["run gate-pr8"]);
    assert_eq!(shell.ui_state().factory.ticket, None, "a run traces no ticket");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_ticket_selected_in_the_factory_is_lit_on_the_timeline_and_back() {
    use swarmception_domain::details::Subject;
    use swarmception_ui::{FactorySelected, Lit};
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let mut state = shell.ui_state();
    state.panes.details = true;
    shell.set_ui_state(state);
    shell.set_factory_at(Timestamp::parse("2026-10-31T03:42:00Z").unwrap());
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    // The factory → the timeline: #9's chip clicked: its bars lit on the timeline.
    let on = chips(&shell, 9)[0].center();
    click_at(&mut shell, &ctx, on);
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerGone]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.ui_state().details.subject, Some(Subject::Ticket { issue: 9 }));
    assert_eq!((shell.timeline().highlighted(), shell.timeline().lit()), (ticket_bars(&mut shell, 9), Lit::Details));
    assert_eq!(shell.factory().drawn().selected, ["bench #9"]);

    // The timeline → the factory: #7's bar chosen there moves the factory's selection to #7.
    choose_ticket(&mut shell, &ctx, 7);
    assert_eq!(shell.factory().selected(), Some(&FactorySelected::Ticket { issue: 7 }));
    assert_eq!(shell.factory().drawn().selected, ["gate #7"]);
    assert_eq!(shell.timeline().highlighted(), ticket_bars(&mut shell, 7));
    std::fs::remove_dir_all(dir).ok();
}

/// #154: the shell builds the timeline in the look egui draws in, light and back to dark, so its
/// colours are the theme's light ones on a light look.
#[test]
fn the_timeline_follows_the_windows_look() {
    use swarmception_domain::theme::Look;
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    frame(&mut shell, &ctx);
    assert_eq!(shell.engine().look(), Look::Dark);
    ctx.set_visuals(egui::Visuals::light());
    frame(&mut shell, &ctx);
    assert_eq!(shell.engine().look(), Look::Light, "a light look builds the timeline light");
    ctx.set_visuals(egui::Visuals::dark());
    frame(&mut shell, &ctx);
    assert_eq!(shell.engine().look(), Look::Dark);
    std::fs::remove_dir_all(dir).ok();
}
