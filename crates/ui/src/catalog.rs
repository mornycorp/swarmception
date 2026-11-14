//! Every word a person reads in the window (#162, swarm-core's docs/app-architecture.md › Colours
//! from the theme, words from the catalog): drawing code writes no word; it takes each from here,
//! a constant, or a function of its values for a text with values in it. A rewording or a second
//! language then changes this file alone, and a test can ask what a view says. The words of the
//! data (a ticket's title, a worker's name, a command's label) come from the data as they are.
//! A test holds the drawing modules to it (`catalog::tests`), each to a count of the literals it
//! still writes (ids, keys and patterns a person doesn't read) that only goes down.
use std::fmt::Display;

// ---- lib ------------------------------------------------------------------------------------

pub const OPEN_THE_DETAILS: &str = "Open the Details";
pub const DETAILS: &str = "details";
pub const NO_PANE_IS_SHOWN_VIEW_TIMELINE: &str = "No pane is shown: View ▸ Timeline or Factory shows one.";
pub const FILE: &str = "File";
pub const VIEW: &str = "View";
pub const TICKETS: &str = "Tickets";
pub const RELOAD_THE_TRACKING_BRANCH: &str = "Reload the tracking branch";
pub const FETCH_THE_REMOTE_S_TRACKING_BRANCH: &str = "Fetch the remote's tracking branch, then reload";
pub const TICKETS_PACKED_ONTO_SHARED_ROWS_IN: &str = "Tickets packed onto shared rows, in the order they began";
pub const TICKETS_BY_WORKER_MACHINE_WORKER_ITS: &str = "Tickets by worker: machine / worker / its rows";
pub const COLLAPSE_LEAVE_OUT_ROWS_WITH_NO: &str = "Collapse: leave out rows with no events at all";
pub const COLLAPSE_TO_THE_WINDOW_LEAVE_OUT: &str = "Collapse to the window: leave out rows with nothing in the visible window, as it settles";
pub fn fold_quiet_time_each_stretch(minutes: &dyn Display) -> String {
    format!("Fold quiet time: each stretch where nothing happened for over {minutes} min drawn as a narrow marker; the ruler jumps over it")
}
pub fn fold_quiet_time(a0: &dyn Display) -> String {
    format!("{}  Fold quiet time", a0)
}
pub fn opening(a0: &dyn Display, path: &dyn Display) -> String {
    format!("{}  opening {path}…", a0)
}
pub fn reading_the_tracking_branch(a0: &dyn Display) -> String {
    format!("{}  reading the tracking branch…", a0)
}
pub const NO_REPOSITORY_FILE_OPEN_O_OPENS: &str = "No repository: File ▸ Open… (⌘O) opens a swarm's clone";
pub fn tickets_merged_merges_to_main(a0: &dyn Display, a1: &dyn Display, a2: &dyn Display, a3: &dyn Display, a4: &dyn Display, a5: &dyn Display) -> String {
    format!("{}  {} tickets · {} merged · {} merges to main · {} workers · {} events", a0, a1, a2, a3, a4, a5)
}
pub fn line(a0: &dyn Display, a1: &dyn Display, a2: &dyn Display) -> String {
    format!("{} line {}: {}", a0, a1, a2)
}
pub fn lines_didn_t_read(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("{}  {} lines didn't read", a0, a1)
}
pub fn min(a0: &dyn Display) -> String {
    format!("{} min", a0)
}
pub fn live_fetch_the_remote_s(a0: &dyn Display) -> String {
    format!(
        "Live: fetch the remote's tracking and main every {}, off the UI thread, and reload when either moved; the view stays, or follows the end when it was there",
        a0
    )
}
pub fn live(a0: &dyn Display) -> String {
    format!("{}  Live", a0)
}
pub fn checked(a0: &dyn Display) -> String {
    format!(" · checked {}", a0)
}
pub fn every_fetches_tracking_and_main(a0: &dyn Display) -> String {
    format!("Every {}: fetches tracking and main, and reloads when either moved.", a0)
}
pub fn last_new_events_utc(a0: &dyn Display) -> String {
    format!("Last new events: {} UTC", a0)
}
pub fn fetch_ms(a0: &dyn Display) -> String {
    format!("fetch {} ms", a0)
}
pub fn read_ms(ms: &dyn Display) -> String {
    format!("read {ms} ms")
}
pub fn build_ms(ms: &dyn Display) -> String {
    format!("build {ms} ms")
}
pub fn new_events(n: &dyn Display) -> String {
    format!("{n} new events")
}
pub fn last_check(a0: &dyn Display) -> String {
    format!("Last check: {}", a0)
}
pub fn retry_at(a0: &dyn Display) -> String {
    format!(" · retry at {}", a0)
}
pub fn check_failed(a0: &dyn Display) -> String {
    format!("check failed ×{}", a0)
}
pub const CHECK_FAILED: &str = "check failed";
pub fn live_2(a0: &dyn Display, a1: &dyn Display, a2: &dyn Display, failed: &dyn Display) -> String {
    format!("{}  live · {failed}{}  {}", a0, a1, a2)
}
pub fn live_3(a0: &dyn Display, checked: &dyn Display) -> String {
    format!("{}  live{checked}", a0)
}
pub fn paused(a0: &dyn Display, checked: &dyn Display) -> String {
    format!("{}  paused{checked}", a0)
}
pub fn click_to_dismiss(note: &dyn Display, file: &dyn Display) -> String {
    format!("{note}\n{file}\n(click to dismiss)")
}
pub const FROM_GITHUB: &str = "from GitHub";
pub const CACHED: &str = "cached";
pub fn issues_and_prs(a0: &dyn Display, repo: &dyn Display, n: &dyn Display, from: &dyn Display) -> String {
    format!("{}  {repo}: {n} issues and PRs {from}", a0)
}
pub fn no_issues(a0: &dyn Display) -> String {
    format!("{}  no issues", a0)
}
pub const FETCHING: &str = " · fetching…";
pub fn fetched(t: &dyn Display) -> String {
    format!("fetched {t}")
}
pub fn cached_in(c: &dyn Display) -> String {
    format!("cached in {c}")
}
pub const FACTORY: &str = "Factory";

