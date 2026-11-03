//! Every pane lands on egui's GUI rounding grid: in a debug build egui outlines a widget whose
//! rect isn't (`style.debug.show_unaligned`), an orange line and the word "Unaligned", which
//! means the widget may draw blurry. A frame of every pane, at a width that isn't a whole
//! number of points, paints none.

use std::path::PathBuf;

use egui::epaint::Shape;
use swarmception_ui::Shell;

/// Worker-1 works on #7 (PR #8) from 03:01, gate-8 merges it at 03:28; nothing then until
/// worker-2 claims #9 (PR #10) at 09:01, gate-10 merging it at 09:28.
fn repo() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-aligned-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [
        (
            "mini-aa1111",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
                r#"{"event":"work","issue":7,"t":"2026-10-31T03:02:00Z"}"#,
                r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:20:00Z"}"#,
                r#"{"event":"session.stop","t":"2026-10-31T03:30:00Z"}"#,
            ],
        ),
        (
            "mini-bb2222",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"worker-2","role":"worker","t":"2026-10-31T09:00:00Z"}"#,
                r#"{"event":"claim","issue":9,"mr":10,"t":"2026-10-31T09:01:00Z"}"#,
                r#"{"event":"work","issue":9,"t":"2026-10-31T09:02:00Z"}"#,
                r#"{"event":"submit","issue":9,"mr":10,"t":"2026-10-31T09:20:00Z"}"#,
            ],
        ),
        (
            "mini-cc3333",
            vec![
                r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T03:00:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #8 (#7); kind=smoke","sub":"gate-8","t":"2026-10-31T03:25:00Z"}"#,
                r#"{"event":"merge","issue":7,"mr":8,"sub":"gate-8","t":"2026-10-31T03:28:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0; merged #8","sub":"gate-8","t":"2026-10-31T03:29:00Z"}"#,
                r#"{"event":"sub.start","name":"gate","note":"local gate: #10 (#9); kind=smoke","sub":"gate-10","t":"2026-10-31T09:25:00Z"}"#,
                r#"{"event":"merge","issue":9,"mr":10,"sub":"gate-10","t":"2026-10-31T09:28:00Z"}"#,
                r#"{"event":"sub.stop","note":"rc=0; merged #10","sub":"gate-10","t":"2026-10-31T09:29:00Z"}"#,
            ],
        ),
    ];
    for (instance, lines) in files {
        std::fs::create_dir_all(dir.join("tracking").join(instance)).unwrap();
        std::fs::write(dir.join("tracking").join(instance).join("work.jsonl"), lines.join("\n") + "\n").unwrap();
    }
    let git = |at: &str, args: &[&str]| {
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
    let early = "2026-10-31T02:00:00Z";
    git(early, &["init", "-q", "-b", "tracking"]);
    git(early, &["add", "."]);
    git(early, &["commit", "-q", "-m", "events"]);
    git(early, &["checkout", "-q", "--orphan", "main"]);
    git(early, &["rm", "-rqf", "."]);
    git(early, &["commit", "-q", "--allow-empty", "-m", "start"]);
    for (pr, at) in [(8, "2026-10-31T03:28:00Z"), (10, "2026-10-31T09:28:00Z")] {
        git(early, &["checkout", "-q", "-b", &format!("pr-{pr}"), "main"]);
        git(at, &["commit", "-q", "--allow-empty", "-m", "work"]);
        git(early, &["checkout", "-q", "main"]);
        git(at, &["merge", "-q", "--no-ff", "-m", &format!("Merge pull request #{pr} from mornycorp/pr-{pr}"), &format!("pr-{pr}")]);
    }
    git(early, &["checkout", "-q", "tracking"]);
    dir
}

/// The "Unaligned" markers a frame painted, by where each one starts.
fn unaligned(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    fn walk(shape: &Shape, out: &mut Vec<String>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
            Shape::Text(text) if text.galley.text() == "Unaligned" => out.push(format!("{:?}", text.pos)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|s| walk(&s.shape, &mut out));
    out
}

fn frame(shell: &mut Shell, ctx: &egui::Context, size: egui::Vec2, events: Vec<egui::Event>) -> Vec<String> {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
    unaligned(&output.shapes)
}

/// Every pane shown, the timeline loaded, then panned, zoomed and hovered by fractional amounts:
/// no marker, at a whole size and a fractional one.
#[test]
fn every_pane_is_on_the_rounding_grid() {
    let dir = repo();
    for size in [egui::vec2(1400.0, 900.0), egui::vec2(1283.5, 811.25)] {
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, Some(&dir));
        ctx.global_style_mut(|s| s.debug.show_unaligned = true);
        // As eframe's window does: the timeline's bars are accessibility nodes then.
        ctx.enable_accesskit();
        for _ in 0..500 {
            frame(&mut shell, &ctx, size, Vec::new());
            if shell.has_timeline() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(shell.has_timeline(), "the tracking branch loaded");
        let mut state = shell.ui_state();
        state.panes.workers = true;
        shell.set_ui_state(state);
        let mut markers = std::collections::BTreeSet::new();
        let mut run = |what: &str, events: Vec<egui::Event>| {
            for found in frame(&mut shell, &ctx, size, events) {
                markers.insert(format!("{what}: {found}"));
            }
        };
        for _ in 0..3 {
            run("still", Vec::new());
        }
        let (mut x, mut y) = (13.37_f32, 17.29_f32);
        while y < size.y {
            while x < size.x {
                let pos = egui::pos2(x, y);
                run("hover", vec![egui::Event::PointerMoved(pos)]);
                run(
                    "wheel",
                    vec![egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(-7.3, -5.7),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Unknown,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
                x += 97.3;
            }
            (x, y) = (13.37, y + 61.7);
        }
        run("still", vec![egui::Event::PointerGone]);
        assert!(markers.is_empty(), "{size:?}: {} unaligned: {markers:#?}", markers.len());
    }
}
