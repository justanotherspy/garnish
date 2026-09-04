#!/usr/bin/env bash
# Full local CI: formatting, strict clippy, tests, doctests, docs, docs-sync.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n\033[1;36m== %s\033[0m\n' "$*"; }

step "cargo fmt --check";   cargo fmt --check
step "cargo clippy";        cargo clippy --all-targets --all-features -- -D warnings
step "cargo nextest run";   cargo nextest run
step "cargo test --doc";    cargo test --doc
step "cargo doc";           RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --quiet
step "docs sync";           cargo nextest run --test docs_sync
printf '\n\033[1;32mCI green\033[0m\n'
