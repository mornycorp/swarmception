//! Each row's lanes sized over the window (#97, the widget's `view.lanes`): the pane turns it on
//! with every load, and the widget packs the lanes again over the new window once the view has
//! held still after a move, which `ui.inspect` reports. Since #117 (the Ready checks row, the
//! one that stacked, gone) every row is one lane: the tickets are packed so they never overlap,
//! and the merges, four of them at once here, are markers of no length (#115).

use std::path::PathBuf;

use swarmception_domain::rows::MERGED_KEY;
use swarmception_engine::factory_cmds::{PlaceAt, PlaceAtParams};
use swarmception_model::Timestamp;
use swarmception_ui::Shell;
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::lanes_cmds::LanesOverWire;
use timeline_engine::state_cmds::{Get, Set, SetParams, TimelineState};
use timeline_engine::tracks_cmds::Rows;
use timeline_engine::wire::SpanWire;

mod support;

/// Four workers each claim a ticket at 03:01, merged all at once at 03:29; worker-1 claims four
/// more one after another, merged ten minutes apart from 04:00.
fn repo() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "swarmception-lanes-window-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    for w in 1..=4_u32 {
        let mut lines = vec![
            format!(r#"{{"event":"session.start","machine":"mini","name":"worker-{w}","role":"worker","t":"2026-10-31T03:00:00Z"}}"#),
            format!(r#"{{"event":"claim","issue":{w},"mr":{},"t":"2026-10-31T03:01:00Z"}}"#, w + 10),
            format!(r#"{{"event":"merge","issue":{w},"mr":{},"t":"2026-10-31T03:29:00Z"}}"#, w + 10),
        ];
        if w == 1 {
            for k in 0..4 {
                let issue = 5 + k;
                lines.push(format!(r#"{{"event":"claim","issue":{issue},"mr":{},"t":"2026-10-31T03:5{k}:00Z"}}"#, issue + 10));
                lines.push(format!(r#"{{"event":"merge","issue":{issue},"mr":{},"t":"2026-10-31T04:{k}0:00Z"}}"#, issue + 10));
            }
        }
        lines.push(r#"{"event":"session.stop","t":"2026-10-31T05:00:00Z"}"#.to_owned());
        let instance = format!("mini-aa{w}{w}{w}{w}");
        std::fs::create_dir_all(dir.join("tracking").join(&instance)).unwrap();
        std::fs::write(dir.join("tracking").join(&instance).join("work.jsonl"), lines.join("\n") + "\n").unwrap();
    }
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(&dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_AUTHOR_DATE", "2026-10-31T05:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-10-31T05:00:00Z")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    run(&["init", "-q", "-b", "tracking"]);
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "events"]);
    dir
}

/// One frame at `time` seconds, 1400 × 900.
fn frame(shell: &mut Shell, ctx: &egui::Context, time: f64) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), time: Some(time), ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
}

fn state(shell: &mut Shell) -> TimelineState {
    shell.timeline().widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap()
}

fn lanes(shell: &mut Shell) -> Vec<u32> {
    shell.timeline().widget().session().call::<Rows>(NoParams {}).unwrap().rows.into_iter().map(|row| row.lanes).collect()
}

fn merged_lanes(shell: &mut Shell) -> u32 {
    let rows = shell.timeline().widget().session().call::<Rows>(NoParams {}).unwrap().rows;
    rows.into_iter().find(|row| row.key == MERGED_KEY).map(|row| row.lanes).unwrap()
}

fn place(shell: &mut Shell, time: &str) -> i64 {
    shell.engine().call::<PlaceAt>(PlaceAtParams { time: Timestamp::parse(time).unwrap() }).unwrap().position
}

#[test]
fn the_lanes_are_packed_over_the_window_once_it_holds_still() {
    let dir = repo();
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, Some(&dir));
    let mut time = 1.0;
    support::until("the tracking branch loaded", || {
        frame(&mut shell, &ctx, time);
        time += 0.01;
        shell.has_timeline()
    });
    // On with the load, packed over the whole swarm shown.
    let loaded = state(&mut shell);
    assert_eq!(loaded.lanes_over, LanesOverWire::Window, "the pane sizes the lanes over the window");
    assert_eq!(loaded.lanes_window, Some(loaded.window));
    assert_eq!(lanes(&mut shell), [1].repeat(lanes(&mut shell).len()), "every row one lane, the four merges at 03:29 too");

    // The window on the later merges: packed over it once the view has held still.
    let (start, end) = (place(&mut shell, "2026-10-31T04:05:00Z"), place(&mut shell, "2026-10-31T04:25:00Z"));
    shell.timeline().widget().session().call::<Set>(SetParams { window: Some(SpanWire { start, end }), ..SetParams::default() }).unwrap();
    frame(&mut shell, &ctx, time);
    assert_eq!(state(&mut shell).lanes_window, Some(loaded.window), "not until the view holds still");
    for _ in 0..3 {
        time += 0.15;
        frame(&mut shell, &ctx, time);
    }
    assert_eq!(state(&mut shell).lanes_window, Some(SpanWire { start, end }));
    assert_eq!(merged_lanes(&mut shell), 1);
    // ui.inspect reports the window they were packed over, as clock times.
    let inspect = shell.inspect();
    assert!(inspect.version >= 14, "lanesWindow since version 14");
    let packed = inspect.timeline.lanes_window.unwrap();
    assert_eq!((packed.start, packed.end), (Timestamp::parse("2026-10-31T04:05:00Z").unwrap(), Timestamp::parse("2026-10-31T04:25:00Z").unwrap()));

    // Back over the four merges at once: still one lane, markers of no length.
    let (start, end) = (place(&mut shell, "2026-10-31T03:20:00Z"), place(&mut shell, "2026-10-31T03:35:00Z"));
    shell.timeline().widget().session().call::<Set>(SetParams { window: Some(SpanWire { start, end }), ..SetParams::default() }).unwrap();
    for _ in 0..4 {
        time += 0.15;
        frame(&mut shell, &ctx, time);
    }
    assert_eq!(state(&mut shell).lanes_window, Some(SpanWire { start, end }));
    assert_eq!(merged_lanes(&mut shell), 1);
    let _ = std::fs::remove_dir_all(dir);
}
