#!/usr/bin/env bash
# Smoke tests for the scripts a release and a review run, which otherwise
# execute for the first time on a real tag or a paid review run:
#
#   changelog-section.sh   the crate's current version has a section, a
#                          version with none exits 1, --body drops the subject
#   review-denials.sh      the exit status and the report for a denied, a
#                          clean and a silent run (scripts/fixtures/review-*.json),
#                          and no assignment value or path ever printed
#   claude-review.yml      the tools a review must not use are disallowed, and
#                          the report step (its shell, as the file has it, with
#                          a stand-in gh) fails on a failed fetch
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

# --- claude-review.yml ------------------------------------------------------
# What the workflow file itself promises (final review of 2026-09-25: a
# fallback turned every failed fetch of the report script green, and `Task`
# and `Skill` stayed callable because leaving a tool off the allowlist does
# not remove it). The report step's shell is run as the file has it, with
# a stand-in `gh` on PATH.
workflow=.github/workflows/claude-review.yml
yaml_step() {
  ruby -ryaml -e '
    step = YAML.load_file(ARGV[0])["jobs"]["review"]["steps"].find { |s| s["name"] == ARGV[1] }
    print(ARGV[2] == "run" ? step["run"] : step["with"][ARGV[2]])' "$workflow" "$1" "$2"
}
if ! command -v ruby > /dev/null 2>&1 || ! command -v jq > /dev/null 2>&1; then
  echo "ruby or jq not installed; skipping the claude-review.yml tests"
else
  disallowed="$(yaml_step Review claude_args | sed -n 's/^--disallowedTools "\(.*\)"$/\1/p')"
  for tool in Task Skill Workflow Write Edit; do
    case ",$disallowed," in
      *",$tool,"*) pass "claude_args disallows $tool" ;;
      *) fail "claude_args does not disallow $tool (--disallowedTools \"$disallowed\")" ;;
    esac
  done
  tmp="$(mktemp -d)"
  yaml_step "Report what the review was refused" run > "$tmp/report.sh"
  mkdir "$tmp/bin"
  report() {
    local gh="$1" execution="$2" want="$3" status=0
    printf '#!/bin/sh\n%s\n' "$gh" > "$tmp/bin/gh"
    chmod +x "$tmp/bin/gh"
    (cd "$tmp" && PATH="$tmp/bin:$PATH" REPO=o/r BASE_BRANCH=main RUNNER_TEMP="$tmp" \
      GITHUB_STEP_SUMMARY="$tmp/summary" EXECUTION_FILE="$execution" \
      bash "$tmp/report.sh" > /dev/null 2>&1) || status=$?
    if [ "$status" = "$want" ]; then
      pass "report step: $4 exits $want"
    else
      fail "report step: $4 exited $status, expected $want"
    fi
  }
  serve="cat '$PWD/scripts/review-denials.sh'"
  broken='echo "gh: HTTP 502" >&2; exit 1'
  report "$serve" "$PWD/scripts/fixtures/review-clean.json" 0 "a clean review"
  report "$serve" "$PWD/scripts/fixtures/review-denied.json" 1 "a refused review"
  report "$broken" "$PWD/scripts/fixtures/review-clean.json" 1 "a failed fetch"
  report "$broken" "" 0 "no execution file"
  rm -rf "$tmp"
fi

if [ "$failures" != 0 ]; then
  echo "$failures script test(s) failed" >&2
  exit 1
fi
echo "script tests passed"
