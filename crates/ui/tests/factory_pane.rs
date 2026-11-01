//! The factory pane over a real repository's tracking branch: it draws what `factory.at` returns
//! for its time, station by station, and puts the timeline's playhead at that time.

use std::path::PathBuf;

use swarmception_domain::factory::{CrateState, FactoryScene};
use swarmception_engine::factory_cmds::{At, AtParams};
use swarmception_model::Timestamp;
use swarmception_ui::{FactorySource, Shell};
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::state_cmds::Get;

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
    for _ in 0..500 {
        frame(&mut shell, &ctx);
        if shell.has_timeline() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
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
    assert_eq!(shell.factory().source(), FactorySource::Set);

    let shown: FactoryScene = shell.factory().scene().cloned().unwrap();
    let asked = shell.engine().call::<At>(AtParams { t: Some(t), recent: Some(shown.recent), ticket: None }).unwrap();
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
    let benches: Vec<(String, Vec<u64>)> = asked.benches.iter().map(|b| (b.key.clone(), issues(&b.crates))).collect();
    assert_eq!(drawn.benches, benches);
    let runs: Vec<String> = asked.gate.running.iter().chain(&asked.gate.recent).map(|r| r.sub.clone()).collect();
    assert_eq!(drawn.runs, runs);
    assert_eq!(drawn.blocks, asked.shipping.blocks.len());
    assert_eq!(drawn.hidden, 0, "everything fits at this size");
    for label in ["#7", "#9", "#11", "#13", "worker-1", "worker-2", "running gate-pr8 · 2m", "2026-10-31 03:42:00 UTC"] {
        assert!(texts.iter().any(|text| text.contains(label)), "{label} drawn: {texts:?}");
    }
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

    let mut state = shell.ui_state();
    state.factory.playing = true;
    state.factory.t = Some(Timestamp::parse("2026-10-31T03:10:00Z").unwrap());
    shell.set_ui_state(state);
    let t = |shell: &Shell| shell.factory().scene().unwrap().t;
    frame(&mut shell, &ctx);
    let first = t(&shell);
    for _ in 0..5 {
        frame(&mut shell, &ctx);
    }
    assert_eq!(shell.factory().source(), FactorySource::Playing);
    assert!(t(&shell) >= first && t(&shell) < end, "play moves on through the window: {first:?} → {:?}", t(&shell));
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
fn hovering_a_ticket_rings_it_everywhere_and_selects_its_bars() {
    use swarmception_engine::factory_cmds::{TicketBars, TicketParams};
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
    let bars = shell.engine().call::<TicketBars>(TicketParams { issue: 7 }).unwrap();
    let want: Vec<(String, usize)> = bars.bars.into_iter().map(|b| (b.track, b.range)).collect();
    assert!(!want.is_empty());
    assert_eq!(selected(&mut shell), want, "its bars selected on the timeline");

    // The pointer leaves: nothing hovered, nothing selected.
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerGone]);
    frame(&mut shell, &ctx);
    assert_eq!(shell.factory().hovered(), None);
    assert!(selected(&mut shell).is_empty());
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
        let lit = selected(shell);
        // Straight on to another ticket, then away.
        let next = chips(shell, 11)[0].center();
        frame_with(shell, &ctx, vec![egui::Event::PointerMoved(next)]);
        frame(shell, &ctx);
        frame_with(shell, &ctx, vec![egui::Event::PointerGone]);
        frame(shell, &ctx);
        assert_eq!(shell.factory().hovered(), None);
        lit
    };

    // The person selected two bars of #11 and focused the second.
    let bars = |shell: &mut Shell, issue| {
        shell.engine().call::<swarmception_engine::factory_cmds::TicketBars>(swarmception_engine::factory_cmds::TicketParams { issue }).unwrap().bars
    };
    let mine = bars(&mut shell, 11);
    assert!(mine.len() >= 2);
    for bar in &mine {
        shell.timeline().widget().session().call::<Toggle>(RangeParams { track: bar.track.clone(), range: bar.range }).unwrap();
    }
    let before = shell.timeline().selection().unwrap();
    let lit = hover_and_leave(&mut shell, 7);
    assert_ne!(lit, selected(&mut shell), "the hover showed #7's bars instead");
    assert_eq!(shell.timeline().selection().unwrap(), before, "and put the person's selection back exactly");

    // A merge's span selected alone, the playhead moved to its start: put back too.
    let session = shell.timeline().widget().session();
    session.call::<timeline_engine::ranges_cmds::Clear>(NoParams {}).unwrap();
    session.call::<Toggle>(RangeParams { track: "merged".into(), range: 0 }).unwrap();
    session.call::<Set>(SetParams { playhead: Some(0), ..SetParams::default() }).unwrap();
    let before = shell.timeline().selection().unwrap();
    assert_eq!((before.ranges.get("merged").cloned(), before.playhead), (Some(vec![0]), Some(0)));
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
fn clicking_an_hours_bar_zooms_the_timeline_to_it_and_moves_t_with_it() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    let three = Timestamp::parse("2026-10-31T03:00:00Z").unwrap();
    let hours = shell.factory().strip().drawn().hours.clone();
    let bar = hours.iter().find(|(start, _)| *start == three).map(|(_, rect)| rect.center()).expect("03:00's bar drawn");
    let button = |pressed| egui::Event::PointerButton { pos: bar, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(bar), button(true)]);
    frame_with(&mut shell, &ctx, vec![button(false)]);
    for _ in 0..3 {
        frame(&mut shell, &ctx);
    }
    // The hour, as much of it as the timeline holds: its first event is at 03:00, its extent
    // starts a minute later.
    let end = Timestamp(three.0 + 3_600);
    let (start, until) = window(&mut shell);
    assert!(three <= start && start <= Timestamp(three.0 + 60) && until == end, "the timeline's window is that hour: {start:?} → {until:?}");
    let t = shell.factory().scene().unwrap().t;
    assert!(start <= t && t < end, "the factory's t is in it: {t:?}");
    let strip = shell.factory().strip().stats().unwrap();
    assert_eq!((strip.start, strip.end), (start, until), "and the strip is over it");
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
