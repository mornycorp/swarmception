<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Kept by hand to start with. Once it grows, generate it: a
     `cargo xtask scorecard` of its own writes it from `scorecard/<area>.toml` rows (`id`, `target`, `status`
     done|partial|missing, `note`, optional `issue` and `gap`) and checks it with `--check`; list
     a generated doc in the `Project`'s `generated`, so `stack` regenerates it on a conflict. -->
# Scorecard

> **Last reviewed:** 2026-11-03 · **Last updated:** 2026-11-03 · **Change:** minor (batch 10: the review's loose ends, the Factory's benches, debug-build markers) · **Target:** M3

Per area, what Swarmception does, measured: each row is **done** (a command or code path, tests,
and for UI a visual check), **partial** or **missing**, with its evidence. The PR that changes a
row's status updates it ([progress-docs.md](progress-docs.md) rule 6); gaps live in
[gaps.md](gaps.md).

## Summary

| Area | Done | Partial | Missing |
|---|---|---|---|
| Timeline (M1) | 8 | 0 | 0 |
| Read and drive (M2) | 4 | 0 | 0 |
| Understand (M3) | 6 | 0 | 0 |

## Measured

<!-- The numbers the coordinator refreshes once per batch, each with its source and date: test
     counts, commands, open gaps and their estimates. -->

| What | Number | Source |
|---|---|---|
| Tests | 370 passed | `cargo nextest run --workspace`, 2026-11-03 |
| Landed per active hour | mc-engine 5.8, swarm-timeline 10.0, Swarmception 6.4 | `stats.get`, 2026-11-03 |
| Rows for swarm-timeline's tickets | 12 flat | `rows.list`, 2026-10-31 |

## Timeline (M1)

