//! A ticket's pages on GitHub from the shell (#17): right-click a bar on the timeline, or a chip
//! in the factory, and choose Open Issue or Open PR; the shell asks `ticket.urls` and hands the
//! page to egui's `open_url`, read here from the frame's output (no browser opens). The repository
//! has a GitHub `origin`, and the `gh` given doesn't exist: the pages come from the remote, the
//! timeline loads, and the status bar says why there are no titles.

use std::path::{Path, PathBuf};

use swarmception_engine::Github;
use swarmception_ui::Shell;

/// A repository whose tracking branch holds #7 (its PR #8), claimed then submitted, with its
/// `origin` on GitHub.
fn repo() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("swarmception-ui-links-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("tracking/mini-aa1111")).unwrap();
    std::fs::write(
        dir.join("tracking/mini-aa1111/work.jsonl"),
        [
            r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#,
            r#"{"event":"claim","issue":7,"mr":8,"t":"2026-10-31T03:01:00Z"}"#,
            r#"{"event":"work","issue":7,"t":"2026-10-31T03:02:00Z"}"#,
            r#"{"event":"submit","issue":7,"mr":8,"t":"2026-10-31T03:30:00Z"}"#,
        ]
        .join("\n"),
    )
    .unwrap();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    run(&["init", "-q", "-b", "tracking"]);
    run(&["add", "."]);
    run(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "events"]);
    run(&["remote", "add", "origin", "https://github.com/o/n.git"]);
    dir
}

/// What a frame drew and asked for: each text and where its middle is, and the pages opened.
struct Frame {
    texts: Vec<(String, egui::Pos2)>,
    opened: Vec<String>,
}

fn frame(shell: &mut Shell, ctx: &egui::Context, events: Vec<egui::Event>) -> Frame {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
    let texts = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.pos + text.galley.rect.center().to_vec2())),
            _ => None,
        })
        .collect();
    let opened = output
        .platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            egui::OutputCommand::OpenUrl(open) => Some(open.url.clone()),
            _ => None,
        })
        .collect();
    Frame { texts, opened }
}

fn loaded(dir: &Path) -> (egui::Context, Shell) {
    let ctx = egui::Context::default();
    let mut shell = Shell::with_github(&ctx, Some(dir), Github { gh: PathBuf::from("/no/such/gh"), cache_dir: None });
    for _ in 0..500 {
        frame(&mut shell, &ctx, Vec::new());
        if shell.has_timeline() && !shell.fetching_issues() && !shell.loading() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(shell.has_timeline(), "the tracking branch loaded");
    assert!(!shell.fetching_issues());
    frame(&mut shell, &ctx, Vec::new());
    (ctx, shell)
}

fn button(at: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton { pos: at, button, pressed, modifiers: egui::Modifiers::default() }
}

/// Right-clicks at `at`, then clicks the menu item `item`: the pages the frames opened.
fn choose(shell: &mut Shell, ctx: &egui::Context, at: egui::Pos2, item: &str) -> Vec<String> {
    use egui::PointerButton::{Primary, Secondary};
    frame(shell, ctx, vec![egui::Event::PointerMoved(at)]);
    frame(shell, ctx, vec![button(at, Secondary, true)]);
    let mut drawn = frame(shell, ctx, vec![button(at, Secondary, false)]);
    for _ in 0..3 {
        if drawn.texts.iter().any(|(t, _)| t == item) {
            break;
        }
        drawn = frame(shell, ctx, Vec::new());
    }
    let menu: Vec<&String> = drawn.texts.iter().map(|(t, _)| t).filter(|t| t.starts_with("Open ")).collect();
    let place = drawn.texts.iter().find(|(t, _)| t == item).map(|(_, p)| *p).unwrap_or_else(|| panic!("no {item} in the menu: {menu:?}"));
    let mut opened = Vec::new();
    for events in [vec![egui::Event::PointerMoved(place)], vec![button(place, Primary, true)], vec![button(place, Primary, false)], Vec::new()] {
        opened.extend(frame(shell, ctx, events).opened);
    }
    opened
}

#[test]
fn a_bars_context_menu_opens_its_issue_and_its_pr() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let at = shell.bar_point(7).expect("#7's bar is drawn");
    assert_eq!(choose(&mut shell, &ctx, at, "Open Issue #7"), ["https://github.com/o/n/issues/7"]);
    assert_eq!(choose(&mut shell, &ctx, at, "Open PR #8"), ["https://github.com/o/n/pull/8"]);
    assert_eq!(shell.opened(), ["https://github.com/o/n/issues/7", "https://github.com/o/n/pull/8"]);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_chips_context_menu_opens_its_pr_and_the_status_says_why_no_titles() {
    let dir = repo();
    let (ctx, mut shell) = loaded(&dir);
    let status = shell.engine().call::<swarmception_engine::repo_cmds::Status>(swarmception_engine::engine_cmds::NoParams {}).unwrap();
    assert_eq!(status.issues.github.as_deref(), Some("o/n"));
    assert!(status.issues.error.is_some_and(|e| e.contains("can't run gh")), "the status bar's warning says why there are no titles");
    assert_eq!(status.error, None, "the timeline loaded all the same");
    let chip = shell.factory().drawn().chips.iter().find(|(issue, _)| *issue == 7).map(|(_, rect)| rect.center()).expect("#7's chip");
    assert_eq!(choose(&mut shell, &ctx, chip, "Open PR #8"), ["https://github.com/o/n/pull/8"]);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn opening_a_link_with_no_github_is_an_error_not_a_page() {
    let ctx = egui::Context::default();
    let mut shell = Shell::with_github(&ctx, None, Github { gh: PathBuf::from("/no/such/gh"), cache_dir: None });
    shell.open_link(&ctx, swarmception_domain::timeline::Link::Issue(7));
    assert!(shell.opened().is_empty());
}
