#!/usr/bin/env bash
# Turn a captured `make check` / `scripts/ci.sh` log into GitHub Actions
# annotations so a failure can be diagnosed from the Checks API alone (job
# logs sit behind a redirect that not every client can follow).
#
#   ./scripts/ci-annotate.sh ci.log
#
# Emits one `::error::` per failed test (anchored to the panic's file:line
# when there is one), clippy error or rustfmt diff, a `::notice::` with each
# failing test's captured output, and, when nothing matched, the tail of the
# log so the failure is never invisible. Portable across GNU and BSD
# userlands (macOS runners): POSIX classes only, no GNU sed/awk extensions.
# Annotations are capped per step (10 errors / 10 warnings / 50 notices).
set -uo pipefail
log="${1:?usage: ci-annotate.sh LOG}"

# Work on a copy with ANSI colour and CRs stripped: CARGO_TERM_COLOR=always
# paints every line, and the escape before "FAIL" hides it from anchored greps.
clean="$log.clean"
esc="$(printf '\033')"
sed "s/${esc}\[[0-9;]*[A-Za-z]//g" "$log" | tr -d '\r' > "$clean"

# Newlines must be percent-encoded inside a workflow-command message.
encode() { awk '{ printf "%s%%0A", $0 }'; }

errors=0
notices=0
# emit_error [file=F,line=N] MESSAGE
emit_error() {
  if [ "$errors" -lt 10 ]; then
    if [ -n "$1" ]; then printf '::error %s::%s\n' "$1" "$2"; else printf '::error::%s\n' "$2"; fi
  fi
  errors=$((errors + 1))
}
emit_notice() {
  if [ "$notices" -lt 50 ]; then printf '::notice::%s\n' "$1"; fi
  notices=$((notices + 1))
}

# nextest: "        FAIL [   0.008s] (1/95) garnish::worker cache_lock_reclaim"
# followed by an indented block (stdout ───, stderr ───, the panic) that ends
# at the next unindented line. The Summary repeats the FAIL line with no
# block. Unit tests print "garnish cache::tests name", so keep every word
# after the "(n/m) " counter as the name.
fail_re='^[[:space:]]*(FAIL|TIMEOUT|SIGSEGV|SIGABRT|ABORT)[[:space:]]+\['
while IFS= read -r name; do
  # (The regex is repeated literally: `awk -v` would eat the backslash.)
  block="$(awk -v n="$name" '
    /^[[:space:]]*(FAIL|TIMEOUT|SIGSEGV|SIGABRT|ABORT)[[:space:]]+\[/ { if (found) exit; s = $0; sub(/^[^)]*\) /, "", s); if (s == n) { found = 1; next } }
    found && /^[^[:space:]]/ { exit }
    found { print }' "$clean")"
  where="$(printf '%s\n' "$block" | grep -E -m1 'panicked at [^ :]+:[0-9]+:[0-9]+:?$' \
    | sed -E 's/.*panicked at ([^ :]+):([0-9]+):[0-9]+:?$/\1 \2/')"
  msg="$(printf '%s\n' "$block" | grep -E -A1 -m1 'panicked at ' | tail -n 1 | sed -E 's/^[[:space:]]+//')"
  if [ -n "$where" ]; then
    emit_error "file=${where% *},line=${where#* }" "test failed: $name: $msg"
  else
    emit_error "" "test failed: $name"
  fi
  if [ -n "$block" ]; then
    emit_notice "${name}%0A$(printf '%s\n' "$block" | head -c 1500 | encode)"
  fi
done < <(grep -E "$fail_re" "$clean" | sed -E 's/^[^)]*\) //' | sort -u)

# clippy/rustc: "error: ..." then "  --> file:line:col"
while IFS= read -r line; do
  emit_error "" "$line"
done < <(grep -E '^error(\[E[0-9]+\])?: ' "$clean" | grep -v -e 'could not compile' -e 'aborting due to' -e "process didn't exit" -e 'test run failed' | sort -u | head -10)

# rustfmt: "Diff in /path/file.rs:LINE:"
while IFS= read -r line; do
  emit_error "" "rustfmt: $line"
done < <(grep -E '^Diff in ' "$clean" | sort -u | head -10)

if [ "$errors" -gt 10 ]; then
  printf '::warning::%d failures in total; only the first 10 are annotated\n' "$errors"
fi

# Nothing recognisable: ship the tail of the log in 1500-byte notices.
if [ "$errors" -eq 0 ] && [ "$notices" -eq 0 ]; then
  emit_error "" "no test/clippy/rustfmt failure recognised in $log; see the log tail notices"
  tail -n 120 "$clean" > "$clean.tail"
  split -b 1500 "$clean.tail" "$clean.tail."
  for part in "$clean.tail."*; do
    emit_notice "log tail: $(encode < "$part")"
  done
fi
