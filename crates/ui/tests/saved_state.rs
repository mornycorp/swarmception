//! The state survives a restart (#53), across the real boundary: a headless shell over a real
//! repository writes its state to a file, a second shell started over the same file reads it back
//! and shows what the first did; a swarm that moved on since keeps a view that followed its end;
//! and a corrupt or a newer build's file is read where it can be, said once, never a crash.

use std::path::{Path, PathBuf};

use swarmception_domain::Grouping;
use swarmception_domain::details::Subject;
use swarmception_engine::Github;
use swarmception_engine::factory_cmds::{PlaceAt, PlaceAtParams};
use swarmception_engine::live_cmds::{Get as GetLive, SetParams as SetLiveParams};
use swarmception_model::Timestamp;
use swarmception_ui::control::{Answer, Inspect};
use swarmception_ui::saved::{self, Keeping, StateStore};
use swarmception_ui::{MenuAction, Shell};
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::rulers_cmds::{Add, AddParams};
use timeline_engine::state_cmds::{Get, Set as SetState, SetParams as SetStateParams};
use timeline_engine::wire::{OriginWire, RulerDomainWire};

mod support;

fn scratch(what: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("worker21-state-{what}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// Two workers over four hours: #7 claimed at 03:01 and submitted at 03:30, #9 claimed at 04:00
/// and submitted at 06:30 (#9 by worker-2).
const EVENTS: [(&str, &str); 2] = [
    (
        "mini-aa1111",
        concat!(
            r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
            "\n",
            r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
            "\n",
            r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:30:00Z"}"#,
            "\n",
        ),
    ),
    (
        "mini-bb2222",
        concat!(
            r#"{"event":"session.start","machine":"mini","name":"worker-2","role":"worker","t":"2026-10-31T03:50:00Z"}"#,
            "\n",
            r#"{"event":"claim","issue":9,"mr":10,"t":"2026-10-31T04:00:00Z"}"#,
            "\n",
            r#"{"event":"submit","issue":9,"mr":10,"t":"2026-10-31T06:30:00Z"}"#,
            "\n",
        ),
    ),
];

fn repo() -> PathBuf {
    let dir = scratch("repo");
    for (instance, lines) in EVENTS {
        std::fs::create_dir_all(dir.join("tracking").join(instance)).unwrap();
        std::fs::write(dir.join("tracking").join(instance).join("work.jsonl"), lines).unwrap();
    }
    git(&dir, &["init", "-q", "-b", "tracking"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "events"]);
    dir
}

/// The swarm moves on: worker-1 claims #11 at 08:00.
fn move_on(dir: &Path) {
    let file = dir.join("tracking/mini-aa1111/work.jsonl");
    let mut text = std::fs::read_to_string(&file).unwrap();
    text.push_str(r#"{"event":"claim","issue":11,"mr":12,"t":"2026-10-31T08:00:00Z"}"#);
    text.push('\n');
    std::fs::write(&file, text).unwrap();
    git(dir, &["commit", "-q", "-am", "more events"]);
}

/// One frame of the shell, 1400 × 900 points, at `time` seconds: the text it drew.
fn frame(shell: &mut Shell, ctx: &egui::Context, time: f64) -> Vec<String> {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), time: Some(time), ..Default::default() };
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

/// A shell over `dir` keeping its state in `store`, read from there first, once the tracking
/// branch has loaded and drawn.
fn started(dir: &Path, store: &StateStore) -> (egui::Context, Shell) {
    let ctx = egui::Context::default();
    let mut shell = Shell::with_github(&ctx, Some(dir), Github { gh: PathBuf::from("/no/such/gh"), cache_dir: None });
    let key = saved::repo_key(dir);
    let loaded = store.load(&key);
    shell.keep_state(store.clone(), &key, Some(&loaded));
    support::until("the tracking branch loaded", || {
        frame(&mut shell, &ctx, 0.0);
        shell.has_timeline() && !shell.fetching_issues() && !shell.loading()
    });
    assert!(shell.has_timeline(), "the tracking branch loaded");
    for _ in 0..3 {
        frame(&mut shell, &ctx, 0.0);
    }
    (ctx, shell)
}

fn set(shell: &mut Shell, params: &str) -> Inspect {
    match shell.answer("ui.set", params) {
        Answer::Now(Ok(text)) => serde_json::from_str(&text).unwrap(),
        other => panic!("ui.set {params}: {other:?}"),
    }
}

fn rulers(shell: &mut Shell) -> Vec<RulerDomainWire> {
    let state = shell.timeline().widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
    state.rulers.iter().map(|r| r.domain).collect()
}

#[test]
fn the_state_survives_a_restart_of_a_headless_shell() {
    let (dir, home) = (repo(), scratch("home"));
    let store = StateStore::new(home.clone());
    let (ctx, mut first) = started(&dir, &store);
    let fresh = first.inspect();
    assert_eq!((fresh.saved.keeping, fresh.saved.loaded_from.as_deref(), fresh.saved.note.as_deref()), (Keeping::Kept, None, None), "no file yet");
    assert_eq!(fresh.saved.file.as_deref(), Some(store.file(&saved::repo_key(&dir)).display().to_string().as_str()));

    // The person hides the factory, groups by worker, zooms to 40 minutes before #9, puts the factory at a
    // time stepping by merges at an hour a second, and looks at #9's details; a Clock ruler
    // under the scrub ruler (the widget's own state).
    frame(&mut first, &ctx, 2.0);
    let settled = first.inspect().saved.writes;
    assert!(settled <= 1, "the first frames' state, written once: {settled}");
    first.run_menu(&ctx, MenuAction::Factory);
    first.run_menu(&ctx, MenuAction::Grouping(Grouping::Workers));
    // Live (#54) turned off by the person, checking every 2 min when on.
    assert!(first.live().default, "nobody chose yet");
    first.set_live(SetLiveParams { on: Some(false), interval_seconds: Some(120) });
    // #9's details first (the timeline brings its bars into view, #55), then a window without
    // them: restored, the window stays where it was left, not moved to the subject's bars.
    first.select(Subject::Ticket { issue: 9 });
    frame(&mut first, &ctx, 9.4);
    frame(&mut first, &ctx, 9.5);
    set(
        &mut first,
        r#"{"window": {"start": "2026-10-31T03:10:00Z", "end": "2026-10-31T03:50:00Z"}, "at": "2026-10-31T04:30:00Z", "speed": "hour", "steps": "merges"}"#,
    );
    first.timeline().widget().session().call::<Add>(AddParams { domain: RulerDomainWire::Clock, origin: Some(OriginWire::Absolute) }).unwrap();
    // The playhead on #7's submit, 03:30.
    let submit = Timestamp::parse("2026-10-31T03:30:00Z").unwrap();
    let item = first.engine().call::<PlaceAt>(PlaceAtParams { time: submit }).unwrap().item;
    first.timeline().widget().session().call::<SetState>(SetStateParams { playhead: item, ..SetStateParams::default() }).unwrap();
    frame(&mut first, &ctx, 10.0);
    // Written once it has held still a second, with no quit.
    assert_eq!(first.inspect().saved.writes, settled, "not while it may still be moving");
    frame(&mut first, &ctx, 11.5);
    let before = first.inspect();
    assert_eq!(before.saved.writes, settled + 1, "written once it held still");
    let written = store.load(&saved::repo_key(&dir)).state.expect("the file reads");
    assert_eq!(Some(written), first.app_state());
    assert_eq!(first.save_state().unwrap(), Some(store.file(&saved::repo_key(&dir))), "and on quit");
    let window = before.timeline.window.expect("a window");
    assert_eq!((window.start.to_string(), window.end.to_string()), ("2026-10-31T03:10:00Z".to_owned(), "2026-10-31T03:50:00Z".to_owned()));
    let rulers_before = rulers(&mut first);
    assert_eq!(rulers_before, [RulerDomainWire::Clock, RulerDomainWire::Clock]);
    drop(first);

    // A new shell over the same file shows what the first did.
    let (ctx2, mut second) = started(&dir, &store);
    let after = second.inspect();
    assert_eq!(after.state, before.state, "the panes and their sizes, the factory's settings and t, the details");
    assert!(!after.state.panes.factory && after.state.sizes.details.is_some());
    assert_eq!(after.state.details.subject, Some(Subject::Ticket { issue: 9 }));
    assert_eq!(second.menu_state().grouping, Grouping::Workers, "the rows by worker, through rows.set");
    let live = second.engine().call::<GetLive>(swarmception_engine::engine_cmds::NoParams {}).unwrap();
    assert_eq!((live.on, live.default, live.interval_seconds), (false, false, 120), "Live as the person left it, through live.set");
    assert_eq!(after.timeline.window, before.timeline.window, "the window it was left at, not moved to the details subject's bars");
    assert_eq!((after.timeline.playhead, before.timeline.playhead), (Some(submit), Some(submit)), "the playhead, by its time");
    assert_eq!(rulers(&mut second), rulers_before, "the widget's own state, through state.save and state.restore");
    let file = store.file(&saved::repo_key(&dir)).display().to_string();
    assert_eq!((after.saved.keeping, after.saved.loaded_from.as_deref(), after.saved.note), (Keeping::Kept, Some(file.as_str()), None));
    // New rows (a load clears the widget's playhead) keep it where it was.
    second.run_menu(&ctx2, MenuAction::Collapse(swarmception_domain::Collapse::Empty));
    frame(&mut second, &ctx2, 0.0);
    assert_eq!(second.inspect().timeline.playhead, Some(submit));
    std::fs::remove_dir_all(dir).ok();
    std::fs::remove_dir_all(home).ok();
}

#[test]
fn a_view_that_followed_the_end_follows_the_swarm_on() {
    let (dir, home) = (repo(), scratch("home"));
    let store = StateStore::new(home.clone());
    let (_ctx, mut first) = started(&dir, &store);
    // The last hour, up to the end of the swarm.
    let end = end_of(&mut first);
    let hour_before = |t: Timestamp| Timestamp(t.0 - 3_600);
    set(&mut first, &format!(r#"{{"window": {{"start": "{}", "end": "{end}"}}}}"#, hour_before(end)));
    let view = first.app_state().unwrap().timeline.unwrap();
    assert!(view.at_end && !view.fitted, "{view:?}");
    first.save_state().unwrap();
    drop(first);

    move_on(&dir);
    let (_ctx, mut second) = started(&dir, &store);
    let window = second.inspect().timeline.window.unwrap();
    assert!(second.live().default, "Live nobody chose stays the default's");
    let new_end = end_of(&mut second);
    assert!(new_end.0 >= Timestamp::parse("2026-10-31T08:00:00Z").unwrap().0, "{new_end}");
    assert_eq!((window.start, window.end), (hour_before(new_end), new_end), "the new end, as wide");

    // A window in the middle stays where it was; the whole swarm shows the whole of it again.
    set(&mut second, r#"{"window": {"start": "2026-10-31T04:00:00Z", "end": "2026-10-31T04:30:00Z"}}"#);
    second.save_state().unwrap();
    drop(second);
    let (_ctx, mut third) = started(&dir, &store);
    let window = third.inspect().timeline.window.unwrap();
    assert_eq!((window.start.to_string(), window.end.to_string()), ("2026-10-31T04:00:00Z".to_owned(), "2026-10-31T04:30:00Z".to_owned()));
    third.timeline().fit();
    let fitted = third.inspect().timeline.window.unwrap();
    assert!(third.app_state().unwrap().timeline.unwrap().fitted);
    third.save_state().unwrap();
    drop(third);
    move_on_again(&dir);
    let (_ctx, mut fourth) = started(&dir, &store);
    let whole = fourth.inspect().timeline.window.unwrap();
    assert_eq!(whole.start, fitted.start);
    assert!(whole.end > fitted.end, "the whole swarm, its new events too: {whole:?} after {fitted:?}");
    std::fs::remove_dir_all(dir).ok();
    std::fs::remove_dir_all(home).ok();
}

/// The end of the timeline's extent, as a clock time: where a fitted window ends.
fn end_of(shell: &mut Shell) -> Timestamp {
    shell.timeline().fit();
    shell.inspect().timeline.window.unwrap().end
}

/// The swarm moves on again: worker-1 submits #11 at 09:00.
fn move_on_again(dir: &Path) {
    let file = dir.join("tracking/mini-aa1111/work.jsonl");
    let mut text = std::fs::read_to_string(&file).unwrap();
    text.push_str(r#"{"event":"submit","issue":11,"mr":12,"t":"2026-10-31T09:00:00Z"}"#);
    text.push('\n');
    std::fs::write(&file, text).unwrap();
    git(dir, &["commit", "-q", "-am", "more events"]);
}

#[test]
fn corrupt_and_newer_files_are_read_where_they_can_be_and_said_once() {
    let (dir, home) = (repo(), scratch("home"));
    let store = StateStore::new(home.clone());
    let key = saved::repo_key(&dir);
    let file = store.file(&key);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();

    // Corrupt: a fresh start, the status bar saying why.
    std::fs::write(&file, "{\"version\": 1, \"ui\": {").unwrap();
    let (ctx, mut shell) = started(&dir, &store);
    let inspect = shell.inspect();
    assert_eq!(inspect.state.panes, swarmception_ui::Panes::default());
    assert_eq!(inspect.saved.loaded_from, None);
    let note = inspect.saved.note.expect("a note");
    assert!(note.contains("is corrupt") && note.contains("starting afresh"), "{note}");
    let drawn = frame(&mut shell, &ctx, 0.0);
    assert!(drawn.iter().any(|t| t.contains("is corrupt")), "the status bar says so: {drawn:?}");
    // Written again, it reads.
    shell.save_state().unwrap();
    assert_eq!(store.load(&key).note, None);
    drop(shell);

    // A newer build's file: what this one reads is kept, the rest left out, and it says so.
    let newer = serde_json::json!({
        "version": 99,
        "repo": key.display().to_string(),
        "ui": {"panes": {"timeline": true, "factory": false, "details": true}, "factory": {"follow": true, "warp": 9}},
        "rows": {"grouping": "workers", "collapse": false},
        "hologram": {"on": true}
    });
    std::fs::write(&file, newer.to_string()).unwrap();
    let (ctx, mut shell) = started(&dir, &store);
    let inspect = shell.inspect();
    assert!(!inspect.state.panes.factory);
    assert_eq!(shell.menu_state().grouping, Grouping::Workers);
    let note = inspect.saved.note.expect("a note");
    assert!(note.contains("newer Swarmception's (version 99") && note.ends_with("left out factory"), "{note}");
    assert_eq!(inspect.saved.loaded_from.as_deref(), Some(file.display().to_string().as_str()));
    let drawn = frame(&mut shell, &ctx, 0.0);
    assert!(drawn.iter().any(|t| t.contains("newer Swarmception")), "{drawn:?}");
    drop(shell);

    // A file of another repository's (a path that hashed alike) is left alone.
    std::fs::write(&file, r#"{"version": 1, "repo": "/somewhere/else", "ui": {"panes": {"factory": false}}}"#).unwrap();
    let (_ctx, shell) = started(&dir, &store);
    assert!(shell.ui_state().panes.factory);
    assert!(shell.saved().note.unwrap().contains("another repository's"));
    std::fs::remove_dir_all(dir).ok();
    std::fs::remove_dir_all(home).ok();
}

#[test]
fn a_shell_given_no_state_keeps_none() {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, None);
    assert_eq!(shell.saved(), saved::SavedInspect::default());
    assert_eq!(shell.saved().keeping, Keeping::Off);
    assert_eq!(shell.app_state(), None);
    assert_eq!(shell.save_state(), Ok(None));
}

/// #117: a state saved while the timeline had the gate's two rows (#84's `gates` and `ready`,
/// hidden or collapsed by the person) reads on without them: no note, the rest of the state kept,
/// and the rows drawn are today's, the merges' and the tickets'.
#[test]
fn a_state_naming_the_gone_gate_rows_reads_on_without_them() {
    use timeline_engine::track_state::TrackState;
    let (dir, home) = (repo(), scratch("home-gate-rows"));
    let store = StateStore::new(home.clone());
    let (ctx, mut first) = started(&dir, &store);
    first.run_menu(&ctx, MenuAction::Factory);
    frame(&mut first, &ctx, 1.0);
    let mut state = first.app_state().expect("a state kept");
    drop(first);
    let view = state.timeline.as_mut().expect("the timeline's view");
    let widget = view.widget.as_mut().expect("the widget's own state");
    widget.tracks.insert("gates".into(), TrackState { hidden: true, ..TrackState::default() });
    widget.tracks.insert("ready".into(), TrackState { collapsed: true, height: Some(4), ..TrackState::default() });
    let file = store.file(&saved::repo_key(&dir));
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, serde_json::to_string_pretty(&state).unwrap()).unwrap();
    assert!(std::fs::read_to_string(&file).unwrap().contains("\"ready\""), "the file names the gone rows");

    let (_ctx2, mut second) = started(&dir, &store);
    let after = second.inspect();
    let shown = file.display().to_string();
    assert_eq!((after.saved.keeping, after.saved.loaded_from.as_deref(), after.saved.note), (Keeping::Kept, Some(shown.as_str()), None));
    assert!(!after.state.panes.factory, "the rest of the state kept");
    assert_eq!(after.timeline.in_view.get(..2).map(<[String]>::to_vec), Some(vec!["main".to_owned(), "merged".to_owned()]));
    assert!(!after.timeline.in_view.iter().any(|k| k == "gates" || k == "ready"), "{:?}", after.timeline.in_view);
    assert!(after.timeline.in_view.iter().any(|k| k.starts_with("tickets/")), "{:?}", after.timeline.in_view);
    std::fs::remove_dir_all(dir).ok();
    std::fs::remove_dir_all(home).ok();
}
