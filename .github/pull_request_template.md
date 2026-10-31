## What

<!-- One paragraph: what changed and why. -->

Closes #<!-- issue number; the merge closes it -->

## Progress

<!-- A note for the coordinator, read at merge; it never lands in the repo. The coordinator updates
     docs/gaps.md from it once per batch, and ROADMAP.md and docs/roadmap.md when a headline
     number, a milestone or the ranking moves (docs/progress-docs.md rules 6 and 8). -->

- **Measured:** <!-- numbers, each with its source (test, command, report) and date; or "nothing measured" -->
- **Moves:** <!-- gap ids (G4), milestone; or "none" -->
- **Still open:** <!-- what this leaves undone, with issue numbers if filed -->
- **Notes:** <!-- anything the coordinator could miss from the diff alone: caveats, surprises, decisions taken -->

## Checklist (AGENTS.md › Before you finish a task)

- [ ] `cargo xtask ci --changed` passes locally (fmt, clippy, tests and doctests, layers, wasm, for what the change affects; the full run when its plan says everything)
- [ ] Tests for the change (and a regression test for any crash fix)
- [ ] Technical docs current; progress summaries left to the coordinator
- [ ] **Progress** section above filled in
- [ ] UI changes checked visually (snapshot PNG attached below)
- [ ] `log/devlog.md` entry written
- [ ] Events logged (`cargo xtask event`: claim, work, submit)

## Screenshots

<!-- Before / after from `cargo run -p swarmception-ui --example snapshot`, for UI changes. -->
