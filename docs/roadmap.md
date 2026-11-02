<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Roadmap

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-02 · **Change:** minor (batch 8: collapse to the window, the loose ends of M2 and M3) · **Target:** M3

Swarmception's milestones, the gates between them and the ranked **Current focus**. Detail lives
in [gaps.md](gaps.md); this page keeps one short sentence a milestone and one line a focus item
([progress-docs.md](progress-docs.md) rule 8). The coordinator edits it, in a batch's progress
commit, only when a milestone or the ranking moves.

## Milestones

| Id | Milestone | Done when |
|---|---|---|
| M1 | Look at what the swarm did | Open any swarm's tracking branch and pan and zoom its tickets' timeline under the merges onto main, packed flat or by worker, in clock time |
| M2 | Read it and drive it | A details pane, the control channel and MCP, the state kept across runs |
| M3 | Understand it | Live watching and analytics panes: gates, send-backs, throughput, models |

## Gates

A milestone is passed when its gaps are closed, with tests and (for UI) a visual check
([progress-docs.md](progress-docs.md) rule 7), and the owner agrees. An architecture review runs
after the first ~20 merges and at each stage gate ([the swarm protocol](swarm-protocol.md#the-coordinators-loop)
step 10).

## Current focus

The same ranked list as `ROADMAP.md`'s **What's next**: its rank, its bold name, its gap link and
at most a short clause. Workers pick from the top ([the swarm protocol](swarm-protocol.md#picking-work)).

Nothing ranked: every gap is closed. M1, M2 and M3 wait on the owner's pass; new work comes
from the owner's review.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-02 | minor | Batch 8: G6, G19 done; no gaps open, no focus, M1–M3 with the owner |
| 2026-11-02 | minor | Batch 7: M3 built (G9 done), with the owner to pass M1–M3; focus G6, G19 |
| 2026-11-02 | minor | Batch 6: G13, G15 done, G9 grew (a gates pane); focus moved to G9, G18, G17 |
| 2026-11-02 | minor | Batch 5: M2 built (G4, G5, G16 done), with the owner to pass it; focus moved to G15, G9, G13 |
| 2026-11-02 | minor | Batch 4: M2 mostly built (G2, G3 done); focus moved to G5, G4, G16 |
| 2026-11-01 | minor | Batch 3: focus moved to G2, G3, G12 (G10, G11 done) |
| 2026-11-01 | minor | Batch 2: focus moved (G11 up); the Factory and statistics are M3 work done early |
| 2026-10-31 | minor | Batch 1: M1 is the tickets' timeline in clock time; the clock left M2; focus moved |
| 2026-10-31 | major | First version, from swarm-core's template |
