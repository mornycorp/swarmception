//! How the panes share the window (#68, G18) and the Details' rows brought into view (G17), over
//! a real repository's tracking branch: twelve workers, each with a ticket, so the rows run past
//! the timeline's height.

use std::path::{Path, PathBuf};

use swarmception_domain::Grouping;
use swarmception_domain::details::Subject;
use swarmception_engine::rows_cmds::{Set as SetRows, SetParams as SetRowsParams};
use swarmception_ui::control::{Answer, Inspect};
use swarmception_ui::layout::{FOLDED_BAND, Folded};
use swarmception_ui::saved::{self, StateStore};
use swarmception_ui::{Shell, UiState};
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::state_cmds::Get as GetState;

mod support;

const WORKERS: usize = 12;

fn scratch(what: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-layout-{what}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Twelve workers on `mini`, worker-<kk> claiming, working on and submitting ticket #<10 + k>,
/// and a coordinator merging each: every ticket on the merges rows at the top too.
fn repo() -> PathBuf {
    let dir = scratch("repo");
    let mut coordinator = vec![r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#.to_owned()];
    coordinator.extend((1..=WORKERS).map(|k| format!(r#"{{"event":"merge","issue":{},"mr":{},"t":"2026-10-31T03:{:02}:00Z"}}"#, 10 + k, 100 + k, 40 + k)));
    std::fs::create_dir_all(dir.join("tracking/mini-co01")).unwrap();
    std::fs::write(dir.join("tracking/mini-co01/work.jsonl"), coordinator.join("\n") + "\n").unwrap();
    for k in 1..=WORKERS {
        let lines = [
            format!(r#"{{"event":"session.start","machine":"mini","name":"worker-{k:02}","role":"worker","t":"2026-10-31T03:00:00Z"}}"#),
            format!(r#"{{"event":"claim","issue":{},"t":"2026-10-31T03:{k:02}:00Z"}}"#, 10 + k),
            format!(r#"{{"event":"work","issue":{},"t":"2026-10-31T03:{k:02}:30Z"}}"#, 10 + k),
            format!(r#"{{"event":"submit","issue":{},"mr":{},"t":"2026-10-31T03:{:02}:00Z"}}"#, 10 + k, 100 + k, 20 + k),
        ];
        let instance = format!("mini-w{k:02}");
        std::fs::create_dir_all(dir.join("tracking").join(&instance)).unwrap();
        std::fs::write(dir.join("tracking").join(&instance).join("work.jsonl"), lines.join("\n") + "\n").unwrap();
    }
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(&dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    git(&["init", "-q", "-b", "tracking"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "events"]);
    dir
}

/// One frame of the shell at `size` points, with `events`: the texts it drew and where.
fn frame_with(shell: &mut Shell, ctx: &egui::Context, size: (f32, f32), events: Vec<egui::Event>) -> Vec<(String, egui::Rect)> {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.0, size.1));
    let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.visual_bounding_rect())),
            _ => None,
        })
        .collect()
}

fn frame(shell: &mut Shell, ctx: &egui::Context, size: (f32, f32)) -> Vec<(String, egui::Rect)> {
    frame_with(shell, ctx, size, Vec::new())
}

/// A click at `at`: the pointer moved there, pressed, released, a frame each.
fn click(shell: &mut Shell, ctx: &egui::Context, size: (f32, f32), at: egui::Pos2) {
    let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(shell, ctx, size, vec![egui::Event::PointerMoved(at)]);
    frame_with(shell, ctx, size, vec![button(true)]);
    frame_with(shell, ctx, size, vec![button(false)]);
    frame(shell, ctx, size);
}

fn loaded(dir: &Path, size: (f32, f32)) -> (egui::Context, Shell) {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, Some(dir));
    support::until("the tracking branch loaded", || {
        frame(&mut shell, &ctx, size);
        shell.has_timeline() && !shell.loading()
    });
    assert!(shell.has_timeline(), "the tracking branch loaded");
    for _ in 0..3 {
        frame(&mut shell, &ctx, size);
    }
    (ctx, shell)
}

fn set(shell: &mut Shell, params: &str) -> Inspect {
    match shell.answer("ui.set", params) {
        Answer::Now(Ok(text)) => serde_json::from_str(&text).unwrap(),
        other => panic!("{params}: {other:?}"),
    }
}

fn band(inspect: &Inspect, name: &str) -> swarmception_ui::layout::BandDrawn {
    inspect.layout.bands.iter().find(|b| b.name == name).cloned().unwrap_or_else(|| panic!("no {name}: {:?}", inspect.layout))
}

#[test]
fn the_timeline_keeps_its_least_with_every_pane_open() {
    let dir = repo();
    for size in [(1440.0, 900.0), (2560.0, 1440.0), (1280.0, 720.0)] {
        let (ctx, mut shell) = loaded(&dir, size);
        frame(&mut shell, &ctx, size);
        let inspect = shell.inspect();
        let layout = &inspect.layout;
        let names: Vec<&str> = layout.bands.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["factory"], "{size:?}: under the timeline (#128: no Gates band)");
        let timeline = layout.timeline.unwrap();
        assert!(timeline + 1 >= layout.timeline_least, "{size:?}: the timeline keeps its least: {layout:?}");
        assert!(layout.timeline_least > 200, "{size:?}: the merges rows and six more, measured: {layout:?}");
        // The merges rows and at least six ticket rows on screen.
        assert!(inspect.timeline.in_view.len() >= 8, "{size:?}: {:?}", inspect.timeline.in_view);
        assert_eq!(&inspect.timeline.in_view[..2], ["main", "merged"]);
        // The window's defaults: the factory open; as asked when there is room.
        let factory = band(&inspect, "factory");
        assert_eq!((factory.folded, factory.default_fold), (false, true), "{size:?}");
        if size.1 >= 1_400.0 {
            assert_eq!(factory.size, factory.wanted, "room for it: {layout:?}");
        }
        assert!(layout.details.as_ref().is_some_and(|d| !d.folded && d.size >= 300), "{size:?}: {:?}", layout.details);
        assert_eq!(inspect.state.folded, Folded::default(), "no choice made: the window decides");
    }
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_bands_give_way_and_come_back_as_the_window_changes() {
    let dir = repo();
    let (big, small) = ((1440.0, 1000.0), (1440.0, 640.0));
    let (ctx, mut shell) = loaded(&dir, big);
    // The person's size: a tall band.
    let mut state = shell.ui_state();
    state.sizes.factory = Some(560);
    state.folded = Folded { factory: Some(false), details: None };
    shell.set_ui_state(state);
    frame(&mut shell, &ctx, small);
    let inspect = shell.inspect();
    let factory = band(&inspect, "factory");
    assert!(factory.size < 560 && factory.size >= 140, "it gives way: {:?}", inspect.layout);
    assert!(inspect.layout.timeline.unwrap() + 1 >= inspect.layout.timeline_least, "{:?}", inspect.layout);
    assert_eq!(shell.ui_state().sizes.factory, Some(560), "what it asked is kept");
    frame(&mut shell, &ctx, (2560.0, 1440.0));
    let inspect = shell.inspect();
    assert_eq!(band(&inspect, "factory").size, 560, "room again: as asked");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_band_folds_from_its_title_and_the_fold_is_kept() {
    let dir = repo();
    let size = (1440.0, 900.0);
    let (ctx, mut shell) = loaded(&dir, size);
    let home = scratch("state");
    let store = StateStore::new(home.clone());
    let key = saved::repo_key(&dir);
    shell.keep_state(store.clone(), &key, None);
    let texts = frame(&mut shell, &ctx, size);
    // The Factory band's title, its chevron first: a click folds it to its header.
    let title = |texts: &[(String, egui::Rect)], name: &str| {
        texts.iter().find(|(t, _)| t.ends_with(&format!("  {name}")) && t.chars().count() > name.len() + 4).map(|(_, r)| r.center())
    };
    let at = title(&texts, "Factory").expect("the Factory's title drawn");
    let before = band(&shell.inspect(), "factory").size;
    click(&mut shell, &ctx, size, at);
    let inspect = shell.inspect();
    assert_eq!(inspect.state.folded.factory, Some(true), "the person's choice now");
    let factory = band(&inspect, "factory");
    assert_eq!((factory.size, factory.folded, factory.default_fold), (FOLDED_BAND as u32, true, false));
    assert!(inspect.layout.timeline.unwrap() >= inspect.layout.timeline_least + before - FOLDED_BAND as u32 - 1, "the timeline has its room");
    // Folded, its title opens it again.
    let texts = frame(&mut shell, &ctx, size);
    let at = title(&texts, "Factory").expect("the folded header's title");
    click(&mut shell, &ctx, size, at);
    assert_eq!(shell.inspect().state.folded.factory, Some(false));

    // Through the control channel: the Factory folded and the Details to a strip.
    let inspect = set(&mut shell, r#"{"folded": {"factory": true, "details": true}}"#);
    assert_eq!(inspect.state.folded, Folded { factory: Some(true), details: Some(true) });
    frame(&mut shell, &ctx, size);
    let inspect = shell.inspect();
    assert!(band(&inspect, "factory").folded && inspect.layout.details.as_ref().is_some_and(|d| d.folded && d.size <= 32), "{:?}", inspect.layout);
    assert!(matches!(shell.answer("ui.set", r#"{"folded": {"timeline": true}}"#), Answer::Now(Err(_))), "the timeline doesn't fold");
    assert!(matches!(shell.answer("ui.set", r#"{"folded": {"gates": true}}"#), Answer::Now(Err(_))), "no Gates band (#128)");

    // Kept across a restart.
    shell.save_state().unwrap();
    let state = store.load(&key).state.expect("saved");
    assert_eq!(state.ui.folded, Folded { factory: Some(true), details: Some(true) });
    let older: UiState = serde_json::from_str(r#"{"version":5,"panes":{"details":false}}"#).unwrap();
    assert_eq!(older.folded, Folded::default(), "an older state folds by the window");
    std::fs::remove_dir_all(dir).ok();
    std::fs::remove_dir_all(home).ok();
}

#[test]
fn a_details_subject_in_a_collapsed_group_opens_it_and_its_rows_come_into_view() {
    let dir = repo();
    let size = (1440.0, 900.0);
    let (ctx, mut shell) = loaded(&dir, size);
    shell.engine().call::<SetRows>(SetRowsParams { grouping: Some(Grouping::Workers), ..SetRowsParams::default() }).unwrap();
    for _ in 0..3 {
        frame(&mut shell, &ctx, size);
    }
    let in_view = |shell: &mut Shell| shell.inspect().timeline.in_view;
    let collapsed = |shell: &mut Shell| shell.timeline().widget().session().call::<GetState>(NoParams {}).unwrap().timeline.unwrap().collapsed;
    let last = format!("w/mini/worker-{WORKERS:02}");
    let row = format!("{last}/1");
    assert!(!in_view(&mut shell).contains(&row), "the last worker's row is below the pane: {:?}", in_view(&mut shell));

    // Scrolled out of view: the details' subject's row scrolled to.
    set(&mut shell, &format!(r#"{{"details": {{"ticket": {{"issue": {}}}}}}}"#, 10 + WORKERS));
    for _ in 0..4 {
        frame(&mut shell, &ctx, size);
    }
    assert!(in_view(&mut shell).contains(&row), "scrolled to: {:?}", in_view(&mut shell));

    // In a collapsed group: the group opens, and its row is scrolled to.
    let other = "w/mini/worker-01";
    let answer = shell.answer("timeline.tracks.collapse", &format!(r#"{{"key": "{other}"}}"#));
    assert!(matches!(answer, Answer::Now(Ok(_))), "{answer:?}");
    set(&mut shell, r#"{"details": {"ticket": {"issue": 13}}}"#);
    for _ in 0..4 {
        frame(&mut shell, &ctx, size);
    }
    assert!(collapsed(&mut shell).contains(&other.to_owned()), "another worker's group is left as it was");
    let answer = shell.answer("timeline.tracks.collapse", &format!(r#"{{"key": "{last}"}}"#));
    assert!(matches!(answer, Answer::Now(Ok(_))), "{answer:?}");
    frame(&mut shell, &ctx, size);
    assert!(collapsed(&mut shell).contains(&last));
    let inspect = set(&mut shell, &format!(r#"{{"details": {{"ticket": {{"issue": {}}}}}}}"#, 10 + WORKERS));
    assert_eq!(inspect.state.details.subject, Some(Subject::Ticket { issue: (10 + WORKERS) as u64 }));
    for _ in 0..4 {
        frame(&mut shell, &ctx, size);
    }
    assert!(!collapsed(&mut shell).contains(&last), "its group opened: {:?}", collapsed(&mut shell));
    assert!(in_view(&mut shell).contains(&row), "its row in view: {:?}", in_view(&mut shell));
    assert!(!shell.timeline().highlighted().is_empty(), "and its bars lit");
    std::fs::remove_dir_all(dir).ok();
}