// ---- details --------------------------------------------------------------------------------

pub const DETAILS_2: &str = "Details";
pub const BACK_TO_WHAT_WAS_SELECTED_BEFORE: &str = "Back to what was selected before";
pub const SELECT_A_BAR_ON_THE_TIMELINE: &str = "Select a bar on the timeline, or a ticket or gate run in the factory, to see its story here.";
pub fn reading(a0: &dyn Display) -> String {
    format!("{}  Reading…", a0)
}
pub const CARRIES: &str = "carries";
pub fn show_their_session(a0: &dyn Display) -> String {
    format!("{}: show their session", a0)
}
pub const SO_FAR: &str = " so far";
pub const SO_FAR_2: &str = " (so far)";
pub fn click_show(a0: &dyn Display) -> String {
    format!("\nclick: show {}", a0)
}
pub fn ticket(issue: &dyn Display) -> String {
    format!("ticket #{issue}")
}
pub fn gate_run(sub: &dyn Display) -> String {
    format!("gate run {sub}")
}
pub fn s_session(instance: &dyn Display) -> String {
    format!("{instance}'s session")
}
pub fn merge(a0: &dyn Display) -> String {
    format!("merge {}", a0)
}

// ---- factory --------------------------------------------------------------------------------

pub const MIN_S: &str = "1 min / s";
pub const MIN_S_2: &str = "10 min / s";
pub const EVERY_MOMENT: &str = "every moment";
pub const CLAIMS: &str = "claims";
pub const WORK_BEGINNING: &str = "work beginning";
pub const SUBMITS: &str = "submits";
pub const GATE_RUNS: &str = "gate runs";
pub const SEND_BACKS: &str = "send backs";
pub const MERGES_ONTO_MAIN: &str = "merges onto main";
pub const AT_THE_TIMELINE_S_PLAYHEAD: &str = "at the timeline's playhead";
pub const AT_THE_TIMELINE_WINDOW_S_END: &str = "at the timeline window's end: no playhead";
pub const PLAYING_THE_PLAYHEAD_RUNS_THROUGH_THE: &str = "playing: the playhead runs through the window";
pub const ON_A_KEY_MOMENT: &str = "on a key moment";
pub const NOT_FOLLOWING_THE_TIMELINE: &str = "not following the timeline";
pub fn not_following_the_timeline_held(h: &dyn Display, m: &dyn Display, s: &dyn Display) -> String {
    format!("not following the timeline: held at {h:02}:{m:02}:{s:02} UTC")
}
pub const AT_THE_LAST_EVENT: &str = "at the last event";
pub const NO_REPOSITORY_IS_OPEN_START_WITH: &str = "No repository is open: start with --repo <path>.";
pub fn reading_the_tracking_branch_2(a0: &dyn Display) -> String {
    format!("{}  Reading the tracking branch…", a0)
}
pub const THE_TRACKING_BRANCH_HOLDS_NO_EVENTS: &str = "The tracking branch holds no events yet.";
pub const PLAY_THE_TIMELINE_S_PLAYHEAD_RUNS: &str = "Play: the timeline's playhead runs through its window, looping, and the factory follows it";
pub fn the_timeline_s_playhead_back(steps: &dyn Display) -> String {
    format!("The timeline's playhead back to the previous key moment ({steps}; ←)\nShift-click: to the first event (Home)")
}
pub fn the_timeline_s_playhead_on(steps: &dyn Display) -> String {
    format!("The timeline's playhead on to the next key moment ({steps}; →)\nShift-click: to the last event (End)")
}
pub fn by(steps: &dyn Display) -> String {
    format!("by {steps}")
}
pub fn follow_the_timeline(a0: &dyn Display) -> String {
    format!("{}  Follow the timeline", a0)
}
pub const THE_FACTORY_SHOWS_THE_TIMELINE_S: &str = "The factory shows the timeline's playhead, else its window's end; off, it holds where it is";
pub fn key_moment_of(a0: &dyn Display, index: &dyn Display) -> String {
    format!("key moment {index} of {}", a0)
}

