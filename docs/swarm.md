<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep the headings the shared protocol links to: Capabilities, The gate's kinds here,
     Each batch. -->
# Running the swarm

> **Last reviewed:** 2026-10-31 · **Last updated:** 2026-11-05 · **Change:** minor (CI is always local, #140) · **Target:** M1

Swarmception's part of running many agents. The protocol every swarm shares (the roles, starting a
worker, picking and claiming work, tracking, watching PRs, helpers, the coordinator's loop,
staying awake, the periodic full gate) is [swarm-protocol.md](swarm-protocol.md), a pinned copy
from swarm-core. This page holds what is this repo's own.

## Machine setup

Once per machine, before an agent starts there:

1. **Tools:** Rust (the workspace's `rust-version`), [cargo-nextest](https://nexte.st),
   [gh](https://cli.github.com), git; on Windows, Git Bash.
   On Linux, the libraries eframe's window needs: `libxkbcommon-dev libwayland-dev
   libgl1-mesa-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev`.
2. **GitHub:** `gh auth login --hostname github.com`, then `gh auth setup-git`, so git and
   cargo (`.cargo/config.toml` sets `net.git-fetch-with-cli`) fetch with gh's token. The account
   must read this repository and [swarm-core](https://github.com/mornycorp/swarm-core), a private
   git dependency. Set `GH_REPO=mornycorp/swarmception`, or always pass `--repo mornycorp/swarmception` to gh.
3. **The clone:** `gh repo clone mornycorp/swarmception`, and `git fetch origin tracking` so the
   timeline is there to read.
4. **The pre-push hook** (optional): `git config core.hooksPath scripts/hooks`.
5. **`cargo xtask ci` passes** on `main` before any agent starts.

## Capabilities

What a machine can run. An issue that needs one carries its label; a worker skips the labels its
machine lacks (the protocol's [Picking work](swarm-protocol.md#picking-work)).

| Label | Needs | Typical tasks |
|---|---|---|
| (none) | the machine setup above | most tasks |
| `full-gate` | a machine capability, not a label: `claim` runs the periodic full gate there when it is due. The default: a Windows machine with room for a build (`SWARMCEPTION_FULLGATE=on\|off` overrides it) | the periodic full gate of `main` |
| `needs::human` | a person | credentials, decisions, anything an agent can't do |


## The gate's kinds here

`cargo xtask gate` picks one of the protocol's kinds from the change
([The coordinator's loop](swarm-protocol.md#the-coordinators-loop) step 3):

- **docs:** only Markdown.
- **tooling:** what this repo's planner counts as tooling (`Hooks::gate_kind` in `xtask/`): until
  it has one, nothing is, and every code change is smoked.
- **smoke:** anything else. This repo's `cargo xtask ci --smoke` runs: fmt, clippy, the build,
  `layers`, `fakes` (no test execs a fake it wrote, #113) and the pinned copies' check.
- **full:** the whole `cargo xtask ci`, the periodic full gate's.

The shared files that conflict most in a stack are AGENTS.md §6's.

## Each batch

In the coordinator's progress commit ([The coordinator's loop](swarm-protocol.md#the-coordinators-loop)
step 4), after folding the batch into `docs/gaps.md`:

- refresh the scorecard (`docs/scorecard.md`: by hand, or `cargo xtask scorecard` once this repo
  generates it);
- the test count (`cargo nextest run --workspace`).

## What CI runs

**CI is always local** (owner, 2026-11-05, #140): nothing runs on GitHub. GitHub Actions is turned
off for the repository and there is no workflow; the coordinator and every worker run
`cargo xtask ci` on their own machines, and those runs are what merges rest on:

- **A worker:** `cargo xtask ci --changed` before `ready` (and `ready` runs it again on the rebased
  tree).
- **The coordinator:** the local gate on the stacked tree before `land`
  ([the swarm protocol](swarm-protocol.md#the-coordinators-loop) step 3), and the periodic full gate
  of `main` ([The periodic full gate](swarm-protocol.md#the-periodic-full-gate)); a full gate that
  fails on `main` files the `main-red` issue, and nothing merges until it is fixed.
- **No remote checks:** a PR has none, so `prwatch` reports none, `land` has none to cancel and
  nothing waits on one; nobody runs `gh workflow run`, `gh run list` or `gh pr checks`. Where the
  shared protocol speaks of `main`'s workflows or a PR's checks, here there are none.
- **Branch protection on `main`:** no required status checks. Require PRs, and allow merge
  commits.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-05 | minor | CI is always local (owner, #140): no workflow, no daily GitHub run, no remote checks |
| 2026-11-03 | minor | The smoke runs `fakes` too (#113) |
| 2026-10-31 | minor | CI runs once a day on `main` (owner), not on pushes or PRs |
| 2026-10-31 | major | First version, from swarm-core's template |
