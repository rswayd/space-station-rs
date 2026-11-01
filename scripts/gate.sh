#!/usr/bin/env bash
# The full gate (AGENTS.md "Done means"): fmt, dependency licenses (deny.toml, needs cargo-deny),
# clippy, every test, the server and client builds, the wasm32 check and the bot seed suite, each step timed. `--no-bots` skips the seed suite. SS13_ASSETS (default
# assets/generated) must hold assets converted from this tree; SS13_BOTS_JOBS sets the suite's
# parallel servers.
set -euo pipefail
cd "$(dirname "$0")/.."
assets=$(realpath "${SS13_ASSETS:-assets/generated}")
step() {
    local name=$1 start=$SECONDS
    shift
    echo "== $name"
    "$@"
    echo "== $name: ok in $((SECONDS - start)) s"
}
step fmt cargo fmt --check
step deny cargo deny --log-level error check licenses sources bans
step clippy cargo lint
step tests cargo nextest run --workspace --all-features --no-fail-fast
step bins cargo bins
# The browser roots (docs/WEB.md): std::fs, threads or Instant in shared code break them.
step wasm cargo check --target wasm32-unknown-unknown -p ss13_server -p ss13_client
if [[ ${1:-} != --no-bots ]]; then
    step bots env SS13_ASSETS="$assets" cargo test --workspace --all-features --test seed -- --ignored
fi
echo "== gate: ok in $SECONDS s"
