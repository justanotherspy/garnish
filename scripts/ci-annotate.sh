#!/usr/bin/env bash
# Turn a captured `make check` / `scripts/ci.sh` log into GitHub Actions
# annotations so a failure can be diagnosed from the Checks API alone (job
# logs sit behind a redirect that not every client can follow).
#
#   ./scripts/ci-annotate.sh ci.log
#
# Emits one `::error::` per failed test, clippy error or rustfmt diff, and a
# `::notice::` carrying the first lines of each failing test's output.
# Annotations are capped per step (10 errors / 10 warnings / 50 notices), so
# only the first ten failures are reported in full.
set -uo pipefail
log="${1:?usage: ci-annotate.sh LOG}"

# Newlines must be percent-encoded inside a workflow-command message.
encode() { sed ':a;N;$!ba;s/\n/%0A/g' | sed 's/\r//g'; }

errors=0
emit_error() {
  if [ "$errors" -lt 10 ]; then printf '::error::%s\n' "$1"; fi
  errors=$((errors + 1))
}

# nextest: "        FAIL [   0.123s] garnish::worker cache_lock_reclaim"
while IFS= read -r line; do
  emit_error "test failed: ${line#*] }"
done < <(grep -E '^\s*(FAIL|TIMEOUT|SIGSEGV|ABORT)\s+\[' "$log" | sort -u)

# nextest: each failing test's output starts "--- STDERR: <bin> <name> ---"
while IFS= read -r header; do
  name="${header#--- STD*: }"
  name="$(printf '%s' "${name% ---}" | tr -s ' ')"
  body="$(awk -v h="$header" 'found && /^--- STD|^\s*(FAIL|PASS|Summary)/ {exit} found {print} $0==h {found=1}' "$log" \
    | head -c 1500 | encode)"
  printf '::notice::%s%%0A%s\n' "$name" "$body"
done < <(grep -E '^--- STD(OUT|ERR): ' "$log" | head -50)

# clippy/rustc: "error: ..." then "  --> file:line:col"
while IFS= read -r line; do
  emit_error "$line"
done < <(grep -E '^error(\[E[0-9]+\])?: ' "$log" | grep -v 'could not compile\|aborting due to\|process didn.t exit' | sort -u | head -10)

# rustfmt: "Diff in /path/file.rs:LINE:"
while IFS= read -r line; do
  emit_error "rustfmt: $line"
done < <(grep -E '^Diff in ' "$log" | sort -u | head -10)

if [ "$errors" -gt 10 ]; then
  printf '::warning::%d failures in total; only the first 10 are annotated\n' "$errors"
fi