// ---- factory_paint --------------------------------------------------------------------------

pub fn more(hidden: &dyn Display) -> String {
    format!("+{hidden} more")
}
pub const UP_NEXT: &str = "UP NEXT";
pub fn in_text(a0: &dyn Display) -> String {
    format!("in {}", a0)
}
pub const SHELF: &str = "SHELF";
pub const BENCHES: &str = "BENCHES";
pub fn working_crates(a0: &dyn Display, working: &dyn Display) -> String {
    format!("{working} working · {} crates", a0)
}
pub const NO_WORKERS_YET: &str = "no workers yet";
pub const NO_WORKER_ON_SHIFT: &str = "no worker on shift";
pub fn off_shift_shown_above_fold(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("{}  {} off shift, shown above: fold them", a0, a1)
}
pub const CLICK_TO_FOLD_THEM_INTO_THIS: &str = "click to fold them into this line";
pub fn off_shift(a0: &dyn Display, a1: &dyn Display, a2: &dyn Display) -> String {
    format!("{}  +{} off shift: {}", a0, a1, a2)
}
pub const CLICK_TO_SHOW_THEIR_BENCHES: &str = "click to show their benches";
pub fn off_shift_holding_nothing(a0: &dyn Display, how: &dyn Display) -> String {
    format!("{} off shift, holding nothing: {how}", a0)
}
pub const DOCK: &str = "DOCK";
pub fn waiting(a0: &dyn Display) -> String {
    format!("{} waiting", a0)
}
pub const EMPTY: &str = "empty";
pub const GATE: &str = "GATE";
pub fn running(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("running {} · {}", a0, a1)
}
pub fn idle_last_ago(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("idle · last {} {} ago", a0, a1)
}
pub const IDLE_NO_RUNS_YET: &str = "idle · no runs yet";
pub fn run(sub: &dyn Display) -> String {
    format!("run {sub}")
}
pub const NOT_YET_RUN: &str = "  not yet run";
pub const MAIN: &str = "MAIN";
pub fn of_merges(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("{} of {} merges", a0, a1)
}
pub fn last_ago(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("last: {} · {} ago", a0, a1)
}
pub fn done_no_merge(a0: &dyn Display) -> String {
    format!("done, no merge · {}", a0)
}
pub fn brick(a0: &dyn Display) -> String {
    format!("brick {}", a0)
}
pub const BORDER_PHASE_FILL_TICKET: &str = "border = phase · fill = ticket";
pub const UP_NEXT_2: &str = "up next";
pub const CLAIMED: &str = "claimed";
pub const WORKING: &str = "working";
pub const SENT_BACK: &str = "sent back";
pub const STOPPED_SHELVED: &str = "stopped / shelved";
pub const WAITING_FOR_MERGE: &str = "waiting for merge";
pub const IN_THE_GATE: &str = "in the gate";
pub const MERGED_A_BRICK_AS_WIDE_AS: &str = "merged: a brick, as wide as it took";
pub const GREY_A_MERGE_NO_TICKET: &str = "grey !13: a merge, no ticket";
pub const RUNNING: &str = "running";
pub const PASSED: &str = "passed";
pub const FAILED: &str = "failed";
pub const NO_RESULT: &str = "no result";
pub const ON_SHIFT: &str = "on shift";
pub const PAUSED: &str = "paused";
pub const OFF_SHIFT: &str = "off shift";
pub fn holds(a0: &dyn Display, status: &dyn Display) -> String {
    format!("{status} · holds {}", a0)
}

