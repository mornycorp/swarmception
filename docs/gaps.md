<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Gaps

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-02 · **Change:** minor (batch 7: workers compared, the panes share the window, opening a repository) · **Target:** M3

Every known shortfall of Swarmception, ranked: **the work list, and the only home for progress
detail** ([progress-docs.md](progress-docs.md) rule 8). The coordinator folds each batch's merged
PRs' Progress sections into these entries, once per batch, in one commit; workers report in their
PR instead of editing this file. `ROADMAP.md` and [roadmap.md](roadmap.md) link here, one line an
item.

Each entry: its id (`G1`, never reused), what is missing and why it matters, what's done and
what's left, the evidence (code paths, tests, commands, issues), measured numbers with their
source and date, and an estimate in agent-hours (a range). Work an agent can't finish alone is
marked **Human:** with what is needed.

## Ranked

| Rank | Gap | Status | Issues | Estimate |
|---|---|---|---|---|
| 1 | [G6 Collapse to the window](#g6-collapse-to-the-window) | open | | 2–4 h |
| 2 | [G19 Loose ends from M2 and M3](#g19-loose-ends-from-m2-and-m3) | open | | 2–4 h |
| — | [G9 Swarm analytics panes](#g9-swarm-analytics-panes) | done | #18, #20, #22, #31–#33, #42, #61, #67 | |
| — | [G18 The panes share the window](#g18-the-panes-share-the-window) | done | #68 | |
| — | [G17 The restored window and the Details' rows are always in view](#g17-the-restored-window-and-the-details-rows-are-always-in-view) | done | #68 | |
| — | [G8 Opening a repository from the app](#g8-opening-a-repository-from-the-app) | done | #69 | |
| — | [G15 Automation reaches the window](#g15-automation-reaches-the-window) | done | #60 | |
| — | [G13 Issues fetched incrementally](#g13-issues-fetched-incrementally) | done | #62 | |
| — | [G5 The state survives a restart](#g5-the-state-survives-a-restart) | done | #53 | |
| — | [G4 Watching a live swarm](#g4-watching-a-live-swarm) | done | #54 | |
| — | [G16 The details pane and the timeline select together](#g16-the-details-pane-and-the-timeline-select-together) | done | #55 | |
| — | [G2 Automation: the control channel and MCP](#g2-automation-the-control-channel-and-mcp) | done | #46 | |
| — | [G3 A details pane for what is selected](#g3-a-details-pane-for-what-is-selected) | done | #47 | |
| — | [G12 A highlight that leaves the selection alone](#g12-a-highlight-that-leaves-the-selection-alone) | done | #48, swarm-timeline#188 | |
| — | [G14 The widget's toolbar and labels read clearly](#g14-the-widgets-toolbar-and-labels-read-clearly) | done | #48, swarm-timeline#183, #188 | |
| — | [G10 The macOS View menu holds the system's tab items](#g10-the-macos-view-menu-holds-the-systems-tab-items) | done | #9 | |
| — | [G11 An abandoned ticket runs on to the last event](#g11-an-abandoned-ticket-runs-on-to-the-last-event) | done | #28 | |
| — | [G7 Tickets carry their titles](#g7-tickets-carry-their-titles) | done | #17 | |
| — | [G1 The ruler reads clock time](#g1-the-ruler-reads-clock-time) | done | #2 | |

## G10 The macOS View menu holds the system's tab items

- **Done** (#9, PR #29): automatic window tabbing is off before the menu bar is built, so the
  View menu reads Timeline, Factory, Flat, By Worker, Collapse Empty Rows, Fit, Enter Full
  Screen, no Show Tab Bar / Show All Tabs (read from the running app's menu bar, 2026-11-01).
- **Evidence:** `apps/app/src/mac_menu.rs` (`no_window_tabs`).

## G11 An abandoned ticket runs on to the last event

- **Done** (#28, PR #30): a claimed, working or sent-back stretch ends where its holder's session
  ended (its stop, a coordinator's `--of`, or its next `session.start`), and the ticket reads
  *abandoned* (tooltip; the Factory's shelf). On swarm-timeline #40 now ends 2026-10-30T21:32:24Z
  and #132 2026-10-31T03:26:40Z, the only two of 82 that changed; mc-engine none (measured
  2026-11-01). A gate that never logged its stop ends the same way, as *no result* (#42, PR #44).
- **Left:** a worker that moves on to other tickets but keeps its session (#132's, 01:37–03:26)
  still reads as working its first ticket to the session's end.
- **Evidence:** `crates/domain/src/tickets.rs`, `activity.rs` (`Agent::session_ended_after`,
  `Sub::unfinished`).

## G12 A highlight that leaves the selection alone

- **Done** (swarm-timeline#188, PR #189; #48, PR #51): the widget has a highlight apart from the
  selection (`highlight.set`/`highlight.clear`, an amber ring, in its state), and hovering a
  Factory chip highlights the ticket's bars instead of borrowing the selection: the selection,
  focus, playhead and the Details pane stay on the person's choice, and `ui.inspect` reports
  `highlighted` beside the selection.
- **Left elsewhere:** zoomed out, a packed bar shows the ring only when its first range is the
  highlighted one (the widget's).
- **Evidence:** `crates/ui/src/factory.rs`, `timeline.rs`; `crates/ui/tests/factory_pane.rs`
  (`hovering_a_chip_highlights_its_bars_and_never_touches_the_selection`); screenshot checked
  2026-11-02.

## G13 Issues fetched incrementally

- **Done** (#62, PR #64): the cache (version 2) keeps its fetch times, ETag and each item's
  `updated`; a refresh asks once for `issues?since=<newest updated − 60 s>` with `If-None-Match`
  (a 304 is "unchanged") and merges what changed; a full fetch only with no usable cache, a week
  after the last full one, or more than 100 changed; a `gh` error leaves the cache alone. An
  issue links to its PR by the PR's head branch (`<n>-…`) too; `repo.status` reports the last
  refresh.
- **Measured:** mc-engine (229 issues and PRs): full 2.2–2.7 s, incremental 0.55 s, unchanged
  0.49–0.58 s (mostly `gh` starting) (2026-11-02).
- **Left:** a live check refreshes issues only when new tickets appear (the rule from #54 when a
  refresh cost 1.4 s; at 0.5 s every check that brings events could, keeping PR states current);
  deleted or transferred issues leave the cache only at the weekly full fetch.
- **Evidence:** `crates/source/src/github.rs`, `crates/model/src/issues.rs`,
  `crates/engine/tests/issues.rs`.

## G14 The widget's toolbar and labels read clearly

- **Done** (swarm-timeline#183, #188, PR #189; #48, PR #51; the pin at 9bf2651): the zoom
  buttons' icons are drawn as lines in the button's colour, as plain as Fit; durations from a
  minute read in their two largest units (`3 min 20 s`, `1 h 59 min`, `25 h 03 min`) in bar
  labels, tooltips, the ruler's Δ and measurements; a strong span is struck three times, plainly
  bold.
- **Left elsewhere:** a selected range's outline in a row scrolled under the ruler can draw over
  the ruler and minimap (the widget's; the highlight had the same fault and was fixed).
- **Evidence:** `crates/ui/tests/timeline_pane.rs`
  (`bar_labels_and_tooltips_read_durations_in_hours_and_minutes`); screenshots checked
  2026-11-02.

## G15 Automation reaches the window

- **Done** (#60, PR #63): `swarmception-cli mcp --control <file>` serves the engine's, the
  window's and the widget's commands as tools run in the window (114 on mc-engine);
  `ui.screenshot` and `--screenshot` write PNG for `.png` (`png` was already in the lock);
  `swarmception-cli send --control <f> <id> [<params>]` and `--script <file.jsonl>` with exit codes;
  `ui.commands` lists the widget's commands with their schemas; `ui.loaded` waits for the data;
  `scripts/screenshots.sh` takes a PR's screenshots.
- **Left:** MCP returns a screenshot's path, not image content.
- **Evidence:** `crates/automation`, `crates/ui/src/shot.rs`, `apps/cli/tests/automation.rs`,
  `apps/app` control tests; PNGs checked 2026-11-02.

## G16 The details pane and the timeline select together

- **Done** (#55, PR #57): `details.get` names its subject's bars on the rows shown, and the
  timeline highlights them (one owner, `TimelinePane::light`: a Factory hover while it lasts,
  else the Details subject), never the selection: a ticket's bars; a gate run's merges onto main
  and its tickets; a session's stretches, merges and their merges onto main; a merge onto main
  and its ticket. If none is in view the window centres on them (or widens to hold them);
  `ui.set {details, clearDetails}`.
- **Left:** choosing a session from its worker's row (a widget change: group-row selection);
  [G17](#g17-the-restored-window-and-the-details-rows-are-always-in-view) (bars in a collapsed
  group).
- **Evidence:** `crates/ui/tests/details_pane.rs`, `factory_pane.rs`; screenshots checked
  2026-11-02.

## G17 The restored window and the Details' rows are always in view

- **Done** (#68, PR #71): a window restored partly or wholly off the monitors moves onto the one
  it overlaps most, or the nearest, and shrinks to fit (`WindowPlace::onto`; unit-tested, the
  app's wiring checked by build only: no monitor to unplug). A Details subject in a collapsed
  group opens that group (`details.get` names its groups), and the scroll reaches a merged
  ticket's own rows, not only the merges row. The widget's selection outline over the ruler
  didn't reproduce; nothing filed.
- **Evidence:** `crates/ui/src/saved.rs`, `crates/ui/tests/layout.rs`.

## G18 The panes share the window

- **Done** (#68, PR #71): `crates/ui/src/layout.rs`. The timeline keeps its chrome, the merges
  rows and six rows under them (all if fewer); the bands under it (Gates, Factory) ask for a
  height (dragged, or a default from the window) and give way first, in proportion to their
  slack, coming back as the window grows. A band folds to its header from its title, Details to
  a strip; the window decides until the person does (saved, `UiState` 6); `ui.set {folded}` and
  `ui.inspect`'s `layout`. At 1440×900 with every pane open the timeline shows all 14 of
  mc-engine's grouped rows (about 4 before).
- **Left:** dragging one band taller at the timeline's minimum takes some of the other's room.
- **Evidence:** `crates/ui/src/layout.rs`, `crates/ui/tests/layout.rs`; screenshots checked
  2026-11-02.

## G19 Loose ends from M2 and M3

- **Missing:** small things the batches left: the workers' `ready` checks fail with no cause
  (their notes say only `rc=1`: a swarm-core change to log the failing step); the Gates band
  draws the ready lane's hours only in a tooltip; `gates.list` counts inferred gate runs that
  `stats.get` doesn't; `ui.set` can't set the Workers pane's sort or window; MCP returns a
  screenshot's path, not image content; off macOS, File ▸ Open… asks for a typed path.
- **Estimate:** 2–4 agent-hours, part in swarm-core.

## G1 The ruler reads clock time

- **Done** (#2, PR #8; swarm-timeline#180): the scrub ruler is a Clock ruler on first load,
  reading UTC (`21:00`, `31 Oct` at midnight, every 3 h at fit); the widget's from/to fields and
  the status bar read `2026-10-30 18:35:57 → 2026-10-31 22:26:46 UTC`; a reload keeps the
  rulers. The Time ruler (seconds from the origin) stays a choice.
- **Left elsewhere:** bars' labels give durations in seconds (`#40 working (90,181 s)`): the
  widget draws them, swarm-timeline#183.
- **Evidence:** `crates/domain/src/timeline.rs` (`clock_epoch`), `crates/ui/src/timeline.rs`,
  `crates/ui/tests/timeline_pane.rs`; screenshot checked 2026-10-31.

## G2 Automation: the control channel and MCP

- **Done** (#46, PR #49): the `swarmception-automation` crate (L4, on `timeline-automation`'s
  pattern and types). `swarmception --repo <r> --control [<port>] [--control-file <f>]` listens
  on loopback with a random token: JSON lines `{v, token, id, command, params}`, typed errors;
  the engine's commands, `ui.commands`, `ui.inspect` (panes, the Factory's *t* and stations, the
  timeline's window, selection and highlight), `ui.set`, `ui.screenshot`, and the widget's
  commands forwarded as `timeline.<id>`. `swarmception-cli serve` speaks it headless and
  `swarmception-cli mcp` serves the engine's commands as MCP tools.
- **Measured:** 22 tests, over a real socket and the CLI as a child process (2026-11-02); the
  app driven on mc-engine over the socket, screenshots checked.
- **Left:** [G15](#g15-automation-reaches-the-window).
- **Evidence:** `crates/automation`, `apps/app/src/control.rs`, `apps/cli/tests/automation.rs`.

## G3 A details pane for what is selected

- **Done** (#47, PR #50): `details.get` (a ticket, a gate run, a session, a merge onto main, or a
  timeline bar) returns a header (colour, title, state, labels, PR, links), figures and the
  story in time order with phase colours and durations; View ▸ Details draws it beside the
  timeline and follows the person's bar selection, a Factory chip, and a gate run clicked in
  the Factory (now clickable); a line, a name or a chip in the pane selects that, ← goes back;
  `--details <subject>` for screenshots.
- **Measured:** mc-engine #190 in 6 lines (claim → merge 21 min 03 s); swarm-timeline #170 in 8
  with its send-back for a conflict; every ticket of this clone's own swarm read in order
  (2026-11-02).
- **Left:** [G16](#g16-the-details-pane-and-the-timeline-select-together).
- **Evidence:** `crates/domain/src/details.rs`, `crates/engine/src/details_cmds.rs`,
  `crates/engine/tests/details.rs`, `crates/ui/tests/details_pane.rs`; screenshots checked
  2026-11-02.

## G4 Watching a live swarm

- **Done** (#54, PR #58): `live.set {on, intervalSeconds}`, `live.get`, `live.check`; a Live
  toggle and interval in the toolbar, View ▸ Live (⌘L), a status-bar segment. A check is one
  background `git fetch` of `tracking` and `main` (30 s deadline) and reads again only if either
  moved; on by default only when the latest event is under 30 min old; failures back off to
  10 min and leave the timeline alone; issues refetched only for new tickets. A reload follows
  the end if the window was there (or shows all if it did), else stays put; the selection and
  focus carry to the same bars (`timeline.carryBars`), and the playhead, highlight and Details
  subject stay.
- **Measured:** on mc-engine (990 events), a quiet check 0.8–0.9 s in the background; one that
  brought an event 2.9 s fetch, 87 ms read, about 3 ms on the UI thread (2026-11-02).
- **Left:** each reload reads the whole branch again (fine at 87 ms).
- **Evidence:** `crates/engine/src/live.rs`, `crates/engine/tests/live.rs`,
  `crates/ui/tests/live_reload.rs`; screenshots checked 2026-11-02.

## G5 The state survives a restart

- **Done** (#53, PR #56): one typed, versioned `AppState` per repository (keyed by its
  canonical path) in `~/Library/Application Support/swarmception/state/` (`--state-dir`,
  `SWARMCEPTION_STATE_DIR`; `--fresh`): the panes and their sizes, the rows spec, the window's
  size and place, the Factory's settings, the Details subject, Live as the person chose it, and
  the timeline's view (the widget's `state.save`, the window in clock times, following the end or
  showing all). Written once the state holds still (1 s, at most every 5 s) and on quit; each
  section read on its own, so a corrupt, foreign, older or newer file keeps what it can and says
  so once. `--screenshot` and `--control` runs keep no state unless given a directory.
- **Left:** [G17](#g17-the-restored-window-and-the-details-rows-are-always-in-view) (a window
  restored onto a monitor since unplugged).
- **Evidence:** `crates/ui/src/saved.rs`, `crates/ui/tests/saved_state.rs`; screenshots checked
  2026-11-02.

## G6 Collapse to the window

- **Missing:** Collapse leaves out rows with no events at all (owner-directions.md); a mode
  that leaves out rows with nothing in the visible window would follow a pan.
- **Estimate:** 2–4 agent-hours.

## G7 Tickets carry their titles

- **Done** (#17, PR #26): each load reads the repo's issues and PRs through `gh api` (90 s
  deadline), cached per repo and versioned (`~/Library/Caches/swarmception/issues/`), refreshed
  in the background; no GitHub, `gh` or network leaves the timeline loading and the status bar
  saying why. Bars read `#58 <title>`, always (#34, PR #36 took the option away); tooltips lead with the title,
  state, labels and the PR; a bar's or a chip's context menu opens the issue or the PR
  (`ticket.urls`).
- **Evidence:** `crates/source/src/github.rs`, `crates/engine/tests/issues.rs`,
  `crates/ui/tests/ticket_links.rs`; screenshots checked 2026-11-01.

## G8 Opening a repository from the app

- **Done** (#69, PR #72): File ▸ Open… (⌘O; AppKit's folder panel through `objc2-app-kit`,
  already in the lock; a typed path elsewhere) and File ▸ Open Recent (10, app-wide, "(missing)"
  marked, Clear Menu), in the window's and the macOS menu bar. `repo.open` checks a folder first
  (5 s deadline) and refuses a missing folder, a file, a non-repository or one with no tracking
  branch with a typed reason, the open repository staying; `repo.recent`, `repo.clearRecent`.
  Switching saves the old repository's state and restores the new one's, Live included. Every
  live check that brings events now refreshes the issues (537 ms incremental on mc-engine, off
  the UI thread).
- **Evidence:** `crates/engine/tests/open.rs`, `crates/ui/tests/open_repo.rs`,
  `crates/engine/tests/live.rs`; screenshots checked 2026-11-02.

## G9 Swarm analytics panes

- **Done:**
  - **The Factory pane** (#18, PR #19): the swarm at a time *t* as a factory, one chip per ticket
    for its whole life, following the timeline or playing; `factory.at`.
  - **Revised from the owner's review** (#31, PR #37; #32, PR #38; #33, PR #43): no ticker, the
    key along the bottom, short benches (8–12 workers fit), the gate as its list of runs (✔ green,
    ✖ red with the cause, ? grey for one that never finished: #42, PR #44); coloured tooltips;
    main's wall of bricks as wide as each ticket took, with its phases as a strip and a red corner
    for each send back; stepping back and forth by key moment (`factory.moments`, `factory.step`).
  - **Statistics** (#20, PR #21; #22, PR #23): `stats.get` and a strip in the Factory's header
    for the visible window; a click on an hour's bar zooms the timeline to it. Tickets landed per
    active hour (active = a worker on shift, less gaps over 15 min with no events): mc-engine 6.9
    (105 in 15.2 h), swarm-timeline 10.6 (86 in 8.12 h) (measured 2026-11-02).
  - **Colours** (#15, PR #25; #34, PR #36; #40, PR #41): one colour per ticket from a fixed
    11-colour palette (scheme B, the owner's choice), shared by the timeline and the Factory, kept
    clear of the phase colours.
  - **Gates** (#61, PR #65): `gates.list` (every gate run and `ready` check in a window, with
    its result and cause, cargo, merges, reruns; figures per lane; per hour; the longest queue)
    and a Gates band under the timeline, aligned with it: a lane each for the gate and the
    ready checks, runs per hour, causes; hover lights a run's tickets, a click tells its story.
    mc-engine: gate 124 runs, 99.2% pass, median 64 s, busy 7.3%; ready 138, 85.3%.
    swarm-timeline: gate 83, 93.8%, median 28 s; ready 107, 96.3% (measured 2026-11-02).
  - **Workers** (#67, PR #70): `workers.compare` (per worker session, per model × effort and all
    together: landed, landed per active hour, claim → merge, working time, send-backs by cause,
    ready pass rate, abandoned, shift and active time; each figure with its sample count, thin
    under 3) and View ▸ Workers, a floating window: sortable, a hover lights a worker's
    tickets, a click tells their session. mc-engine: high 2.7 landed / active h (n=50, claim →
    merge 26 min), medium 3.6 (n=29, 16 min), low 3.8 (n=17, 11 min) (measured 2026-11-02; the
    efforts ran on different tickets, so a hint).
- **Left:** [G19](#g19-loose-ends-from-m2-and-m3).
- **Evidence:** `crates/domain/src/factory.rs`, `moments.rs`, `stats.rs`, `colours.rs`;
  `crates/ui/src/factory*.rs`.
- **Estimate:** 3–6 agent-hours, in issues of their own.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-02 | minor | Batch 7: G8, G9, G17, G18 done (#67–#69); G19 new; M3 built |
| 2026-11-02 | minor | Batch 6: G13, G15 done (#60, #62); G9 grew (#61, a gates pane); G18 new; focus G9, G18, G17 |
| 2026-11-02 | minor | Batch 5: G4, G5, G16 done (#53–#55); G17 new; M2 built; focus G15, G9, G13 |
| 2026-11-02 | minor | Batch 4: G2, G3, G12, G14 done (#46–#48, swarm-timeline#183, #188); G15, G16 new; focus G5, G4, G16 |
| 2026-11-01 | minor | Batch 3: G10, G11 done (#9, #28); G9 grew (#31–#33, #40, #42); G14 new; focus G2, G3, G12 |
| 2026-11-01 | minor | Batch 2: G7 done (#17); G9 partial (#18, #20, #22, #15); G12, G13 new; G11 up |
| 2026-10-31 | minor | Batch 1: G1 done (#2); G10 (#9) and G11 new; ranking moved |
| 2026-10-31 | major | First version, from swarm-core's template |
