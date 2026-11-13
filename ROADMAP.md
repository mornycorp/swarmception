<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Swarmception roadmap

> **Last reviewed:** 2026-11-13 · **Last updated:** 2026-11-13 · **Change:** minor (batch 21: G34 done, the UI as swarm-core's contract has it, six issues; no gaps open) · **Target:** M3

One page: the stage, the headline numbers and what's next. Detail lives in
[docs/gaps.md](docs/gaps.md), milestones in [docs/roadmap.md](docs/roadmap.md)
([docs/progress-docs.md](docs/progress-docs.md) rule 8).

**Stage:** M1 built (the tickets' timeline, packed rows, clock time, the macOS menu bar; batch 1), with the owner to pass it. Batch 2 added issue titles and links, a colour per ticket, and early M3 work: the Factory pane and statistics. Batch 3 revised the Factory from the owner's review (the wall, stepping by moment), took colour scheme B, cut the timeline's toolbar to what Swarmception needs, and ended abandoned tickets and gates. Batch 4 built most of M2: a Details pane that tells the story of what is selected, and the control channel and MCP so an agent can drive the app; the widget gained a highlight apart from the selection and durations in hours and minutes. Batch 5 finished M2, with the owner to pass it: the state is kept across runs, the app watches a live swarm (a fetch every 30 s, the view following the end), and what the Details pane shows is highlighted on the timeline. Batch 6 began M3 with a Gates band (every gate run and ready check along the timeline, with pass rates and causes), and finished the automation (MCP through the running window, PNG screenshots, a scripting client) and an incremental issue fetch (0.5 s a reload on mc-engine). Batch 7 finished M3 with a Workers pane (model and effort compared, with sample sizes), made the panes share the window (the timeline keeps its rows; bands fold), and added File ▸ Open and Open Recent. Batch 8 closed the last gaps: Collapse to the window, both Gates lanes per hour, one definition of a gate run, a failed ready check's cause (swarm-core#12), and the Workers window and screenshots over automation. Batch 9 took the owner's review: a ticket's rework after a send-back in its own colour, gate runs and ready checks as timeline rows, the Factory following the timeline (no clock of its own, selection from the timeline), and folding the timeline's quiet time (swarm-timeline#191). Batch 10 tied off the review's loose ends: a row's lanes sized over the visible window (swarm-timeline#197), the minimap marking folds, ready checks in the Factory's Gate list, a send-back's rework logged by swarm-core's `prwatch` (swarm-core#14), tests that wait on the session rather than the clock, and two of the owner's bugs (the Factory's benches with dozens of workers; debug-build markers over the bars). M1, M2 and M3 are built and wait on the owner; Batch 11 drew each ticket in its own colour all along, its stages as a strip along its foot and its send-backs and merge as marks (swarm-timeline#205, #206: the owner's, on mc-engine's #169), kept failures visible when bars pack, read a gate's failure as a send-back, and fixed a test slowed by macOS's first-exec check. Batch 12 made merges zero-length markers, gave a ticket's tooltip its strip's colours, removed the Gate runs and Ready checks rows (the owner reversing batch 9), kept tooltips whole over a short timeline, and stopped tests exec'ing freshly written fakes. Batch 13 took the owner's next look: a sent-back ticket shows the factory's red corner on its bar (swarm-timeline#219), merge dots are optional, a red dot marks a ticket given up, and the Gates band is gone. Batch 14 closed G28: a silent gate rerun is a rerun (swarm-core#20), and the minimap shows the tickets' stages (swarm-timeline#222); swarm-core also gained `claim --take-over`, stacked PRs that survive `land`, `ready --no-ci` gated with tests, and one test run at a time on a 24 GB machine. Batch 15 took the owner's ask: Alt + click or drag on a ticket's bar selects it with the playhead, and the Factory, at its end (swarm-timeline#228, #230). Batch 16 ended the ghost ticket bars the owner saw on Nsight (main's merge messages, GitLab batches included, close tickets tracking never merged; a pause is an instant; a day's silence ends a bar, outcome unknown), fixed the Windows full gate that turned `main` red, and made CI local only (the owner's: no GitHub Actions). Batch 17 took two more of the owner's: the Factory's idle workers share one tile, its tooltip naming them and saying idle is read from the tracking log, not proven; and dragging on the timeline crosses rows again, X held to keep to the starting row, with or without Alt (swarm-timeline#233). Batch 18 brought swarm-core up to 57fb6a2 (its later protocol, the backends layer, `Project::proof`) and took the owner's next: colours from swarm-core's theme, a ticket's colour its pick of the theme's 32-colour range and each stage the token named for what it means, with `theme.get` giving the theme as data. Batch 19 drew them in both looks: each colour the theme's dark and light pair, the window's look picking one (#154). Batch 20 took swarm-core's shared palette (d4df5a2), egui drawn in it, and a review against swarm-core's UI contract opened G34. Batch 21 closed it: a saved state this build can't read is left alone, `repo.open` checks its folder off the UI's thread, File's commands come from the registry, one file writer and one process runner below the UI (`xtask writes`), typed egui ids, and words from a catalog. No gaps are open; M1–M3 wait on the owner.

## Headline numbers

Each cell is the number and its source (a test, a command, a generated doc) and links the gap or
doc that holds the detail. Measured beats estimated: say which.

| What | Number | Source |
|---|---|---|
| Scorecard rows done | 8 of 8 (M1); 4 of 4 (M2); 6 of 6 (M3), measured | [docs/scorecard.md](docs/scorecard.md) |
| Gaps open | 0 | [docs/gaps.md](docs/gaps.md) |
| Tickets landed per active hour | mc-engine 5.6 (125 in 22.4 h), swarm-timeline 9.0 (96 in 10.6 h), Swarmception 6.2 (54 in 8.74 h), measured 2026-11-03 (`stats.get`) | [G9](docs/gaps.md#g9-swarm-analytics-panes) |
| Tests | 437, measured 2026-11-13 (`cargo nextest run --workspace`; batch 21's G34) | [docs/scorecard.md](docs/scorecard.md) |
| Rows for swarm-timeline's 84 tickets | 12 flat, measured 2026-10-31 (82 before #5) | [docs/scorecard.md](docs/scorecard.md) |

## What's next

The same ranked list as [docs/roadmap.md › Current focus](docs/roadmap.md#current-focus).

None: every gap is closed. New work comes from the owner (M1–M3 wait on their pass), or from a review.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-13 | minor | Batch 21: stage, tests, gaps open; G34 done, no focus |
| 2026-11-13 | minor | Batch 20: stage, gaps open; focus G34 |
| 2026-11-13 | minor | Batch 19: stage, tests, gaps open; G33 done |
| 2026-11-13 | minor | Batch 18: stage, tests, gaps open; G33 partial (#154) |
| 2026-11-06 | minor | Batch 17: stage, tests; G32 done, no gaps open |
| 2026-11-06 | minor | Batch 16: stage, tests; G31 done, no gaps open |
| 2026-11-05 | minor | Batch 15: stage, tests; no gaps open |
| 2026-11-04 | minor | Batch 14: stage; G28 done, no gaps open |
| 2026-11-04 | minor | Batch 13: stage, tests; focus G28 |
| 2026-11-03 | minor | Batch 12: stage, landed per active hour, tests; focus G29, G28 |
| 2026-11-03 | minor | Batch 11: stage, landed per active hour, tests; focus G28 |
| 2026-11-03 | minor | Batch 10: stage, landed per active hour (Swarmception's own too), tests; focus G25 |
| 2026-11-02 | minor | Batch 9: stage, gaps open, landed per active hour, tests; focus G24 |
| 2026-11-02 | minor | Batch 8: stage (no gaps open), gaps open, landed per active hour, tests; focus emptied |
| 2026-11-02 | minor | Batch 7: stage (M3 built), scorecard rows, gaps open, tests; focus moved |
| 2026-11-02 | minor | Batch 6: stage, scorecard rows, gaps open, landed per active hour, tests; focus moved |
| 2026-11-02 | minor | Batch 5: stage (M2 built), scorecard rows, gaps open, landed per active hour, tests; focus moved |
| 2026-11-02 | minor | Batch 4: stage, scorecard rows (M2), gaps open, landed per active hour, tests; focus moved |
| 2026-11-01 | minor | Batch 3: stage, scorecard rows, gaps open, landed per active hour, tests; focus moved |
| 2026-11-01 | minor | Batch 2: stage, gaps open, landed per active hour, tests; focus moved |
| 2026-10-31 | minor | Batch 1: stage, gaps open, packed rows; focus moved |
| 2026-10-31 | major | First version, from swarm-core's template |