// ---- factory_stats --------------------------------------------------------------------------

pub fn utc(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("{} → {} UTC", a0, a1)
}
pub fn the_timeline_s_visible_window(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("The timeline's visible window: {} → {}", a0, a1)
}
pub fn tickets_landed_per_active_hour(a0: &dyn Display, a1: &dyn Display, a2: &dyn Display, a3: &dyn Display, quiet: &dyn Display) -> String {
    format!(
        "Tickets landed per active hour: the {} tickets merged in the window ÷ its {} active hours.\n\
             Active: the time at least one worker has a session going ({} h on shift), its pauses left out, \
             less every gap of over {quiet} with no event at all ({} such gaps).",
        a0, a1, a2, a3
    )
}
pub fn landed_active_h(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("{} landed · {} active h", a0, a1)
}
pub const LANDED_ACTIVE_H: &str = "landed / active h";
pub const NO_HOURS: &str = "no hours";
pub const LANDED_PER_ACTIVE_HOUR: &str = "Landed per active hour";
pub const EACH_CLOCK_HOUR_UTC_OF_THE: &str = "Each clock hour (UTC) of the window; the hour holding the factory's t is marked.";
pub fn utc_2(a0: &dyn Display) -> String {
    format!("{} UTC", a0)
}
pub const CLICK_THE_TIMELINE_TO_THIS_HOUR: &str = "Click: the timeline to this hour, its playhead at its end; the strip keeps its range";
pub const CLAIM_MERGE: &str = "claim → merge";
pub fn the_median_over_the_tickets(a0: &dyn Display) -> String {
    format!("The median, over the {} tickets landed in the window, of the time from a ticket's first claim to its merge (wall clock).", a0)
}
pub const DOCK_WAIT: &str = "dock wait";
pub fn the_median_wait_at_the(a0: &dyn Display) -> String {
    format!("The median wait at the dock, over the {} tickets landed: from a ticket's last submit to its merge (wall clock).", a0)
}
pub const GATES_PASS: &str = "gates pass";
pub const SENDBACKS_LANDED: &str = "sendbacks / landed";
pub fn the_sendbacks_in_the_window(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("The {} sendbacks in the window ÷ the {} tickets landed.", a0, a1)
}
pub const WORKING_IN_FLIGHT: &str = "working · in flight";
pub fn the_mean_tickets_working_from(a0: &dyn Display) -> String {
    format!(
        "The mean tickets working (from work to its submit, sendback, stop or merge), and in flight (claimed, working, waiting or sent back), over the active time: each ticket's time in the phase ÷ the {} active hours.",
        a0
    )
}
pub const WORKERS_BUSY: &str = "workers busy";
#[expect(clippy::too_many_arguments, reason = "a text template takes as many values as its text has")]
pub fn the_coordinator_s_gates_that(
    a0: &dyn Display,
    a1: &dyn Display,
    a2: &dyn Display,
    a3: &dyn Display,
    a4: &dyn Display,
    a5: &dyn Display,
    a6: &dyn Display,
    a7: &dyn Display,
    a8: &dyn Display,
    a9: &dyn Display,
) -> String {
    format!(
        "The coordinator's gates that started in the window: {} passed of {} with a result ({} started, {} with none, {} running, {} read from merges and send backs).\nPassed: rc=0, or a merge names the gate. Failed: another rc.\nFailures by cause: {}\nWorkers' own ready gates: {} passed of {} ({}).",
        a0, a1, a2, a3, a4, a5, a6, a7, a8, a9
    )
}
pub const NONE: &str = "none";
pub fn utilisation_the_time_a_worker(a0: &dyn Display, quiet: &dyn Display) -> String {
    format!(
        "Utilisation: the time a worker's tickets were working ÷ its time on shift, both less the gaps of over {quiet} with no event, in the window. All together: {}.",
        a0
    )
}
pub fn working_of_on_shift_landed(a0: &dyn Display, a1: &dyn Display, a2: &dyn Display, a3: &dyn Display, a4: &dyn Display, a5: &dyn Display) -> String {
    format!("{} ({}): {} · {} working of {} on shift · {} landed", a0, a1, a2, a3, a4, a5)
}
pub fn landed_in_active_landed_active(a0: &dyn Display, a1: &dyn Display, a2: &dyn Display) -> String {
    format!("{} landed in {} active · {} landed / active h", a0, a1, a2)
}

