<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep the heading the shared protocol links to: How many builds at once. -->
# Development

> **Last reviewed:** 2026-10-31 · **Last updated:** 2026-11-03 · **Change:** minor (tests' fakes run by `sh`, the `fakes` step, #113) · **Target:** M1

How to build, test and run Swarmception, and how many builds a machine takes.

## Build, test, run

```sh
cargo build
cargo nextest run --workspace     # or cargo test
cargo xtask ci                    # the whole gate: what the periodic full gate runs
cargo xtask ci --changed          # a worker's gate before ready: what the change affects
```

Run the app on any swarm's clone (any directory in it; the current directory by default):

```sh
cargo run -- --repo ../swarm-timeline
cargo run -- --repo ../swarm-timeline --grouping workers --collapse window
cargo run -- --repo ../swarm-timeline --screenshot shot.png   # draw, save the window (PNG, or PPM for .ppm), quit
cargo run -- --repo ../swarm-timeline --panes factory --at 2026-10-30T23:47:30Z   # the factory at a moment
cargo run -- --repo ../swarm-timeline --at 2026-10-30T23:47:30Z --hover 58 --trace 58 --screenshot shot.png
cargo run -- --repo . --off-shift --screenshot shot.png   # the benches off shift opened out of their fold (#106)
cargo run -- --repo ../swarm-timeline --fresh   # start without the saved state
```

The app keeps its state per repository across a restart (the panes and their sizes, the rows,
the factory's settings, the details' subject, the timeline's view, the window), under
`~/Library/Application Support/swarmception/state/` on macOS. A `--screenshot` or `--control` run
neither reads nor writes it unless a directory is named (`--state-dir <dir>` or
`SWARMCEPTION_STATE_DIR`): agents' runs leave the owner's alone and draw the same picture each time.

The CLI runs the engine headless, one session, one JSON result a line:

```sh
cargo run -p swarmception-cli -- commands
cargo run -p swarmception-cli -- --repo ../swarm-timeline run repo.status rows.set '{"grouping":"workers"}' rows.list
```

## `cargo xtask ci`

The gate's steps, as `CI summary` names them (the `ci` contract, swarm-core's
docs/ci-contract.md, says which forms Swarm Core's gate calls and what it reads back):

| Step | Runs | In |
|---|---|---|
| `fmt` | `cargo fmt --all -- --check` | full, `--changed`, `--smoke` |
| `clippy` | `cargo clippy --workspace --all-targets --locked -- -D warnings` | full, `--changed` (its packages), `--smoke` |
| `test` | `cargo nextest run --profile ci --workspace`, then `cargo test --doc` | full, `--changed` (its packages) |
| `build` | `cargo build --workspace --exclude xtask` | `--smoke` |
| `layers` | `cargo xtask layers` | every form |
| `fakes` | `cargo xtask fakes`: no Rust source writes a `#!` script or makes a file executable ([Fakes in tests](#fakes-in-tests)) | every form |
| `assets` | `cargo xtask swarm-core sync-assets --check` | every form |

`ci` records its checkout in the target dir (`swarm_core::checkout::claim`) and refuses one
another checkout used last: one target dir per checkout, never shared.

## Fakes in tests

A test that fakes `gh` or `ssh` writes a shell script, with no `#!` line, and has the system's
`sh` run it; it never execs a file it just wrote. macOS vets a new executable on its first exec,
which took 0.3 s idle and 10–34 s under load (#106), and serialises it: on this repo's tests, 12
copies of a binary at once took 15 s with exec'd fakes, 3–4 s run by `sh` (#113).

- `gh`: `crates/engine/tests/support`'s `fake_gh(repo, script)` (the source crate's tests have
  `sh_gh`). `gh` runs in the repository with `api` its first argument, so the fake is `/bin/sh`
  and the script `<repo>/api`, gh's arguments in `$@`.
- `ssh`: `fake_ssh(clone, dir, remote)` sets `core.sshCommand` to `sh <script>` and
  `ssh.variant` to `simple`.
- `cargo xtask fakes`, a step of every `ci`, fails on a `#!` line in a Rust source, or a mode set
  with an execute bit (or one it can't read). A line that must carries `// fakes: allowed: <why>`.

## How many builds at once

Measure it on each machine the swarm builds on, and record it here: the protocol's worker limit
and `cargo xtask capacity`'s suggestion rest on it
([the swarm protocol](swarm-protocol.md#helpers-in-worktrees)).

How: run the full `cargo xtask ci` with dependencies warm and the workspace's crates rebuilt (as a
gate after a change), 1, 2 and 4 at once, each in its own worktree and target dir at one commit,
and note each gate's time, the memory above the desktop's own at the peak, and whether it passed.

| Builds at once | Each gate | Memory above the desktop's own | Result |
|---|---|---|---|
| 1 | <!-- min --> | <!-- GB at its peak --> | |
| 2 | | | |
| 4 | | | |

`cargo xtask capacity` counts about one build per 8 free cores and 5 GB of free memory, at most 3
more, and none under 20 GB of free disk (swarm-core's `capacity`). Memory usually decides,
not cores. If this machine's numbers differ, record them here and tell the coordinator.

## Swarm Core

`xtask` dispatches the swarm's commands (`claim`, `event`, `gate`, `stack`, `land`, …) to
swarm-core, pinned by `rev` in the workspace manifest. `cargo xtask swarm-core --version`
says which; `cargo xtask swarm-core bump` moves it to Swarm Core's `main`, then
`cargo xtask swarm-core sync-assets` refreshes the pinned copies (`docs/swarm-core-assets.md`).
To try a Swarm Core change before it merges, patch it in your **user-level**
`~/.cargo/config.toml`, never in the repo:

```toml
[patch."https://github.com/mornycorp/swarm-core.git"]
swarm-core = { path = "<your swarm-core checkout>/crates/swarm-core" }
```

## Scorecard

`docs/scorecard.md` says, per area, what is done, partial and missing, with evidence. The PR that
changes a row's status updates it; the coordinator refreshes its measured numbers once per batch
([docs/swarm.md › Each batch](swarm.md#each-batch)).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-03 | minor | Tests' fakes run by `sh`, never exec'd new; the `layers` and `fakes` steps (#113) |
| 2026-11-03 | minor | The app's `--off-shift`: the Factory's benches off shift opened out of their fold (#106) |
| 2026-11-01 | minor | The app's `--panes`, `--at`, `--trace` and `--hover`, for the factory (#18) |
| 2026-10-31 | major | First version, from swarm-core's template |
