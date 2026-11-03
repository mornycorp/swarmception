//! The factory's benches with dozens of workers (#94): each reads on its own line, never on top of
//! another; the workers off shift holding nothing fold into a line, and the rest scroll, the
//! bench holding a selected ticket brought into view.

use std::path::PathBuf;

use swarmception_domain::details::Subject;
use swarmception_domain::factory::BenchStatus;
use swarmception_ui::Shell;

/// A tracking branch with 40 workers on one machine: `working` of them each working on its own
/// ticket (#101, #102, …), then `idle` on shift holding nothing, the rest off shift.
fn repo(working: usize, idle: usize) -> PathBuf {
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
        if n <= working {
            let issue = 100 + n;
            lines.push(format!(r#"{{"event":"claim","issue":{issue},"t":"2026-10-31T04:00:00Z"}}"#));
            lines.push(format!(r#"{{"event":"work","issue":{issue},"t":"2026-10-31T04:01:00Z"}}"#));
        } else if n > working + idle {
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
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
}

/// The shell over `dir`, the factory short under the timeline and the gates, as the owner saw it.
fn loaded(dir: &std::path::Path) -> (egui::Context, Shell) {
    let ctx = egui::Context::default();
    let mut shell = Shell::new(&ctx, Some(dir));
    let mut state = shell.ui_state();
    state.panes.details = false;
    state.panes.gates = true;
    state.panes.timeline = true;
    state.panes.factory = true;
    shell.set_ui_state(state);
    for _ in 0..500 {
        frame(&mut shell, &ctx);
        if shell.has_timeline() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
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
