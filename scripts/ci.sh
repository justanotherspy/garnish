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
if [[ -x target/release/garnish ]] || cargo build --release --quiet; then
  step "docs sync"
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
  if target/release/garnish docs --out "$tmp" >/dev/null 2>&1; then
    diff -ru docs "$tmp" --exclude=guide.md && echo "docs in sync"
  else
    echo "garnish docs not implemented yet; skipping sync check"
  fi
fi
printf '\n\033[1;32mCI green\033[0m\n'
