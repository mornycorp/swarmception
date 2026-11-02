<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Gaps

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-02 · **Change:** minor (batch 4: the control channel and MCP, the Details pane, the widget's highlight and durations) · **Target:** M2

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
| 1 | [G5 The state survives a restart](#g5-the-state-survives-a-restart) | open | | 2–4 h |
| 2 | [G4 Watching a live swarm](#g4-watching-a-live-swarm) | open | | 2–4 h |
| 3 | [G16 The details pane and the timeline select together](#g16-the-details-pane-and-the-timeline-select-together) | open | | 2–4 h |
| 4 | [G15 Automation reaches the window](#g15-automation-reaches-the-window) | open | | 2–4 h |
| 5 | [G9 Swarm analytics panes](#g9-swarm-analytics-panes) | partial | #18, #20, #22, #31–#33, #42 | 3–6 h |
| 6 | [G13 Issues fetched incrementally](#g13-issues-fetched-incrementally) | open | | 1–2 h |
| 7 | [G6 Collapse to the window](#g6-collapse-to-the-window) | open | | 2–4 h |
| 8 | [G8 Opening a repository from the app](#g8-opening-a-repository-from-the-app) | open | | 1–2 h |
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

- **Missing:** every load (Reload too) runs `gh api --paginate` for all the repo's issues and
  PRs: about 1.4 s for mc-engine's 166 (#17, PR #26, measured 2026-11-01). A `since=` or ETag
  fetch over the cache would make a reload near free on a big repo.
- **Left:** also linking an issue to its PR by branch name, not only closing keywords and the
  tracking branch's `mr`.
- **Evidence:** `crates/source/src/github.rs`, `crates/model/src/issues.rs`.
- **Estimate:** 1–2 agent-hours.

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

- **Missing:** after G2 (#46, PR #49) an agent drives the window over `--control` by hand: MCP
  serves the headless engine only, so its tools don't reach a running window; `ui.screenshot`
  writes PPM only (no PNG encoder in the lock); there's no `swarmception-cli` subcommand that
  sends a series of requests to a running app; the widget's forwarded commands aren't in
  `ui.commands` (`timeline.engine.commands` lists them).
- **Left:** `swarmception-cli mcp --control <file>` (tools for `ui.*` and `timeline.*` through a
  running window); PNG screenshots; a CLI client (`swarmception-cli send` / a script of steps);
  the widget's commands listed with schemas.
- **Evidence:** `crates/automation`, `apps/cli`.
- **Estimate:** 2–4 agent-hours.

## G16 The details pane and the timeline select together

- **Missing:** after G3 (#47, PR #50) the Details pane follows the timeline, but not the other
  way: selecting a subject in the pane (a line, a name, a chip, ←) doesn't select or highlight
  its bars on the timeline; a session can't be chosen from the timeline (the widget has no
  group-row selection); `ui.set` has no `details` field.
- **Left:** the pane's subject highlights (or selects) its bars; a session from its worker's row
  (a widget change); `ui.set {details}`.
- **Evidence:** `crates/ui/src/details.rs`, `crates/ui/tests/details_pane.rs`.
- **Estimate:** 2–4 agent-hours, part in swarm-timeline.

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

- **Missing:** the app reads the branch when asked (Reload, Fetch and Reload). Watching a swarm
  at work wants a fetch on an interval, keeping the view (or following the latest time).
- **Estimate:** 2–4 agent-hours.

## G5 The state survives a restart

- **Missing:** the rows spec, the panes, the window and the widget's saved state
  (`state.save`) start afresh each run.
- **Estimate:** 2–4 agent-hours.

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

- **Missing:** File › Open… (a folder picker) and recent repositories; today `--repo` or the
  current directory.
- **Estimate:** 1–2 agent-hours.

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
    active hour (active = a worker on shift, less gaps over 15 min with no events): mc-engine 7.2
    (98 in 13.69 h), swarm-timeline 10.6 (86 in 8.12 h) (measured 2026-11-02).
  - **Colours** (#15, PR #25; #34, PR #36; #40, PR #41): one colour per ticket from a fixed
    11-colour palette (scheme B, the owner's choice), shared by the timeline and the Factory, kept
    clear of the phase colours.
- **Left:** a gates pane (runs, causes, durations over time); model and effort compared per
  worker.
- **Evidence:** `crates/domain/src/factory.rs`, `moments.rs`, `stats.rs`, `colours.rs`;
  `crates/ui/src/factory*.rs`.
- **Estimate:** 3–6 agent-hours, in issues of their own.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-02 | minor | Batch 4: G2, G3, G12, G14 done (#46–#48, swarm-timeline#183, #188); G15, G16 new; focus G5, G4, G16 |
| 2026-11-01 | minor | Batch 3: G10, G11 done (#9, #28); G9 grew (#31–#33, #40, #42); G14 new; focus G2, G3, G12 |
| 2026-11-01 | minor | Batch 2: G7 done (#17); G9 partial (#18, #20, #22, #15); G12, G13 new; G11 up |
| 2026-10-31 | minor | Batch 1: G1 done (#2); G10 (#9) and G11 new; ranking moved |
| 2026-10-31 | major | First version, from swarm-core's template |