// ---- open -----------------------------------------------------------------------------------

pub const HOME: &str = "HOME";
pub const NO_RECENT_REPOSITORIES: &str = "No Recent Repositories";
pub fn can_t_open_that_folder(a0: &dyn Display) -> String {
    format!("{}  Can't open that folder", a0)
}
pub fn that_folder(rest: &dyn Display) -> String {
    format!("That folder{rest}.")
}
pub fn stays_open(repo: &dyn Display) -> String {
    format!("{repo} stays open.")
}
pub const OPEN_A_SWARM_S_CLONE_A: &str = "Open a swarm's clone: a git repository with a tracking branch.";
pub const OK: &str = "OK";
pub fn open(a0: &dyn Display) -> String {
    format!("{}  Open…", a0)
}
pub fn open_a_repository(a0: &dyn Display) -> String {
    format!("{}  Open a Repository", a0)
}
pub const A_FOLDER_IN_A_SWARM_S: &str = "A folder in a swarm's clone (a git repository with a tracking branch):";
pub const OPEN: &str = "Open";
pub const CANCEL: &str = "Cancel";

// ---- timeline -------------------------------------------------------------------------------

pub fn scale_event_order_idle_time(a0: &dyn Display) -> String {
    format!("{} scale: event order, idle time cut out", a0)
}
pub fn pointer(a0: &dyn Display) -> String {
    format!("  ·  pointer {}", a0)
}

// ---- workers --------------------------------------------------------------------------------

pub const MODEL_EFFORT_WORKER: &str = "Model × effort  ·  worker";
pub const SORT_AS_WORKERS_COMPARE_LISTS_THEM: &str = "Sort as workers.compare lists them: by model, then effort, most first; each worker by its start.";
pub const CLAIMED_2: &str = "Claimed";
pub const THE_TICKETS_IT_CLAIMED_IN_THE: &str = "The tickets it claimed in the window (its own claim, not a gate's).";
pub const LANDED: &str = "Landed";
pub const THE_TICKETS_MERGED_IN_THE_WINDOW: &str = "The tickets merged in the window that it held last.";
pub const LANDED_ACTIVE_H_2: &str = "Landed / active h";
pub const LANDED_ITS_ACTIVE_HOURS_ACTIVE_ON: &str = "Landed ÷ its active hours. Active: on shift, less the swarm's quiet gaps (15 min with no event of anyone's) and its own idle time (15 min without a word while it held no ticket). n: the tickets landed. More is better.";
pub const CLAIM_MERGE_2: &str = "Claim → merge";
pub const THE_MEDIAN_OVER_ITS_LANDED_TICKETS: &str =
    "The median, over its landed tickets, of its first claim or work of each to its merge. n: the tickets. Shorter is quicker.";
