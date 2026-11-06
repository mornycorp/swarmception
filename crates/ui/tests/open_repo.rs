//! Opening a repository from the window (#69, G8), across the real boundary: a headless shell
//! over a real repository opens another through File ▸ Open… (a folder picker that answers at
//! once, as the app's AppKit panel would), File ▸ Open Recent and the control channel's
//! `repo.open`; each repository keeps its own saved state, the recent list is kept app-wide, and a
//! folder that isn't a swarm's clone is said in the window while the open one stays.

use std::path::{Path, PathBuf};

use swarmception_domain::Grouping;
use swarmception_engine::Github;
use swarmception_engine::engine_cmds::NoParams;
use swarmception_engine::live_cmds::{Get as GetLive, SetParams as SetLiveParams};
use swarmception_ui::control::{Answer, Inspect};
use swarmception_ui::saved::{self, StateStore};
use swarmception_ui::{MenuAction, Picked, Shell};

mod support;

fn scratch(what: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("worker29-open-{what}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let canonical = std::fs::canonicalize(&dir).unwrap();
    // Match Git's spelling, which is what the opened repository reports.
    #[cfg(windows)]
    let canonical = PathBuf::from(canonical.to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/"));
    canonical
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

/// A swarm's clone whose worker claimed `issue`.
fn swarm(what: &str, issue: u64) -> PathBuf {
    let dir = scratch(what);
    std::fs::create_dir_all(dir.join("tracking/mini-aa1111")).unwrap();
    let lines = format!(
        "{}\n{{\"event\":\"claim\",\"issue\":{issue},\"mr\":8,\"t\":\"2026-10-31T03:01:00Z\"}}\n{{\"event\":\"submit\",\"issue\":{issue},\"mr\":8,\"t\":\"2026-10-31T03:30:00Z\"}}\n",
        r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#
    );
    std::fs::write(dir.join("tracking/mini-aa1111/work.jsonl"), lines).unwrap();
    git(&dir, &["init", "-q", "-b", "tracking"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "events"]);
    dir
}

/// One frame of the shell, 1400 × 900 points: the text it drew.
fn frame(shell: &mut Shell, ctx: &egui::Context) -> Vec<String> {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), time: Some(0.0), ..Default::default() };
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

/// Frames until the read has landed and the timeline has drawn it.
fn settle(shell: &mut Shell, ctx: &egui::Context) {
    support::until("the repository loaded", || {
        frame(shell, ctx);
        shell.has_timeline() && !shell.fetching_issues() && !shell.loading()
    });
}

fn inspect(shell: &mut Shell) -> Inspect {
    match shell.answer("ui.inspect", "") {
        Answer::Now(Ok(text)) => serde_json::from_str(&text).unwrap(),
        other => panic!("{other:?}"),
    }
}

/// A shell over `dir` keeping its state in `store`, loaded.
fn started(dir: &Path, store: &StateStore) -> (egui::Context, Shell) {
    let ctx = egui::Context::default();
    let mut shell = Shell::with_github(&ctx, Some(dir), Github { gh: PathBuf::from("/no/such/gh"), cache_dir: None });
    let key = saved::repo_key(dir);
    let loaded = store.load(&key);
    shell.keep_state(store.clone(), &key, Some(&loaded));
    settle(&mut shell, &ctx);
    (ctx, shell)
}

#[test]
fn open_switches_repositories_each_with_its_own_state() {
    let (a, b) = (swarm("a", 7), swarm("b", 9));
    let state = scratch("state");
    let store = StateStore::new(state.clone());
    let (ctx, mut shell) = started(&a, &store);
    // a's own choices: by worker, Live chosen on.
    shell.run_menu(&ctx, MenuAction::Grouping(Grouping::Workers));
    shell.set_live(SetLiveParams { on: Some(true), interval_seconds: Some(60) });
    shell.select(swarmception_domain::details::Subject::Ticket { issue: 7 });

    // File ▸ Open…: the picker starts in a's parent and answers b.
    let asked = std::rc::Rc::new(std::cell::RefCell::new(None));
    let seen = std::rc::Rc::clone(&asked);
    let answer = b.clone();
    shell.set_folder_picker(Box::new(move |start| {
        *seen.borrow_mut() = start.map(Path::to_path_buf);
        Picked::Folder(answer.clone())
    }));
    shell.run_menu(&ctx, MenuAction::Open);
    assert_eq!(asked.borrow().as_deref(), a.parent());
    settle(&mut shell, &ctx);
    let now = inspect(&mut shell);
    assert_eq!(now.repo.as_deref(), Some(b.display().to_string().as_str()));
    assert_eq!(now.saved.file, Some(store.file(&b).display().to_string()), "b's state is kept in b's own file");
    assert_eq!(now.state.details.subject, None, "a's details aren't b's");
    // The old watch stopped; b's is its default (an old swarm: off).
    let live = shell.engine().call::<GetLive>(NoParams {}).unwrap();
    assert!(!live.on && live.default, "{live:?}");
    // a's state was written as it was left.
    let kept_a = store.load(&a).state.unwrap();
    assert_eq!((kept_a.rows.grouping, kept_a.live.on), (Grouping::Workers, Some(true)));
    // The recent list, newest first, kept app-wide.
    assert_eq!(shell.recent().iter().map(|r| PathBuf::from(&r.path)).collect::<Vec<_>>(), [b.clone(), a.clone()]);
    assert_eq!(store.load_recent(), [b.clone(), a.clone()]);
    assert_eq!(shell.menu_state().recent, 2);
    let labels = inspect(&mut shell).open.recent;
    assert!(labels[0].starts_with(&format!("{} — ", b.file_name().unwrap().to_string_lossy())), "{labels:?}");

    // b's own choice, then Open Recent ▸ a: a's state comes back, Live and all.
    shell.run_menu(&ctx, MenuAction::Grouping(Grouping::Flat));
    shell.run_menu(&ctx, MenuAction::OpenRecent(1));
    settle(&mut shell, &ctx);
    let back = inspect(&mut shell);
    assert_eq!(back.repo.as_deref(), Some(a.display().to_string().as_str()));
    assert_eq!(back.state.details.subject, Some(swarmception_domain::details::Subject::Ticket { issue: 7 }));
    assert_eq!(shell.menu_state().grouping, Grouping::Workers);
    let live = shell.engine().call::<GetLive>(NoParams {}).unwrap();
    assert!(live.on && !live.default && live.interval_seconds == 60, "a's Live as chosen: {live:?}");
    assert_eq!(store.load_recent(), [a.clone(), b.clone()]);

    // Clear Menu: empty, in the file too; a stays open.
    shell.run_menu(&ctx, MenuAction::ClearRecent);
    frame(&mut shell, &ctx);
    assert!(shell.recent().is_empty() && store.load_recent().is_empty());
    assert!(!shell.menu_state().enabled(MenuAction::ClearRecent));
    assert_eq!(inspect(&mut shell).repo.as_deref(), Some(a.display().to_string().as_str()));
    for dir in [a, b, state] {
        std::fs::remove_dir_all(dir).ok();
    }
}

#[test]
fn a_refused_folder_is_said_in_the_window_and_the_open_repository_stays() {
    let a = swarm("stay", 7);
    let plain = scratch("plain");
    let state = scratch("refused-state");
    let store = StateStore::new(state.clone());
    let (ctx, mut shell) = started(&a, &store);
    let before = inspect(&mut shell);

    // Through the control channel, as File ▸ Open… would.
    let params = serde_json::json!({ "path": plain.display().to_string() }).to_string();
    assert!(matches!(shell.answer("repo.open", &params), Answer::Now(Err(_))));
    // A new window sizes itself on its first frame, unseen: the second draws it.
    frame(&mut shell, &ctx);
    let drawn = frame(&mut shell, &ctx).join("\n");
    assert!(drawn.contains("Can't open that folder"), "{drawn}");
    assert!(drawn.contains("is not in a git repository"), "{drawn}");
    assert!(drawn.contains(&format!("{} stays open.", a.display())), "{drawn}");
    let now = inspect(&mut shell);
    assert_eq!((now.repo, now.timeline.has_data), (before.repo, true), "the open repository stays, drawn");
    let refused = now.open.refused.unwrap();
    assert_eq!(refused.path, plain.display().to_string());
    assert_eq!(store.load_recent(), std::slice::from_ref(&a), "a refused folder isn't remembered");

    // A missing folder from Open Recent, through the picker: refused the same way.
    shell.set_folder_picker(Box::new(|_| Picked::Folder(PathBuf::from("/no/such/swarm"))));
    shell.run_menu(&ctx, MenuAction::Open);
    assert!(shell.refused().is_some_and(|r| r.why.contains("no such folder")), "{:?}", shell.refused());
    // Cancelled: nothing more is said or changed.
    shell.set_folder_picker(Box::new(|_| Picked::Cancelled));
    shell.run_menu(&ctx, MenuAction::Open);
    assert_eq!(inspect(&mut shell).repo.as_deref(), Some(a.display().to_string().as_str()));
    for dir in [a, plain, state] {
        std::fs::remove_dir_all(dir).ok();
    }
}

#[test]
fn an_agents_run_keeps_no_recent_list_and_a_kept_one_comes_back() {
    let a = swarm("agent", 7);
    let ctx = egui::Context::default();
    // No state kept (an agent's run): the list lives in the session only.
    let mut shell = Shell::with_github(&ctx, Some(&a), Github { gh: PathBuf::from("/no/such/gh"), cache_dir: None });
    settle(&mut shell, &ctx);
    assert_eq!(shell.recent().len(), 1);
    assert_eq!(inspect(&mut shell).open.recent_file, None);

    // A kept list comes back after the repository opened at the start.
    let state = scratch("kept-recent");
    let store = StateStore::new(state.clone());
    store.save_recent(&["/no/such/older".to_owned(), a.display().to_string()]).unwrap();
    let (_ctx, mut shell) = started(&a, &store);
    let recent: Vec<String> = shell.recent().iter().map(|r| r.path.clone()).collect();
    assert_eq!(recent, [a.display().to_string(), "/no/such/older".to_owned()]);
    assert!(shell.recent()[1].missing);
    assert_eq!(inspect(&mut shell).open.recent_file, Some(state.join("recent.json").display().to_string()));
    for dir in [a, state] {
        std::fs::remove_dir_all(dir).ok();
    }
}
