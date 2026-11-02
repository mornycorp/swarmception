//! A live reload (#54) over a clone of a local bare remote whose swarm logs events between
//! checks: the timeline's window follows the end when it was there and stays put otherwise; the
//! selection, the playhead, the highlight and the details' subject are kept.

use std::path::{Path, PathBuf};
use std::time::Duration;

use swarmception_domain::details::Subject;
use swarmception_engine::factory_cmds::{TicketBars, TicketParams};
use swarmception_engine::live::LiveStatus;
use swarmception_engine::{Github, Session};
use swarmception_model::Timestamp;
use swarmception_ui::control::{Answer, Inspect};
use swarmception_ui::{Shell, TimelinePane};
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::state_cmds::{Get, Set, SetParams};
use timeline_engine::wire::SpanWire;

fn git(dir: &Path, args: &[&str]) {
    let mut all = vec!["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"];
    all.extend_from_slice(args);
    let out = std::process::Command::new("git").current_dir(dir).args(&all).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

const START: i64 = 1_793_416_800; // 2026-10-31T03:00:00Z

fn line(seconds: i64, rest: &str) -> String {
    format!("{{{rest},\"t\":\"{}\"}}\n", Timestamp(START + seconds))
}

/// The swarm's repository, its bare remote and the clone the app opens: worker-1 claimed #7 a
/// minute in and began work on it at two.
struct Swarm {
    work: PathBuf,
    remote: PathBuf,
    clone: PathBuf,
}

impl Swarm {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let work = std::env::temp_dir().join(format!("swarmception-ui-live-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let swarm = Self { remote: work.with_extension("git"), clone: work.with_extension("clone"), work };
        for dir in [&swarm.remote, &swarm.clone] {
            let _ = std::fs::remove_dir_all(dir);
        }
        std::fs::create_dir_all(swarm.work.join("tracking/mini-aa1111")).unwrap();
        let events = [
            line(0, r#""event":"session.start","machine":"mini","name":"worker-1","role":"worker""#),
            line(60, r#""event":"claim","issue":7,"mr":8"#),
            line(120, r#""event":"work","issue":7"#),
        ];
        std::fs::write(swarm.file(), events.concat()).unwrap();
        git(&swarm.work, &["init", "-q", "-b", "tracking"]);
        git(&swarm.work, &["add", "."]);
        git(&swarm.work, &["commit", "-q", "-m", "events"]);
        git(&swarm.work, &["clone", "-q", "--bare", swarm.work.to_str().unwrap(), swarm.remote.to_str().unwrap()]);
        git(&swarm.work, &["clone", "-q", "--no-checkout", swarm.remote.to_str().unwrap(), swarm.clone.to_str().unwrap()]);
        swarm
    }

    fn file(&self) -> PathBuf {
        self.work.join("tracking/mini-aa1111/work.jsonl")
    }

    /// The swarm logs `rest` at `seconds` in and pushes.
    fn log(&self, seconds: i64, rest: &str) {
        let mut text = std::fs::read_to_string(self.file()).unwrap();
        text.push_str(&line(seconds, rest));
        std::fs::write(self.file(), text).unwrap();
        git(&self.work, &["commit", "-q", "-am", "an event"]);
        git(&self.work, &["push", "-q", self.remote.to_str().unwrap(), "tracking"]);
    }

    fn github(&self) -> Github {
        Github { gh: self.work.join("no-gh"), cache_dir: None }
    }
}

impl Drop for Swarm {
    fn drop(&mut self) {
        for dir in [&self.work, &self.remote, &self.clone] {
            std::fs::remove_dir_all(dir).ok();
        }
    }
}

/// The widget's extent and window.
fn view(pane: &mut TimelinePane) -> (SpanWire, SpanWire) {
    let state = pane.widget().session().call::<Get>(NoParams {}).unwrap().timeline.unwrap();
    (state.extent, state.window)
}

fn check(engine: &mut Session) -> LiveStatus {
    serde_json::from_str(&engine.execute("live.check", "").unwrap()).unwrap()
}

#[test]
fn a_reload_follows_the_end_when_the_window_was_there_and_stays_put_otherwise() {
    let swarm = Swarm::new();
    let mut engine = Session::new();
    engine.set_github(swarm.github());
    let path = serde_json::to_string(&swarm.clone.display().to_string()).unwrap();
    engine.execute("repo.open", &format!("{{\"path\":{path}}}")).unwrap();
    let mut pane = TimelinePane::default();
    pane.sync(&mut engine).unwrap();
    let (extent, window) = view(&mut pane);
    assert_eq!(window, extent, "the first load shows the whole swarm");

    // The whole swarm shown: after new events, the whole swarm still.
    swarm.log(3_600, r#""event":"submit","issue":7,"mr":8"#);
    assert_eq!(check(&mut engine).reloads, 1);
    pane.sync(&mut engine).unwrap();
    let (grown, window) = view(&mut pane);
    assert!(grown.end > extent.end, "the new event is further on");
    assert_eq!(window, grown);
    assert!(pane.load_time().is_some());

    // The last quarter shown: it follows the new end at its width.
    let width = (grown.end - grown.start) / 4;
    let tail = SpanWire { start: grown.end - width, end: grown.end };
    pane.widget().session().call::<Set>(SetParams { window: Some(tail), ..SetParams::default() }).unwrap();
    swarm.log(7_200, r#""event":"claim","issue":9"#);
    check(&mut engine);
    pane.sync(&mut engine).unwrap();
    let (later, window) = view(&mut pane);
    assert!(later.end > grown.end);
    assert_eq!((window.end, window.end - window.start), (later.end, width), "at the new end, as wide");

    // A window in the middle stays where it was, and so does the highlight's ticket.
    let middle = SpanWire { start: later.start + width / 2, end: later.start + width };
    pane.widget().session().call::<Set>(SetParams { window: Some(middle), ..SetParams::default() }).unwrap();
    assert!(pane.light_ticket(&mut engine, Some(7)));
    swarm.log(10_800, r#""event":"work","issue":9"#);
    check(&mut engine);
    pane.sync(&mut engine).unwrap();
    assert_eq!(view(&mut pane).1, middle, "put");
    assert!(pane.light_ticket(&mut engine, Some(7)), "new rows: lit again");
    let bars = engine.call::<TicketBars>(TicketParams { issue: 7 }).unwrap().bars;
    assert_eq!(pane.highlighted().values().map(Vec::len).sum::<usize>(), bars.len());

    // A check that found nothing new leaves the widget alone.
    check(&mut engine);
    pane.sync(&mut engine).unwrap();
    assert_eq!(view(&mut pane).1, middle);
}

/// One frame of the shell, 1400 × 900 points.
fn frame(shell: &mut Shell, ctx: &egui::Context) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
    let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| shell.ui(ui));
    output.textures_delta.clear();
}

/// Frames until `done`, or ten seconds.
fn frames_until(shell: &mut Shell, ctx: &egui::Context, done: impl Fn(&mut Shell) -> bool) {
    for _ in 0..1_000 {
        frame(shell, ctx);
        if done(shell) {
            frame(shell, ctx);
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("never happened");
}

fn answer(shell: &mut Shell, command: &str, params: &str) -> String {
    match shell.answer(command, params) {
        Answer::Now(result) => result.unwrap_or_else(|e| panic!("{command}: {e:?}")),
        Answer::Screenshot(_) => panic!("a screenshot"),
    }
}

#[test]
fn a_live_reload_keeps_the_selection_the_playhead_and_the_details() {
    let swarm = Swarm::new();
    let ctx = egui::Context::default();
    let mut shell = Shell::with_github(&ctx, Some(&swarm.clone), swarm.github());
    frames_until(&mut shell, &ctx, |shell| shell.has_timeline() && !shell.loading());
    assert!(!shell.live().on, "an old swarm: not live by default");
    assert!(!shell.menu_state().live);

    // #7 selected (its bars on the timeline, its journey traced, its details), the factory at
    // the moment work began.
    let at = Timestamp(START + 120);
    answer(&mut shell, "ui.set", &format!(r#"{{"ticket": 7, "at": "{at}"}}"#));
    frames_until(&mut shell, &ctx, |shell| shell.ui_state().details.subject.is_some());
    let before: Inspect = serde_json::from_str(&answer(&mut shell, "ui.inspect", "")).unwrap();
    assert_eq!(before.state.details.subject, Some(Subject::Ticket { issue: 7 }));
    assert!(!before.timeline.selected.is_empty());
    assert_eq!(before.timeline.playhead, Some(at));

    // A ticket claimed earlier than #7 arrives late: it takes #7's row, and the timeline's
    // items shift under the playhead.
    swarm.log(30, r#""event":"claim","issue":5,"mr":6"#);
    let generation = shell.engine().revision().0;
    let checking: LiveStatus = serde_json::from_str(&answer(&mut shell, "live.check", "")).unwrap();
    assert!(checking.checking, "in the background");
    assert!(!shell.loading(), "a live check isn't a read the toolbar waits on");
    frames_until(&mut shell, &ctx, |shell| shell.engine().revision().0 != generation && shell.live().checks == 1);

    let after: Inspect = serde_json::from_str(&answer(&mut shell, "ui.inspect", "")).unwrap();
    let bars = shell.engine().call::<TicketBars>(TicketParams { issue: 7 }).unwrap().bars;
    let mut want = std::collections::BTreeMap::<String, Vec<usize>>::new();
    for bar in bars {
        want.entry(bar.track).or_default().push(bar.range);
    }
    assert_eq!(after.timeline.selected, want, "#7's bars, in the new rows");
    assert_ne!(after.timeline.selected, before.timeline.selected, "which moved");
    assert_eq!(after.timeline.playhead, Some(at), "the playhead at the same time");
    assert_ne!(after.timeline.playhead_item, before.timeline.playhead_item, "on another item");
    assert_eq!(after.state.details.subject, Some(Subject::Ticket { issue: 7 }));
    assert_eq!(after.state.factory.ticket, Some(7));
    // The details' subject keeps the highlight (#55), on its bars in the new rows.
    assert_eq!(before.timeline.lit, swarmception_ui::Lit::Details);
    assert_eq!(after.timeline.lit, swarmception_ui::Lit::Details);
    assert_eq!(after.timeline.highlighted, want, "lit on the new rows");
    assert_eq!(after.error, None);
    assert_eq!(shell.live().reloads, 1);
    assert_eq!(shell.live().cost.and_then(|c| c.new_events), Some(1));

    // The toggle is `live.set`, the menu's mark follows; turned on, it checks at once.
    shell.run_menu(&ctx, swarmception_ui::MenuAction::Live);
    frames_until(&mut shell, &ctx, |shell| shell.live().on && shell.live().checks == 2);
    assert!(shell.menu_state().live);
    assert_eq!(shell.live().reloads, 1, "nothing new: no reload");
}
