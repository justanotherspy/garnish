#!/usr/bin/env bash
# Print one release's section of CHANGELOG.md on stdout, in the shape of the
# release's tag message (CLAUDE.md § Release process): the heading text
# without its `## ` as the subject line, a blank line, then the body.
#
#   scripts/changelog-section.sh 0.2.0            the "## 0.2.0 …" section
#   scripts/changelog-section.sh v0.2.0           the same (a leading v is dropped)
#   scripts/changelog-section.sh --body 0.2.0     the body only (release notes)
#
# Exits 1 with a note on stderr when CHANGELOG.md has no such section, which
# is how the release workflow's verify job refuses a tag the changelog does
# not mention. Portable awk and sed only: the macOS runner has no GNU
# extensions.
set -euo pipefail
cd "$(dirname "$0")/.."

body_only=0
version=""
for arg in "$@"; do
  case "$arg" in
    --body) body_only=1 ;;
    -h | --help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "changelog-section: unknown flag $arg" >&2; exit 2 ;;
    *) version="$arg" ;;
  esac
done
if [ -z "$version" ]; then
  echo "usage: scripts/changelog-section.sh [--body] <version>" >&2
  exit 2
fi
version="${version#v}"

# The second word of a `## ` heading is the version ("## 0.2.0 — 2026-09-06
# (PLAN Phases 12–18)"); the section runs to the next `## ` heading.
section="$(awk -v want="$version" '
  /^## / {
    if (found) exit
    found = ($2 == want)
    if (found) { sub(/^## /, ""); print }
    next
  }
  found { print }
' CHANGELOG.md)"

if [ -z "$section" ]; then
  echo "changelog-section: CHANGELOG.md has no \"## $version\" section" >&2
  exit 1
fi

if [ "$body_only" = 1 ]; then
  # Drop the subject line and the blank lines that follow it.
  printf '%s\n' "$section" | sed '1d' | sed '/./,$!d'
else
  printf '%s\n' "$section"
fi
