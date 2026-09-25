#!/usr/bin/env bash
# Smoke tests for the scripts a release and a review run, which otherwise
# execute for the first time on a real tag or a paid review run:
#
#   changelog-section.sh   the crate's current version has a section, a
#                          version with none exits 1, --body drops the subject
#   review-denials.sh      the exit status and the report for a denied, a
#                          clean and a silent run (scripts/fixtures/review-*.json),
#                          and no assignment value or path ever printed
#
# render-cask.sh downloads the release's archives, so it has no offline test.
# Runs from scripts/ci.sh and on the macOS job, whose BSD userland is what
# these scripts must also work under.
set -euo pipefail
cd "$(dirname "$0")/.."

failures=0
fail() { echo "FAIL: $*" >&2; failures=$((failures + 1)); }
pass() { echo "ok: $*"; }

# --- changelog-section.sh --------------------------------------------------
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
if section="$(scripts/changelog-section.sh "$version")"; then
  case "$(printf '%s\n' "$section" | head -n 1)" in
    "$version "* | "$version") pass "changelog-section.sh $version" ;;
    *) fail "changelog-section.sh $version: the subject line is not the version heading" ;;
  esac
else
  fail "changelog-section.sh $version: CHANGELOG.md has no section for the crate's version"
fi
if scripts/changelog-section.sh 999.999.999 > /dev/null 2>&1; then
  fail "changelog-section.sh 999.999.999 should exit non-zero"
else
  pass "changelog-section.sh refuses a version with no section"
fi
# `|| true`: under `set -e` a missing section would end the run here, before
# the review-denials.sh tests and the failure count.
body="$(scripts/changelog-section.sh --body "$version" || true)"
if [ -z "$body" ] || [ "$(printf '%s\n' "$body" | head -n 1)" = "$(printf '%s\n' "$section" | head -n 1)" ]; then
  fail "changelog-section.sh --body $version: empty, or still carries the subject line"
else
  pass "changelog-section.sh --body"
fi

# --- review-denials.sh -----------------------------------------------------
# Runs the script on a fixture and checks its exit status.
denials() {
  local fixture="$1" want="$2" status=0 path=""
  # An empty argument is what the workflow passes when there is no file.
  if [ -n "$fixture" ]; then path="scripts/fixtures/$fixture"; fi
  out="$(scripts/review-denials.sh "$path")" || status=$?
  if [ "$status" != "$want" ]; then
    fail "review-denials.sh $fixture exited $status, expected $want"
    return 1
  fi
  pass "review-denials.sh $fixture exits $want"
}
has() { printf '%s\n' "$out" | grep -qF -- "$1" || fail "review-denials.sh output lacks: $1"; }
lacks() { if printf '%s\n' "$out" | grep -qF -- "$1"; then fail "review-denials.sh output shows: $1"; fi; }

if ! command -v jq > /dev/null 2>&1; then
  echo "jq not installed; skipping the review-denials.sh tests (scripts/setup.sh --bench installs it)"
else
  if denials review-denied.json 1; then
    has 'Bash(git diff:*)'
    has 'Bash(<path>:*)'
    has 'WebFetch'
    has 'curl → <path>'
    lacks 'secret-value'
    lacks 'abc123'
    lacks '/abs/'
  fi
  if denials review-clean.json 0; then
    has 'Nothing was refused.'
    has '1 in the tracking comment'
  fi
  if denials review-silent.json 1; then
    has 'The review posted no summary.'
  fi
  if denials '' 0; then
    has 'no execution file'
  fi
fi

if [ "$failures" != 0 ]; then
  echo "$failures script test(s) failed" >&2
  exit 1
fi
echo "script tests passed"
