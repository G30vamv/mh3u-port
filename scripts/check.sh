#!/usr/bin/env bash
# The checks CI runs, in the same order, stopping at the first failure. Run it before every commit.
set -euo pipefail
cd "$(dirname "$0")/.."
# the tests that need the game's files find them through MH3U_GAME_DIR; without it they skip
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
# known vulnerabilities in the dependencies (https://rustsec.org). CI always runs this; here it runs when cargo-audit is installed
if command -v cargo-audit >/dev/null 2>&1; then
    cargo audit
else
    echo "note: cargo-audit is not installed, so the dependency audit was skipped here (CI runs it)" >&2
fi