pub const WORKING_2: &str = "Working";
pub const THE_MEDIAN_TIME_EACH_LANDED_TICKET: &str =
    "The median time each landed ticket was working by it, summed over its stretches. n: the tickets. Shorter is quicker.";
pub const SEND_BACKS_LANDED: &str = "Send backs / landed";
pub const THE_GATE_S_SEND_BACKS_OF: &str =
    "The gate's send backs of tickets it held, ÷ the tickets it landed. n: the tickets landed. Fewer is better; the causes on hover.";
pub const READY_PASS: &str = "Ready pass";
pub const ITS_OWN_READY_CHECKS_BEFORE_IT: &str =
    "Its own ready checks before it submits: passed ÷ (passed + failed). n: the checks with a result. More is better.";
pub const ABANDONED: &str = "Abandoned";
pub const THE_TICKETS_ITS_SESSION_S_END: &str = "The tickets its session's end cut off while it held them (#28), whether or not another took them up.";
pub const ACTIVE_ON_SHIFT: &str = "Active / on shift";
pub const ITS_ACTIVE_HOURS_AS_LANDED_ACTIVE: &str = "Its active hours (as Landed / active h has them), and its time on shift: its sessions less their pauses.";
pub fn min_s(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("{} min {:02} s", a0, a1)
}
pub fn h_min(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("{} h {:02} min", a0, a1)
}
pub fn thin_under_samples(a0: &dyn Display) -> String {
    format!(": thin, under {} samples", a0)
}
pub fn samples(a0: &dyn Display, thin: &dyn Display) -> String {
    format!("{} samples{thin}", a0)
}
pub fn landed_in_active_on_shift(a0: &dyn Display, a1: &dyn Display, a2: &dyn Display, a3: &dyn Display) -> String {
    format!("{} landed in {} active ({} on shift) · {}", a0, a1, a2, a3)
}
pub fn median_claim_merge(a0: &dyn Display) -> String {
    format!("median claim → merge · {}", a0)
}
pub fn median_working_time_per_landed(a0: &dyn Display) -> String {
    format!("median working time per landed ticket · {}", a0)
}
pub fn by_cause(a0: &dyn Display) -> String {
    format!("\nby cause: {}", a0)
}
pub fn send_backs_over_landed(a0: &dyn Display, a1: &dyn Display, a2: &dyn Display, causes: &dyn Display) -> String {
    format!("{} send backs over {} landed · {}{causes}", a0, a1, a2)
}
pub fn ready_checks_passed_failed_with(a0: &dyn Display, a1: &dyn Display, a2: &dyn Display, a3: &dyn Display, a4: &dyn Display) -> String {
    format!("{} ready checks: {} passed, {} failed, {} with no result · {}", a0, a1, a2, a3, a4)
}
pub fn active_of_on_shift(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("active {} of {} on shift", a0, a1)
}
pub fn workers_model_effort(a0: &dyn Display) -> String {
    format!("{}  Workers: model × effort", a0)
}
pub const NO_WORKERS_TO_COMPARE_YET: &str = "No workers to compare yet.";
pub const NO_WORKER_WAS_ON_SHIFT_IN: &str = "No worker was on shift in this window.";
pub const TICKETS_DIFFER_IN_SIZE_AND_KIND: &str =
    "Tickets differ in size and kind, and a worker's tickets aren't drawn at random: read a gap between models as a hint, the more samples (n) the firmer.";
