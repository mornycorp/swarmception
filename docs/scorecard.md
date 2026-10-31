<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Kept by hand to start with. Once it grows, generate it: a
     `cargo xtask scorecard` of its own writes it from `scorecard/<area>.toml` rows (`id`, `target`, `status`
     done|partial|missing, `note`, optional `issue` and `gap`) and checks it with `--check`; list
     a generated doc in the `Project`'s `generated`, so `stack` regenerates it on a conflict. -->
# Scorecard

> **Last reviewed:** 2026-10-31 · **Last updated:** 2026-10-31 · **Change:** major (first version, from swarm-core's template) · **Target:** M1

Per area, what Swarmception does, measured: each row is **done** (a command or code path, tests,
and for UI a visual check), **partial** or **missing**, with its evidence. The PR that changes a
row's status updates it ([progress-docs.md](progress-docs.md) rule 6); gaps live in
[gaps.md](gaps.md).

## Summary

| Area | Done | Partial | Missing |
|---|---|---|---|
| Timeline (M1) | 6 | 0 | 0 |

## Measured

<!-- The numbers the coordinator refreshes once per batch, each with its source and date: test
     counts, commands, open gaps and their estimates. -->

None yet.

## Timeline (M1)

| Id | Target | Status | Evidence |
|---|---|---|---|
| TL-1 | Open any swarm's tracking branch with `--repo <path>` | done | `repo.open`; `crates/source` tests; `crates/engine/tests/commands.rs` |
| TL-2 | Every ticket's life (claimed, working, waiting, merged) on one timeline, under the merges onto main | done | `crates/domain/src/tickets.rs`, `rows.rs` tests; screenshot checked 2026-10-31 (#1) |
| TL-3 | Merges onto main, from git | done | `crates/source` and `crates/model/src/merges.rs` tests (#1) |
| TL-4 | Tickets flat or by worker (machine / worker / ticket) | done | `rows.set {grouping}`; `rows.rs` tests |
| TL-5 | Collapse: no empty rows | done | `rows.set {collapse}`; `rows.rs` tests |
| TL-6 | Pan and zoom in time; View menu of panes; Font Awesome toolbar | done | `crates/ui`; screenshots checked 2026-10-31 |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-31 | major | First version, from swarm-core's template |
