#!/usr/bin/env bash
# Takes the screenshots a PR needs of a swarm's clone (docs/architecture.md › Automation): builds
# the app and the CLI, starts the app with --control, waits for its control file, sends it a
# script of requests (scripts/screenshots.jsonl unless one is given) with `swarmception-cli
# send`, and closes the app. The PNGs land in the output directory (the current one by default).
#
#   scripts/screenshots.sh <clone> [<out dir>] [<script.jsonl>]
#
# Honours CARGO_TARGET_DIR (relative to the repository's root, as the swarm's are).
set -euo pipefail

usage="usage: scripts/screenshots.sh <clone> [<out dir>] [<script.jsonl>]"
root="$(cd "$(dirname "$0")/.." && pwd)"
repo="${1:?$usage}"
out="${2:-.}"
script="${3:-$root/scripts/screenshots.jsonl}"
mkdir -p "$out"
# Absolute before moving: the app runs from the root, the client from the output directory.
repo="$(cd "$repo" && pwd)"
out="$(cd "$out" && pwd)"
script="$(cd "$(dirname "$script")" && pwd)/$(basename "$script")"
target="${CARGO_TARGET_DIR:-target}"
case "$target" in
  /*) ;;
  *) target="$root/$target" ;;
esac

(cd "$root" && cargo build -q -p swarmception -p swarmception-cli)

scratch="$(mktemp -d)"
control="$scratch/control.json"
"$target/debug/swarmception" --repo "$repo" --control --control-file "$control" --size 1600 1000 >/dev/null &
app=$!
trap 'kill "$app" 2>/dev/null || true; rm -rf "$scratch"' EXIT

for _ in $(seq 200); do
  [ -s "$control" ] && break
  kill -0 "$app" 2>/dev/null || { echo "screenshots: the app quit before it listened" >&2; exit 3; }
  sleep 0.1
done
[ -s "$control" ] || { echo "screenshots: no control file after 20 s" >&2; exit 3; }

cd "$out"
"$target/debug/swarmception-cli" send --control "$control" --script "$script"
