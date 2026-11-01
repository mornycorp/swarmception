<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Kept by hand to start with. Once it grows, generate it: a
     `cargo xtask scorecard` of its own writes it from `scorecard/<area>.toml` rows (`id`, `target`, `status`
     done|partial|missing, `note`, optional `issue` and `gap`) and checks it with `--check`; list
     a generated doc in the `Project`'s `generated`, so `stack` regenerates it on a conflict. -->
# Scorecard

> **Last reviewed:** 2026-11-01 · **Last updated:** 2026-11-01 · **Change:** minor (batch 3: the Factory revised, scheme B, the reduced toolbar) · **Target:** M1

Per area, what Swarmception does, measured: each row is **done** (a command or code path, tests,
and for UI a visual check), **partial** or **missing**, with its evidence. The PR that changes a
row's status updates it ([progress-docs.md](progress-docs.md) rule 6); gaps live in
[gaps.md](gaps.md).

## Summary

| Area | Done | Partial | Missing |
|---|---|---|---|
| Timeline (M1) | 8 | 0 | 0 |
| Understand (M3) | 3 | 1 | 1 |

## Measured

<!-- The numbers the coordinator refreshes once per batch, each with its source and date: test
     counts, commands, open gaps and their estimates. -->

| What | Number | Source |
|---|---|---|
| Tests | 167 passed | `cargo nextest run --workspace`, 2026-11-01 |
| Landed per active hour | mc-engine 7.3, swarm-timeline 10.5 | `stats.get`, 2026-11-01 |
| Rows for swarm-timeline's tickets | 12 flat | `rows.list`, 2026-10-31 |

## Timeline (M1)

| Id | Target | Status | Evidence |
|---|---|---|---|
| TL-1 | Open any swarm's tracking branch with `--repo <path>` | done | `repo.open`; `crates/source` tests; `crates/engine/tests/commands.rs` |
| TL-2 | Every ticket's life (claimed, working, waiting, merged) on one timeline, under the merges onto main | done | `crates/domain/src/tickets.rs`, `rows.rs` tests; screenshot checked 2026-10-31 (#1) |
| TL-3 | Merges onto main, from git | done | `crates/source` and `crates/model/src/merges.rs` tests (#1) |
| TL-4 | Tickets flat or by worker (machine / worker / ticket) | done | `rows.set {grouping}`; `rows.rs` tests |
| TL-5 | Collapse: no empty rows | done | `rows.set {collapse}`; `rows.rs` tests |
| TL-6 | Pan and zoom in time; View menu of panes; Font Awesome toolbar | done | `crates/ui`; screenshots checked 2026-10-31; Ctrl + drag pans on macOS (#11); the widget's toolbar cut to Fit, Zoom to Selection, Undo Zoom, zoom buttons and Δ, the scale, origin, minimap and click fixed by command (#35) |
| TL-7 | A colour per ticket; merges as green 10 s spans; coloured tooltips | done | `crates/domain/src/colours.rs`, `tooltip.rs` tests; scheme B; screenshots checked 2026-11-01 (#15, #40, #35) |
| TL-8 | Titles on bars and tooltips; open the issue or PR from a bar | done | `crates/source/src/github.rs`, `crates/ui/tests/ticket_links.rs` (#17) |

## Understand (M3)

| Id | Target | Status | Evidence |
|---|---|---|---|
| UN-1 | The Factory: the swarm at a time as a factory, following the timeline | done | `factory.at`; `crates/ui/tests/factory_pane.rs`; screenshots checked 2026-11-01 (#18) |
| UN-5 | The Factory steps back and forth by key moment | done | `factory.moments`, `factory.step`; `crates/domain/src/moments.rs` tests; UI test by clicks and keys (#33) |
| UN-2 | Statistics: landed per active hour and the rest, for the visible window | done | `stats.get`; `crates/domain/src/stats.rs` tests; checked by hand 2026-11-01 (#20, #22) |
| UN-3 | Gates analytics: runs, causes, durations | partial | gate pass rate and causes in `stats.get`; no pane ([G9](gaps.md#g9-swarm-analytics-panes)) |
| UN-4 | Live watching | missing | [G4](gaps.md#g4-watching-a-live-swarm) |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-01 | minor | Batch 3: TL-6, TL-7 widened; UN-5; measured numbers |
| 2026-11-01 | minor | Batch 2: TL-7, TL-8; the Understand area; measured numbers |
| 2026-10-31 | major | First version, from swarm-core's template |
