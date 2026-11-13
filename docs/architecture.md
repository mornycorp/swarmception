<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Fill in the parts in comments like this one; keep the rules, which the swarms before
     you paid for (docs/lessons.md). -->
# Architecture

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-13 · **Change:** minor (typed egui ids, #163; one file writer and one process runner in `source`, `cargo xtask writes`, #164; File's commands from the registry, `engine.states`, #160; `repo.open` checks the folder in the background, #161; a saved state this build can't wholly read is left alone, #165; the swarm's colours in both looks, `Paint` and `theme.set`, #154; colours from swarm-core's theme, `theme.get`, #152; group idle benches and opt-in X row constraint, #147; reconcile main merge evidence and bound stopped/unresolved ticket histories, #142; Alt + click on a ticket's bar shows its end, by the pin to swarm-timeline 7e372fe, #137; the minimap draws the tickets' strips, #134; a sent-back ticket's red corner, merge dots optional, a red dot where given up, #127; the Gates band removed, #128; a stage named everywhere in its strip's colour, `Stage::colour` the one source, and no Gate runs or Ready checks rows, #117; merges as markers of no length, an item of no width each, #115; a ticket is one bar in its own colour, its stages a strip along its foot, its send-backs and merge marks, by the pin to swarm-timeline da78ff5 (its glyphs and strips), #110; a gate's failure reads as a send-back, the tracking format's rule as swarm-core's `prwatch` reads it; the Factory's benches in natural order, the fold of those off shift opening on a click; the slow live test steady, #106; each row's lanes sized over the window, the widget's `view.lanes`, and the minimap's fold marks, by the pin to swarm-timeline 3692af8, #97; background reads under load: a last wake when a read is over, tests that wait on it, refs looked up in one git call, #96; ready checks in the Factory's Gate list, a selected run drawn selected after *t*, #95; the Factory's benches with dozens of workers: off shift folded, the rest scrolling, #94; folding the timeline's quiet time: the widget's `view.fold`, the toolbar's checkbox and View ▸ Fold Quiet Time, the Gates band's hours through the folds, #86; the Factory follows the timeline: no clock of its own, a fixed header, the timeline's selection, the strip's own range, #85; gate runs and ready checks as rows on the timeline, #84; a submit straight after a send-back is its rework, #83; a failed `ready` check's cause and step, #77; Collapse to the visible window, #74; `ui.set` reaches the Workers window, MCP returns images, Open… off macOS, #76; one definition of a gate run, the Gates band's hours for both lanes, #75; opening a repository from the app, #69; model and effort compared per worker, #67; the panes share the window, the restored window on screen, #68; a gates pane, #61; the state survives a restart, #53; watching a live swarm, #54; the details pane and the timeline select together, #55; the factory's hover highlights a ticket's bars, #48) · **Target:** M1

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
  sent back, stopped). `Activity::with_merges` also supplies main's full merge messages:
  explicit local closing lists (`Closes #12, #3, and #9`) and matching PR/MR identities end
  unfinished tickets even when tracking omitted their merge. GitLab's `See merge request`
  trailer identifies the landing batch. Recorded tracking merges retain their provenance;
  recovered ones link to the main commit. Incidental issue mentions are not closure evidence.
  A paused/blocked/handoff stop is a finite instant; the factory shelves it with its stop reason, and
  later work resumes normally. Unfinished history with no ticket update for 24 hours, whose
  holder has ended its session or also been silent for 24 hours, ends at its last ticket
  observation with **outcome unknown**. This neither claims a merge nor declares an agent dead;
  fresh waiting stays open, and recorded later merges preserve genuine long waits. Staleness
  uses the last recorded swarm event, never the viewer's clock.
  A claimed, working or sent back stretch ends where its holder's session
  ended, the ticket *abandoned* (#28: `Ticket::abandoned`, the bar's tooltip says whose session
  ended when, and the factory puts it on the shelf then). A worker's `submit` straight after a
  send-back, with no `work` logged between, is its rework (#83: `Stretch::rework`): the send-back
  keeps a minute's mark (`SENT_BACK_MARK`), the rest up to the submit is *working* again, so a
  ticket's second run is drawn in its own colour as its first was. **A send-back** is the
  tracking format's (its Send-backs, #106: `sendbacks`): a `sendback` event, or a gate (`gate…`)
  that failed for the change's cause (`rc≠0`, a cause not the machine's, `sendbacks::TRANSIENT`)
  with no later gate saying it reruns it (`rerun of <sub>:`), which sends back every ticket its
  start note names before its first `;` at its stop, unless the ticket merged since it was last
  claimed or submitted; a `sendback` after that failure and before the ticket's next gate is the
  same send-back, in the coordinator's words. `Activity::of` adds each to `issue_events` as a
  `sendback` at the gate's stop and drops the events it answers (`sendbacks::told`), so the
  tickets, the timeline, the details, the factory and `stats.get` read one list; a test checks it
  against swarm-core's pinned `prwatch::sendback`. `rows` turns them into the rows asked for (`RowsSpec`: the tickets flat or
  by worker, collapsed: `Collapse` off, `empty` (no events at all) or `window`, #74, which `true`
  and `false` still read as), under two rows at the top, the merges onto `main` and the tickets'
  merges, each a marker of no length at its time (#115: its bar's start its end; `TOP_KEYS` the
  two). The gate's runs have no rows (#117, the owner's, reversing #84's Gate runs and Ready
  checks): a run naming a ticket is the ticket's *in the gate* stage, a failure its send-back; a
  saved state naming the old rows' keys (`gates`, `ready`) reads on without them. A ticket is one
  bar a row (#110, `Style::Ticket`): from its first stretch to its last, in its own colour all
  along, labelled once; its stages (`Stage`: claimed, working, waiting for merge, in the gate
  where a gate run naming it ran, sent back, stopped) are its `segments`, each in the factory's
  phase colour (`colours::WORKING` and `GATING` moved to the domain) and importance, which
  `timeline` gives the widget as the range's strip along its foot (swarm-timeline §6.6); sent
  back, it carries the factory's red corner (#127, `Bar::corner`, `BarCorner`: the count when
  more than once), the widget's range corner (§6.6); its `marks`, the widget's glyphs (§11.5),
  are a red dot at its end where it was given up (`MarkKind::GivenUp`: abandoned, or closed on
  GitHub without a merge; not one stopped `done`) and, only when `rows.set`'s `mergeMarks` asks
  (View ▸ Merge Marks; `RowsSpec::merge_marks`, saved with the rows), a green dot at its merge;
  its importance is its most important stage's; all Ranges
  rows (`Node::Group` or `Node::Ranges`); each bar carries its label, its ticket (none for a merge onto main), its style,
  its colour and its tooltip, as plain text (`tooltip`) and as styled lines (`tooltip_lines`,
  #35: `tooltip::TipLine`, runs of text each in a colour from `colours` or none, strong or not:
  each stage named in its strip's colour, `Stage::colour` the one source (#117: the strip's
  segments, the widget's segment line and its ■ swatch, the tooltip's stretch lines and
  milestones and the details' story all call it; a wait's gate runs listed under it, *in the
  gate*, by id), merged green, sent back, its cause and abandoned red, durations and the title
  strong). `colours` is the one source of the swarm's colours, the timeline's and the
  factory's (#15), each from `theme` (#152, the owner's: swarm-core's skeleton theme as it is,
  its docs/app-architecture.md › Colours from the theme; since #158 the palette the swarm's
  tools share, swarm-core#100): a ticket's own colour is
  `RANGE_32`'s pick of its issue number (FNV-1a over its digits, the same on every run and
  machine), in which its bar and its merge are drawn; a stage takes the token named for what
  it means: claimed `warning`, working `strong`, waiting for merge `link` (the
  shared blue that reads as text: its `accent` carries none), in the gate
  Swarmception's `gate`, sent back `error`, stopped `muted`, merges onto main `success`; an idle
  bench Swarmception's `idle`; the Workers pane's groups the `SERIES` by number (entry `n % 7`).
  The theme's tests hold each value to the shared palette's, measure each token's contrast where
  it is read (4.5:1 for text, both looks), each range entry's (3:1), every shortfall of the
  shared palette's kept and recorded with its ratio (`KEPT_SHORT`), the gate and idle nudged to 4.5:1,
  strong text on each entry made a text background, the ranges' spacing (CIEDE2000) and the
  pick's pins; `theme.get` returns it (each token's name, meaning, dark and light; each range's
  entries), swarm-core's `ui.theme` under the engine's name, as `ui.*` is the window's. Each of
  the swarm's colours is a `Paint` (#154): its dark and its light value, the theme's pair, so a
  command's answer carries both; the UI draws the one egui's look takes (`factory_chip::drawn`),
  and the session builds the timeline's data in its look (`theme.set {"look": "dark"|"light"}`,
  which the shell sends when egui's look changes; the same look changes nothing). `timeline` builds
  swarm-timeline's host data (`TimelineData`) from the rows, each range in its bar's colour: the axis's items are the distinct
  times, with a `Time` (Spans) scale, positions in nanoseconds from the first time floored to
  the hour; a time with a bar of no length at it (a merge, #115) has an item of no width first,
  which is that bar's, so it has no length on `Time` (the widget packs it to 3 px, its §6.1) and
  is one event wide on Unit. It depends on `timeline-core` (pure) and nothing else of the widget. `stats` reads
  the swarm's statistics over a window (`Stats`): **tickets landed per active hour** (active:
  the time a worker has a session going, less its pauses and every gap of over 15 minutes with no
  event at all), its hourly series, the medians claim → merge and submit → merge, gate and
  `ready` pass rates with failures by cause (the runs `gates` defines, counted as `gates.list`
  counts them, #75), sendbacks, work in progress and each worker's
  utilisation; the module's docs define each figure.
  `factory` is the swarm at one moment *t* as a factory (#18): each ticket a crate in its own
  colour (`colours`, as on the timeline), up next, on the shelf, at its worker's bench, at the dock, in a gate run or shipped; each
  worker's actor and helper. `FactoryScene::idle` groups idle benches with no current/recent
  helper into one tile, retaining individual benches for tools. Its tooltip lists every worker
  key and explains that an unpaused session without bench work does not prove availability:
  missing session stops may leave old workers idle. Paused, off-shift, busy and checking workers
  are excluded. `ui.inspect` version 18 reports the tile's members as `idleBenches`;
 the gate's runs (a merge whose gate logged no `sub.start` counts as a
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
  pauses, helpers and gate runs) or a merge onto main; each `Entry` a line in its colour (a stage
  in its strip's, #117; the ticket's own for its merges, green passed, red failed or sent back),
  with a header (chip, mark, title, GitHub's state, PR and labels), `Fact`s, the
  `Related` subjects it names and its `Page`s. `bar_subject` reads a timeline bar as its ticket
  or its merge onto main.   `details_on` (#55) adds where the subject is drawn on the rows
  (`OnTimeline`: its bars as `rows::BarRef`s and the times they span): a ticket's bars; a gate
  run's merges onto main and its tickets' bars; a session's, the stretches its events began in
  its time, the tickets it merged and their merges onto main; a merge onto main's bar and its
  ticket's. `gates` (#61) is how the gate behaved over a window (`GatesList`): every run, the
  coordinator's gates (`factory::gate_runs`, inferred ones too) and the workers' `ready` checks
  in two `Lane`s, each with its start, end, duration, kind, `GateResult` (passed, failed with its
  cause, no result, running); a failed `ready` check's stop also names the `ci` step it failed at
  (`rc=<n>; cause=<cause>; step=<step>`, swarm-core's `ready`, #77: `Run::step`), and its
  failures count by `stats::failure_label` (`build/clippy`; an older stop's cause unknown,
  `unstated`). **One definition of a gate run** (#75): `gates::runs` reads them
  and `gates::figures` counts them, for `gates.list` and `stats.get` (`Stats::gates` and `ready`,
  `From<&Figures>`) alike, so one window gives one count from both; a gate that logged no
  `sub.start`, read from the merges and send backs naming it, counts, and each lane's
  `inferred` says how many did. Each run has the PRs and tickets it carried, its
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
- **L2 `swarmception-source`**: **the layer that touches the outside** (#164, swarm-core's
  contract's `backends`): the one atomic file writer (`files::write_atomic`, `write_private`: a
  temporary file beside the target, synced, renamed over it) and the one place a program starts
  (`process::Program`: a deadline, a cap, its output read aside), which git, `gh`, the window's
  state and recent list, the issues' cache, the control channel's token file, screenshots and the
  folder picker all go through; `cargo xtask writes`, a step of `ci`, fails on a write or a
  program started anywhere else outside tests. Then the tracking branch through `git` (`refs/remotes/origin/tracking`,
  else `refs/heads/tracking`): `rev-parse`, one `cat-file --batch-check` for every ref it looks
  for (#96: a git child a ref was most of a read's time on a loaded machine), `ls-tree`, one
  `cat-file --batch`, and `fetch`; and
  `main`'s first-parent merge commits (`git log --merges`, `origin/main` else `main`, or `master`);
  each call with a deadline and capped output, every path checked as an instance id. `fetch`
  takes `tracking` and `main` in one round trip (`tracking` alone from a remote with no `main`);
  `heads` reads the two refs' commits with two git calls, so a live check (#54) reads the
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
  `repo.open` (#69: the folder checked first, `repo_cmds::check`, two git calls through the
  source's `probe`: a missing folder, a file, a folder in no git repository and one with no
  tracking branch are `Refusal::NoSuchFolder`, `NotADirectory`, `NotARepository` and
  `NoTrackingBranch`, and nothing changes, the repository open staying open; opened, the old
  read and watch are dropped, Live starts afresh at the new repository's default,
  `Session::opens` moves and the repository goes first in the recent list), `repo.recent` and
  `repo.clearRecent` (#69: the repositories opened, newest first, ten at most, each with its
  name and whether it is still there; a front end hands back what it kept with
  `Session::remember_all`), `repo.reload`, `repo.fetch`, `repo.status`, `rows.get`, `rows.set`, `rows.list`,
  `rows.window` (#74: `{start?, end?, keep?}`; collapsed to the window, the rows and groups a
  window leaves out, `rows::left_out`: a row none of whose bars overlaps it unless `keep` names
  it, a group with no row left in place of its rows; and those it shows, top to bottom),
  `timeline.timeAt`, `timeline.bar` (#84: `{track, range}`, the bar there; `timeline.runBars`
  gone with the gate's rows, #117), `stats.get` (`{start?, end?}`, each a UTC time or epoch seconds, the whole
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
  place; every check that read the branch again (new events, or a merge onto main) reads the
  issues again in the background, so PRs' states keep up (#69: `LiveCost::issues`; with #64's
  incremental fetch about 0.5 s on mc-engine, off the UI thread)), `timeline.carryBars` (#54: where bars of an earlier generation of rows are now, by
  ticket, start and style; the last eight generations remembered). Every load
  (`repo.open`, `repo.reload`, `repo.fetch`) also reads the repository's GitHub issues (#17): the
  cache first, then `gh`, cached in turn, never in the way of the tracking branch; what came, from
  where and why not is `repo.status`'s `issues`, and new issues build the rows again. Where they
  come from (`gh`, the cache directory) is `Session::set_github`. A session made with a waker
  reads in the background and lands through `Session::poll`; it is woken after each piece that
  lands and once more when a read is over (#96), so a front end that draws only when woken sees
  it end. The engine's tests block on that waker (`tests/support`), the UI's frame until a
  condition with a deadline only a failure reaches, never a count of tries. The host API `Session::widget_timeline` hands the widget its timeline
  (`timeline_engine::timeline::Timeline`), rebuilt when `Session::revision`'s generation moves.
- **L4 `swarmception-ui`**: the shell: the menu bar (File: Open… (⌘O), Open Recent (#69), Reload (⌘R), Fetch and Reload (⇧⌘R), Quit; View:
  the panes, the grouping, Collapse Empty Rows and Collapse to Window (#74), Fold Quiet Time (#86), Fit, Live), as data (`menu`: `MenuAction`, `MenuState`) that the
  in-window bar draws and `Shell::run_menu` runs; on macOS the app puts it in the system menu
  bar instead (`apps/app/src/mac_menu.rs`, muda), the same items through the same `run_menu`,
  their marks following `Shell::menu_state`. **File's commands come from the registry** (#160,
  swarm-core's contract): a command's `MenuPlace` (its path, label, group, shortcut and what it
  asks for, `repo.open`'s folder) makes it a `MenuAction::Command` there, enabled by the engine's
  own check (`engine.states`; a greyed item's hover says why) and its shortcut taken in the window's
  own bar; the menu module names no command. The window's own items stay hand-placed, each for a
  reason: Open Recent's list (the repositories `repo.recent` lists), Quit, and View's check
  items, which show and set state (the panes, `rows.set`'s fields, the widget's fold and fit,
  `live.set`'s on) rather than run an action; the toolbar (Font Awesome 5's solid face,
  `assets/`), the status bar, and the panes. The timeline pane owns swarm-timeline's widget
  (`timeline-egui`): it loads the engine's timeline when the rows move on, keeping the view
  (#54): a window at the timeline's end follows the new end at its width (the whole swarm shown
  stays the whole swarm), any other stays put; the selection and focus carried to the same bars
  (`timeline.carryBars`), the playhead to the same time, the highlight lit again on the new rows.
  Collapsed to the window (#74, `TimelinePane::collapse_to_window`), it hides what `rows.window`
  leaves out (the widget's `tracks.set` `hidden`, never a row the person hid) once the window has
  held still 250 ms with no button held (`SETTLE`; at once after `ui.set`'s window, a reload or a
  change of selection), so nothing reloads: the selection, highlight, details and the rows' own
  states stay; the rows holding the selection are kept, a selection made on rows the window left
  out shows them first (the widget selects only shown rows), and the hidden flags it set are left
  out of the saved state; `ui.inspect`'s `timeline.leftOut` and `collapseUs` say what and at what
  cost. It drives the widget through its own commands (`view.fit`, `state.set`). The toolbar's Live
  toggle (and View ▸ Live, ⌘L) is `live.set`, its interval beside it while on; the status bar says
  live and when it last checked, paused, or that the last check failed and when it tries again,
  the why on hover; the shell asks for a frame when the next check is due. On every load it fixes
  what the owner fixed (#35) by command: the `Time` scale and the absolute origin (`state.set`),
  the minimap always (`view.minimap`), a click selecting the nearest bar (`view.clickMode`; with
  Alt the playhead, and so the factory, goes to the bar's end rather than its start, #137), and
  each row's lanes sized over the window (`view.lanes`, #97: a row is as tall as the most of its
  bars that overlap in the window, the widget packing them again once the view holds still; the
  pane packs them at once after a load, a restore and its own moves, and `ui.inspect`'s
  `timeline.lanesWindow` says over which window); the widget's toolbar shows only `TIMELINE_TOOLBAR` (`toolbar.set`: Fit, Zoom to Selection, Undo
  Zoom, its zoom in / out / fit buttons, the measurement), and the app's own toolbar has no zoom
  buttons. The bars' tooltips reach the widget in colour (`timeline::tooltip_line`, the host
  data's `range_tooltip_lines`).
  The factory's header carries a statistics strip: what `stats.get` returns for the timeline's
  visible window (landed per active hour, its hourly sparkline with *t*'s hour marked, and the
  compact figures, each tooltip saying how it is computed), asked again only when the window or
  the data moves on. Clicking an hour's bar (#31) zooms the timeline's window to that hour
  (`TimelinePane::show_span`: `timeline.placeAt`'s positions, clamped to the extent, through the
  widget's `state.set`) and puts the playhead (the factory following) at its last second; the
  strip keeps its own range (#85: `StatsStrip::keep`, not the window the click made) with the
  hour marked under its bar, until the timeline's window moves off that one (`ui.set`'s `hour`
  does as the click).
  **The timeline is the source of truth** (#85, the owner's): the factory has no clock of its
  own. Its header puts its buttons first at fixed places (play and speed, ⏮ / ⏭ and what they
  step by, Follow; the combo boxes a fixed width, truncating; `HeaderDrawn`), then the words in
  a small label: the moment a step landed on, else where *t* comes from. *t* is the timeline's
  playhead, else its window's end; not following, the last time it showed (`held at …`). Play,
  steps, `--at`, `ui.set`'s `at` and an hour's click move the playhead; the playhead stands on an
  item (a time a bar names), so the time put is kept precisely while the playhead stays on that
  item, and the playhead's own time once it moves (`FactoryPane::put`, `Looking::playhead_item`).
  What is selected follows the timeline (#85): the details' subject (a bar chosen on the
  timeline, a gate run chosen in the factory's Gate list, a click in the factory) is
  the factory's selection (`factory_paint::Selected`): a ticket's chip at whichever station holds
  it and its brick (its journey traced too), a merge's brick (`Block::commit`), a gate run's line
  in the gate's list, each ringed in the selection's colour and brought into view (a list cut
  short shows it in place of its last; main's wall scrolls to a newly selected brick); a click on
  a grey brick selects its merge.
  The factory pane, under the timeline, draws what `factory.at` returns, its stations left to
  right and the key along the bottom in one row (two when narrow); the benches are short strips
  (name, model and helper on one line, the status under it) in more columns when many workers
  would not fit in one; when even those squash them, the workers off shift holding nothing fold
  into one line under the rest (`+22 off shift: …`), the busy come first and the rest scroll, each
  strip at least a line high, the bench holding a newly selected ticket scrolled into view (#94:
  `ui.inspect` version 13's `factory.drawn.benchesFolded` and `benchesHidden`); each group in
  natural order (`factory::natural`: `worker-2` before `worker-10`, #106), and a click on the
  folded line opens it, their benches after the rest, scrolling with them, the first of them
  scrolled into view, the line then folding them again (`FactoryView::off_shift`, `UiState`
  version 11; `ui.set`'s `offShift`; `ui.inspect` version 15's `factory.drawn.benchesUnfolded`), the gate is its list of runs (✔/✖, tickets, cause, age) under what it is
  gating (#31), the workers' ready checks among them in time order (#95: `Gate::checks`, each a
  `GateRun` of `Lane::Ready` with its `worker`; a flask in a second column, read as its worker's
  name; the gate's counts its own), a run selected kept in the list however old
  (`factory.at`'s `run`, `keep_run`), drawn selected while it runs past *t*, and as `not yet
  run` before it begins; a click on a check's line tells its story in Details, as a gate run's. The tooltips are the scene's lines in colour (`factory_tip`: phases in their
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
  main no ticket names is a small grey brick marked `!<pr>`; its *t* is the timeline's (above),
  play runs the playhead through the window, and steps (#33): ⏮ / ⏭, and ← / → while the factory
  has the keyboard (a press in it takes it, as one on the timeline takes it there), put the
  playhead at the key moment `factory.step` returns for the kinds chosen beside them
  (`FactoryView::steps`: every moment, or only claims, work, submits, gate runs, send backs or
  merges), the moment named in the header; shift-click, Home and End go to the first and last
  event.
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
  hover's ticket while it lasts, else a Workers pane hover's (`Hover`), else the details' bars, so
  a hover's leave gives them back (`TimelinePane::lit`, `ui.inspect`'s `timeline.lit`; #84's
  hovered run and `timeline.hoveredRun` gone with the gate's rows, `ui.inspect` version 16,
  #117).
  **Folding quiet time** (#86, swarm-timeline's §5.3): View ▸ Fold Quiet Time, the toolbar's
  checkbox beside the Collapse toggles and `ui.set`'s `foldQuiet` (`UiState::fold`, `FoldView`)
  have the widget fold every stretch where no row has a bar for longer than the threshold
  (`ui.set`'s `foldAfterSeconds`, else the swarm's own quiet gap, `stats.get`'s
  `quietAfterSeconds`, 15 min): each a narrow zig-zag marker with its tooltip (`quiet 5 h 32
  min`, `03:29 → 09:01`), the ruler jumping over it with its edges' times. The pane tells the
  widget (`view.fold`) when it changes and again after every load and restore
  (`TimelinePane::fold_quiet`), so it composes with Collapse, the grouping and a live reload;
  `ui.inspect`'s `timeline.folds` (version 12) are the stretches folded, as clock times. What is
  drawn aligned with the timeline goes through its folds: the pane keeps last frame's
  `TimeAxis` (the widget's window across its width as `PixelMap::folded` maps it, its ends on
  screen, the `Time` scale's origin). There is no gates pane (#128, the owner's: #61's Gates band
  removed); how the gate behaved is `gates.list` and `stats.get` for agents, the factory's
  figures and its Gate list for the person. The factory's floor keeps within its pane, its header wrapping, so a narrower
  pane never cuts off main.
  The panes share the window (#68) by one rule, `layout`'s plain functions: the timeline keeps
  its least height (`TimelinePane::least`, `layout::timeline_least`: its toolbar, the rulers, the
  merges rows and six rows under them, all when fewer, and the scroller); the bands under it
  (`BandKind::ALL`, top to bottom: the Factory; a pane that slots in under the timeline adds
  itself there) each ask for the person's height (`PaneSizes`, set only by dragging a band's top
  edge) or the window's default (`layout::defaults`: the Factory about a third of the height), and
  when they don't fit over the timeline's least they give way (`layout::share`), each in
  proportion to what it has over its own least (Factory 140), before the timeline does; they come back when the window grows. Each band folds to
  its header (28 points) with a click on its title's chevron, the Details to a strip (30 points);
  folded is the person's choice once made (`UiState::folded`, `layout::Folded`, saved), else the
  window's size (the Factory under 560 high, the Details under 1000 wide).
  Without the timeline the last open band fills the rest. `ui.inspect`'s `layout` reports what
  each got (`LayoutDrawn`: the timeline's height and least, each band's height, what it asked and
  whether it is folded, by default or by choice), `timeline.inView` the rows on screen, and
  `ui.set`'s `folded` folds or opens a pane.
  The workers pane (#67, View ▸ Workers, `--panes …,workers`, off by default) is a window over the
  others, opening up from the bottom over the bands so the timeline's rows stay in sight, and
  taking no room in the layout: a comparison is opened, read and put away. It draws what
  `workers.compare` returns for the timeline's window (or the whole run, its choice; asked again
  only when the window or the data moves): a row per model × effort (a swatch in its colour, the
  theme's series by its place), its workers under it (name, start, `open` for a session with no
  stop), then all workers; each figure with a small bar scaled to its column's largest, in the
  group's colour, and its samples (`n=…`); a thin one marked `~`, its bar only outlined. A
  header sorts by its column (again: the other way; none last either way; the first column: the
  command's order). Hovering a row lights its tickets' bars on the timeline (`Hover::Worker`,
  `Lit::HoveredWorker`, after a factory hover); clicking a worker makes its session the
  details' subject. `--hover-worker <name>` puts the pointer on a worker's row. Its window's
  place (#76: `WorkersView::place`, its top-left corner in whole points; none, its default up
  from the bottom) is set by `ui.set`'s `workers.place` and moved there once (egui's kept area
  state moved first, since a window dragged by its title bar puts `current_pos` back), and a
  drag by the person is kept once they let go (moved, its size the same); `ui.set`'s `workers`
  also opens or closes it and sets its sort (as a header's first click, `WorkersView::sort_by`)
  and its window of time; `ui.inspect`'s `workers.rect` is where it was drawn.
  `UiState` (the panes shown and their sizes, the factory's follow, play, speed, traced ticket
  and the moments it steps by, what the details are of; version 5: the gates pane, #61; version 6:
  the folded panes, #68; version 7: the workers pane and its `WorkersView`, window or whole run
  and sort, #67; version 8: its window's place, #76; version 9: no factory *t*, #85, an older
  state's `t` dropped as it reads; version 10: `fold`, folding the timeline's quiet time, #86, an
  older state's off; version 11: the benches off shift, #106; version 12: no gates pane, #128, an
  older state's `gates` dropped as it reads) is serde data. `saved` (#53) keeps the
  app's state across a restart: `AppState` (version 1: the `UiState`, the rows spec, the window's
  size and place, Live as chosen (#54), and the timeline's `TimelineView`: the widget's own `state.save` without its
  playhead, the window as clock times, whether it was fitted or ended at the last event, the
  playhead's time) per repository (`repo_key`: its canonical top level), in
  `<dir>/state/<name>-<FNV-1a of the path>.json` under the platform's app-data directory
  (`~/Library/Application Support/swarmception` on macOS, `%APPDATA%`, `$XDG_STATE_HOME`), written
  whole when it changes and holds still a second (at most every five while it keeps changing,
  `Keeper`) and on quit. That is sooner than swarm-core's contract asks (saved on close): a crash
  then costs at most five seconds of the layout, and the file is still written whole or not at all.
  `StateStore::load` never fails: an older file is migrated, each section read as this version's;
  a file this build can't wholly read is **the person's, left alone** (#165, docs/app-architecture.md
  › State on disk): a newer build's, a corrupt one or another repository's is refused (the window
  starts from the defaults), one with a section that doesn't read keeps what does, and either way
  the `Keeper` never writes over it that run (`ui.inspect`'s `saved.keeping` is `leftAlone`). The
  status bar says so once (a click puts it away). `recent.json` likewise: a newer or corrupt one
  lists nothing and is never written over. The restored view comes back where it was, at
  the new end as wide when it ended at the last event, the whole swarm when fitted, the details'
  subject not moving it, the playhead at its time (kept across the loads after, as #54's reload
  keeps it), and Live as the person left it (`live.set`, only when they chose it); a window
  restored partly or wholly off the monitors winit lists (one since unplugged) is moved onto the
  one it overlaps most, else the nearest, shrunk to fit (`WindowPlace::onto`, #68);
  `ui.inspect`'s `saved` says where it is kept and was read from.
  `open` (#69) opens repositories from the window: File ▸ Open… asks the host's `FolderPicker`
  (on macOS the app's AppKit open panel, `apps/app/src/folder.rs`, through the objc2 crates
  already locked; elsewhere, #76, `apps/app/src/picker.rs`: the desktop's own as a child process
  with a deadline (ten minutes, then stopped and taken as cancelled), on Linux and the BSDs
  `zenity --file-selection --directory` or `kdialog --getexistingdirectory`, the one of the
  desktop `XDG_CURRENT_DESKTOP` names first, on Windows PowerShell's `FolderBrowserDialog`
  (`powershell`, else `pwsh`, the start folder handed over in the environment), the choice
  (`picker::candidates`) and the reading of its exit and output (`picker::outcome`) pure
  functions; the picker answers `Picked::Folder`, `Cancelled` or `NoPicker`, and without one, or
  when none runs, the window asks for a path) and runs `Shell::open_repo`, the open
  repository's state written first, then `repo.open`; a refusal is a window saying why, the
  repository open staying. File ▸ Open Recent lists `repo.recent` (each name and its folder,
  `menu::recent_label`; `MenuAction::OpenRecent(n)`) with Clear Menu (`repo.clearRecent`); the
  list is app-wide, `<dir>/recent.json` (`saved::RecentFile`, version 1), written when it
  changes and read back after the repository opened at the start. Whatever opened a repository
  (the menus, the control channel's `repo.open`, which the shell takes itself), the shell sees
  `Session::opens` move and switches: the panes afresh (the panes shown, their sizes and folds
  kept), then the new repository's own saved state restored and kept, Live as chosen for it.
  `ui.inspect` (version 6; 8 since #76's `workers.rect`) reports it as `open`: the refusal
  shown, the path being typed, Open Recent's items and where the list is kept.
  `control` (#46) is the window's side of the control channel: `Shell::answer`
  runs `ui.commands`, `ui.inspect`, `ui.set` and `ui.screenshot`, forwards `timeline.<id>` to
  the widget and every other command to the engine ([Automation](#automation)).
- **L4 `swarmception-automation`** (#46): the control protocol as types (`protocol`: `Request`,
  `Reply`, `ProtocolError`, version 1), the loopback server (`tcp`), the UI thread's handler
  (`channel`), the headless handler (`session`), MCP over stdio (`mcp`) and a typed client
  (`client`). It depends on the engine and on swarm-timeline's `timeline-automation` (same pin),
  never on the UI: the app joins the two.
- **Apps:** `swarmception` (the window; `--repo`, `--grouping`, `--collapse [off|empty|window]`,
  `--panes`, `--details`, `--at`, `--hover`, `--hover-bar`, `--hover-worker`, `--fold` and `--hover-fold <n>` (#86: quiet time folded, the pointer on a fold's marker),
  `--menu file|file.recent` (#69: the window's own menu bar, File clicked open and the pointer on
  Open Recent, for a screenshot), `--step`, `--steps`, `--size`, `--zoom` (the UI's
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
  result schemas, which the CLI's help and MCP's tools are built from, and its menu place (#160);
  `engine.states` says which can run now and why not (each command's `enabled`, a panic in it
  caught).

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
- **Persisted state carries a version**: an older build's is migrated, and one this build can't
  wholly read (a newer build's, a corrupt one) is never written over (#165).

## The UI

- **Thin and data-driven.** The UI's state is plain data (serde), so the control channel reads it
  (`ui.inspect`) and drives it (`ui.set`), and the app saves and restores it.
- **What is drawn is what a command returns.** A pane draws a command's result; a test checks
  that what it drew equals the command's answer. Nothing the UI decides is untestable headless.
- **Colours from theme tokens, text from a string catalog**, never literals.
- **Never block the UI** (swarm-core's contract, The UI is a client; #161). Whatever starts a
  program or reads the disk runs in the background in a session with a waker and lands through
  `Session::poll`: a read and a fetch of the branch, a live check, the issues, and `repo.open`'s
  check of the folder (`git rev-parse` and the refs: its answer says `opening`, and the repository
  opens, or `repo.status`'s `refused` says why not, when the check lands; the control channel's
  `repo.open` is answered then, `Answer::Opening`, the UI's thread drawing meanwhile). The calls the
  panes make each frame only read or change memory, which the contract lets the UI's thread make;
  measured on mc-engine (214 tickets, 2226 events; release, 2026-11-13): `factory.at` 2.6 ms,
  `rows.set`'s rebuild 2.3 ms, `factory.moments` 1.1 ms, `workers.compare` 0.9 ms,
  `stats.get` 0.5 ms, `details.get` 0.3 ms, each with its JSON. The CLI and agents' headless
  sessions run everything in the foreground.
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
  `UiState`, and what the panes show: the factory's *t* (the time it shows), where it came
  from, the key moment, what it has selected and where it drew it, its header's words and its
  strip's range (version 11, #85), what each station drew last frame, the timeline's scale, window, playhead, selected ranges and
  highlighted ones),
  `ui.set` (`UiSet`: panes, speed, steps, `at`, follow, playing, a ticket traced and its bars
  selected, a step, the details' subject or none (#55), the window, an hour of the strip's (#85);
  `workers` (#76: `open`,
  `sort`, `ascending`, `wholeRun`, `place` within ±16384 points, `defaultPlace`); `ui.inspect`'s
  `details` report its subject, title, error and bars), `ui.loaded` (`{"within": <s>}`, at most 19:
  answered with `ui.inspect`'s answer once nothing is loading, the issues are in and the
  timeline has its data, or the read failed, or there is no repository; a `ui` error after
  `within`) and `ui.screenshot` (`{"path": "<file>.png"}`, PNG, or a binary PPM for `.ppm`;
  taken a few frames later so a `ui.set` has settled, answered with its size once saved;
  `swarmception_ui::shot`); or `timeline.<id>`, the
  timeline widget's own command forwarded (`timeline.view.fit`, `timeline.state.get`,
  `timeline.ui.inspect`; `timeline.engine.commands` lists them: the engine's own `timeline.*` ids
  have one dot). The errors are `ProtocolError`, tagged `problem`: `bad_request`, `too_long`,
  `wrong_version`, `bad_token`, `no_ui`, `unavailable`, `busy`, `engine` (the engine's
  `EngineError`), `widget` (the widget's), `ui`; and MCP's own two, `image_too_large` and
  `image_unreadable` (#76). A connection whose first line lacks the token is
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
  minutes) is opened again; a window gone is the tool's `unavailable` error. A screenshot saved
  as PNG is answered as MCP's image too (#76): its text (`{path, width, height}`), then
  `{"type":"image","data":<base64>,"mimeType":"image/png"}` (`mcp::Content`; the base64 is
  `mcp::base64`, the standard library's), read back by the MCP server up to `mcp::IMAGE_MOST`
  (3 MiB, 4 MiB as base64, under the 5 MB a model takes an image at); a bigger one is the tool's
  `image_too_large` error with its path and size, the file saved all the same, and one that
  isn't a PNG `image_unreadable`. A PPM is answered as text alone.
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
| 2026-11-13 | minor | #163: egui's state keyed by typed salts (`part::Part`: panels, windows, combo boxes, the areas that take clicks; a part drawn many times its data beside it), never a string; a test holds the crate to it |
| 2026-11-13 | minor | #164: `source` is the layer that touches the outside: `files` (the one atomic writer, `write_private` for the token file) and `process` (`Program`, the one runner; git's and the picker's on it); the UI's state, screenshots, the token file and the issues' cache written through `files`; `cargo xtask writes` in `ci` |
| 2026-11-13 | minor | #160: File's commands from the registry: `Command::MENU` (`MenuPlace`: path, label, group, `Shortcut`, `Ask`), `engine.commands`' `menu`, `engine.states` (`Entry::state`), `Unavailable::NothingRecent`; `MenuAction::Command`, `file_items`, `CommandsEnabled`; the macOS menu and the window's shortcuts from the same places |
| 2026-11-13 | minor | #161: `repo.open`'s check of the folder runs in the background in a session with a waker (`StatusResult::opening`, `refused`; `Session::opening`, `refused`); the window's `repo.open` over the control channel answers once it lands (`Answer::Opening`); the panes' memory-only calls measured and kept on the UI's thread |
| 2026-11-13 | minor | #165: a saved state or recent list from a newer build, corrupt, foreign or with a section that doesn't read is left alone, never written over (`Loaded::left_alone`, `Keeping::LeftAlone`); older ones migrate |
| 2026-11-13 | minor | #158: swarm-core d4df5a2, its theme the palette the swarm's tools share (22 tokens with the gate and idle, nudged to 4.5:1 on the new backgrounds; the series and markers in place of range8; range32 with 11 entries nudged); waiting for merge `link`, the Workers groups the series; `ui::theme::apply` styles egui's visuals in the tokens, so the window, the Factory and the timeline widget's background are the shared palette's; a brick's edge the theme's `darker` |
| 2026-11-13 | minor | #154: a colour is a `Paint`, the theme's dark and light (a breaking change to every colour in a command's answer: `[r,g,b]` became `{dark, light}`); the UI draws egui's look's, `factory_tip::legible`'s darkening gone; `theme.set` builds the timeline in the window's look |
| 2026-11-13 | minor | #152: colours from swarm-core's theme (`domain::theme`: tokens, ranges, the pick, `behind_text`, its tests); a ticket range32's pick, the stages the tokens named for what they mean, the gate and idle Swarmception's own tokens; `theme.get`; scheme B's palette gone |
| 2026-11-06 | minor | #142: main merge closure evidence shared by ticket consumers; finite paused/blocked stops; stale unfinished histories end at last observation with outcome unknown |
| 2026-11-04 | minor | #134: the minimap draws the tickets' strips (`view.minimapStrips`, set in `TimelinePane::fix_view` with the minimap's visibility), by the pin to swarm-timeline c5a7b06 (#222) |
| 2026-11-03 | minor | #127: a sent-back ticket's bar carries the factory's red corner with its count (`Bar::corner`, swarm-timeline's range corner, pinned at 19eb5e0), its diamonds gone; merge dots only on `rows.set {"mergeMarks": true}` (View ▸ Merge Marks); a red dot where a ticket was given up (`MarkKind::GivenUp`); `SwarmData` forwards `range_corner` |
| 2026-11-03 | minor | #128: the Gates band removed (the owner's): `crates/ui/src/gates.rs`, View ▸ Gates, `--panes …,gates`, `Panes`/`PaneSizes`/`Folded`'s `gates`, `BandKind::Gates`, `ui.inspect`'s `gates` (version 17); `UiState` version 12, an older state's `gates` dropped as it reads; `gates.list` and `stats.get` kept |
| 2026-11-03 | minor | #117: every place that names a stage takes its colour from `Stage::colour` (`colours::phase_colour` gone): the tooltip's stretch lines and milestones (working the strip's warm white, not the ticket's colour), a wait's gate runs listed under it *in the gate*, the details' story and facts; the Gate runs and Ready checks rows removed (`GATES_KEY`, `READY_KEY`, `RunBar`, `Bar::run`, `Style::GateRun`/`ReadyCheck`, `gates::told_runs`, `timeline.runBars`, the run hover, `--hover-gate`, the rows' importance cases), `ui.inspect` version 16 |
| 2026-11-03 | minor | #115: the top rows' merges are markers of no length (no `MERGE_SECONDS`); `timeline` gives each such time an item of no width, the bar's |
| 2026-11-03 | minor | #110: a ticket is one bar a row in its own colour (`Style::Ticket`), its stages (`Stage`, `BarSegment`) a strip along its foot and its send-backs and merge `BarMark`s, through swarm-timeline's range strips (§6.6) and marks' glyphs (§11.5), pinned at its main; a session's bars are the tickets' bars holding the stretches it began |
| 2026-11-03 | minor | #106: a gate's failure for the change's cause is a send-back with or without a `sendback` (`sendbacks`, `Activity::issue_events`), as swarm-core's `prwatch::sendback` reads it; the Factory's benches in natural order (`factory::natural`), the fold of those off shift opening on a click and closing again (`FactoryView::off_shift`, `UiState` 11, `ui.set`'s `offShift`, `ui.inspect` 15's `benchesUnfolded`); the live test's fakes run by `sh`, never exec'd new |
| 2026-11-03 | minor | Each row's lanes sized over the window (`view.lanes`, #97), the widget pinned at swarm-timeline 3692af8 (with the minimap's fold marks); `ui.inspect` 14: `timeline.lanesWindow` |
| 2026-11-03 | minor | Background reads under load (#96): a last wake when the issues' read is over; the engine's tests block on the waker, the UI's wait to a deadline; the source looks up its refs in one `cat-file --batch-check` |
| 2026-11-03 | minor | Ready checks in the Factory's Gate list (#95): `Gate::checks` (`ready_checks`, `GateRun::lane` and `worker`), in time order with the gate's runs, a flask apart; `factory.at`'s `run` keeps a run selected in the lists (`keep_run`, `is_run`); a selected run running past *t* drawn selected, one after it `not yet run`; a ready check's details its own story (`Subject::Gate` by its id: `details`' `Ctx::checks`; its bar's subject and its line in its ticket's story go to it); `Drawn::run_lines` |
| 2026-11-03 | minor | The Factory's benches with dozens of workers (#94): those off shift holding nothing fold into a line when the rest lack room, the busy first, the rest scrolling (`BenchGrid`, `WallView::benches`), never shorter than a line; `Drawn::bench_cells`, `benches_folded`, `benches_hidden`; `ui.inspect` version 13 |
| 2026-11-02 | minor | Folding the timeline's quiet time (#86): swarm-timeline at efdf212 (its §5.3); View ▸ Fold Quiet Time, the toolbar's checkbox, `ui.set`'s `foldQuiet` and `foldAfterSeconds`, `UiState` version 10 (`fold`), `ui.inspect` version 12 (`timeline.folds`); `TimelinePane::fold_quiet` and `time_axis`, the Gates band's hours through the folds (`TimeAxis`) |
| 2026-11-02 | minor | The Factory follows the timeline (#85): no clock of its own (its *t* the playhead's, a time put on it kept while the playhead stays on its item; `UiState` version 9 without `t`), its header's buttons first at fixed places, the details' subject selected in it and brought into view (`Selected`, `Block::commit`), the strip keeping its range after an hour's click (`ui.set`'s `hour`); `ui.inspect` version 11 |
| 2026-11-02 | minor | Gate runs and ready checks as rows on the timeline (#84): the domain's Gate runs and Ready checks rows under the merges rows (`Style::GateRun`, `Style::ReadyCheck`, `Bar::run`, `RunBar`, `TOP_KEYS`), `gates.list`'s runs a bar each; `bar_subject` and a gate run's `OnTimeline` read them; `timeline.bar`, `timeline.runBars`; the timeline's hovered run lights its tickets (`TimelinePane::hovered`); the Gates band keeps its figures, causes and hours, its lanes gone; `ui.inspect` version 10 (`timeline.hoveredRun`; `gates.runs` gone, `gates.hovered` an hour) |
| 2026-11-02 | minor | A submit straight after a send-back is its rework, drawn as working in the ticket's colour after a minute's sent-back mark (#83) |
| 2026-11-02 | minor | `ui.set` reaches the Workers window (#76: `workers`, its place in `WorkersView` and `UiState` version 8, `ui.inspect` version 8 with `workers.rect`); MCP answers a screenshot's PNG as an image up to `IMAGE_MOST` (`mcp::Content`, `image_too_large`, `image_unreadable`); Open… off macOS through the desktop's picker as a child process (`picker`, `Picked`) |
| 2026-11-02 | minor | One definition of a gate run (#75): `gates::runs` and `gates::figures`, counted by `gates.list` and `stats.get` alike (`Stats::gates` gains `running` and `inferred`, its `noResult` no longer counting running runs; `Figures::inferred`); the Gates band draws both lanes' hours, mirrored (`GatesDrawn::hour_columns`), `ui.inspect` version 7 with `gates.hours` |
| 2026-11-02 | minor | Opening a repository from the app (#69): the source's `probe`, `repo.open`'s refusals before anything changes, `repo.recent` / `repo.clearRecent`, `Session::opens`; File ▸ Open… (AppKit's open panel, ⌘O) and Open Recent, the shell switching state per repository, `recent.json`, `ui.inspect` version 6 with `open`, `--menu`; every live check that reads again refreshes the issues |
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
