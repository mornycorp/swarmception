<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Fill in the parts in comments like this one; keep the rules, which the swarms before
     you paid for (docs/lessons.md). -->
# Architecture

> **Last reviewed:** 2026-11-01 · **Last updated:** 2026-11-01 · **Change:** minor (main's wall: each brick as wide as its ticket took, its phases along it, its sendbacks marked) · **Target:** M1

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
  that doesn't read is skipped and counted; an issue's PR is the newest whose body closes it), and
  `IssueCache`, their versioned JSON on disk, which reads from an older or newer build too.
- **L1 `swarmception-domain`**: `activity` reads each instance's sessions (split at its pauses),
  its work on issues, its helper sub-agents (a gate's `rc`, the PRs its note names) and its marks,
  and applies what another instance ends (`of` on `session.stop` and `release`), and keeps every
  issue event in time order. `tickets` reads each ticket's life from them: when it was claimed,
  began, finished (its submit) and merged, and its stretches (claimed, working, waiting for merge,
  sent back, stopped); a claimed, working or sent back stretch ends where its holder's session
  ended, the ticket *abandoned* (#28: `Ticket::abandoned`, the bar's tooltip says whose session
  ended when, and the factory puts it on the shelf then). `rows` turns them into the rows asked for (`RowsSpec`: the tickets flat or
  by worker, collapsed), under two rows at the top, the merges onto `main` and the tickets'
  merges, each a 10 s span from its time (`MERGE_SECONDS`), all Ranges rows (`Node::Group` or
  `Node::Ranges`); each bar carries its label, its ticket (none for a merge onto main), its style,
  its colour and its tooltip. `colours` is the one source of colours, the timeline's and the
  factory's (#15): a ticket's own colour from its issue number (hues a golden angle apart, kept
  clear of the fixed colours' hues, legible labels on every one), in which its *working* bars and
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
  run), main's blocks (each with its ticket's life up to the merge, #32: `took`, claim → merge;
  `segments`, its stretches with the gate runs naming it carved out of its waits; `sent_back`), and
  what happened in the `recent` seconds before *t*. Nothing after *t* is
  told; `titled` adds what GitHub says (each crate's title, its tooltip's title, state and labels
  as of *t*). `titles` (#17) is the text from GitHub's issues: a bar's label with its ticket's
  title (`#58 Architecture review`, always: no option since #34), and the lines a
  ticket's tooltips start with (title; state, labels, author; its PR); `rows_with` builds the rows
  with them, and each bar carries its PR. `timeline`'s host data gives a ticket's bar (and a
  merge onto main's) the context-menu items Open Issue and Open PR, as `Link`s by host item id.
- **L2 `swarmception-source`**: the tracking branch through `git` (`refs/remotes/origin/tracking`,
  else `refs/heads/tracking`): `rev-parse`, `ls-tree`, one `cat-file --batch`, and `fetch`; and
  `main`'s first-parent merge commits (`git log --merges`, `origin/main` else `main`, or `master`);
  each call with a deadline and capped output, every path checked as an instance id. `github`
  (#17): the `origin` remote read as GitHub's `owner/name`, the issues fetched with `gh api
  --paginate` (the same runner as git: a deadline, a cap, no prompts), and a cache per repository
  in the user's cache directory (`~/Library/Caches/swarmception/issues/<owner>--<name>.json` on
  macOS), written whole or not at all. No remote on GitHub, no `gh`, no network: a `GithubError`.
- **L3 `swarmception-engine`**: the `Session` (the repository, the activity, the rows spec, the
  rows and the timeline built from them) and the command registry: `engine.commands`,
  `repo.open`, `repo.reload`, `repo.fetch`, `repo.status`, `rows.get`, `rows.set`, `rows.list`,
  `timeline.timeAt`, `stats.get` (`{start?, end?}`, each a UTC time or epoch seconds, the whole
  run by default), `timeline.placeAt` (a time's position and item), `timeline.ticketBars` (a
  ticket's bars, to highlight), `factory.at` (the factory at *t*, and a ticket's journey),
  `ticket.urls` (a ticket's issue page and its PR's, and a number's page as a PR). Every load
  (`repo.open`, `repo.reload`, `repo.fetch`) also reads the repository's GitHub issues (#17): the
  cache first, then `gh`, cached in turn, never in the way of the tracking branch; what came, from
  where and why not is `repo.status`'s `issues`, and new issues build the rows again. Where they
  come from (`gh`, the cache directory) is `Session::set_github`. A session made with a waker
  reads in the background and lands through `Session::poll`. The host API `Session::widget_timeline` hands the widget its timeline
  (`timeline_engine::timeline::Timeline`), rebuilt when `Session::revision`'s generation moves.
- **L4 `swarmception-ui`**: the shell: the menu bar (File: Reload, Fetch and Reload, Quit; View:
  the panes, the grouping, Collapse, Fit), as data (`menu`: `MenuAction`, `MenuState`) that the
  in-window bar draws and `Shell::run_menu` runs; on macOS the app puts it in the system menu
  bar instead (`apps/app/src/mac_menu.rs`, muda), the same items through the same `run_menu`,
  their marks following `Shell::menu_state`; the toolbar (Font Awesome 5's solid face,
  `assets/`), the status bar, and the panes. The timeline pane owns swarm-timeline's widget
  (`timeline-egui`): it loads the engine's timeline when the rows move on, keeping the scale and
  window, and drives the widget through its own commands (`view.fit`, `view.zoom`, `state.set`).
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
  gliding and resizing as *t* moves; hovering one rings it everywhere and selects its bars on the
  timeline (`timeline.ticketBars`, the widget's `ranges.toggle`; the person's own selection is put
  aside on the first hover and put back exactly when the pointer leaves), clicking one traces its
  journey; main's wall (#32) packs a brick per merge in rows from the ground up, each as wide as
  its ticket's claim → merge time against the median (no narrower than its number, no wider than
  four times the median's), a strip along its foot with its life's segments in
  their phase colours, a red corner when it was sent back (the count in it from two), shrinking
  to the smallest size and then scrolling with the wheel, the newest rows first; a merge onto
  main no ticket names is a small grey brick marked `!<pr>`; its *t* follows the timeline (the pointer, else the playhead,
  else the window's end) or plays through the window, the playhead following.
  Right-clicking a bar (the widget's host items) or a chip offers Open Issue and Open PR: the
  shell runs `ticket.urls` and hands the page to egui's `open_url`. Bars always carry titles (#34);
  the status bar says how many issues came and from where,
  or warns why none did.
  `UiState` (the panes shown, the factory's follow, play, speed and *t*) is serde data.
- **Apps:** `swarmception` (the window; `--repo`, `--grouping`, `--collapse`,
  `--panes`, `--at`, `--hover`, `--hover-bar`, `--screenshot`) and `swarmception-cli` (the engine
  headless: `commands`, `run`).

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
  without a window, and the smoke gate's headless run.
- **The control channel**: the desktop app, started with `--control`, listens on loopback with a
  token, JSON lines in and out: every engine command, plus `ui.inspect` and `ui.set`.
- **MCP**: the same registry as tools.

There is no control channel or MCP yet ([G2](gaps.md#g2-automation-the-control-channel-and-mcp)):
the CLI and the engine's tests are the headless way in today.

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
| 2026-11-01 | minor | Main's wall (#32): bricks sized by claim → merge, their phases along them, sendbacks marked; the blocks carry `took`, `segments`, `sent_back` |
| 2026-11-01 | minor | The factory's layout (#31): no ticker, the key along the bottom, short bench strips, the gate as its list of runs, coloured tooltips, the sparkline's hours clickable; the factory's own colours softened |
| 2026-11-01 | minor | An abandoned ticket: a claimed, working or sent back stretch ends where its holder's session did (#28) |
| 2026-11-01 | minor | The repository's GitHub issues: fetched with `gh`, cached per repository; titles on bars and first in tooltips; `ticket.urls` and the bars' and chips' Open Issue / Open PR (#17) |
| 2026-11-01 | minor | A colour per ticket from `colours`
, shared with the factory; the top rows as 10 s spans; bars carry their ticket and colour, no more pins (#15) |
| 2026-11-01 | minor | The factory's statistics strip, over `stats.get` for the window (#22) |
| 2026-11-01 | minor | The factory: `factory.at`, `timeline.placeAt`, `timeline.ticketBars` and the Factory pane (#18) |
| 2026-11-01 | minor | The swarm's statistics: the domain's `stats`, the command `stats.get` (#20) |
| 2026-10-31 | minor | The menus as data; on macOS in the system menu bar (#3) |
| 2026-10-31 | major | First version, from swarm-core's template |
