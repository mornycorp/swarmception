//! The factory's benches with dozens of workers (#94): each reads on its own line, never on top of
//! another; the workers off shift holding nothing fold into a line, and the rest scroll, the
//! bench holding a selected ticket brought into view. Each group in natural order, and the fold
//! opens on a click and closes again (#106).

use std::path::PathBuf;

use swarmception_domain::details::Subject;
use swarmception_domain::factory::BenchStatus;
use swarmception_ui::Shell;

mod support;

/// A tracking branch with 40 workers on one machine: `working` of them each working on its own
/// ticket (#101, #102, …), then `idle` on shift holding nothing, the rest off shift.
fn repo(working: usize, idle: usize) -> PathBuf {
    let working: Vec<usize> = (1..=working).collect();
    let idle: Vec<usize> = (working.len() + 1..=working.len() + idle).collect();
    repo_of(&working, &idle)
}

/// A tracking branch with 40 workers on one machine, `worker-1` to `worker-40`: those numbered in
/// `working` each working on its own ticket (#100 + n), those in `idle` on shift holding nothing,
/// the rest off shift.
fn repo_of(working: &[usize], idle: &[usize]) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "swarmception-benches-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    for n in 1..=40usize {
        let start = format!(
            r#"{{"event":"session.start","machine":"mini","name":"worker-{n}","model":"claude-opus-5-5","effort":"high","role":"worker","t":"2026-10-31T03:{:02}:00Z"}}"#,
            n % 60
        );
        let mut lines = vec![start];
        if working.contains(&n) {
            let issue = 100 + n;
            lines.push(format!(r#"{{"event":"claim","issue":{issue},"t":"2026-10-31T04:00:00Z"}}"#));
            lines.push(format!(r#"{{"event":"work","issue":{issue},"t":"2026-10-31T04:01:00Z"}}"#));
        } else if !idle.contains(&n) {
            lines.push(r#"{"event":"session.stop","t":"2026-10-31T04:02:00Z"}"#.to_owned());
        }
        let instance = format!("mini-{n:06x}");
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

fn frame(shell: &mut Shell, ctx: &egui::Context) {
    frame_with(shell, ctx, Vec::new());
}

fn frame_with(shell: &mut Shell, ctx: &egui::Context, events: Vec<egui::Event>) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
}

/// A click at `on`, and the frames after it.
fn click(shell: &mut Shell, ctx: &egui::Context, on: egui::Pos2) {
    let button = |pressed| egui::Event::PointerButton { pos: on, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(shell, ctx, vec![egui::Event::PointerMoved(on), button(true)]);
    frame_with(shell, ctx, vec![button(false)]);
    frame_with(shell, ctx, vec![egui::Event::PointerGone]);
    frame(shell, ctx);
    frame(shell, ctx);
}

fn keys(drawn: &[(String, Vec<u64>)]) -> Vec<&str> {
    drawn.iter().map(|(key, _)| key.as_str()).collect()
}

fn workers(numbers: &[usize]) -> Vec<String> {
    numbers.iter().map(|n| format!("mini/worker-{n}")).collect()
}

/// The shell over `dir`, the factory short under the timeline, as the owner saw it.
fn loaded(dir: &std::path::Path) -> (egui::Context, Shell) {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, Some(dir));
    let mut state = shell.ui_state();
    state.panes.details = false;
    state.panes.timeline = true;
    state.panes.factory = true;
    shell.set_ui_state(state);
    support::until("the tracking branch loaded", || {
        frame(&mut shell, &ctx);
        shell.has_timeline()
    });
    assert!(shell.has_timeline(), "the tracking branch loaded");
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    (ctx, shell)
}

/// No two of `cells` overlap, each tall enough for a line, all inside the floor.
fn assert_apart(shell: &Shell) {
    let drawn = shell.factory().drawn();
    let floor = shell.factory().floor().unwrap();
    assert_eq!(drawn.bench_cells.len(), drawn.benches.len());
    assert!(!drawn.bench_cells.is_empty());
    for (i, a) in drawn.bench_cells.iter().enumerate() {
        assert!(a.height() >= 26.0, "bench {} is {} high: too short to read", drawn.benches[i].0, a.height());
        assert!(floor.contains_rect(*a), "bench {} at {a:?} outside the floor {floor:?}", drawn.benches[i].0);
        for (j, b) in drawn.bench_cells.iter().enumerate().skip(i + 1) {
            let overlap = a.intersect(*b);
            assert!(overlap.width() <= 0.0 || overlap.height() <= 0.0, "{} {a:?} overlaps {} {b:?}", drawn.benches[i].0, drawn.benches[j].0);
        }
    }
}

#[test]
fn forty_benches_never_overlap_and_those_off_shift_fold_away() {
    let dir = repo(3, 7);
    let (_ctx, shell) = loaded(&dir);
    let scene = shell.factory().scene().cloned().unwrap();
    assert_eq!(scene.benches.len(), 40);
    let off: Vec<String> = scene.benches.iter().filter(|b| b.status == BenchStatus::Off).map(|b| b.key.clone()).collect();
    assert_eq!(off.len(), 30);

    assert_apart(&shell);
    let drawn = shell.factory().drawn();
    assert_eq!(drawn.benches_folded, off, "the 30 off shift fold into one line");
    assert_eq!(drawn.benches.len() + drawn.benches_hidden + drawn.benches_folded.len(), 40, "every bench drawn, scrolled away or folded");
    // The busy first: the three working are in view whatever else scrolls.
    let working: Vec<&str> = drawn.benches.iter().take(3).map(|(key, _)| key.as_str()).collect();
    assert_eq!(working, ["mini/worker-1", "mini/worker-2", "mini/worker-3"]);
    assert_eq!(drawn.benches[0].1, [101]);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn benches_that_dont_fit_scroll_to_a_selected_tickets() {
    // Forty working: nothing folds, so they scroll.
    let dir = repo(40, 0);
    let (ctx, mut shell) = loaded(&dir);
    assert_apart(&shell);
    let drawn = shell.factory().drawn().clone();
    assert!(drawn.benches_folded.is_empty());
    assert!(drawn.benches_hidden > 0, "forty don't fit in the short factory: {} drawn", drawn.benches.len());
    assert_eq!(drawn.benches.len() + drawn.benches_hidden, 40);

    // A ticket whose bench is out of view, selected: its bench scrolls into view, drawn selected.
    let scene = shell.factory().scene().cloned().unwrap();
    let hidden = scene.benches.iter().rev().find(|b| !drawn.benches.iter().any(|(key, _)| *key == b.key)).unwrap();
    let issue = hidden.crates[0].issue;
    shell.select(Subject::Ticket { issue });
    frame(&mut shell, &ctx);
    frame(&mut shell, &ctx);
    assert_apart(&shell);
    let drawn = shell.factory().drawn();
    assert!(
        drawn.benches.iter().any(|(key, tickets)| *key == hidden.key && tickets.contains(&issue)),
        "{} scrolled into view: {:?}",
        hidden.key,
        drawn.benches
    );
    assert!(drawn.selected.contains(&format!("bench #{issue}")), "{:?}", drawn.selected);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn each_group_of_benches_is_in_natural_order() {
    let dir = repo_of(&[21, 10, 2, 3], &[12, 4]);
    let (_ctx, shell) = loaded(&dir);
    let scene = shell.factory().scene().cloned().unwrap();
    let order: Vec<&str> = scene.benches.iter().take(12).map(|b| b.key.as_str()).collect();
    assert_eq!(order, workers(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]), "factory.at's benches: worker-2 before worker-10");
    let drawn = shell.factory().drawn();
    assert_eq!(keys(&drawn.benches)[..6], workers(&[2, 3, 10, 21, 4, 12]), "the busy, then the idle, each in natural order");
    let off: Vec<usize> = (1..=40).filter(|n| ![21, 10, 2, 3, 12, 4].contains(n)).collect();
    assert_eq!(drawn.benches_folded, workers(&off), "those off shift too");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_fold_opens_on_a_click_scrolls_with_the_rest_and_closes() {
    let dir = repo(3, 7);
    let (ctx, mut shell) = loaded(&dir);
    let before = shell.factory().drawn().clone();
    assert_eq!(before.benches_folded.len(), 30);
    assert!(before.benches_unfolded.is_empty());
    assert!(!shell.ui_state().factory.off_shift);

    // A click on the fold's line: their benches after the rest, the first of them scrolled into view.
    click(&mut shell, &ctx, before.fold_line.unwrap().center());
    assert!(shell.ui_state().factory.off_shift, "the click opened the fold");
    assert_apart(&shell);
    let open = shell.factory().drawn().clone();
    assert!(open.benches_folded.is_empty());
    assert_eq!(open.benches_unfolded, before.benches_folded, "every one of them, in order");
    assert_eq!(open.benches.len() + open.benches_hidden, 40, "all forty benches, in view or scrolled");
    assert!(keys(&open.benches).contains(&"mini/worker-11"), "the first off shift in view: {:?}", keys(&open.benches));
    assert!(open.fold_line.is_some(), "its line stays, to close it");
    let inspect = shell.inspect();
    assert_eq!((inspect.version, inspect.factory.drawn.benches_unfolded.len()), (swarmception_ui::control::INSPECT_VERSION, 30));

    // The wheel scrolls them with the rest: back to the top, the busy first.
    let over = open.bench_cells[0].center();
    for _ in 0..20 {
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 400.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
        };
        frame_with(&mut shell, &ctx, vec![egui::Event::PointerMoved(over), wheel]);
    }
    for _ in 0..30 {
        frame(&mut shell, &ctx);
    }
    let top = shell.factory().drawn().clone();
    assert_eq!(keys(&top.benches)[..3], workers(&[1, 2, 3]), "scrolled back to the top");
    assert!(top.benches_hidden > 0);

    // A second click closes it again.
    click(&mut shell, &ctx, top.fold_line.unwrap().center());
    assert!(!shell.ui_state().factory.off_shift);
    let closed = shell.factory().drawn();
    assert_eq!(closed.benches_folded, before.benches_folded);
    assert!(closed.benches_unfolded.is_empty());
    assert_eq!(keys(&closed.benches), keys(&before.benches));

    // `ui.set` opens it as the click does, and folds it again.
    for (open, unfolded) in [(true, 30), (false, 0)] {
        let set = format!(r#"{{"offShift": {open}}}"#);
        assert!(matches!(shell.answer("ui.set", &set), swarmception_ui::control::Answer::Now(Ok(_))), "{set}");
        frame(&mut shell, &ctx);
        frame(&mut shell, &ctx);
        assert_eq!((shell.ui_state().factory.off_shift, shell.factory().drawn().benches_unfolded.len()), (open, unfolded));
    }
    std::fs::remove_dir_all(dir).ok();
}
