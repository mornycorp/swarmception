<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Fill in the parts in comments like this one; keep the rules, which the swarms before
     you paid for (docs/lessons.md). -->
# Architecture

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-02 · **Change:** minor (model and effort compared per worker, #67; the panes share the window, the restored window on screen, #68; a gates pane, #61; the state survives a restart, #53; watching a live swarm, #54; the details pane and the timeline select together, #55; the factory's hover highlights a ticket's bars, #48) · **Target:** M1

Swarmception is built the way every swarm on Swarm Core builds: **an engine of typed commands that
every front end drives.** The UI, the CLI, the control channel and MCP are thin callers of one
command registry, so whatever a person can do, an agent can do and a test can check, without a
window.

## Layers

From the bottom. A crate depends only on crates in strictly lower layers; apps and tooling sit
on top and nothing depends on them. `cargo xtask layers` enforces it from the table in `xtask/`
(`Hooks::layers`) and writes the crate map in [AGENTS.md](../AGENTS.md#2-workspace-map).

| Layer | Holds | Never |
|---|---|---|
| **L0 model** | The domain's data and its pure algorithms: parsing hostile input, the core computations, no I/O, no clock | a UI crate, I/O, `SystemTime::now` |
| **L1 domain** | What the product computes over the model: layouts, statistics, queries | a UI crate, I/O |
| **L2 sources** (when there are any) | Where data comes from: files, git, the network, each call with a deadline and capped output | a UI crate |
| **L3 engine** | The `Session` and the **command registry**: every action is a typed command with an id | a UI crate |
| **L4 front ends** | `swarmception-ui` (the UI shell: panes, menus, UI state as data) and automation (the control channel's protocol, the headless server, MCP) | engine internals: only commands |
| **apps, tooling** | `swarmception` and `swarmception-cli`, the binaries that wire it together; `xtask` | (nothing depends on them) |

**Here** (the crate map is in [AGENTS.md](../AGENTS.md#2-workspace-map)):

- **L0 `swarmception-model`**: swarm-core's tracking format (docs/tracking-format.md there,
  version 3) as types: each line of `tracking/<instance>/work.jsonl` parsed into an `Event`
  (`Kind::SessionStart`, `Claim`, `SubStart`, …; an unknown event kept as `Kind::Other`), its
  UTC time a `Timestamp` with its own calendar arithmetic. A line that doesn't read is a
  `LineError` with its number; lines and notes are capped. `issues` (#17): a repository's GitHub
  issues and pull requests parsed from what `gh api --paginate …/issues?state=all` prints (an item
  that doesn't read is skipped and counted; an issue's PR is the newest whose body closes it or
  whose head branch is the issue's, `<n>-…`), the pulls' head branches, and `IssueCache`, their
  versioned JSON on disk, which reads from an older or newer build too and merges in what changed
  (#62).
- **L1 `swarmception-domain`**: `activity` reads each instance's sessions (split at its pauses),
  its work on issues, its helper sub-agents (a gate's `rc`, the PRs its note names) and its marks,
  and applies what another instance ends (`of` on `session.stop` and `release`), and keeps every
  issue event in time order. A gate that never logged its `sub.stop` ends at its coordinator's
  session's end or the next gate's start, whichever comes first (#42: `Sub::unfinished`, `never
  finished: …`), with no result; one whose session goes on with no gate after it still runs. `tickets` reads each ticket's life from them: when it was claimed,
  began, finished (its submit) and merged, and its stretches (claimed, working, waiting for merge,
  sent back, stopped); a claimed, working or sent back stretch ends where its holder's session
  ended, the ticket *abandoned* (#28: `Ticket::abandoned`, the bar's tooltip says whose session
  ended when, and the factory puts it on the shelf then). `rows` turns them into the rows asked for (`RowsSpec`: the tickets flat or
  by worker, collapsed), under two rows at the top, the merges onto `main` and the tickets'
  merges, each a 10 s span from its time (`MERGE_SECONDS`), all Ranges rows (`Node::Group` or
  `Node::Ranges`); each bar carries its label, its ticket (none for a merge onto main), its style,
  its colour and its tooltip, as plain text (`tooltip`) and as styled lines (`tooltip_lines`,
  #35: `tooltip::TipLine`, runs of text each in a colour from `colours` or none, strong or not:
  the phase names in their colours, merged green, sent back, its cause and abandoned red,
  durations and the title strong). `colours` is the one source of colours, the timeline's and the
  factory's (#15): a ticket's own colour from its issue number (its entry in a fixed, curated palette of
  eleven muted colours, the owner's scheme B (#40): consecutive ids never share one, none is
  near a phase's hue, dark labels legible on every one), in which its *working* bars and
  its merge are drawn; claimed yellow, waiting for merge blue, sent back red, stopped grey, merges
  onto main green. `timeline` builds
  swarm-timeline's host data (`TimelineData`) from the rows, each range in its bar's colour: the axis's items are the distinct
  times, with a `Time` (Spans) scale, positions in nanoseconds from the first time floored to
  the hour. It depends on `timeline-core` (pure) and nothing else of the widget. `stats` reads
  the swarm's statistics over a window (`Stats`): **tickets landed per active hour** (active:
  the time a worker has a session going, less its pauses and every gap of over 15 minutes with no
  event at all), its hourly series, the medians claim → merge and submit → merge, gate and
  `ready` pass rates with failures by cause, sendbacks, work in progress and each worker's
  utilisation; the module's docs define each figure.
  `factory` is the swarm at one moment *t* as a factory (#18): each ticket a crate in its own
  colour (`colours`, as on the timeline), up next, on the shelf, at its worker's bench, at the dock, in a gate run or shipped; each
  worker's actor and helper; the gate's runs (a merge whose gate logged no `sub.start` counts as a
  run; one that never finished has no result, counted in `Gate::no_result`, #42), main's blocks (each with its ticket's life up to the merge, #32: `took`, claim → merge;
  `segments`, its stretches with the gate runs naming it carved out of its waits; `sent_back`), and
  what happened in the `recent` seconds before *t*. Nothing after *t* is
  told; `titled` adds what GitHub says (each crate's title, its tooltip's title, state and labels
  as of *t*). `moments` (#33) are the factory's key moments, each named as a person says it
  (`#120 sent back by the gate · alder`): a claim, work beginning, a submit (the stretches'
  starts, so a resumed `work` is none), a gate run's pass or fail (a run that never said how it
  went is none), a send back, a merge onto main (a ticket's merge without a main); `step` finds
  the next or previous one of the kinds asked for, the moments at one second one step.
  `titles` (#17) is the text from GitHub's issues: a bar's label with its ticket's
  title (`#58 Architecture review`, always: no option since #34), and the lines a
  ticket's tooltips start with (title; state, labels, author; its PR); `rows_with` builds the rows
  with them, and each bar carries its PR. `timeline`'s host data gives a ticket's bar (and a
  merge onto main's) the context-menu items Open Issue and Open PR, as `Link`s by host item id.
  `details` (#47) tells the story of one `Subject` in time order: a ticket (its stretches with
  their times and durations, abandonment, its worker's `ready` checks, the gate runs that named
  it, releases, its merge and the commit onto main), a gate run (its start and note, its merges
  and send backs, the commits onto main, how it ended), a session (its claims, work, submits,
  pauses, helpers and gate runs) or a merge onto main; each `Entry` a line in its colour (the
  ticket's own for working and merges, the phases' fixed colours, green passed, red failed or
  sent back), with a header (chip, mark, title, GitHub's state, PR and labels), `Fact`s, the
  `Related` subjects it names and its `Page`s. `bar_subject` reads a timeline bar as its ticket
  or its merge onto main. `details_on` (#55) adds where the subject is drawn on the rows
  (`OnTimeline`: its bars as `rows::BarRef`s and the times they span): a ticket's bars; a gate
  run's merges onto main and its tickets' bars; a session's, the stretches its events began in
  its time, the tickets it merged and their merges onto main; a merge onto main's bar and its
  ticket's. `gates` (#61) is how the gate behaved over a window (`GatesList`): every run, the
  coordinator's gates (`factory::gate_runs`, inferred ones too) and the workers' `ready` checks
  in two `Lane`s, each with its start, end, duration, kind, `GateResult` (passed, failed with its
  cause, no result, running; as `stats.get` judges them), the PRs and tickets it carried, its
  merges (with their commits onto main) and send backs, the run it reran (`gate-43b` of
  `gate-43`, `-rerun`, the same id again), its colour, its tooltip lines and its details'
  `Subject`; each lane's `Figures` over the runs that started in the window (pass rate, median
  and longest run, busy time and share, reruns, failures by cause), per-hour figures, and the
  longest queue at the dock (the most tickets waiting for merge at once). `workers` (#67) compares
  the workers by the model and effort their `session.start` logged (`Comparison`): every worker
  session (an instance, not a coordinator) and, pooled, each model × effort (`Group`, its label
  `opus 5.5 · high`, more effort first, unknown last) and all of them: tickets claimed and landed,
  landed per active hour (active: on shift less the swarm's quiet gaps, as `stats`, and less its
  own idle time, a gap of over 15 min in its own events while it held no ticket claimed, working
  or sent back to it), the median claim → merge and working time, send backs per landed and their
  causes (`cause=conflict: …` is `conflict`), its `ready` checks' pass rate, tickets abandoned,
  and time on shift and active; each rate and median a `Measure` with the samples it rests on,
  `thin` under three (`THIN`). Each worker carries the tickets it touched in the window (what a
  hover lights) and its session as its details' `Subject`.
- **L2 `swarmception-source`**: the tracking branch through `git` (`refs/remotes/origin/tracking`,
  else `refs/heads/tracking`): `rev-parse`, `ls-tree`, one `cat-file --batch`, and `fetch`; and
  `main`'s first-parent merge commits (`git log --merges`, `origin/main` else `main`, or `master`);
  each call with a deadline and capped output, every path checked as an instance id. `fetch`
  takes `tracking` and `main` in one round trip (`tracking` alone from a remote with no `main`);
  `heads` reads the two refs' commits with a few `rev-parse`s, so a live check (#54) reads the
  branch again only when one moved (`Heads::of` a read's). `github`
  (#17): the `origin` remote read as GitHub's `owner/name`, the issues fetched with `gh api
  --paginate` (the same runner as git: a deadline, a cap, no prompts), and a cache per repository
  in the user's cache directory (`~/Library/Caches/swarmception/issues/<owner>--<name>.json` on
  macOS), written whole or not at all. `refresh` (#62) fetches every page (and the pulls', for
  their branches) only with no cache, one of another version or one a week past its last full
  fetch; else one page of what changed since the newest change it holds (`since=`), sent with the
  same request's ETag so a `304` costs nothing, merged in. No remote on GitHub, no `gh`, no
  network: a `GithubError`, and the cache as it was.
- **L3 `swarmception-engine`**: the `Session` (the repository, the activity, the rows spec, the
  rows and the timeline built from them) and the command registry: `engine.commands`,
  `repo.open`, `repo.reload`, `repo.fetch`, `repo.status`, `rows.get`, `rows.set`, `rows.list`,
  `timeline.timeAt`, `stats.get` (`{start?, end?}`, each a UTC time or epoch seconds, the whole
  run by default), `timeline.placeAt` (a time's position and item), `timeline.ticketBars` (a
  ticket's bars, to highlight), `factory.at` (the factory at *t*, and a ticket's journey), `factory.moments` (the key moments in
  a window, of the kinds asked for), `factory.step` (the next, previous, first or last key moment
  from a time: its time, its moments and its name),
  `ticket.urls` (a ticket's issue page and its PR's, and a number's page as a PR),
  `gates.list` (#61: `{start?, end?}` as `stats.get`; `gates::GatesList`), `workers.compare` (#67:
  `{start?, end?}` as `stats.get`; `workers::Comparison`),
  `details.get` (#47: exactly one of `ticket`, `gate`, `session`, `merge`, or a bar's `track` and
  `range`, with `at` to pick a rerun or a session; `details::Details` with its pages' URLs and,
  in `timeline`, its bars on the rows shown now (#55); a subject the swarm never had is
  `Refusal::NotFound`), `live.get`, `live.set` (`{on?, intervalSeconds?}`, 5 to 3600 s) and
  `live.check` (#54, `live`: the watch of a live swarm: on by default only when the latest event
  was under 30 min old at the first read; every interval, 30 s by default, a check from
  `Session::poll` in the background fetches `tracking` and `main` with a 30 s deadline and reads
  them again only when one moved; `LiveStatus` says when it last checked, what that cost
  (`LiveCost`: fetch, read, build, new events) and why the last failed, apart from the session's
  error, each failure in a row doubling the wait up to 10 min; a Reload or Fetch takes a check's
  place; a reload that names tickets the swarm hadn't before reads the issues again, no other
  does), `timeline.carryBars` (#54: where bars of an earlier generation of rows are now, by
  ticket, start and style; the last eight generations remembered). Every load
  (`repo.open`, `repo.reload`, `repo.fetch`) also reads the repository's GitHub issues (#17): the
  cache first, then `gh`, cached in turn, never in the way of the tracking branch; what came, from
  where and why not is `repo.status`'s `issues`, and new issues build the rows again. Where they
  come from (`gh`, the cache directory) is `Session::set_github`. A session made with a waker
  reads in the background and lands through `Session::poll`. The host API `Session::widget_timeline` hands the widget its timeline
  (`timeline_engine::timeline::Timeline`), rebuilt when `Session::revision`'s generation moves.
- **L4 `swarmception-ui`**: the shell: the menu bar (File: Reload, Fetch and Reload, Quit; View:
  the panes, the grouping, Collapse, Fit, Live), as data (`menu`: `MenuAction`, `MenuState`) that the
  in-window bar draws and `Shell::run_menu` runs; on macOS the app puts it in the system menu
  bar instead (`apps/app/src/mac_menu.rs`, muda), the same items through the same `run_menu`,
  their marks following `Shell::menu_state`; the toolbar (Font Awesome 5's solid face,
  `assets/`), the status bar, and the panes. The timeline pane owns swarm-timeline's widget
  (`timeline-egui`): it loads the engine's timeline when the rows move on, keeping the view
  (#54): a window at the timeline's end follows the new end at its width (the whole swarm shown
  stays the whole swarm), any other stays put; the selection and focus carried to the same bars
  (`timeline.carryBars`), the playhead to the same time, the highlight lit again on the new rows;
  and drives the widget through its own commands (`view.fit`, `state.set`). The toolbar's Live
  toggle (and View ▸ Live, ⌘L) is `live.set`, its interval beside it while on; the status bar says
  live and when it last checked, paused, or that the last check failed and when it tries again,
  the why on hover; the shell asks for a frame when the next check is due. On every load it fixes
  what the owner fixed (#35) by command: the `Time` scale and the absolute origin (`state.set`),
  the minimap always (`view.minimap`), a click selecting the nearest bar (`view.clickMode`); the
  widget's toolbar shows only `TIMELINE_TOOLBAR` (`toolbar.set`: Fit, Zoom to Selection, Undo
  Zoom, its zoom in / out / fit buttons, the measurement), and the app's own toolbar has no zoom
  buttons. The bars' tooltips reach the widget in colour (`timeline::tooltip_line`, the host
  data's `range_tooltip_lines`).
  The factory's header carries a statistics strip: what `stats.get` returns for the timeline's
  visible window (landed per active hour, its hourly sparkline with *t*'s hour marked, and the
  compact figures, each tooltip saying how it is computed), asked again only when the window or
  the data moves on. Clicking an hour's bar (#31) zooms the timeline's window to that hour
  (`TimelinePane::show_span`: `timeline.placeAt`'s positions, clamped to the extent, through the
  widget's `state.set`) and puts the factory's *t* and the playhead at its last second.
  The factory pane, under the timeline, draws what `factory.at` returns, its stations left to
  right and the key along the bottom in one row (two when narrow); the benches are short strips
  (name, model and helper on one line, the status under it) in more columns when many workers
  would not fit in one, the gate is its list of runs (✔/✖, tickets, cause, age) under what it is
  gating (#31). The tooltips are the scene's lines in colour (`factory_tip`: phases in their
  colours, passes green, failures red, durations emphasised, clock times quiet). Each ticket is
  the same chip at every station (its fill its colour, its border its phase, its size by station),
  gliding and resizing as *t* moves; hovering one rings it everywhere and highlights its bars on
  the timeline (#48: `timeline.ticketBars`, the widget's `highlight.set` through
  `Timeline::set_highlight`, cleared on leave; drawn as amber rings apart from the selection,
  which a hover never touches; the pane tells the widget only when the ticket or the rows'
  generation moves, so a reload under a hover highlights again), clicking one traces its
  journey; main's wall (#32) packs a brick per merge in rows from the ground up, each as wide as
  its ticket's claim → merge time against the median (no narrower than its number, no wider than
  four times the median's), a strip along its foot with its life's segments in
  their phase colours, a red corner when it was sent back (the count in it from two), shrinking
  to the smallest size and then scrolling with the wheel, the newest rows first; a merge onto
  main no ticket names is a small grey brick marked `!<pr>`; its *t* follows the timeline (the pointer, else the playhead,
  else the window's end) or plays through the window, the playhead following, or steps (#33):
  ⏮ / ⏭, and ← / → while the factory has the keyboard (a press in it takes it, as one on the
  timeline takes it there), put *t* at the key moment `factory.step` returns for the kinds
  chosen beside them (`FactoryView::steps`: every moment, or only claims, work, submits, gate
  runs, send backs or merges), the moment named beside the clock and the playhead following;
  shift-click, Home and End go to the first and last event.
  Right-clicking a bar (the widget's host items) or a chip offers Open Issue and Open PR: the
  shell runs `ticket.urls` and hands the page to egui's `open_url`. Bars always carry titles (#34);
  the status bar says how many issues came and from where,
  or warns why none did.
  The details pane (#47, View ▸ Details, on by default), right of the others, draws what
  `details.get` returns for `UiState::details`' subject: the header, the figures as pills, the
  subjects it carries as chips, then the story a line an entry (time, a dot in its colour, what
  happened in that colour, the rest, who, how long; notes under it). It follows the selection:
  the person's own bar on the timeline (`TimelinePane::chosen_bar`, not a factory hover's), a chip
  clicked in the factory (its traced ticket), a gate run clicked in the gate's list; and within
  the pane, a line, a name (their session) or a chip, ← going back. Its pages open as a bar's
  right-click does. The timeline selects with it (#55): the subject's bars (`details.get`'s
  `timeline`) are the widget's highlight, never the selection, and a new subject's bars are
  brought into view when none is (`TimelinePane::reveal`: the window moves, by `view.show`,
  centred on them at its width when they fit, else just holding them; the groups holding their
  rows open, by `tracks.expand`, when collapsed (#68: `details.get`'s `timeline.groups`); the
  rows scroll to the first of theirs under the merges rows, which a merged ticket is always on,
  when none is on screen). The highlight has one owner, `TimelinePane::light` (`light_hover`): a factory
  hover's ticket while it lasts, else a gates pane hover's run's tickets (`Hover`), else the
  details' bars, so a hover's leave gives them back (`TimelinePane::lit`, `ui.inspect`'s
  `timeline.lit`).
  The gates pane (#61, View ▸ Gates, on by default) is a band between the timeline and the
  factory drawing what `gates.list` returns for the timeline's window (asked again only when
  the window or the data moves): the headline figures as pills along its top (the gate's pass
  rate, median run, busy share, reruns, the longest queue, the `ready` checks' pass rate and
  median), then the runs along time under the timeline's rows (`TimelinePane::axis`: the window's
  ends across the screen, right of the label column), a lane of the coordinator's gate runs and
  one of the `ready` checks, each run a bar as long as it took in its result's colour (overlaps
  side by side), the gate's runs per hour stacked by result under them, and in the left column
  the lanes' names and the failures by cause, gate and ready. Hovering a run gives its tooltip
  and lights its tickets' bars; clicking it makes it the details' subject (a `ready` check's:
  its ticket). Without the timeline it shows the whole run on its own axis; before the timeline
  has data it waits. The factory's floor keeps within its pane, its header wrapping, so a narrower
  pane never cuts off main.
  The panes share the window (#68) by one rule, `layout`'s plain functions: the timeline keeps
  its least height (`TimelinePane::least`, `layout::timeline_least`: its toolbar, the rulers, the
  merges rows and six rows under them, all when fewer, and the scroller); the bands under it
  (`BandKind::ALL`, top to bottom: the Gates, the Factory; a pane that slots in under the timeline
  adds itself there) each ask for the person's height (`PaneSizes`, set only by dragging a band's
  top edge) or the window's default (`layout::defaults`: the Factory about a third of the height,
  the Gates 130, 170 on a tall window), and when they don't fit over the timeline's least they
  give way (`layout::share`), each in proportion to what it has over its own least (Gates 90,
  Factory 140), before the timeline does; they come back when the window grows. Each band folds to
  its header (28 points) with a click on its title's chevron, the Details to a strip (30 points);
  folded is the person's choice once made (`UiState::folded`, `layout::Folded`, saved), else the
  window's size (the Gates under 760 high, the Factory under 560, the Details under 1000 wide).
  Without the timeline the last open band fills the rest. `ui.inspect`'s `layout` reports what
  each got (`LayoutDrawn`: the timeline's height and least, each band's height, what it asked and
  whether it is folded, by default or by choice), `timeline.inView` the rows on screen, and
  `ui.set`'s `folded` folds or opens a pane.
  The workers pane (#67, View ▸ Workers, `--panes …,workers`, off by default) is a window over the
  others, opening up from the bottom over the bands so the timeline's rows stay in sight, and
  taking no room in the layout: a comparison is opened, read and put away. It draws what
  `workers.compare` returns for the timeline's window (or the whole run, its choice; asked again
  only when the window or the data moves): a row per model × effort (a swatch in its colour, the
  tickets' palette by its place), its workers under it (name, start, `open` for a session with no
  stop), then all workers; each figure with a small bar scaled to its column's largest, in the
  group's colour, and its samples (`n=…`); a thin one marked `~`, its bar only outlined. A
  header sorts by its column (again: the other way; none last either way; the first column: the
  command's order). Hovering a row lights its tickets' bars on the timeline (`Hover::Worker`,
  `Lit::HoveredWorker`, after a factory or gates hover); clicking a worker makes its session the
  details' subject. `--hover-worker <name>` puts the pointer on a worker's row.
  `UiState` (the panes shown and their sizes, the factory's follow, play, speed, *t* and the
  moments it steps by, what the details are of; version 5: the gates pane, #61; version 6: the
  folded panes, #68; version 7: the workers pane and its `WorkersView`, window or whole run and
  sort, #67) is serde data. `saved` (#53) keeps the
  app's state across a restart: `AppState` (version 1: the `UiState`, the rows spec, the window's
  size and place, Live as chosen (#54), and the timeline's `TimelineView`: the widget's own `state.save` without its
  playhead, the window as clock times, whether it was fitted or ended at the last event, the
  playhead's time) per repository (`repo_key`: its canonical top level), in
  `<dir>/state/<name>-<FNV-1a of the path>.json` under the platform's app-data directory
  (`~/Library/Application Support/swarmception` on macOS, `%APPDATA%`, `$XDG_STATE_HOME`), written
  whole when it changes and holds still a second (at most every five while it keeps changing,
  `Keeper`) and on quit. `StateStore::load` never fails: each section reads on its own, so a
  corrupt, another repository's, older or newer file keeps what it can, and the status bar says
  what was left out once (a click puts it away). The restored view comes back where it was, at
  the new end as wide when it ended at the last event, the whole swarm when fitted, the details'
  subject not moving it, the playhead at its time (kept across the loads after, as #54's reload
  keeps it), and Live as the person left it (`live.set`, only when they chose it); a window
  restored partly or wholly off the monitors winit lists (one since unplugged) is moved onto the
  one it overlaps most, else the nearest, shrunk to fit (`WindowPlace::onto`, #68);
  `ui.inspect`'s `saved` says where it is kept and was read from. `control` (#46) is the window's side of the control channel: `Shell::answer`
  runs `ui.commands`, `ui.inspect`, `ui.set` and `ui.screenshot`, forwards `timeline.<id>` to
  the widget and every other command to the engine ([Automation](#automation)).
- **L4 `swarmception-automation`** (#46): the control protocol as types (`protocol`: `Request`,
  `Reply`, `ProtocolError`, version 1), the loopback server (`tcp`), the UI thread's handler
  (`channel`), the headless handler (`session`), MCP over stdio (`mcp`) and a typed client
  (`client`). It depends on the engine and on swarm-timeline's `timeline-automation` (same pin),
  never on the UI: the app joins the two.
- **Apps:** `swarmception` (the window; `--repo`, `--grouping`, `--collapse`,
  `--panes`, `--details`, `--at`, `--hover`, `--hover-bar`, `--hover-gate`, `--hover-worker`, `--step`, `--steps`, `--size`, `--zoom` (the UI's
  scale: a 2560 × 1440 layout in a window the screen holds, #68), `--screenshot`, `--control`,
  `--control-file`, `--state-dir` (else `$SWARMCEPTION_STATE_DIR`, else the platform's), `--fresh`;
  a `--screenshot` or `--control` run keeps the state only in a directory named) and `swarmception-cli` (the engine headless: `commands`, `run`, `serve`,
  `mcp`).

Two sessions meet in the timeline pane: Swarmception's own (what the swarm did, the rows) and the
widget's (the view: zoom, pan, scale, selection). Nothing reaches across: the UI moves data from
one to the other through their host APIs and commands.

## How a command flows

```text
 menu click / shortcut ─┐
 control channel (TCP) ─┼─► the shell's dispatch ─► (pure UI state? handle it here) ─► Session::execute(id, params)
 headless / CLI / MCP ──┘                                                                  │
                                                       registry: find(id) ─► enabled? ─► run ─► typed result
```

- **Everything is a command.** New user-visible behaviour is an engine command first: an id
  (`area.name`, lowerCamelCase segments), a label, its menu path and shortcut if it has them, an
  `enabled` check that says why not, and `run`. The UI, the CLI, the control channel, the headless
  server and MCP all dispatch by id: a command with a menu path shows in the menu with no UI
  change, and is an MCP tool with no MCP change. Only pure view state (theme, open windows) is the
  shell's own.
- **`Session::execute`** checks the params and enablement, catches a panic inside a command (an
  error; the session stays usable) and journals the commands that change state.
- **The registry lists itself.** `engine.commands` returns every command with its params and
  result schemas, which the CLI's help and MCP's tools are built from.

## Typed contracts (from the first commit)

Every boundary another tool or agent crosses is a type: command params and results, control
messages, persisted state, errors. A swarm that started untyped built 100+ commands in a week and
then spent seven slices typing them ([lessons.md](lessons.md) §1).

- **Commands are typed.** A command is a params type (`Deserialize`, a JSON Schema, unknown
  fields refused) and a result type (`Serialize`, a JSON Schema), registered through one typed
  adapter. The adapter bounds and parses the params and serializes the result: no command reads
  raw JSON.
- **Consumers read typed results.** The UI, the CLI, automation and tests deserialize a reply into
  its result type; they never index a `serde_json::Value` (`v["field"]`, `.get("field")`,
  `.as_object()`). `cargo xtask layers` refuses a new one in the directories the table names.
- **Errors are typed**, each variant with a stable error code in transport replies.
- **Persisted state carries a version**, and a state from an older or newer build still reads,
  keeping every section it can.

## The UI

- **Thin and data-driven.** The UI's state is plain data (serde), so the control channel reads it
  (`ui.inspect`) and drives it (`ui.set`), and the app saves and restores it.
- **What is drawn is what a command returns.** A pane draws a command's result; a test checks
  that what it drew equals the command's answer. Nothing the UI decides is untestable headless.
- **Colours from theme tokens, text from a string catalog**, never literals.
- **Never block the UI.** Slow reads run in the background and land through the session's poll;
  the CLI and agents run them in the foreground.
- **Verify visually.** Render offscreen (a snapshot example writing a PNG, driven by a script of
  commands) or launch with the control channel and drive it; attach before and after images to
  the PR.

## Automation

- **The CLI** (`swarmception-cli`) runs the engine headless: list the commands, run one or a script of them
  (`swarmception-cli run …`), serve the control protocol, serve MCP. It is how an agent checks a change
  without a window, and the smoke gate's headless run. It is also the window's client (#60):
  `send` and `mcp --control`, below.
- **The control channel** (#46, L4 `swarmception-automation`, after swarm-timeline's
  `timeline-automation`, whose line reader, token, loopback bind and limits it reuses):
  `swarmception --repo <r> --control [<port>] [--control-file <path>]` listens on 127.0.0.1 (any
  free port by default) with a fresh 128-bit token, and prints `{"address":"127.0.0.1:<port>",
  "token":"<hex>"}` as one line on stdout (and writes it, owner-only, to the file). JSON lines in
  and out, a line at most 1 MiB:

  ```text
  → {"v":1,"token":"<hex>","id":7,"command":"ui.set","params":{"at":"2026-11-01T10:56:34Z","ticket":172}}
  ← {"id":7,"result":{…}}
  ← {"id":7,"result":null,"error":{"problem":"engine","error":{"code":"invalid","refusal":{"reason":"badTime","text":"teatime"}}}}
  ```

  `command` is an engine command's id (`engine.commands` lists them with their schemas); the
  window's own `ui.commands` (the window's commands, then the widget's as `timeline.<id>`, each
  with its params' and result's schemas; the widget's registry says nothing of read-only, so
  none of its is), `ui.inspect` (`swarmception_ui::control::Inspect`: the shell's
  `UiState`, and what the panes show: the factory's *t*, where it came from, the key moment, what
  each station drew last frame, the timeline's scale, window, playhead, selected ranges and
  highlighted ones),
  `ui.set` (`UiSet`: panes, speed, steps, `at`, follow, playing, a ticket traced and its bars
  selected, a step, the details' subject or none (#55), the window; `ui.inspect`'s `details`
  report its subject, title, error and bars), `ui.loaded` (`{"within": <s>}`, at most 19:
  answered with `ui.inspect`'s answer once nothing is loading, the issues are in and the
  timeline has its data, or the read failed, or there is no repository; a `ui` error after
  `within`) and `ui.screenshot` (`{"path": "<file>.png"}`, PNG, or a binary PPM for `.ppm`;
  taken a few frames later so a `ui.set` has settled, answered with its size once saved;
  `swarmception_ui::shot`); or `timeline.<id>`, the
  timeline widget's own command forwarded (`timeline.view.fit`, `timeline.state.get`,
  `timeline.ui.inspect`; `timeline.engine.commands` lists them: the engine's own `timeline.*` ids
  have one dot). The errors are `ProtocolError`, tagged `problem`: `bad_request`, `too_long`,
  `wrong_version`, `bad_token`, `no_ui`, `unavailable`, `busy`, `engine` (the engine's
  `EngineError`), `widget` (the widget's), `ui`. A connection whose first line lacks the token is
  answered and closed; at most eight connections, ten seconds for the first line. Requests reach
  the UI thread through a channel (`channel::ChannelHandler`), which `Shell::answer` answers
  between frames. `client::Client` is the typed client tests use.
- **Headless:** `swarmception-cli [--repo <r>] serve [--port <p>] [--control-file <path>]` speaks
  the same protocol on its own session; `ui.*` and `timeline.<id>` are `no_ui` there.
- **MCP:** `swarmception-cli [--repo <r>] mcp` serves every engine command as an MCP tool over
  stdio (newline-delimited JSON-RPC 2.0: `initialize`, `tools/list`, `tools/call`, `ping`), named
  with `_` for `.` (`factory_at`, `timeline_view_fit`: no id has a `_`, which a test checks); a
  command's error is the tool's `isError` with its `ProtocolError` as its text (the engine's as
  `{"problem":"engine","error":…}`). `mcp::serve` runs over any `Handler`: the headless session,
  or a window's.
- **MCP through the window** (#60): `swarmception-cli mcp --control <file>`, the file the app's
  `--control-file` (or `serve`'s) wrote, lists `engine.commands` and `ui.commands` as tools, with
  their schemas, and runs each call in that window over its control channel
  (`client::Remote`): `ui_set`, `ui_inspect`, `ui_loaded`, `ui_screenshot` (which writes the PNG
  itself), `timeline_view_fit`, `factory_at` on the window's session. A relative screenshot path
  is the MCP server's directory, not the window's. A connection the window closed (ten idle
  minutes) is opened again; a window gone is the tool's `unavailable` error.
- **The CLI client** (#60): `swarmception-cli send --control <file> <id> [<params JSON>]` sends one
  request and prints its result; `send --control <file> --script <file.jsonl>` sends a script
  (`script::Step` a line, `{"command": <id>, "params": {…}}`; blank lines and `#` lines skipped),
  printing each result as a line and stopping at the first error, which goes to stderr with its
  `ProtocolError` as JSON. Exit codes: 0 every request answered; 1 the window answered with an
  error; 2 bad usage or a script that doesn't read (nothing sent); 3 no window to reach (no
  control file, nothing listening, no answer in time). `scripts/screenshots.jsonl` takes the
  screenshots a PR needs, and `scripts/screenshots.sh <clone> [<out dir>]` starts the app with
  `--control`, sends it that script and closes it.

## Never crash

A malformed file, a bad command or control param, a corrupt saved state or an odd keystroke is an
error the user or agent can act on, never a panic.

- Non-test code never panics (`unwrap`, `expect`, `panic!`, `unreachable!`, `todo!`); no
  `unsafe`. Every crate root says so, and `cargo xtask layers` checks it.
- Input-derived numbers are hostile: `get()` not `[i]`, `checked_*`/`saturating_*`, division and
  NaN guarded, allocations and strings capped.
- Nothing reaches a command line unchecked; every external call has a deadline.
- A **panic hunt** test runs every command with adversarial params and must stay green; every
  crash fix comes with the test that panicked before it.

## Adding a command

1. **The algorithm** goes in the lowest layer that fits, with its own tests and no UI.
2. **The command** goes in the engine (`<area>_cmds.rs`), typed, registered in one line.
3. **Tests:** behaviour, disabled states and bad params (each an `Err`, never a panic), reading
   the reply as its result type; the panic hunt picks it up.
4. **UI** only if it needs a pane or dialog, its state in the UI state, its text in the catalog.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-02 | minor | Model and effort compared per worker (#67): the domain's `workers`, `workers.compare`, the Workers pane as a window (View ▸ Workers, `--panes …,workers`, `--hover-worker`), `Hover::Worker`, `UiState` version 7, `ui.inspect` version 5 with `workers` |
| 2026-11-02 | minor | A gates pane (#61): the domain's `gates`, `gates.list`, the Gates pane under the timeline (View ▸ Gates, `--panes …,gates`, `--hover-gate`), `TimelinePane::light_hover` and `axis`, `UiState` version 5, `ui.inspect` version 3 with `gates` |
| 2026-11-02 | minor | Automation reaches the window (#60): `mcp --control` (tools through a running window, over `client::Remote`), `send` and its scripts (`script`), `ui.screenshot` and `--screenshot` as PNG (`swarmception_ui::shot`), `ui.commands` listing the widget's commands, `ui.loaded`; an MCP tool's error is its `ProtocolError`; `scripts/screenshots.{jsonl,sh}` |
| 2026-11-02 | minor | The state survives a restart (#53): `saved` (`AppState` version 1, `StateStore`, `Keeper`), `UiState` version 4 with the panes' sizes, the timeline pane's `TimelineView`, `ui.inspect` version 2 with `saved`; `--state-dir`, `--fresh` |
| 2026-11-02 | minor | Watching a live swarm (#54): `live.get` / `live.set` / `live.check`, the source's one-trip `fetch` and `heads`, a reload that keeps or follows the view and carries the selection (`timeline.carryBars`), Live in the toolbar, View menu and status bar; `ui.inspect`'s `timeline.loadMs` |
| 2026-11-02 | minor | The details pane and the timeline select together (#55): `details.get`'s `timeline` (the subject's bars), highlighted with one owner (a factory hover wins while it lasts) and brought into view; `ui.set`'s `details`, `ui.inspect`'s `details` and `timeline.lit` |
| 2026-11-02 | minor | The factory's chip hover uses the widget's highlight (`Timeline::set_highlight`), not the selection; `ui.inspect`'s `highlighted`; swarm-timeline pinned at 9bf2651 (#48) |
| 2026-11-02 | minor | The details of what is selected (#47): the domain's `details`, `details.get`, the details pane following the timeline's and the factory's selection; `--details` |
| 2026-11-02 | minor | Automation (#46): `swarmception-automation`, the app's `--control`, `ui.inspect` / `ui.set` / `ui.screenshot`, the widget's commands as `timeline.<id>`, `swarmception-cli serve` and `mcp` |
| 2026-11-01 | minor | A gate that never logged its stop ends at its coordinator's session's end or the next gate's start, with no result (#42); the factory's working border a warm white and in-the-gate a magenta, apart from scheme B |
| 2026-11-01 | minor | The factory steps by key moment, back and forth (#33): the domain's `moments`, `factory.moments` and `factory.step`; ⏮ / ⏭ and ← / →, the moment named beside the clock |
| 2026-11-01 | minor | The timeline's toolbar shows only what Swarmception needs; the scale, origin, minimap and click fixed by command; the bars' tooltips in colour from the domain (#35) |
| 2026-11-01 | minor | Tickets' colours from a fixed, curated palette (scheme B, #40), the phase colours retuned to sit with it |
| 2026-11-01 | minor | A gate that never logged its stop ends at its coordinator's session's end or the next gate's start, with no result (#42); the factory's working border a warm white and in-the-gate a magenta, apart from scheme B |
| 2026-11-01 | minor | Main's wall (#32): bricks sized by claim → merge, their phases along them, sendbacks marked; the blocks carry `took`, `segments`, `sent_back` |
| 2026-11-01 | minor | The factory's layout (#31): no ticker, the key along the bottom, short bench strips, the gate as its list of runs, coloured tooltips, the sparkline's hours clickable; the factory's own colours softened |
| 2026-11-01 | minor | An abandoned ticket: a claimed, working or sent back stretch ends where its holder's session did (#28) |
| 2026-11-01 | minor | The repository's GitHub issues: fetched with `gh`, cached per repository; titles on bars and first in tooltips; `ticket.urls` and the bars' and chips' Open Issue / Open PR (#17) |
| 2026-11-01 | minor | A colour per ticket from `colours`
, shared with the factory; the top rows as 10 s spans; bars carry their ticket and colour, no more pins (#15) |

Older rows: `git log -p docs/architecture.md`.