pub fn worker(a0: &dyn Display, a1: &dyn Display) -> String {
    format!("{} worker{}", a0, a1)
}
pub const OPEN_2: &str = " · open";
pub const ALL: &str = "all";
pub const ALL_WORKERS: &str = "All workers";
pub fn sessions(a0: &dyn Display) -> String {
    format!("{} sessions", a0)
}
pub const TIMELINE_S_WINDOW: &str = "Timeline's window";
pub const COMPARE_OVER_THE_TIMELINE_S_VISIBLE: &str = "Compare over the timeline's visible window, following it";
pub const WHOLE_RUN: &str = "Whole run";
pub const COMPARE_OVER_THE_WHOLE_RUN_THE: &str = "Compare over the whole run: the first event to the last";
pub fn utc_4(a0: &dyn Display, a1: &dyn Display, a2: &dyn Display) -> String {
    format!("{} → {} UTC · {}", a0, a1, a2)
}
pub fn n_the_samples_a_figure(a0: &dyn Display) -> String {
    format!("n: the samples a figure rests on · ~ thin: under {} samples, its bar outlined", a0)
}
pub const GOING_ON: &str = "(going on)";
pub const OPEN_NO_SESSION_STOP_LOGGED_SO: &str = "\nopen: no session.stop logged, so on shift to the last event";
#[expect(clippy::too_many_arguments, reason = "a text template takes as many values as its text has")]
pub fn utc_tickets_click_its_session(
    a0: &dyn Display,
    a1: &dyn Display,
    a2: &dyn Display,
    a3: &dyn Display,
    a4: &dyn Display,
    group: &dyn Display,
    end: &dyn Display,
    open: &dyn Display,
) -> String {
    format!("{} · {} · {group}\n{} → {end} UTC{open}\n{} tickets: {}\nclick: its session's story in Details", a0, a1, a2, a3, a4)
}

// ---- layout ---------------------------------------------------------------------------------

pub fn fold_the_to_its_header(name: &dyn Display) -> String {
    format!("Fold the {name} to its header")
}
pub fn fold_the_to_a_strip(name: &dyn Display) -> String {
    format!("Fold the {name} to a strip")
}
pub fn open_the(name: &dyn Display) -> String {
    format!("Open the {name}")
}

// ---- menu -----------------------------------------------------------------------------------

