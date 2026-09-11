#!/usr/bin/env bash
# Render the Homebrew cask for one released version on stdout: download the
# four release archives from GitHub, take their sha256, and fill the
# placeholders of .github/homebrew/garnish-cask.rb.tmpl. The release
# workflow pushes the result to justanotherspy/homebrew-tap as
# Casks/garnish.rb (CLAUDE.md § Release process).
#
#   scripts/render-cask.sh 0.3.0 > garnish.rb
#   scripts/render-cask.sh v0.3.0     the same (a leading v is dropped)
#
# Every archive must already be attached to the release (the workflow's
# build job puts them there); a missing one fails the download. Portable
# shell only (shasum, not sha256sum: the macOS runner has no coreutils).
set -euo pipefail
cd "$(dirname "$0")/.."

version="${1:-}"
if [ -z "$version" ]; then
  echo "usage: scripts/render-cask.sh <version>" >&2
  exit 2
fi
version="${version#v}"
repo="${GITHUB_REPOSITORY:-justanotherspy/garnish}"
base="https://github.com/$repo/releases/download/v$version"
template=".github/homebrew/garnish-cask.rb.tmpl"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Downloads the archive for one target and prints its sha256.
sha() {
  local archive="garnish-$1.tar.gz"
  curl -fsSL --retry 5 --retry-all-errors -o "$tmp/$archive" "$base/$archive"
  shasum -a 256 "$tmp/$archive" | cut -d' ' -f1
}

rendered="$(sed \
  -e "s/@@VERSION@@/$version/" \
  -e "s/@@SHA_X86_64_APPLE_DARWIN@@/$(sha x86_64-apple-darwin)/" \
  -e "s/@@SHA_AARCH64_APPLE_DARWIN@@/$(sha aarch64-apple-darwin)/" \
  -e "s/@@SHA_X86_64_LINUX@@/$(sha x86_64-unknown-linux-gnu)/" \
  -e "s/@@SHA_AARCH64_LINUX@@/$(sha aarch64-unknown-linux-gnu)/" \
  "$template")"

# A placeholder the template added but this script does not know would
# otherwise reach the tap as literal text.
if printf '%s\n' "$rendered" | grep -q '@@'; then
  echo "render-cask: unreplaced placeholder in $template:" >&2
  printf '%s\n' "$rendered" | grep '@@' >&2
  exit 1
fi
printf '%s\n' "$rendered"
