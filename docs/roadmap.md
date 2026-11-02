<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Roadmap

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-02 · **Change:** minor (batch 4: the control channel and MCP, the Details pane, the widget's highlight and durations) · **Target:** M2

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

1. **The state survives a restart** ([G5](gaps.md#g5-the-state-survives-a-restart)): the last of M2
2. **Watching a live swarm** ([G4](gaps.md#g4-watching-a-live-swarm))
3. **The details pane and the timeline select together** ([G16](gaps.md#g16-the-details-pane-and-the-timeline-select-together))

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-02 | minor | Batch 4: M2 mostly built (G2, G3 done); focus moved to G5, G4, G16 |
| 2026-11-01 | minor | Batch 3: focus moved to G2, G3, G12 (G10, G11 done) |
| 2026-11-01 | minor | Batch 2: focus moved (G11 up); the Factory and statistics are M3 work done early |
| 2026-10-31 | minor | Batch 1: M1 is the tickets' timeline in clock time; the clock left M2; focus moved |
| 2026-10-31 | major | First version, from swarm-core's template |
