#!/usr/bin/env bash
# Full local CI: formatting, strict clippy, shellcheck, tests, doctests, docs.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n\033[1;36m== %s\033[0m\n' "$*"; }

step "cargo fmt --check";   cargo fmt --check
step "cargo clippy";        cargo clippy --all-targets --all-features -- -D warnings
# CLAUDE.md § Style makes this a rule for every script here; nothing used to
# check it, and scripts/render-cask.sh and changelog-section.sh are what a
# release runs. Skipped with a note where shellcheck is not installed, so a
# contributor without it still gets the rest of the gate.
step "shellcheck"
if command -v shellcheck >/dev/null; then
  shellcheck scripts/*.sh bench/*.sh .claude/hooks/*.sh
else
  echo "shellcheck not installed; skipping (run scripts/setup.sh)"
fi
# The docs-sync suite runs inside this: `cargo nextest run` includes every
# integration binary, `docs_sync` among them.
step "cargo nextest run";   cargo nextest run
step "cargo test --doc";    cargo test --doc
step "cargo doc";           RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --quiet
printf '\n\033[1;32mCI green\033[0m\n'
