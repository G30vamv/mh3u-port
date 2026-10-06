#!/usr/bin/env bash
# The checks to run before every commit, stopping at the first failure. There is no CI: the repository is private and Actions are off.
set -euo pipefail
cd "$(dirname "$0")/.."
# the tests that need the game's files find them through MH3U_GAME_DIR; without it they skip
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
# known vulnerabilities in the dependencies (https://rustsec.org). it runs when cargo-audit is installed (cargo install cargo-audit --locked)
if command -v cargo-audit >/dev/null 2>&1; then
    cargo audit
else
    echo "note: cargo-audit is not installed, so the dependency audit was skipped here" >&2
fi
