<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep the headings the shared protocol links to: Capabilities, The gate's kinds here,
     Each batch. -->
# Running the swarm

> **Last reviewed:** 2026-10-31 · **Last updated:** 2026-10-31 · **Change:** major (first version, from swarm-core's template) · **Target:** M1

Swarmception's part of running many agents. The protocol every swarm shares (the roles, starting a
worker, picking and claiming work, tracking, watching PRs, helpers, the coordinator's loop,
staying awake, the periodic full gate) is [swarm-protocol.md](swarm-protocol.md), a pinned copy
from swarm-core. This page holds what is this repo's own.

## Machine setup

Once per machine, before an agent starts there:

1. **Tools:** Rust (the workspace's `rust-version`), [cargo-nextest](https://nexte.st),
   [gh](https://cli.github.com), git; on Windows, Git Bash.
   On Linux, the libraries eframe's window needs (`.github/workflows/ci.yml` lists them).
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
  `layers` and the pinned copies' check.
- **full:** the whole `cargo xtask ci`, the periodic full gate's.

The shared files that conflict most in a stack are AGENTS.md §6's.

## Each batch

In the coordinator's progress commit ([The coordinator's loop](swarm-protocol.md#the-coordinators-loop)
step 4), after folding the batch into `docs/gaps.md`:

- refresh the scorecard (`docs/scorecard.md`: by hand, or `cargo xtask scorecard` once this repo
  generates it);
- the test count (`cargo nextest run --workspace`).

## What CI runs

- `ci` (ubuntu-latest): `cargo xtask ci`, about 10 minutes cold.

`.github/workflows/ci.yml` runs `cargo xtask ci`. The rules it keeps:

- No run on draft PRs or on the `tracking` branch; one when a PR is marked ready, on each push to
  a ready PR, and on `main`. A new run of a ref cancels the one still going.
- **Fetching Swarm Core:** it is a private repository, so the job points its URL at a read-only
  token before cargo runs: a fine-grained token with **Contents: read** on
  `mornycorp/swarm-core` **and `mornycorp/swarm-timeline`** (the timeline widget is a private
  git dependency too), stored as this repository's `SWARM_CORE_TOKEN` secret (its owner makes
  it, once).
- **Branch protection on `main`:** no required status checks, since the coordinator's local gate
  is the check before a merge, and `land` cancels a PR's runs when it lands it (a required check
  would block the merge). Require PRs, and allow merge commits.
- Nothing merges while `main`'s CI is red.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-31 | major | First version, from swarm-core's template |