| Id | Target | Status | Evidence |
|---|---|---|---|
| TL-1 | Open any swarm's tracking branch with `--repo <path>` | done | `repo.open`; `crates/source` tests; `crates/engine/tests/commands.rs` |
| TL-2 | Every ticket's life (claimed, working, waiting, merged) on one timeline, under the merges onto main | done | `crates/domain/src/tickets.rs`, `rows.rs` tests; screenshot checked 2026-10-31 (#1) |
| TL-3 | Merges onto main, from git | done | `crates/source` and `crates/model/src/merges.rs` tests (#1) |
| TL-4 | Tickets flat or by worker (machine / worker / ticket) | done | `rows.set {grouping}`; `rows.rs` tests |
| TL-5 | Collapse: no empty rows | done | `rows.set {collapse}`; `rows.rs` tests; or none outside the window, following a pan: `rows.window`, `crates/ui/tests/collapse_window.rs`, screenshots checked 2026-11-02 (#74) |
| TL-6 | Pan and zoom in time; View menu of panes; Font Awesome toolbar | done | `crates/ui`; screenshots checked 2026-10-31; Ctrl + drag pans on macOS (#11); the widget's toolbar cut to Fit, Zoom to Selection, Undo Zoom, zoom buttons and Δ, the scale, origin, minimap and click fixed by command (#35); its zoom buttons as plain as Fit (#48); quiet time folded (#86, swarm-timeline#191); a row's lanes sized over the visible window, the minimap marking folds (#97, swarm-timeline#197, #198) |
| TL-7 | A colour per ticket; merges as green 10 s spans; coloured tooltips; durations in h/min/s | done | `crates/domain/src/colours.rs`, `tooltip.rs` tests; scheme B; `timeline_pane.rs` durations test; screenshots checked 2026-11-02 (#15, #40, #35, #48); a send-back's rework from a logged `work` (#98, swarm-core#14) |
| TL-8 | Titles on bars and tooltips; open the issue or PR from a bar | done | `crates/source/src/github.rs`, `crates/ui/tests/ticket_links.rs` (#17) |

## Read and drive (M2)

| Id | Target | Status | Evidence |
|---|---|---|---|
| RD-1 | A details pane: the story of a ticket, gate run, session or merge, following the selection | done | `details.get`; `crates/engine/tests/details.rs`, `crates/ui/tests/details_pane.rs`; screenshots checked 2026-11-02 (#47); its subject's bars highlighted on the timeline (#55) |
| RD-2 | The control channel: drive the window as a person does | done | `--control`; `crates/automation/tests/tcp.rs`, `apps/app` control tests; the app driven on mc-engine 2026-11-02 (#46) |
| RD-3 | MCP and a headless server | done | `swarmception-cli serve`, `mcp`, `mcp --control` through the window, `send`; `apps/cli/tests/automation.rs`, the app's control tests; PNGs checked 2026-11-02 (#46, #60); screenshots as MCP image content, the Workers window set by `ui.set` (#76) |
| RD-4 | The state kept across runs | done | `crates/ui/src/saved.rs`; `crates/ui/tests/saved_state.rs` (a headless restart; corrupt and newer files); screenshots checked 2026-11-02 (#53) |

## Understand (M3)

| Id | Target | Status | Evidence |
|---|---|---|---|
| UN-1 | The Factory: the swarm at a time as a factory, following the timeline | done | `factory.at`; `crates/ui/tests/factory_pane.rs`; screenshots checked 2026-11-01 (#18); a hovered chip highlights its bars, leaving the selection alone (#48); no clock of its own, the timeline's selection selects in it (#85); dozens of workers' benches read apart (#94); ready checks in its Gate list (#95) |
| UN-5 | The Factory steps back and forth by key moment | done | `factory.moments`, `factory.step`; `crates/domain/src/moments.rs` tests; UI test by clicks and keys (#33) |
| UN-2 | Statistics: landed per active hour and the rest, for the visible window | done | `stats.get`; `crates/domain/src/stats.rs` tests; checked by hand 2026-11-01 (#20, #22) |
| UN-3 | Gates analytics: runs, causes, durations | done | `gates.list`; the Gates band; `crates/domain/src/gates.rs`, `crates/ui/tests/gates_pane.rs`; screenshots checked 2026-11-02 (#61); both lanes per hour, one gate-run definition with `stats.get` (#75); a failed ready check's cause and step (#77, swarm-core#12); runs as timeline rows (#84) |
| UN-6 | Model and effort compared per worker | done | `workers.compare`; View ▸ Workers; `crates/domain/src/workers.rs`, `crates/ui/tests/workers_pane.rs`; screenshots checked 2026-11-02 (#67) |
| UN-4 | Live watching | done | `live.set`, `live.get`; `crates/engine/tests/live.rs` (a remote that gains events), `crates/ui/tests/live_reload.rs`; mc-engine watched 2026-11-02 (#54) |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-03 | minor | Batch 10: TL-6, TL-7, UN-1 evidence (#94, #95, #97, #98); measured numbers |
| 2026-11-02 | minor | Batch 9: TL-6, UN-1, UN-3 evidence (#84–#86); measured numbers |
| 2026-11-02 | minor | Batch 8: TL-5, RD-3, UN-3 evidence (#74–#77); measured numbers |
| 2026-11-02 | minor | Batch 7: UN-6 done; measured numbers |
| 2026-11-02 | minor | Batch 6: RD-3, UN-3 done; UN-6 added; measured numbers |
| 2026-11-02 | minor | Batch 5: RD-1 widened, RD-4 and UN-4 done; measured numbers |
| 2026-11-02 | minor | Batch 4: the Read and drive area (RD-1–RD-4); TL-6, TL-7, UN-1 widened; measured numbers |
| 2026-11-01 | minor | Batch 3: TL-6, TL-7 widened; UN-5; measured numbers |
| 2026-11-01 | minor | Batch 2: TL-7, TL-8; the Understand area; measured numbers |
| 2026-10-31 | major | First version, from swarm-core's template |