pub const OPEN_RECENT: &str = "Open Recent";
pub const RECENT_REPOSITORY: &str = "Recent Repository";
pub const QUIT: &str = "Quit";
pub const TIMELINE: &str = "Timeline";
pub const WORKERS: &str = "Workers";
pub const FLAT: &str = "Flat";
pub const BY_WORKER: &str = "By Worker";
pub const COLLAPSE_EMPTY_ROWS: &str = "Collapse Empty Rows";
pub const COLLAPSE_TO_WINDOW: &str = "Collapse to Window";
pub const EVERY_ROW: &str = "Every Row";
pub const FOLD_QUIET_TIME: &str = "Fold Quiet Time";
pub const MERGE_MARKS: &str = "Merge Marks";
pub const FIT: &str = "Fit";
pub const LIVE: &str = "Live";
pub const MISSING: &str = " (missing)";

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    /// The string literals each source still writes in its non-test code that hold a word (a
    /// placeholder's name and an escape aside): ids, keys, serde names and patterns a person
    /// doesn't read, which stay where they are, and the texts of the modules that aren't drawing
    /// code. A count only goes down: one over it is a word to move here, one under it a count to
    /// lower (#162).
    const ALLOWED: &[(&str, usize, &str)] = &[
        ("control.rs", 25, "the control protocol: its commands' ids and errors, for agents"),
        ("details.rs", 0, ""),
        ("factory.rs", 13, "its steps' ids and the header's button names, which ui.inspect reports"),
        ("factory_chip.rs", 0, ""),
        ("factory_paint.rs", 8, "the stations a chip is at, which ui.inspect reports; a model's and a ready check's prefixes"),
        ("factory_stats.rs", 0, ""),
        ("factory_tip.rs", 28, "the words a tooltip colours, matched in its text, and the units it reads"),
        ("density.rs", 0, ""),
        ("fonts.rs", 17, "the faces' names and files, the named families' and text styles' keys: ids egui looks them up by"),
        ("icons.rs", 0, ""),
        ("layout.rs", 0, ""),
        ("lib.rs", 2, "a band's name, which ui.inspect reports, and the refs' prefix"),
        ("menu.rs", 33, "the menu items' ids, the registry's menu paths and the commands' ids for their glyphs"),
        ("open.rs", 0, ""),
        ("saved.rs", 43, "the state file's notes and errors, made where the file is read: not drawing code"),
        ("shot.rs", 3, "the image formats' names"),
        ("timeline.rs", 0, ""),
        ("workers.rs", 0, ""),
    ];

    /// The sources that may: this catalog, the theme (where a colour is made) and the id module.
    const EXEMPT: &[&str] = &["catalog.rs", "theme.rs", "part.rs"];

    /// The non-test code of `text`: up to its first `#[cfg(test)]`.
    fn code(text: &str) -> &str {
        text.find("\n#[cfg(test)]").map_or(text, |at| &text[..at])
    }

    /// The string literals of `code`, comments and attributes left out.
    fn literals(code: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in code.lines().filter(|l| !l.trim_start().starts_with("#[") && !l.trim_start().starts_with("//")) {
            let mut chars = line.char_indices().peekable();
            while let Some((i, c)) = chars.next() {
                if c == '/' && line[i..].starts_with("//") {
                    break;
                }
                if c == '\'' {
                    // A char literal: skip it whole; a lifetime has no closing quote.
                    let rest = &line[i + 1..];
                    let len = if rest.starts_with('\\') {
                        rest.find('\'').map(|e| e + 1)
                    } else {
                        rest.chars().nth(1).filter(|c| *c == '\'').map(|_| rest.chars().next().map_or(1, char::len_utf8) + 1)
                    };
                    if let Some(len) = len {
                        for _ in 0..rest[..len].chars().count() {
                            chars.next();
                        }
                    }
                    continue;
                }
                if c != '"' {
                    continue;
                }
                let mut text = String::new();
                while let Some((_, c)) = chars.next() {
                    match c {
                        '\\' => {
                            chars.next();
                            text.push(' ');
                        }
                        '"' => break,
                        c => text.push(c),
                    }
                }
                out.push(text);
            }
        }
        out
    }

    /// Whether a literal holds a word: two letters together, outside a placeholder.
    fn has_word(text: &str) -> bool {
        let mut bare = String::new();
        let mut depth = 0;
        for c in text.chars() {
            match c {
                '{' => depth += 1,
                '}' => depth = (depth - 1).max(0),
                c if depth == 0 => bare.push(c),
                _ => {}
            }
        }
        bare.chars().collect::<Vec<_>>().windows(2).any(|w| w[0].is_ascii_alphabetic() && w[1].is_ascii_alphabetic())
    }

    fn sources() -> Vec<(String, String)> {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out: Vec<(String, String)> = std::fs::read_dir(&src)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap()))
            .filter(|(name, _)| !EXEMPT.contains(&name.as_str()))
            .collect();
        out.sort();
        out
    }

    /// **Words from the catalog**: each source writes no more words than its count, and its count
    /// is what it writes (lower it when a word moves here).
    #[test]
    fn drawing_code_takes_its_words_from_the_catalog() {
        let counted: BTreeMap<String, usize> =
            sources().into_iter().map(|(name, text)| (name, literals(code(&text)).iter().filter(|l| has_word(l)).count())).collect();
        let allowed: BTreeMap<String, usize> = ALLOWED.iter().map(|(name, n, _)| ((*name).to_owned(), *n)).collect();
        assert_eq!(counted, allowed, "a count over its allowance is a word to move to the catalog; under it, lower the allowance");
    }

    /// **No colour made in drawing code**: a colour is a theme token or a range's entry
    /// (`theme`, `factory_chip`'s conversion of the swarm's), never `Color32::…` but its sentinels.
    #[test]
    fn drawing_code_makes_no_colour() {
        let made: Vec<String> = sources()
            .into_iter()
            .filter(|(name, _)| name != "factory_chip.rs")
            .flat_map(|(name, text)| {
                code(&text)
                    .lines()
                    .enumerate()
                    .filter(|(_, l)| !l.trim_start().starts_with("//"))
                    .filter(|(_, l)| l.contains("Color32::") && l.replace("Color32::TRANSPARENT", "").replace("Color32::PLACEHOLDER", "").contains("Color32::"))
                    .map(|(n, l)| format!("{name}:{}: {}", n + 1, l.trim()))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(made.is_empty(), "colours made in drawing code:\n{}", made.join("\n"));
    }

    #[test]
    fn literals_and_words_are_read_as_written() {
        assert_eq!(literals(r#"let a = "one {x}"; // "not this" "#), ["one {x}"]);
        assert_eq!(literals(r#"f('"', "two\"s", 'a')"#), ["two s"]);
        assert!(has_word("Open…") && !has_word("{}  {repo}") && !has_word("{mo:02}-{d:02}") && !has_word("x"));
    }
}
