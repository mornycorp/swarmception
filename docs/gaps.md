<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Gaps

> **Last reviewed:** 2026-11-06 · **Last updated:** 2026-11-06 · **Change:** minor (batch 17: G32 done, idle benches grouped and dragging across rows, X to hold the row; no gaps open) · **Target:** M3

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
| — | [G32 Idle benches together, and dragging across rows](#g32-idle-benches-together-and-dragging-across-rows) | done | #147, swarm-timeline#233 | |
| — | [G31 No ghost ticket bars](#g31-no-ghost-ticket-bars) | done | #140, #142, #144 | |
| — | [G28 Small ends after batch 11](#g28-small-ends-after-batch-11) | done | #113, #134, swarm-core#20, swarm-timeline#222 | |
| — | [G30 The owner's ticket bars, and no Gates band](#g30-the-owners-ticket-bars-and-no-gates-band) | done | #127, #128, #137, swarm-timeline#218, #219, #228, #230 | |
| — | [G29 Merges as markers, and one colour per stage](#g29-merges-as-markers-and-one-colour-per-stage) | done | #115, #117, #119, #121, #124 | |
| — | [G27 A ticket in its own colour all along](#g27-a-ticket-in-its-own-colour-all-along) | done | #110, swarm-timeline#205, #206 | |
| — | [G26 Failures survive packing](#g26-failures-survive-packing) | done | #108 | |
| — | [G25 Small ends after batch 10](#g25-small-ends-after-batch-10) | done | #106 | |
| — | [G24 Loose ends from the owner's review](#g24-loose-ends-from-the-owners-review) | done | #92–#98, swarm-timeline#197, swarm-core#14 | |
| — | [G20 A ticket keeps its colour across a send-back](#g20-a-ticket-keeps-its-colour-across-a-send-back) | done | #83 | |
| — | [G21 Gate runs and ready checks on the timeline](#g21-gate-runs-and-ready-checks-on-the-timeline) | done | #84 | |
| — | [G22 The Factory follows the timeline](#g22-the-factory-follows-the-timeline) | done | #85 | |
| — | [G23 Fold the timeline's quiet time](#g23-fold-the-timelines-quiet-time) | done | #86, swarm-timeline#191 | |
| — | [G6 Collapse to the window](#g6-collapse-to-the-window) | done | #74 | |
| — | [G19 Loose ends from M2 and M3](#g19-loose-ends-from-m2-and-m3) | done | #75–#77, swarm-core#12 | |
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

- **Done:**
  - **The Gates band's hours** (#75, PR #78): both lanes drawn per hour under the timeline, gate
    runs up from a baseline and ready checks down, each on its own scale; `ui.inspect`'s
    `gates.hours`.
  - **One definition of a gate run** (#75, PR #78): `gates::runs`/`gates::figures` behind both
    `gates.list` and `stats.get`; inferred runs (a merge or send-back naming a gate that logged
    neither start nor stop) counted and said (`inferred`); `stats.get`'s `noResult` no longer
    holds runs still going (`running`). Both commands gave the same figures before and after on
    mc-engine, swarm-timeline and this repo (2026-11-02): they now can't disagree.
  - **A failed `ready` check says why** (#77, PR #81; swarm-core#12, pinned at 86ca3f0): its
    `sub.stop` is `rc=<n>; cause=<cause>; step=<the failing ci step>` (`rc=101; cause=build;
    step=clippy`); the Gates band's causes, `stats.get`, the tooltip and Details read it as
    `build/clippy`; older `rc=1` stops still read `unstated`.
  - **`ui.set` reaches the Workers window** (#76, PR #80): open, sort, whole run or window, and
    its place (saved; a drag kept on release); `ui.inspect`'s `workers.rect`.
  - **MCP returns the image** (#76, PR #80): `ui_screenshot` through `mcp --control` answers
    with the PNG as image content, up to 3 MiB (a typed `ImageTooLarge` past it).
  - **Open… off macOS** (#76, PR #80): zenity or kdialog, or PowerShell's folder browser, as a
    child process with a deadline, falling back to a typed path; not run on those platforms.
- **Left:** the gate's own failure notes carry no `step=` (only `ready`'s do); the Workers
  window's size can't be set; the off-macOS pickers are unverified; the ready lane's
  per-column counts show only from a 64 px chart (the band's default is smaller).
- **Evidence:** `crates/domain/src/gates.rs`, `crates/ui/tests/gates_pane.rs`,
  `crates/ui/tests/workers_pane.rs`, `apps/app/src/picker.rs`, `apps/cli/tests/automation.rs`;
  screenshots checked 2026-11-02.

## G20 A ticket keeps its colour across a send-back

- **Done** (#83, PR #87): the owner saw mc-engine's #169 change colour after a send-back. The
  colour never changed: a worker's fix after a send-back logs no `work`, only the next `submit`,
  so the whole fix read as *sent back* (red). A worker's submit straight after a send-back now
  makes that stretch rework, in the ticket's colour, the send-back keeping a 60 s red mark; the
  stretch is said to be inferred. A gate's submit is never rework; a re-claim starts anew. Every
  sent-back ticket on mc-engine had it (#52, #68, #71, #96, #120, #134, #169).
- **Left:** the rework's start is inferred: swarm-core could log `work` when a worker picks up a
  send-back ([G24](#g24-loose-ends-from-the-owners-review)).
- **Evidence:** `crates/domain/src/tickets.rs`,
  `colours::tests::a_tickets_colour_holds_across_a_send_back_and_a_re_claim` (#169's real
  events); screenshots checked 2026-11-02.

## G21 Gate runs and ready checks on the timeline

- **Done** (#84, PR #88): two timeline rows under *Tickets merged*, **Gate runs** and **Ready
  checks**, a bar per run coloured by result, labelled with its tickets and a failure's cause
  (`#83 · build/clippy`); they pan, zoom, select (Details tells the story), highlight and
  collapse like any row; a hover lights the run's tickets. `timeline.bar`, `timeline.runBars`.
  The Gates band lost its lanes and keeps its figures, causes and hours.
- **Measured:** mc-engine's 136 gate runs and 147 ready checks equal `gates.list`'s (2026-11-02).
- **Reversed** (owner, 2026-11-03; #117, PR #118): the two rows are gone. A gate run that merges
  is its tickets' *in the gate* stretch, a failure their send-back ([G29](#g29-merges-as-markers-and-one-colour-per-stage)).
- **Evidence:** `crates/domain/src/gates.rs`, `crates/engine/tests/gates.rs`,
  `crates/ui/tests/gates_pane.rs`; screenshots checked 2026-11-02.

## G22 The Factory follows the timeline

- **Done** (#85, PR #89): the timeline is the source of truth (owner). The Factory's header puts
  its buttons first at fixed places, the words after; it has no clock of its own (the timeline's
  playhead, or the window's end; held and said so when not following); play, steps and an hour's
  click move the playhead. Selecting on the timeline selects in the Factory: a ticket's chip and
  brick, a merge's brick, a gate run in the Gate list, brought into view. The strip keeps its
  range after an hour's click, the hour marked. Hovering the timeline no longer moves the Factory.
- **Left:** ready checks aren't in the Factory's Gate list; a run ending after the Factory's time
  isn't drawn selected ([G24](#g24-loose-ends-from-the-owners-review)).
- **Evidence:** `crates/ui/src/factory*.rs`, `crates/ui/tests/factory_pane.rs`; screenshots
  checked 2026-11-02.

## G23 Fold the timeline's quiet time

- **Done** (#86, PR #90; swarm-timeline#191, PR #192, pinned at efdf212): the widget folds
  quiet stretches (or a host's ranges) into 12 px zig-zag markers; every mapping goes through the
  folded axis, the ruler jumps over a fold and labels its edges, a marker's tooltip says what was
  folded (`quiet 12 h 19 min`, `22:33 → 10:53`). Here: a toolbar checkbox *Fold quiet time*, View
  ▸ Fold Quiet Time, `ui.set {foldQuiet, foldAfterSeconds}`, saved; 15 min unless set (the quiet
  gap of `stats.get`); it holds through Collapse and reloads; the Gates band's hours follow the
  folded axis.
- **Left:** the scroller stays linear and the minimap draws no markers; the Factory strip keeps
  its own even axis ([G24](#g24-loose-ends-from-the-owners-review)).
- **Evidence:** swarm-timeline `crates/core/src/fold.rs`, `crates/engine/tests/fold.rs`;
  `crates/ui/tests/fold_quiet.rs`; screenshots checked 2026-11-02.

## G24 Loose ends from the owner's review

- **Done:**
  - **A row's lanes over the visible window** (#97, PR #103; swarm-timeline#197, PR #201): the
    widget packs a row's items into the lanes the window needs once the view holds still (250 ms,
    no button held), and the pane turns it on with every load. Ready checks is one lane where
    one check runs at a time, four only over the overlap. `ui.inspect` 14: `timeline.lanesWindow`.
  - **The minimap marks folds** (swarm-timeline#198, PR #200, by swarm-timeline's own swarm;
    pinned by #103).
  - **Ready checks in the Factory's Gate list** (#95, PR #102): beside the gate's runs in time
    order (⚗, by worker), selected both ways with the timeline; a check tells its own story in
    Details (reversing #84, where it told its ticket's); a run selected on the timeline is drawn
    selected though it ends after *t*, or `not yet run` before it starts.
  - **A send-back's rework logged** (swarm-core#14, PR #15; #98, PR #104): `prwatch` logs `work
    --issue <n> --mr <m>` once when the worker's PR is sent back; the domain starts the rework
    there, the time before it reading as sent back, and #83's inference stays for older runs.
  - **Tests that wait on the session, not the clock** (#96, PR #100): the engine's background
    tests wait on the waker, the UI's on a deadline, not a frame count; the issues read now wakes
    the front end once it ends (a real bug: "fetching…" could linger); a repo opens in 7 git calls,
    not 13.
  - **From the owner, the same day:** the Factory's benches with dozens of workers (#94, PR #99:
    busy first, the rest scroll, those off shift fold to one line; `ui.inspect` 13), and orange
    "Unaligned" markers over the bars in a debug build (#92, PR #93; swarm-timeline#193, PR #194).
- **Measured:** mc-engine's Ready checks over 24 equal windows: 4 lanes in 1, 2 in 4, 1 in 19;
  the issues test under load: median 3.9 s, max 6.7 s (was 4.1 s, 8.5 s), its limit now only
  reached on failure (2026-11-03).
- **Left:** [G25](#g25-small-ends-after-batch-10).
- **Evidence:** `crates/ui/tests/lanes_window.rs`, `factory_gate_list.rs`, `factory_benches.rs`,
  `aligned.rs`; `crates/domain/src/tickets.rs`; `crates/engine/tests/support`; screenshots checked
  2026-11-03.

## G25 Small ends after batch 10

- **Done** (#106, PR #107): a gate's failure reads as sent back at its stop, with no `sendback`
  needed (`crates/domain/src/sendbacks.rs`, the tracking format's rule, checked against swarm-core's
  `prwatch::sendback` on the same files; swarm-timeline's #142 and #185 now read sent back); the
  Factory's benches in natural order, and the off-shift fold opens on a click (`UiState` 11,
  `ui.inspect` 15, `--off-shift`); the slow live test's cause was macOS vetting a newly written
  executable on its first exec (10–34 s at times): its fakes now run through `/bin/sh`.
- **Measured:** the live test, 12 copies at once: median 3.18 s, max 3.35 s (was 7.86 s, 9.84 s;
  single runs 28 s and 73 s) (2026-11-03).
- **Left:** [G28](#g28-small-ends-after-batch-11).
- **Evidence:** `crates/domain/src/sendbacks.rs`, `crates/ui/tests/factory_benches.rs`,
  `crates/engine/tests/live.rs`.

## G26 Failures survive packing

- **Done** (#108, PR #109): every bar carries an importance from one table beside the phase
  colours (`colours::importance`): a failed run or check and a sent-back stretch high, a `?` run
  and a stopped stretch middle, the rest low, merges the default; zoomed out, a packed column takes
  its most important bar's colour (swarm-timeline#202).
- **Measured:** mc-engine fitted, 1600 px: red columns in Ready checks 75 → 79, sent-back in the
  ticket rows 14 → 22 (2026-11-03).
- **Evidence:** `colours::tests::every_kind_of_bar_has_its_importance`,
  `timeline::tests::a_failure_survives_packing`.

## G27 A ticket in its own colour all along

- **Done** (#110, PR #111; swarm-timeline#205, #206, PRs #207–#209): the owner, on mc-engine's
  #169, saw a ticket's *waiting* stretch filled blue and read it as another colour. Each ticket is
  now one bar per row in its own colour, labelled once; its stages (claimed, working, waiting, in
  the gate, sent back, stopped) are a strip along its foot in the Factory's phase colours, as on
  Main's bricks; a send-back is a red diamond at its foot and the merge a green dot at its end
  (the widget's new range strips and mark glyphs). Details, selection with the Factory, highlight,
  collapse and lanes over the window work as before.
- **Evidence:** `rows::tests::a_ticket_is_one_bar_in_its_colour_its_stages_a_strip_and_its_merge_a_mark`,
  `crates/ui/tests/timeline_pane.rs`; screenshots of #169 checked 2026-11-03.

## G28 Small ends after batch 11

- **Done** (#113, PR #114): no test execs a file it just wrote (macOS vets a new executable on its
  first exec): the fakes run through `/bin/sh` with shared helpers, and `cargo xtask fakes`, a
  step of every `ci`, fails on a new one.
- **Measured:** 12 copies at once: engine `issues` 14.9 → 2.8 s, `details` 14.7 → 2.9 s, the
  source crate 16.6 → 4.0 s (2026-11-03).
- **Done** (batch 14):
  - **A silent rerun is a rerun** (swarm-core#20, PR #24; pinned by #133): `gate` notes a gate of
    the same failed, unchanged stack `rerun of <sub>:` itself, so a flake it answers is no
    send-back.
  - **The minimap shows the tickets' strips** (#134, PR #135; swarm-timeline#222, PR #225): each
    ticket's stages in the minimap, the most important winning its column.
- **Left:** none.
- **Evidence:** `xtask/src/fakes.rs`, `crates/engine/tests/support`.

## G29 Merges as markers, and one colour per stage

- **Done:**
  - **Merges as markers** (#115, PR #116; #119, PR #120; swarm-timeline#211, PR #212): each merge
    on Merges to main and Tickets merged is a range of no length at its time, drawn as a 3 px
    marker by the widget's packing, sitting on its ticket's merge dot; its tooltip has no
    duration line.
  - **A ticket's tooltip in its strip's colours** (#117, PR #118): `Stage::colour` is the one
    source for the strip, the tooltip, the widget's segment line and Details; a wait's gate runs
    show as *in the gate* lines.
  - **No Gate runs or Ready checks rows** (#117, PR #118, the owner reversing #84): the Gates
    band's figures, the Factory's Gate list, `gates.list` and Details' gate stories stay;
    `ui.inspect` 16.
  - **Tooltips whole** (#121, PR #122; swarm-timeline#213, PR #214): drawn in egui's tooltip
    layer, bounded by the window, not cut to the timeline's rows.
  - **A tooltip's box fits its text** (#124, PR #125; swarm-timeline#215, PR #216): sized from the
    text as laid out, not 7 px a character (egui's font is ~7.9 px, so long lines spilled).
- **Left:** labels and readouts still use a fixed width a character (the widget's
  `LABEL_CHAR_WIDTH`).
- **Evidence:** `rows::tests::a_tooltip_names_each_stage_in_its_strip_segments_colour`,
  `timeline::tests::a_merge_of_no_length_is_drawn_and_picked`, `crates/ui/tests/timeline_pane.rs`
  (`crates/ui/tests/gates_pane.rs` went with the band, #128); screenshots checked 2026-11-03.

## G30 The owner's ticket bars, and no Gates band

- **Done** (the owner's, 2026-11-03):
  - **A sent-back ticket's red corner** (#127, PR #130; swarm-timeline#219, PR #221): the
    factory's brick's corner on its timeline bar, top right, the count inside when more than once
    (swarm-timeline's range corner, §6.6); the red diamonds along its foot gone.
  - **Merge dots optional** (#127): off by default, the bar ending at its merge says it;
    `rows.set {"mergeMarks": true}` and View ▸ Merge Marks bring them back, saved with the rows.
  - **A red dot where a ticket was given up** (#127): abandoned, or closed on GitHub without a
    merge (not one stopped `done`, nor one whose pull request GitHub merged).
  - **No Gates band** (#128, PR #129): the band, View ▸ Gates and `--panes gates` gone;
    `UiState` 12 drops an older state's `gates`, `ui.inspect` 17; `gates.list`, `stats.get`, the
    Factory's figures and Gate list stay.
  - **A selected bar keeps its hatch when hovered** (swarm-timeline#218, PR #220).
  - **Alt + click or drag shows a ticket's end** (#137, PR #138; swarm-timeline#228, PR #229,
    and #230, PR #231; the owner's, 2026-11-04): a plain click selects the ticket with the
    playhead, and the Factory, at its claim; with Alt, at the end of its bar, so the Factory shows
    it merged or given up. Pressing or letting go of Alt mid-drag hops to the other end.
- **Measured:** 389 tests, `cargo nextest run --workspace` (2026-11-04); checked by eye on
  mc-engine: red corners on the sent-back bars, no green dots, one red dot at an abandoned
  ticket's end; Alt + click on mc-engine's #309 showed 13:23:43, its brick on main.
- **Evidence:** `rows::tests::a_corner_counts_send_backs_and_a_red_dot_marks_only_what_was_given_up`,
  `timeline::tests::a_failure_survives_packing`, engine
  `merge_marks_are_off_until_rows_set_asks_for_them`, `saved::tests` (version 11's gates dropped),
  `crates/ui/tests/layout.rs`, `factory_pane::a_click_on_a_tickets_bar_shows_its_start_and_alt_click_its_end`.

## G31 No ghost ticket bars

- **Done** (batch 16; the owner's, 2026-11-06, on Nsight):
  - **Main's merges end tickets** (#142, PR #145): main's full merge messages are read, so a
    GitHub merge's PR, a GitLab batch's `Closes #12, #3, and #9` and its `See merge request`
    trailer end an unfinished ticket that tracking never logged merged; the recovered merge
    links to its main commit. A mention that isn't a closing keyword closes nothing.
  - **A pause is an instant** (#145): a paused, blocked or handed-off stop no longer runs on as a
    stretch; the Factory shelves the ticket with its reason.
  - **Silence ends a bar** (#145): a ticket with no update for a day, whose holder ended its
    session or also fell silent, ends at its last event, *outcome unknown*, neither merged nor
    abandoned; counted from the swarm's last event, never the viewer's clock.
  - **The Windows full gate is green** (#144, PR #145): Unix-only test helpers gated, Windows
    paths read for the platform asked, a live test that could poll twice on a slow machine.
  - **CI is always local** (#140, PR #141; the owner's, 2026-11-05): no GitHub workflow or daily
    run; `ci --changed`, the coordinator's gate and the periodic full gate run on our machines.
- **Measured:** on frozen Nsight refs, 672 ticket bars kept, bars over 500 h from 65 to 0, 43
  merges recovered from main, 29 outcomes unknown; #12 is 1 h 19 m (was 570 h) (the worker's,
  Windows, 2026-11-06). 401 tests, `cargo nextest run --workspace` (2026-11-06, macOS).
- **Evidence:** `crates/domain/src/lifecycle_tests.rs`, `merges::tests::gitlab_batch_reads_closures_from_the_body_not_incidental_mentions`,
  `source` `reads_multiline_gitlab_merges_without_losing_adjacent_records`, `Ticket::complete_from_main`,
  `Ticket::bound_unresolved`, `tickets::STALE_SECONDS`; screenshots in `docs/screenshots/`.

## G32 Idle benches together, and dragging across rows

- **Done** (batch 17; the owner's, 2026-11-06):
  - **One tile for the idle** (#147, PR #148): the Factory's on-shift workers with no ticket and
    no check share one tile, `N idle · names`, its tooltip every name and what idle means: an
    unpaused session with nothing at the bench, read from the tracking log, so a missing session
    stop can leave a worker idle there. `FactoryScene::idle`; `ui.inspect` 18
    (`factory.drawn.idleBenches`).
  - **Dragging crosses rows; X holds the row** (swarm-timeline#233, PR #234; pinned by #148): a
    drag on the timeline selects the ticket under the pointer on any row, a move straight up or
    down included; X held keeps it to the row where the drag began, with or without Alt.
- **Measured:** 404 tests, `cargo nextest run --workspace` (2026-11-06); swarm-timeline 698.
- **Evidence:** `factory::tests::idle_group_follows_the_playhead_and_does_not_claim_availability`,
  `factory_benches::the_idle_tiles_tooltip_contains_all_names_and_the_availability_explanation`,
  `factory_pane::dragging_crosses_ticket_rows_and_x_constrains_with_or_without_alt`; screenshots
  in `docs/screenshots/idle-benches-*.png`.

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

- **Done** (#74, PR #79): Collapse is a typed choice, `off` / `empty` / `window` (the old
  `true`/`false` read as `empty`/`off`; saved rows spec version 2). `window` leaves out the rows
  with no bar in the visible window, and a worker's group with none folds away whole;
  `rows.window {start, end, keep}` says which. Set by `rows.set`, `ui.set {collapse}`, View ▸
  Collapse to Window, a toolbar toggle and `--collapse`. The pane hides rows through the widget's
  own per-row flag, so nothing reloads: the selection's rows are kept, selecting a hidden ticket
  shows it, a row the person hid stays hidden, and the window's hiding isn't saved. It follows a
  pan once the window has been still 250 ms with no button held (no rows moving mid-drag).
- **Measured:** 5–137 µs a collapse on mc-engine by worker (`ui.inspect`'s
  `timeline.collapseUs`, 2026-11-02).
- **Left:** the minimap shows only the shown rows' lanes (the widget's way with hidden rows).
- **Evidence:** `crates/domain/src/rows.rs`, `crates/engine` (`rows.window`),
  `crates/ui/tests/collapse_window.rs`; screenshots checked 2026-11-02.

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
- **Left:** nothing open (G19 done).
- **Evidence:** `crates/domain/src/factory.rs`, `moments.rs`, `stats.rs`, `colours.rs`;
  `crates/ui/src/factory*.rs`.
- **Estimate:** 3–6 agent-hours, in issues of their own.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-06 | minor | Batch 17: G32 done (#147, swarm-timeline#233); no gaps open |
| 2026-11-06 | minor | Batch 16: G31 done (#140, #142, #144); no gaps open |
| 2026-11-05 | minor | Batch 15: G30 gains Alt + click (#137, swarm-timeline#228, #230) |
| 2026-11-04 | minor | Batch 14: G28 done (#134, swarm-core#20, swarm-timeline#222); no gaps open |
| 2026-11-04 | minor | Batch 13: G30 new and done (#127, #128, swarm-timeline#218, #219); G28's leftovers filed (swarm-core#20, swarm-timeline#222) |
| 2026-11-03 | minor | G29 done (#124, swarm-timeline#215) |
| 2026-11-03 | minor | Batch 12: G28 partial (#113); G29 new (#115, #117, #119, #121); G21 reversed |
| 2026-11-03 | minor | Batch 11: G25 done (#106); G26, G27 new and done (#108, #110, swarm-timeline#205, #206); G28 new |
| 2026-11-03 | minor | Batch 10: G24 done (#92–#98, swarm-timeline#197, #198, swarm-core#14); G25 new |
| 2026-11-02 | minor | Batch 9: the owner's review: G20–G23 done (#83–#86, swarm-timeline#191); G24 new |
| 2026-11-02 | minor | Batch 8: G6, G19 done (#74–#77, swarm-core#12); no gaps open |
| 2026-11-02 | minor | Batch 7: G8, G9, G17, G18 done (#67–#69); G19 new; M3 built |
| 2026-11-02 | minor | Batch 6: G13, G15 done (#60, #62); G9 grew (#61, a gates pane); G18 new; focus G9, G18, G17 |
| 2026-11-02 | minor | Batch 5: G4, G5, G16 done (#53–#55); G17 new; M2 built; focus G15, G9, G13 |
| 2026-11-02 | minor | Batch 4: G2, G3, G12, G14 done (#46–#48, swarm-timeline#183, #188); G15, G16 new; focus G5, G4, G16 |
| 2026-11-01 | minor | Batch 3: G10, G11 done (#9, #28); G9 grew (#31–#33, #40, #42); G14 new; focus G2, G3, G12 |
| 2026-11-01 | minor | Batch 2: G7 done (#17); G9 partial (#18, #20, #22, #15); G12, G13 new; G11 up |
| 2026-10-31 | minor | Batch 1: G1 done (#2); G10 (#9) and G11 new; ranking moved |
| 2026-10-31 | major | First version, from swarm-core's template |
